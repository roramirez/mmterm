use super::*;
use crate::terminal::grid::{Color, GridColors};

fn make_pane(cols: usize, rows: usize) -> Pane {
    Pane::new_with_colors(
        cols,
        rows,
        [0, 0, cols as u32 * 8, rows as u32 * 16],
        GridColors {
            fg: Color::WHITE,
            bg: Color::BLACK,
            cursor: Color::CURSOR,
            selection: Color::SELECTION,
            palette: [Color::BLACK; 16],
        },
        10_000,
    )
}

#[test]
fn new_pane_scroll_offset_is_zero() {
    let pane = make_pane(80, 24);
    assert_eq!(pane.scroll_offset, 0);
}

#[test]
fn process_pins_view_when_scrolled_up() {
    // With a 5-row pane, each \r\n beyond row 5 adds a line to scrollback.
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_up(3);
    let sb_before = pane.parser.grid.scrollback_len();
    let offset_before = pane.scroll_offset;

    // Write one more line — scrollback grows by 1.
    pane.process(b"new\r\n");
    let sb_after = pane.parser.grid.scrollback_len();

    // offset must have grown by the same amount so the view stays pinned.
    assert_eq!(pane.scroll_offset, offset_before + (sb_after - sb_before));
}

#[test]
fn process_stays_at_bottom_when_not_scrolled() {
    let mut pane = make_pane(80, 24);
    for _ in 0..30 {
        pane.process(b"line\r\n");
    }
    assert_eq!(pane.scroll_offset, 0);
    pane.process(b"new output");
    assert_eq!(pane.scroll_offset, 0);
}

#[test]
fn process_clamps_offset_when_scrollback_shrinks() {
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_top();
    let max_before = pane.scroll_offset;
    assert!(max_before > 0);
    // Enter alternate screen — scrollback is now empty
    pane.process(b"\x1b[?1049h");
    assert_eq!(pane.scroll_offset, 0);
}

#[test]
fn scroll_up_clamps_to_scrollback_len() {
    let mut pane = make_pane(80, 5);
    for _ in 0..3 {
        pane.process(b"line\r\n");
    }
    let max = pane.parser.grid.scrollback_len();
    pane.scroll_up(max + 100);
    assert_eq!(pane.scroll_offset, max);
}

#[test]
fn scroll_down_clamps_at_zero() {
    let mut pane = make_pane(80, 5);
    pane.scroll_down(10);
    assert_eq!(pane.scroll_offset, 0);
}

#[test]
fn scroll_top_sets_max_offset() {
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_top();
    assert_eq!(pane.scroll_offset, pane.parser.grid.scrollback_len());
}

#[test]
fn scroll_bottom_resets_to_zero() {
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_top();
    pane.scroll_bottom();
    assert_eq!(pane.scroll_offset, 0);
}

#[test]
fn resize_updates_grid_and_rect() {
    let mut pane = make_pane(80, 24);
    pane.resize(40, 12, [0, 0, 320, 192]);
    assert_eq!(pane.parser.grid.cols, 40);
    assert_eq!(pane.parser.grid.rows, 12);
    assert_eq!(pane.rect, [0, 0, 320, 192]);
}

#[test]
fn scroll_up_increments_offset_by_n() {
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_up(3);
    assert_eq!(pane.scroll_offset, 3);
}

#[test]
fn scroll_down_decrements_offset_by_n() {
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_up(6);
    pane.scroll_down(2);
    assert_eq!(pane.scroll_offset, 4);
}

#[test]
fn scroll_up_then_down_round_trips() {
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_up(5);
    pane.scroll_down(5);
    assert_eq!(pane.scroll_offset, 0);
}

#[test]
fn scroll_top_then_scroll_bottom_resets() {
    let mut pane = make_pane(80, 5);
    for _ in 0..15 {
        pane.process(b"line\r\n");
    }
    let max = pane.parser.grid.scrollback_len();
    pane.scroll_top();
    assert_eq!(pane.scroll_offset, max);
    pane.scroll_bottom();
    assert_eq!(pane.scroll_offset, 0);
}

// ── Scenario tests ───────────────────────────────────────────────────────────
// Each test simulates a complete user interaction sequence to guard against
// regressions across the scroll-preservation and snap-to-bottom behaviours.

#[test]
fn scenario_scroll_up_then_user_input_snaps_to_bottom() {
    // User scrolls up to review history, then presses Enter.
    // The view must snap to the bottom so the response is visible.
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_up(4);
    assert!(pane.scroll_offset > 0);

    // Simulate SendToPty: snap to bottom before writing input.
    pane.scroll_bottom();
    assert_eq!(pane.scroll_offset, 0);

    // New output from the command stays at the bottom.
    pane.process(b"result\r\n");
    assert_eq!(pane.scroll_offset, 0);
}

