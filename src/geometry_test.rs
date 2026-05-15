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
