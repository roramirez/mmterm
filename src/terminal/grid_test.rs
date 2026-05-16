use super::*;

fn make_grid(cols: usize, rows: usize) -> Grid {
    Grid::with_colors(
        cols,
        rows,
        GridColors {
            fg: Color::WHITE,
            bg: Color::BLACK,
            cursor: Color::CURSOR,
            selection: Color::SELECTION,
            palette: [Color::BLACK; 16],
        },
        10_000,
    )
}

#[test]
fn write_char_stores_and_advances_cursor() {
    let mut g = make_grid(10, 5);
    g.write_char('A');
    assert_eq!(g.cell(0, 0).c, 'A');
    assert_eq!(g.cursor_col, 1);
}

#[test]
fn write_char_wraps_to_next_row() {
    let mut g = make_grid(3, 5);
    g.write_char('A');
    g.write_char('B');
    g.write_char('C');
    // cursor_col is now 3; next write wraps
    g.write_char('D');
    assert_eq!(g.cell(0, 1).c, 'D');
    assert_eq!(g.cursor_col, 1);
    assert_eq!(g.cursor_row, 1);
}

#[test]
fn scroll_up_pushes_line_to_scrollback() {
    let mut g = make_grid(4, 3);
    g.write_char('A');
    assert_eq!(g.scrollback_len(), 0);
    g.scroll_up(1);
    assert_eq!(g.scrollback_len(), 1);
    assert_eq!(g.scrollback[0][0].c, 'A');
}

#[test]
fn scroll_down_shifts_content() {
    let mut g = make_grid(4, 3);
    g.write_char('A');
    g.cursor_col = 0;
    g.cursor_row = 1;
    g.write_char('B');
    g.scroll_down(1);
    assert_eq!(g.cell(0, 0).c, ' ');
    assert_eq!(g.cell(0, 1).c, 'A');
}

#[test]
fn resize_preserves_existing_content() {
    let mut g = make_grid(5, 5);
    g.write_char('X');
    g.resize(8, 8);
    assert_eq!(g.cell(0, 0).c, 'X');
    assert_eq!(g.cols, 8);
    assert_eq!(g.rows, 8);
}

#[test]
fn selected_text_single_row() {
    let mut g = make_grid(10, 5);
    for c in "hello".chars() {
        g.write_char(c);
    }
    assert_eq!(g.selected_text(0, 0, 4, 0, 0), "hello");
}

#[test]
fn selected_text_reversed_selection() {
    let mut g = make_grid(10, 5);
    for c in "hello".chars() {
        g.write_char(c);
    }
    assert_eq!(g.selected_text(4, 0, 0, 0, 0), "hello");
}

#[test]
fn selected_text_trims_trailing_spaces() {
    let mut g = make_grid(10, 5);
    g.write_char('H');
    g.write_char('i');
    assert_eq!(g.selected_text(0, 0, 9, 0, 0), "Hi");
}

#[test]
fn alternate_screen_saves_and_restores() {
    let mut g = make_grid(10, 5);
    g.write_char('A');
    g.enter_alternate_screen();
    assert_eq!(g.cell(0, 0).c, ' ');
    g.write_char('B');
    g.exit_alternate_screen();
    assert_eq!(g.cell(0, 0).c, 'A');
}

#[test]
fn alternate_screen_double_enter_is_noop() {
    let mut g = make_grid(10, 5);
    g.write_char('A');
    g.enter_alternate_screen();
    g.enter_alternate_screen();
    g.exit_alternate_screen();
    assert_eq!(g.cell(0, 0).c, 'A');
}

#[test]
fn clear_line_blanks_row() {
    let mut g = make_grid(5, 3);
    g.write_char('X');
    g.clear_line(0);
    assert_eq!(g.cell(0, 0).c, ' ');
}

#[test]
fn clear_screen_blanks_all_cells() {
    let mut g = make_grid(5, 3);
    g.write_char('X');
    g.clear_screen();
    for row in 0..3 {
        for col in 0..5 {
            assert_eq!(g.cell(col, row).c, ' ');
        }
    }
}

