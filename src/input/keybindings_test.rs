use winit::keyboard::{Key, NamedKey, SmolStr};

use super::*;
use crate::input::mode::InputMode;

fn char_key(s: &str) -> Key {
    Key::Character(SmolStr::new(s))
}

fn named(k: NamedKey) -> Key {
    Key::Named(k)
}

// ── ctrl_w_action ────────────────────────────────────────────────────────────

#[test]
fn ctrl_w_v_splits_horizontal() {
    assert!(matches!(ctrl_w_action(&char_key("v")), Action::SplitH));
}

#[test]
fn ctrl_w_s_splits_vertical() {
    assert!(matches!(ctrl_w_action(&char_key("s")), Action::SplitV));
}

#[test]
fn ctrl_w_a_auto_splits() {
    assert!(matches!(ctrl_w_action(&char_key("a")), Action::AutoSplit));
}

#[test]
fn ctrl_w_h_focuses_left() {
    assert!(matches!(ctrl_w_action(&char_key("h")), Action::FocusLeft));
}

#[test]
fn ctrl_w_l_focuses_right() {
    assert!(matches!(ctrl_w_action(&char_key("l")), Action::FocusRight));
}

#[test]
fn ctrl_w_k_focuses_up() {
    assert!(matches!(ctrl_w_action(&char_key("k")), Action::FocusUp));
}

#[test]
fn ctrl_w_j_focuses_down() {
    assert!(matches!(ctrl_w_action(&char_key("j")), Action::FocusDown));
}

#[test]
fn ctrl_w_w_focuses_next() {
    assert!(matches!(ctrl_w_action(&char_key("w")), Action::FocusNext));
}

#[test]
fn ctrl_w_q_closes_pane() {
    assert!(matches!(ctrl_w_action(&char_key("q")), Action::ClosePane));
}

#[test]
fn ctrl_w_z_zooms_pane() {
    assert!(matches!(ctrl_w_action(&char_key("z")), Action::ZoomPane));
}

#[test]
fn ctrl_w_r_rotates_forward() {
    assert!(matches!(
        ctrl_w_action(&char_key("r")),
        Action::RotatePanesForward
    ));
}

#[test]
fn ctrl_w_uppercase_r_rotates_backward() {
    assert!(matches!(
        ctrl_w_action(&char_key("R")),
        Action::RotatePanesBackward
    ));
}

#[test]
fn ctrl_w_uppercase_v_splits_horizontal() {
    assert!(matches!(ctrl_w_action(&char_key("V")), Action::SplitH));
}

#[test]
fn ctrl_w_arrow_left_focuses_left() {
    assert!(matches!(
        ctrl_w_action(&named(NamedKey::ArrowLeft)),
        Action::FocusLeft
    ));
}

#[test]
fn ctrl_w_arrow_right_focuses_right() {
    assert!(matches!(
        ctrl_w_action(&named(NamedKey::ArrowRight)),
        Action::FocusRight
    ));
}

#[test]
fn ctrl_w_arrow_up_focuses_up() {
    assert!(matches!(
        ctrl_w_action(&named(NamedKey::ArrowUp)),
        Action::FocusUp
    ));
}

#[test]
fn ctrl_w_arrow_down_focuses_down() {
    assert!(matches!(
        ctrl_w_action(&named(NamedKey::ArrowDown)),
        Action::FocusDown
    ));
}

#[test]
fn ctrl_w_unknown_key_returns_none() {
    assert!(matches!(ctrl_w_action(&char_key("x")), Action::None));
}

#[test]
fn ctrl_w_named_escape_returns_none() {
    assert!(matches!(
        ctrl_w_action(&named(NamedKey::Escape)),
        Action::None
    ));
}

// ── handle_key_inner — global shortcuts ─────────────────────────────────────

fn insert() -> InputMode {
    InputMode::Insert
}
fn normal() -> InputMode {
    InputMode::Normal
}
fn visual() -> InputMode {
    InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 0,
        cur_row: 0,
        anchored: false,
    }
}

#[test]
fn ctrl_w_char_returns_ctrl_w_prefix() {
    let a = handle_key_inner(&char_key("w"), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::CtrlWPrefix));
}

#[test]
fn ctrl_dot_from_insert_enters_normal() {
    let a = handle_key_inner(&char_key("."), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Normal)));
}

#[test]
fn ctrl_dot_from_normal_enters_visual() {
    let a = handle_key_inner(&char_key("."), true, false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Visual { .. })));
}

#[test]
fn ctrl_dot_from_visual_enters_insert() {
    let a = handle_key_inner(&char_key("."), true, false, false, &visual(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn ctrl_dot_from_search_enters_insert() {
    let mode = InputMode::Search {
        query: String::new(),
    };
    let a = handle_key_inner(&char_key("."), true, false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn ctrl_backslash_enters_normal() {
    let a = handle_key_inner(
        &char_key("\\"),
        true,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SetMode(InputMode::Normal)));
}

#[test]
fn ctrl_pipe_enters_normal() {
    let a = handle_key_inner(&char_key("|"), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Normal)));
}

#[test]
fn ctrl_shift_v_pastes() {
    let a = handle_key_inner(&char_key("v"), true, true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::Paste));
}

#[test]
fn ctrl_shift_w_closes_tab() {
    let a = handle_key_inner(&char_key("w"), true, true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::CloseTab));
}

