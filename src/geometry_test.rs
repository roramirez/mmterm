use super::*;

#[test]
fn pane_at_pixel_hit_first_pane() {
    let rects = vec![(1, [0, 0, 100, 50]), (2, [100, 0, 100, 50])];
    assert_eq!(pane_at_pixel(&rects, 50.0, 25.0), Some(1));
}

#[test]
fn pane_at_pixel_hit_second_pane() {
    let rects = vec![(1, [0, 0, 100, 50]), (2, [100, 0, 100, 50])];
    assert_eq!(pane_at_pixel(&rects, 150.0, 25.0), Some(2));
}

#[test]
fn pane_at_pixel_miss_returns_none() {
    let rects = vec![(1, [0, 0, 100, 50])];
    assert_eq!(pane_at_pixel(&rects, 200.0, 25.0), None);
}

#[test]
fn pane_at_pixel_right_edge_is_exclusive() {
    let rects = vec![(1, [0, 0, 100, 50])];
    assert_eq!(pane_at_pixel(&rects, 100.0, 25.0), None);
}

#[test]
fn pane_at_pixel_bottom_edge_is_exclusive() {
    let rects = vec![(1, [0, 0, 100, 50])];
    assert_eq!(pane_at_pixel(&rects, 50.0, 50.0), None);
}

#[test]
fn pane_at_pixel_top_left_corner_hits() {
    let rects = vec![(1, [10, 20, 100, 50])];
    assert_eq!(pane_at_pixel(&rects, 10.0, 20.0), Some(1));
}

#[test]
fn pane_at_pixel_empty_list_returns_none() {
    assert_eq!(pane_at_pixel(&[], 50.0, 25.0), None);
}

#[test]
fn pixel_to_cell_basic() {
    // rect [0,0,100,60], cell 10×12, grid 10×5 — pixel (15, 13) → col 1, row 1
    let result = pixel_to_cell([0, 0, 100, 60], 10, 12, 10, 5, 15.0, 13.0);
    assert_eq!(result, Some((1, 1)));
}

#[test]
fn pixel_to_cell_origin() {
    let result = pixel_to_cell([0, 0, 100, 60], 10, 12, 10, 5, 0.0, 0.0);
    assert_eq!(result, Some((0, 0)));
}

#[test]
fn pixel_to_cell_out_of_rect_left() {
    assert_eq!(
        pixel_to_cell([50, 50, 100, 60], 10, 12, 10, 5, 10.0, 60.0),
        None
    );
}

#[test]
fn pixel_to_cell_out_of_rect_above() {
    assert_eq!(
        pixel_to_cell([50, 50, 100, 60], 10, 12, 10, 5, 60.0, 10.0),
        None
    );
}

#[test]
fn pixel_to_cell_clamps_col_to_last() {
    // pixel at x=99 in a 100-wide rect with cell_w=10 → col=9 (clamped)
    let (col, _) = pixel_to_cell([0, 0, 100, 60], 10, 12, 10, 5, 99.0, 0.0).unwrap();
    assert_eq!(col, 9);
}

#[test]
fn pixel_to_cell_clamps_row_to_last() {
    let (_, row) = pixel_to_cell([0, 0, 100, 60], 10, 12, 10, 5, 0.0, 59.0).unwrap();
    assert_eq!(row, 4);
}

#[test]
fn pixel_to_cell_offset_rect() {
    // rect starts at [100, 50]; pixel (110, 62) → col 1, row 1
    let result = pixel_to_cell([100, 50, 200, 120], 10, 12, 20, 10, 110.0, 62.0);
    assert_eq!(result, Some((1, 1)));
}

// ── cell_url_at_scroll ────────────────────────────────────────────────────────

use crate::terminal::grid::{Color, Grid, GridColors};
use std::sync::Arc;

fn make_grid(cols: usize, rows: usize) -> Grid {
    Grid::with_colors(
        cols,
        rows,
        GridColors {
            fg: Color::WHITE,
            bg: Color::BLACK,
            cursor: Color::CURSOR,
            selection: Color::SELECTION,
            palette: [Color::BLACK; 16],
        },
        1000,
    )
}

#[test]
fn cell_url_live_grid_no_scroll() {
    let mut g = make_grid(10, 3);
    g.write_char('L');
    g.cell_mut(0, 0).url = Some(Arc::new("https://example.com".to_string()));
    let url = cell_url_at_scroll(&g, 0, 0, 0).unwrap();
    assert_eq!(url.as_ref(), "https://example.com");
}

#[test]
fn cell_without_url_returns_none() {
    let mut g = make_grid(10, 3);
    g.write_char('X');
    assert!(cell_url_at_scroll(&g, 0, 0, 0).is_none());
}

#[test]
fn col_out_of_bounds_in_scrollback_returns_none() {
    let mut g = make_grid(5, 2);
    // push a short line into scrollback
    for _ in 0..10 {
        g.write_char(' ');
    }
    if !g.scrollback.is_empty() {
        // col well beyond scrollback line length
        assert!(cell_url_at_scroll(&g, 1, 999, 0).is_none());
    }
}

#[test]
fn row_out_of_bounds_with_scroll_returns_none() {
    let g = make_grid(10, 3);
    // scroll_offset=1 but no scrollback → sb_row falls past everything
    assert!(cell_url_at_scroll(&g, 1, 0, 999).is_none());
}
