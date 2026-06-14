use super::*;
use crate::input::keybindings::Action;
use crate::input::mode::InputMode;
use winit::keyboard::NamedKey;

fn mods(ctrl: bool, shift: bool, alt: bool, cmd: bool) -> Mods {
    Mods {
        ctrl,
        shift,
        alt,
        cmd,
    }
}

#[test]
fn parse_simple_cmd_v() {
    let (scope, key) = parse_binding("cmd+v").expect("should parse");
    assert_eq!(scope, ModeClass::Global);
    assert_eq!(key.mods, mods(false, false, false, true));
    assert_eq!(key.token, KeyToken::Char("v".into()));
    assert!(key.chord_tail.is_none());
}

#[test]
fn parse_is_modifier_order_insensitive() {
    let a = parse_binding("ctrl+shift+v").unwrap().1;
    let b = parse_binding("shift+ctrl+v").unwrap().1;
    assert_eq!(a, b);
}

#[test]
fn parse_letters_are_lowercased() {
    let a = parse_binding("ctrl+V").unwrap().1;
    assert_eq!(a.token, KeyToken::Char("v".into()));
}

#[test]
fn parse_named_keys() {
    assert_eq!(
        parse_binding("shift+pageup").unwrap().1.token,
        KeyToken::Named(NamedKey::PageUp)
    );
    assert_eq!(
        parse_binding("enter").unwrap().1.token,
        KeyToken::Named(NamedKey::Enter)
    );
    assert_eq!(
        parse_binding("ctrl+arrowleft").unwrap().1.token,
        KeyToken::Named(NamedKey::ArrowLeft)
    );
    assert_eq!(
        parse_binding("f12").unwrap().1.token,
        KeyToken::Named(NamedKey::F12)
    );
}

#[test]
fn parse_punctuation_tokens() {
    assert_eq!(
        parse_binding("ctrl+,").unwrap().1.token,
        KeyToken::Char(",".into())
    );
    assert_eq!(
        parse_binding("cmd++").unwrap().1.token,
        KeyToken::Char("+".into())
    );
    assert_eq!(
        parse_binding("cmd+=").unwrap().1.token,
        KeyToken::Char("=".into())
    );
}

#[test]
fn parse_mode_prefix() {
    let (scope, _) = parse_binding("normal:g").unwrap();
    assert_eq!(scope, ModeClass::Normal);
    let (scope, _) = parse_binding("visual:w").unwrap();
    assert_eq!(scope, ModeClass::Visual);
}

#[test]
fn parse_chord_tail() {
    let key = parse_binding("ctrl+w x").unwrap().1;
    assert_eq!(key.mods, mods(true, false, false, false));
    assert_eq!(key.token, KeyToken::Char("w".into()));
    let (tmods, ttoken) = key.chord_tail.expect("chord tail");
    assert_eq!(tmods, mods(false, false, false, false));
    assert_eq!(ttoken, KeyToken::Char("x".into()));
}

#[test]
fn parse_chord_tail_preserves_uppercase() {
    // Ctrl+W R must keep the uppercase tail (shift) distinct from Ctrl+W r.
    let key = parse_binding("ctrl+w R").unwrap().1;
    let (_tmods, ttoken) = key.chord_tail.unwrap();
    assert_eq!(ttoken, KeyToken::Char("R".into()));
}

#[test]
fn parse_empty_trailing_plus_errors() {
    assert!(parse_binding("ctrl+").is_err());
}

#[test]
fn parse_unknown_modifier_errors() {
    assert!(parse_binding("hyper+v").is_err());
}

#[test]
fn parse_unknown_named_key_errors() {
    assert!(parse_binding("ctrl+nope").is_err());
}

#[test]
fn parse_empty_errors() {
    assert!(parse_binding("").is_err());
    assert!(parse_binding("   ").is_err());
}

// ── Action registry ──────────────────────────────────────────────────────────

fn ctx(grid_rows: usize, mode: InputMode) -> DispatchCtx {
    DispatchCtx {
        grid_rows,
        mode: InputModeKind::of(&mode),
    }
}

#[test]
fn registry_paste() {
    assert!(matches!(
        action_from_name("paste", ctx(24, InputMode::Insert)),
        Some(Action::Paste)
    ));
}

#[test]
fn registry_new_tab() {
    assert!(matches!(
        action_from_name("new_tab", ctx(24, InputMode::Insert)),
        Some(Action::NewTab)
    ));
}

#[test]
fn registry_scroll_page_up_uses_grid_rows() {
    assert!(matches!(
        action_from_name("scroll_page_up", ctx(40, InputMode::Insert)),
        Some(Action::ScrollUp(40))
    ));
}

#[test]
fn registry_go_to_tab_1_is_index_0() {
    assert!(matches!(
        action_from_name("go_to_tab_1", ctx(24, InputMode::Insert)),
        Some(Action::GoToTab(0))
    ));
    assert!(matches!(
        action_from_name("go_to_tab_9", ctx(24, InputMode::Insert)),
        Some(Action::GoToTab(8))
    ));
}

#[test]
fn registry_cycle_mode_from_insert_is_normal() {
    assert!(matches!(
        action_from_name("cycle_mode", ctx(24, InputMode::Insert)),
        Some(Action::SetMode(InputMode::Normal))
    ));
}

#[test]
fn registry_cycle_mode_from_normal_is_visual() {
    assert!(matches!(
        action_from_name("cycle_mode", ctx(24, InputMode::Normal)),
        Some(Action::SetMode(InputMode::Visual {
            anchored: false,
            ..
        }))
    ));
}

