use super::*;

// ── next_tab_index ────────────────────────────────────────────────────────────

#[test]
fn next_wraps_from_last_to_first() {
    assert_eq!(next_tab_index(2, 3), 0);
}

#[test]
fn next_advances_by_one() {
    assert_eq!(next_tab_index(0, 3), 1);
    assert_eq!(next_tab_index(1, 3), 2);
}

#[test]
fn next_with_single_tab_stays() {
    assert_eq!(next_tab_index(0, 1), 0);
}

// ── prev_tab_index ────────────────────────────────────────────────────────────

#[test]
fn prev_wraps_from_first_to_last() {
    assert_eq!(prev_tab_index(0, 3), 2);
}

#[test]
fn prev_decrements_by_one() {
    assert_eq!(prev_tab_index(2, 3), 1);
    assert_eq!(prev_tab_index(1, 3), 0);
}

#[test]
fn prev_with_single_tab_stays() {
    assert_eq!(prev_tab_index(0, 1), 0);
}

// ── close_tab_index ───────────────────────────────────────────────────────────

#[test]
fn close_last_tab_clamps_to_new_last() {
    // Removing index 2 from [0,1,2] → new last is 1.
    assert_eq!(close_tab_index(2, 3), 1);
}

#[test]
fn close_middle_tab_keeps_index() {
    // Removing index 1 from [0,1,2] → active stays at 1 (now pointing to old 2).
    assert_eq!(close_tab_index(1, 3), 1);
}

#[test]
fn close_first_tab_keeps_zero() {
    assert_eq!(close_tab_index(0, 3), 0);
}

#[test]
fn close_only_remaining_pair() {
    // Removing index 1 from [0,1] → clamps to 0.
    assert_eq!(close_tab_index(1, 2), 0);
    // Removing index 0 from [0,1] → stays at 0.
    assert_eq!(close_tab_index(0, 2), 0);
}

// ── move_tab_index ────────────────────────────────────────────────────────────

#[test]
fn move_left_decrements() {
    assert_eq!(move_tab_index(2, 3, true), 1);
}

#[test]
fn move_left_at_first_stays() {
    assert_eq!(move_tab_index(0, 3, true), 0);
}

#[test]
fn move_right_increments() {
    assert_eq!(move_tab_index(0, 3, false), 1);
}

#[test]
fn move_right_at_last_stays() {
    assert_eq!(move_tab_index(2, 3, false), 2);
}

#[test]
fn move_with_single_tab_stays() {
    assert_eq!(move_tab_index(0, 1, true), 0);
    assert_eq!(move_tab_index(0, 1, false), 0);
}
