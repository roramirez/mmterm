use super::extend_visual_mode;
use crate::input::InputMode;

// ── Shift+Click extend-selection decision ────────────────────────────────────

#[test]
fn extend_from_anchored_visual_keeps_anchor_moves_cursor() {
    let current = InputMode::Visual {
        start_col: 3,
        start_row: 1,
        cur_col: 5,
        cur_row: 2,
        anchored: true,
    };
    let extended = extend_visual_mode(&current, 10, 7).expect("extends an existing selection");
    assert_eq!(
        extended,
        InputMode::Visual {
            start_col: 3,
            start_row: 1,
            cur_col: 10,
            cur_row: 7,
            anchored: true,
        },
        "anchor (start_*) is preserved and cursor (cur_*) jumps to the clicked cell"
    );
}

#[test]
fn extend_from_unanchored_visual_still_grows_from_anchor() {
    // Even a not-yet-anchored Visual selection carries a start; shift-click
    // anchors it and extends the cursor to the click point.
    let current = InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 4,
        cur_row: 0,
        anchored: false,
    };
    let extended = extend_visual_mode(&current, 8, 3).expect("extends the visual selection");
    assert_eq!(
        extended,
        InputMode::Visual {
            start_col: 0,
            start_row: 0,
            cur_col: 8,
            cur_row: 3,
            anchored: true,
        }
    );
}

#[test]
fn extend_with_no_selection_returns_none_for_fresh_start() {
    // No prior Visual selection → caller should fall back to a fresh selection.
    assert!(extend_visual_mode(&InputMode::Insert, 2, 2).is_none());
    assert!(extend_visual_mode(&InputMode::Normal, 2, 2).is_none());
}