#[test]
fn ctrl_shift_r_renames_tab() {
    let a = handle_key_inner(&char_key("r"), true, true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::RenameTab));
}

#[test]
fn ctrl_shift_arrow_up_resizes_pane_up() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowUp),
        true,
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ResizePaneUp));
}

#[test]
fn ctrl_shift_arrow_down_resizes_pane_down() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowDown),
        true,
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ResizePaneDown));
}

#[test]
fn ctrl_shift_k_clears_scrollback() {
    let a = handle_key_inner(&char_key("k"), true, true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::ClearScrollback));
}

#[test]
fn ctrl_shift_l_toggles_log() {
    let a = handle_key_inner(&char_key("l"), true, true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::ToggleLog));
}

#[test]
fn ctrl_shift_l_uppercase_toggles_log() {
    let a = handle_key_inner(&char_key("L"), true, true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::ToggleLog));
}

#[test]
fn ctrl_shift_page_up_moves_tab_left() {
    let a = handle_key_inner(
        &named(NamedKey::PageUp),
        true,
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::MoveTabLeft));
}

#[test]
fn ctrl_shift_page_down_moves_tab_right() {
    let a = handle_key_inner(
        &named(NamedKey::PageDown),
        true,
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::MoveTabRight));
}

#[test]
fn ctrl_shift_home_scrolls_to_top() {
    let a = handle_key_inner(
        &named(NamedKey::Home),
        true,
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollToTop));
}

#[test]
fn ctrl_shift_end_scrolls_to_bottom() {
    let a = handle_key_inner(
        &named(NamedKey::End),
        true,
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollToBottom));
}

#[test]
fn ctrl_c_in_visual_copies() {
    let a = handle_key_inner(&char_key("c"), true, false, false, &visual(), 80, 24, false);
    assert!(matches!(a, Action::Copy));
}

#[test]
fn ctrl_c_in_insert_does_not_copy() {
    // In insert mode ctrl+c is sent as byte 0x03 to the PTY
    let a = handle_key_inner(&char_key("c"), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[3]));
}

#[test]
fn ctrl_q_quits() {
    let a = handle_key_inner(&char_key("q"), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::Quit));
}

#[test]
fn ctrl_comma_opens_config() {
    let a = handle_key_inner(&char_key(","), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::OpenConfig));
}

#[test]
fn ctrl_t_opens_new_tab() {
    let a = handle_key_inner(&char_key("t"), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::NewTab));
}

#[test]
fn ctrl_plus_increases_font_size() {
    let a = handle_key_inner(&char_key("+"), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::IncreaseFontSize));
}

#[test]
fn ctrl_equals_increases_font_size() {
    let a = handle_key_inner(&char_key("="), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::IncreaseFontSize));
}

#[test]
fn ctrl_minus_decreases_font_size() {
    let a = handle_key_inner(&char_key("-"), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::DecreaseFontSize));
}

#[test]
fn ctrl_zero_resets_font_size() {
    let a = handle_key_inner(&char_key("0"), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::ResetFontSize));
}

#[test]
fn ctrl_page_up_prev_tab() {
    let a = handle_key_inner(
        &named(NamedKey::PageUp),
        true,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::PrevTab));
}

#[test]
fn ctrl_page_down_next_tab() {
    let a = handle_key_inner(
        &named(NamedKey::PageDown),
        true,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::NextTab));
}

#[test]
fn shift_page_up_scrolls_full_page() {
    let a = handle_key_inner(
        &named(NamedKey::PageUp),
        false,
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollUp(24)));
}

#[test]
fn shift_page_down_scrolls_full_page() {
    let a = handle_key_inner(
        &named(NamedKey::PageDown),
        false,
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollDown(24)));
}

// ── Insert mode ──────────────────────────────────────────────────────────────

#[test]
fn insert_escape_sends_esc_byte() {
    let a = handle_key_inner(
        &named(NamedKey::Escape),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[0x1b]));
}

#[test]
fn insert_enter_sends_cr() {
    let a = handle_key_inner(
        &named(NamedKey::Enter),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[b'\r']));
}

#[test]
fn insert_backspace_sends_del() {
    let a = handle_key_inner(
        &named(NamedKey::Backspace),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[0x7f]));
}

#[test]
fn insert_tab_sends_tab_byte() {
    let a = handle_key_inner(
        &named(NamedKey::Tab),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[b'\t']));
}

#[test]
fn insert_shift_tab_sends_backtab_sequence() {
    let a = handle_key_inner(
        &named(NamedKey::Tab),
        false,
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[Z"));
}

#[test]
fn normal_shift_tab_returns_none() {
    let a = handle_key_inner(
        &named(NamedKey::Tab),
        false,
        true,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

#[test]
fn visual_shift_tab_returns_none() {
    let a = handle_key_inner(
        &named(NamedKey::Tab),
        false,
        true,
        false,
        &visual(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

#[test]
fn ctrl_shift_tab_sends_backtab_sequence() {
    // ctrl block in handle_insert only intercepts Character keys, so
    // Ctrl+Shift+Tab falls through to the `Tab if shift` arm → same as Shift+Tab.
    let a = handle_key_inner(
        &named(NamedKey::Tab),
        true,
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[Z"));
}

#[test]
fn insert_space_sends_space() {
    let a = handle_key_inner(
        &named(NamedKey::Space),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[b' ']));
}

#[test]
fn insert_char_sends_utf8_bytes() {
    let a = handle_key_inner(
        &char_key("a"),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"a"));
}

#[test]
fn ctrl_enter_toggles_fullscreen_from_insert() {
    let a = handle_key_inner(
        &named(NamedKey::Enter),
        true,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ToggleFullscreen));
}

#[test]
fn ctrl_enter_toggles_fullscreen_from_normal() {
    let a = handle_key_inner(
        &named(NamedKey::Enter),
        true,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ToggleFullscreen));
}

#[test]
fn ctrl_enter_toggles_fullscreen_from_visual() {
    let a = handle_key_inner(
        &named(NamedKey::Enter),
        true,
        false,
        false,
        &visual(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ToggleFullscreen));
}

#[test]
fn insert_arrow_up_normal_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowUp),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[A"));
}

#[test]
fn insert_arrow_up_application_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowUp),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        true,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1bOA"));
}

#[test]
fn insert_arrow_down_normal_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowDown),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[B"));
}

#[test]
fn insert_arrow_right_normal_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowRight),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[C"));
}

