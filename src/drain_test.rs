use std::collections::HashMap;
use std::time::Instant;

use crossbeam_channel::unbounded;

use crate::app_state::{PaneEntry, TabState};
use crate::renderer::FontMetrics;
use crate::terminal::grid::{Color, GridColors};
use crate::ui::Pane;
use crate::ui::layout::Layout;

use super::{poll_pane_bytes, update_tab_after_pane_poll};

fn dummy_metrics() -> FontMetrics {
    FontMetrics {
        font_px: 16.0,
        cell_width: 8,
        cell_height: 16,
        baseline: 13,
    }
}

fn make_tab() -> TabState {
    TabState {
        panes: HashMap::new(),
        layout: Layout::new(0, 800, 600),
        active: 0,
        metrics: dummy_metrics(),
        name: None,
        zoomed: false,
        has_activity: false,
        bell_flash_until: None,
        passthrough: false,
    }
}

/// Create a PaneEntry where `entry.rx` is a channel fully controlled by the
/// returned sender — the PTY output goes to a separate, discarded channel.
fn make_pane_entry() -> (PaneEntry, crossbeam_channel::Sender<Vec<u8>>) {
    let (test_tx, test_rx) = unbounded::<Vec<u8>>();
    let (pty_tx, _pty_rx) = unbounded::<Vec<u8>>();
    let colors = GridColors {
        fg: Color::WHITE,
        bg: Color::BLACK,
        cursor: Color::WHITE,
        selection: Color::WHITE,
        palette: [Color::BLACK; 16],
    };
    let pty = crate::pty::PtySession::spawn_with_shell(
        80,
        24,
        pty_tx,
        "/bin/true",
        None,
        Box::new(|| {}),
    )
    .expect("PTY spawn failed");
    let pane = Pane::new_with_colors(80, 24, [0, 22, 800, 556], colors, 1000);
    let entry = PaneEntry {
        pane,
        pty,
        rx: test_rx,
        log_file: None,
    };
    (entry, test_tx)
}

// ── update_tab_after_pane_poll ────────────────────────────────────────────────

#[test]
fn no_data_leaves_activity_unchanged() {
    let mut tab = make_tab();
    let (entry, _tx) = make_pane_entry();
    tab.panes.insert(1, entry);
    tab.has_activity = false;
    update_tab_after_pane_poll(&mut tab, 1, false, false, true);
    assert!(!tab.has_activity);
}

#[test]
fn got_data_background_tab_sets_activity() {
    let mut tab = make_tab();
    let (entry, _tx) = make_pane_entry();
    tab.panes.insert(1, entry);
    update_tab_after_pane_poll(&mut tab, 1, true, false, true);
    assert!(tab.has_activity);
}

#[test]
fn got_data_foreground_tab_no_activity() {
    let mut tab = make_tab();
    let (entry, _tx) = make_pane_entry();
    tab.panes.insert(1, entry);
    update_tab_after_pane_poll(&mut tab, 1, true, false, false);
    assert!(!tab.has_activity);
}

#[test]
fn bell_pending_sets_bell_flash_until() {
    let mut tab = make_tab();
    let (mut entry, _tx) = make_pane_entry();
    entry.pane.parser.grid.bell_pending = true;
    tab.panes.insert(1, entry);
    assert!(tab.bell_flash_until.is_none());
    update_tab_after_pane_poll(&mut tab, 1, false, false, false);
    assert!(tab.bell_flash_until.is_some());
}

#[test]
fn bell_pending_cleared_after_poll() {
    let mut tab = make_tab();
    let (mut entry, _tx) = make_pane_entry();
    entry.pane.parser.grid.bell_pending = true;
    tab.panes.insert(1, entry);
    update_tab_after_pane_poll(&mut tab, 1, false, false, false);
    assert!(!tab.panes[&1].pane.parser.grid.bell_pending);
}

#[test]
fn pane_not_found_no_bell_flash() {
    let mut tab = make_tab();
    // id 99 not in panes — bell/URL code is skipped, should not panic
    update_tab_after_pane_poll(&mut tab, 99, false, false, false);
    assert!(tab.bell_flash_until.is_none());
}

