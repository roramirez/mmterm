use super::*;

#[test]
fn default_config_parses_successfully() {
    let cfg = Config::default();
    assert_eq!(cfg.font.size, 16.0);
    assert_eq!(cfg.window.width, 800);
    assert_eq!(cfg.window.height, 600);
    assert_eq!(cfg.window.cursor_blink_ms, 500);
    assert_eq!(cfg.window.inactive_dim, 0.55);
    assert_eq!(cfg.colors.palette.len(), 16);
}

#[test]
fn parse_hex_rrggbb() {
    let c = parse_hex("#ff8800");
    assert_eq!(c.r, 0xff);
    assert_eq!(c.g, 0x88);
    assert_eq!(c.b, 0x00);
}

#[test]
fn parse_hex_without_hash() {
    let c = parse_hex("1a2b3c");
    assert_eq!(c.r, 0x1a);
    assert_eq!(c.g, 0x2b);
    assert_eq!(c.b, 0x3c);
}

#[test]
fn parse_hex_invalid_returns_black() {
    let c = parse_hex("zzzzzz");
    assert_eq!(c.r, 0);
    assert_eq!(c.g, 0);
    assert_eq!(c.b, 0);
}

#[test]
fn colors_config_bg_fg_cursor_selection() {
    let cfg = Config::default();
    let bg = cfg.colors.bg();
    let fg = cfg.colors.fg();
    let cursor = cfg.colors.cursor();
    let selection = cfg.colors.selection();
    assert_eq!(bg, parse_hex("#121212"));
    assert_eq!(fg, parse_hex("#a0a0a0"));
    assert_eq!(cursor, parse_hex("#bbbbbb"));
    assert_eq!(selection, parse_hex("#3d3d3d"));
}

#[test]
fn palette_colors_returns_16_entries() {
    let cfg = Config::default();
    let palette = cfg.colors.palette_colors();
    assert_eq!(palette.len(), 16);
    assert_eq!(palette[0], parse_hex("#1b1d1e"));
    assert_eq!(palette[15], parse_hex("#f8f8f2"));
}

#[test]
fn palette_colors_truncates_at_16() {
    let mut cfg = Config::default();
    // push extra entries — should be ignored
    cfg.colors.palette.push("#ffffff".to_string());
    cfg.colors.palette.push("#ffffff".to_string());
    let palette = cfg.colors.palette_colors();
    assert_eq!(palette.len(), 16);
}

#[test]
fn toml_roundtrip_preserves_values() {
    let original = Config::default();
    let serialized = toml::to_string_pretty(&original).expect("serialize failed");
    let restored: Config = toml::from_str(&serialized).expect("deserialize failed");
    assert_eq!(restored.font.size, original.font.size);
    assert_eq!(restored.window.width, original.window.width);
    assert_eq!(restored.colors.background, original.colors.background);
}

#[test]
fn individual_default_impls() {
    let _ = FontConfig::default();
    let _ = WindowConfig::default();
    let _ = ShellConfig::default();
    let _ = ColorsConfig::default();
}

#[test]
fn write_default_if_missing_does_not_panic() {
    // If file exists → returns early (line 134). If not → creates it.
    // Either path should not panic.
    Config::write_default_if_missing();
}

#[test]
fn load_falls_back_to_defaults_when_no_file() {
    // In a test environment there's no ~/.config/mmterm/config.toml;
    // load() should return defaults without panicking.
    let cfg = Config::load();
    assert!(cfg.font.size > 0.0);
    assert!(cfg.window.width > 0);
}

#[test]
fn inactive_dim_default_applied_when_missing() {
    let toml = r###"
[font]
family = "Mono"
size = 14.0
[window]
width = 800
height = 600
title = "t"
cursor_blink_ms = 500
[shell]
[colors]
background = "#000000"
foreground = "#ffffff"
cursor = "#ffffff"
selection = "#333333"
palette = []
"###;
    let cfg: Config = toml::from_str(toml).expect("parse failed");
    assert_eq!(cfg.window.inactive_dim, 0.55);
}

