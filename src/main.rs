mod config;
mod input;
mod pty;
mod renderer;
mod terminal;
mod tui_config;
mod ui;

use arboard::Clipboard;
use config::Config;
use crossbeam_channel::{Receiver, unbounded};
use input::{InputMode, handle_ctrl_w, handle_key};
use renderer::{FontMetrics, PaneView, Renderer};
use std::collections::HashMap;
use std::io::Write as _;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tui_config::{ConfigAction, ConfigPanel};
use ui::{Layout, Pane, SplitDir};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Modifiers, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Icon, Window, WindowId};

use crate::input::keybindings::Action;
use crate::ui::layout::{STATUS_BAR_H, TAB_BAR_H};

// ── Per-pane state ───────────────────────────────────────────────────────────

struct PaneEntry {
    pane: Pane,
    pty: pty::PtySession,
    rx: Receiver<Vec<u8>>,
    /// Active log file; None when logging is off. Dropped (closed) on toggle-off.
    log_file: Option<std::fs::File>,
}

// ── Per-tab state ────────────────────────────────────────────────────────────

struct TabState {
    panes: HashMap<usize, PaneEntry>,
    layout: Layout,
    active: usize,
    /// Session-only font metrics — not saved to config
    metrics: FontMetrics,
    /// Optional user-defined name; falls back to numeric index
    name: Option<String>,
    /// Temporary full-screen zoom of the active pane
    zoomed: bool,
    /// True when PTY output arrived while this tab was not active
    has_activity: bool,
    /// Set when a BEL is received; expires after a short flash duration
    bell_flash_until: Option<Instant>,
}

// ── App ──────────────────────────────────────────────────────────────────────

struct App {
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    renderer: Renderer,
    tabs: Vec<TabState>,
    active_tab: usize,
    next_pane_id: usize,
    mode: InputMode,
    modifiers: Modifiers,
    cursor_blink: bool,
    blink_last: Instant,
    ctrl_w_pending: bool,
    config: Config,
    config_panel: Option<ConfigPanel>,
    clipboard: Option<Clipboard>,
    mouse_pos: Option<(f64, f64)>,
    mouse_selecting: bool,
    proxy: EventLoopProxy<()>,
    surface_size: (u32, u32),
    search_matches: Vec<(usize, usize, usize)>,
    search_current: usize,
    hovered_url: Option<String>,
    // Swallow the first Tab keypress after regaining focus so that the Tab
    // from an Alt+Tab window switch isn't forwarded to the PTY.
    swallow_next_tab: bool,
    wakeup_pending: Arc<AtomicBool>,
}

impl App {
    fn new(config: Config, proxy: EventLoopProxy<()>) -> Self {
        let renderer = Renderer::new(&config.font.family, config.font.size);
        Self {
            window: None,
            surface: None,
            renderer,
            tabs: Vec::new(),
            active_tab: 0,
            next_pane_id: 0,
            mode: InputMode::Insert,
            modifiers: Modifiers::default(),
            cursor_blink: true,
            blink_last: Instant::now(),
            ctrl_w_pending: false,
            config_panel: None,
            config,
            clipboard: Clipboard::new().ok(),
            mouse_pos: None,
            mouse_selecting: false,
            proxy,
            surface_size: (0, 0),
            search_matches: Vec::new(),
            search_current: 0,
            hovered_url: None,
            swallow_next_tab: false,
            wakeup_pending: Arc::new(AtomicBool::new(false)),
        }
    }

    // ── Helpers that delegate to the active tab ──────────────────────────────