#[test]
fn insert_arrow_left_normal_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowLeft),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[D"));
}

#[test]
fn insert_home_normal() {
    let a = handle_key_inner(
        &named(NamedKey::Home),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[1~"));
}

#[test]
fn insert_home_application_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::Home),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        true,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1bOH"));
}

#[test]
fn insert_end_normal() {
    let a = handle_key_inner(
        &named(NamedKey::End),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[4~"));
}

#[test]
fn insert_page_up_sends_csi_5() {
    let a = handle_key_inner(
        &named(NamedKey::PageUp),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[5~"));
}

#[test]
fn insert_page_down_sends_csi_6() {
    let a = handle_key_inner(
        &named(NamedKey::PageDown),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[6~"));
}

#[test]
fn insert_delete_sends_csi_3() {
    let a = handle_key_inner(
        &named(NamedKey::Delete),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[3~"));
}

#[test]
fn insert_f1_through_f4() {
    let cases: &[(NamedKey, &[u8])] = &[
        (NamedKey::F1, b"\x1bOP"),
        (NamedKey::F2, b"\x1bOQ"),
        (NamedKey::F3, b"\x1bOR"),
        (NamedKey::F4, b"\x1bOS"),
    ];
    for (k, expected) in cases {
        let a = handle_key_inner(&named(*k), false, false, false, &insert(), 80, 24, false);
        assert!(
            matches!(a, Action::SendToPty(ref v) if v.as_slice() == *expected),
            "F key mismatch"
        );
    }
}

#[test]
fn insert_f5_through_f12() {
    let cases: &[(NamedKey, &[u8])] = &[
        (NamedKey::F5, b"\x1b[15~"),
        (NamedKey::F6, b"\x1b[17~"),
        (NamedKey::F7, b"\x1b[18~"),
        (NamedKey::F8, b"\x1b[19~"),
        (NamedKey::F9, b"\x1b[20~"),
        (NamedKey::F10, b"\x1b[21~"),
        (NamedKey::F11, b"\x1b[23~"),
        (NamedKey::F12, b"\x1b[24~"),
    ];
    for (k, expected) in cases {
        let a = handle_key_inner(&named(*k), false, false, false, &insert(), 80, 24, false);
        assert!(
            matches!(a, Action::SendToPty(ref v) if v.as_slice() == *expected),
            "F key mismatch"
        );
    }
}

// ── Normal mode ──────────────────────────────────────────────────────────────

#[test]
fn normal_escape_enters_insert() {
    let a = handle_key_inner(
        &named(NamedKey::Escape),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn normal_i_enters_insert() {
    let a = handle_key_inner(
        &char_key("i"),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn normal_v_enters_visual() {
    let a = handle_key_inner(
        &char_key("v"),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SetMode(InputMode::Visual { .. })));
}

#[test]
fn normal_q_closes_pane() {
    let a = handle_key_inner(
        &char_key("q"),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ClosePane));
}

#[test]
fn normal_slash_opens_search() {
    let a = handle_key_inner(
        &char_key("/"),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SearchOpen));
}

#[test]
fn normal_n_search_next() {
    let a = handle_key_inner(
        &char_key("n"),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SearchNext));
}

#[test]
fn normal_shift_n_search_prev() {
    let a = handle_key_inner(
        &char_key("N"),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SearchPrev));
}

#[test]
fn normal_j_scrolls_down_3() {
    let a = handle_key_inner(
        &char_key("j"),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollDown(3)));
}

#[test]
fn normal_k_scrolls_up_3() {
    let a = handle_key_inner(
        &char_key("k"),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollUp(3)));
}

#[test]
fn normal_page_up_scrolls_full_grid() {
    let a = handle_key_inner(
        &named(NamedKey::PageUp),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollUp(24)));
}

#[test]
fn normal_page_down_scrolls_full_grid() {
    let a = handle_key_inner(
        &named(NamedKey::PageDown),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollDown(24)));
}

#[test]
fn normal_unknown_key_returns_none() {
    let a = handle_key_inner(
        &char_key("x"),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

// ── Visual mode ──────────────────────────────────────────────────────────────

fn visual_at(sc: usize, sr: usize, cc: usize, cr: usize) -> InputMode {
    InputMode::Visual {
        start_col: sc,
        start_row: sr,
        cur_col: cc,
        cur_row: cr,
        anchored: false,
    }
}

fn vis_col_row(a: Action) -> (usize, usize) {
    match a {
        Action::SetMode(InputMode::Visual {
            cur_col, cur_row, ..
        }) => (cur_col, cur_row),
        _ => panic!("expected Visual SetMode"),
    }
}

#[test]
fn visual_escape_enters_insert() {
    let mode = visual_at(0, 0, 5, 5);
    let a = handle_key_inner(
        &named(NamedKey::Escape),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn visual_h_moves_left() {
    let mode = visual_at(0, 0, 5, 5);
    let (col, row) = vis_col_row(handle_key_inner(
        &char_key("h"),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 4);
    assert_eq!(row, 5);
}

#[test]
fn visual_l_moves_right() {
    let mode = visual_at(0, 0, 5, 5);
    let (col, row) = vis_col_row(handle_key_inner(
        &char_key("l"),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 6);
    assert_eq!(row, 5);
}

#[test]
fn visual_k_moves_up() {
    let mode = visual_at(0, 0, 5, 5);
    let (col, row) = vis_col_row(handle_key_inner(
        &char_key("k"),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 5);
    assert_eq!(row, 4);
}

#[test]
fn visual_j_moves_down() {
    let mode = visual_at(0, 0, 5, 5);
    let (col, row) = vis_col_row(handle_key_inner(
        &char_key("j"),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 5);
    assert_eq!(row, 6);
}

#[test]
fn visual_zero_jumps_to_col_zero() {
    let mode = visual_at(0, 0, 10, 5);
    let (col, _) = vis_col_row(handle_key_inner(
        &char_key("0"),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 0);
}

#[test]
fn visual_dollar_jumps_to_last_col() {
    let mode = visual_at(0, 0, 0, 0);
    let (col, _) = vis_col_row(handle_key_inner(
        &char_key("$"),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 79);
}

#[test]
fn visual_g_jumps_to_row_zero() {
    let mode = visual_at(0, 0, 5, 10);
    let (_, row) = vis_col_row(handle_key_inner(
        &char_key("g"),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(row, 0);
}

#[test]
fn visual_capital_g_jumps_to_last_row() {
    let mode = visual_at(0, 0, 5, 0);
    let (_, row) = vis_col_row(handle_key_inner(
        &char_key("G"),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(row, 23);
}

#[test]
fn visual_v_sets_anchor() {
    // 'v' in Visual mode sets the anchor at the current cursor position.
    let mode = visual_at(0, 0, 5, 5);
    let a = handle_key_inner(&char_key("v"), false, false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::VisualAnchor));
}

#[test]
fn visual_q_enters_insert() {
    let mode = visual_at(0, 0, 5, 5);
    let a = handle_key_inner(&char_key("q"), false, false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn visual_arrow_left_moves_cursor() {
    let mode = visual_at(0, 0, 5, 5);
    let (col, _) = vis_col_row(handle_key_inner(
        &named(NamedKey::ArrowLeft),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 4);
}

#[test]
fn visual_arrow_right_moves_cursor() {
    let mode = visual_at(0, 0, 5, 5);
    let (col, _) = vis_col_row(handle_key_inner(
        &named(NamedKey::ArrowRight),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 6);
}

#[test]
fn visual_home_jumps_to_col_zero() {
    let mode = visual_at(0, 0, 10, 5);
    let (col, _) = vis_col_row(handle_key_inner(
        &named(NamedKey::Home),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 0);
}

#[test]
fn visual_end_jumps_to_last_col() {
    let mode = visual_at(0, 0, 0, 0);
    let (col, _) = vis_col_row(handle_key_inner(
        &named(NamedKey::End),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 79);
}

#[test]
fn visual_h_at_col_zero_clamps() {
    let mode = visual_at(0, 0, 0, 5);
    let (col, _) = vis_col_row(handle_key_inner(
        &char_key("h"),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 0);
}

#[test]
fn visual_k_at_row_zero_scrolls_up() {
    // At row 0, 'k' triggers boundary scroll instead of clamping.
    let mode = visual_at(0, 0, 5, 0);
    let a = handle_key_inner(&char_key("k"), false, false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::VisualBoundaryUp(1)));
}

#[test]
fn visual_l_at_last_col_clamps() {
    let mode = visual_at(0, 0, 79, 5);
    let (col, _) = vis_col_row(handle_key_inner(
        &char_key("l"),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 79);
}

#[test]
fn visual_start_coords_preserved_on_move() {
    let mode = visual_at(3, 7, 5, 5);
    let a = handle_key_inner(&char_key("j"), false, false, false, &mode, 80, 24, false);
    match a {
        Action::SetMode(InputMode::Visual {
            start_col,
            start_row,
            ..
        }) => {
            assert_eq!(start_col, 3);
            assert_eq!(start_row, 7);
        }
        _ => panic!("expected Visual"),
    }
}

// ── RenameTab and Search modes pass through ──────────────────────────────────

#[test]
fn rename_tab_mode_returns_none() {
    let mode = InputMode::RenameTab { buf: String::new() };
    let a = handle_key_inner(&char_key("a"), false, false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::None));
}

#[test]
fn search_mode_returns_none() {
    let mode = InputMode::Search {
        query: "foo".into(),
    };
    let a = handle_key_inner(&char_key("a"), false, false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::None));
}

// ── Insert: application cursor keys (remaining directions) ───────────────────

#[test]
fn insert_arrow_down_application_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowDown),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        true,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1bOB"));
}

#[test]
fn insert_arrow_right_application_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowRight),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        true,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1bOC"));
}

#[test]
fn insert_arrow_left_application_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowLeft),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        true,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1bOD"));
}

#[test]
fn insert_end_application_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::End),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        true,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1bOF"));
}

// ── Insert: ctrl + char with raw code 1-26 ───────────────────────────────────

#[test]
fn insert_ctrl_char_with_code_1_sends_raw_byte() {
    // '\x01' has code point 1, which falls in the raw 1..=26 branch
    let a = handle_key_inner(
        &char_key("\x01"),
        true,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[1u8]));
}

#[test]
fn insert_ctrl_char_with_code_26_sends_raw_byte() {
    let a = handle_key_inner(
        &char_key("\x1a"),
        true,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[26u8]));
}

// ── Insert: unrecognized named key returns None ───────────────────────────────

#[test]
fn insert_unrecognized_named_key_returns_none() {
    let a = handle_key_inner(
        &named(NamedKey::Alt),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

// ── Normal: unrecognized named key returns None ───────────────────────────────

#[test]
fn normal_unrecognized_named_key_returns_none() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowLeft),
        false,
        false,
        false,
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

// ── Visual: arrow up / down ───────────────────────────────────────────────────

#[test]
fn visual_arrow_up_moves_cursor() {
    let mode = visual_at(0, 0, 5, 5);
    let (col, row) = vis_col_row(handle_key_inner(
        &named(NamedKey::ArrowUp),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 5);
    assert_eq!(row, 4);
}

#[test]
fn visual_arrow_down_moves_cursor() {
    let mode = visual_at(0, 0, 5, 5);
    let (col, row) = vis_col_row(handle_key_inner(
        &named(NamedKey::ArrowDown),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 5);
    assert_eq!(row, 6);
}

// ── Visual: page up / page down ──────────────────────────────────────────────

#[test]
fn visual_page_up_scrolls_full_page() {
    let mode = visual_at(0, 0, 5, 10);
    let a = handle_key_inner(
        &named(NamedKey::PageUp),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::VisualBoundaryUp(24)));
}

#[test]
fn visual_page_down_scrolls_full_page() {
    let mode = visual_at(0, 0, 5, 10);
    let a = handle_key_inner(
        &named(NamedKey::PageDown),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::VisualBoundaryDown(24)));
}

#[test]
fn visual_page_up_with_anchored_selection() {
    let mode = InputMode::Visual {
        start_col: 0,
        start_row: 5,
        cur_col: 3,
        cur_row: 10,
        anchored: true,
    };
    let a = handle_key_inner(
        &named(NamedKey::PageUp),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::VisualBoundaryUp(24)));
}

#[test]
fn visual_page_down_with_anchored_selection() {
    let mode = InputMode::Visual {
        start_col: 0,
        start_row: 5,
        cur_col: 3,
        cur_row: 10,
        anchored: true,
    };
    let a = handle_key_inner(
        &named(NamedKey::PageDown),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::VisualBoundaryDown(24)));
}

// ── Visual: catch-all returns None ───────────────────────────────────────────

#[test]
fn visual_unknown_char_returns_none() {
    let mode = visual_at(0, 0, 5, 5);
    let a = handle_key_inner(&char_key("x"), false, false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::None));
}

#[test]
fn visual_unrecognized_named_key_returns_none() {
    let mode = visual_at(0, 0, 5, 5);
    let a = handle_key_inner(
        &named(NamedKey::Alt),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

// ── Alt modifier encoding ─────────────────────────────────────────────────────

#[test]
fn insert_alt_tab_returns_none() {
    let a = handle_key_inner(
        &named(NamedKey::Tab),
        false,
        false,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

#[test]
fn insert_alt_shift_tab_returns_none() {
    let a = handle_key_inner(
        &named(NamedKey::Tab),
        false,
        true,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

#[test]
fn insert_alt_char_sends_esc_prefixed() {
    let a = handle_key_inner(&char_key("b"), false, false, true, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[0x1b, b'b']));
}

#[test]
fn insert_alt_enter_sends_esc_cr() {
    let a = handle_key_inner(
        &named(NamedKey::Enter),
        false,
        false,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[0x1b, b'\r']));
}

#[test]
fn insert_plain_tab_still_sends_tab_byte() {
    let a = handle_key_inner(
        &named(NamedKey::Tab),
        false,
        false,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[b'\t']));
}

#[test]
fn insert_alt_backspace_sends_esc_del() {
    let a = handle_key_inner(
        &named(NamedKey::Backspace),
        false,
        false,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[0x1b, 0x7f]));
}

#[test]
fn insert_alt_arrow_falls_through_to_regular_match() {
    // alt + ArrowLeft hits `_ => None` in the alt match block, then falls
    // through to the regular key match and produces the normal CSI sequence.
    let a = handle_key_inner(
        &named(NamedKey::ArrowLeft),
        false,
        false,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1b[D"));
}

#[test]
fn insert_alt_tab_consumed_silently() {
    // Alt+Tab is intercepted before the alt-encoding block to avoid sending
    // it to the PTY while the window manager switches focus.
    let a = handle_key_inner(
        &named(NamedKey::Tab),
        false,
        false,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

#[test]
fn insert_alt_enter_sends_escape_cr() {
    let a = handle_key_inner(
        &named(NamedKey::Enter),
        false,
        false,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[0x1b, b'\r']));
}

#[test]
fn insert_alt_backspace_sends_escape_del() {
    let a = handle_key_inner(
        &named(NamedKey::Backspace),
        false,
        false,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[0x1b, 0x7f]));
}

// ── Alt+1..9 → GoToTab ──────────────────────────────────────────────────────

#[test]
fn alt_1_goes_to_tab_0() {
    let a = handle_key_inner(&char_key("1"), false, false, true, &insert(), 80, 24, false);
    assert!(matches!(a, Action::GoToTab(0)));
}

#[test]
fn alt_5_goes_to_tab_4() {
    let a = handle_key_inner(&char_key("5"), false, false, true, &insert(), 80, 24, false);
    assert!(matches!(a, Action::GoToTab(4)));
}

#[test]
fn alt_9_goes_to_tab_8() {
    let a = handle_key_inner(&char_key("9"), false, false, true, &insert(), 80, 24, false);
    assert!(matches!(a, Action::GoToTab(8)));
}

#[test]
fn alt_0_does_not_go_to_tab() {
    let a = handle_key_inner(&char_key("0"), false, false, true, &insert(), 80, 24, false);
    assert!(!matches!(a, Action::GoToTab(_)));
}

#[test]
fn alt_1_works_from_normal_mode() {
    let a = handle_key_inner(&char_key("1"), false, false, true, &normal(), 80, 24, false);
    assert!(matches!(a, Action::GoToTab(0)));
}

#[test]
fn ctrl_alt_1_does_not_go_to_tab() {
    let a = handle_key_inner(&char_key("1"), true, false, true, &insert(), 80, 24, false);
    assert!(!matches!(a, Action::GoToTab(_)));
}

// ── Ctrl+Enter / Alt+key in Insert mode ──────────────────────────────────────

#[test]
fn ctrl_enter_toggles_fullscreen() {
    // Ctrl+Enter is intercepted globally before per-mode handling.
    let a = handle_key_inner(
        &named(NamedKey::Enter),
        true,
        false,
        false,
        &InputMode::Insert,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ToggleFullscreen));
}

#[test]
fn alt_tab_is_consumed_silently() {
    // Alt+Tab is swallowed so the WM focus-switch keystroke isn't forwarded to the PTY.
    let a = handle_key_inner(
        &named(NamedKey::Tab),
        false,
        false,
        true,
        &InputMode::Insert,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

#[test]
fn alt_enter_in_insert_sends_esc_cr() {
    let a = handle_key_inner(
        &named(NamedKey::Enter),
        false,
        false,
        true,
        &InputMode::Insert,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[0x1b, b'\r']));
}

#[test]
fn alt_backspace_in_insert_sends_esc_del() {
    let a = handle_key_inner(
        &named(NamedKey::Backspace),
        false,
        false,
        true,
        &InputMode::Insert,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[0x1b, 0x7f]));
}

// ── Visual swap anchor ───────────────────────────────────────────────────────

#[test]
fn visual_o_swaps_anchor() {
    let mode = visual_at(2, 3, 7, 9);
    let a = handle_key_inner(&char_key("o"), false, false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::VisualSwapAnchor));
}

#[test]
fn visual_o_with_zero_anchor_swaps_anchor() {
    let mode = visual_at(0, 0, 10, 5);
    let a = handle_key_inner(&char_key("o"), false, false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::VisualSwapAnchor));
}

#[test]
fn visual_j_at_last_row_scrolls_down() {
    // At the last row, 'j' triggers boundary scroll instead of clamping.
    let mode = visual_at(0, 0, 5, 23); // rows-1 = 23 (grid has 24 rows → rows arg = 23)
    let a = handle_key_inner(&char_key("j"), false, false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::VisualBoundaryDown(1)));
}

#[test]
fn visual_arrow_up_at_row_zero_scrolls_up() {
    let mode = visual_at(0, 0, 5, 0);
    let a = handle_key_inner(
        &named(NamedKey::ArrowUp),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::VisualBoundaryUp(1)));
}

#[test]
fn visual_arrow_down_at_last_row_scrolls_down() {
    let mode = visual_at(0, 0, 5, 23);
    let a = handle_key_inner(
        &named(NamedKey::ArrowDown),
        false,
        false,
        false,
        &mode,
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::VisualBoundaryDown(1)));
}

// ── Command Palette keybinding tests ────────────────────────────────────────

fn palette_mode() -> InputMode {
    InputMode::CommandPalette {
        query: String::new(),
        selected: 0,
    }
}

#[test]
fn ctrl_shift_p_opens_palette_from_insert() {
    let a = handle_key_inner(&char_key("p"), true, true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::OpenCommandPalette));
}

#[test]
fn ctrl_shift_p_opens_palette_from_normal() {
    let a = handle_key_inner(&char_key("p"), true, true, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::OpenCommandPalette));
}

#[test]
fn ctrl_shift_p_opens_palette_from_visual() {
    let a = handle_key_inner(&char_key("p"), true, true, false, &visual(), 80, 24, false);
    assert!(matches!(a, Action::OpenCommandPalette));
}

#[test]
fn ctrl_shift_p_opens_palette_uppercase_p() {
    let a = handle_key_inner(&char_key("P"), true, true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::OpenCommandPalette));
}

#[test]
fn command_palette_mode_swallows_chars() {
    // Keys in CommandPalette mode return None so the handler in main.rs can intercept them.
    let a = handle_key_inner(
        &char_key("x"),
        false,
        false,
        false,
        &palette_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::None));
}

#[test]
fn ctrl_shift_p_not_triggered_without_ctrl() {
    let a = handle_key_inner(&char_key("p"), false, true, false, &insert(), 80, 24, false);
    assert!(!matches!(a, Action::OpenCommandPalette));
}

#[test]
fn ctrl_shift_p_not_triggered_without_shift() {
    // Ctrl+P without shift is forwarded to PTY (SendToPty), not OpenCommandPalette.
    let a = handle_key_inner(&char_key("p"), true, false, false, &insert(), 80, 24, false);
    assert!(!matches!(a, Action::OpenCommandPalette));
}

// ── pick_seq ─────────────────────────────────────────────────────────────────

#[test]
fn pick_seq_app_true_returns_app_seq() {
    assert_eq!(pick_seq(true, b"\x1bOA", b"\x1b[A"), b"\x1bOA");
}

#[test]
fn pick_seq_app_false_returns_vt_seq() {
    assert_eq!(pick_seq(false, b"\x1bOA", b"\x1b[A"), b"\x1b[A");
}

// ── cursor_seq ───────────────────────────────────────────────────────────────

#[test]
fn cursor_seq_arrow_up_vt() {
    assert_eq!(
        cursor_seq(&named(NamedKey::ArrowUp), false),
        Some(b"\x1b[A".as_ref())
    );
}

#[test]
fn cursor_seq_arrow_up_app() {
    assert_eq!(
        cursor_seq(&named(NamedKey::ArrowUp), true),
        Some(b"\x1bOA".as_ref())
    );
}

#[test]
fn cursor_seq_unknown_returns_none() {
    assert!(cursor_seq(&named(NamedKey::F1), false).is_none());
}

// ── handle_ctrl_only ─────────────────────────────────────────────────────────

#[test]
fn handle_ctrl_only_w_returns_ctrl_w_prefix() {
    assert!(matches!(
        handle_ctrl_only(&char_key("w"), false, &insert()),
        Some(Action::CtrlWPrefix)
    ));
}

#[test]
fn handle_ctrl_only_q_returns_quit() {
    assert!(matches!(
        handle_ctrl_only(&char_key("q"), false, &insert()),
        Some(Action::Quit)
    ));
}

#[test]
fn handle_ctrl_only_c_in_visual_returns_copy() {
    let mode = InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 0,
        cur_row: 0,
        anchored: true,
    };
    assert!(matches!(
        handle_ctrl_only(&char_key("c"), false, &mode),
        Some(Action::Copy)
    ));
}

#[test]
fn handle_ctrl_only_c_in_insert_not_copy() {
    assert!(!matches!(
        handle_ctrl_only(&char_key("c"), false, &insert()),
        Some(Action::Copy)
    ));
}

// ── visual_up_action / visual_down_action ────────────────────────────────────

#[test]
fn visual_up_at_top_returns_boundary_up() {
    let move_to = |_c: usize, _r: usize| Action::None;
    assert!(matches!(
        visual_up_action(0, 0, &move_to),
        Action::VisualBoundaryUp(1)
    ));
}

#[test]
fn visual_up_not_at_top_moves_cursor() {
    let move_to = |_c: usize, r: usize| Action::ScrollUp(r);
    assert!(matches!(
        visual_up_action(0, 5, &move_to),
        Action::ScrollUp(4)
    ));
}

#[test]
fn visual_down_at_bottom_returns_boundary_down() {
    let move_to = |_c: usize, _r: usize| Action::None;
    assert!(matches!(
        visual_down_action(0, 10, 10, &move_to),
        Action::VisualBoundaryDown(1)
    ));
}

#[test]
fn visual_down_not_at_bottom_moves_cursor() {
    let move_to = |_c: usize, r: usize| Action::ScrollDown(r);
    assert!(matches!(
        visual_down_action(0, 5, 10, &move_to),
        Action::ScrollDown(6)
    ));
}

// ── visual_char_action ───────────────────────────────────────────────────────

#[test]
fn visual_char_w_returns_word_forward() {
    let move_to = |_c: usize, _r: usize| Action::None;
    assert!(matches!(
        visual_char_action("w", 0, 0, 80, 24, &move_to),
        Action::VisualWordForward
    ));
}

#[test]
fn visual_char_y_returns_copy() {
    let move_to = |_c: usize, _r: usize| Action::None;
    assert!(matches!(
        visual_char_action("y", 5, 3, 80, 24, &move_to),
        Action::Copy
    ));
}

#[test]
fn visual_char_h_moves_left() {
    let move_to = |c: usize, r: usize| {
        Action::SetMode(crate::input::InputMode::Visual {
            start_col: 0,
            start_row: 0,
            cur_col: c,
            cur_row: r,
            anchored: false,
        })
    };
    let result = visual_char_action("h", 5, 3, 80, 24, &move_to);
    assert!(matches!(
        result,
        Action::SetMode(crate::input::InputMode::Visual {
            cur_col: 4,
            cur_row: 3,
            ..
        })
    ));
}

#[test]
fn visual_char_unknown_returns_none() {
    let move_to = |_c: usize, _r: usize| Action::None;
    assert!(matches!(
        visual_char_action("z", 0, 0, 80, 24, &move_to),
        Action::None
    ));
}

// ── ctrl_shift_char_action ───────────────────────────────────────────────────

#[test]
fn ctrl_shift_char_v_pastes() {
    assert!(matches!(ctrl_shift_char_action("v"), Some(Action::Paste)));
}

#[test]
fn ctrl_shift_char_uppercase_v_pastes() {
    assert!(matches!(ctrl_shift_char_action("V"), Some(Action::Paste)));
}

#[test]
fn ctrl_shift_char_w_closes_tab() {
    assert!(matches!(
        ctrl_shift_char_action("w"),
        Some(Action::CloseTab)
    ));
}

#[test]
fn ctrl_shift_char_unknown_returns_none() {
    assert!(ctrl_shift_char_action("x").is_none());
}

// ── ctrl_char_key_action ─────────────────────────────────────────────────────

#[test]
fn ctrl_char_key_q_quits() {
    assert!(matches!(ctrl_char_key_action("q"), Some(Action::Quit)));
}

#[test]
fn ctrl_char_key_uppercase_q_quits() {
    assert!(matches!(ctrl_char_key_action("Q"), Some(Action::Quit)));
}

#[test]
fn ctrl_char_key_plus_increases_font() {
    assert!(matches!(
        ctrl_char_key_action("+"),
        Some(Action::IncreaseFontSize)
    ));
}

#[test]
fn ctrl_char_key_equals_increases_font() {
    assert!(matches!(
        ctrl_char_key_action("="),
        Some(Action::IncreaseFontSize)
    ));
}

#[test]
fn ctrl_char_key_unknown_returns_none() {
    assert!(ctrl_char_key_action("z").is_none());
}

// ── Screenshot keybindings ───────────────────────────────────────────────────

fn screenshot_mode() -> InputMode {
    InputMode::Screenshot {
        cx: 400,
        cy: 300,
        half_w: 100,
        half_h: 100,
    }
}

#[test]
fn ctrl_w_p_opens_screenshot() {
    assert!(matches!(
        ctrl_w_action(&char_key("p")),
        Action::ScreenshotOpen
    ));
}

#[test]
fn screenshot_arrow_right_moves_right() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowRight),
        false,
        false,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScreenshotMove(20, 0)));
}

