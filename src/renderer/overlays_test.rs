use std::collections::HashSet;

use super::*;
use crate::config::KeybindingsConfig;
use crate::config::tui_config::{ConfigPanel, Field, FieldKind};

fn make_panel(value: &str, kind: FieldKind) -> ConfigPanel {
    ConfigPanel {
        fields: vec![Field {
            label: "Test",
            hint: "hint",
            value: value.to_string(),
            kind,
            section: None,
        }],
        selected: 0,
        editing: false,
        edit_buf: String::new(),
        status: None,
        collapsed: HashSet::new(),
        keybindings: KeybindingsConfig::default(),
    }
}

// ── field_value_display ───────────────────────────────────────────────────────

#[test]
fn field_value_display_text_not_editing_no_cursor() {
    let panel = make_panel("hello", FieldKind::Text);
    let s = field_value_display(&panel, 0, false, false, false);
    assert_eq!(s, "hello");
}

#[test]
fn field_value_display_text_editing_appends_cursor() {
    let panel = make_panel("hello", FieldKind::Text);
    let s = field_value_display(&panel, 0, false, false, true);
    assert_eq!(s, "hello_");
}

#[test]
fn field_value_display_select_not_selected_shows_plain() {
    let options = vec!["a".to_string(), "b".to_string()];
    let panel = make_panel("a", FieldKind::Select(options));
    // is_select=true but is_sel=false → plain value
    let s = field_value_display(&panel, 0, true, false, false);
    assert_eq!(s, "a");
}

#[test]
fn field_value_display_select_selected_shows_arrows() {
    let options = vec!["dark".to_string(), "light".to_string()];
    let panel = make_panel("dark", FieldKind::Select(options));
    let s = field_value_display(&panel, 0, true, true, false);
    assert_eq!(s, "\u{2190} dark \u{2192}");
}

#[test]
fn field_value_display_select_selected_no_cursor_even_when_editing() {
    let options = vec!["dark".to_string()];
    let panel = make_panel("dark", FieldKind::Select(options));
    // Select + selected → arrows, no cursor even if is_editing=true
    let s = field_value_display(&panel, 0, true, true, true);
    assert_eq!(s, "\u{2190} dark \u{2192}");
    assert!(!s.contains('_'));
}

// ── draw_hex_color_swatch ─────────────────────────────────────────────────────

fn make_layout(bw: u32) -> FieldRowLayout {
    FieldRowLayout {
        px: 10,
        panel_w: bw - 20,
        pad: 10,
        cw: 8,
        fp: 14.0,
        row_h: 20,
        bg: 0xff_00_00_00,
        border: 0xff_ff_ff_ff,
        sel: 0,
    }
}

#[test]
fn draw_hex_color_swatch_non_hexcolor_field_leaves_buffer_unchanged() {
    let bw = 400u32;
    let bh = 100u32;
    let mut buf = vec![0u32; (bw * bh) as usize];
    let panel = make_panel("hello", FieldKind::Text);
    let l = make_layout(bw);
    draw_hex_color_swatch(&mut buf, bw, &panel, 0, 5, &l);
    assert!(
        buf.iter().all(|&p| p == 0),
        "non-HexColor must not touch buffer"
    );
}

#[test]
fn draw_hex_color_swatch_invalid_hex_leaves_buffer_unchanged() {
    let bw = 400u32;
    let bh = 100u32;
    let mut buf = vec![0u32; (bw * bh) as usize];
    let panel = make_panel("gggggg", FieldKind::HexColor);
    let l = make_layout(bw);
    draw_hex_color_swatch(&mut buf, bw, &panel, 0, 5, &l);
    assert!(
        buf.iter().all(|&p| p == 0),
        "invalid hex must not touch buffer"
    );
}

