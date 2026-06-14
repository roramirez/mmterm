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

// ── next_pane_in_layout ───────────────────────────────────────────────────────

#[test]
fn next_pane_advances_to_next_leaf() {
    assert_eq!(next_pane_in_layout(&[10, 20, 30], 10), 20);
    assert_eq!(next_pane_in_layout(&[10, 20, 30], 20), 30);
}

#[test]
fn next_pane_wraps_from_last_to_first() {
    assert_eq!(next_pane_in_layout(&[10, 20, 30], 30), 10);
}

#[test]
fn next_pane_single_pane_returns_same() {
    assert_eq!(next_pane_in_layout(&[5], 5), 5);
}

#[test]
fn next_pane_active_not_in_leaves_returns_active() {
    assert_eq!(next_pane_in_layout(&[1, 2, 3], 99), 99);
}

// ── needs_quit_confirm ────────────────────────────────────────────────────────

#[test]
fn single_tab_single_pane_no_confirm() {
    assert!(!needs_quit_confirm(1, 1));
}

#[test]
fn multiple_tabs_needs_confirm() {
    assert!(needs_quit_confirm(2, 1));
    assert!(needs_quit_confirm(5, 1));
}

#[test]
fn single_tab_multiple_panes_needs_confirm() {
    assert!(needs_quit_confirm(1, 2));
    assert!(needs_quit_confirm(1, 4));
}

#[test]
fn multiple_tabs_multiple_panes_needs_confirm() {
    assert!(needs_quit_confirm(3, 3));
}

// ── tab_label ─────────────────────────────────────────────────────────────────

#[test]
fn tab_label_defaults_to_1_indexed_number() {
    assert_eq!(tab_label(0, None, None, false, None), " 1 ");
    assert_eq!(tab_label(2, None, None, false, None), " 3 ");
}

#[test]
fn tab_label_uses_user_name() {
    assert_eq!(tab_label(0, Some("shell"), None, false, None), " shell ");
}

#[test]
fn tab_label_uses_osc_title_when_no_user_name() {
    assert_eq!(tab_label(0, None, Some("vim"), false, None), " vim ");
}

#[test]
fn tab_label_user_name_takes_priority_over_osc() {
    assert_eq!(
        tab_label(0, Some("myshell"), Some("vim"), false, None),
        " myshell "
    );
}

#[test]
fn tab_label_rename_buf_shown_when_active() {
    assert_eq!(tab_label(0, None, None, true, Some("new")), " new| ");
}

#[test]
fn tab_label_rename_buf_ignored_when_not_active() {
    // rename_buf only applies to the active tab
    assert_eq!(tab_label(0, None, None, false, Some("new")), " 1 ");
}

// ── should_show_cursor ────────────────────────────────────────────────────────

#[test]
fn cursor_shown_when_active_insert_blink_no_scroll() {
    assert!(should_show_cursor(true, true, true, 0));
}

#[test]
fn cursor_hidden_when_not_active() {
    assert!(!should_show_cursor(false, true, true, 0));
}

#[test]
fn cursor_hidden_when_not_insert_mode() {
    assert!(!should_show_cursor(true, false, true, 0));
}

#[test]
fn cursor_hidden_when_blink_off() {
    assert!(!should_show_cursor(true, true, false, 0));
}

#[test]
fn cursor_hidden_when_scrolled() {
    assert!(!should_show_cursor(true, true, true, 5));
}
