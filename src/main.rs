mod input;
mod pty;
mod renderer;
mod terminal;
mod ui;

use arboard::Clipboard;
use crossbeam_channel::unbounded;
use input::{handle_key, InputMode};
use renderer::Renderer;
use std::num::NonZeroU32;
use std::sync::Arc;
use ui::{Layout, Pane};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Modifiers, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::input::keybindings::Action;

const FONT_PX: f32 = 16.0;

struct App {
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    renderer: Renderer,
    pane: Option<Pane>,
    layout: Layout,
    pty: Option<pty::PtySession>,
    pty_rx: Option<crossbeam_channel::Receiver<Vec<u8>>>,
    mode: InputMode,
    modifiers: Modifiers,
    cursor_blink: bool,
    blink_ticks: u32,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            surface: None,
            renderer: Renderer::new(FONT_PX),
            pane: None,
            layout: Layout::new(800, 600),
            pty: None,
            pty_rx: None,
            mode: InputMode::Insert,
            modifiers: Modifiers::default(),
            cursor_blink: true,
            blink_ticks: 0,
        }
    }

    fn drain_pty(&mut self) {
        let Some(rx) = &self.pty_rx else { return };
        let Some(pane) = &mut self.pane else { return };
        while let Ok(bytes) = rx.try_recv() {
            pane.process(&bytes);
        }
    }

    fn redraw(&mut self) {
        self.drain_pty();

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

        if w == 0 || h == 0 {
            return;
        }

        if let (Ok(w_nz), Ok(h_nz)) = (NonZeroU32::try_from(w), NonZeroU32::try_from(h)) {
            let _ = surface.resize(w_nz, h_nz);
        }

        let mut buf = surface.buffer_mut().unwrap();
        let pixels: &mut [u32] = &mut buf;

        if let Some(pane) = &self.pane {
            let show_cursor = matches!(self.mode, InputMode::Insert)
                && self.cursor_blink
                && pane.scroll_offset == 0;
            self.renderer.draw(
                pixels, w, h,
                &pane.parser.grid,
                pane.scroll_offset,
                &self.mode,
                show_cursor,
            );
        } else {
            pixels.fill(0xFF1E1E2E);
        }

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
        self.layout = Layout::new(size.width, size.height);

        let (cols, rows) = self.renderer.grid_size(size.width, size.height);
        let rect = self.layout.full_rect();
        self.pane = Some(Pane::new(cols, rows, rect));

        let (tx, rx) = unbounded::<Vec<u8>>();
        match pty::PtySession::spawn(cols as u16, rows as u16, tx) {
            Ok(session) => {
                self.pty = Some(session);
                self.pty_rx = Some(rx);
            }
            Err(e) => log::error!("Failed to spawn PTY: {e}"),
        }

        self.surface = Some(surface);
        self.window = Some(window.clone());
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                self.layout.resize(size.width, size.height);
                let (cols, rows) = self.renderer.grid_size(size.width, size.height);
                let rect = self.layout.full_rect();
                if let Some(pane) = &mut self.pane {
                    pane.resize(cols, rows, rect);
                }
                if let Some(pty) = &self.pty {
                    let _ = pty.resize(cols as u16, rows as u16);
                }
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods;
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                let (grid_cols, grid_rows) = self.pane.as_ref()
                    .map(|p| (p.parser.grid.cols, p.parser.grid.rows))
                    .unwrap_or((80, 24));
                match handle_key(&event, &self.modifiers, &self.mode, grid_cols, grid_rows) {
                    Action::SendToPty(bytes) => {
                        if let Some(pty) = &mut self.pty {
                            let _ = pty.write_input(&bytes);
                        }
                    }
                    Action::SetMode(new_mode) => {
                        // When entering visual mode from outside, anchor cursor at PTY cursor pos
                        let mode = if let InputMode::Visual { .. } = &new_mode {
                            if matches!(self.mode, InputMode::Visual { .. }) {
                                // Already visual — accept the updated mode as-is (cursor moved)
                                new_mode
                            } else {
                                let (col, row) = self.pane.as_ref()
                                    .map(|p| (p.parser.grid.cursor_col, p.parser.grid.cursor_row))
                                    .unwrap_or((0, 0));
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
                        if let Some(pane) = &mut self.pane {
                            pane.scroll_up(n);
                        }
                    }
                    Action::ScrollDown(n) => {
                        if let Some(pane) = &mut self.pane {
                            pane.scroll_down(n);
                        }
                    }
                    Action::ScrollToTop => {
                        if let Some(pane) = &mut self.pane {
                            pane.scroll_top();
                        }
                    }
                    Action::ScrollToBottom => {
                        if let Some(pane) = &mut self.pane {
                            pane.scroll_bottom();
                        }
                    }
                    Action::Paste => {
                        if let Some(pty) = &mut self.pty {
                            match Clipboard::new().and_then(|mut cb| cb.get_text()) {
                                Ok(text) => {
                                    // Bracketed paste: wrap with ESC[200~ ... ESC[201~
                                    let mut data = b"\x1b[200~".to_vec();
                                    data.extend_from_slice(text.as_bytes());
                                    data.extend_from_slice(b"\x1b[201~");
                                    let _ = pty.write_input(&data);
                                }
                                Err(e) => log::warn!("Clipboard read failed: {e}"),
                            }
                        }
                    }
                    Action::Quit => event_loop.exit(),
                    Action::None => {}
                }
            }

            WindowEvent::RedrawRequested => {
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
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