#[test]
fn screenshot_arrow_left_moves_left() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowLeft),
        false,
        false,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScreenshotMove(-20, 0)));
}

#[test]
fn screenshot_arrow_down_moves_down() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowDown),
        false,
        false,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScreenshotMove(0, 20)));
}

#[test]
fn screenshot_arrow_up_moves_up() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowUp),
        false,
        false,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScreenshotMove(0, -20)));
}

#[test]
fn screenshot_shift_up_shrinks_height() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowUp),
        false,
        true,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScreenshotEdgeResize(0, -1)));
}

#[test]
fn screenshot_shift_down_grows_height() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowDown),
        false,
        true,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScreenshotEdgeResize(0, 1)));
}

#[test]
fn screenshot_shift_left_shrinks_width() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowLeft),
        false,
        true,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScreenshotEdgeResize(-1, 0)));
}

#[test]
fn screenshot_shift_right_grows_width() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowRight),
        false,
        true,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScreenshotEdgeResize(1, 0)));
}

#[test]
fn screenshot_enter_captures() {
    let a = handle_key_inner(
        &named(NamedKey::Enter),
        false,
        false,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScreenshotCapture));
}

#[test]
fn screenshot_space_captures() {
    let a = handle_key_inner(
        &named(NamedKey::Space),
        false,
        false,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScreenshotCapture));
}

