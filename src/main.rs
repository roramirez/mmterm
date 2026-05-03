mod config;
mod input;
mod pty;
mod renderer;
mod terminal;
mod tui_config;
mod ui;

use arboard::Clipboard;
use config::Config;
use crossbeam_channel::{unbounded, Receiver};
use input::{handle_ctrl_w, handle_key, InputMode};
use renderer::{FontMetrics, PaneView, Renderer};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tui_config::{ConfigAction, ConfigPanel};
use ui::{Layout, Pane, SplitDir};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, Modifiers, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::input::keybindings::Action;
use crate::ui::layout::TAB_BAR_H;

// ── Per-pane state ───────────────────────────────────────────────────────────

struct PaneEntry {
    pane: Pane,
    pty: pty::PtySession,
    rx: Receiver<Vec<u8>>,
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
}

impl App {
    fn new(config: Config) -> Self {
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
            cols, rows, rect,
            c.fg(), c.bg(), c.cursor(), c.selection(),
            c.palette_colors(),
        );
        let (tx, rx) = unbounded::<Vec<u8>>();
        let shell = self.config.shell.program.clone()
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/bash".to_string());
        match pty::PtySession::spawn_with_shell(cols as u16, rows as u16, tx, &shell, cwd.as_ref()) {
            Ok(pty) => { self.tabs[tab_idx].panes.insert(id, PaneEntry { pane, pty, rx }); }
            Err(e) => log::error!("PTY spawn failed: {e}"),
        }
        id
    }

    fn spawn_pane(&mut self, rect: [u32; 4]) -> usize {
        let cwd = self.tabs.get(self.active_tab)
            .and_then(|t| t.panes.get(&t.active))
            .and_then(|e| e.pty.cwd());
        self.spawn_pane_into(self.active_tab, rect, cwd)
    }

    // ── Tab management ───────────────────────────────────────────────────────

    fn new_tab(&mut self, win_w: u32, win_h: u32) {
        let cwd = self.tabs.get(self.active_tab)
            .and_then(|t| t.panes.get(&t.active))
            .and_then(|e| e.pty.cwd());
        let metrics = self.renderer.make_metrics(self.renderer.font_px);
        let layout = Layout::new(0, win_w, win_h);
        let initial_rect = layout.rects().first().map(|(_, r)| *r)
            .unwrap_or([0, TAB_BAR_H, win_w, win_h]);
        let tab_idx = self.tabs.len();
        self.tabs.push(TabState {
            panes: HashMap::new(),
            layout: Layout::new(0, win_w, win_h),
            active: 0,
            metrics,
            name: None,
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
            self.active_tab = self.active_tab.checked_sub(1)
                .unwrap_or(self.tabs.len() - 1);
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

    /// Drain all pending PTY output and return (tab_idx, pane_id) pairs for
    /// any panes whose PTY process has exited (sender disconnected).
    fn drain_all(&mut self) -> Vec<(usize, usize)> {
        let mut exited = Vec::new();
        for (tab_idx, tab) in self.tabs.iter_mut().enumerate() {
            let ids: Vec<usize> = tab.panes.keys().copied().collect();
            for id in ids {
                let entry = tab.panes.get_mut(&id).unwrap();
                loop {
                    match entry.rx.try_recv() {
                        Ok(bytes) => entry.pane.process(&bytes),
                        Err(crossbeam_channel::TryRecvError::Empty) => break,
                        Err(crossbeam_channel::TryRecvError::Disconnected) => {
                            exited.push((tab_idx, id));
                            break;
                        }
                    }
                }
            }
        }
        exited
    }

    fn close_pane_on_tab(&mut self, tab_idx: usize, pane_id: usize, event_loop: &ActiveEventLoop) {
        if tab_idx >= self.tabs.len() { return; }
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
        let active = self.tab().active;
        let active_rect = self.tab().layout.rects()
            .into_iter().find(|(id, _)| *id == active)
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

    // ── Mouse selection ───────────────────────────────────────────────────────

    fn pane_at_pixel(&self, px: f64, py: f64) -> Option<usize> {
        let rects = self.tab().layout.rects();
        for (id, [rx, ry, rw, rh]) in rects {
            if px >= rx as f64 && py >= ry as f64
                && px < (rx + rw) as f64 && py < (ry + rh) as f64
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
        if px < rx as f64 || py < ry as f64
            || px >= (rx + rw) as f64 || py >= (ry + rh) as f64
        {
            return None;
        }
        let col = ((px - rx as f64) / m.cell_width as f64) as usize;
        let row = ((py - ry as f64) / m.cell_height as f64) as usize;
        let cols = entry.pane.parser.grid.cols;
        let rows = entry.pane.parser.grid.rows;
        Some((col.min(cols.saturating_sub(1)), row.min(rows.saturating_sub(1))))
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
            }
        }
    }

    fn update_mouse_selection(&mut self, px: f64, py: f64) {
        if let InputMode::Visual { start_col, start_row, .. } = self.mode.clone() {
            let active = self.tab().active;
            if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                self.mode = InputMode::Visual { start_col, start_row, cur_col: col, cur_row: row };
            }
        }
    }

    fn finish_mouse_selection(&mut self) {
        if let InputMode::Visual { start_col, start_row, cur_col, cur_row } = self.mode.clone() {
            if start_col == cur_col && start_row == cur_row {
                self.mode = InputMode::Insert;
                return;
            }
            let active = self.tab().active;
            if let Some(entry) = self.tab().panes.get(&active) {
                let text = entry.pane.parser.grid.selected_text(
                    start_col, start_row, cur_col, cur_row,
                );
                if !text.is_empty() {
                    let cb = self.clipboard.get_or_insert_with(|| {
                        Clipboard::new().expect("clipboard unavailable")
                    });
                    match cb.set_text(text) {
                        Ok(()) => log::info!("Copied mouse selection to clipboard"),
                        Err(e) => log::warn!("Clipboard write failed: {e}"),
                    }
                }
            }
        }
    }

    // ── Config panel ──────────────────────────────────────────────────────────

    fn change_font_size(&mut self, delta: f32) {
        let current = self.tabs[self.active_tab].metrics.font_px;
        let new_size = (current + delta).clamp(6.0, 72.0);
        if (new_size - current).abs() < 0.1 { return; }
        // Only update active tab's metrics — config is not touched
        let new_metrics = self.renderer.make_metrics(new_size);
        let idx = self.active_tab;
        self.tabs[idx].metrics = new_metrics;
        Self::sync_pane_sizes_tab(&mut self.tabs[idx]);
        log::info!("Tab {} font size: {current} → {new_size}", idx + 1);
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
            Key::Named(NamedKey::Escape)    => panel.handle_escape(),
            Key::Named(NamedKey::Enter)     => panel.handle_char('\r'),
            Key::Named(NamedKey::Backspace) => panel.handle_backspace(),
            Key::Named(NamedKey::ArrowUp)   => panel.handle_up(),
            Key::Named(NamedKey::ArrowDown) => panel.handle_down(),
            Key::Named(NamedKey::Space)     => panel.handle_char(' '),
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
                if let Some(w) = window { self.apply_config(cfg, &w); }
            }
            ConfigAction::Cancel => { self.config_panel = None; }
            ConfigAction::None => {}
        }
    }

    // ── Render ────────────────────────────────────────────────────────────────

    fn redraw(&mut self) {
        if self.blink_last.elapsed() >= Duration::from_millis(self.config.window.cursor_blink_ms as u64) {
            self.blink_last = Instant::now();
            self.cursor_blink = !self.cursor_blink;
        }

        let Some(surface) = &mut self.surface else { return };
        let Some(window) = &self.window else { return };
        let size = window.inner_size();
        let (w, h) = (size.width, size.height);
        if w == 0 || h == 0 { return; }

        if let (Ok(wn), Ok(hn)) = (NonZeroU32::try_from(w), NonZeroU32::try_from(h)) {
            let _ = surface.resize(wn, hn);
        }

        let mut buf = surface.buffer_mut().unwrap();
        let pixels: &mut [u32] = &mut buf;

        if self.tabs.is_empty() { buf.present().unwrap(); return; }

        let tab = &self.tabs[self.active_tab];
        let rects = tab.layout.rects();
        let separators = tab.layout.separators();

        let views: Vec<PaneView> = rects.iter().filter_map(|(id, rect)| {
            let entry = tab.panes.get(id)?;
            let is_active = *id == tab.active;
            let show_cursor = is_active
                && matches!(self.mode, InputMode::Insert)
                && self.cursor_blink
                && entry.pane.scroll_offset == 0;
            Some(PaneView {
                grid: &entry.pane.parser.grid,
                rect: *rect,
                scroll_offset: entry.pane.scroll_offset,
                is_active,
                show_cursor,
            })
        }).collect();

        let tab_titles: Vec<(String, bool)> = self.tabs.iter().enumerate()
            .map(|(i, tab)| {
                let is_active = i == self.active_tab;
                let label = if is_active {
                    if let InputMode::RenameTab { buf } = &self.mode {
                        format!(" {}| ", buf)
                    } else {
                        tab.name.as_deref()
                            .map(|n| format!(" {} ", n))
                            .unwrap_or_else(|| format!(" {} ", i + 1))
                    }
                } else {
                    tab.name.as_deref()
                        .map(|n| format!(" {} ", n))
                        .unwrap_or_else(|| format!(" {} ", i + 1))
                };
                (label, is_active)
            })
            .collect();

        let metrics = self.tabs[self.active_tab].metrics.clone();
        self.renderer.draw(pixels, w, h, &views, &separators, &self.mode, &tab_titles, &metrics);

        if let Some(panel) = &self.config_panel {
            self.renderer.draw_config_panel(pixels, w, h, panel);
        }

        buf.present().unwrap();
        window.request_redraw();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title(self.config.window.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.window.width,
                self.config.window.height,
            ));

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
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

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods;
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed { return; }

                if self.config_panel.is_some() {
                    self.handle_config_key(&event);
                    return;
                }

                if matches!(self.mode, InputMode::RenameTab { .. }) {
                    self.handle_rename_key(&event);
                    return;
                }

                if self.ctrl_w_pending {
                    self.ctrl_w_pending = false;
                    match handle_ctrl_w(&event) {
                        Action::SplitH    => self.do_split(SplitDir::H),
                        Action::SplitV    => self.do_split(SplitDir::V),
                        Action::FocusLeft => self.focus_dir(-1, 0),
                        Action::FocusRight=> self.focus_dir(1, 0),
                        Action::FocusUp   => self.focus_dir(0, -1),
                        Action::FocusDown => self.focus_dir(0, 1),
                        Action::FocusNext => self.focus_next(),
                        Action::ClosePane => self.do_close_pane(event_loop),
                        _ => {}
                    }
                    return;
                }

                let (grid_cols, grid_rows, app_cursor) = {
                    let tab = self.tab();
                    tab.panes.get(&tab.active)
                        .map(|e| (e.pane.parser.grid.cols, e.pane.parser.grid.rows, e.pane.parser.grid.application_cursor_keys))
                        .unwrap_or((80, 24, false))
                };

                match handle_key(&event, &self.modifiers, &self.mode, grid_cols, grid_rows, app_cursor) {
                    Action::CtrlWPrefix => { self.ctrl_w_pending = true; }

                    Action::SendToPty(bytes) => {
                        let active = self.tab().active;
                        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
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
                                    tab.panes.get(&tab.active)
                                        .map(|e| (e.pane.parser.grid.cursor_col, e.pane.parser.grid.cursor_row))
                                        .unwrap_or((0, 0))
                                };
                                InputMode::Visual { start_col: col, start_row: row, cur_col: col, cur_row: row }
                            }
                        } else {
                            new_mode
                        };
                        self.mode = mode;
                    }

                    Action::ScrollUp(n) => {
                        let active = self.tab().active;
                        if let Some(e) = self.tab_mut().panes.get_mut(&active) { e.pane.scroll_up(n); }
                    }
                    Action::ScrollDown(n) => {
                        let active = self.tab().active;
                        if let Some(e) = self.tab_mut().panes.get_mut(&active) { e.pane.scroll_down(n); }
                    }
                    Action::ScrollToTop => {
                        let active = self.tab().active;
                        if let Some(e) = self.tab_mut().panes.get_mut(&active) { e.pane.scroll_top(); }
                    }
                    Action::ScrollToBottom => {
                        let active = self.tab().active;
                        if let Some(e) = self.tab_mut().panes.get_mut(&active) { e.pane.scroll_bottom(); }
                    }

                    Action::Paste => {
                        let text = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok())
                            .or_else(|| Clipboard::new().ok()?.get_text().ok());
                        if let Some(text) = text {
                            let active = self.tab().active;
                            if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                                let mut data = b"\x1b[200~".to_vec();
                                data.extend_from_slice(text.as_bytes());
                                data.extend_from_slice(b"\x1b[201~");
                                let _ = entry.pty.write_input(&data);
                            }
                        } else {
                            log::warn!("Clipboard read failed");
                        }
                    }

                    Action::Copy => {
                        if let InputMode::Visual { start_col, start_row, cur_col, cur_row } = self.mode.clone() {
                            let active = self.tab().active;
                            if let Some(entry) = self.tab().panes.get(&active) {
                                let text = entry.pane.parser.grid.selected_text(
                                    start_col, start_row, cur_col, cur_row,
                                );
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

                    Action::SplitH    => self.do_split(SplitDir::H),
                    Action::SplitV    => self.do_split(SplitDir::V),
                    Action::FocusLeft => self.focus_dir(-1, 0),
                    Action::FocusRight=> self.focus_dir(1, 0),
                    Action::FocusUp   => self.focus_dir(0, -1),
                    Action::FocusDown => self.focus_dir(0, 1),
                    Action::FocusNext => self.focus_next(),
                    Action::ClosePane => self.do_close_pane(event_loop),

                    Action::NewTab  => {
                        let (w, h) = self.window.as_ref()
                            .map(|w| { let s = w.inner_size(); (s.width, s.height) })
                            .unwrap_or((800, 600));
                        self.new_tab(w, h);
                    }
                    Action::NextTab   => self.next_tab(),
                    Action::PrevTab   => self.prev_tab(),
                    Action::CloseTab  => self.close_tab(event_loop),
                    Action::RenameTab => {
                        let current = self.tabs[self.active_tab].name.clone()
                            .unwrap_or_default();
                        self.mode = InputMode::RenameTab { buf: current };
                    }

                    Action::IncreaseFontSize => self.change_font_size(1.0),
                    Action::DecreaseFontSize => self.change_font_size(-1.0),
                    Action::ResetFontSize => {
                        let default = self.config.font.size;
                        let current = self.tabs[self.active_tab].metrics.font_px;
                        self.change_font_size(default - current);
                    }

                    Action::OpenConfig => self.open_config_panel(),
                    Action::Quit => event_loop.exit(),
                    Action::None => {}
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = Some((position.x, position.y));
                if self.mouse_selecting {
                    self.update_mouse_selection(position.x, position.y);
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
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
                        let text = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok())
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

            WindowEvent::RedrawRequested => {
                let exited = self.drain_all();
                for (tab_idx, pane_id) in exited {
                    self.close_pane_on_tab(tab_idx, pane_id, event_loop);
                }
                self.redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();
    Config::write_default_if_missing();
    let config = Config::load();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(config);
    event_loop.run_app(&mut app).unwrap();
}
