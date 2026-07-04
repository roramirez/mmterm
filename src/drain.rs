use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Instant;

use base64::Engine as _;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TryRecvError};

use crate::app_state::TabState;
use crate::terminal::TerminalParser;
use crate::terminal::grid::Grid;

use super::App;

// ── Lock hierarchy (per pane) ─────────────────────────────────────────────────
//
// Each `PaneEntry` carries three locks shared between the parser thread (this
// module) and the main thread (`renderer/render_ops.rs`, `pane_ops.rs`):
//
//   * `grid`           — `Arc<RwLock<Grid>>` (cell grid + scrollback)
//   * `log_file`       — `Arc<Mutex<Option<File>>>` (raw-byte session log)
//   * `pending_resize` — `Arc<Mutex<Option<(cols, rows)>>>` (main → parser signal)
//
// PRIMARY INVARIANT — never hold more than one of these at a time.
// Every site acquires one lock, does the minimum work, and releases it before
// acquiring the next. This is the rule that actually prevents deadlock and it
// holds everywhere today:
//   - parser thread (below): `log_file` is taken and dropped (~:130), then
//     `pending_resize.take()` is taken and dropped (~:137), and only then is the
//     single `grid.write()` held (~:143). The resize is applied *inside* that
//     grid write lock so the main thread never needs `grid.write()` — it only
//     sets `pending_resize` and returns.
//   - `render_ops::redraw` reads `grid` in a scoped block, drops it, then checks
//     `log_file` separately.
//   - `pane_ops::sync_pane_sizes_tab` reads `grid` in a scoped block, drops it,
//     then sets `pending_resize`.
//
// SECONDARY RULE — should future code ever be forced to hold two at once,
// acquire them outermost-to-innermost in this order, and never acquire an outer
// lock while already holding an inner one:
//
//       log_file  >  grid  >  pending_resize
//
// The `log_file > grid` half is load-bearing: the parser reaches `log_file`
// before `grid`, so the main thread must never hold `grid` while waiting on
// `log_file` (it would stall both threads — see the note in
// `renderer/render_ops.rs::redraw`).

#[cfg(test)]
#[path = "drain_test.rs"]
mod tests;

// ── ParseEffect ───────────────────────────────────────────────────────────────

/// Side-effects produced by a parser thread batch and consumed on the main thread.
pub enum ParseEffect {
    PtyResponse(Vec<u8>),
    ClipboardWrite(String),
    ClipboardRead,
    Bell,
    /// Scrollback length changed; main thread adjusts scroll_offset to match.
    /// `old` may be greater than `new` on alternate screen entry (clamp case).
    ScrollbackChanged {
        old: usize,
        new: usize,
    },
    /// Grid was resized by the parser thread; main thread adjusts scroll_offset.
    Resized {
        /// Signed delta from Grid::resize (positive = lines added to scrollback).
        delta: isize,
        /// Scrollback length after resize.
        new_sb: usize,
    },
    /// Parser thread's PTY EOF — pane should be closed.
    Disconnected,
    /// OSC 777 desktop notification request (title, body); dispatched on the
    /// main thread, gated by general.desktop_notifications.
    Notification {
        title: String,
        body: String,
    },
    /// The parser processed a non-empty batch of PTY bytes this iteration.
    /// Emitted once per batch so the main thread can flag activity on inactive
    /// tabs (output that only repaints the visible grid produces no other effect).
    Output,
}

// ── Parser thread ─────────────────────────────────────────────────────────────

/// Bytes drained from the PTY channel per parser iteration.
/// Caps write-lock duration at ~36 ms (32 KiB / 885 KiB/s).
const PARSE_BATCH_MAX: usize = 32 * 1024;

