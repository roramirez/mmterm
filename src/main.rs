mod app_event;
mod app_state;
mod command_palette;
mod config;
mod drain;
mod font;
mod geometry;
mod input;
mod logging;
mod motion;
mod mouse;
mod pty;
mod renderer;
mod restore;
mod search;
mod session;
mod statusbar;
mod tabs;
mod terminal;
mod theme;
mod tui_config;
mod ui;
mod views;

pub use app_state::{AppEffect, AppState, PaneEntry, TabState};

#[cfg(test)]
#[path = "main_test.rs"]
mod tests;

use arboard::Clipboard;
use chrono::Local;
use config::Config;
use crossbeam_channel::unbounded;
use input::InputMode;
use renderer::Renderer;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use ui::{Layout, Pane, SplitDir};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, Modifiers, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Fullscreen, Icon, Window, WindowId};

use crate::input::keybindings::Action;
use crate::terminal::grid::GridColors;
use crate::theme::{default_theme, install_bundled_themes, load_theme, themes_dir};
use crate::ui::layout::{PANE_PADDING, TAB_BAR_H};

// ── App ──────────────────────────────────────────────────────────────────────

struct App {
    state: AppState,
    // ── winit / rendering infrastructure ────────────────────────────────────
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    renderer: Renderer,
    modifiers: Modifiers,
    proxy: EventLoopProxy<()>,
    surface_size: (u32, u32),
    wakeup_pending: Arc<AtomicBool>,
    /// Timestamp of the last frame where PTY data was actually consumed.
    /// Used to drive a vsync-style render loop while output is flowing.
    last_pty_data: Option<Instant>,
    /// Pending screenshot crop [x, y, w, h]; captured in redraw() before overlays are drawn.
    pending_screenshot: Option<[u32; 4]>,
    /// Named session scope from `--scope <name>`; `None` means the default session.
    scope: Option<String>,
}

fn bracketed_paste_encode(text: &str, bracketed: bool) -> Vec<u8> {
    let mut data = Vec::new();
    if bracketed {
        data.extend_from_slice(b"\x1b[200~");
    }
    data.extend_from_slice(text.as_bytes());
    if bracketed {
        data.extend_from_slice(b"\x1b[201~");
    }
    data
}

impl App {
    fn new(config: Config, proxy: EventLoopProxy<()>, scope: Option<String>) -> Self {
        let renderer = Renderer::new(&config.font.family, config.font.size);
        let td = themes_dir();
        install_bundled_themes(&td);
        let theme = load_theme(&config.theme.name, &td).unwrap_or_else(|e| {
            log::warn!("{e} — using default theme");
            default_theme()
        });
        let wakeup_pending = Arc::new(AtomicBool::new(false));
        let state = AppState::new(config, theme);
        Self {
            state,
            window: None,
            surface: None,
            renderer,
            modifiers: Modifiers::default(),
            proxy,
            surface_size: (0, 0),
            wakeup_pending,
            last_pty_data: None,
            pending_screenshot: None,
            scope,
        }
    }

    // ── Delegate to AppState ─────────────────────────────────────────────────

    fn tab(&self) -> &TabState {
        self.state.tab()
    }

    fn tab_mut(&mut self) -> &mut TabState {
        self.state.tab_mut()
    }

    // ── Pane spawning ────────────────────────────────────────────────────────

    fn spawn_pane_into(
        &mut self,
        tab_idx: usize,
        rect: [u32; 4],
        cwd: Option<std::path::PathBuf>,
    ) -> usize {
        let id = self.state.next_pane_id;
        self.state.next_pane_id += 1;
        let [_, _, w, h] = rect;
        let pad2 = PANE_PADDING * 2;
        let (cols, rows) = self.state.tabs[tab_idx]
            .metrics
            .grid_size_for(w.saturating_sub(pad2), h.saturating_sub(pad2));
        let t = &self.state.theme;
        let pane = Pane::new_with_colors(
            cols,
            rows,
            rect,
            GridColors {
                fg: t.foreground,
                bg: t.background,
                cursor: t.cursor,
                selection: t.selection,
                palette: t.palette,
            },
            self.state.config.terminal.scrollback_lines,
        );
        let (tx, rx) = unbounded::<Vec<u8>>();
        let shell = self
            .state
            .config
            .shell
            .program
            .clone()
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/bash".to_string());
        let proxy = self.proxy.clone();
        let wakeup_pending = Arc::clone(&self.wakeup_pending);
        let wakeup = Box::new(move || {
            // Only send one event while one is already in flight; the event
            // loop clears the flag in user_event before requesting a redraw.
            if !wakeup_pending.swap(true, Ordering::AcqRel) {
                let _ = proxy.send_event(());
            }
        });
        match pty::PtySession::spawn_with_shell(
            cols as u16,
            rows as u16,
            tx,
            &shell,
            cwd.as_ref(),
            wakeup,
        ) {
            Ok(pty) => {
                let log_file = if self.state.config.logging.auto_log {
                    open_log_file(id, &self.state.config.logging.log_dir)
                } else {
                    None
                };
                self.state.tabs[tab_idx].panes.insert(
                    id,
                    PaneEntry {
                        pane,
                        pty,
                        rx,
                        log_file,
                    },
                );
            }
            Err(e) => log::error!("PTY spawn failed: {e}"),
        }
        id
    }

