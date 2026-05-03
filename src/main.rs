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
use tui_config::{ConfigAction, ConfigPanel};
use ui::{Layout, Pane, SplitDir};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Modifiers, WindowEvent};
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
    blink_ticks: u32,
    ctrl_w_pending: bool,
    config: Config,
    config_panel: Option<ConfigPanel>,
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
            blink_ticks: 0,
            ctrl_w_pending: false,
            config_panel: None,
            config,
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
        match pty::PtySession::spawn_with_shell(cols as u16, rows as u16, tx, &shell) {
            Ok(pty) => { self.tabs[tab_idx].panes.insert(id, PaneEntry { pane, pty, rx }); }
            Err(e) => log::error!("PTY spawn failed: {e}"),
        }
        id
    }

    fn spawn_pane(&mut self, rect: [u32; 4]) -> usize {
        self.spawn_pane_into(self.active_tab, rect)
    }

    // ── Tab management ───────────────────────────────────────────────────────

    fn new_tab(&mut self, win_w: u32, win_h: u32) {
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
        });
        let id = self.spawn_pane_into(tab_idx, initial_rect);
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

    fn drain_all(&mut self) {
        for tab in &mut self.tabs {
            let ids: Vec<usize> = tab.panes.keys().copied().collect();
            for id in ids {
                let entry = tab.panes.get_mut(&id).unwrap();
                while let Ok(bytes) = entry.rx.try_recv() {
                    entry.pane.process(&bytes);
                }
            }
        }
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
        self.drain_all();

        self.blink_ticks += 1;
        if self.blink_ticks >= 30 {
            self.blink_ticks = 0;
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

        // Tab titles: just number + active indicator
        let tab_titles: Vec<(String, bool)> = self.tabs.iter().enumerate()
            .map(|(i, _)| (format!(" {} ", i + 1), i == self.active_tab))
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

                let (grid_cols, grid_rows) = {
                    let tab = self.tab();
                    tab.panes.get(&tab.active)
                        .map(|e| (e.pane.parser.grid.cols, e.pane.parser.grid.rows))
                        .unwrap_or((80, 24))
                };

                match handle_key(&event, &self.modifiers, &self.mode, grid_cols, grid_rows) {
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
                        let active = self.tab().active;
                        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                            match Clipboard::new().and_then(|mut cb| cb.get_text()) {
                                Ok(text) => {
                                    let mut data = b"\x1b[200~".to_vec();
                                    data.extend_from_slice(text.as_bytes());
                                    data.extend_from_slice(b"\x1b[201~");
                                    let _ = entry.pty.write_input(&data);
                                }
                                Err(e) => log::warn!("Clipboard read failed: {e}"),
                            }
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
                    Action::NextTab  => self.next_tab(),
                    Action::PrevTab  => self.prev_tab(),
                    Action::CloseTab => self.close_tab(event_loop),

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

            WindowEvent::RedrawRequested => self.redraw(),
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