#[test]
fn detect_urls_with_data_does_not_panic() {
    let mut tab = make_tab();
    let (entry, _tx) = make_pane_entry();
    tab.panes.insert(1, entry);
    // Just verify no panic; URL detection outcome depends on grid content.
    update_tab_after_pane_poll(&mut tab, 1, true, true, false);
}

// ── poll_pane_bytes ───────────────────────────────────────────────────────────

#[test]
fn empty_channel_returns_no_data() {
    let (mut entry, _tx) = make_pane_entry();
    let (got_data, has_more, disc) = poll_pane_bytes(&mut entry, &mut None);
    assert!(!got_data);
    assert!(!has_more);
    assert!(!disc);
}

#[test]
fn one_chunk_received_returns_got_data() {
    let (mut entry, tx) = make_pane_entry();
    tx.send(b"hello".to_vec()).unwrap();
    // Keep tx alive so the next try_recv sees Empty, not Disconnected.
    // Dropping tx here would cause the loop to see Disconnected after the
    // chunk and return (false, false, true) — not (true, false, false).
    let (got_data, has_more, disc) = poll_pane_bytes(&mut entry, &mut None);
    drop(tx);
    assert!(got_data);
    assert!(!has_more);
    assert!(!disc);
}

#[test]
fn disconnected_channel_returns_disc() {
    let (mut entry, tx) = make_pane_entry();
    drop(tx);
    let (got_data, has_more, disc) = poll_pane_bytes(&mut entry, &mut None);
    assert!(!got_data);
    assert!(!has_more);
    assert!(disc);
}

#[test]
fn bytes_over_limit_returns_has_more() {
    let (mut entry, tx) = make_pane_entry();
    // 256 KB + 1 byte spread across two chunks triggers the frame budget.
    let chunk_a = vec![b'A'; 256 * 1024];
    let chunk_b = vec![b'B'; 1];
    tx.send(chunk_a).unwrap();
    tx.send(chunk_b).unwrap();
    let (got_data, has_more, disc) = poll_pane_bytes(&mut entry, &mut None);
    assert!(got_data);
    assert!(has_more);
    assert!(!disc);
}

// ── process_pane_bytes ────────────────────────────────────────────────────────

#[test]
fn process_bytes_updates_grid() {
    let (mut entry, _tx) = make_pane_entry();
    // Write ASCII 'X' to the grid and verify cursor advanced
    let before_col = entry.pane.parser.grid.cursor_col;
    super::process_pane_bytes(b"X".to_vec(), &mut entry, &mut None);
    assert!(entry.pane.parser.grid.cursor_col > before_col);
}

#[test]
fn process_bytes_clipboard_write_consumed() {
    let (mut entry, _tx) = make_pane_entry();
    entry.pane.parser.grid.pending_clipboard_write = Some("copied text".into());
    // clipboard is None — write is attempted and silently dropped
    super::process_pane_bytes(vec![], &mut entry, &mut None);
    assert!(entry.pane.parser.grid.pending_clipboard_write.is_none());
}

#[test]
fn process_bytes_clipboard_read_cleared() {
    let (mut entry, _tx) = make_pane_entry();
    entry.pane.parser.grid.pending_clipboard_read = true;
    // clipboard is None — read returns empty string, response written to PTY
    super::process_pane_bytes(vec![], &mut entry, &mut None);
    assert!(!entry.pane.parser.grid.pending_clipboard_read);
}

#[test]
fn bell_flash_until_is_recent_after_bell() {
    let mut tab = make_tab();
    let (mut entry, _tx) = make_pane_entry();
    entry.pane.parser.grid.bell_pending = true;
    tab.panes.insert(1, entry);
    let before = Instant::now();
    update_tab_after_pane_poll(&mut tab, 1, false, false, false);
    let after = Instant::now();
    let flash = tab.bell_flash_until.unwrap();
    assert!(flash > before);
    assert!(flash <= after + std::time::Duration::from_millis(200));
}