    fn spawn_pane(&mut self, rect: [u32; 4]) -> usize {
        let cwd = self
            .state
            .tabs
            .get(self.state.active_tab)
            .and_then(|t| t.panes.get(&t.active))
            .and_then(|e| e.pty.cwd());
        self.spawn_pane_into(self.state.active_tab, rect, cwd)
    }

    // ── Tab management ───────────────────────────────────────────────────────

    fn new_tab(&mut self, win_w: u32, win_h: u32) {
        let cwd = self
            .state
            .tabs
            .get(self.state.active_tab)
            .and_then(|t| t.panes.get(&t.active))
            .and_then(|e| e.pty.cwd());
        let metrics = self.renderer.make_metrics(self.renderer.font_px);
        let layout = Layout::new(0, win_w, win_h);
        let initial_rect = layout
            .rects()
            .first()
            .map(|(_, r)| *r)
            .unwrap_or([0, TAB_BAR_H, win_w, win_h]);
        let tab_idx = self.state.tabs.len();
        self.state.tabs.push(TabState {
            panes: HashMap::new(),
            layout: Layout::new(0, win_w, win_h),
            active: 0,
            metrics,
            name: None,
            zoomed: false,
            has_activity: false,
            bell_flash_start: None,
            bell_flash_until: None,
            bell_cooldown_until: None,
            passthrough: false,
        });
        let id = self.spawn_pane_into(tab_idx, initial_rect, cwd);
        self.state.tabs[tab_idx].layout = Layout::new(id, win_w, win_h);
        self.state.tabs[tab_idx].active = id;
        self.state.active_tab = tab_idx;
    }

    fn close_tab(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.tabs.len() == 1 {
            event_loop.exit();
            return;
        }
        let old_active = self.state.active_tab;
        let old_count = self.state.tabs.len();
        self.state.tabs.remove(old_active);
        self.state.active_tab = tabs::close_tab_index(old_active, old_count);
    }

    // ── Drain PTY output ─────────────────────────────────────────────────────

    /// Drain pending PTY output up to a per-frame byte budget. Returns
    /// (exited pairs, has_more) — callers should request another redraw when
    /// has_more is true so the display stays live during high-throughput output.
    fn close_pane_on_tab(&mut self, tab_idx: usize, pane_id: usize, event_loop: &ActiveEventLoop) {
        if tab_idx >= self.state.tabs.len() {
            return;
        }
        if self.state.tabs[tab_idx].panes.len() == 1 {
            if self.state.tabs.len() == 1 {
                event_loop.exit();
                return;
            }
            self.state.tabs.remove(tab_idx);
            if self.state.active_tab >= self.state.tabs.len() {
                self.state.active_tab = self.state.tabs.len() - 1;
            }
            return;
        }
        let tab = &mut self.state.tabs[tab_idx];
        let new_focus = tab.layout.remove(pane_id);
        tab.panes.remove(&pane_id);
        if tab.active == pane_id {
            tab.active = new_focus.unwrap_or_else(|| *tab.panes.keys().next().unwrap());
        }
        Self::sync_pane_sizes_tab(&mut self.state.tabs[tab_idx]);
    }

    // ── Resize ───────────────────────────────────────────────────────────────

    fn sync_pane_sizes_tab(tab: &mut TabState) {
        let rects = tab.layout.rects();
        for (id, rect) in rects {
            if let Some(entry) = tab.panes.get_mut(&id) {
                let [_, _, w, h] = rect;
                let pad2 = PANE_PADDING * 2;
                let (cols, rows) = tab
                    .metrics
                    .grid_size_for(w.saturating_sub(pad2), h.saturating_sub(pad2));
                if entry.pane.parser.grid.cols != cols || entry.pane.parser.grid.rows != rows {
                    entry.pane.resize(cols, rows, rect);
                    let _ = entry.pty.resize(cols as u16, rows as u16);
                }
            }
        }
    }

    fn sync_all_pane_sizes(&mut self) {
        for tab in &mut self.state.tabs {
            Self::sync_pane_sizes_tab(tab);
        }
    }

    // ── Split / pane management ───────────────────────────────────────────────

