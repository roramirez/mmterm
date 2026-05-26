use super::*;
use crate::tui_config::{ConfigPanel, Field, FieldKind};

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