#[test]
fn scenario_view_stays_pinned_across_multiple_output_batches() {
    // User scrolls up; PTY sends several bursts of output.
    // The view must remain anchored to the same scrollback content each time.
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_up(3);

    for _ in 0..5 {
        let sb_before = pane.parser.grid.scrollback_len();
        let offset_before = pane.scroll_offset;
        pane.process(b"more\r\n");
        let added = pane.parser.grid.scrollback_len().saturating_sub(sb_before);
        assert_eq!(
            pane.scroll_offset,
            offset_before + added,
            "view drifted after a burst of output"
        );
    }
}

#[test]
fn scenario_scroll_to_top_clamps_when_scrollback_saturates() {
    // User scrolls to the very top; heavy output keeps arriving.
    // scroll_offset must never exceed scrollback_len (no panic on render).
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_top();

    for _ in 0..20 {
        pane.process(b"flood\r\n");
        let max = pane.parser.grid.scrollback_len();
        assert!(
            pane.scroll_offset <= max,
            "scroll_offset exceeded scrollback_len"
        );
    }
}

#[test]
fn scenario_scroll_up_user_input_then_more_output_stays_at_bottom() {
    // Full interaction cycle: scroll → type → output → stays at bottom.
    let mut pane = make_pane(80, 5);
    for _ in 0..10 {
        pane.process(b"line\r\n");
    }
    pane.scroll_up(5);

    // User presses Enter (SendToPty calls scroll_bottom first).
    pane.scroll_bottom();

    // Multiple lines of output arrive after the command.
    for _ in 0..5 {
        pane.process(b"output\r\n");
        assert_eq!(
            pane.scroll_offset, 0,
            "view left the bottom after user input"
        );
    }
}

// ── Reflow scroll_offset tests ───────────────────────────────────────────────

#[test]
fn resize_adjusts_scroll_offset_on_widen() {
    // 4-col, 1-row pane: "ABCDEFGH\r\n" autowraps at col 4, pushing ABCD (soft)
    // then the \n pushes EFGH (hard). Together they form the logical line ABCDEFGH.
    // Then "IJKLMNOP\r\n" does the same, adding IJKL(soft) + MNOP(hard).
    // scrollback = 4 rows.  Widen to 8: 4 rows collapse to 2 logical lines.
    let mut pane = make_pane(4, 1);
    pane.process(b"ABCDEFGH\r\nIJKLMNOP\r\n");
    let sb_before = pane.parser.grid.scrollback_len();
    assert!(sb_before >= 2, "expected scrollback rows, got {sb_before}");

    pane.scroll_up(sb_before); // scroll to top
    let offset_before = pane.scroll_offset;

    pane.resize(8, 1, [0, 0, 64, 16]);

    let sb_after = pane.parser.grid.scrollback_len();
    // Scrollback should have shrunk (soft+hard pairs joined into single rows).
    assert!(
        sb_after < sb_before,
        "expected scrollback to shrink on widen ({sb_before} → {sb_after})"
    );
    // scroll_offset must not exceed new scrollback length.
    assert!(
        pane.scroll_offset <= sb_after,
        "scroll_offset {} exceeds new sb_len {}",
        pane.scroll_offset,
        sb_after
    );
    // Offset should have decreased when scrollback shrank.
    assert!(
        pane.scroll_offset < offset_before,
        "expected offset to decrease: before={offset_before}, after={}",
        pane.scroll_offset
    );
}

#[test]
fn resize_adjusts_scroll_offset_on_narrow() {
    // 8-col, 1-row pane: "ABCDEFGH\r\n" fills row 0 then \n pushes it as hard-wrap.
    // Same for "IJKLMNOP\r\n". scrollback = 2 hard-wrapped rows.
    // Narrow to 4: each 8-col row splits into 2 rows of 4, so delta=+2.
    let mut pane = make_pane(8, 1);
    pane.process(b"ABCDEFGH\r\n");
    pane.process(b"IJKLMNOP\r\n");
    let sb_before = pane.parser.grid.scrollback_len();
    assert!(sb_before >= 2, "expected at least 2 scrollback rows");

    pane.scroll_up(1);
    assert_eq!(pane.scroll_offset, 1);

    pane.resize(4, 1, [0, 0, 32, 16]);

    let sb_after = pane.parser.grid.scrollback_len();
    // Scrollback should have grown (each 8-col row splits into 2 rows of 4).
    assert!(
        sb_after > sb_before,
        "expected scrollback to grow on narrow ({sb_before} → {sb_after})"
    );
    // scroll_offset must still be within bounds.
    assert!(
        pane.scroll_offset <= sb_after,
        "scroll_offset {} exceeds new sb_len {}",
        pane.scroll_offset,
        sb_after
    );
    // Offset should have grown to track the added rows.
    assert!(
        pane.scroll_offset > 1,
        "offset should have grown after narrowing (was 1, got {})",
        pane.scroll_offset
    );
}