    fn do_split(&mut self, dir: SplitDir) {
        self.tab_mut().zoomed = false;
        let active = self.tab().active;
        let active_rect = self
            .tab()
            .layout
            .rects()
            .into_iter()
            .find(|(id, _)| *id == active)
            .map(|(_, r)| r)
            .unwrap_or([0, TAB_BAR_H, 100, 100]);

        let new_rect = match dir {
            SplitDir::H => {
                let hw = active_rect[2] / 2;
                [active_rect[0] + hw, active_rect[1], hw, active_rect[3]]
            }
            SplitDir::V => {
                let hh = active_rect[3] / 2;
                [active_rect[0], active_rect[1] + hh, active_rect[2], hh]
            }
        };

        let new_id = self.spawn_pane(new_rect);
        let tab = self.tab_mut();
        tab.layout.split(active, new_id, dir);
        tab.active = new_id;
        let idx = self.state.active_tab;
        Self::sync_pane_sizes_tab(&mut self.state.tabs[idx]);
    }

    fn do_close_pane(&mut self, event_loop: &ActiveEventLoop) {
        self.tab_mut().zoomed = false;
        let tab = self.tab_mut();
        if tab.panes.len() == 1 {
            let _ = tab;
            self.close_tab(event_loop);
            return;
        }
        let active = tab.active;
        let new_focus = tab.layout.remove(active);
        tab.panes.remove(&active);
        tab.active = new_focus.unwrap_or_else(|| *tab.panes.keys().next().unwrap());
        let idx = self.state.active_tab;
        Self::sync_pane_sizes_tab(&mut self.state.tabs[idx]);
    }

    // ── Mouse reporting ──────────────────────────────────────────────────────

    /// Returns the mouse_mode and mouse_sgr flags for the active pane.
    fn active_mouse_mode(&self) -> (u16, bool) {
        let active = self.tab().active;
        self.tab()
            .panes
            .get(&active)
            .map(|e| (e.pane.parser.grid.mouse_mode, e.pane.parser.grid.mouse_sgr))
            .unwrap_or((0, false))
    }