#[test]
fn screenshot_esc_exits_to_insert() {
    let a = handle_key_inner(
        &named(NamedKey::Escape),
        false,
        false,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn screenshot_ctrl_shift_arrow_resizes_pane_not_screenshot() {
    // Global shortcuts still fire even in Screenshot mode — Ctrl+Shift+Arrow resizes panes.
    let a = handle_key_inner(
        &named(NamedKey::ArrowRight),
        true,
        true,
        false,
        &screenshot_mode(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ResizePaneRight));
}

// ── TogglePassthrough (Ctrl+B) ────────────────────────────────────────────────

#[test]
fn ctrl_b_in_insert_toggles_passthrough() {
    let a = handle_key_inner(&char_key("b"), true, false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::TogglePassthrough));
}

#[test]
fn ctrl_b_in_normal_toggles_passthrough() {
    // Global shortcut fires regardless of mode.
    let a = handle_key_inner(&char_key("b"), true, false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::TogglePassthrough));
}

#[test]
fn ctrl_b_in_visual_toggles_passthrough() {
    let a = handle_key_inner(&char_key("b"), true, false, false, &visual(), 80, 24, false);
    assert!(matches!(a, Action::TogglePassthrough));
}

#[test]
fn ctrl_shift_b_does_not_toggle_passthrough() {
    // Ctrl+Shift+B is not a bound shortcut, should not toggle passthrough.
    let a = handle_key_inner(&char_key("b"), true, true, false, &insert(), 80, 24, false);
    assert!(!matches!(a, Action::TogglePassthrough));
}

#[test]
fn ctrl_alt_b_also_toggles_passthrough() {
    // Ctrl takes priority over Alt for char keys — consistent with Ctrl+Alt+T → NewTab.
    let a = handle_key_inner(&char_key("b"), true, false, true, &insert(), 80, 24, false);
    assert!(matches!(a, Action::TogglePassthrough));
}