#[test]
fn draw_hex_color_swatch_valid_hex_fills_swatch_pixels() {
    let bw = 400u32;
    let bh = 100u32;
    let mut buf = vec![0u32; (bw * bh) as usize];
    // Red: #FF0000 → packed as 0xff_ff_00_00
    let panel = make_panel("#FF0000", FieldKind::HexColor);
    let l = make_layout(bw);
    let draw_y = 10u32;
    draw_hex_color_swatch(&mut buf, bw, &panel, 0, draw_y, &l);

    // The swatch is drawn at x = px + panel_w - pad - 10, y = draw_y + 2, 8×(row_h-4)
    let swatch_x = l.px + l.panel_w - l.pad - 10;
    let swatch_y = draw_y + 2;
    let expected_color = 0xff_00_00_00 | 0xff_00_00u32; // 0xff_ff_00_00
    let idx = (swatch_y * bw + swatch_x) as usize;
    assert_eq!(
        buf[idx], expected_color,
        "swatch pixel must have the hex color with full alpha"
    );
}

#[test]
fn draw_hex_color_swatch_without_hash_prefix_also_works() {
    let bw = 400u32;
    let bh = 100u32;
    let mut buf = vec![0u32; (bw * bh) as usize];
    // Green without # prefix
    let panel = make_panel("00FF00", FieldKind::HexColor);
    let l = make_layout(bw);
    let draw_y = 5u32;
    draw_hex_color_swatch(&mut buf, bw, &panel, 0, draw_y, &l);

    let swatch_x = l.px + l.panel_w - l.pad - 10;
    let swatch_y = draw_y + 2;
    let expected_color = 0xff_00_00_00 | 0x00_ff_00u32;
    let idx = (swatch_y * bw + swatch_x) as usize;
    assert_eq!(buf[idx], expected_color);
}

// ── hint_text_y ───────────────────────────────────────────────────────────────

#[test]
fn hint_text_y_places_below_rect_when_room() {
    // bottom + 4 + line_h <= bh → use bottom + 4
    let y = hint_text_y(100, 200, 20, 400);
    assert_eq!(y, 204);
}

#[test]
fn hint_text_y_places_above_rect_when_below_clips() {
    // bottom=380, line_h=20, bh=400 → below clips (380+4+20=404 > 400)
    // top=10 >= line_h+4=24 → above: 10 - 20 - 4 < 0, so check: top(10) >= 24? No.
    // Fallback: bh.saturating_sub(line_h + 4) = 400-24 = 376
    // Actually: top=50 >= 24 → above = 50 - 20 - 4 = 26
    let y = hint_text_y(50, 380, 20, 400);
    assert_eq!(y, 26);
}

#[test]
fn hint_text_y_fallback_when_neither_above_nor_below_fits() {
    // bottom=390, line_h=20, bh=400 → below clips (414 > 400)
    // top=5, line_h+4=24 → top(5) < 24, so above doesn't fit
    // fallback: 400 - 24 = 376
    let y = hint_text_y(5, 390, 20, 400);
    assert_eq!(y, 376);
}

// ── Renderer overlay methods ──────────────────────────────────────────────────

fn make_renderer() -> crate::renderer::Renderer {
    crate::renderer::Renderer::new("JetBrainsMono", 16.0)
}

#[test]
fn draw_command_palette_empty_entries_does_not_panic() {
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    r.draw_command_palette(&mut buf, 800, 600, "", &[], 0);
}

#[test]
fn draw_command_palette_with_entries_does_not_panic() {
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    let entries = vec![
        ("Split Vertical", "Ctrl+W s"),
        ("New Tab", "Ctrl+T"),
        ("Quit", "Ctrl+Q"),
    ];
    r.draw_command_palette(&mut buf, 800, 600, "sp", &entries, 0);
    assert!(buf.iter().any(|&p| p != 0));
}

#[test]
fn draw_command_palette_selected_entry_differs_from_unselected() {
    let mut r = make_renderer();
    let entries = vec![("Split Vertical", "Ctrl+W s"), ("New Tab", "Ctrl+T")];

    let mut buf_sel0 = vec![0u32; 800 * 600];
    r.draw_command_palette(&mut buf_sel0, 800, 600, "", &entries, 0);

    let mut buf_sel1 = vec![0u32; 800 * 600];
    r.draw_command_palette(&mut buf_sel1, 800, 600, "", &entries, 1);

    assert_ne!(
        buf_sel0, buf_sel1,
        "selected row should produce different pixels"
    );
}

#[test]
fn draw_screenshot_selector_does_not_panic() {
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    r.draw_screenshot_selector(&mut buf, 800, 600, 400, 300, 100, 80);
}