    /// Encode a mouse event and write it to the active pane's PTY.
    /// `btn` is the X10 button code (0=left, 1=middle, 2=right, 32=motion).
    /// `release` is only meaningful for non-SGR encoding.
    fn send_mouse_event(&mut self, btn: u8, col: usize, row: usize, release: bool, sgr: bool) {
        let active = self.tab().active;
        let data = mouse::encode_mouse_event(btn, col, row, release, sgr);
        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
            let _ = entry.pty.write_input(&data);
        }
    }

    // ── Mouse selection ───────────────────────────────────────────────────────

    fn pane_at_pixel(&self, px: f64, py: f64) -> Option<usize> {
        let rects = self.tab().layout.rects();
        geometry::pane_at_pixel(&rects, px, py)
    }

    fn pixel_to_cell(&self, pane_id: usize, px: f64, py: f64) -> Option<(usize, usize)> {
        let tab = self.tab();
        let entry = tab.panes.get(&pane_id)?;
        let m = &tab.metrics;
        geometry::pixel_to_cell(
            entry.pane.rect,
            m.cell_width,
            m.cell_height,
            entry.pane.parser.grid.cols,
            entry.pane.parser.grid.rows,
            px,
            py,
        )
    }

    fn url_at_pixel(&self, px: f64, py: f64) -> Option<String> {
        let pane_id = self.pane_at_pixel(px, py)?;
        let (col, row) = self.pixel_to_cell(pane_id, px, py)?;
        let tab = self.tab();
        let entry = tab.panes.get(&pane_id)?;
        let url = geometry::cell_url_at_scroll(
            &entry.pane.parser.grid,
            entry.pane.scroll_offset,
            col,
            row,
        )?;
        Some(url.as_ref().clone())
    }

    fn start_mouse_selection(&mut self, px: f64, py: f64) {
        if let Some(pane_id) = self.pane_at_pixel(px, py) {
            self.tab_mut().active = pane_id;
            if let Some((col, row)) = self.pixel_to_cell(pane_id, px, py) {
                self.state.mode = InputMode::Visual {
                    start_col: col,
                    start_row: row,
                    cur_col: col,
                    cur_row: row,
                    anchored: true,
                };
                self.state.mouse_selecting = true;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
    }

    fn update_mouse_selection(&mut self, px: f64, py: f64) {
        if let InputMode::Visual {
            start_col,
            start_row,
            ..
        } = self.state.mode.clone()
        {
            let active = self.tab().active;
            if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                self.state.mode = InputMode::Visual {
                    start_col,
                    start_row,
                    cur_col: col,
                    cur_row: row,
                    anchored: true,
                };
            }
        }
    }

    fn finish_mouse_selection(&mut self) {
        if let InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            cur_row,
            ..
        } = self.state.mode.clone()
        {
            self.state.mode = InputMode::Insert;
            if start_col == cur_col && start_row == cur_row {
                if let Some(url) = self.state.hovered_url.clone() {
                    open_url(&url);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
                return;
            }
            self.copy_selection_to_clipboard(start_col, start_row, cur_col, cur_row);
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn copy_selection_to_clipboard(
        &mut self,
        start_col: usize,
        start_row: usize,
        cur_col: usize,
        cur_row: usize,
    ) {
        let active = self.tab().active;
        if let Some(entry) = self.tab().panes.get(&active) {
            let scroll_offset = entry.pane.scroll_offset;
            let text = entry.pane.parser.grid.selected_text(
                start_col,
                start_row,
                cur_col,
                cur_row,
                scroll_offset,
            );
            if !text.is_empty() {
                let cb = self
                    .state
                    .clipboard
                    .get_or_insert_with(|| Clipboard::new().expect("clipboard unavailable"));
                match cb.set_text(text) {
                    Ok(()) => log::info!("Copied mouse selection to clipboard"),
                    Err(e) => log::warn!("Clipboard write failed: {e}"),
                }
            }
        }
    }

    // ── Window helpers ────────────────────────────────────────────────────────

    pub(crate) fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    // ── Action dispatch ───────────────────────────────────────────────────────

    /// Execute an action: pure state mutations go through AppState::dispatch_action;
    /// effects that need winit or the renderer are handled here.
    fn execute_action(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        let focus_before = (
            self.state.active_tab,
            self.state.tabs[self.state.active_tab].active,
        );
        let effects = self.state.dispatch_action(action);
        for effect in effects {
            match effect {
                AppEffect::Redraw => self.request_redraw(),
                AppEffect::Quit => {
                    event_loop.exit();
                }
                AppEffect::SaveSessionAndQuit => {
                    let s = self.build_saved_session();
                    let path = session::session_path_for(self.scope.as_deref());
                    if let Err(e) = session::save_to(&path, &s) {
                        log::warn!("session save failed: {e}");
                    }
                    event_loop.exit();
                }
                AppEffect::QuitPending => self.request_redraw(),
                AppEffect::ToggleFullscreen => self.do_toggle_fullscreen(),
                AppEffect::NewTab => self.do_new_tab(),
                AppEffect::ClosePane => self.do_close_pane(event_loop),
                AppEffect::CloseTab => self.close_tab(event_loop),
                AppEffect::SplitPane(dir) => self.do_split(dir),
                AppEffect::AutoSplitPane => self.do_auto_split(),
                AppEffect::ChangeFontSize(delta) => self.change_font_size(delta),
                AppEffect::ResizePane { split_h, delta } => self.do_resize_pane(split_h, delta),
                AppEffect::ToggleLog => self.do_toggle_log(),
                AppEffect::SendToPty(bytes) => self.do_send_to_pty(bytes),
                AppEffect::Paste => self.do_paste(),
                AppEffect::RotatePanes(forward) => self.do_rotate_panes(forward),
                AppEffect::ScreenshotOpen => {
                    let (w, h) = self.surface_size;
                    let half = w.min(h) / 4;
                    self.state.mode = InputMode::Screenshot {
                        cx: w / 2,
                        cy: h / 2,
                        half_w: half,
                        half_h: half,
                    };
                    self.request_redraw();
                }
                AppEffect::TakeScreenshot {
                    cx,
                    cy,
                    half_w,
                    half_h,
                } => {
                    let (w, h) = self.surface_size;
                    let x = cx.saturating_sub(half_w);
                    let y = cy.saturating_sub(half_h);
                    let sw = (half_w * 2).min(w.saturating_sub(x));
                    let sh = (half_h * 2).min(h.saturating_sub(y));
                    self.pending_screenshot = Some([x, y, sw, sh]);
                    self.state.mode = InputMode::Insert;
                    self.request_redraw();
                }
            }
        }
        let focus_after = (
            self.state.active_tab,
            self.state.tabs[self.state.active_tab].active,
        );
        if focus_before != focus_after {
            self.send_pane_focus_seq(focus_before.0, focus_before.1, false);
            self.send_pane_focus_seq(focus_after.0, focus_after.1, true);
        }
    }

    // ── Focus reporting ───────────────────────────────────────────────────────

    fn send_pane_focus_seq(&mut self, tab_idx: usize, pane_id: usize, gained: bool) {
        if let Some(entry) = self
            .state
            .tabs
            .get_mut(tab_idx)
            .and_then(|t| t.panes.get_mut(&pane_id))
            && entry.pane.parser.grid.focus_report
        {
            let seq: &[u8] = if gained { b"\x1b[I" } else { b"\x1b[O" };
            let _ = entry.pty.write_input(seq);
        }
    }

    // ── Effect helpers ────────────────────────────────────────────────────────

    fn do_auto_split(&mut self) {
        let active = self.tab().active;
        let rect = self
            .tab()
            .layout
            .rects()
            .into_iter()
            .find(|(id, _)| *id == active)
            .map(|(_, r)| r)
            .unwrap_or([0, TAB_BAR_H, 100, 100]);
        let dir = if rect[2] >= rect[3] {
            SplitDir::H
        } else {
            SplitDir::V
        };
        self.do_split(dir);
    }

    fn do_resize_pane(&mut self, split_h: bool, delta: f32) {
        let active = self.tab().active;
        let ai = self.state.active_tab;
        self.state.tabs[ai]
            .layout
            .nudge_pane(active, split_h, delta);
        Self::sync_pane_sizes_tab(&mut self.state.tabs[ai]);
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn do_rotate_panes(&mut self, forward: bool) {
        let ai = self.state.active_tab;
        self.state.tabs[ai].layout.rotate_leaves(forward);
        Self::sync_pane_sizes_tab(&mut self.state.tabs[ai]);
        self.request_redraw();
    }

    fn do_toggle_log(&mut self) {
        let active = self.tab().active;
        let log_dir = self.state.config.logging.log_dir.clone();
        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
            if entry.log_file.is_some() {
                entry.log_file = None;
                log::info!("Logging stopped for pane {active}");
            } else {
                entry.log_file = open_log_file(active, &log_dir);
            }
        }
    }

    fn do_paste(&mut self) {
        let text = self
            .state
            .clipboard
            .as_mut()
            .and_then(|cb| cb.get_text().ok())
            .or_else(|| Clipboard::new().ok()?.get_text().ok());
        if let Some(text) = text {
            let active = self.tab().active;
            if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                let data = bracketed_paste_encode(&text, entry.pane.parser.grid.bracketed_paste);
                let _ = entry.pty.write_input(&data);
            }
        } else {
            log::warn!("Clipboard read failed");
        }
    }

    fn do_toggle_fullscreen(&mut self) {
        if let Some(w) = &self.window {
            let fs = if w.fullscreen().is_some() {
                None
            } else {
                Some(Fullscreen::Borderless(None))
            };
            w.set_fullscreen(fs);
        }
    }

    fn do_new_tab(&mut self) {
        let (w, h) = self
            .window
            .as_ref()
            .map(|win| {
                let s = win.inner_size();
                (s.width, s.height)
            })
            .unwrap_or((800, 600));
        self.new_tab(w, h);
    }

    fn do_send_to_pty(&mut self, bytes: Vec<u8>) {
        let active = self.tab().active;
        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
            let _ = entry.pty.write_input(&bytes);
        }
    }

    // ── Config panel ──────────────────────────────────────────────────────────

    fn change_font_size(&mut self, delta: f32) {
        let current = self.state.tabs[self.state.active_tab].metrics.font_px;
        let Some(new_size) = font::apply_delta(current, delta) else {
            return;
        };
        let new_metrics = self.renderer.make_metrics(new_size);
        let idx = self.state.active_tab;
        self.state.tabs[idx].metrics = new_metrics;
        Self::sync_pane_sizes_tab(&mut self.state.tabs[idx]);
        log::info!("Tab {} font size: {current} → {new_size}", idx + 1);
    }

    // ── Render helpers ────────────────────────────────────────────────────────

    // ── Render ────────────────────────────────────────────────────────────────

    fn redraw(&mut self) {
        if self.state.blink_last.elapsed()
            >= Duration::from_millis(self.state.config.window.cursor_blink_ms as u64)
        {
            self.state.blink_last = Instant::now();
            self.state.cursor_blink = !self.state.cursor_blink;
        }

        let Some(surface) = &mut self.surface else {
            return;
        };
        let Some(window) = &self.window else { return };
        let size = window.inner_size();
        let (w, h) = (size.width, size.height);
        if w == 0 || h == 0 {
            return;
        }

        if self.surface_size != (w, h)
            && let (Ok(wn), Ok(hn)) = (NonZeroU32::try_from(w), NonZeroU32::try_from(h))
        {
            let _ = surface.resize(wn, hn);
            self.surface_size = (w, h);
        }

        let mut buf = surface.buffer_mut().unwrap();
        let pixels: &mut [u32] = &mut buf;

        if self.state.tabs.is_empty() {
            buf.present().unwrap();
            return;
        }

        self.state.tabs[self.state.active_tab].has_activity = false;

        let (separators, zoomed, active_id) = {
            let tab = &self.state.tabs[self.state.active_tab];
            (tab.layout.separators(), tab.zoomed, tab.active)
        };

        let views = views::collect_pane_views(&self.state, w, h);
        let tab_titles = views::build_tab_titles(&self.state);

        let metrics = self.state.tabs[self.state.active_tab].metrics.clone();
        let draw_separators: &[[u32; 4]] = if zoomed { &[] } else { &separators };
        let home = std::env::var("HOME").unwrap_or_default();
        let cwd_owned: Option<String> = self.state.tabs[self.state.active_tab]
            .panes
            .get(&active_id)
            .and_then(|e| e.pane.parser.grid.cwd.as_deref())
            .map(|p| statusbar::shorten_home(p, &home));
        let right_text = statusbar::resolve(
            &self.state.config.status_bar.right,
            cwd_owned.as_deref(),
            &Local::now(),
        );
        const BELL_DURATION_MS: f32 = 150.0;
        let bell_flash_intensity = self.state.tabs[self.state.active_tab]
            .bell_flash_start
            .and_then(|start| {
                let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
                if elapsed_ms >= BELL_DURATION_MS {
                    None
                } else {
                    let t = elapsed_ms / BELL_DURATION_MS;
                    // ease-out: 1 - t^2 (fast peak, gradual fade)
                    Some(1.0 - t * t)
                }
            });
        let is_logging = self.state.tabs[self.state.active_tab]
            .panes
            .get(&active_id)
            .is_some_and(|e| e.log_file.is_some());
        let pane_title_raw = self.state.tabs[self.state.active_tab]
            .panes
            .get(&active_id)
            .and_then(|e| e.pane.parser.grid.osc_title.as_deref());
        let pwd_in_right = self.state.config.status_bar.right.contains("%pwd");
        let pane_title =
            statusbar::pane_title_for_display(pane_title_raw, pwd_in_right, cwd_owned.as_deref());
        self.renderer.draw(
            pixels,
            w,
            h,
            &views,
            draw_separators,
            &self.state.mode,
            self.state.tabs[self.state.active_tab].passthrough,
            &tab_titles,
            &metrics,
            self.state.search_matches.len(),
            self.state.search_current,
            right_text.as_deref(),
            pane_title,
            self.state.config.window.inactive_dim,
            bell_flash_intensity,
            self.state.config.general.visual_bell,
            is_logging,
            &self.state.theme,
        );

        if let Some([px, py, pw, ph]) = self.pending_screenshot.take() {
            match save_screenshot(
                pixels,
                w,
                px,
                py,
                pw,
                ph,
                &self.state.config.general.screenshot_dir,
            ) {
                Ok(path) => self
                    .state
                    .copy_text_to_clipboard(path.to_string_lossy().into_owned()),
                Err(e) => log::warn!("screenshot save failed: {e}"),
            }
        }

        draw_overlays(&mut self.renderer, &self.state, pixels, w, h);
        buf.present().unwrap();
    }

    fn handle_focus_changed(&mut self, gained: bool) {
        if gained {
            self.state.swallow_next_tab = true;
        } else {
            self.modifiers = Modifiers::default();
        }
        let active_tab = self.state.active_tab;
        let tab_active = self.state.tabs[active_tab].active;
        self.send_pane_focus_seq(active_tab, tab_active, gained);
    }

    fn handle_redraw_requested(&mut self, event_loop: &ActiveEventLoop) {
        let (exited, has_more) = self.drain_all();
        for (tab_idx, pane_id) in exited {
            self.close_pane_on_tab(tab_idx, pane_id, event_loop);
        }
        self.redraw();
        if has_more && let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

fn draw_overlays(renderer: &mut Renderer, state: &AppState, pixels: &mut [u32], w: u32, h: u32) {
    if let Some(panel) = &state.config_panel {
        renderer.draw_config_panel(pixels, w, h, panel);
    }
    if let InputMode::CommandPalette { query, selected } = &state.mode {
        let filtered = command_palette::filter(query);
        let entries: Vec<(&str, &str)> = filtered
            .iter()
            .map(|&i| {
                (
                    command_palette::entry_label(i),
                    command_palette::entry_shortcut(i),
                )
            })
            .collect();
        renderer.draw_command_palette(pixels, w, h, query, &entries, *selected);
    }
    if state.quit_pending {
        renderer.draw_quit_confirm(pixels, w, h, &state.theme);
    }
    if matches!(state.mode, InputMode::QuitSave) {
        renderer.draw_save_session_confirm(pixels, w, h, &state.theme);
    }
    if let InputMode::Screenshot {
        cx,
        cy,
        half_w,
        half_h,
    } = state.mode
    {
        renderer.draw_screenshot_selector(pixels, w, h, cx, cy, half_w, half_h);
    }
}

fn expand_tilde(path: &str) -> std::path::PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(rest),
        None => std::path::PathBuf::from(path),
    }
}