#[test]
fn scrollback_len_increments_on_scroll_up() {
    let mut g = make_grid(4, 2);
    assert_eq!(g.scrollback_len(), 0);
    g.scroll_up(1);
    assert_eq!(g.scrollback_len(), 1);
    g.scroll_up(2);
    assert_eq!(g.scrollback_len(), 3);
}

#[test]
fn write_wide_char_occupies_two_columns() {
    let mut g = make_grid(10, 5);
    g.write_char('日'); // unicode width 2
    assert_eq!(g.cell(0, 0).c, '日');
    assert!(g.cell(0, 0).wide);
    assert!(g.cell(1, 0).wide_cont);
    assert_eq!(g.cursor_col, 2);
}

#[test]
fn scroll_up_within_region_does_not_push_to_scrollback() {
    let mut g = make_grid(5, 5);
    g.scroll_top = 2;
    g.scroll_bottom = 4;
    g.cursor_row = 2;
    g.write_char('X');
    g.scroll_up(1);
    assert_eq!(g.scrollback_len(), 0);
}

#[test]
fn selected_text_multi_row_joins_with_newline() {
    let mut g = make_grid(10, 5);
    g.write_char('A');
    g.cursor_col = 0;
    g.cursor_row = 1;
    g.write_char('B');
    let text = g.selected_text(0, 0, 0, 1, 0);
    assert_eq!(text, "A\nB");
}

#[test]
fn selected_text_in_scrollback() {
    // Write "AB" to row 0, then scroll it into scrollback
    let mut g = make_grid(10, 2);
    g.write_char('A');
    g.write_char('B');
    // Scroll grid up twice: row 0 ("AB") goes to scrollback, then another blank row
    g.scroll_up(1);
    g.scroll_up(1);
    // Now scrollback has 2 rows; scroll_offset=2 means we see from the top of scrollback
    let sb_len = g.scrollback_len();
    assert!(sb_len >= 1);
    // Select the first scrollback row (visual row 0 with scroll_offset = sb_len)
    let text = g.selected_text(0, 0, 1, 0, sb_len);
    assert_eq!(text, "AB");
}

#[test]
fn in_alternate_screen_reflects_state() {
    let mut g = make_grid(10, 5);
    assert!(!g.in_alternate_screen());
    g.enter_alternate_screen();
    assert!(g.in_alternate_screen());
    g.exit_alternate_screen();
    assert!(!g.in_alternate_screen());
}

#[test]
fn cursor_visible_restored_after_alt_screen_hide() {
    let mut g = make_grid(10, 5);
    // App hides cursor, enters alt screen (e.g. Claude Code / vim)
    g.cursor_visible = false;
    g.enter_alternate_screen();
    // Inside alt screen the cursor is reset to visible
    assert!(g.cursor_visible);
    // App hides cursor again inside alt screen
    g.cursor_visible = false;
    g.exit_alternate_screen();
    // After exit, original hidden state must be restored (not forced visible)
    assert!(!g.cursor_visible);
}

#[test]
fn cursor_visible_reset_on_enter_alt_screen() {
    let mut g = make_grid(10, 5);
    g.cursor_visible = false;
    g.enter_alternate_screen();
    assert!(
        g.cursor_visible,
        "alt screen should reset cursor to visible"
    );
    g.exit_alternate_screen();
}

#[test]
fn exit_alternate_screen_when_not_active_is_noop() {
    let mut g = make_grid(10, 5);
    g.write_char('A');
    g.exit_alternate_screen(); // not in alt screen — should not panic
    assert_eq!(g.cell(0, 0).c, 'A');
}

#[test]
fn cell_default_is_space_with_standard_colors() {
    let c = Cell::default();
    assert_eq!(c.c, ' ');
    assert!(!c.bold);
    assert!(c.url.is_none());
}

#[test]
fn scrollback_is_capped_at_max() {
    let mut g = make_grid(4, 2);
    // SCROLLBACK_MAX = 10_000; push one extra to trigger pop_front
    g.scroll_up(10_001);
    assert_eq!(g.scrollback_len(), 10_000);
}

