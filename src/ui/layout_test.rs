use super::*;

const W: u32 = 800;
const H: u32 = 600;

#[test]
fn new_layout_has_single_leaf() {
    let layout = Layout::new(0, W, H);
    assert_eq!(layout.leaves(), vec![0]);
}

#[test]
fn split_h_creates_two_panes() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let leaves = layout.leaves();
    assert_eq!(leaves.len(), 2);
    assert!(leaves.contains(&0));
    assert!(leaves.contains(&1));
}

#[test]
fn split_v_creates_two_panes() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::V);
    assert_eq!(layout.leaves().len(), 2);
}

#[test]
fn split_h_rects_widths_sum_to_total() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let rects = layout.rects();
    let total: u32 = rects.iter().map(|(_, r)| r[2]).sum::<u32>() + SEP;
    assert_eq!(total, W);
}

#[test]
fn split_v_rects_heights_sum_to_usable() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::V);
    let rects = layout.rects();
    let usable = H - STATUS_BAR_H - TAB_BAR_H;
    let total: u32 = rects.iter().map(|(_, r)| r[3]).sum::<u32>() + SEP;
    assert_eq!(total, usable);
}

#[test]
fn single_pane_rect_spans_full_width() {
    let layout = Layout::new(0, W, H);
    let rects = layout.rects();
    assert_eq!(rects.len(), 1);
    assert_eq!(rects[0].1[0], 0);
    assert_eq!(rects[0].1[2], W);
}

#[test]
fn remove_returns_sibling_id() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let sibling = layout.remove(0);
    assert_eq!(sibling, Some(1));
    assert_eq!(layout.leaves(), vec![1]);
}

#[test]
fn remove_last_pane_returns_none() {
    let mut layout = Layout::new(0, W, H);
    let result = layout.remove(0);
    assert_eq!(result, None);
}

#[test]
fn separators_empty_for_single_pane() {
    let layout = Layout::new(0, W, H);
    assert!(layout.separators().is_empty());
}

#[test]
fn separators_one_for_split() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    assert_eq!(layout.separators().len(), 1);
}

#[test]
fn focus_dir_right() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    assert_eq!(layout.focus_dir(0, 1, 0), Some(1));
}

#[test]
fn focus_dir_left() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    assert_eq!(layout.focus_dir(1, -1, 0), Some(0));
}

#[test]
fn focus_dir_no_pane_returns_none() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    assert_eq!(layout.focus_dir(0, -1, 0), None);
}

#[test]
fn resize_updates_dimensions() {
    let mut layout = Layout::new(0, W, H);
    layout.resize(1024, 768);
    assert_eq!(layout.width, 1024);
    assert_eq!(layout.height, 768);
}

#[test]
fn focus_dir_up_and_down() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::V);
    assert_eq!(layout.focus_dir(0, 0, 1), Some(1));
    assert_eq!(layout.focus_dir(1, 0, -1), Some(0));
}

#[test]
fn v_split_separator_is_horizontal() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::V);
    let seps = layout.separators();
    assert_eq!(seps.len(), 1);
    // horizontal separator spans full width
    assert_eq!(seps[0][2], W);
    assert_eq!(seps[0][3], SEP);
}

#[test]
fn remove_second_pane_leaves_first() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let sibling = layout.remove(1);
    assert_eq!(sibling, Some(0));
    assert_eq!(layout.leaves(), vec![0]);
}

#[test]
fn nested_split_has_three_panes() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.split(1, 2, SplitDir::V);
    assert_eq!(layout.leaves().len(), 3);
    assert_eq!(layout.separators().len(), 2);
}

#[test]
fn remove_inner_pane_from_a_subtree() {
    // Build: Split{ Split{Leaf(0), Leaf(2)}, Leaf(1) }
    // Removing pane 2 triggers Replace result on the a subtree (lines 100-101)
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.split(0, 2, SplitDir::V);
    let sibling = layout.remove(2);
    assert_eq!(sibling, Some(0));
    assert!(layout.leaves().contains(&0));
    assert!(!layout.leaves().contains(&2));
}

#[test]
fn remove_inner_pane_from_b_subtree() {
    // Build: Split{ Leaf(0), Split{Leaf(1), Leaf(2)} }
    // Removing pane 2 triggers Replace result on the b subtree (lines 106-107)
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.split(1, 2, SplitDir::V);
    let sibling = layout.remove(2);
    assert_eq!(sibling, Some(0));
    assert!(layout.leaves().contains(&1));
    assert!(!layout.leaves().contains(&2));
}