fn pixel_to_rgb(p: u32) -> [u8; 3] {
    [
        ((p >> 16) & 0xff) as u8,
        ((p >> 8) & 0xff) as u8,
        (p & 0xff) as u8,
    ]
}

#[allow(clippy::too_many_arguments)]
fn save_screenshot(
    buf: &[u32],
    buf_width: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    dir: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let dir = expand_tilde(dir);
    use anyhow::Context as _;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("cannot create screenshot directory {}", dir.display()))?;

    let rgb: Vec<u8> = (y..y + h)
        .flat_map(|row| (x..x + w).map(move |col| (row * buf_width + col) as usize))
        .flat_map(|idx| pixel_to_rgb(buf.get(idx).copied().unwrap_or(0)))
        .collect();

    let timestamp = chrono::Local::now().format("%Y%m%dT%H%M%S");
    let filename = format!("mmterm-{timestamp}.png");
    let path = dir.join(&filename);
    let img = image::RgbImage::from_raw(w, h, rgb)
        .ok_or_else(|| anyhow::anyhow!("invalid image dimensions {w}x{h}"))?;
    img.save(&path)
        .with_context(|| format!("cannot write PNG to {}", path.display()))?;
    log::info!("screenshot saved: {}", path.display());
    Ok(path)
}

impl App {
    fn handle_resize(&mut self, w: u32, h: u32) {
        for tab in &mut self.state.tabs {
            tab.layout.resize(w, h);
        }
        self.sync_all_pane_sizes();
    }

