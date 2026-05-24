mod app_state;
mod config;
mod font;
mod geometry;
mod input;
mod logging;
mod motion;
mod mouse;
mod pty;
mod renderer;
mod search;
mod statusbar;
mod tabs;
mod terminal;
mod theme;
mod tui_config;
mod ui;

pub use app_state::{AppEffect, AppState, PaneEntry, TabState};

#[cfg(test)]
#[path = "main_test.rs"]
mod tests;

use arboard::Clipboard;
use base64::Engine as _;
use chrono::Local;
use config::Config;
use crossbeam_channel::unbounded;
use input::{InputMode, handle_ctrl_w, handle_key};
use renderer::{PaneView, Renderer};
use std::collections::HashMap;
use std::io::Write as _;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tui_config::ConfigAction;
use ui::{Layout, Pane, SplitDir};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Modifiers, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Fullscreen, Icon, Window, WindowId};

use crate::input::keybindings::Action;
use crate::terminal::grid::GridColors;
use crate::theme::{default_theme, install_bundled_themes, load_theme, themes_dir};
use crate::ui::layout::{PANE_PADDING, STATUS_BAR_H, TAB_BAR_H};

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
}

impl App {
    fn new(config: Config, proxy: EventLoopProxy<()>) -> Self {
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
            bell_flash_until: None,
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
    fn drain_all(&mut self) -> (Vec<(usize, usize)>, bool) {
        // ~256 KB per frame keeps parsing under ~4 ms while still showing
        // progressive output for commands like `find .`.
        const BYTES_PER_FRAME: usize = 256 * 1024;
        let active_tab = self.state.active_tab;
        let detect_urls = self.state.config.window.detect_urls;
        let mut exited = Vec::new();
        let mut has_more = false;
        for (tab_idx, tab) in self.state.tabs.iter_mut().enumerate() {
            let ids: Vec<usize> = tab.panes.keys().copied().collect();
            for id in ids {
                let entry = tab.panes.get_mut(&id).unwrap();
                let mut got_data = false;
                let mut bytes_this_frame = 0;
                loop {
                    match entry.rx.try_recv() {
                        Ok(bytes) => {
                            bytes_this_frame += bytes.len();
                            if let Some(f) = &mut entry.log_file {
                                let _ = f.write_all(&bytes);
                            }
                            entry.pane.process(&bytes);
                            let responses =
                                std::mem::take(&mut entry.pane.parser.grid.pending_responses);
                            if !responses.is_empty() {
                                let _ = entry.pty.write_input(&responses);
                            }
                            if let Some(text) =
                                entry.pane.parser.grid.pending_clipboard_write.take()
                                && let Some(cb) = self.state.clipboard.as_mut()
                            {
                                let _ = cb.set_text(text);
                            }
                            if std::mem::take(&mut entry.pane.parser.grid.pending_clipboard_read) {
                                let text = self
                                    .state
                                    .clipboard
                                    .as_mut()
                                    .and_then(|cb| cb.get_text().ok())
                                    .unwrap_or_default();
                                let encoded = base64::engine::general_purpose::STANDARD
                                    .encode(text.as_bytes());
                                let resp = format!("\x1b]52;c;{encoded}\x1b\\");
                                let _ = entry.pty.write_input(resp.as_bytes());
                            }
                            got_data = true;
                            if bytes_this_frame >= BYTES_PER_FRAME {
                                has_more = true;
                                break;
                            }
                        }
                        Err(crossbeam_channel::TryRecvError::Empty) => break,
                        Err(crossbeam_channel::TryRecvError::Disconnected) => {
                            exited.push((tab_idx, id));
                            break;
                        }
                    }
                }
                if got_data && detect_urls {
                    entry.pane.parser.grid.scan_urls();
                }
                if got_data && tab_idx != active_tab {
                    tab.has_activity = true;
                }
                if entry.pane.parser.grid.bell_pending {
                    entry.pane.parser.grid.bell_pending = false;
                    tab.bell_flash_until = Some(Instant::now() + Duration::from_millis(100));
                }
            }
        }
        if has_more {
            self.last_pty_data = Some(Instant::now());
        } else {
            // No more pending data — stop the vsync loop.
            self.last_pty_data = None;
        }
        (exited, has_more)
    }

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
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    // ── Action dispatch ───────────────────────────────────────────────────────

    /// Execute an action: pure state mutations go through AppState::dispatch_action;
    /// effects that need winit or the renderer are handled here.
    fn execute_action(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        let effects = self.state.dispatch_action(action);
        for effect in effects {
            match effect {
                AppEffect::Redraw => {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
                AppEffect::Quit => event_loop.exit(),
                AppEffect::QuitPending => {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
                AppEffect::ToggleFullscreen => {
                    if let Some(w) = &self.window {
                        let fs = if w.fullscreen().is_some() {
                            None
                        } else {
                            Some(Fullscreen::Borderless(None))
                        };
                        w.set_fullscreen(fs);
                    }
                }
                AppEffect::NewTab => {
                    let (w, h) = self
                        .window
                        .as_ref()
                        .map(|w| {
                            let s = w.inner_size();
                            (s.width, s.height)
                        })
                        .unwrap_or((800, 600));
                    self.new_tab(w, h);
                }
                AppEffect::ClosePane => self.do_close_pane(event_loop),
                AppEffect::CloseTab => self.close_tab(event_loop),
                AppEffect::SplitPane(dir) => self.do_split(dir),
                AppEffect::ChangeFontSize(delta) => self.change_font_size(delta),
                AppEffect::ResizePane { split_h, delta } => {
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
                AppEffect::ToggleLog => {
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
                AppEffect::SendToPty(bytes) => {
                    let active = self.tab().active;
                    if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                        let _ = entry.pty.write_input(&bytes);
                    }
                }
                AppEffect::Paste => {
                    let text = self
                        .state
                        .clipboard
                        .as_mut()
                        .and_then(|cb| cb.get_text().ok())
                        .or_else(|| Clipboard::new().ok()?.get_text().ok());
                    if let Some(text) = text {
                        let active = self.tab().active;
                        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                            let bracketed = entry.pane.parser.grid.bracketed_paste;
                            let mut data = Vec::new();
                            if bracketed {
                                data.extend_from_slice(b"\x1b[200~");
                            }
                            data.extend_from_slice(text.as_bytes());
                            if bracketed {
                                data.extend_from_slice(b"\x1b[201~");
                            }
                            let _ = entry.pty.write_input(&data);
                        }
                    } else {
                        log::warn!("Clipboard read failed");
                    }
                }
            }
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

    // ── Search ────────────────────────────────────────────────────────────────

    fn handle_search_key(&mut self, event: &winit::event::KeyEvent) {
        use winit::keyboard::{Key, NamedKey};
        let query = if let InputMode::Search { query } = &self.state.mode {
            query.clone()
        } else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.state.mode = InputMode::Normal;
                self.state.search_matches.clear();
            }
            Key::Named(NamedKey::Enter) if !self.state.search_matches.is_empty() => {
                let next = (self.state.search_current + 1) % self.state.search_matches.len();
                self.scroll_to_match(next);
            }
            Key::Named(NamedKey::Backspace) => {
                let mut q = query;
                q.pop();
                self.state.mode = InputMode::Search { query: q };
                self.update_search_matches();
            }
            // Ctrl+C or c — copy current match text to clipboard
            Key::Character(s) if s == "\x03" || s == "c" => {
                self.copy_current_match();
            }
            Key::Character(s) => {
                let mut q = query;
                q.push_str(s);
                self.state.mode = InputMode::Search { query: q };
                self.update_search_matches();
            }
            _ => {}
        }
    }

    fn copy_current_match(&mut self) {
        let Some(&(abs_row, col, len)) = self.state.search_matches.get(self.state.search_current)
        else {
            return;
        };
        let tab_idx = self.state.active_tab;
        let active = self.state.tabs[tab_idx].active;
        let Some(entry) = self.state.tabs[tab_idx].panes.get(&active) else {
            return;
        };
        let grid = &entry.pane.parser.grid;
        let text = search::extract_match_text(grid, abs_row, col, len);
        if !text.is_empty() {
            let cb = self
                .state
                .clipboard
                .get_or_insert_with(|| Clipboard::new().expect("clipboard unavailable"));
            match cb.set_text(text) {
                Ok(()) => log::info!("Copied search match to clipboard"),
                Err(e) => log::warn!("Clipboard write failed: {e}"),
            }
        }
    }

    fn update_search_matches(&mut self) {
        let query = match &self.state.mode {
            InputMode::Search { query } => query.clone(),
            _ => return,
        };

        self.state.search_matches.clear();
        self.state.search_current = 0;

        let tab_idx = self.state.active_tab;
        let active = self.state.tabs[tab_idx].active;

        if let Some(entry) = self.state.tabs[tab_idx].panes.get(&active) {
            self.state.search_matches =
                search::compute_search_matches(&entry.pane.parser.grid, &query);
        }

        if !self.state.search_matches.is_empty() {
            self.scroll_to_match(0);
        }
    }

    fn scroll_to_match(&mut self, idx: usize) {
        if idx >= self.state.search_matches.len() {
            return;
        }
        self.state.search_current = idx;
        let (abs_row, _, _) = self.state.search_matches[idx];

        let tab_idx = self.state.active_tab;
        let active = self.state.tabs[tab_idx].active;

        let (sb_len, grid_rows) = self.state.tabs[tab_idx]
            .panes
            .get(&active)
            .map(|e| (e.pane.parser.grid.scrollback.len(), e.pane.parser.grid.rows))
            .unwrap_or((0, 24));

        let new_offset = search::compute_scroll_offset(abs_row, sb_len, grid_rows);

        if let Some(entry) = self.state.tabs[tab_idx].panes.get_mut(&active) {
            entry.pane.scroll_offset = new_offset;
        }
    }

    fn apply_config(&mut self, new_cfg: Config, window: &Window) {
        let td = themes_dir();
        let new_theme = load_theme(&new_cfg.theme.name, &td).unwrap_or_else(|e| {
            log::warn!("{e} — keeping current theme");
            self.state.theme.clone()
        });
        self.state.theme = new_theme;
        self.reseed_pane_palettes();
        new_cfg.save();
        window.set_title(&new_cfg.window.title);
        self.state.config = new_cfg;
        self.state.config_panel = None;
    }

    fn reseed_pane_palettes(&mut self) {
        let t = &self.state.theme;
        for tab in &mut self.state.tabs {
            for entry in tab.panes.values_mut() {
                let g = &mut entry.pane.parser.grid;
                g.palette = t.palette;
                g.default_fg = t.foreground;
                g.default_bg = t.background;
                g.cursor_color = t.cursor;
                g.selection_color = t.selection;
            }
        }
    }

    fn handle_rename_key(&mut self, event: &winit::event::KeyEvent) {
        use winit::keyboard::{Key, NamedKey};
        let buf = if let InputMode::RenameTab { buf } = &self.state.mode {
            buf.clone()
        } else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.state.mode = InputMode::Insert;
            }
            Key::Named(NamedKey::Enter) => {
                let name = buf.trim().to_string();
                self.state.tabs[self.state.active_tab].name =
                    if name.is_empty() { None } else { Some(name) };
                self.state.mode = InputMode::Insert;
            }
            Key::Named(NamedKey::Backspace) => {
                let mut b = buf;
                b.pop();
                self.state.mode = InputMode::RenameTab { buf: b };
            }
            Key::Character(s) => {
                let mut b = buf;
                b.push_str(s);
                self.state.mode = InputMode::RenameTab { buf: b };
            }
            _ => {}
        }
    }

    fn handle_config_key(&mut self, event: &winit::event::KeyEvent) {
        use winit::keyboard::{Key, NamedKey};
        let ctrl = self.modifiers.state().control_key();
        let panel = match &mut self.state.config_panel {
            Some(p) => p,
            None => return,
        };

        let action = match &event.logical_key {
            Key::Named(NamedKey::Escape) => panel.handle_escape(),
            Key::Named(NamedKey::Enter) => panel.handle_char('\r'),
            Key::Named(NamedKey::Backspace) => panel.handle_backspace(),
            Key::Named(NamedKey::ArrowUp) => panel.handle_up(),
            Key::Named(NamedKey::ArrowDown) => panel.handle_down(),
            Key::Named(NamedKey::ArrowLeft) => panel.handle_left(),
            Key::Named(NamedKey::ArrowRight) => panel.handle_right(),
            Key::Named(NamedKey::Space) => panel.handle_char(' '),
            Key::Character(s) => {
                if ctrl && s.eq_ignore_ascii_case("s") {
                    panel.save()
                } else {
                    let c = s.chars().next().unwrap_or(' ');
                    panel.handle_char(c)
                }
            }
            _ => ConfigAction::None,
        };

        match action {
            ConfigAction::Save(cfg) => {
                let window = self.window.clone();
                if let Some(w) = window {
                    self.apply_config(*cfg, &w);
                }
            }
            ConfigAction::Cancel => {
                self.state.config_panel = None;
            }
            ConfigAction::PreviewTheme(name) => {
                let td = themes_dir();
                match load_theme(&name, &td) {
                    Ok(t) => {
                        self.state.theme = t;
                        self.reseed_pane_palettes();
                    }
                    Err(e) => log::warn!("{e}"),
                }
            }
            ConfigAction::None => {}
        }
    }

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

        let tab = &self.state.tabs[self.state.active_tab];
        let rects = tab.layout.rects();
        let separators = tab.layout.separators();

        let active_id = tab.active;
        let zoomed = tab.zoomed;
        let has_search = !self.state.search_matches.is_empty();
        let search_matches = &self.state.search_matches;
        let search_current_val = self.state.search_current;

        let views: Vec<PaneView> = if zoomed {
            let entry = tab.panes.get(&active_id);
            if let Some(entry) = entry {
                // cursor_visible is ignored in Insert mode: Ink and other TUI
                // frameworks hide the terminal cursor (?25l) during rendering
                // and sometimes do not restore it, leaving the cursor permanently
                // hidden. In mmterm's Insert mode the user always needs to see
                // the cursor, so we honour only our own modal state here.
                let show_cursor = tabs::should_show_cursor(
                    true,
                    matches!(self.state.mode, InputMode::Insert),
                    self.state.cursor_blink,
                    entry.pane.scroll_offset,
                );
                let (sm, sc) = if has_search {
                    (search_matches.as_slice(), Some(search_current_val))
                } else {
                    (&[][..], None)
                };
                vec![PaneView {
                    grid: &entry.pane.parser.grid,
                    rect: [0, TAB_BAR_H, w, h.saturating_sub(TAB_BAR_H + STATUS_BAR_H)],
                    scroll_offset: entry.pane.scroll_offset,
                    is_active: true,
                    show_cursor,
                    blink_visible: self.state.cursor_blink,
                    search_matches: sm,
                    search_current: sc,
                    hovered_url: self.state.hovered_url.as_deref(),
                    cursor_shape: entry.pane.parser.grid.cursor_shape,
                }]
            } else {
                vec![]
            }
        } else {
            rects
                .iter()
                .filter_map(|(id, rect)| {
                    let entry = tab.panes.get(id)?;
                    let is_active = *id == active_id;
                    let show_cursor = tabs::should_show_cursor(
                        is_active,
                        matches!(self.state.mode, InputMode::Insert),
                        self.state.cursor_blink,
                        entry.pane.scroll_offset,
                    );
                    let (sm, sc) = if is_active && has_search {
                        (search_matches.as_slice(), Some(search_current_val))
                    } else {
                        (&[][..], None)
                    };
                    Some(PaneView {
                        grid: &entry.pane.parser.grid,
                        rect: *rect,
                        scroll_offset: entry.pane.scroll_offset,
                        is_active,
                        show_cursor,
                        blink_visible: self.state.cursor_blink,
                        search_matches: sm,
                        search_current: sc,
                        hovered_url: self.state.hovered_url.as_deref(),
                        cursor_shape: entry.pane.parser.grid.cursor_shape,
                    })
                })
                .collect()
        };

        let tab_titles: Vec<(String, bool, bool)> = self
            .state
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let is_active = i == self.state.active_tab;
                let osc_title = tab
                    .panes
                    .get(&tab.active)
                    .and_then(|e| e.pane.parser.grid.osc_title.as_deref())
                    .filter(|t| !t.starts_with('/') && !t.starts_with('~'));
                let rename_buf = if is_active {
                    if let InputMode::RenameTab { buf } = &self.state.mode {
                        Some(buf.as_str())
                    } else {
                        None
                    }
                } else {
                    None
                };
                let label =
                    tabs::tab_label(i, tab.name.as_deref(), osc_title, is_active, rename_buf);
                (label, is_active, tab.has_activity)
            })
            .collect();

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
        let bell_flash = self.state.tabs[self.state.active_tab]
            .bell_flash_until
            .is_some_and(|t| t > Instant::now());
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
            &tab_titles,
            &metrics,
            self.state.search_matches.len(),
            self.state.search_current,
            right_text.as_deref(),
            pane_title,
            self.state.config.window.inactive_dim,
            bell_flash,
            is_logging,
            &self.state.theme,
        );

        if let Some(panel) = &self.state.config_panel {
            self.renderer.draw_config_panel(pixels, w, h, panel);
        }

        if self.state.quit_pending {
            self.renderer
                .draw_quit_confirm(pixels, w, h, &self.state.theme);
        }

        buf.present().unwrap();
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
        self.new_tab(size.width, size.height);

        self.surface = Some(surface);
        self.window = Some(window.clone());
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                for tab in &mut self.state.tabs {
                    tab.layout.resize(size.width, size.height);
                }
                self.sync_all_pane_sizes();
            }

            WindowEvent::Focused(gained) => {
                if gained {
                    // The Tab from the Alt+Tab that brought focus back may
                    // arrive as a plain Tab (Alt already released by the WM).
                    // Mark it to be swallowed so it isn't sent to the PTY.
                    self.state.swallow_next_tab = true;
                } else {
                    // Clear modifier state — the WM won't send release events
                    // for keys held when focus leaves.
                    self.modifiers = Modifiers::default();
                }
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods;
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }

                // Swallow the first Tab that arrives after regaining focus —
                // it is the Tab from the Alt+Tab that transferred focus to us.
                if self.state.swallow_next_tab {
                    self.state.swallow_next_tab = false;
                    if event.logical_key == Key::Named(NamedKey::Tab) {
                        return;
                    }
                }

                // Reset blink on every keypress so cursor is always visible after input.
                self.state.cursor_blink = true;
                self.state.blink_last = Instant::now();

                if self.state.quit_pending {
                    let confirmed = matches!(
                        event.logical_key,
                        Key::Character(ref s) if s.eq_ignore_ascii_case("y")
                    );
                    self.state.quit_pending = false;
                    if confirmed {
                        event_loop.exit();
                    } else if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }

                if self.state.config_panel.is_some() {
                    self.handle_config_key(&event);
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }

                if matches!(self.state.mode, InputMode::RenameTab { .. }) {
                    self.handle_rename_key(&event);
                    return;
                }

                if matches!(self.state.mode, InputMode::Search { .. }) {
                    self.handle_search_key(&event);
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }

                if self.state.ctrl_w_pending {
                    self.state.ctrl_w_pending = false;
                    let action = handle_ctrl_w(&event);
                    self.execute_action(action, event_loop);
                    return;
                }

                let (grid_cols, grid_rows, app_cursor) = {
                    let tab = self.tab();
                    tab.panes
                        .get(&tab.active)
                        .map(|e| {
                            (
                                e.pane.parser.grid.cols,
                                e.pane.parser.grid.rows,
                                e.pane.parser.grid.application_cursor_keys,
                            )
                        })
                        .unwrap_or((80, 24, false))
                };

                let action = handle_key(
                    &event,
                    &self.modifiers,
                    &self.state.mode,
                    grid_cols,
                    grid_rows,
                    app_cursor,
                );
                self.execute_action(action, event_loop);
            }

            WindowEvent::CursorMoved { position, .. } => {
                let (px, py) = (position.x, position.y);
                self.state.mouse_pos = Some((px, py));

                // Active separator drag takes full priority
                if let Some(handle) = self.state.drag_separator {
                    let new_pos = match handle.dir {
                        SplitDir::H => px as u32,
                        SplitDir::V => py as u32,
                    };
                    let ai = self.state.active_tab;
                    self.state.tabs[ai].layout.move_separator(handle, new_pos);
                    Self::sync_pane_sizes_tab(&mut self.state.tabs[ai]);
                    let icon = match handle.dir {
                        SplitDir::H => CursorIcon::ColResize,
                        SplitDir::V => CursorIcon::RowResize,
                    };
                    if let Some(w) = &self.window {
                        w.set_cursor(icon);
                        w.request_redraw();
                    }
                    return;
                }

                // Hover detection: show resize cursor near separators
                let hover_sep = {
                    let tab = &self.state.tabs[self.state.active_tab];
                    if !tab.zoomed {
                        tab.layout.separator_at_pixel(px as u32, py as u32, 4)
                    } else {
                        None
                    }
                };

                let url = self.url_at_pixel(px, py);
                let icon = if let Some(h) = &hover_sep {
                    match h.dir {
                        SplitDir::H => CursorIcon::ColResize,
                        SplitDir::V => CursorIcon::RowResize,
                    }
                } else if url.is_some() {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Text
                };
                if let Some(w) = &self.window {
                    w.set_cursor(icon);
                }
                let url_changed = self.state.hovered_url != url;
                self.state.hovered_url = url;
                if url_changed && let Some(w) = &self.window {
                    w.request_redraw();
                }
                let (mouse_mode, mouse_sgr) = self.active_mouse_mode();
                if mouse_mode >= 1002 {
                    // Button-motion or any-motion: report if button is held (selecting) or always
                    let report = mouse_mode >= 1003 || self.state.mouse_selecting;
                    if report {
                        let active = self.tab().active;
                        if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                            // btn 32 = motion with no button, 32 = left held already encoded as 32
                            let btn = if self.state.mouse_selecting { 32 } else { 35 };
                            self.send_mouse_event(btn, col, row, false, mouse_sgr);
                        }
                    }
                } else if self.state.mouse_selecting {
                    self.update_mouse_selection(px, py);
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                // End an active separator drag before any other handling
                if button == MouseButton::Left
                    && state == ElementState::Released
                    && self.state.drag_separator.take().is_some()
                {
                    return;
                }

                // Separator click takes priority over PTY mouse and text selection
                if button == MouseButton::Left
                    && state == ElementState::Pressed
                    && let Some((mx, my)) = self.state.mouse_pos
                {
                    let sep = {
                        let tab = &self.state.tabs[self.state.active_tab];
                        if !tab.zoomed {
                            tab.layout.separator_at_pixel(mx as u32, my as u32, 4)
                        } else {
                            None
                        }
                    };
                    if let Some(handle) = sep {
                        self.state.drag_separator = Some(handle);
                        return;
                    }
                }

                let btn_code = match button {
                    MouseButton::Left => 0u8,
                    MouseButton::Middle => 1u8,
                    MouseButton::Right => 2u8,
                    _ => 3u8,
                };
                let (mouse_mode, mouse_sgr) = self.active_mouse_mode();
                if mouse_mode >= 1000 && btn_code < 3 {
                    // Forward event to PTY
                    if let Some((px, py)) = self.state.mouse_pos {
                        let active = self.tab().active;
                        if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                            let release = state == ElementState::Released;
                            self.send_mouse_event(btn_code, col, row, release, mouse_sgr);
                        }
                    }
                    // Track left-button held state so motion reporting knows
                    if button == MouseButton::Left {
                        self.state.mouse_selecting = state == ElementState::Pressed;
                    }
                    return;
                }

                if button == MouseButton::Left {
                    match state {
                        ElementState::Pressed => {
                            if let Some((mx, my)) = self.state.mouse_pos {
                                self.start_mouse_selection(mx, my);
                            }
                        }
                        ElementState::Released => {
                            if self.state.mouse_selecting {
                                self.state.mouse_selecting = false;
                                self.finish_mouse_selection();
                            }
                        }
                    }
                } else if button == MouseButton::Middle {
                    // Middle-click paste
                    if state == ElementState::Pressed {
                        let text = self
                            .state
                            .clipboard
                            .as_mut()
                            .and_then(|cb| cb.get_text().ok())
                            .or_else(|| Clipboard::new().ok()?.get_text().ok());
                        if let Some(text) = text {
                            let active = self.tab().active;
                            if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                                let mut data = b"\x1b[200~".to_vec();
                                data.extend_from_slice(text.as_bytes());
                                data.extend_from_slice(b"\x1b[201~");
                                let _ = entry.pty.write_input(&data);
                            }
                        }
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as f32,
                };
                let (mouse_mode, mouse_sgr) = self.active_mouse_mode();
                if mouse_mode >= 1000 {
                    // btn 64 = scroll up, 65 = scroll down
                    let steps = lines.abs().ceil() as usize;
                    let btn = if lines > 0.0 { 64u8 } else { 65u8 };
                    if let Some((px, py)) = self.state.mouse_pos {
                        let active = self.tab().active;
                        if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                            for _ in 0..steps.max(1) {
                                self.send_mouse_event(btn, col, row, false, mouse_sgr);
                            }
                        }
                    }
                } else if lines > 0.0 {
                    let n = lines.ceil() as usize;
                    let active = self.tab().active;
                    if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                        entry.pane.scroll_up(n);
                    }
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                } else {
                    let n = (-lines).ceil() as usize;
                    let active = self.tab().active;
                    if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                        entry.pane.scroll_down(n);
                    }
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                let (exited, has_more) = self.drain_all();
                for (tab_idx, pane_id) in exited {
                    self.close_pane_on_tab(tab_idx, pane_id, event_loop);
                }
                self.redraw();
                if has_more && let Some(w) = &self.window {
                    w.request_redraw();
                }
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
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + FRAME_16MS));
            return;
        }

        let blink_dur = Duration::from_millis(self.state.config.window.cursor_blink_ms as u64);
        let elapsed = self.state.blink_last.elapsed();
        if elapsed >= blink_dur {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + blink_dur));
        } else {
            let mut next = Instant::now() + (blink_dur - elapsed);
            // Wake up early if a bell flash is still active so we clear it on expiry.
            for tab in &self.state.tabs {
                if let Some(expiry) = tab.bell_flash_until
                    && expiry > Instant::now()
                    && expiry < next
                {
                    next = expiry;
                }
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(next));
        }
    }
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

    Config::write_default_if_missing();
    let config = Config::load();
    let event_loop = EventLoop::new().unwrap();
    let proxy = event_loop.create_proxy();
    let mut app = App::new(config, proxy);
    event_loop.run_app(&mut app).unwrap();
}
