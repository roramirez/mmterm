use super::*;
use crate::config::Config;

fn make_panel() -> ConfigPanel {
    ConfigPanel::from_config(&Config::default())
}

// ── from_config ───────────────────────────────────────────────────────────────

#[test]
fn from_config_has_correct_field_count() {
    let panel = make_panel();
    // 13 base fields + 16 palette = 29
    assert_eq!(panel.fields.len(), 29);
}

#[test]
fn from_config_initial_state() {
    let panel = make_panel();
    assert_eq!(panel.selected, 0);
    assert!(!panel.editing);
    assert!(panel.edit_buf.is_empty());
    assert!(panel.status.is_none());
}

#[test]
fn from_config_font_size_matches() {
    let cfg = Config::default();
    let panel = ConfigPanel::from_config(&cfg);
    assert_eq!(panel.fields[F_FONT_SIZE].value, cfg.font.size.to_string());
}

#[test]
fn from_config_shell_none_is_empty_string() {
    let cfg = Config::default();
    let panel = ConfigPanel::from_config(&cfg);
    let expected = cfg.shell.program.unwrap_or_default();
    assert_eq!(panel.fields[F_SHELL].value, expected);
}

#[test]
fn from_config_palette_has_16_entries_at_end() {
    let panel = make_panel();
    assert_eq!(panel.fields[F_PALETTE].section, Some("Palette"));
    for i in 1..16 {
        assert!(panel.fields[F_PALETTE + i].section.is_none());
    }
}

// ── Navigation ────────────────────────────────────────────────────────────────

#[test]
fn handle_char_j_moves_down() {
    let mut panel = make_panel();
    assert_eq!(panel.selected, 0);
    panel.handle_char('j');
    assert_eq!(panel.selected, 1);
}

#[test]
fn handle_char_k_moves_up() {
    let mut panel = make_panel();
    panel.selected = 3;
    panel.handle_char('k');
    assert_eq!(panel.selected, 2);
}

#[test]
fn handle_up_and_down() {
    let mut panel = make_panel();
    panel.handle_down();
    panel.handle_down();
    assert_eq!(panel.selected, 2);
    panel.handle_up();
    assert_eq!(panel.selected, 1);
}

#[test]
fn move_up_at_zero_stays_at_zero() {
    let mut panel = make_panel();
    panel.handle_up();
    assert_eq!(panel.selected, 0);
}

#[test]
fn move_down_at_last_stays() {
    let mut panel = make_panel();
    panel.selected = panel.fields.len() - 1;
    panel.handle_down();
    assert_eq!(panel.selected, panel.fields.len() - 1);
}

// ── Cancel / quit ─────────────────────────────────────────────────────────────

#[test]
fn handle_char_unknown_not_editing_is_noop() {
    let mut panel = make_panel();
    let action = panel.handle_char('x');
    assert!(matches!(action, ConfigAction::None));
    assert_eq!(panel.selected, 0);
}

#[test]
fn handle_char_q_returns_cancel() {
    let mut panel = make_panel();
    assert!(matches!(panel.handle_char('q'), ConfigAction::Cancel));
}

#[test]
fn handle_char_escape_returns_cancel_when_not_editing() {
    let mut panel = make_panel();
    assert!(matches!(panel.handle_char('\x1b'), ConfigAction::Cancel));
}

#[test]
fn handle_escape_not_editing_returns_cancel() {
    let mut panel = make_panel();
    assert!(matches!(panel.handle_escape(), ConfigAction::Cancel));
}

// ── Editing ───────────────────────────────────────────────────────────────────

#[test]
fn handle_char_i_starts_editing() {
    let mut panel = make_panel();
    panel.handle_char('i');
    assert!(panel.editing);
    assert_eq!(panel.edit_buf, panel.fields[0].value);
}

#[test]
fn handle_char_enter_starts_editing_when_not_editing() {
    let mut panel = make_panel();
    panel.handle_char('\r');
    assert!(panel.editing);
}

#[test]
fn handle_char_appends_when_editing() {
    let mut panel = make_panel();
    panel.handle_char('i'); // start editing
    panel.edit_buf.clear();
    panel.handle_char('a');
    panel.handle_char('b');
    assert_eq!(panel.edit_buf, "ab");
}