/// Arguments for [`spawn_parser_thread`].
pub struct ParserThreadArgs {
    pub rx: Receiver<Vec<u8>>,
    pub grid: Arc<RwLock<Grid>>,
    pub log_file: Arc<Mutex<Option<std::fs::File>>>,
    pub effects_tx: Sender<ParseEffect>,
    /// App-level wakeup flag (same Arc used by the wakeup closure). The parser
    /// reads it to decide how long to yield after each batch — ensuring the
    /// render thread can acquire the grid read-lock regularly even during heavy output.
    pub wakeup_pending: Arc<AtomicBool>,
    /// Non-blocking resize request set by the main thread; parser applies it
    /// within its existing write lock so the event loop never blocks on grid.write().
    pub pending_resize: Arc<Mutex<Option<(usize, usize)>>>,
    pub wakeup: Box<dyn Fn() + Send + 'static>,
}

/// Spawn a per-pane parser thread that owns the VTE state machine.
/// The thread reads PTY bytes from `rx`, parses them into `grid` (write lock),
/// and sends side-effects to `effects_tx`. All bytes are processed in order —
/// no discard path. The bounded PTY channel (see `pane_ops`) caps the backlog
/// at ~1 MB and provides natural backpressure during heavy output.
///
/// `wakeup_pending` is the app-level wakeup flag. The parser reads it to decide
/// how long to yield after each batch so the render thread gets regular access.
pub fn spawn_parser_thread(args: ParserThreadArgs) -> thread::JoinHandle<()> {
    let ParserThreadArgs {
        rx,
        grid,
        log_file,
        effects_tx,
        wakeup_pending,
        pending_resize,
        wakeup,
    } = args;
    thread::spawn(move || {
        let mut parser = TerminalParser::new();
        // Maximum time to wait for PTY bytes before checking for a pending resize.
        const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);

        loop {
            // Wait for the first chunk from the PTY reader thread.
            // Use a timeout so that a pending resize is applied even when the
            // terminal is idle (no PTY output) — e.g. user resizes after Ctrl+C
            // but before the shell redraws its prompt.
            let first = match rx.recv_timeout(IDLE_TIMEOUT) {
                Ok(b) => b,
                Err(RecvTimeoutError::Timeout) => {
                    // Check for a pending resize while the channel is quiet.
                    let pending = pending_resize.lock().unwrap().take();
                    if let Some((new_cols, new_rows)) = pending {
                        let mut g = grid.write().unwrap();
                        let delta = g.resize(new_cols, new_rows);
                        let new_sb = g.scrollback_len();
                        drop(g);
                        let _ = effects_tx.send(ParseEffect::Resized { delta, new_sb });
                        wakeup();
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = effects_tx.send(ParseEffect::Disconnected);
                    return;
                }
            };

            // Drain any further immediately available chunks (up to batch cap).
            let mut batch = first;
            while batch.len() < PARSE_BATCH_MAX {
                match rx.try_recv() {
                    Ok(more) => batch.extend_from_slice(&more),
                    Err(_) => break,
                }
            }

            // Log raw bytes before parsing.
            if let Ok(mut guard) = log_file.lock()
                && let Some(f) = guard.as_mut()
            {
                let _ = f.write_all(&batch);
            }

            // Extract pending resize BEFORE the write lock (no nested lock acquisition).
            let pending = pending_resize.lock().unwrap().take();

            // Parse, scan URLs, optionally resize, and extract side-effects in one write lock.
            // Resize is applied here so the main thread never calls grid.write() — it just
            // sets pending_resize and returns, keeping the event loop and renders fluid.
            let (
                old_sb,
                new_sb,
                resp,
                clipboard_write,
                clipboard_read,
                bell,
                notification,
                resize_effect,
            ) = {
                let mut g = grid.write().unwrap();
                let old = g.scrollback_len();
                parser.process(&batch, &mut g);
                g.scan_urls();
                let new = g.scrollback_len(); // parse-only scrollback delta (before any resize)
                let resize_effect = pending.map(|(new_cols, new_rows)| {
                    let delta = g.resize(new_cols, new_rows);
                    ParseEffect::Resized {
                        delta,
                        new_sb: g.scrollback_len(),
                    }
                });
                let resp = std::mem::take(&mut g.pending_responses);
                let cw = g.pending_clipboard_write.take();
                let cr = std::mem::take(&mut g.pending_clipboard_read);
                let b = std::mem::take(&mut g.bell_pending);
                let notif = g.pending_notification.take();
                (old, new, resp, cw, cr, b, notif, resize_effect)
            };

            if new_sb != old_sb {
                let _ = effects_tx.send(ParseEffect::ScrollbackChanged {
                    old: old_sb,
                    new: new_sb,
                });
            }
            if let Some(effect) = resize_effect {
                let _ = effects_tx.send(effect);
            }
            if !resp.is_empty() {
                let _ = effects_tx.send(ParseEffect::PtyResponse(resp));
            }
            if let Some(t) = clipboard_write {
                let _ = effects_tx.send(ParseEffect::ClipboardWrite(t));
            }
            if clipboard_read {
                let _ = effects_tx.send(ParseEffect::ClipboardRead);
            }
            if bell {
                let _ = effects_tx.send(ParseEffect::Bell);
            }
            if let Some((title, body)) = notification {
                let _ = effects_tx.send(ParseEffect::Notification { title, body });
            }
            // Signal that this pane produced output this batch. Batches only form
            // when PTY bytes arrived, so reaching here always means real output.
            let _ = effects_tx.send(ParseEffect::Output);

            wakeup();

            // Cooperative yield: prevent back-to-back write locks from starving
            // the render thread's read-lock requests.
            //
            // Without this, `find /` saturates the parser at ~100% CPU, holding
            // the write lock for ~36 ms per batch with no gap. The render thread
            // (needing a read lock) can never make progress → terminal appears
            // frozen and window resize hangs.
            //
            // Strategy: always yield once (sched_yield, ~0–1 ms on Linux), then
            // if the render thread still hasn't consumed the wakeup event
            // (wakeup_pending=true), sleep 4 ms to give it a full scheduling
            // window. This caps effective batch rate at ~1000/(36+4) ≈ 25 fps
            // under maximum load while keeping throughput near 885 KiB/s.
            std::thread::yield_now();
            if wakeup_pending.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
        }
    })
}

