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
    }
}

#[test]
fn ctrl_w_char_returns_ctrl_w_prefix() {
    let a = handle_key_inner(&char_key("w"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::CtrlWPrefix));
}

#[test]
fn ctrl_dot_from_insert_enters_normal() {
    let a = handle_key_inner(&char_key("."), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Normal)));
}

#[test]
fn ctrl_dot_from_normal_enters_visual() {
    let a = handle_key_inner(&char_key("."), true, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Visual { .. })));
}

#[test]
fn ctrl_dot_from_visual_enters_insert() {
    let a = handle_key_inner(&char_key("."), true, false, &visual(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn ctrl_dot_from_search_enters_insert() {
    let mode = InputMode::Search {
        query: String::new(),
    };
    let a = handle_key_inner(&char_key("."), true, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn ctrl_backslash_enters_normal() {
    let a = handle_key_inner(&char_key("\\"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Normal)));
}

#[test]
fn ctrl_pipe_enters_normal() {
    let a = handle_key_inner(&char_key("|"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Normal)));
}

#[test]
fn ctrl_shift_v_pastes() {
    let a = handle_key_inner(&char_key("v"), true, true, &insert(), 80, 24, false);
    assert!(matches!(a, Action::Paste));
}

#[test]
fn ctrl_shift_w_closes_tab() {
    let a = handle_key_inner(&char_key("w"), true, true, &insert(), 80, 24, false);
    assert!(matches!(a, Action::CloseTab));
}

#[test]
fn ctrl_shift_r_renames_tab() {
    let a = handle_key_inner(&char_key("r"), true, true, &insert(), 80, 24, false);
    assert!(matches!(a, Action::RenameTab));
}

#[test]
fn ctrl_shift_arrow_up_scrolls_up_1() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowUp),
        true,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollUp(1)));
}

#[test]
fn ctrl_shift_arrow_down_scrolls_down_1() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowDown),
        true,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollDown(1)));
}

#[test]
fn ctrl_shift_page_up_scrolls_half_page() {
    let a = handle_key_inner(
        &named(NamedKey::PageUp),
        true,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollUp(12)));
}

#[test]
fn ctrl_shift_page_down_scrolls_half_page() {
    let a = handle_key_inner(
        &named(NamedKey::PageDown),
        true,
        true,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollDown(12)));
}

#[test]
fn ctrl_shift_home_scrolls_to_top() {
    let a = handle_key_inner(&named(NamedKey::Home), true, true, &insert(), 80, 24, false);
    assert!(matches!(a, Action::ScrollToTop));
}

#[test]
fn ctrl_shift_end_scrolls_to_bottom() {
    let a = handle_key_inner(&named(NamedKey::End), true, true, &insert(), 80, 24, false);
    assert!(matches!(a, Action::ScrollToBottom));
}

#[test]
fn ctrl_c_in_visual_copies() {
    let a = handle_key_inner(&char_key("c"), true, false, &visual(), 80, 24, false);
    assert!(matches!(a, Action::Copy));
}

#[test]
fn ctrl_c_in_insert_does_not_copy() {
    // In insert mode ctrl+c is sent as byte 0x03 to the PTY
    let a = handle_key_inner(&char_key("c"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[3]));
}

#[test]
fn ctrl_q_quits() {
    let a = handle_key_inner(&char_key("q"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::Quit));
}

#[test]
fn ctrl_comma_opens_config() {
    let a = handle_key_inner(&char_key(","), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::OpenConfig));
}

#[test]
fn ctrl_t_opens_new_tab() {
    let a = handle_key_inner(&char_key("t"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::NewTab));
}

#[test]
fn ctrl_plus_increases_font_size() {
    let a = handle_key_inner(&char_key("+"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::IncreaseFontSize));
}

#[test]
fn ctrl_equals_increases_font_size() {
    let a = handle_key_inner(&char_key("="), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::IncreaseFontSize));
}

#[test]
fn ctrl_minus_decreases_font_size() {
    let a = handle_key_inner(&char_key("-"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::DecreaseFontSize));
}

#[test]
fn ctrl_zero_resets_font_size() {
    let a = handle_key_inner(&char_key("0"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::ResetFontSize));
}

#[test]
fn ctrl_page_up_prev_tab() {
    let a = handle_key_inner(
        &named(NamedKey::PageUp),
        true,
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
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[b'\t']));
}

#[test]
fn insert_space_sends_space() {
    let a = handle_key_inner(
        &named(NamedKey::Space),
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
    let a = handle_key_inner(&char_key("a"), false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"a"));
}

#[test]
fn insert_ctrl_enter_sends_newline() {
    let a = handle_key_inner(
        &named(NamedKey::Enter),
        true,
        false,
        &insert(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[b'\n']));
}

#[test]
fn insert_arrow_up_normal_cursor() {
    let a = handle_key_inner(
        &named(NamedKey::ArrowUp),
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
        let a = handle_key_inner(&named(*k), false, false, &insert(), 80, 24, false);
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
        let a = handle_key_inner(&named(*k), false, false, &insert(), 80, 24, false);
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
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn normal_i_enters_insert() {
    let a = handle_key_inner(&char_key("i"), false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn normal_v_enters_visual() {
    let a = handle_key_inner(&char_key("v"), false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Visual { .. })));
}

#[test]
fn normal_q_closes_pane() {
    let a = handle_key_inner(&char_key("q"), false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::ClosePane));
}

#[test]
fn normal_slash_opens_search() {
    let a = handle_key_inner(&char_key("/"), false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::SearchOpen));
}

