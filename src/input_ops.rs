use arboard::Clipboard;

use crate::input::InputMode;
use crate::input::keybindings::Action;
use crate::ui::SplitDir;
use crate::{AppEffect, pane_ops, session};
use winit::event::KeyEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::Fullscreen;

use super::App;

pub(super) fn bracketed_paste_encode(text: &str, bracketed: bool) -> Vec<u8> {
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
    pub(crate) fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    pub(crate) fn execute_action(&mut self, action: Action, event_loop: &ActiveEventLoop) {
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
                    let path = self.session_path();
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

    pub(crate) fn send_pane_focus_seq(&mut self, tab_idx: usize, pane_id: usize, gained: bool) {
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

    pub(crate) fn do_auto_split(&mut self) {
        let active = self.tab().active;
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let rect = self
            .tab()
            .layout
            .rects_scaled(tab_h, status_h)
            .into_iter()
            .find(|(id, _)| *id == active)
            .map(|(_, r)| r)
            .unwrap_or([0, tab_h, 100, 100]);
        let dir = if rect[2] >= rect[3] {
            SplitDir::H
        } else {
            SplitDir::V
        };
        self.do_split(dir);
    }

    pub(crate) fn do_resize_pane(&mut self, split_h: bool, delta: f32) {
        let active = self.tab().active;
        let ai = self.state.active_tab;
        self.state.tabs[ai]
            .layout
            .nudge_pane(active, split_h, delta);
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let pane_padding = self.pane_padding();
        Self::sync_pane_sizes_tab(&mut self.state.tabs[ai], tab_h, status_h, pane_padding);
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    pub(crate) fn do_rotate_panes(&mut self, forward: bool) {
        let ai = self.state.active_tab;
        self.state.tabs[ai].layout.rotate_leaves(forward);
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let pane_padding = self.pane_padding();
        Self::sync_pane_sizes_tab(&mut self.state.tabs[ai], tab_h, status_h, pane_padding);
        self.request_redraw();
    }

    pub(crate) fn do_toggle_log(&mut self) {
        let active = self.tab().active;
        let log_dir = self.state.config.logging.log_dir.clone();
        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
            if entry.log_file.is_some() {
                entry.log_file = None;
                log::info!("Logging stopped for pane {active}");
            } else {
                entry.log_file = pane_ops::open_log_file(active, &log_dir);
            }
        }
    }

    pub(crate) fn do_paste(&mut self) {
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

    pub(crate) fn do_toggle_fullscreen(&mut self) {
        if let Some(w) = &self.window {
            let fs = if w.fullscreen().is_some() {
                None
            } else {
                Some(Fullscreen::Borderless(None))
            };
            w.set_fullscreen(fs);
        }
    }

    pub(crate) fn do_new_tab(&mut self) {
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

    pub(crate) fn do_send_to_pty(&mut self, bytes: Vec<u8>) {
        let active = self.tab().active;
        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
            let _ = entry.pty.write_input(&bytes);
        }
    }

    pub(crate) fn change_font_size(&mut self, delta: f32) {
        let idx = self.state.active_tab;
        let logical = self.state.tabs[idx].logical_font_size;
        let Some((new_logical, new_metrics)) =
            crate::scaling::apply_font_delta(logical, delta, self.scale, &mut self.renderer)
        else {
            return;
        };
        self.state.tabs[idx].logical_font_size = new_logical;
        self.state.tabs[idx].metrics = new_metrics;
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let pane_padding = self.pane_padding();
        Self::sync_pane_sizes_tab(&mut self.state.tabs[idx], tab_h, status_h, pane_padding);
    }

    pub(crate) fn should_swallow_key(&mut self, event: &KeyEvent) -> bool {
        if self.state.swallow_next_tab {
            self.state.swallow_next_tab = false;
            if event.logical_key == Key::Named(NamedKey::Tab) {
                return true;
            }
        }
        false
    }
}
