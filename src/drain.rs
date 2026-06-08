use std::io::Write as _;
use std::time::Instant;

use arboard::Clipboard;
use base64::Engine as _;

use crate::app_state::{PaneEntry, TabState};

use super::App;

#[cfg(test)]
#[path = "drain_test.rs"]
mod tests;

impl App {
    pub(super) fn drain_all(&mut self) -> (Vec<(usize, usize)>, bool) {
        let active_tab = self.state.active_tab;
        let detect_urls = self.state.config.window.detect_urls;
        let mut exited = Vec::new();
        let mut has_more = false;
        for (tab_idx, tab) in self.state.tabs.iter_mut().enumerate() {
            let (more, disc) = drain_tab_panes(
                tab_idx,
                tab,
                &mut self.state.clipboard,
                detect_urls,
                active_tab,
            );
            if more {
                has_more = true;
            }
            exited.extend(disc.into_iter().map(|id| (tab_idx, id)));
        }
        if has_more {
            self.last_pty_data = Some(Instant::now());
        } else {
            self.last_pty_data = None;
        }
        (exited, has_more)
    }
}

fn drain_tab_panes(
    tab_idx: usize,
    tab: &mut TabState,
    clipboard: &mut Option<arboard::Clipboard>,
    detect_urls: bool,
    active_tab: usize,
) -> (bool, Vec<usize>) {
    let ids: Vec<usize> = tab.panes.keys().copied().collect();
    let mut has_more = false;
    let mut disconnected = Vec::new();
    for id in ids {
        let (got_data, more, disc) = {
            let entry = tab.panes.get_mut(&id).unwrap();
            poll_pane_bytes(entry, clipboard)
        };
        if more {
            has_more = true;
        }
        if disc {
            disconnected.push(id);
        }
        update_tab_after_pane_poll(tab, id, got_data, detect_urls, tab_idx != active_tab);
    }
    (has_more, disconnected)
}

pub(super) fn update_tab_after_pane_poll(
    tab: &mut TabState,
    id: usize,
    got_data: bool,
    detect_urls: bool,
    tab_is_background: bool,
) {
    if let Some(entry) = tab.panes.get_mut(&id) {
        if got_data && detect_urls {
            entry.pane.parser.grid.scan_urls();
        }
        if entry.pane.parser.grid.bell_pending {
            entry.pane.parser.grid.bell_pending = false;
            let now = Instant::now();
            let cooled = tab.bell_cooldown_until.is_none_or(|until| now >= until);
            if cooled {
                tab.bell_flash_start = Some(now);
                tab.bell_flash_until = Some(now + std::time::Duration::from_millis(150));
                tab.bell_cooldown_until = Some(now + std::time::Duration::from_millis(500));
            }
        }
    }
    if got_data && tab_is_background {
        tab.has_activity = true;
    }
}

/// Poll one pane's PTY channel up to BYTES_PER_FRAME bytes.
/// Returns (got_data, has_more, disconnected).
pub(super) fn poll_pane_bytes(
    entry: &mut PaneEntry,
    clipboard: &mut Option<Clipboard>,
) -> (bool, bool, bool) {
    const BYTES_PER_FRAME: usize = 256 * 1024;
    let mut got_data = false;
    let mut bytes_this_frame = 0usize;
    loop {
        match entry.rx.try_recv() {
            Ok(bytes) => {
                got_data = true;
                bytes_this_frame += bytes.len();
                process_pane_bytes(bytes, entry, clipboard);
                if bytes_this_frame >= BYTES_PER_FRAME {
                    return (true, true, false);
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => return (got_data, false, false),
            Err(crossbeam_channel::TryRecvError::Disconnected) => return (false, false, true),
        }
    }
}

pub(super) fn process_pane_bytes(
    bytes: Vec<u8>,
    entry: &mut PaneEntry,
    clipboard: &mut Option<Clipboard>,
) {
    if let Some(f) = &mut entry.log_file {
        let _ = f.write_all(&bytes);
    }
    entry.pane.process(&bytes);
    let responses = std::mem::take(&mut entry.pane.parser.grid.pending_responses);
    if !responses.is_empty() {
        let _ = entry.pty.write_input(&responses);
    }
    if let Some(text) = entry.pane.parser.grid.pending_clipboard_write.take()
        && let Some(cb) = clipboard.as_mut()
    {
        let _ = cb.set_text(text);
    }
    if std::mem::take(&mut entry.pane.parser.grid.pending_clipboard_read) {
        let text = clipboard
            .as_mut()
            .and_then(|cb| cb.get_text().ok())
            .unwrap_or_default();
        let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
        let resp = format!("\x1b]52;c;{encoded}\x1b\\");
        let _ = entry.pty.write_input(resp.as_bytes());
    }
    if let Some((title, body)) = entry.pane.parser.grid.pending_notification.take() {
        dispatch_notification(title, body);
    }
}

#[cfg(not(test))]
fn dispatch_notification(title: String, body: String) {
    #[cfg(target_os = "linux")]
    std::thread::spawn(move || {
        let _ = std::process::Command::new("notify-send")
            .arg(&title)
            .arg(&body)
            .status();
    });
    #[cfg(target_os = "macos")]
    std::thread::spawn(move || {
        let script = format!("display notification {body:?} with title {title:?}");
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .status();
    });
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let _ = (title, body);
}

#[cfg(test)]
fn dispatch_notification(_title: String, _body: String) {}
