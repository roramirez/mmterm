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

// ── Session round-trip ────────────────────────────────────────────────────────

#[test]
fn to_saved_node_single_pane_returns_leaf_slot_0() {
    let layout = Layout::new(42, W, H);
    let (node, id_order) = layout.to_saved_node();
    assert_eq!(id_order, vec![42]);
    assert!(matches!(node, crate::session::SavedNode::Leaf { slot: 0 }));
}

#[test]
fn to_saved_node_h_split_dfs_order() {
    // After split(0, 1, H): tree = Split(H, Leaf(0), Leaf(1))
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let (node, id_order) = layout.to_saved_node();
    assert_eq!(id_order, vec![0, 1]);
    let crate::session::SavedNode::Split { a, b, .. } = node else {
        panic!("expected Split");
    };
    assert!(matches!(
        a.as_ref(),
        crate::session::SavedNode::Leaf { slot: 0 }
    ));
    assert!(matches!(
        b.as_ref(),
        crate::session::SavedNode::Leaf { slot: 1 }
    ));
}

#[test]
fn roundtrip_single_pane_preserves_rects() {
    let layout = Layout::new(7, W, H);
    let rects_before = layout.rects();
    let (node, id_order) = layout.to_saved_node();
    // Map slot 0 → pane ID 99 (simulating a restored session with a new ID)
    let slot_to_id = vec![99usize];
    let _ = id_order; // id_order has [7]; we use a new id
    let restored = Layout::from_saved_node(&node, &slot_to_id, W, H);
    let rects_after = restored.rects();
    assert_eq!(rects_before.len(), rects_after.len());
    // Rects should have same geometry, only pane IDs differ
    assert_eq!(rects_before[0].1, rects_after[0].1);
    assert_eq!(rects_after[0].0, 99);
}

#[test]
fn roundtrip_h_split_preserves_rects_and_ids() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let rects_before: std::collections::HashMap<_, _> = layout.rects().into_iter().collect();

    let (node, _id_order) = layout.to_saved_node();
    // Restore with new IDs: slot 0 → 10, slot 1 → 11
    let slot_to_id = vec![10usize, 11];
    let restored = Layout::from_saved_node(&node, &slot_to_id, W, H);
    let rects_after: std::collections::HashMap<_, _> = restored.rects().into_iter().collect();

    assert_eq!(rects_after[&10], rects_before[&0]);
    assert_eq!(rects_after[&11], rects_before[&1]);
}

#[test]
fn roundtrip_three_pane_nested() {
    // Layout: H{ V{ Leaf(0), Leaf(2) }, Leaf(1) }
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.split(0, 2, SplitDir::V);
    let rects_before: std::collections::HashMap<_, _> = layout.rects().into_iter().collect();

    let (node, id_order) = layout.to_saved_node();
    assert_eq!(id_order.len(), 3);

    // Restore with shifted IDs: slot i → i + 20
    let slot_to_id: Vec<usize> = (0..id_order.len()).map(|i| i + 20).collect();
    let restored = Layout::from_saved_node(&node, &slot_to_id, W, H);
    let rects_after: std::collections::HashMap<_, _> = restored.rects().into_iter().collect();

    for (old_id, new_id) in id_order.iter().zip(slot_to_id.iter()) {
        assert_eq!(
            rects_before[old_id], rects_after[new_id],
            "rect mismatch for pane originally {old_id} → {new_id}"
        );
    }
}

// ── *_scaled / usable_h_for ──────────────────────────────────────────────────

#[test]
fn usable_h_1x() {
    let l = Layout::new(0, W, H);
    assert_eq!(
        l.usable_h_for(TAB_BAR_H, STATUS_BAR_H),
        H - TAB_BAR_H - STATUS_BAR_H
    );
}

#[test]
fn usable_h_2x() {
    let l = Layout::new(0, W, H);
    assert_eq!(l.usable_h_for(44, 44), H - 88);
}

#[test]
fn rects_1x_pane_top_at_22() {
    let l = Layout::new(0, W, H);
    let rects = l.rects_scaled(TAB_BAR_H, STATUS_BAR_H);
    assert_eq!(rects[0].1[1], 22);
}

#[test]
fn rects_2x_pane_top_at_44() {
    let l = Layout::new(0, W, H);
    let rects = l.rects_scaled(44, 44);
    assert_eq!(rects[0].1[1], 44);
}

#[test]
fn pixel_in_tab_bar_not_in_any_pane_at_2x() {
    // At 2× scale the physical tab-bar height is 44 px.
    // A physical y=30 is inside the tab bar and must NOT land in any pane rect.
    // A physical y=50 is below the tab bar and MUST land in a pane rect.
    let l = Layout::new(0, W, H);
    let rects = l.rects_scaled(44, 44); // 2× chrome heights
    // rect tuple: (pane_id, [x, y, w, h]) — [1]=y, [3]=h
    let hit_at_30 = rects.iter().any(|(_, r)| 30u32 >= r[1] && 30 < r[1] + r[3]);
    assert!(
        !hit_at_30,
        "y=30 is inside the 44px tab bar and must not hit any pane"
    );
    let hit_at_50 = rects.iter().any(|(_, r)| 50u32 >= r[1] && 50 < r[1] + r[3]);
    assert!(
        hit_at_50,
        "y=50 is below the 44px tab bar and must hit a pane"
    );
}