#[test]
fn remove_nonexistent_pane_leaves_layout_unchanged() {
    // a.remove_leaf → NotFound, b.remove_leaf → NotFound → hits `r => r` catch-all (line 107)
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let _ = layout.remove(99); // pane 99 doesn't exist
    assert_eq!(layout.leaves().len(), 2);
}

#[test]
fn remove_deeply_nested_pane_propagates_done() {
    // 4-pane tree: Split{ Split{Split{Leaf(0),Leaf(3)}, Leaf(2)}, Leaf(1) }
    // Removing pane 3: inner Replace bubbles up as Done to the root match → line 101
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.split(0, 2, SplitDir::V);
    layout.split(0, 3, SplitDir::H);
    let _ = layout.remove(3);
    assert!(!layout.leaves().contains(&3));
    assert!(layout.leaves().contains(&0));
    assert_eq!(layout.leaves().len(), 3);
}

#[test]
fn focus_dir_updates_best_when_closer_candidate_found() {
    // Build: Split{ Split{Leaf(0), Leaf(2)}, Leaf(1) } (V splits)
    // Pane 0 = top quarter, pane 2 = middle quarter, pane 1 = bottom half
    // From pane 1 looking up: first finds pane 0, then finds pane 2 (closer) → line 197
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::V);
    layout.split(0, 2, SplitDir::V);
    let result = layout.focus_dir(1, 0, -1);
    assert_eq!(result, Some(2)); // pane 2 is closer than pane 0
}

#[test]
fn rects_start_at_tab_bar_height() {
    let layout = Layout::new(0, W, H);
    let rects = layout.rects();
    assert_eq!(rects[0].1[1], TAB_BAR_H);
}

#[test]
fn four_pane_layout_has_four_rects() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.split(0, 2, SplitDir::V);
    layout.split(1, 3, SplitDir::V);
    assert_eq!(layout.leaves().len(), 4);
    assert_eq!(layout.rects().len(), 4);
    assert_eq!(layout.separators().len(), 3);
}

#[test]
fn h_split_separator_is_vertical() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let seps = layout.separators();
    assert_eq!(seps.len(), 1);
    // vertical separator spans full usable height, width is SEP
    assert_eq!(seps[0][2], SEP);
    assert_eq!(seps[0][3], H - STATUS_BAR_H - TAB_BAR_H);
}

#[test]
fn split_then_remove_restores_single_pane() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.remove(1);
    assert_eq!(layout.leaves(), vec![0]);
    let rects = layout.rects();
    assert_eq!(rects.len(), 1);
    assert_eq!(rects[0].1[2], W);
}

#[test]
fn pane_rects_cover_full_usable_area_in_h_split() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let rects = layout.rects();
    // Both panes start at same y (TAB_BAR_H) and have same height
    assert_eq!(rects[0].1[1], rects[1].1[1]);
    assert_eq!(rects[0].1[3], rects[1].1[3]);
    // Widths + separator = total width
    let total: u32 = rects.iter().map(|(_, r)| r[2]).sum::<u32>() + SEP;
    assert_eq!(total, W);
}

// ── separator_at_pixel ────────────────────────────────────────────────────────

#[test]
fn separator_at_pixel_h_split_hit() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    // Separator is at x = W/2 (ratio 0.5), spanning full usable height
    let sep_x = W / 2;
    let mid_y = TAB_BAR_H + (H - STATUS_BAR_H - TAB_BAR_H) / 2;
    let handle = layout.separator_at_pixel(sep_x, mid_y, 4);
    assert!(handle.is_some());
    assert!(matches!(handle.unwrap().dir, SplitDir::H));
}

#[test]
fn separator_at_pixel_v_split_hit() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::V);
    // Separator is at y = TAB_BAR_H + usable_h/2
    let usable_h = H - STATUS_BAR_H - TAB_BAR_H;
    let sep_y = TAB_BAR_H + usable_h / 2;
    let handle = layout.separator_at_pixel(W / 2, sep_y, 4);
    assert!(handle.is_some());
    assert!(matches!(handle.unwrap().dir, SplitDir::V));
}

#[test]
fn separator_at_pixel_miss() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    // Far from separator
    assert!(layout.separator_at_pixel(10, TAB_BAR_H + 10, 4).is_none());
}