#[test]
fn handle_char_backspace_removes_last_char() {
    let mut panel = make_panel();
    panel.handle_char('i');
    panel.edit_buf = "hello".to_string();
    panel.handle_char('\x7f');
    assert_eq!(panel.edit_buf, "hell");
}

#[test]
fn handle_backspace_while_editing_removes_char() {
    let mut panel = make_panel();
    panel.editing = true;
    panel.edit_buf = "abc".to_string();
    panel.handle_backspace();
    assert_eq!(panel.edit_buf, "ab");
}

#[test]
fn handle_backspace_not_editing_is_noop() {
    let mut panel = make_panel();
    panel.handle_backspace(); // should not panic
    assert!(panel.edit_buf.is_empty());
}

#[test]
fn handle_escape_while_editing_cancels() {
    let mut panel = make_panel();
    panel.editing = true;
    panel.edit_buf = "something".to_string();
    let action = panel.handle_escape();
    assert!(!panel.editing);
    assert!(panel.edit_buf.is_empty());
    assert!(matches!(action, ConfigAction::None));
}

#[test]
fn handle_char_escape_while_editing_cancels() {
    let mut panel = make_panel();
    panel.handle_char('i');
    panel.edit_buf = "test".to_string();
    panel.handle_char('\x1b');
    assert!(!panel.editing);
    assert!(panel.edit_buf.is_empty());
}

#[test]
fn confirm_edit_valid_value_updates_field() {
    let mut panel = make_panel();
    panel.selected = F_FONT_SIZE;
    panel.editing = true;
    panel.edit_buf = "20.0".to_string();
    panel.handle_char('\r'); // confirm
    assert!(!panel.editing);
    assert_eq!(panel.fields[F_FONT_SIZE].value, "20.0");
}

#[test]
fn confirm_edit_invalid_value_sets_status() {
    let mut panel = make_panel();
    panel.selected = F_FONT_SIZE;
    panel.editing = true;
    panel.edit_buf = "notanumber".to_string();
    panel.handle_char('\r');
    assert!(panel.editing); // stays in editing mode
    assert!(panel.status.is_some());
}

#[test]
fn confirm_edit_hex_color_normalizes() {
    let mut panel = make_panel();
    panel.selected = F_COLOR_BG;
    panel.editing = true;
    panel.edit_buf = "ff0000".to_string(); // without #
    panel.handle_char('\r');
    assert_eq!(panel.fields[F_COLOR_BG].value, "#FF0000");
}

// ── display_value ─────────────────────────────────────────────────────────────

#[test]
fn display_value_shows_edit_buf_when_editing_selected() {
    let mut panel = make_panel();
    panel.editing = true;
    panel.edit_buf = "draft".to_string();
    assert_eq!(panel.display_value(0), "draft");
}

#[test]
fn display_value_shows_field_value_for_other_fields() {
    let mut panel = make_panel();
    panel.editing = true;
    panel.selected = 0;
    assert_eq!(panel.display_value(1), panel.fields[1].value.as_str());
}

#[test]
fn display_value_shows_field_value_when_not_editing() {
    let panel = make_panel();
    assert_eq!(panel.display_value(0), panel.fields[0].value.as_str());
}

// ── save / build_config ───────────────────────────────────────────────────────

#[test]
fn save_valid_config_returns_save_action() {
    let mut panel = make_panel();
    let action = panel.save();
    assert!(matches!(action, ConfigAction::Save(_)));
    assert!(panel.status.is_some());
}

#[test]
fn save_while_editing_confirms_first() {
    let mut panel = make_panel();
    panel.selected = F_FONT_SIZE;
    panel.editing = true;
    panel.edit_buf = "18.0".to_string();
    let action = panel.save();
    assert!(!panel.editing);
    assert!(matches!(action, ConfigAction::Save(_)));
}

#[test]
fn save_invalid_field_returns_none_with_error_status() {
    let mut panel = make_panel();
    panel.fields[F_FONT_FAMILY].value = String::new(); // empty family = invalid
    let action = panel.save();
    assert!(matches!(action, ConfigAction::None));
    assert!(panel.status.as_deref().unwrap_or("").contains("Error"));
}

#[test]
fn build_config_roundtrip_preserves_font_size() {
    let cfg = Config::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    if let ConfigAction::Save(rebuilt) = panel.save() {
        assert_eq!(rebuilt.font.size, cfg.font.size);
        assert_eq!(rebuilt.window.width, cfg.window.width);
    } else {
        panic!("expected Save action");
    }
}

