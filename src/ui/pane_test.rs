use std::sync::{Arc, RwLock};

use super::*;
use crate::terminal::TerminalParser;
use crate::terminal::grid::{Color, Grid, GridColors};

/// Test helper: pairs a Pane with a TerminalParser, simulating the
/// ParseEffect::ScrollbackDelta path that drain_effects handles at runtime.
struct TestPane {
    pub pane: Pane,
    parser: TerminalParser,
}

impl TestPane {
    fn new(cols: usize, rows: usize) -> Self {
        let grid = Arc::new(RwLock::new(Grid::with_colors(
            cols,
            rows,
            GridColors {
                fg: Color::WHITE,
                bg: Color::BLACK,
                cursor: Color::CURSOR,
                selection: Color::SELECTION,
                palette: [Color::BLACK; 16],
            },
            10_000,
        )));
        let pane = Pane::new(grid, [0, 0, cols as u32 * 8, rows as u32 * 16]);
        TestPane {
            pane,
            parser: TerminalParser::new(),
        }
    }

    /// Parse bytes and apply the scroll_offset adjustment that drain_effects does.
    fn process(&mut self, bytes: &[u8]) {
        let old_sb = self.pane.grid.read().unwrap().scrollback_len();
        {
            let mut g = self.pane.grid.write().unwrap();
            self.parser.process(bytes, &mut g);
        }
        let new_sb = self.pane.grid.read().unwrap().scrollback_len();
        if new_sb != old_sb && self.pane.scroll_offset > 0 {
            let added = new_sb.saturating_sub(old_sb);
            self.pane.scroll_offset = (self.pane.scroll_offset + added).min(new_sb);
        }
    }
}

#[test]
fn new_pane_scroll_offset_is_zero() {
    let tp = TestPane::new(80, 24);
    assert_eq!(tp.pane.scroll_offset, 0);
}

#[test]
fn process_pins_view_when_scrolled_up() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_up(3);
    let sb_before = tp.pane.grid.read().unwrap().scrollback_len();
    let offset_before = tp.pane.scroll_offset;

    tp.process(b"new\r\n");
    let sb_after = tp.pane.grid.read().unwrap().scrollback_len();

    assert_eq!(
        tp.pane.scroll_offset,
        offset_before + (sb_after - sb_before)
    );
}

#[test]
fn process_stays_at_bottom_when_not_scrolled() {
    let mut tp = TestPane::new(80, 24);
    for _ in 0..30 {
        tp.process(b"line\r\n");
    }
    assert_eq!(tp.pane.scroll_offset, 0);
    tp.process(b"new output");
    assert_eq!(tp.pane.scroll_offset, 0);
}

#[test]
fn process_clamps_offset_when_scrollback_shrinks() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_top();
    assert!(tp.pane.scroll_offset > 0);
    // Enter alternate screen — scrollback is now empty
    tp.process(b"\x1b[?1049h");
    assert_eq!(tp.pane.scroll_offset, 0);
}

#[test]
fn scroll_up_clamps_to_scrollback_len() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..3 {
        tp.process(b"line\r\n");
    }
    let max = tp.pane.grid.read().unwrap().scrollback_len();
    tp.pane.scroll_up(max + 100);
    assert_eq!(tp.pane.scroll_offset, max);
}

#[test]
fn scroll_down_clamps_at_zero() {
    let mut tp = TestPane::new(80, 5);
    tp.pane.scroll_down(10);
    assert_eq!(tp.pane.scroll_offset, 0);
}

#[test]
fn scroll_top_sets_max_offset() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_top();
    assert_eq!(
        tp.pane.scroll_offset,
        tp.pane.grid.read().unwrap().scrollback_len()
    );
}

#[test]
fn scroll_bottom_resets_to_zero() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_top();
    tp.pane.scroll_bottom();
    assert_eq!(tp.pane.scroll_offset, 0);
}

#[test]
fn resize_updates_grid_and_rect() {
    let mut tp = TestPane::new(80, 24);
    tp.pane.resize(40, 12, [0, 0, 320, 192]);
    let g = tp.pane.grid.read().unwrap();
    assert_eq!(g.cols, 40);
    assert_eq!(g.rows, 12);
    assert_eq!(tp.pane.rect, [0, 0, 320, 192]);
}

#[test]
fn scroll_up_increments_offset_by_n() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_up(3);
    assert_eq!(tp.pane.scroll_offset, 3);
}

#[test]
fn scroll_down_decrements_offset_by_n() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_up(6);
    tp.pane.scroll_down(2);
    assert_eq!(tp.pane.scroll_offset, 4);
}

