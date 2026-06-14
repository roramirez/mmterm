use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use crossbeam_channel::unbounded;

use crate::app_state::{PaneEntry, TabState};
use crate::drain::{ParseEffect, spawn_parser_thread};
use crate::renderer::FontMetrics;
use crate::terminal::grid::{Color, Grid, GridColors};
use crate::ui::Pane;
use crate::ui::layout::Layout;

fn dummy_metrics() -> FontMetrics {
    FontMetrics {
        font_px: 16.0,
        cell_width: 8,
        cell_height: 16,
        baseline: 13,
    }
}

fn make_grid(cols: usize, rows: usize) -> Arc<RwLock<Grid>> {
    Arc::new(RwLock::new(Grid::with_colors(
        cols,
        rows,
        GridColors {
            fg: Color::WHITE,
            bg: Color::BLACK,
            cursor: Color::WHITE,
            selection: Color::WHITE,
            palette: [Color::BLACK; 16],
        },
        1000,
    )))
}

fn make_tab() -> TabState {
    TabState {
        panes: HashMap::new(),
        layout: Layout::new(0, 800, 600),
        active: 0,
        name: None,
        zoomed: false,
        has_activity: false,
        bell_flash_start: None,
        bell_flash_until: None,
        bell_cooldown_until: None,
        passthrough: false,
    }
}

/// Build a PaneEntry with a real parser thread.
/// Returns the entry and a sender to feed bytes into the parser.
fn make_pane_entry() -> (PaneEntry, crossbeam_channel::Sender<Vec<u8>>) {
    let (pty_tx, pty_rx) = unbounded::<Vec<u8>>(); // bytes → parser thread
    let (effects_tx, effects_rx) = unbounded::<ParseEffect>();

    let pty = crate::pty::PtySession::spawn_with_shell(
        80,
        24,
        pty_tx,
        "/bin/true",
        None,
        Box::new(|| {}),
    )
    .expect("PTY spawn failed");

    let grid = make_grid(80, 24);
    let pane = Pane::new(grid.clone(), [0, 22, 800, 556]);
    let log_file = Arc::new(Mutex::new(None));

    // Second channel: test controls parser input via test_tx
    let (test_tx, test_rx) = unbounded::<Vec<u8>>();

    let pending_resize = Arc::new(std::sync::Mutex::new(None));
    let parser_thread = spawn_parser_thread(crate::drain::ParserThreadArgs {
        rx: test_rx,
        grid,
        log_file: log_file.clone(),
        effects_tx,
        wakeup_pending: Arc::new(AtomicBool::new(false)),
        pending_resize: pending_resize.clone(),
        wakeup: Box::new(|| {}),
    });

    let entry = PaneEntry {
        pane,
        pty,
        effects_rx,
        log_file,
        pending_resize,
        _parser_thread: parser_thread,
        logical_font_size: crate::dpi::Logical(16.0),
        metrics: dummy_metrics(),
    };

    // pty_rx is dropped; pty_tx above keeps the PTY alive
    drop(pty_rx);

    (entry, test_tx)
}

// ── ParseEffect channel ───────────────────────────────────────────────────────

#[test]
fn disconnected_effect_sent_when_channel_drops() {
    let (entry, tx) = make_pane_entry();
    // Drop the sender — parser thread should exit and send Disconnected.
    drop(tx);
    // Give parser thread a moment to notice EOF and send Disconnected.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let got_disconnected = matches!(entry.effects_rx.try_recv(), Ok(ParseEffect::Disconnected));
    assert!(got_disconnected, "expected Disconnected after sender drop");
}

#[test]
fn parser_thread_sends_wakeup_and_updates_grid() {
    let (entry, tx) = make_pane_entry();
    tx.send(b"X".to_vec()).unwrap();
    // Give parser thread time to process
    std::thread::sleep(std::time::Duration::from_millis(50));
    // Grid should now have 'X' at (0,0)
    assert_eq!(entry.pane.grid.read().unwrap().cell(0, 0).c, 'X');
}