#[test]
fn registry_cycle_mode_from_visual_is_insert() {
    let visual = InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 0,
        cur_row: 0,
        anchored: true,
    };
    assert!(matches!(
        action_from_name("cycle_mode", ctx(24, visual)),
        Some(Action::SetMode(InputMode::Insert))
    ));
}

#[test]
fn registry_enter_normal_mode() {
    assert!(matches!(
        action_from_name("enter_normal_mode", ctx(24, InputMode::Insert)),
        Some(Action::SetMode(InputMode::Normal))
    ));
}

#[test]
fn registry_unknown_returns_none() {
    assert!(action_from_name("definitely_not_an_action", ctx(24, InputMode::Insert)).is_none());
}

#[test]
fn registry_none_keyword_is_not_an_action() {
    // "none" is the reserved disable value handled by from_config, NOT a bindable action.
    assert!(action_from_name("none", ctx(24, InputMode::Insert)).is_none());
}

#[test]
fn name_of_action_roundtrips_paste() {
    assert_eq!(name_of_action(&Action::Paste), Some("paste"));
}

#[test]
fn name_of_action_internal_returns_none() {
    assert_eq!(name_of_action(&Action::SendToPty(vec![1])), None);
    assert_eq!(name_of_action(&Action::None), None);
}

// ── KeyMap + default_keymap + lookup ──────────────────────────────────────────

fn tok(c: &str) -> KeyToken {
    KeyToken::Char(c.into())
}

#[test]
fn default_has_ctrl_q_quit() {
    let km = default_keymap();
    let key = BindingKey {
        mods: mods(true, false, false, false),
        token: tok("q"),
        chord_tail: None,
    };
    assert_eq!(km.lookup(ModeClass::Global, &key), Some("quit"));
}

#[test]
fn default_has_ctrl_shift_v_paste() {
    let km = default_keymap();
    let key = BindingKey {
        mods: mods(true, true, false, false),
        token: tok("v"),
        chord_tail: None,
    };
    assert_eq!(km.lookup(ModeClass::Global, &key), Some("paste"));
}

#[test]
fn default_has_cmd_v_paste() {
    let km = default_keymap();
    let key = BindingKey {
        mods: mods(false, false, false, true),
        token: tok("v"),
        chord_tail: None,
    };
    assert_eq!(km.lookup(ModeClass::Global, &key), Some("paste"));
}

#[test]
fn default_has_cmd_digit_go_to_tab() {
    let km = default_keymap();
    let key = BindingKey {
        mods: mods(false, false, false, true),
        token: tok("3"),
        chord_tail: None,
    };
    assert_eq!(km.lookup(ModeClass::Global, &key), Some("go_to_tab_3"));
}

#[test]
fn default_has_alt_digit_go_to_tab() {
    let km = default_keymap();
    let key = BindingKey {
        mods: mods(false, false, true, false),
        token: tok("1"),
        chord_tail: None,
    };
    assert_eq!(km.lookup(ModeClass::Global, &key), Some("go_to_tab_1"));
}

#[test]
fn default_has_ctrl_w_prefix_bare() {
    let km = default_keymap();
    let key = BindingKey {
        mods: mods(true, false, false, false),
        token: tok("w"),
        chord_tail: None,
    };
    assert_eq!(km.lookup(ModeClass::Global, &key), Some("ctrl_w_prefix"));
}

#[test]
fn default_has_ctrl_w_chord_split() {
    let km = default_keymap();
    let key = BindingKey {
        mods: mods(true, false, false, false),
        token: tok("w"),
        chord_tail: Some((mods(false, false, false, false), tok("v"))),
    };
    assert_eq!(km.lookup(ModeClass::Global, &key), Some("split_horizontal"));
}

#[test]
fn default_has_ctrl_w_chord_uppercase_r_backward() {
    let km = default_keymap();
    let key = BindingKey {
        mods: mods(true, false, false, false),
        token: tok("w"),
        chord_tail: Some((mods(false, false, false, false), KeyToken::Char("R".into()))),
    };
    assert_eq!(
        km.lookup(ModeClass::Global, &key),
        Some("rotate_panes_backward")
    );
}

#[test]
fn lookup_miss_returns_none() {
    let km = default_keymap();
    let key = BindingKey {
        mods: mods(false, false, false, false),
        token: tok("a"),
        chord_tail: None,
    };
    assert_eq!(km.lookup(ModeClass::Global, &key), None);
}

// ── token_from_key (runtime winit Key → KeyToken) ─────────────────────────────

#[test]
fn token_from_key_lowercases_when_requested() {
    let upper = winit::keyboard::Key::Character("V".into());
    assert_eq!(
        token_from_key(&upper, true),
        Some(KeyToken::Char("v".into()))
    );
}

#[test]
fn token_from_key_preserves_case_for_chord_tail() {
    let upper = winit::keyboard::Key::Character("R".into());
    assert_eq!(
        token_from_key(&upper, false),
        Some(KeyToken::Char("R".into()))
    );
}

#[test]
fn token_from_key_named_key() {
    let enter = winit::keyboard::Key::Named(NamedKey::Enter);
    assert_eq!(
        token_from_key(&enter, true),
        Some(KeyToken::Named(NamedKey::Enter))
    );
}

#[test]
fn token_from_key_unmapped_returns_none() {
    let dead = winit::keyboard::Key::Dead(None);
    assert_eq!(token_from_key(&dead, true), None);
}