#[test]
fn blank_cell_uses_default_bg_not_sgr_bg() {
    let mut g = make_grid(10, 5);
    g.bg = Color::rgb(0xff, 0x00, 0x00);
    let blank = g.blank_cell();
    assert_eq!(blank.bg, g.default_bg);
}

#[test]
fn erase_cell_uses_current_sgr_bg() {
    let mut g = make_grid(10, 5);
    let red = Color::rgb(0xff, 0x00, 0x00);
    g.bg = red;
    let erased = g.erase_cell();
    assert_eq!(erased.bg, red);
}

#[test]
fn write_char_stamps_bold_attribute() {
    let mut g = make_grid(10, 5);
    g.bold = true;
    g.write_char('X');
    assert!(g.cell(0, 0).bold);
}

#[test]
fn write_char_stamps_dim_attribute() {
    let mut g = make_grid(10, 5);
    g.dim = true;
    g.write_char('X');
    assert!(g.cell(0, 0).dim);
}

#[test]
fn write_char_stamps_underline_attribute() {
    let mut g = make_grid(10, 5);
    g.underline = true;
    g.write_char('X');
    assert!(g.cell(0, 0).underline);
}

#[test]
fn write_char_stamps_strikethrough_attribute() {
    let mut g = make_grid(10, 5);
    g.strikethrough = true;
    g.write_char('X');
    assert!(g.cell(0, 0).strikethrough);
}

#[test]
fn write_char_reverse_stores_original_colors_and_sets_flag() {
    // Colors are stored as-is; the renderer does the swap based on cell.reverse.
    let mut g = make_grid(10, 5);
    let original_fg = g.fg;
    let original_bg = g.bg;
    g.reverse = true;
    g.write_char('X');
    assert_eq!(g.cell(0, 0).fg, original_fg);
    assert_eq!(g.cell(0, 0).bg, original_bg);
    assert!(g.cell(0, 0).reverse);
}

#[test]
fn write_char_stamps_url_on_cell() {
    use std::sync::Arc;
    let mut g = make_grid(10, 5);
    g.current_url = Some(Arc::new("https://example.com".to_string()));
    g.write_char('X');
    assert_eq!(
        g.cell(0, 0).url.as_deref().map(|s| s.as_str()),
        Some("https://example.com")
    );
}

#[test]
fn advance_row_increments_cursor_when_not_at_bottom() {
    let mut g = make_grid(5, 5);
    g.cursor_row = 2;
    g.advance_row();
    assert_eq!(g.cursor_row, 3);
    assert_eq!(g.scrollback_len(), 0);
}

#[test]
fn advance_row_at_scroll_bottom_triggers_scroll_up() {
    let mut g = make_grid(5, 5);
    g.cursor_row = g.scroll_bottom;
    g.write_char('Z');
    let before_scrollback = g.scrollback_len();
    g.cursor_col = 0;
    g.cursor_row = g.scroll_bottom;
    g.advance_row();
    assert_eq!(g.scrollback_len(), before_scrollback + 1);
    assert_eq!(g.cursor_row, g.scroll_bottom);
}

#[test]
fn scroll_up_multiple_lines_adds_to_scrollback() {
    let mut g = make_grid(4, 5);
    for c in "ABCDE".chars() {
        g.cursor_col = 0;
        g.cursor_row = 0;
        g.write_char(c);
        g.scroll_up(1);
    }
    assert_eq!(g.scrollback_len(), 5);
}

#[test]
fn scroll_down_multiple_lines_shifts_content() {
    let mut g = make_grid(4, 4);
    g.write_char('A');
    g.scroll_down(2);
    assert_eq!(g.cell(0, 0).c, ' ');
    assert_eq!(g.cell(0, 2).c, 'A');
}

fn write_str(g: &mut Grid, s: &str) {
    for c in s.chars() {
        g.write_char(c);
    }
}

#[test]
fn scan_urls_detects_https_url() {
    let url = "https://example.com/path";
    let mut g = make_grid(url.len() + 5, 3);
    write_str(&mut g, url);
    g.scan_urls();
    assert_eq!(g.cell(0, 0).url.as_deref().map(|s| s.as_str()), Some(url));
    assert_eq!(
        g.cell(url.len() - 1, 0).url.as_deref().map(|s| s.as_str()),
        Some(url)
    );
    assert!(g.cell(url.len(), 0).url.is_none());
}