#[test]
fn default_inactive_dim_value() {
    assert_eq!(default_inactive_dim(), 0.55);
}

#[test]
fn default_detect_urls_value() {
    assert!(default_detect_urls());
}

#[test]
fn detect_urls_default_applied_when_missing() {
    let toml = r###"
[font]
family = "Mono"
size = 14.0
[window]
width = 800
height = 600
title = "t"
cursor_blink_ms = 500
[shell]
[colors]
background = "#000000"
foreground = "#ffffff"
cursor = "#ffffff"
selection = "#333333"
palette = []
"###;
    let cfg: Config = toml::from_str(toml).expect("parse failed");
    assert!(cfg.window.detect_urls);
}

#[test]
fn save_does_not_panic() {
    Config::default().save();
}

#[test]
fn default_auto_log_is_false() {
    let cfg = Config::default();
    assert!(!cfg.logging.auto_log);
}

#[test]
fn default_log_dir_is_empty() {
    let cfg = Config::default();
    assert!(cfg.logging.log_dir.is_empty());
}

#[test]
fn logging_section_parsed_from_toml() {
    let toml = r###"
[font]
family = "Mono"
size = 14.0
[window]
width = 800
height = 600
title = "t"
cursor_blink_ms = 500
[shell]
[logging]
auto_log = true
log_dir  = "/tmp/logs"
[colors]
background = "#000000"
foreground = "#ffffff"
cursor = "#ffffff"
selection = "#333333"
palette = []
"###;
    let cfg: Config = toml::from_str(toml).expect("parse failed");
    assert!(cfg.logging.auto_log);
    assert_eq!(cfg.logging.log_dir, "/tmp/logs");
}

#[test]
fn logging_defaults_when_section_missing() {
    let toml = r###"
[font]
family = "Mono"
size = 14.0
[window]
width = 800
height = 600
title = "t"
cursor_blink_ms = 500
[shell]
[colors]
background = "#000000"
foreground = "#ffffff"
cursor = "#ffffff"
selection = "#333333"
palette = []
"###;
    let cfg: Config = toml::from_str(toml).expect("parse failed");
    assert!(!cfg.logging.auto_log);
    assert!(cfg.logging.log_dir.is_empty());
}

#[test]
fn default_scrollback_lines_value() {
    assert_eq!(default_scrollback_lines(), 10_000);
}

#[test]
fn default_log_dir_is_empty_string() {
    assert!(default_log_dir().is_empty());
}

#[test]
fn config_roundtrip_preserves_scrollback_lines() {
    let mut cfg = Config::default();
    cfg.terminal.scrollback_lines = 5000;
    let s = toml::to_string_pretty(&cfg).expect("serialize failed");
    let restored: Config = toml::from_str(&s).expect("deserialize failed");
    assert_eq!(restored.terminal.scrollback_lines, 5000);
}

#[test]
fn terminal_config_default_scrollback_is_ten_thousand() {
    let tc = TerminalConfig::default();
    assert_eq!(tc.scrollback_lines, 10_000);
}

#[test]
fn default_status_bar_right_returns_pwd_token() {
    assert_eq!(default_status_bar_right(), "%pwd");
}

#[test]
fn status_bar_config_default_has_pwd_token() {
    let cfg = StatusBarConfig::default();
    assert_eq!(cfg.right, "%pwd");
}

#[test]
fn status_bar_right_parses_string() {
    let toml = r#"right = "%pwd  %date{%H:%M}""#;
    let cfg: StatusBarConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.right, "%pwd  %date{%H:%M}");
}

#[test]
fn status_bar_right_preserves_spaces() {
    let toml = r#"right = "%pwd  |  %date{%H:%M}""#;
    let cfg: StatusBarConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.right, "%pwd  |  %date{%H:%M}");
}

#[test]
fn general_update_defaults() {
    let cfg = Config::default();
    assert!(cfg.general.auto_update_check); // daily check on by default
    assert!(!cfg.general.auto_update_install); // silent self-replace opt-in (off)
}