    fn should_swallow_key(&mut self, event: &KeyEvent) -> bool {
        if self.state.swallow_next_tab {
            self.state.swallow_next_tab = false;
            if event.logical_key == Key::Named(NamedKey::Tab) {
                return true;
            }
        }
        false
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
        let icon = image::load_from_memory(ICON_PNG)
            .map(|img| {
                let rgba = img.into_rgba8();
                let (w, h) = rgba.dimensions();
                Icon::from_rgba(rgba.into_raw(), w, h).ok()
            })
            .ok()
            .flatten();

        let attrs = Window::default_attributes()
            .with_title(self.state.config.window.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.state.config.window.width,
                self.state.config.window.height,
            ))
            .with_window_icon(icon);

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        window.set_cursor(CursorIcon::Text);
        let ctx = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&ctx, window.clone()).unwrap();

        let size = window.inner_size();
        let session_path = session::session_path_for(self.scope.as_deref());
        let did_restore = self.state.config.general.restore_session
            && session::load_from(&session_path)
                .map(|s| self.restore_session(s, size.width, size.height))
                .unwrap_or(false);
        if !did_restore {
            self.new_tab(size.width, size.height);
        }

        self.surface = Some(surface);
        self.window = Some(window.clone());
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.execute_action(Action::Quit, event_loop);
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(size.width, size.height);
            }
            WindowEvent::Focused(gained) => {
                self.handle_focus_changed(gained);
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods;
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !self.should_swallow_key(&event) =>
            {
                self.handle_keyboard_input(event, event_loop);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_moved(position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_input(state, button);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(delta);
            }
            WindowEvent::RedrawRequested => {
                self.handle_redraw_requested(event_loop);
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        // Clear the flag before requesting redraw so PTY threads can queue
        // the next wakeup as soon as more data arrives after this frame.
        self.wakeup_pending.store(false, Ordering::Release);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        const FRAME_16MS: Duration = Duration::from_millis(16);

        // While PTY data is flowing, keep rendering at ~60fps so output
        // appears progressively instead of in large batches.
        if self.last_pty_data.is_some() {
            self.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + FRAME_16MS));
            return;
        }

        let blink_dur = Duration::from_millis(self.state.config.window.cursor_blink_ms as u64);
        let elapsed = self.state.blink_last.elapsed();
        if elapsed >= blink_dur {
            self.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + blink_dur));
        } else {
            let default_next = Instant::now() + (blink_dur - elapsed);
            event_loop.set_control_flow(ControlFlow::WaitUntil(next_bell_wakeup(
                &self.state.tabs,
                default_next,
            )));
        }
    }
}

