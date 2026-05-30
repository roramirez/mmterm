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
fn exit_alternate_screen_after_resize_clamps_cursor_and_refits_cells() {
    // Reproduce the panic: enter alt screen at 79×30, resize to 80×30 while in alt
    // screen, then exit — without the fix this causes an index-out-of-bounds panic
    // on the next cell access because saved.cells.len()=2370 but self.cols=80.
    let mut g = make_grid(79, 30);
    // Place a character so there is content to verify after restore.
    g.write_char('X');
    g.enter_alternate_screen();
    // Simulate a resize while the alternate screen is active.
    g.resize(80, 30);
    // Exiting must not panic and must produce a valid, fully-accessible grid.
    g.exit_alternate_screen();
    assert_eq!(g.cols, 80);
    assert_eq!(g.rows, 30);
    assert_eq!(g.cells.len(), 80 * 30);
    // Cursor must be within bounds.
    assert!(g.cursor_col < g.cols);
    assert!(g.cursor_row < g.rows);
    // Every cell must be accessible without panicking.
    for r in 0..g.rows {
        for c in 0..g.cols {
            let _ = g.cell(c, r);
        }
    }
}

#[test]
fn exit_alternate_screen_after_shrink_clamps_cursor_and_refits_cells() {
    // Same scenario but the terminal shrinks while vim is open.
    let mut g = make_grid(80, 31);
    // Move cursor to the last row so saved.cursor_row is at the boundary.
    for _ in 0..30 {
        g.write_char('\n');
    }
    g.enter_alternate_screen();
    g.resize(79, 30);
    g.exit_alternate_screen();
    assert_eq!(g.cols, 79);
    assert_eq!(g.rows, 30);
    assert_eq!(g.cells.len(), 79 * 30);
    assert!(g.cursor_col < g.cols);
    assert!(g.cursor_row < g.rows);
    for r in 0..g.rows {
        for c in 0..g.cols {
            let _ = g.cell(c, r);
        }
    }
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

#[test]
fn scroll_up_moves_all_rows_to_correct_position() {
    // 4 cols × 4 rows: write a distinct char in col 0 of each row, scroll up.
    // After scroll: row 0 = old row 1, row 1 = old row 2, row 2 = old row 3, row 3 = blank.
    let mut g = make_grid(4, 4);
    for (row, c) in "ABCD".chars().enumerate() {
        g.cursor_col = 0;
        g.cursor_row = row;
        g.write_char(c);
    }
    g.scroll_up(1);
    assert_eq!(g.cell(0, 0).c, 'B');
    assert_eq!(g.cell(0, 1).c, 'C');
    assert_eq!(g.cell(0, 2).c, 'D');
    assert_eq!(g.cell(0, 3).c, ' ');
    // Old row 0 ('A') must be in scrollback.
    assert_eq!(g.scrollback[0][0].c, 'A');
}

#[test]
fn scroll_down_moves_all_rows_to_correct_position() {
    // 4 cols × 4 rows: write a distinct char in col 0 of each row, scroll down.
    // After scroll: row 0 = blank, row 1 = old row 0, row 2 = old row 1, row 3 = old row 2.
    let mut g = make_grid(4, 4);
    for (row, c) in "ABCD".chars().enumerate() {
        g.cursor_col = 0;
        g.cursor_row = row;
        g.write_char(c);
    }
    g.scroll_down(1);
    assert_eq!(g.cell(0, 0).c, ' ');
    assert_eq!(g.cell(0, 1).c, 'A');
    assert_eq!(g.cell(0, 2).c, 'B');
    assert_eq!(g.cell(0, 3).c, 'C');
}

#[test]
fn scroll_up_within_region_moves_content_correctly() {
    // 5 cols × 5 rows, scroll region rows 2‥4 (scroll_top=2, scroll_bottom=4).
    // Row 0 and row 1 must stay untouched; rows 2‥4 shift up by one.
    let mut g = make_grid(5, 5);
    g.scroll_top = 2;
    g.scroll_bottom = 4;
    for (row, c) in "ABCDE".chars().enumerate() {
        g.cursor_col = 0;
        g.cursor_row = row;
        g.write_char(c);
    }
    g.scroll_up(1);
    // Outside the scroll region — unchanged.
    assert_eq!(g.cell(0, 0).c, 'A');
    assert_eq!(g.cell(0, 1).c, 'B');
    // Inside the region — shifted up by one.
    assert_eq!(g.cell(0, 2).c, 'D'); // old row 3
    assert_eq!(g.cell(0, 3).c, 'E'); // old row 4
    assert_eq!(g.cell(0, 4).c, ' '); // new blank bottom row
    // scroll_top != 0 → no scrollback.
    assert_eq!(g.scrollback_len(), 0);
}

#[test]
fn scroll_down_within_region_moves_content_correctly() {
    // 5 cols × 5 rows, scroll region rows 1‥3.
    let mut g = make_grid(5, 5);
    g.scroll_top = 1;
    g.scroll_bottom = 3;
    for (row, c) in "ABCDE".chars().enumerate() {
        g.cursor_col = 0;
        g.cursor_row = row;
        g.write_char(c);
    }
    g.scroll_down(1);
    // Outside the region — unchanged.
    assert_eq!(g.cell(0, 0).c, 'A');
    assert_eq!(g.cell(0, 4).c, 'E');
    // Inside the region — shifted down.
    assert_eq!(g.cell(0, 1).c, ' '); // new blank top row
    assert_eq!(g.cell(0, 2).c, 'B'); // old row 1
    assert_eq!(g.cell(0, 3).c, 'C'); // old row 2
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

// ── strip_trailing_punct ──────────────────────────────────────────────────────

#[test]
fn strip_trailing_punct_removes_dot() {
    let chars: Vec<char> = "https://example.com.".chars().collect();
    let len = strip_trailing_punct(&chars, 8, chars.len());
    assert_eq!(
        &chars[..len].iter().collect::<String>(),
        "https://example.com"
    );
}

#[test]
fn strip_trailing_punct_keeps_balanced_parens() {
    let chars: Vec<char> = "https://x.com/Rust_(lang)".chars().collect();
    let len = strip_trailing_punct(&chars, 8, chars.len());
    assert_eq!(len, chars.len());
}

#[test]
fn strip_trailing_punct_strips_unbalanced_close_paren() {
    let chars: Vec<char> = "https://x.com/foo)".chars().collect();
    let len = strip_trailing_punct(&chars, 8, chars.len());
    assert_eq!(
        &chars[..len].iter().collect::<String>(),
        "https://x.com/foo"
    );
}

// ── stamp_url_span ────────────────────────────────────────────────────────────

#[test]
fn stamp_url_span_sets_url_on_cells() {
    let mut g = make_grid(10, 1);
    let url = Arc::new("https://x.com".to_string());
    let cols = g.cols;
    let row_cells = &mut g.cells[0..cols];
    stamp_url_span(row_cells, 0, 3, &url);
    assert!(g.cells[0].url.is_some());
    assert!(g.cells[1].url.is_some());
    assert!(g.cells[2].url.is_some());
    assert!(g.cells[3].url.is_none());
}

#[test]
fn stamp_url_span_does_not_overwrite_existing_url() {
    let mut g = make_grid(10, 1);
    let existing = Arc::new("existing".to_string());
    g.cells[0].url = Some(existing.clone());
    let new_url = Arc::new("https://x.com".to_string());
    let cols = g.cols;
    let row_cells = &mut g.cells[0..cols];
    stamp_url_span(row_cells, 0, 3, &new_url);
    assert_eq!(
        g.cells[0].url.as_deref().map(|s| s.as_str()),
        Some("existing")
    );
    assert!(g.cells[1].url.is_some());
}

// ── collect_row_text ─────────────────────────────────────────────────────────

#[test]
fn collect_row_text_returns_chars_in_range() {
    let mut g = make_grid(10, 2);
    write_str(&mut g, "hello");
    let text = g.collect_row_text(0, 0, 4, 0);
    assert_eq!(text, "hello");
}

#[test]
fn collect_row_text_single_cell() {
    let mut g = make_grid(5, 1);
    write_str(&mut g, "abc");
    let text = g.collect_row_text(0, 1, 1, 0);
    assert_eq!(text, "b");
}

// ── Reflow tests ─────────────────────────────────────────────────────────────

#[test]
fn scroll_up_marks_autowrap_as_soft() {
    // Use a 1-row grid so that autowrap triggers scroll_up (hitting scroll_bottom=0).
    // Writing the 5th char must push row 0 to scrollback with soft_wrap=true.
    let mut g = make_grid(4, 1);
    g.write_char('A');
    g.write_char('B');
    g.write_char('C');
    g.write_char('D');
    // Next char causes autowrap on the only row → scroll_up is called.
    g.write_char('E');
    assert_eq!(g.scrollback_len(), 1);
    assert!(
        g.scrollback_wrapped[0],
        "autowrap-pushed line must be soft_wrap=true"
    );
}

#[test]
fn scroll_up_marks_explicit_newline_as_hard() {
    // A direct scroll_up (simulating \n on the last row) must produce soft_wrap=false.
    // Use a 1-row grid so scroll_up always pushes to scrollback.
    let mut g = make_grid(4, 1);
    g.write_char('A');
    // Explicitly scroll — no autowrap was triggered, so row_wrapped[0]=false.
    g.scroll_up(1);
    assert_eq!(g.scrollback_len(), 1);
    assert!(
        !g.scrollback_wrapped[0],
        "explicit scroll_up must produce soft_wrap=false"
    );
}

#[test]
fn reflow_wider_joins_soft_wrapped_lines() {
    // Use a 1-row grid so every autowrap immediately pushes to scrollback.
    // Write ABCDE: ABCD pushed as soft_wrap=true; E sits in row 0.
    // Write FGHI: EFGH pushed as soft_wrap=true; I sits in row 0.
    let mut g = make_grid(4, 1);
    for c in b"ABCDEFGHI" {
        g.write_char(*c as char);
    }
    // scrollback: [ABCD (soft), EFGH (soft)]; live row 0: [I, ...]
    assert_eq!(g.scrollback_len(), 2);
    assert!(g.scrollback_wrapped[0]);
    assert!(g.scrollback_wrapped[1]);

    g.resize(8, 1);
    // Two soft-wrapped rows join into one logical line.
    assert_eq!(
        g.scrollback_len(),
        1,
        "two soft-wrapped rows should join into one after widening"
    );
    let joined: String = g.scrollback[0]
        .iter()
        .take_while(|c| c.c != ' ')
        .map(|c| c.c)
        .collect();
    assert_eq!(joined, "ABCDEFGH");
}

#[test]
fn reflow_narrower_splits_hard_wrapped_logical_line() {
    // Hard-push a 6-char row from an 8-col grid, then resize to 4 cols.
    // The 6-char logical line splits into two physical rows.
    let mut g = make_grid(8, 1);
    for c in b"ABCDEF" {
        g.write_char(*c as char);
    }
    g.scroll_up(1); // hard push (no autowrap preceded this)
    assert_eq!(g.scrollback_len(), 1);
    assert!(!g.scrollback_wrapped[0]);

    g.resize(4, 1);
    assert_eq!(
        g.scrollback_len(),
        2,
        "6-char line at 8 cols should split into 2 rows at 4 cols"
    );
    // First chunk is soft-wrapped into the second.
    assert!(
        g.scrollback_wrapped[0],
        "first chunk must be soft_wrap=true"
    );
    assert!(
        !g.scrollback_wrapped[1],
        "last chunk must be soft_wrap=false"
    );
    let row0: String = g.scrollback[0][..4].iter().map(|c| c.c).collect();
    assert_eq!(row0, "ABCD");
    let row1_content: String = g.scrollback[1]
        .iter()
        .take_while(|c| c.c != ' ')
        .map(|c| c.c)
        .collect();
    assert_eq!(row1_content, "EF");
}

#[test]
fn reflow_preserves_cell_attributes() {
    // Bold cells must survive a widen reflow.
    // 1-row grid: ABCD pushed to scrollback (soft_wrap=true) when E is written.
    // EFGH pushed (soft_wrap=true) when I is written.
    let mut g = make_grid(4, 1);
    g.bold = true;
    for c in b"ABCDEFGHI" {
        g.write_char(*c as char);
    }
    assert_eq!(g.scrollback_len(), 2);

    g.resize(8, 1);
    assert_eq!(g.scrollback_len(), 1);
    for cell in g.scrollback[0].iter().take(8) {
        assert!(cell.bold, "cell {:?} must be bold after reflow", cell.c);
    }
}

#[test]
fn reflow_hard_wrapped_lines_not_joined() {
    // Hard-push two separate lines from an 8-col grid (cursor reset between pushes).
    // On widen to 12 cols, they must NOT be joined.
    let mut g = make_grid(8, 1);
    for c in b"HELLO" {
        g.write_char(*c as char);
    }
    g.scroll_up(1); // hard push — no autowrap preceded this
    // Reset cursor to row 0 for the next line.
    g.cursor_col = 0;
    g.cursor_row = 0;
    for c in b"WORLD" {
        g.write_char(*c as char);
    }
    g.scroll_up(1); // hard push
    assert_eq!(g.scrollback_len(), 2);
    assert!(!g.scrollback_wrapped[0], "HELLO must be hard-wrapped");
    assert!(!g.scrollback_wrapped[1], "WORLD must be hard-wrapped");

    g.resize(12, 1);
    assert_eq!(
        g.scrollback_len(),
        2,
        "hard-wrapped lines must not be joined on widen"
    );
}

#[test]
fn reflow_wide_chars_not_split_across_row() {
    // '日'(2-wide) '本'(2-wide) fill a 4-col row exactly.
    // Resize to 3 cols: the split point falls on '本' (wide=true).
    // The reflow must move '本' entirely to the next row.
    let mut g = make_grid(4, 1);
    g.write_char('日'); // cols 0-1
    g.write_char('本'); // cols 2-3
    g.scroll_up(1); // hard push: row = ['日', wide_cont, '本', wide_cont]
    assert_eq!(g.scrollback_len(), 1);

    g.resize(3, 1);
    // '日' (+ its wide_cont) fits in 2 cols of the 3-col row; row 0 must not
    // contain '本' (it should be pushed to row 1).
    let row0: Vec<char> = g.scrollback[0].iter().map(|c| c.c).collect();
    assert!(
        !row0.contains(&'本'),
        "wide char '本' must not appear in row 0 after split at 3 cols; row0={row0:?}"
    );
}

// ── Live-grid reflow tests ────────────────────────────────────────────────────

#[test]
fn live_grid_reflow_wider_joins_wrapped_lines() {
    // 4-col, 3-row grid. Write "ABCDE" — A-D fill row 0 (row_wrapped[0]=true),
    // E goes to row 1. Resize to 8 cols: rows 0+1 join into one 8-col row.
    let mut g = make_grid(4, 3);
    for c in b"ABCDE" {
        g.write_char(*c as char);
    }
    assert!(g.row_wrapped[0], "row 0 must be soft-wrapped after ABCDE");

    g.resize(8, 3);

    // Row 0 should contain ABCDE (joined); row 1 should be blank.
    let row0: String = g.cells[..8]
        .iter()
        .take_while(|c| c.c != ' ')
        .map(|c| c.c)
        .collect();
    assert_eq!(
        row0, "ABCDE",
        "row 0 must contain joined content after widen"
    );
    assert!(
        !g.row_wrapped[0],
        "row 0 must not be soft-wrapped after join"
    );
}

#[test]
fn live_grid_reflow_narrower_splits_line() {
    // 8-col, 3-row grid. Write "ABCDEFGH" to row 0, then narrow to 4 cols.
    // ABCDEFGH splits into ABCD + EFGH.  Because the trailing blank rows in the
    // original grid + the extra split row exceed new_rows, ABCD spills to
    // scrollback and EFGH becomes the first visible row — correct terminal behaviour.
    let mut g = make_grid(8, 3);
    for c in b"ABCDEFGH" {
        g.write_char(*c as char);
    }
    assert!(!g.row_wrapped[0]);
    g.cursor_col = 0;
    g.cursor_row = 0;

    g.resize(4, 3);

    // ABCD spilled to scrollback; EFGH is now the first live-grid row.
    assert_eq!(g.scrollback_len(), 1, "ABCD should spill to scrollback");
    let sb0: String = g.scrollback[0]
        .iter()
        .take_while(|c| c.c != ' ')
        .map(|c| c.c)
        .collect();
    assert_eq!(sb0, "ABCD");
    let row0: String = g.cells[..4]
        .iter()
        .take_while(|c| c.c != ' ')
        .map(|c| c.c)
        .collect();
    assert_eq!(row0, "EFGH");
}

#[test]
fn live_grid_reflow_cursor_stays_in_bounds() {
    // After any resize, cursor must remain within [0, cols) × [0, rows).
    let mut g = make_grid(8, 5);
    for c in b"HELLO WORLD" {
        g.write_char(*c as char);
    }
    g.resize(4, 3);
    assert!(
        g.cursor_col < 4,
        "cursor_col {} out of bounds (cols=4)",
        g.cursor_col
    );
    assert!(
        g.cursor_row < 3,
        "cursor_row {} out of bounds (rows=3)",
        g.cursor_row
    );
}

#[test]
fn live_grid_reflow_hard_wrapped_rows_not_joined() {
    // Two separate logical lines in the live grid must not join on widen.
    let mut g = make_grid(4, 4);
    // Write "AB" then advance row (hard wrap).
    g.write_char('A');
    g.write_char('B');
    g.cursor_col = 0;
    g.cursor_row = 1; // move to next row without setting row_wrapped[0]
    g.write_char('C');
    g.write_char('D');
    // row_wrapped = [false, false, false, false]

    g.resize(8, 4);

    // AB and CD should still be on separate rows, not joined.
    let row0_start = g.cells[0].c;
    let row1_start = g.cells[8].c; // col 0 of row 1 in 8-col grid
    assert_eq!(row0_start, 'A');
    assert_eq!(row1_start, 'C', "CD must remain on its own row after widen");
}
