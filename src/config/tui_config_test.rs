use super::*;
use crate::config::Config;

fn make_panel() -> ConfigPanel {
    ConfigPanel::from_config(&Config::default())
}

// ── from_config ───────────────────────────────────────────────────────────────

#[test]
fn from_config_has_correct_field_count() {
    let panel = make_panel();
    // 9 base + 1 scrollback + 2 logging + 1 theme + 4 colors + 16 palette + 1 status_bar + 3 general + 2 updates + 2 shell/notify + 1 opacity = 42
    assert_eq!(panel.fields.len(), 42);
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

/// A Config whose every field carries a distinct, non-default value, so that a
/// mis-routed `F_*` index (reading/writing the wrong field) changes the
/// round-trip result and is caught below.
fn distinct_config() -> Config {
    Config {
        font: FontConfig {
            family: "TestMono".into(),
            size: 13.5,
        },
        window: WindowConfig {
            width: 801,
            height: 607,
            title: "title-x".into(),
            cursor_blink_ms: 523,
            inactive_dim: 0.42,
            detect_urls: true,
            opacity: 0.8,
        },
        shell: ShellConfig {
            program: Some("/bin/xyzsh".into()),
        },
        terminal: TerminalConfig {
            scrollback_lines: 4097,
        },
        logging: LogConfig {
            auto_log: false,
            log_dir: "/var/log/mmterm-x".into(),
        },
        colors: ColorsConfig {
            background: "#010203".into(),
            foreground: "#040506".into(),
            cursor: "#070809".into(),
            selection: "#0A0B0C".into(),
            palette: (0..16).map(|i| format!("#{:02X}0000", i + 1)).collect(),
        },
        theme: ThemeConfig {
            name: "customtheme".into(),
        },
        status_bar: StatusBarConfig {
            right: "%pwd-x".into(),
        },
        general: GeneralConfig {
            restore_session: false,
            screenshot_dir: "/shots-x".into(),
            visual_bell: true,
            auto_update_check: true,
            auto_update_install: false,
            shell_integration: true,
            desktop_notifications: true,
        },
    }
}

/// Guard against silent desync between the `F_*` constants, the field order in
/// `from_config()`, and the index reads in `build_config()`. CLAUDE.md marks
/// this as a critical invariant with no runtime validation.
#[test]
fn field_index_sanity() {
    // Every named F_* constant, with F_PALETTE expanded to its 16 contiguous
    // slots. If a constant is added/removed without adjusting the others, the
    // contiguity assertion below fails.
    let mut occupied: Vec<usize> = vec![
        F_RESTORE_SESSION,
        F_SCREENSHOT_DIR,
        F_VISUAL_BELL,
        F_FONT_FAMILY,
        F_FONT_SIZE,
        F_WIN_WIDTH,
        F_WIN_HEIGHT,
        F_WIN_TITLE,
        F_BLINK_MS,
        F_DIM,
        F_DETECT_URLS,
        F_SHELL,
        F_SCROLLBACK,
        F_LOG_AUTO,
        F_LOG_DIR,
        F_THEME_NAME,
        F_COLOR_BG,
        F_COLOR_FG,
        F_COLOR_CUR,
        F_COLOR_SEL,
        F_STATUS_BAR_RIGHT,
        F_AUTO_UPDATE_CHECK,
        F_AUTO_UPDATE_INSTALL,
        F_SHELL_INTEGRATION,
        F_DESKTOP_NOTIFICATIONS,
        F_OPACITY,
    ];
    occupied.extend((0..16).map(|i| F_PALETTE + i));
    occupied.sort_unstable();

    // Contiguous 0,1,2,…,N-1 with no gaps and no overlaps (duplicate index).
    let expected: Vec<usize> = (0..occupied.len()).collect();
    assert_eq!(
        occupied, expected,
        "F_* indices must be contiguous with no gaps or overlapping slots"
    );

    // The index space must exactly cover the field vec built by from_config().
    let panel = ConfigPanel::from_config(&Config::default());
    assert_eq!(
        occupied.len(),
        panel.fields.len(),
        "number of F_* slots must equal from_config().fields.len()"
    );
}

/// Full round-trip: `build_config(from_config(cfg)) == cfg` for a config whose
/// fields are all distinct. Catches a field read at the wrong `F_*` index even
/// when the count and contiguity still line up.
#[test]
fn build_config_full_roundtrip_preserves_every_field() {
    let cfg = distinct_config();
    let panel = ConfigPanel::from_config(&cfg);
    let rebuilt = panel.build_config().expect("distinct_config must rebuild");
    assert_eq!(
        rebuilt, cfg,
        "every field must survive the from/build round-trip"
    );
}

#[test]
fn build_config_roundtrip_toggles_shell_integration() {
    let mut panel = make_panel();
    assert_eq!(panel.fields[F_SHELL_INTEGRATION].value, "true");
    panel.fields[F_SHELL_INTEGRATION].value = "false".to_string();
    if let ConfigAction::Save(cfg) = panel.save() {
        assert!(!cfg.general.shell_integration);
    } else {
        panic!("expected Save action");
    }
}

#[test]
fn build_config_roundtrip_toggles_desktop_notifications() {
    let mut panel = make_panel();
    assert_eq!(panel.fields[F_DESKTOP_NOTIFICATIONS].value, "true");
    panel.fields[F_DESKTOP_NOTIFICATIONS].value = "false".to_string();
    if let ConfigAction::Save(cfg) = panel.save() {
        assert!(!cfg.general.desktop_notifications);
    } else {
        panic!("expected Save action");
    }
}

#[test]
fn build_config_preserves_opacity() {
    let mut panel = make_panel();
    panel.fields[F_OPACITY].value = "0.75".to_string();
    if let ConfigAction::Save(cfg) = panel.save() {
        assert_eq!(cfg.window.opacity, 0.75);
    } else {
        panic!("expected Save action");
    }
}

#[test]
fn build_config_clamps_opacity_above_one() {
    let mut panel = make_panel();
    panel.fields[F_OPACITY].value = "2.5".to_string();
    let cfg = panel
        .build_config()
        .expect("opacity clamps, does not error");
    assert_eq!(cfg.window.opacity, 1.0);
}

#[test]
fn build_config_clamps_opacity_below_zero() {
    let mut panel = make_panel();
    panel.fields[F_OPACITY].value = "-1.0".to_string();
    let cfg = panel
        .build_config()
        .expect("opacity clamps, does not error");
    assert_eq!(cfg.window.opacity, 0.0);
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

#[test]
fn build_config_zero_scrollback_returns_error() {
    let mut panel = make_panel();
    panel.fields[F_SCROLLBACK].value = "0".to_string();
    let action = panel.save();
    assert!(matches!(action, ConfigAction::None));
    assert!(panel.status.as_deref().unwrap_or("").contains("Error"));
}

#[test]
fn field_select_cycles_forward() {
    let mut panel = make_panel();
    panel.selected = F_THEME_NAME;
    panel.fields[F_THEME_NAME].kind =
        FieldKind::Select(vec!["alpha".to_string(), "beta".to_string()]);
    panel.fields[F_THEME_NAME].value = "alpha".to_string();
    let action = panel.handle_right();
    assert!(matches!(action, ConfigAction::PreviewTheme(ref n) if n == "beta"));
    assert_eq!(panel.fields[F_THEME_NAME].value, "beta");
}

#[test]
fn field_select_cycles_backward_wraps() {
    let mut panel = make_panel();
    panel.selected = F_THEME_NAME;
    panel.fields[F_THEME_NAME].kind =
        FieldKind::Select(vec!["alpha".to_string(), "beta".to_string()]);
    panel.fields[F_THEME_NAME].value = "alpha".to_string();
    let action = panel.handle_left();
    assert!(matches!(action, ConfigAction::PreviewTheme(ref n) if n == "beta"));
    assert_eq!(panel.fields[F_THEME_NAME].value, "beta");
}

#[test]
fn field_select_cycles_forward_wraps_at_end() {
    let mut panel = make_panel();
    panel.selected = F_THEME_NAME;
    panel.fields[F_THEME_NAME].kind =
        FieldKind::Select(vec!["alpha".to_string(), "beta".to_string()]);
    panel.fields[F_THEME_NAME].value = "beta".to_string();
    let action = panel.handle_right();
    assert!(matches!(action, ConfigAction::PreviewTheme(ref n) if n == "alpha"));
}

#[test]
fn handle_right_on_non_select_field_returns_none() {
    let mut panel = make_panel();
    panel.selected = F_FONT_SIZE;
    let action = panel.handle_right();
    assert!(matches!(action, ConfigAction::None));
}

#[test]
fn handle_left_on_non_select_field_returns_none() {
    let mut panel = make_panel();
    panel.selected = F_FONT_SIZE;
    let action = panel.handle_left();
    assert!(matches!(action, ConfigAction::None));
}

#[test]
fn build_config_preserves_selected_theme_name() {
    let mut panel = make_panel();
    panel.selected = F_THEME_NAME;
    panel.fields[F_THEME_NAME].kind =
        FieldKind::Select(vec!["default".to_string(), "custom".to_string()]);
    panel.fields[F_THEME_NAME].value = "custom".to_string();
    if let ConfigAction::Save(cfg) = panel.save() {
        assert_eq!(cfg.theme.name, "custom");
    } else {
        panic!("expected Save action");
    }
}

#[test]
fn validate_select_kind_always_passes() {
    let mut panel = make_panel();
    panel.selected = F_THEME_NAME;
    panel.fields[F_THEME_NAME].kind =
        FieldKind::Select(vec!["default".to_string(), "other".to_string()]);
    panel.fields[F_THEME_NAME].value = "other".to_string();
    // Manually enter editing mode (bypasses start_edit guard) and confirm.
    panel.editing = true;
    panel.edit_buf = "other".to_string();
    panel.handle_char('\r');
    assert!(!panel.editing);
    assert_eq!(panel.fields[F_THEME_NAME].value, "other");
}

#[test]
fn cycle_select_empty_options_returns_none() {
    let mut panel = make_panel();
    // Replace the Theme field's option list with an empty vec.
    panel.selected = F_THEME_NAME;
    if let FieldKind::Select(ref mut opts) = panel.fields[F_THEME_NAME].kind {
        opts.clear();
    }
    // Both directions should return ConfigAction::None without panicking.
    assert!(matches!(panel.handle_left(), ConfigAction::None));
    assert!(matches!(panel.handle_right(), ConfigAction::None));
}

// ── Collapse / expand ─────────────────────────────────────────────────────────

#[test]
fn palette_collapsed_by_default() {
    let panel = make_panel();
    assert!(panel.collapsed.contains("Palette"));
}

#[test]
fn visible_indices_hides_palette_body() {
    let panel = make_panel();
    // 42 total - 15 palette body fields = 27 visible
    assert_eq!(panel.visible_indices().len(), 27);
}

#[test]
fn toggle_on_palette_header_expands() {
    let mut panel = make_panel();
    panel.selected = F_PALETTE;
    panel.toggle_collapse();
    assert!(!panel.collapsed.contains("Palette"));
    assert_eq!(panel.visible_indices().len(), 42);
}

#[test]
fn toggle_twice_restores_collapsed() {
    let mut panel = make_panel();
    panel.selected = F_PALETTE;
    panel.toggle_collapse();
    panel.toggle_collapse();
    assert!(panel.collapsed.contains("Palette"));
    assert_eq!(panel.visible_indices().len(), 27);
}

#[test]
fn space_key_on_section_header_toggles() {
    let mut panel = make_panel();
    panel.selected = F_PALETTE;
    let before = panel.collapsed.contains("Palette");
    panel.handle_char(' ');
    assert_ne!(panel.collapsed.contains("Palette"), before);
}

#[test]
fn space_key_on_non_section_row_is_noop() {
    let mut panel = make_panel();
    panel.selected = F_FONT_SIZE; // no section on this field
    let count_before = panel.visible_indices().len();
    panel.handle_char(' ');
    assert_eq!(panel.visible_indices().len(), count_before);
}

#[test]
fn space_while_editing_goes_to_buf() {
    let mut panel = make_panel();
    panel.handle_char('i'); // start editing
    panel.edit_buf.clear();
    panel.handle_char(' ');
    assert_eq!(panel.edit_buf, " ");
    assert!(panel.collapsed.contains("Palette")); // unchanged
}

// ── Navigation with collapsed sections ───────────────────────────────────────

#[test]
fn move_down_skips_collapsed_palette() {
    let mut panel = make_panel();
    // palette is collapsed by default; move to the palette header
    panel.selected = F_PALETTE;
    panel.handle_char('j');
    // next visible after the palette header is F_STATUS_BAR_RIGHT
    assert_eq!(panel.selected, F_STATUS_BAR_RIGHT);
}

#[test]
fn move_up_skips_collapsed_palette() {
    let mut panel = make_panel();
    panel.selected = F_STATUS_BAR_RIGHT;
    panel.handle_char('k');
    // previous visible before Status Bar right is the palette header (F_PALETTE)
    assert_eq!(panel.selected, F_PALETTE);
}

#[test]
fn move_down_at_last_visible_clamps() {
    let mut panel = make_panel();
    // F_OPACITY is the last field and is always visible
    panel.selected = F_OPACITY;
    panel.handle_down();
    assert_eq!(panel.selected, F_OPACITY);
}

#[test]
fn move_up_at_first_visible_clamps() {
    let mut panel = make_panel();
    panel.selected = 0;
    panel.handle_up();
    assert_eq!(panel.selected, 0);
}

// ── Section jump ─────────────────────────────────────────────────────────────

#[test]
fn jump_forward_from_font_lands_on_window() {
    let mut panel = make_panel();
    panel.selected = F_FONT_FAMILY;
    panel.jump_section_forward();
    assert_eq!(panel.selected, F_WIN_WIDTH);
}

#[test]
fn jump_backward_from_window_lands_on_font() {
    let mut panel = make_panel();
    panel.selected = F_WIN_WIDTH;
    panel.jump_section_backward();
    assert_eq!(panel.selected, F_FONT_FAMILY);
}

#[test]
fn jump_forward_wraps_at_last_section() {
    let mut panel = make_panel();
    panel.selected = F_AUTO_UPDATE_CHECK; // Updates is the last section
    panel.jump_section_forward();
    assert_eq!(panel.selected, F_RESTORE_SESSION); // wraps to first (General)
}

#[test]
fn jump_backward_wraps_at_first_section() {
    let mut panel = make_panel();
    panel.selected = F_RESTORE_SESSION; // General is the first section
    panel.jump_section_backward();
    assert_eq!(panel.selected, F_AUTO_UPDATE_CHECK); // wraps to last (Updates)
}

// ── section_of helper ─────────────────────────────────────────────────────────

#[test]
fn section_of_header_field_returns_own_section() {
    let panel = make_panel();
    assert_eq!(panel.section_of(F_FONT_FAMILY), Some("Font"));
    assert_eq!(panel.section_of(F_PALETTE), Some("Palette"));
}

#[test]
fn section_of_body_field_returns_enclosing_section() {
    let panel = make_panel();
    assert_eq!(panel.section_of(F_FONT_SIZE), Some("Font"));
    assert_eq!(panel.section_of(F_PALETTE + 3), Some("Palette"));
}

// ── collapsed_count ──────────────────────────────────────────────────────────

#[test]
fn collapsed_count_palette_is_15() {
    let panel = make_panel();
    assert_eq!(panel.collapsed_count("Palette"), 15);
}

#[test]
fn collapsed_count_font_is_1() {
    let panel = make_panel();
    // Font section has F_FONT_FAMILY (header) + F_FONT_SIZE (body)
    assert_eq!(panel.collapsed_count("Font"), 1);
}