fn next_bell_wakeup(tabs: &[TabState], default: Instant) -> Instant {
    let now = Instant::now();
    let mut next = default;
    for tab in tabs {
        if let Some(expiry) = tab.bell_flash_until
            && expiry > now
            && expiry < next
        {
            next = expiry;
        }
    }
    next
}

fn open_log_file(pane_id: usize, log_dir: &str) -> Option<std::fs::File> {
    let dir = logging::resolve_log_dir(log_dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Failed to create log directory {dir}: {e}");
        return None;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = logging::log_file_path(&dir, ts, pane_id);
    match std::fs::File::create(&path) {
        Ok(f) => {
            log::info!("Logging started: {path}");
            Some(f)
        }
        Err(e) => {
            log::warn!("Failed to open log file {path}: {e}");
            None
        }
    }
}

fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", url])
            .spawn();
    }
}

/// Returns `true` if `--version` or `-V` appears in the given argument iterator.
pub(crate) fn version_requested(mut args: impl Iterator<Item = String>) -> bool {
    args.any(|a| a == "--version" || a == "-V")
}

/// Returns `true` if `--help` or `-h` appears in the given argument iterator.
pub(crate) fn help_requested(mut args: impl Iterator<Item = String>) -> bool {
    args.any(|a| a == "--help" || a == "-h")
}