#[test]
fn scroll_up_then_down_round_trips() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_up(5);
    tp.pane.scroll_down(5);
    assert_eq!(tp.pane.scroll_offset, 0);
}

#[test]
fn scroll_top_then_scroll_bottom_resets() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..15 {
        tp.process(b"line\r\n");
    }
    let max = tp.pane.grid.read().unwrap().scrollback_len();
    tp.pane.scroll_top();
    assert_eq!(tp.pane.scroll_offset, max);
    tp.pane.scroll_bottom();
    assert_eq!(tp.pane.scroll_offset, 0);
}

#[test]
fn scenario_scroll_up_then_user_input_snaps_to_bottom() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_up(4);
    assert!(tp.pane.scroll_offset > 0);
    tp.pane.scroll_bottom();
    assert_eq!(tp.pane.scroll_offset, 0);
    tp.process(b"result\r\n");
    assert_eq!(tp.pane.scroll_offset, 0);
}

#[test]
fn scenario_view_stays_pinned_across_multiple_output_batches() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_up(3);

    for _ in 0..5 {
        let sb_before = tp.pane.grid.read().unwrap().scrollback_len();
        let offset_before = tp.pane.scroll_offset;
        tp.process(b"more\r\n");
        let added = tp
            .pane
            .grid
            .read()
            .unwrap()
            .scrollback_len()
            .saturating_sub(sb_before);
        assert_eq!(
            tp.pane.scroll_offset,
            offset_before + added,
            "view drifted after a burst of output"
        );
    }
}

#[test]
fn scenario_scroll_to_top_clamps_when_scrollback_saturates() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_top();

    for _ in 0..20 {
        tp.process(b"flood\r\n");
        let max = tp.pane.grid.read().unwrap().scrollback_len();
        assert!(
            tp.pane.scroll_offset <= max,
            "scroll_offset exceeded scrollback_len"
        );
    }
}

#[test]
fn scenario_scroll_up_user_input_then_more_output_stays_at_bottom() {
    let mut tp = TestPane::new(80, 5);
    for _ in 0..10 {
        tp.process(b"line\r\n");
    }
    tp.pane.scroll_up(5);
    tp.pane.scroll_bottom();
    for _ in 0..5 {
        tp.process(b"output\r\n");
        assert_eq!(
            tp.pane.scroll_offset, 0,
            "view left the bottom after user input"
        );
    }
}

#[test]
fn resize_adjusts_scroll_offset_on_widen() {
    let mut tp = TestPane::new(4, 1);
    tp.process(b"ABCDEFGH\r\nIJKLMNOP\r\n");
    let sb_before = tp.pane.grid.read().unwrap().scrollback_len();
    assert!(sb_before >= 2, "expected scrollback rows, got {sb_before}");

    tp.pane.scroll_up(sb_before);
    let offset_before = tp.pane.scroll_offset;

    tp.pane.resize(8, 1, [0, 0, 64, 16]);

    let sb_after = tp.pane.grid.read().unwrap().scrollback_len();
    assert!(
        sb_after < sb_before,
        "expected scrollback to shrink on widen ({sb_before} → {sb_after})"
    );
    assert!(
        tp.pane.scroll_offset <= sb_after,
        "scroll_offset {} exceeds new sb_len {}",
        tp.pane.scroll_offset,
        sb_after
    );
    assert!(
        tp.pane.scroll_offset < offset_before,
        "expected offset to decrease: before={offset_before}, after={}",
        tp.pane.scroll_offset
    );
}

#[test]
fn resize_adjusts_scroll_offset_on_narrow() {
    let mut tp = TestPane::new(8, 1);
    tp.process(b"ABCDEFGH\r\n");
    tp.process(b"IJKLMNOP\r\n");
    let sb_before = tp.pane.grid.read().unwrap().scrollback_len();
    assert!(sb_before >= 2, "expected at least 2 scrollback rows");

    tp.pane.scroll_up(1);
    assert_eq!(tp.pane.scroll_offset, 1);

    tp.pane.resize(4, 1, [0, 0, 32, 16]);

    let sb_after = tp.pane.grid.read().unwrap().scrollback_len();
    assert!(
        sb_after > sb_before,
        "expected scrollback to grow on narrow ({sb_before} → {sb_after})"
    );
    assert!(
        tp.pane.scroll_offset <= sb_after,
        "scroll_offset {} exceeds new sb_len {}",
        tp.pane.scroll_offset,
        sb_after
    );
    assert!(
        tp.pane.scroll_offset > 1,
        "offset should have grown after narrowing (was 1, got {})",
        tp.pane.scroll_offset
    );
}