#[test]
fn build_config_shell_empty_becomes_none() {
    let mut panel = make_panel();
    panel.fields[F_SHELL].value = String::new();
    if let ConfigAction::Save(cfg) = panel.save() {
        assert!(cfg.shell.program.is_none());
    } else {
        panic!("expected Save action");
    }
}

#[test]
fn build_config_shell_nonempty_becomes_some() {
    let mut panel = make_panel();
    panel.fields[F_SHELL].value = "/bin/zsh".to_string();
    if let ConfigAction::Save(cfg) = panel.save() {
        assert_eq!(cfg.shell.program, Some("/bin/zsh".to_string()));
    } else {
        panic!("expected Save action");
    }
}

// ── Validation helpers ────────────────────────────────────────────────────────

#[test]
fn validate_text_empty_fails() {
    let mut panel = make_panel();
    panel.selected = F_FONT_FAMILY; // Text kind
    panel.editing = true;
    panel.edit_buf = String::new();
    panel.handle_char('\r');
    assert!(panel.status.is_some()); // validation failed
}

#[test]
fn validate_opt_text_empty_passes() {
    let mut panel = make_panel();
    panel.selected = F_SHELL; // OptText kind
    panel.editing = true;
    panel.edit_buf = String::new();
    panel.handle_char('\r');
    assert!(!panel.editing); // confirmed
}

#[test]
fn validate_float_negative_fails() {
    let mut panel = make_panel();
    panel.selected = F_FONT_SIZE;
    panel.editing = true;
    panel.edit_buf = "-1.0".to_string();
    panel.handle_char('\r');
    assert!(panel.status.is_some());
}

#[test]
fn validate_uint_zero_fails() {
    let mut panel = make_panel();
    panel.selected = F_WIN_WIDTH;
    panel.editing = true;
    panel.edit_buf = "0".to_string();
    panel.handle_char('\r');
    assert!(panel.status.is_some());
}

#[test]
fn validate_hex_invalid_fails() {
    let mut panel = make_panel();
    panel.selected = F_COLOR_BG;
    panel.editing = true;
    panel.edit_buf = "zzzzzz".to_string();
    panel.handle_char('\r');
    assert!(panel.status.is_some());
}

#[test]
fn validate_hex_valid_with_hash() {
    let mut panel = make_panel();
    panel.selected = F_COLOR_BG;
    panel.editing = true;
    panel.edit_buf = "#aabbcc".to_string();
    panel.handle_char('\r');
    assert!(!panel.editing);
    assert_eq!(panel.fields[F_COLOR_BG].value, "#AABBCC");
}

#[test]
fn validate_bool_true_passes() {
    let mut panel = make_panel();
    panel.selected = F_DETECT_URLS; // Bool kind
    panel.editing = true;
    panel.edit_buf = "true".to_string();
    panel.handle_char('\r');
    assert!(!panel.editing);
}

#[test]
fn validate_bool_false_passes() {
    let mut panel = make_panel();
    panel.selected = F_DETECT_URLS;
    panel.editing = true;
    panel.edit_buf = "false".to_string();
    panel.handle_char('\r');
    assert!(!panel.editing);
}

#[test]
fn validate_bool_invalid_fails() {
    let mut panel = make_panel();
    panel.selected = F_DETECT_URLS;
    panel.editing = true;
    panel.edit_buf = "yes".to_string();
    panel.handle_char('\r');
    assert!(panel.status.is_some());
}

#[test]
fn build_config_empty_font_family_returns_error() {
    let mut panel = make_panel();
    panel.fields[F_FONT_FAMILY].value = String::new();
    let action = panel.save();
    assert!(matches!(action, ConfigAction::None));
    assert!(panel.status.as_deref().unwrap_or("").contains("Error"));
}

#[test]
fn build_config_zero_font_size_returns_error() {
    let mut panel = make_panel();
    // bypass validate() by writing directly — tests the <= 0 guard in build_config
    panel.fields[F_FONT_SIZE].value = "0.0".to_string();
    let action = panel.save();
    assert!(matches!(action, ConfigAction::None));
    assert!(panel.status.as_deref().unwrap_or("").contains("Error"));
}