pub(crate) fn print_help() {
    println!(
        "mmterm {version}

A cross-platform CPU-rendered terminal emulator.

Usage: mmterm [OPTIONS]

Options:
  --version, -V       Print version and exit
  --help,    -h       Print this help and exit
  --debug             Enable debug logging to ~/.mmterm/debug-<ts>.log
  --scope <name>      Use a named session scope (~/.config/mmterm/sessions/<name>.toml)
  --scope=<name>      Same as --scope <name>
  -s <name>           Short form of --scope
  --list-scopes       Print all saved scope names and exit",
        version = env!("MMTERM_VERSION")
    );
}

/// Extracts the `--scope <name>` / `--scope=<name>` / `-s <name>` value from args.
pub(crate) fn scope_from_args(args: impl Iterator<Item = String>) -> Option<String> {
    let args: Vec<String> = args.collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--scope" || args[i] == "-s" {
            return args.get(i + 1).cloned();
        }
        if let Some(val) = args[i].strip_prefix("--scope=") {
            return Some(val.to_string());
        }
        i += 1;
    }
    None
}

/// Returns `true` if `--list-scopes` appears in the given argument iterator.
pub(crate) fn list_scopes_requested(mut args: impl Iterator<Item = String>) -> bool {
    args.any(|a| a == "--list-scopes")
}

/// Returns the debug log path when `--debug` is in argv, otherwise `None`.
pub fn debug_log_path() -> Option<String> {
    if !std::env::args().any(|a| a == "--debug") {
        return None;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = format!("{home}/.mmterm");
    std::fs::create_dir_all(&dir).ok()?;
    let ts = chrono::Local::now().format("%Y%m%dT%H%M%S");
    Some(format!("{dir}/debug-{ts}.log"))
}

fn init_logging(log_path: Option<&str>) {
    let level = if log_path.is_some() {
        log::LevelFilter::Debug
    } else {
        // Respect RUST_LOG when not in debug mode, defaulting to Warn.
        std::env::var("RUST_LOG")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(log::LevelFilter::Warn)
    };

    let mut dispatch = fern::Dispatch::new().level(level).chain(
        fern::Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!("[{}] {}", record.level(), message))
            })
            .chain(std::io::stderr()),
    );

    if let Some(path) = log_path {
        match fern::log_file(path) {
            Ok(file) => {
                dispatch = dispatch.chain(
                    fern::Dispatch::new()
                        .format(|out, message, record| {
                            out.finish(format_args!(
                                "{ts} [{level}] {target} — {msg}",
                                ts = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f"),
                                level = record.level(),
                                target = record.target(),
                                msg = message
                            ))
                        })
                        .chain(file),
                );
            }
            Err(e) => {
                eprintln!("mmterm: could not open debug log {path}: {e}");
            }
        }
    }

    if let Err(e) = dispatch.apply() {
        eprintln!("mmterm: logging init failed: {e}");
    }
}

fn main() {
    if version_requested(std::env::args()) {
        println!("mmterm {}", env!("MMTERM_VERSION"));
        return;
    }

    if help_requested(std::env::args()) {
        print_help();
        return;
    }

    let log_path = debug_log_path();
    init_logging(log_path.as_deref());

    if let Some(ref path) = log_path {
        // Install panic hook so the log location is always visible on crash.
        let p = path.clone();
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log::error!("panic: {info}");
            default_hook(info);
            eprintln!("\nmmterm: debug log saved to {p}");
        }));
        log::info!("debug logging enabled → {path}");
    }

    if list_scopes_requested(std::env::args()) {
        for name in session::list_scopes() {
            println!("{name}");
        }
        return;
    }

    let scope = scope_from_args(std::env::args());

    Config::write_default_if_missing();
    let config = Config::load();
    let event_loop = EventLoop::new().unwrap();
    let proxy = event_loop.create_proxy();
    let mut app = App::new(config, proxy, scope);
    event_loop.run_app(&mut app).unwrap();
}