#[test]
fn scan_urls_detects_http_url() {
    let url = "http://example.com";
    let mut g = make_grid(url.len() + 4, 3);
    write_str(&mut g, url);
    g.scan_urls();
    assert_eq!(g.cell(0, 0).url.as_deref().map(|s| s.as_str()), Some(url));
}

#[test]
fn scan_urls_stops_at_space() {
    let mut g = make_grid(40, 3);
    write_str(&mut g, "go to https://example.com now");
    g.scan_urls();
    // cells before the URL have no url
    assert!(g.cell(0, 0).url.is_none());
    // cells in the URL have it set
    let url_start = "go to ".len();
    let url = "https://example.com";
    assert_eq!(
        g.cell(url_start, 0).url.as_deref().map(|s| s.as_str()),
        Some(url)
    );
    assert_eq!(
        g.cell(url_start + url.len() - 1, 0)
            .url
            .as_deref()
            .map(|s| s.as_str()),
        Some(url)
    );
    // cell after the URL (space) has no url
    assert!(g.cell(url_start + url.len(), 0).url.is_none());
}

#[test]
fn scan_urls_does_not_override_osc8_url() {
    use std::sync::Arc;
    let url = "https://plain.com";
    let mut g = make_grid(url.len() + 5, 3);
    // Simulate OSC 8 URL already set on cell 0
    let osc_url = Arc::new("https://osc8.com".to_string());
    g.current_url = Some(osc_url.clone());
    g.write_char('h');
    g.current_url = None;
    // Write rest of url manually so scan_urls would match
    for c in "ttps://plain.com".chars() {
        g.write_char(c);
    }
    g.scan_urls();
    // Cell 0 had OSC 8 URL — should not be overwritten
    assert_eq!(
        g.cell(0, 0).url.as_deref().map(|s| s.as_str()),
        Some("https://osc8.com")
    );
}

#[test]
fn scan_urls_no_false_positive_on_plain_text() {
    let mut g = make_grid(20, 3);
    write_str(&mut g, "not a url at all");
    g.scan_urls();
    for col in 0..16 {
        assert!(g.cell(col, 0).url.is_none());
    }
}

#[test]
fn scan_urls_strips_trailing_closing_paren() {
    // URL wrapped in parens: the trailing ) should not be part of the URL.
    let url = "https://example.com/path";
    let text = format!("({})", url);
    let mut g = make_grid(text.len() + 4, 3);
    write_str(&mut g, &text);
    g.scan_urls();
    let url_start = 1; // after '('
    assert_eq!(
        g.cell(url_start, 0).url.as_deref().map(|s| s.as_str()),
        Some(url)
    );
    // The closing ')' cell must not carry a URL
    assert!(g.cell(url_start + url.len(), 0).url.is_none());
}

#[test]
fn scan_urls_keeps_paren_when_balanced_inside_url() {
    // URLs like https://en.wikipedia.org/wiki/Rust_(language) keep the ')'
    // because there is a matching '(' inside the URL body.
    let url = "https://en.wikipedia.org/wiki/Rust_(language)";
    let mut g = make_grid(url.len() + 4, 3);
    write_str(&mut g, url);
    g.scan_urls();
    assert_eq!(
        g.cell(url.len() - 1, 0).url.as_deref().map(|s| s.as_str()),
        Some(url)
    );
}

#[test]
fn scan_urls_strips_trailing_dot_and_comma() {
    for suffix in [".", ","] {
        let url = "https://example.com/path";
        let text = format!("{}{}", url, suffix);
        let mut g = make_grid(text.len() + 4, 3);
        write_str(&mut g, &text);
        g.scan_urls();
        assert_eq!(
            g.cell(0, 0).url.as_deref().map(|s| s.as_str()),
            Some(url),
            "trailing {suffix:?} should be stripped"
        );
        assert!(
            g.cell(url.len(), 0).url.is_none(),
            "trailing {suffix:?} cell should have no URL"
        );
    }
}