#[test]
fn separator_at_pixel_single_pane_none() {
    let layout = Layout::new(0, W, H);
    assert!(layout.separator_at_pixel(W / 2, H / 2, 4).is_none());
}

// ── move_separator ────────────────────────────────────────────────────────────

#[test]
fn move_separator_h_changes_rects() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let sep_x = W / 2;
    let mid_y = TAB_BAR_H + (H - STATUS_BAR_H - TAB_BAR_H) / 2;
    let handle = layout.separator_at_pixel(sep_x, mid_y, 4).unwrap();
    // Move separator to ~25% of width
    layout.move_separator(handle, W / 4);
    let rects = layout.rects();
    let left_w = rects.iter().find(|(id, _)| *id == 0).unwrap().1[2];
    // Left pane should now be around 25% wide (within 10px tolerance)
    assert!((left_w as i32 - (W / 4) as i32).abs() < 10);
}

#[test]
fn move_separator_v_changes_rects() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::V);
    let usable_h = H - STATUS_BAR_H - TAB_BAR_H;
    let sep_y = TAB_BAR_H + usable_h / 2;
    let handle = layout.separator_at_pixel(W / 2, sep_y, 4).unwrap();
    // Move separator to ~25% of usable height
    layout.move_separator(handle, TAB_BAR_H + usable_h / 4);
    let rects = layout.rects();
    let top_h = rects.iter().find(|(id, _)| *id == 0).unwrap().1[3];
    assert!((top_h as i32 - (usable_h / 4) as i32).abs() < 10);
}

#[test]
fn move_separator_clamps_minimum() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let sep_x = W / 2;
    let mid_y = TAB_BAR_H + (H - STATUS_BAR_H - TAB_BAR_H) / 2;
    let handle = layout.separator_at_pixel(sep_x, mid_y, 4).unwrap();
    // Move to far left (below 10% minimum)
    layout.move_separator(handle, 0);
    let rects = layout.rects();
    let left_w = rects.iter().find(|(id, _)| *id == 0).unwrap().1[2];
    // Must be at least 10% of W
    assert!(left_w >= W / 10);
}

// ── nudge_pane ───────────────────────────────────────────────────────────────

#[test]
fn nudge_pane_h_right_grows_active() {
    // Ctrl+Shift+Right: active pane grows horizontally regardless of position
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    // pane 0 is in 'a' (left side)
    let before = layout.rects().iter().find(|(id, _)| *id == 0).unwrap().1[2];
    layout.nudge_pane(0, true, 0.05);
    let after = layout.rects().iter().find(|(id, _)| *id == 0).unwrap().1[2];
    assert!(after > before);
}

#[test]
fn nudge_pane_h_right_separator_moves_right_for_b_side() {
    // pane 1 is in 'b' (right side) — Right moves separator right, so pane 1 shrinks
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let before = layout.rects().iter().find(|(id, _)| *id == 1).unwrap().1[2];
    layout.nudge_pane(1, true, 0.05);
    let after = layout.rects().iter().find(|(id, _)| *id == 1).unwrap().1[2];
    assert!(after < before); // separator moved right → b side shrinks
}

#[test]
fn nudge_pane_h_left_shrinks_active() {
    // Ctrl+Shift+Left: active pane shrinks horizontally
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let before = layout.rects().iter().find(|(id, _)| *id == 0).unwrap().1[2];
    layout.nudge_pane(0, true, -0.05);
    let after = layout.rects().iter().find(|(id, _)| *id == 0).unwrap().1[2];
    assert!(after < before);
}

#[test]
fn nudge_pane_clamps_at_max() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    for _ in 0..20 {
        layout.nudge_pane(0, true, 0.05);
    }
    let rects = layout.rects();
    let left_w = rects.iter().find(|(id, _)| *id == 0).unwrap().1[2];
    // Must not exceed 90% of W
    assert!(left_w <= (W as f32 * 0.91) as u32);
}

#[test]
fn nudge_pane_nested_innermost_v() {
    // Layout: H{ V{ Leaf(0), Leaf(2) }, Leaf(1) }
    // Ctrl+Shift+Down on pane 0 should affect the inner V split (pane 0 is in 'a')
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.split(0, 2, SplitDir::V);
    let before_0 = layout.rects().iter().find(|(id, _)| *id == 0).unwrap().1[3];
    layout.nudge_pane(0, false, 0.05);
    let after_0 = layout.rects().iter().find(|(id, _)| *id == 0).unwrap().1[3];
    assert!(after_0 > before_0);
}
