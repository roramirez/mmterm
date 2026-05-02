mod input;
mod pty;
mod renderer;
mod terminal;
mod ui;

use arboard::Clipboard;
use crossbeam_channel::{unbounded, Receiver};
use input::{handle_ctrl_w, handle_key, InputMode};
use renderer::{PaneView, Renderer};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use ui::{Layout, Pane, SplitDir};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Modifiers, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::input::keybindings::Action;

const FONT_PX: f32 = 16.0;

struct PaneEntry {
    pane: Pane,
    pty: pty::PtySession,
    rx: Receiver<Vec<u8>>,
}

struct App {
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    renderer: Renderer,
    panes: HashMap<usize, PaneEntry>,
    next_id: usize,
    active: usize,
    layout: Layout,
    mode: InputMode,
    modifiers: Modifiers,
    cursor_blink: bool,
    blink_ticks: u32,
    ctrl_w_pending: bool,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            surface: None,
            renderer: Renderer::new(FONT_PX),
            panes: HashMap::new(),
            next_id: 0,
            active: 0,
            layout: Layout::new(0, 800, 600),
            mode: InputMode::Insert,
            modifiers: Modifiers::default(),
            cursor_blink: true,
            blink_ticks: 0,
            ctrl_w_pending: false,
        }
    }

    fn spawn_pane(&mut self, rect: [u32; 4]) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let [_, _, w, h] = rect;
        let (cols, rows) = self.renderer.grid_size_for(w, h);
        let pane = Pane::new(cols, rows, rect);
        let (tx, rx) = unbounded::<Vec<u8>>();
        match pty::PtySession::spawn(cols as u16, rows as u16, tx) {
            Ok(pty) => { self.panes.insert(id, PaneEntry { pane, pty, rx }); }
            Err(e) => log::error!("PTY spawn failed: {e}"),
        }
        id
    }

    fn drain_all(&mut self) {
        let ids: Vec<usize> = self.panes.keys().copied().collect();
        for id in ids {
            let entry = self.panes.get_mut(&id).unwrap();
            while let Ok(bytes) = entry.rx.try_recv() {
                entry.pane.process(&bytes);
            }
        }
    }

    fn sync_pane_sizes(&mut self) {
        let rects = self.layout.rects();
        for (id, rect) in rects {
            if let Some(entry) = self.panes.get_mut(&id) {
                let [_, _, w, h] = rect;
                let (cols, rows) = self.renderer.grid_size_for(w, h);
                if entry.pane.parser.grid.cols != cols || entry.pane.parser.grid.rows != rows {
                    entry.pane.resize(cols, rows, rect);
                    let _ = entry.pty.resize(cols as u16, rows as u16);
                }
            }
        }
    }

    fn do_split(&mut self, dir: SplitDir) {
        let active_rect = self.layout.rects()
            .into_iter()
            .find(|(id, _)| *id == self.active)
            .map(|(_, r)| r)
            .unwrap_or([0, 0, 100, 100]);

        // Estimate new pane rect (half the active rect)
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
        self.layout.split(self.active, new_id, dir);
        self.active = new_id;
        self.sync_pane_sizes();
    }

    fn do_close_pane(&mut self, event_loop: &ActiveEventLoop) {
        if self.panes.len() == 1 {
            event_loop.exit();
            return;
        }
        let new_focus = self.layout.remove(self.active);
        self.panes.remove(&self.active);
        if let Some(id) = new_focus {
            self.active = id;
        } else {
            self.active = *self.panes.keys().next().unwrap();
        }
        self.sync_pane_sizes();
    }

    fn focus_dir(&mut self, dx: i32, dy: i32) {
        if let Some(id) = self.layout.focus_dir(self.active, dx, dy) {
            self.active = id;
        }
    }

    fn focus_next(&mut self) {
        let leaves = self.layout.leaves();
        if let Some(pos) = leaves.iter().position(|&id| id == self.active) {
            self.active = leaves[(pos + 1) % leaves.len()];
        }
    }

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
        let w = size.width;
        let h = size.height;
        if w == 0 || h == 0 { return; }

        if let (Ok(wn), Ok(hn)) = (NonZeroU32::try_from(w), NonZeroU32::try_from(h)) {
            let _ = surface.resize(wn, hn);
        }

        let mut buf = surface.buffer_mut().unwrap();
        let pixels: &mut [u32] = &mut buf;

        let rects = self.layout.rects();
        let separators = self.layout.separators();

        let views: Vec<PaneView> = rects.iter().filter_map(|(id, rect)| {
            let entry = self.panes.get(id)?;
            let is_active = *id == self.active;
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

        self.renderer.draw(pixels, w, h, &views, &separators, &self.mode);
        buf.present().unwrap();
        window.request_redraw();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("mmterm")
            .with_inner_size(winit::dpi::LogicalSize::new(800u32, 600u32));

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let ctx = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&ctx, window.clone()).unwrap();

        let size = window.inner_size();
        self.layout = Layout::new(0, size.width, size.height);

        // Compute the initial pane rect (full usable area)
        let initial_rects = self.layout.rects();
        let initial_rect = initial_rects.first().map(|(_, r)| *r).unwrap_or([0, 0, size.width, size.height]);

        let id = self.spawn_pane(initial_rect);
        // Re-create layout with the real ID
        self.layout = Layout::new(id, size.width, size.height);
        self.active = id;

        self.surface = Some(surface);
        self.window = Some(window.clone());
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                self.layout.resize(size.width, size.height);
                self.sync_pane_sizes();
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods;
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }

                // Ctrl+W prefix mode
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
                        _ => {}
                    }
                    return;
                }

                let (grid_cols, grid_rows) = self.panes.get(&self.active)
                    .map(|e| (e.pane.parser.grid.cols, e.pane.parser.grid.rows))
                    .unwrap_or((80, 24));

                match handle_key(&event, &self.modifiers, &self.mode, grid_cols, grid_rows) {
                    Action::CtrlWPrefix => { self.ctrl_w_pending = true; }

                    Action::SendToPty(bytes) => {
                        if let Some(entry) = self.panes.get_mut(&self.active) {
                            let _ = entry.pty.write_input(&bytes);
                        }
                    }

                    Action::SetMode(new_mode) => {
                        let mode = if let InputMode::Visual { .. } = &new_mode {
                            if matches!(self.mode, InputMode::Visual { .. }) {
                                new_mode
                            } else {
                                let (col, row) = self.panes.get(&self.active)
                                    .map(|e| (e.pane.parser.grid.cursor_col, e.pane.parser.grid.cursor_row))
                                    .unwrap_or((0, 0));
                                InputMode::Visual { start_col: col, start_row: row, cur_col: col, cur_row: row }
                            }
                        } else {
                            new_mode
                        };
                        self.mode = mode;
                    }

                    Action::ScrollUp(n) => {
                        if let Some(e) = self.panes.get_mut(&self.active) { e.pane.scroll_up(n); }
                    }
                    Action::ScrollDown(n) => {
                        if let Some(e) = self.panes.get_mut(&self.active) { e.pane.scroll_down(n); }
                    }
                    Action::ScrollToTop => {
                        if let Some(e) = self.panes.get_mut(&self.active) { e.pane.scroll_top(); }
                    }
                    Action::ScrollToBottom => {
                        if let Some(e) = self.panes.get_mut(&self.active) { e.pane.scroll_bottom(); }
                    }

                    Action::Paste => {
                        if let Some(entry) = self.panes.get_mut(&self.active) {
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

                    // Split/focus/close via keybindings (duplicates for completeness)
                    Action::SplitH => self.do_split(SplitDir::H),
                    Action::SplitV => self.do_split(SplitDir::V),
                    Action::FocusLeft => self.focus_dir(-1, 0),
                    Action::FocusRight => self.focus_dir(1, 0),
                    Action::FocusUp => self.focus_dir(0, -1),
                    Action::FocusDown => self.focus_dir(0, 1),
                    Action::FocusNext => self.focus_next(),
                    Action::ClosePane => self.do_close_pane(event_loop),

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
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
