use super::*;
use crate::terminal::grid::Color;

fn make_pane(cols: usize, rows: usize) -> Pane {
    Pane::new_with_colors(
        cols,
        rows,
        [0, 0, cols as u32 * 8, rows as u32 * 16],
        Color::WHITE,
        Color::BLACK,
        Color::CURSOR,
        Color::SELECTION,
        [Color::BLACK; 16],
    )
}

#[test]
fn new_pane_scroll_offset_is_zero() {
    let pane = make_pane(80, 24);
    assert_eq!(pane.scroll_offset, 0);
}

#[test]
fn process_resets_scroll_offset() {
    let mut pane = make_pane(80, 24);
    // fill scrollback by writing enough lines
    for _ in 0..30 {
        pane.process(b"line\r\n");
    }
    pane.scroll_up(5);
    assert!(pane.scroll_offset > 0);
    pane.process(b"new output");
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
