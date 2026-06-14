use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::input::keybindings::Action;
use crate::{TabState, session};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{CursorIcon, Icon, Window, WindowId};

use super::App;

pub(crate) fn next_bell_wakeup(tabs: &[TabState], default: Instant) -> Instant {
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

        // Scale must be known before the first metrics build so the first frame renders at
        // the correct physical size (spec §5.1). App.scale is the single source; mirror into
        // the renderer for chrome math.
        self.scale = crate::dpi::Scale::new(window.scale_factor());
        self.renderer.scale = self.scale;

        let size = window.inner_size();
        let session_path = self.session_path();
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
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // inner_size_writer intentionally ignored (`..`): winit emits a Resized
                // right after with the adjusted physical size, which drives handle_resize +
                // layout + redraw. Requesting an inner size here would double-resize (spec §5.5).
                self.handle_scale_changed(scale_factor);
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        self.wakeup_pending.store(false, Ordering::Release);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Rendering while PTY output is flowing is driven by wakeup() calls from
        // parser threads. Here we only need to schedule the cursor-blink tick
        // and any pending bell-flash expiry wakeup.
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