#[test]
fn normal_n_search_next() {
    let a = handle_key_inner(&char_key("n"), false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::SearchNext));
}

#[test]
fn normal_shift_n_search_prev() {
    let a = handle_key_inner(&char_key("N"), false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::SearchPrev));
}

#[test]
fn normal_j_scrolls_down_3() {
    let a = handle_key_inner(&char_key("j"), false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::ScrollDown(3)));
}

#[test]
fn normal_k_scrolls_up_3() {
    let a = handle_key_inner(&char_key("k"), false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::ScrollUp(3)));
}

#[test]
fn normal_page_up_scrolls_full_grid() {
    let a = handle_key_inner(
        &named(NamedKey::PageUp),
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
        &normal(),
        80,
        24,
        false,
    );
    assert!(matches!(a, Action::ScrollDown(24)));
}

#[test]
fn normal_unknown_key_returns_none() {
    let a = handle_key_inner(&char_key("x"), false, false, &normal(), 80, 24, false);
    assert!(matches!(a, Action::None));
}

// ── Visual mode ──────────────────────────────────────────────────────────────

fn visual_at(sc: usize, sr: usize, cc: usize, cr: usize) -> InputMode {
    InputMode::Visual {
        start_col: sc,
        start_row: sr,
        cur_col: cc,
        cur_row: cr,
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
    let a = handle_key_inner(&named(NamedKey::Escape), false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn visual_h_moves_left() {
    let mode = visual_at(0, 0, 5, 5);
    let (col, row) = vis_col_row(handle_key_inner(
        &char_key("h"),
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
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(row, 23);
}

#[test]
fn visual_v_enters_insert() {
    let mode = visual_at(0, 0, 5, 5);
    let a = handle_key_inner(&char_key("v"), false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn visual_q_enters_insert() {
    let mode = visual_at(0, 0, 5, 5);
    let a = handle_key_inner(&char_key("q"), false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::SetMode(InputMode::Insert)));
}

#[test]
fn visual_arrow_left_moves_cursor() {
    let mode = visual_at(0, 0, 5, 5);
    let (col, _) = vis_col_row(handle_key_inner(
        &named(NamedKey::ArrowLeft),
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
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 0);
}

#[test]
fn visual_k_at_row_zero_clamps() {
    let mode = visual_at(0, 0, 5, 0);
    let (_, row) = vis_col_row(handle_key_inner(
        &char_key("k"),
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
fn visual_l_at_last_col_clamps() {
    let mode = visual_at(0, 0, 79, 5);
    let (col, _) = vis_col_row(handle_key_inner(
        &char_key("l"),
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
    let a = handle_key_inner(&char_key("j"), false, false, &mode, 80, 24, false);
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
    let a = handle_key_inner(&char_key("a"), false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::None));
}

#[test]
fn search_mode_returns_none() {
    let mode = InputMode::Search {
        query: "foo".into(),
    };
    let a = handle_key_inner(&char_key("a"), false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::None));
}

// ── Insert: application cursor keys (remaining directions) ───────────────────

#[test]
fn insert_arrow_down_application_cursor() {
    let a = handle_key_inner(&named(NamedKey::ArrowDown), false, false, &insert(), 80, 24, true);
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1bOB"));
}

#[test]
fn insert_arrow_right_application_cursor() {
    let a = handle_key_inner(&named(NamedKey::ArrowRight), false, false, &insert(), 80, 24, true);
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1bOC"));
}

#[test]
fn insert_arrow_left_application_cursor() {
    let a = handle_key_inner(&named(NamedKey::ArrowLeft), false, false, &insert(), 80, 24, true);
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1bOD"));
}

#[test]
fn insert_end_application_cursor() {
    let a = handle_key_inner(&named(NamedKey::End), false, false, &insert(), 80, 24, true);
    assert!(matches!(a, Action::SendToPty(ref v) if v == b"\x1bOF"));
}

// ── Insert: ctrl + char with raw code 1-26 ───────────────────────────────────

#[test]
fn insert_ctrl_char_with_code_1_sends_raw_byte() {
    // '\x01' has code point 1, which falls in the raw 1..=26 branch
    let a = handle_key_inner(&char_key("\x01"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[1u8]));
}

#[test]
fn insert_ctrl_char_with_code_26_sends_raw_byte() {
    let a = handle_key_inner(&char_key("\x1a"), true, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::SendToPty(ref v) if v == &[26u8]));
}

// ── Insert: unrecognized named key returns None ───────────────────────────────

#[test]
fn insert_unrecognized_named_key_returns_none() {
    let a = handle_key_inner(&named(NamedKey::Alt), false, false, &insert(), 80, 24, false);
    assert!(matches!(a, Action::None));
}

// ── Normal: unrecognized named key returns None ───────────────────────────────

#[test]
fn normal_unrecognized_named_key_returns_none() {
    let a = handle_key_inner(&named(NamedKey::ArrowLeft), false, false, &normal(), 80, 24, false);
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
        &mode,
        80,
        24,
        false,
    ));
    assert_eq!(col, 5);
    assert_eq!(row, 6);
}

// ── Visual: catch-all returns None ───────────────────────────────────────────

#[test]
fn visual_unknown_char_returns_none() {
    let mode = visual_at(0, 0, 5, 5);
    let a = handle_key_inner(&char_key("x"), false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::None));
}

#[test]
fn visual_unrecognized_named_key_returns_none() {
    let mode = visual_at(0, 0, 5, 5);
    let a = handle_key_inner(&named(NamedKey::Alt), false, false, &mode, 80, 24, false);
    assert!(matches!(a, Action::None));
}