// ── Main-thread drain ─────────────────────────────────────────────────────────

impl App {
    /// Consume side-effects from all pane parser threads.
    /// Returns (tab_idx, pane_id) pairs for panes whose PTY disconnected.
    pub(super) fn drain_effects(&mut self) -> Vec<(usize, usize)> {
        // Phase 1: drain per-pane effects that only touch the pane/PTY.
        // Defer clipboard and bell effects (need self-level access) for phase 2.
        struct Deferred {
            tab_idx: usize,
            pane_id: usize,
            kind: DeferredKind,
        }
        enum DeferredKind {
            ClipboardWrite(String),
            ClipboardRead,
            Disconnected,
        }
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut bell_tabs: std::collections::HashSet<usize> = Default::default();

        for tab_idx in 0..self.state.tabs.len() {
            let pane_ids: Vec<usize> = self.state.tabs[tab_idx].panes.keys().copied().collect();
            for pane_id in pane_ids {
                loop {
                    let recv = self.state.tabs[tab_idx]
                        .panes
                        .get_mut(&pane_id)
                        .map(|e| e.effects_rx.try_recv());
                    let effect = match recv {
                        None | Some(Err(TryRecvError::Empty)) => break,
                        // Parser thread panicked without sending Disconnected —
                        // treat channel closure the same as an explicit Disconnected.
                        Some(Err(TryRecvError::Disconnected)) => {
                            deferred.push(Deferred {
                                tab_idx,
                                pane_id,
                                kind: DeferredKind::Disconnected,
                            });
                            break;
                        }
                        Some(Ok(e)) => e,
                    };
                    match effect {
                        ParseEffect::PtyResponse(r) => {
                            if let Some(e) = self.state.tabs[tab_idx].panes.get_mut(&pane_id) {
                                let _ = e.pty.write_input(&r);
                            }
                        }
                        ParseEffect::ScrollbackChanged { old, new } => {
                            if let Some(e) = self.state.tabs[tab_idx].panes.get_mut(&pane_id)
                                && e.pane.scroll_offset > 0
                            {
                                let added = new.saturating_sub(old);
                                // Cap by the CURRENT scrollback length, not the effect's
                                // `new` value. A resize between effect generation and
                                // drain could have shrunk the scrollback, making `new`
                                // stale. Using `new` would allow scroll_offset > actual
                                // scrollback, causing a viewport glitch.
                                let current_sb = e.pane.grid.read().unwrap().scrollback_len();
                                e.pane.scroll_offset =
                                    (e.pane.scroll_offset + added).min(current_sb);
                            }
                        }
                        ParseEffect::Resized { delta, new_sb } => {
                            if let Some(e) = self.state.tabs[tab_idx].panes.get_mut(&pane_id)
                                && e.pane.scroll_offset > 0
                            {
                                e.pane.scroll_offset =
                                    ((e.pane.scroll_offset as isize) + delta).max(0) as usize;
                                e.pane.scroll_offset = e.pane.scroll_offset.min(new_sb);
                            }
                        }
                        ParseEffect::Bell => {
                            bell_tabs.insert(tab_idx);
                        }
                        ParseEffect::Output => {
                            // Flag activity on inactive tabs so the tab bar shows a
                            // dot. The active tab's flag is cleared every redraw
                            // (see render_ops), so marking it here would be moot.
                            if tab_idx != self.state.active_tab {
                                self.state.tabs[tab_idx].has_activity = true;
                            }
                        }
                        ParseEffect::ClipboardWrite(t) => {
                            deferred.push(Deferred {
                                tab_idx,
                                pane_id,
                                kind: DeferredKind::ClipboardWrite(t),
                            });
                        }
                        ParseEffect::ClipboardRead => {
                            deferred.push(Deferred {
                                tab_idx,
                                pane_id,
                                kind: DeferredKind::ClipboardRead,
                            });
                        }
                        ParseEffect::Notification { title, body } => {
                            if self.state.config.general.desktop_notifications {
                                dispatch_notification(title, body);
                            }
                        }
                        ParseEffect::Disconnected => {
                            deferred.push(Deferred {
                                tab_idx,
                                pane_id,
                                kind: DeferredKind::Disconnected,
                            });
                            break;
                        }
                    }
                }
            }
        }

        // Phase 2: apply deferred effects (clipboard / disconnect).
        let mut exited = Vec::new();
        let now = Instant::now();
        for d in deferred {
            match d.kind {
                DeferredKind::ClipboardWrite(t) => {
                    if let Some(cb) = self.state.clipboard.as_mut() {
                        let _ = cb.set_text(t);
                    }
                }
                DeferredKind::ClipboardRead => {
                    let text = self
                        .state
                        .clipboard
                        .as_mut()
                        .and_then(|cb| cb.get_text().ok())
                        .unwrap_or_default();
                    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
                    let resp = format!("\x1b]52;c;{encoded}\x1b\\");
                    if let Some(e) = self.state.tabs[d.tab_idx].panes.get_mut(&d.pane_id) {
                        let _ = e.pty.write_input(resp.as_bytes());
                    }
                }
                DeferredKind::Disconnected => exited.push((d.tab_idx, d.pane_id)),
            }
        }
        for tab_idx in bell_tabs {
            if let Some(tab) = self.state.tabs.get_mut(tab_idx) {
                trigger_bell(tab, now);
            }
        }
        exited
    }
}

fn trigger_bell(tab: &mut TabState, now: Instant) {
    let cooled = tab.bell_cooldown_until.is_none_or(|until| now >= until);
    if cooled {
        tab.bell_flash_start = Some(now);
        tab.bell_flash_until = Some(now + std::time::Duration::from_millis(150));
        tab.bell_cooldown_until = Some(now + std::time::Duration::from_millis(500));
    }
}

/// Dispatch an OSC 777 desktop notification from a detached thread so the event
/// loop never blocks on the external notifier process.
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