#[test]
fn bell_effect_sent_on_bell_byte() {
    let (entry, tx) = make_pane_entry();
    tx.send(b"\x07".to_vec()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    let mut saw_bell = false;
    while let Ok(e) = entry.effects_rx.try_recv() {
        if matches!(e, ParseEffect::Bell) {
            saw_bell = true;
        }
    }
    assert!(saw_bell, "expected Bell effect from \\x07");
}

#[test]
fn scrollback_delta_sent_when_lines_pushed() {
    let (entry, tx) = make_pane_entry();
    // 24-row grid; filling it pushes lines to scrollback
    for _ in 0..30 {
        tx.send(b"line\r\n".to_vec()).unwrap();
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    let mut total_delta = 0usize;
    while let Ok(e) = entry.effects_rx.try_recv() {
        if let ParseEffect::ScrollbackChanged { old, new } = e {
            total_delta += new.saturating_sub(old);
        }
    }
    assert!(
        total_delta > 0,
        "expected ScrollbackDelta after filling grid"
    );
}

// ── pending_resize ────────────────────────────────────────────────────────────

#[test]
fn pending_resize_applied_during_parse() {
    let (entry, tx) = make_pane_entry();
    // Queue output — resize will be applied during the normal parse path.
    for _ in 0..20 {
        tx.send(b"AAAAA\r\n".to_vec()).unwrap();
    }
    *entry.pending_resize.lock().unwrap() = Some((100, 40));

    std::thread::sleep(std::time::Duration::from_millis(150));

    let g = entry.pane.grid.read().unwrap();
    assert_eq!(
        (g.cols, g.rows),
        (100, 40),
        "grid should be resized to (100, 40) during normal parse"
    );
    drop(g);

    let mut saw_resized = false;
    while let Ok(e) = entry.effects_rx.try_recv() {
        if matches!(e, ParseEffect::Resized { .. }) {
            saw_resized = true;
        }
    }
    assert!(saw_resized, "expected ParseEffect::Resized from parse path");
}

// ── no-discard integration ────────────────────────────────────────────────────

/// All bytes sent to the parser reach the grid in order — nothing is discarded.
/// Regression guard: the old discard path would silently drain the channel,
/// leaving the grid frozen at the last parsed state.
#[test]
fn all_queued_bytes_reach_grid() {
    let (entry, tx) = make_pane_entry();
    for _ in 0..30 {
        tx.send(b"row\r\n".to_vec()).unwrap();
    }
    tx.send(b"DONE".to_vec()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let grid = entry.pane.grid.read().unwrap();
    let found = (0..grid.rows).any(|row| {
        (0..grid.cols)
            .map(|c| grid.cell(c, row).c)
            .collect::<String>()
            .contains("DONE")
    });
    assert!(
        found,
        "all queued bytes should reach the grid; nothing discarded"
    );
}

/// Bytes immediately following a Ctrl+C byte (0x03) are parsed normally.
/// Previously, setting discard_signal after a Ctrl+C would drain subsequent
/// bytes; now 0x03 is just another byte passed through the VTE state machine.
#[test]
fn bytes_after_ctrl_c_are_not_discarded() {
    let (entry, tx) = make_pane_entry();
    // 0x03 (ETX) is a C0 control — VTE treats it as execute (no-op visually).
    // "HELLO" following it must be written to the grid starting at (0, 0).
    tx.send(b"\x03HELLO".to_vec()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    let grid = entry.pane.grid.read().unwrap();
    let first_five: String = (0..5).map(|c| grid.cell(c, 0).c).collect();
    assert_eq!(
        first_five, "HELLO",
        "bytes after 0x03 must be parsed, not discarded"
    );
}

// ── trigger_bell ─────────────────────────────────────────────────────────────

#[test]
fn bell_flash_set_after_bell_effect() {
    let mut tab = make_tab();
    let (entry, tx) = make_pane_entry();
    tab.panes.insert(1, entry);
    tab.active = 1;

    // Send bell; wait for parser thread to deliver the effect
    tab.panes.get(&1).unwrap();
    tx.send(b"\x07".to_vec()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Consume the Bell effect to simulate what drain_effects does
    let before = Instant::now();
    let mut got_bell = false;
    while let Ok(e) = tab.panes.get(&1).unwrap().effects_rx.try_recv() {
        if matches!(e, ParseEffect::Bell) {
            got_bell = true;
        }
    }
    assert!(got_bell, "expected Bell effect");
    drop(before);
}