    fn tab(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    fn tab_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    // ── Pane spawning ────────────────────────────────────────────────────────

    fn spawn_pane_into(
        &mut self,
        tab_idx: usize,
        rect: [u32; 4],
        cwd: Option<std::path::PathBuf>,
    ) -> usize {
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        let [_, _, w, h] = rect;
        let (cols, rows) = self.tabs[tab_idx].metrics.grid_size_for(w, h);
        let c = &self.config.colors;
        let pane = Pane::new_with_colors(
            cols,
            rows,
            rect,
            c.fg(),
            c.bg(),
            c.cursor(),
            c.selection(),
            c.palette_colors(),
        );
        let (tx, rx) = unbounded::<Vec<u8>>();
        let shell = self
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
                let log_file = if self.config.logging.auto_log {
                    open_log_file(id, &self.config.logging.log_dir)
                } else {
                    None
                };
                self.tabs[tab_idx].panes.insert(
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
            .tabs
            .get(self.active_tab)
            .and_then(|t| t.panes.get(&t.active))
            .and_then(|e| e.pty.cwd());
        self.spawn_pane_into(self.active_tab, rect, cwd)
    }

    // ── Tab management ───────────────────────────────────────────────────────

    fn new_tab(&mut self, win_w: u32, win_h: u32) {
        let cwd = self
            .tabs
            .get(self.active_tab)
            .and_then(|t| t.panes.get(&t.active))
            .and_then(|e| e.pty.cwd());
        let metrics = self.renderer.make_metrics(self.renderer.font_px);
        let layout = Layout::new(0, win_w, win_h);
        let initial_rect = layout
            .rects()
            .first()
            .map(|(_, r)| *r)
            .unwrap_or([0, TAB_BAR_H, win_w, win_h]);
        let tab_idx = self.tabs.len();
        self.tabs.push(TabState {
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
        self.tabs[tab_idx].layout = Layout::new(id, win_w, win_h);
        self.tabs[tab_idx].active = id;
        self.active_tab = tab_idx;
    }

    fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = self
                .active_tab
                .checked_sub(1)
                .unwrap_or(self.tabs.len() - 1);
        }
    }

    fn move_tab_left(&mut self) {
        if self.tabs.len() > 1 && self.active_tab > 0 {
            self.tabs.swap(self.active_tab, self.active_tab - 1);
            self.active_tab -= 1;
        }
    }

    fn move_tab_right(&mut self) {
        if self.tabs.len() > 1 && self.active_tab + 1 < self.tabs.len() {
            self.tabs.swap(self.active_tab, self.active_tab + 1);
            self.active_tab += 1;
        }
    }

    fn close_tab(&mut self, event_loop: &ActiveEventLoop) {
        if self.tabs.len() == 1 {
            event_loop.exit();
            return;
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    // ── Drain PTY output ─────────────────────────────────────────────────────

    /// Drain pending PTY output up to a per-frame byte budget. Returns
    /// (exited pairs, has_more) — callers should request another redraw when
    /// has_more is true so the display stays live during high-throughput output.
    fn drain_all(&mut self) -> (Vec<(usize, usize)>, bool) {
        // ~256 KB per frame keeps parsing under ~4 ms while still showing
        // progressive output for commands like `find .`.
        const BYTES_PER_FRAME: usize = 256 * 1024;
        let active_tab = self.active_tab;
        let detect_urls = self.config.window.detect_urls;
        let mut exited = Vec::new();
        let mut has_more = false;
        for (tab_idx, tab) in self.tabs.iter_mut().enumerate() {
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
        (exited, has_more)
    }

    fn close_pane_on_tab(&mut self, tab_idx: usize, pane_id: usize, event_loop: &ActiveEventLoop) {
        if tab_idx >= self.tabs.len() {
            return;
        }
        if self.tabs[tab_idx].panes.len() == 1 {
            if self.tabs.len() == 1 {
                event_loop.exit();
                return;
            }
            self.tabs.remove(tab_idx);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            }
            return;
        }
        let tab = &mut self.tabs[tab_idx];
        let new_focus = tab.layout.remove(pane_id);
        tab.panes.remove(&pane_id);
        if tab.active == pane_id {
            tab.active = new_focus.unwrap_or_else(|| *tab.panes.keys().next().unwrap());
        }
        Self::sync_pane_sizes_tab(&mut self.tabs[tab_idx]);
    }

    // ── Resize ───────────────────────────────────────────────────────────────

    fn sync_pane_sizes_tab(tab: &mut TabState) {
        let rects = tab.layout.rects();
        for (id, rect) in rects {
            if let Some(entry) = tab.panes.get_mut(&id) {
                let [_, _, w, h] = rect;
                let (cols, rows) = tab.metrics.grid_size_for(w, h);
                if entry.pane.parser.grid.cols != cols || entry.pane.parser.grid.rows != rows {
                    entry.pane.resize(cols, rows, rect);
                    let _ = entry.pty.resize(cols as u16, rows as u16);
                }
            }
        }
    }

    fn sync_all_pane_sizes(&mut self) {
        for tab in &mut self.tabs {
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
        let idx = self.active_tab;
        Self::sync_pane_sizes_tab(&mut self.tabs[idx]);
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
        let idx = self.active_tab;
        Self::sync_pane_sizes_tab(&mut self.tabs[idx]);
    }

    fn focus_dir(&mut self, dx: i32, dy: i32) {
        let active = self.tab().active;
        if let Some(id) = self.tab().layout.focus_dir(active, dx, dy) {
            self.tab_mut().active = id;
        }
    }

    fn focus_next(&mut self) {
        let active = self.tab().active;
        let leaves = self.tab().layout.leaves();
        if let Some(pos) = leaves.iter().position(|&id| id == active) {
            self.tab_mut().active = leaves[(pos + 1) % leaves.len()];
        }
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
        let data = if sgr {
            let suffix = if release { 'm' } else { 'M' };
            format!("\x1b[<{};{};{}{}", btn, col + 1, row + 1, suffix).into_bytes()
        } else {
            // X10/normal encoding: clamped to 223 to fit in a byte
            let b = btn + 32;
            let c = (col + 1 + 32) as u8;
            let r = (row + 1 + 32) as u8;
            vec![0x1b, b'[', b'M', b, c, r]
        };
        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
            let _ = entry.pty.write_input(&data);
        }
    }

    // ── Mouse selection ───────────────────────────────────────────────────────

    fn pane_at_pixel(&self, px: f64, py: f64) -> Option<usize> {
        let rects = self.tab().layout.rects();
        for (id, [rx, ry, rw, rh]) in rects {
            if px >= rx as f64 && py >= ry as f64 && px < (rx + rw) as f64 && py < (ry + rh) as f64
            {
                return Some(id);
            }
        }
        None
    }

    fn pixel_to_cell(&self, pane_id: usize, px: f64, py: f64) -> Option<(usize, usize)> {
        let tab = self.tab();
        let entry = tab.panes.get(&pane_id)?;
        let [rx, ry, rw, rh] = entry.pane.rect;
        let m = &tab.metrics;
        if px < rx as f64 || py < ry as f64 || px >= (rx + rw) as f64 || py >= (ry + rh) as f64 {
            return None;
        }
        let col = ((px - rx as f64) / m.cell_width as f64) as usize;
        let row = ((py - ry as f64) / m.cell_height as f64) as usize;
        let cols = entry.pane.parser.grid.cols;
        let rows = entry.pane.parser.grid.rows;
        Some((
            col.min(cols.saturating_sub(1)),
            row.min(rows.saturating_sub(1)),
        ))
    }

    fn url_at_pixel(&self, px: f64, py: f64) -> Option<String> {
        let pane_id = self.pane_at_pixel(px, py)?;
        let (col, row) = self.pixel_to_cell(pane_id, px, py)?;
        let tab = self.tab();
        let entry = tab.panes.get(&pane_id)?;
        let grid = &entry.pane.parser.grid;
        let scroll_offset = entry.pane.scroll_offset;
        let cell = if scroll_offset == 0 {
            grid.cell(col, row)
        } else {
            let sb_len = grid.scrollback.len();
            let sb_start = sb_len.saturating_sub(scroll_offset);
            let sb_row = sb_start + row;
            if sb_row < sb_len {
                let line = &grid.scrollback[sb_row];
                if col < line.len() {
                    &line[col]
                } else {
                    return None;
                }
            } else {
                let live_row = sb_row.saturating_sub(sb_len);
                if live_row < grid.rows {
                    grid.cell(col, live_row)
                } else {
                    return None;
                }
            }
        };
        cell.url.as_ref().map(|u| u.as_ref().clone())
    }

    fn start_mouse_selection(&mut self, px: f64, py: f64) {
        if let Some(pane_id) = self.pane_at_pixel(px, py) {
            self.tab_mut().active = pane_id;
            if let Some((col, row)) = self.pixel_to_cell(pane_id, px, py) {
                self.mode = InputMode::Visual {
                    start_col: col,
                    start_row: row,
                    cur_col: col,
                    cur_row: row,
                };
                self.mouse_selecting = true;
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
        } = self.mode.clone()
        {
            let active = self.tab().active;
            if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                self.mode = InputMode::Visual {
                    start_col,
                    start_row,
                    cur_col: col,
                    cur_row: row,
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
        } = self.mode.clone()
        {
            self.mode = InputMode::Insert;
            if start_col == cur_col && start_row == cur_row {
                if let Some(url) = self.hovered_url.clone() {
                    open_url(&url);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
                return;
            }
            let active = self.tab().active;
            if let Some(entry) = self.tab().panes.get(&active) {
                let text = entry
                    .pane
                    .parser
                    .grid
                    .selected_text(start_col, start_row, cur_col, cur_row);
                if !text.is_empty() {
                    let cb = self
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

    // ── Config panel ──────────────────────────────────────────────────────────

    fn change_font_size(&mut self, delta: f32) {
        let current = self.tabs[self.active_tab].metrics.font_px;
        let new_size = (current + delta).clamp(6.0, 72.0);
        if (new_size - current).abs() < 0.1 {
            return;
        }
        // Only update active tab's metrics — config is not touched
        let new_metrics = self.renderer.make_metrics(new_size);
        let idx = self.active_tab;
        self.tabs[idx].metrics = new_metrics;
        Self::sync_pane_sizes_tab(&mut self.tabs[idx]);
        log::info!("Tab {} font size: {current} → {new_size}", idx + 1);
    }

    // ── Search ────────────────────────────────────────────────────────────────

    fn handle_search_key(&mut self, event: &winit::event::KeyEvent) {
        use winit::keyboard::{Key, NamedKey};
        let query = if let InputMode::Search { query } = &self.mode {
            query.clone()
        } else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.mode = InputMode::Normal;
                self.search_matches.clear();
            }
            Key::Named(NamedKey::Enter) if !self.search_matches.is_empty() => {
                let next = (self.search_current + 1) % self.search_matches.len();
                self.scroll_to_match(next);
            }
            Key::Named(NamedKey::Backspace) => {
                let mut q = query;
                q.pop();
                self.mode = InputMode::Search { query: q };
                self.update_search_matches();
            }
            // Ctrl+C or c — copy current match text to clipboard
            Key::Character(s) if s == "\x03" || s == "c" => {
                self.copy_current_match();
            }
            Key::Character(s) => {
                let mut q = query;
                q.push_str(s);
                self.mode = InputMode::Search { query: q };
                self.update_search_matches();
            }
            _ => {}
        }
    }

    fn copy_current_match(&mut self) {
        let Some(&(abs_row, col, len)) = self.search_matches.get(self.search_current) else {
            return;
        };
        let tab_idx = self.active_tab;
        let active = self.tabs[tab_idx].active;
        let Some(entry) = self.tabs[tab_idx].panes.get(&active) else {
            return;
        };
        let grid = &entry.pane.parser.grid;
        let sb_len = grid.scrollback.len();
        let text: String = if abs_row < sb_len {
            grid.scrollback[abs_row]
                .iter()
                .skip(col)
                .take(len)
                .map(|c| c.c)
                .collect()
        } else {
            let row = abs_row - sb_len;
            (col..col + len)
                .filter(|&c| c < grid.cols)
                .map(|c| grid.cell(c, row).c)
                .collect()
        };
        if !text.is_empty() {
            let cb = self
                .clipboard
                .get_or_insert_with(|| Clipboard::new().expect("clipboard unavailable"));
            match cb.set_text(text) {
                Ok(()) => log::info!("Copied search match to clipboard"),
                Err(e) => log::warn!("Clipboard write failed: {e}"),
            }
        }
    }

    fn update_search_matches(&mut self) {
        let query = match &self.mode {
            InputMode::Search { query } => query.clone(),
            _ => return,
        };

        self.search_matches.clear();
        self.search_current = 0;

        if query.is_empty() {
            return;
        }

        let re = match regex::Regex::new(&query) {
            Ok(r) => r,
            Err(_) => return,
        };

        let tab_idx = self.active_tab;
        let active = self.tabs[tab_idx].active;

        let matches: Vec<(usize, usize, usize)> = {
            if let Some(entry) = self.tabs[tab_idx].panes.get(&active) {
                let grid = &entry.pane.parser.grid;
                let sb_len = grid.scrollback.len();
                let mut m: Vec<(usize, usize, usize)> = Vec::new();

                for (abs_row, line) in grid.scrollback.iter().enumerate() {
                    let text: String = line.iter().map(|c| c.c).collect();
                    for mat in re.find_iter(&text) {
                        let col = text[..mat.start()].chars().count();
                        let len = mat.as_str().chars().count();
                        m.push((abs_row, col, len));
                    }
                }

                for row in 0..grid.rows {
                    let abs_row = sb_len + row;
                    let text: String = (0..grid.cols).map(|c| grid.cell(c, row).c).collect();
                    for mat in re.find_iter(&text) {
                        let col = text[..mat.start()].chars().count();
                        let len = mat.as_str().chars().count();
                        m.push((abs_row, col, len));
                    }
                }

                m
            } else {
                Vec::new()
            }
        };

        self.search_matches = matches;

        if !self.search_matches.is_empty() {
            self.scroll_to_match(0);
        }
    }

    fn scroll_to_match(&mut self, idx: usize) {
        if idx >= self.search_matches.len() {
            return;
        }
        self.search_current = idx;
        let (abs_row, _, _) = self.search_matches[idx];

        let tab_idx = self.active_tab;
        let active = self.tabs[tab_idx].active;

        let (sb_len, grid_rows) = self.tabs[tab_idx]
            .panes
            .get(&active)
            .map(|e| (e.pane.parser.grid.scrollback.len(), e.pane.parser.grid.rows))
            .unwrap_or((0, 24));

        let new_offset = if abs_row >= sb_len {
            0
        } else {
            let target_row = grid_rows / 2;
            (sb_len + target_row).saturating_sub(abs_row).min(sb_len)
        };

        if let Some(entry) = self.tabs[tab_idx].panes.get_mut(&active) {
            entry.pane.scroll_offset = new_offset;
        }
    }

    fn open_config_panel(&mut self) {
        self.config_panel = Some(ConfigPanel::from_config(&self.config));
    }

    fn apply_config(&mut self, new_cfg: Config, window: &Window) {
        new_cfg.save();
        window.set_title(&new_cfg.window.title);
        self.config = new_cfg;
        self.config_panel = None;
    }

    fn handle_rename_key(&mut self, event: &winit::event::KeyEvent) {
        use winit::keyboard::{Key, NamedKey};
        let buf = if let InputMode::RenameTab { buf } = &self.mode {
            buf.clone()
        } else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.mode = InputMode::Insert;
            }
            Key::Named(NamedKey::Enter) => {
                let name = buf.trim().to_string();
                self.tabs[self.active_tab].name = if name.is_empty() { None } else { Some(name) };
                self.mode = InputMode::Insert;
            }
            Key::Named(NamedKey::Backspace) => {
                let mut b = buf;
                b.pop();
                self.mode = InputMode::RenameTab { buf: b };
            }
            Key::Character(s) => {
                let mut b = buf;
                b.push_str(s);
                self.mode = InputMode::RenameTab { buf: b };
            }
            _ => {}
        }
    }

    fn handle_config_key(&mut self, event: &winit::event::KeyEvent) {
        use winit::keyboard::{Key, NamedKey};
        let ctrl = self.modifiers.state().control_key();
        let panel = match &mut self.config_panel {
            Some(p) => p,
            None => return,
        };

        let action = match &event.logical_key {
            Key::Named(NamedKey::Escape) => panel.handle_escape(),
            Key::Named(NamedKey::Enter) => panel.handle_char('\r'),
            Key::Named(NamedKey::Backspace) => panel.handle_backspace(),
            Key::Named(NamedKey::ArrowUp) => panel.handle_up(),
            Key::Named(NamedKey::ArrowDown) => panel.handle_down(),
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
                self.config_panel = None;
            }
            ConfigAction::None => {}
        }
    }

    // ── Render ────────────────────────────────────────────────────────────────

    fn redraw(&mut self) {
        if self.blink_last.elapsed()
            >= Duration::from_millis(self.config.window.cursor_blink_ms as u64)
        {
            self.blink_last = Instant::now();
            self.cursor_blink = !self.cursor_blink;
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

        if self.tabs.is_empty() {
            buf.present().unwrap();
            return;
        }

        self.tabs[self.active_tab].has_activity = false;

        let tab = &self.tabs[self.active_tab];
        let rects = tab.layout.rects();
        let separators = tab.layout.separators();

        let active_id = tab.active;
        let zoomed = tab.zoomed;
        let has_search = !self.search_matches.is_empty();
        let search_matches = &self.search_matches;
        let search_current_val = self.search_current;

        let views: Vec<PaneView> = if zoomed {
            let entry = tab.panes.get(&active_id);
            if let Some(entry) = entry {
                let show_cursor = matches!(self.mode, InputMode::Insert)
                    && self.cursor_blink
                    && entry.pane.scroll_offset == 0
                    && entry.pane.parser.grid.cursor_visible;
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
                    blink_visible: self.cursor_blink,
                    search_matches: sm,
                    search_current: sc,
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
                    let show_cursor = is_active
                        && matches!(self.mode, InputMode::Insert)
                        && self.cursor_blink
                        && entry.pane.scroll_offset == 0
                        && entry.pane.parser.grid.cursor_visible;
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
                        blink_visible: self.cursor_blink,
                        search_matches: sm,
                        search_current: sc,
                    })
                })
                .collect()
        };

        let tab_titles: Vec<(String, bool, bool)> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let is_active = i == self.active_tab;
                let osc_title = tab
                    .panes
                    .get(&tab.active)
                    .and_then(|e| e.pane.parser.grid.osc_title.as_deref())
                    .filter(|t| !t.starts_with('/') && !t.starts_with('~'));
                let label = if is_active {
                    if let InputMode::RenameTab { buf } = &self.mode {
                        format!(" {}| ", buf)
                    } else {
                        tab.name
                            .as_deref()
                            .or(osc_title)
                            .map(|n| format!(" {} ", n))
                            .unwrap_or_else(|| format!(" {} ", i + 1))
                    }
                } else {
                    tab.name
                        .as_deref()
                        .or(osc_title)
                        .map(|n| format!(" {} ", n))
                        .unwrap_or_else(|| format!(" {} ", i + 1))
                };
                (label, is_active, tab.has_activity)
            })
            .collect();

        let metrics = self.tabs[self.active_tab].metrics.clone();
        let draw_separators: &[[u32; 4]] = if zoomed { &[] } else { &separators };
        let home = std::env::var("HOME").unwrap_or_default();
        let cwd_owned: Option<String> = self.tabs[self.active_tab]
            .panes
            .get(&active_id)
            .and_then(|e| e.pane.parser.grid.cwd.as_deref())
            .map(|p| {
                if !home.is_empty() && p.starts_with(&home) {
                    format!("~{}", &p[home.len()..])
                } else {
                    p.to_string()
                }
            });
        let bell_flash = self.tabs[self.active_tab]
            .bell_flash_until
            .is_some_and(|t| t > Instant::now());
        let is_logging = self.tabs[self.active_tab]
            .panes
            .get(&active_id)
            .is_some_and(|e| e.log_file.is_some());
        self.renderer.draw(
            pixels,
            w,
            h,
            &views,
            draw_separators,
            &self.mode,
            &tab_titles,
            &metrics,
            self.search_matches.len(),
            self.search_current,
            cwd_owned.as_deref(),
            self.config.window.inactive_dim,
            bell_flash,
            is_logging,
        );

        if let Some(panel) = &self.config_panel {
            self.renderer.draw_config_panel(pixels, w, h, panel);
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
            .with_title(self.config.window.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.window.width,
                self.config.window.height,
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
                for tab in &mut self.tabs {
                    tab.layout.resize(size.width, size.height);
                }
                self.sync_all_pane_sizes();
            }

            WindowEvent::Focused(gained) => {
                if gained {
                    // The Tab from the Alt+Tab that brought focus back may
                    // arrive as a plain Tab (Alt already released by the WM).
                    // Mark it to be swallowed so it isn't sent to the PTY.
                    self.swallow_next_tab = true;
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
                if self.swallow_next_tab {
                    self.swallow_next_tab = false;
                    if event.logical_key == Key::Named(NamedKey::Tab) {
                        return;
                    }
                }

                // Reset blink on every keypress so cursor is always visible after input.
                self.cursor_blink = true;
                self.blink_last = Instant::now();

                if self.config_panel.is_some() {
                    self.handle_config_key(&event);
                    return;
                }

                if matches!(self.mode, InputMode::RenameTab { .. }) {
                    self.handle_rename_key(&event);
                    return;
                }

                if matches!(self.mode, InputMode::Search { .. }) {
                    self.handle_search_key(&event);
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }

                if self.ctrl_w_pending {
                    self.ctrl_w_pending = false;
                    match handle_ctrl_w(&event) {
                        Action::SplitH => self.do_split(SplitDir::H),
                        Action::SplitV => self.do_split(SplitDir::V),
                        Action::FocusLeft => self.focus_dir(-1, 0),
                        Action::FocusRight => self.focus_dir(1, 0),
                        Action::FocusUp => self.focus_dir(0, -1),
                        Action::FocusDown => self.focus_dir(0, 1),
                        Action::FocusNext => self.focus_next(),
                        Action::ClosePane => self.do_close_pane(event_loop),
                        Action::ZoomPane => {
                            self.tab_mut().zoomed = !self.tab().zoomed;
                        }
                        _ => {}
                    }
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
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

                match handle_key(
                    &event,
                    &self.modifiers,
                    &self.mode,
                    grid_cols,
                    grid_rows,
                    app_cursor,
                ) {
                    Action::CtrlWPrefix => {
                        self.ctrl_w_pending = true;
                    }

                    Action::SendToPty(bytes) => {
                        let active = self.tab().active;
                        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                            entry.pane.scroll_bottom();
                            let _ = entry.pty.write_input(&bytes);
                        }
                    }

                    Action::SetMode(new_mode) => {
                        let mode = if let InputMode::Visual { .. } = &new_mode {
                            if matches!(self.mode, InputMode::Visual { .. }) {
                                new_mode
                            } else {
                                let (col, row) = {
                                    let tab = self.tab();
                                    tab.panes
                                        .get(&tab.active)
                                        .map(|e| {
                                            (
                                                e.pane.parser.grid.cursor_col,
                                                e.pane.parser.grid.cursor_row,
                                            )
                                        })
                                        .unwrap_or((0, 0))
                                };
                                InputMode::Visual {
                                    start_col: col,
                                    start_row: row,
                                    cur_col: col,
                                    cur_row: row,
                                }
                            }
                        } else {
                            new_mode
                        };
                        self.mode = mode;
                    }

                    Action::ScrollUp(n) => {
                        let active = self.tab().active;
                        if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                            e.pane.scroll_up(n);
                        }
                    }
                    Action::ScrollDown(n) => {
                        let active = self.tab().active;
                        if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                            e.pane.scroll_down(n);
                        }
                    }
                    Action::ScrollToTop => {
                        let active = self.tab().active;
                        if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                            e.pane.scroll_top();
                        }
                    }
                    Action::ScrollToBottom => {
                        let active = self.tab().active;
                        if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                            e.pane.scroll_bottom();
                        }
                    }
                    Action::ClearScrollback => {
                        let active = self.tab().active;
                        if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                            e.pane.parser.grid.scrollback.clear();
                            e.pane.parser.grid.clear_screen();
                            e.pane.parser.grid.cursor_col = 0;
                            e.pane.parser.grid.cursor_row = 0;
                            e.pane.scroll_bottom();
                        }
                        self.search_matches.clear();
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }

                    Action::Paste => {
                        let text = self
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

                    Action::Copy => {
                        if let InputMode::Visual {
                            start_col,
                            start_row,
                            cur_col,
                            cur_row,
                        } = self.mode.clone()
                        {
                            let active = self.tab().active;
                            if let Some(entry) = self.tab().panes.get(&active) {
                                let text = entry
                                    .pane
                                    .parser
                                    .grid
                                    .selected_text(start_col, start_row, cur_col, cur_row);
                                if !text.is_empty() {
                                    let cb = self.clipboard.get_or_insert_with(|| {
                                        Clipboard::new().expect("clipboard unavailable")
                                    });
                                    match cb.set_text(text) {
                                        Ok(()) => log::info!("Copied selection to clipboard"),
                                        Err(e) => log::warn!("Clipboard write failed: {e}"),
                                    }
                                }
                            }
                            self.mode = InputMode::Insert;
                        }
                    }

                    Action::SplitH => self.do_split(SplitDir::H),
                    Action::SplitV => self.do_split(SplitDir::V),
                    Action::FocusLeft => self.focus_dir(-1, 0),
                    Action::FocusRight => self.focus_dir(1, 0),
                    Action::FocusUp => self.focus_dir(0, -1),
                    Action::FocusDown => self.focus_dir(0, 1),
                    Action::FocusNext => self.focus_next(),
                    Action::ClosePane => self.do_close_pane(event_loop),

                    Action::NewTab => {
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
                    Action::NextTab => {
                        self.next_tab();
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    Action::PrevTab => {
                        self.prev_tab();
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    Action::MoveTabLeft => {
                        self.move_tab_left();
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    Action::MoveTabRight => {
                        self.move_tab_right();
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    Action::CloseTab => self.close_tab(event_loop),
                    Action::RenameTab => {
                        let current = self.tabs[self.active_tab].name.clone().unwrap_or_default();
                        self.mode = InputMode::RenameTab { buf: current };
                    }

                    Action::SearchOpen => {
                        self.search_matches.clear();
                        self.search_current = 0;
                        self.mode = InputMode::Search {
                            query: String::new(),
                        };
                    }
                    Action::SearchNext => {
                        if !self.search_matches.is_empty() {
                            let next = (self.search_current + 1) % self.search_matches.len();
                            self.scroll_to_match(next);
                        }
                    }
                    Action::SearchPrev => {
                        if !self.search_matches.is_empty() {
                            let prev = if self.search_current == 0 {
                                self.search_matches.len() - 1
                            } else {
                                self.search_current - 1
                            };
                            self.scroll_to_match(prev);
                        }
                    }

                    Action::IncreaseFontSize => self.change_font_size(1.0),
                    Action::DecreaseFontSize => self.change_font_size(-1.0),
                    Action::ResetFontSize => {
                        let default = self.config.font.size;
                        let current = self.tabs[self.active_tab].metrics.font_px;
                        self.change_font_size(default - current);
                    }

                    Action::ToggleLog => {
                        let active = self.tab().active;
                        let log_dir = self.config.logging.log_dir.clone();
                        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                            if entry.log_file.is_some() {
                                entry.log_file = None;
                                log::info!("Logging stopped for pane {active}");
                            } else {
                                entry.log_file = open_log_file(active, &log_dir);
                            }
                        }
                    }

                    Action::OpenConfig => self.open_config_panel(),
                    Action::Quit => event_loop.exit(),
                    Action::ZoomPane => {
                        self.tab_mut().zoomed = !self.tab().zoomed;
                    }
                    Action::None => {}
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = Some((position.x, position.y));
                let url = self.url_at_pixel(position.x, position.y);
                let icon = if url.is_some() {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Text
                };
                if let Some(w) = &self.window {
                    w.set_cursor(icon);
                }
                self.hovered_url = url;
                let (mouse_mode, mouse_sgr) = self.active_mouse_mode();
                if mouse_mode >= 1002 {
                    // Button-motion or any-motion: report if button is held (selecting) or always
                    let report = mouse_mode >= 1003 || self.mouse_selecting;
                    if report {
                        let px = position.x;
                        let py = position.y;
                        let active = self.tab().active;
                        if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                            // btn 32 = motion with no button, 32 = left held already encoded as 32
                            let btn = if self.mouse_selecting { 32 } else { 35 };
                            self.send_mouse_event(btn, col, row, false, mouse_sgr);
                        }
                    }
                } else if self.mouse_selecting {
                    self.update_mouse_selection(position.x, position.y);
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let btn_code = match button {
                    MouseButton::Left => 0u8,
                    MouseButton::Middle => 1u8,
                    MouseButton::Right => 2u8,
                    _ => 3u8,
                };
                let (mouse_mode, mouse_sgr) = self.active_mouse_mode();
                if mouse_mode >= 1000 && btn_code < 3 {
                    // Forward event to PTY
                    if let Some((px, py)) = self.mouse_pos {
                        let active = self.tab().active;
                        if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                            let release = state == ElementState::Released;
                            self.send_mouse_event(btn_code, col, row, release, mouse_sgr);
                        }
                    }
                    // Track left-button held state so motion reporting knows
                    if button == MouseButton::Left {
                        self.mouse_selecting = state == ElementState::Pressed;
                    }
                    return;
                }

                if button == MouseButton::Left {
                    match state {
                        ElementState::Pressed => {
                            if let Some((mx, my)) = self.mouse_pos {
                                self.start_mouse_selection(mx, my);
                            }
                        }
                        ElementState::Released => {
                            if self.mouse_selecting {
                                self.mouse_selecting = false;
                                self.finish_mouse_selection();
                            }
                        }
                    }
                } else if button == MouseButton::Middle {
                    // Middle-click paste
                    if state == ElementState::Pressed {
                        let text = self
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
                    if let Some((px, py)) = self.mouse_pos {
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
                if has_more {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
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
        let blink_dur = Duration::from_millis(self.config.window.cursor_blink_ms as u64);
        let elapsed = self.blink_last.elapsed();
        if elapsed >= blink_dur {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + blink_dur));
        } else {
            let mut next = Instant::now() + (blink_dur - elapsed);
            // Wake up early if a bell flash is still active so we clear it on expiry.
            for tab in &self.tabs {
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
    let dir = if log_dir.is_empty() {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{home}/.mmterm")
    } else {
        log_dir.to_string()
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Failed to create log directory {dir}: {e}");
        return None;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = format!("{dir}/mmterm-{ts}-pane{pane_id}.log");
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

fn main() {
    env_logger::init();
    Config::write_default_if_missing();
    let config = Config::load();
    let event_loop = EventLoop::new().unwrap();
    let proxy = event_loop.create_proxy();
    let mut app = App::new(config, proxy);
    event_loop.run_app(&mut app).unwrap();
}