#[test]
fn draw_screenshot_selector_dims_outside_rect() {
    let mut r = make_renderer();
    // Fill with white; after draw the outside should be dimmed.
    let mut buf = vec![0xff_ff_ff_ffu32; 800 * 600];
    r.draw_screenshot_selector(&mut buf, 800, 600, 400, 300, 50, 50);
    // Pixel at top-left corner (0, 0) is outside the selection → dimmed
    let top_left = buf[0];
    assert!(
        (top_left >> 16) & 0xFF < 0xFF,
        "outside pixel must be dimmed"
    );
}

#[test]
fn draw_screenshot_selector_zero_size_does_not_panic() {
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    // half_w=0, half_h=0 → sel_w=0, sel_h=0 → draw_selection_border exits early
    r.draw_screenshot_selector(&mut buf, 800, 600, 400, 300, 0, 0);
}

#[test]
fn draw_save_session_confirm_does_not_panic() {
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    let theme = crate::theme::default_theme();
    r.draw_save_session_confirm(&mut buf, 800, 600, &theme);
    assert!(buf.iter().any(|&p| p != 0));
}

#[test]
fn draw_save_session_confirm_dims_background() {
    let mut r = make_renderer();
    let theme = crate::theme::default_theme();
    let mut buf = vec![0xff_80_80_80u32; 800 * 600];
    r.draw_save_session_confirm(&mut buf, 800, 600, &theme);
    assert!(buf.iter().any(|&p| ((p >> 16) & 0xFF) < 0x80));
}

// ── collapse_indicator ────────────────────────────────────────────────────────

#[test]
fn collapse_indicator_collapsed_returns_plus() {
    assert_eq!(collapse_indicator(true), "[+]");
}

#[test]
fn collapse_indicator_expanded_returns_minus() {
    assert_eq!(collapse_indicator(false), "[-]");
}

// ── config_panel_hint ─────────────────────────────────────────────────────────

fn make_section_panel(collapsed: bool) -> ConfigPanel {
    use crate::config::tui_config::Field;
    let mut c = HashSet::new();
    if collapsed {
        c.insert("General");
    }
    ConfigPanel {
        fields: vec![Field {
            label: "Restore Session",
            hint: "restore on launch",
            value: "true".to_string(),
            kind: FieldKind::Bool,
            section: Some("General"),
        }],
        selected: 0,
        editing: false,
        edit_buf: String::new(),
        status: None,
        collapsed: c,
        keybindings: KeybindingsConfig::default(),
    }
}

#[test]
fn config_panel_hint_non_section_field_shows_field_hint() {
    let panel = make_panel("hello", FieldKind::Text);
    let hint = config_panel_hint(&panel);
    assert!(
        hint.contains("hint:"),
        "expected 'hint:' prefix, got: {hint}"
    );
}

#[test]
fn config_panel_hint_collapsed_section_says_expand() {
    let panel = make_section_panel(true);
    let hint = config_panel_hint(&panel);
    assert!(
        hint.contains("expand"),
        "collapsed section should say 'expand', got: {hint}"
    );
}

#[test]
fn config_panel_hint_expanded_section_says_collapse() {
    let panel = make_section_panel(false);
    let hint = config_panel_hint(&panel);
    assert!(
        hint.contains("collapse"),
        "expanded section should say 'collapse', got: {hint}"
    );
}

// ── draw_screenshot_name_input ────────────────────────────────────────────────

#[test]
fn draw_screenshot_name_input_does_not_panic() {
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    r.draw_screenshot_name_input(&mut buf, 800, 600, 400, 300, 100, 80, "myshot");
}

#[test]
fn draw_screenshot_name_input_draws_something() {
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    r.draw_screenshot_name_input(&mut buf, 800, 600, 400, 300, 100, 80, "test");
    assert!(
        buf.iter().any(|&p| p != 0),
        "draw_screenshot_name_input must write at least one pixel"
    );
}

// ── panel_font_metrics ────────────────────────────────────────────────────────

#[test]
fn panel_font_metrics_returns_consistent_values() {
    let mut r = make_renderer();
    let (fp, cw, row_h) = r.panel_font_metrics();
    assert!(fp > 0.0, "fp must be positive");
    assert!(cw > 0, "cw must be positive");
    assert!(row_h > 0, "row_h must be positive");
}