// ── best_dir_candidate ───────────────────────────────────────────────────────

#[test]
fn best_dir_candidate_picks_closer_candidate() {
    // rect at x=100, width=50 → cx=125; from_cx=0, dx=1 (rightward)
    let rect_far: [u32; 4] = [200, 0, 50, 50]; // cx=225
    let rect_near: [u32; 4] = [100, 0, 50, 50]; // cx=125
    let from_cx = 0_i32;
    let from_cy = 25_i32;

    let best = best_dir_candidate(1, &rect_far, from_cx, from_cy, 1, 0, None);
    let best = best_dir_candidate(2, &rect_near, from_cx, from_cy, 1, 0, best);
    assert_eq!(best.map(|(id, _)| id), Some(2));
}

#[test]
fn best_dir_candidate_rejects_wrong_direction() {
    // rect to the left (cx < from_cx), but dx=1 (rightward)
    let rect: [u32; 4] = [0, 0, 50, 50]; // cx=25
    let from_cx = 100_i32;
    let from_cy = 25_i32;
    let best = best_dir_candidate(1, &rect, from_cx, from_cy, 1, 0, None);
    assert!(best.is_none());
}

#[test]
fn best_dir_candidate_returns_current_when_farther() {
    let rect_far: [u32; 4] = [200, 0, 50, 50]; // cx=225
    let from_cx = 0_i32;
    let from_cy = 25_i32;
    let existing = Some((99_usize, 50_i32));
    let best = best_dir_candidate(1, &rect_far, from_cx, from_cy, 1, 0, existing);
    assert_eq!(best.map(|(id, _)| id), Some(99));
}

// ── rotate_leaves ─────────────────────────────────────────────────────────────

#[test]
fn rotate_forward_single_pane_is_noop() {
    let mut layout = Layout::new(0, W, H);
    layout.rotate_leaves(true);
    assert_eq!(layout.leaves(), vec![0]);
}

#[test]
fn rotate_backward_single_pane_is_noop() {
    let mut layout = Layout::new(0, W, H);
    layout.rotate_leaves(false);
    assert_eq!(layout.leaves(), vec![0]);
}

#[test]
fn rotate_forward_two_panes_swaps() {
    // Split(H, Leaf(0), Leaf(1)) — DFS order [0, 1]
    // forward → rotate_right(1) → [1, 0]
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.rotate_leaves(true);
    assert_eq!(layout.leaves(), vec![1, 0]);
}

#[test]
fn rotate_backward_two_panes_swaps() {
    // forward and backward are symmetric for 2 panes
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.rotate_leaves(false);
    assert_eq!(layout.leaves(), vec![1, 0]);
}

#[test]
fn rotate_forward_three_panes() {
    // Split(H, Leaf(0), Split(V, Leaf(1), Leaf(2))) — DFS order [0, 1, 2]
    // forward → rotate_right(1) → [2, 0, 1]
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.split(1, 2, SplitDir::V);
    layout.rotate_leaves(true);
    assert_eq!(layout.leaves(), vec![2, 0, 1]);
}

#[test]
fn rotate_backward_three_panes() {
    // DFS order [0, 1, 2]
    // backward → rotate_left(1) → [1, 2, 0]
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.split(1, 2, SplitDir::V);
    layout.rotate_leaves(false);
    assert_eq!(layout.leaves(), vec![1, 2, 0]);
}

#[test]
fn rotate_forward_twice_returns_to_original_for_two_panes() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.rotate_leaves(true);
    layout.rotate_leaves(true);
    assert_eq!(layout.leaves(), vec![0, 1]);
}

#[test]
fn rotate_forward_then_backward_is_identity() {
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    layout.split(1, 2, SplitDir::V);
    let before = layout.leaves();
    layout.rotate_leaves(true);
    layout.rotate_leaves(false);
    assert_eq!(layout.leaves(), before);
}

#[test]
fn rotate_preserves_geometry() {
    // Rotating IDs must not change the rect geometry (only which ID occupies which slot)
    let mut layout = Layout::new(0, W, H);
    layout.split(0, 1, SplitDir::H);
    let rects_before: Vec<[u32; 4]> = layout.rects().into_iter().map(|(_, r)| r).collect();
    layout.rotate_leaves(true);
    let rects_after: Vec<[u32; 4]> = layout.rects().into_iter().map(|(_, r)| r).collect();
    assert_eq!(rects_before, rects_after);
}
