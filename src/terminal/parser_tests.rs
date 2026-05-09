use super::super::grid::Color;
use super::*;

fn make_parser(cols: usize, rows: usize) -> TerminalParser {
    TerminalParser::new_with_colors(
        cols,
        rows,
        Color::WHITE,
        Color::BLACK,
        Color::CURSOR,
        Color::SELECTION,
        [Color::BLACK; 16],
    )
}

#[test]
fn prints_characters() {
    let mut p = make_parser(10, 5);
    p.process(b"hi");
    assert_eq!(p.grid.cell(0, 0).c, 'h');
    assert_eq!(p.grid.cell(1, 0).c, 'i');
}

#[test]
fn carriage_return_resets_col() {
    let mut p = make_parser(10, 5);
    p.process(b"hello\r");
    assert_eq!(p.grid.cursor_col, 0);
}

#[test]
fn linefeed_advances_row() {
    let mut p = make_parser(10, 5);
    p.process(b"\n");
    assert_eq!(p.grid.cursor_row, 1);
}

#[test]
fn backspace_moves_cursor_back() {
    let mut p = make_parser(10, 5);
    p.process(b"AB\x08");
    assert_eq!(p.grid.cursor_col, 1);
}

#[test]
fn tab_advances_to_8_boundary() {
    let mut p = make_parser(20, 5);
    p.process(b"\t");
    assert_eq!(p.grid.cursor_col, 8);
    p.process(b"\t");
    assert_eq!(p.grid.cursor_col, 16);
}

#[test]
fn bell_sets_pending_flag() {
    let mut p = make_parser(10, 5);
    p.process(b"\x07");
    assert!(p.grid.bell_pending);
}

#[test]
fn cursor_up_csi_a() {
    let mut p = make_parser(10, 5);
    p.grid.cursor_row = 3;
    p.process(b"\x1b[2A");
    assert_eq!(p.grid.cursor_row, 1);
}

#[test]
fn cursor_down_csi_b() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[2B");
    assert_eq!(p.grid.cursor_row, 2);
}

#[test]
fn cursor_forward_csi_c() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[3C");
    assert_eq!(p.grid.cursor_col, 3);
}

#[test]
fn cursor_back_csi_d() {
    let mut p = make_parser(10, 5);
    p.grid.cursor_col = 5;
    p.process(b"\x1b[2D");
    assert_eq!(p.grid.cursor_col, 3);
}

#[test]
fn cursor_position_csi_h() {
    let mut p = make_parser(10, 10);
    p.process(b"\x1b[3;5H");
    assert_eq!(p.grid.cursor_row, 2);
    assert_eq!(p.grid.cursor_col, 4);
}

#[test]
fn cursor_position_home_no_params() {
    let mut p = make_parser(10, 10);
    p.grid.cursor_row = 5;
    p.grid.cursor_col = 5;
    p.process(b"\x1b[H");
    assert_eq!(p.grid.cursor_row, 0);
    assert_eq!(p.grid.cursor_col, 0);
}

#[test]
fn cha_sets_column() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[5G");
    assert_eq!(p.grid.cursor_col, 4);
}

#[test]
fn vpa_sets_row() {
    let mut p = make_parser(10, 10);
    p.process(b"\x1b[3d");
    assert_eq!(p.grid.cursor_row, 2);
}

#[test]
fn sgr_bold() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[1m");
    assert!(p.grid.bold);
}

#[test]
fn sgr_dim_and_underline() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[2;4m");
    assert!(p.grid.dim);
    assert!(p.grid.underline);
}

#[test]
fn sgr_reverse() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[7m");
    assert!(p.grid.reverse);
    p.process(b"\x1b[27m");
    assert!(!p.grid.reverse);
}

#[test]
fn sgr_strikethrough() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[9m");
    assert!(p.grid.strikethrough);
    p.process(b"\x1b[29m");
    assert!(!p.grid.strikethrough);
}

#[test]
fn sgr_reset_clears_attributes() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[1;2;4;9m");
    p.process(b"\x1b[0m");
    assert!(!p.grid.bold);
    assert!(!p.grid.dim);
    assert!(!p.grid.underline);
    assert!(!p.grid.strikethrough);
}

#[test]
fn erase_in_line_to_end() {
    let mut p = make_parser(10, 5);
    p.process(b"hello");
    p.grid.cursor_col = 2;
    p.process(b"\x1b[K");
    assert_eq!(p.grid.cell(0, 0).c, 'h');
    assert_eq!(p.grid.cell(1, 0).c, 'e');
    assert_eq!(p.grid.cell(2, 0).c, ' ');
    assert_eq!(p.grid.cell(4, 0).c, ' ');
}

#[test]
fn erase_in_display_below() {
    let mut p = make_parser(5, 4);
    p.process(b"AAAAA");
    p.grid.cursor_row = 0;
    p.grid.cursor_col = 0;
    p.process(b"\x1b[J");
    assert_eq!(p.grid.cell(0, 0).c, ' ');
}

#[test]
fn dch_deletes_characters() {
    let mut p = make_parser(10, 5);
    p.process(b"ABCDE");
    p.grid.cursor_col = 1;
    p.process(b"\x1b[2P");
    assert_eq!(p.grid.cell(0, 0).c, 'A');
    assert_eq!(p.grid.cell(1, 0).c, 'D');
    assert_eq!(p.grid.cell(2, 0).c, 'E');
    assert_eq!(p.grid.cell(3, 0).c, ' ');
}

#[test]
fn ich_inserts_blank_characters() {
    let mut p = make_parser(10, 5);
    p.process(b"ABCDE");
    p.grid.cursor_col = 1;
    p.process(b"\x1b[2@");
    assert_eq!(p.grid.cell(0, 0).c, 'A');
    assert_eq!(p.grid.cell(1, 0).c, ' ');
    assert_eq!(p.grid.cell(2, 0).c, ' ');
    assert_eq!(p.grid.cell(3, 0).c, 'B');
    assert_eq!(p.grid.cell(4, 0).c, 'C');
}

#[test]
fn ech_erases_in_place() {
    let mut p = make_parser(10, 5);
    p.process(b"ABCDE");
    p.grid.cursor_col = 1;
    p.process(b"\x1b[3X");
    assert_eq!(p.grid.cell(0, 0).c, 'A');
    assert_eq!(p.grid.cell(1, 0).c, ' ');
    assert_eq!(p.grid.cell(2, 0).c, ' ');
    assert_eq!(p.grid.cell(3, 0).c, ' ');
    assert_eq!(p.grid.cell(4, 0).c, 'E');
}

#[test]
fn alternate_screen_via_csi_1049() {
    let mut p = make_parser(10, 5);
    p.process(b"hello");
    p.process(b"\x1b[?1049h");
    assert_eq!(p.grid.cell(0, 0).c, ' ');
    p.process(b"\x1b[?1049l");
    assert_eq!(p.grid.cell(0, 0).c, 'h');
}

#[test]
fn application_cursor_keys_mode() {
    let mut p = make_parser(10, 5);
    assert!(!p.grid.application_cursor_keys);
    p.process(b"\x1b[?1h");
    assert!(p.grid.application_cursor_keys);
    p.process(b"\x1b[?1l");
    assert!(!p.grid.application_cursor_keys);
}

#[test]
fn bracketed_paste_mode() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[?2004h");
    assert!(p.grid.bracketed_paste);
    p.process(b"\x1b[?2004l");
    assert!(!p.grid.bracketed_paste);
}

#[test]
fn scroll_region_sets_top_and_bottom() {
    let mut p = make_parser(10, 10);
    p.process(b"\x1b[2;5r");
    assert_eq!(p.grid.scroll_top, 1);
    assert_eq!(p.grid.scroll_bottom, 4);
}

#[test]
fn decsc_decrc_saves_and_restores_cursor() {
    let mut p = make_parser(10, 10);
    p.grid.cursor_col = 3;
    p.grid.cursor_row = 4;
    p.process(b"\x1b7");
    p.grid.cursor_col = 0;
    p.grid.cursor_row = 0;
    p.process(b"\x1b8");
    assert_eq!(p.grid.cursor_col, 3);
    assert_eq!(p.grid.cursor_row, 4);
}

#[test]
fn osc_title() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b]0;my title\x07");
    assert_eq!(p.grid.osc_title.as_deref(), Some("my title"));
}

#[test]
fn osc7_cwd() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b]7;file:///home/user/code\x07");
    assert_eq!(p.grid.cwd.as_deref(), Some("/home/user/code"));
}

#[test]
fn color256_palette_range() {
    let palette = [Color::rgb(1, 2, 3); 16];
    assert_eq!(color256(0, &palette), Color::rgb(1, 2, 3));
    assert_eq!(color256(15, &palette), Color::rgb(1, 2, 3));
}

#[test]
fn color256_grayscale_range() {
    let palette = [Color::BLACK; 16];
    assert_eq!(color256(232, &palette), Color::rgb(8, 8, 8));
    assert_eq!(color256(255, &palette), Color::rgb(238, 238, 238));
}

#[test]
fn parse_osc7_absolute_path() {
    assert_eq!(
        parse_osc7_uri("file:///home/user"),
        Some("/home/user".to_string())
    );
}

#[test]
fn parse_osc7_with_hostname() {
    assert_eq!(
        parse_osc7_uri("file://mymachine/home/user"),
        Some("/home/user".to_string())
    );
}

// ── DEC private modes ────────────────────────────────────────────────────────

#[test]
fn cursor_visibility_mode() {
    let mut p = make_parser(10, 5);
    assert!(p.grid.cursor_visible);
    p.process(b"\x1b[?25l");
    assert!(!p.grid.cursor_visible);
    p.process(b"\x1b[?25h");
    assert!(p.grid.cursor_visible);
}

#[test]
fn mouse_mode_1000() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[?1000h");
    assert_eq!(p.grid.mouse_mode, 1000);
    p.process(b"\x1b[?1000l");
    assert_eq!(p.grid.mouse_mode, 0);
}

#[test]
fn mouse_mode_1002() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[?1002h");
    assert_eq!(p.grid.mouse_mode, 1002);
    p.process(b"\x1b[?1002l");
    assert_eq!(p.grid.mouse_mode, 0);
}

#[test]
fn mouse_mode_1003() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[?1003h");
    assert_eq!(p.grid.mouse_mode, 1003);
    p.process(b"\x1b[?1003l");
    assert_eq!(p.grid.mouse_mode, 0);
}

#[test]
fn mouse_sgr_mode_1006() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[?1006h");
    assert!(p.grid.mouse_sgr);
    p.process(b"\x1b[?1006l");
    assert!(!p.grid.mouse_sgr);
}

// ── Erase in display ─────────────────────────────────────────────────────────

#[test]
fn erase_in_display_above() {
    let mut p = make_parser(5, 4);
    for _ in 0..4 {
        p.process(b"AAAAA\r\n");
    }
    p.grid.cursor_row = 2;
    p.grid.cursor_col = 2;
    p.process(b"\x1b[1J"); // erase above cursor
    assert_eq!(p.grid.cell(0, 0).c, ' ');
    assert_eq!(p.grid.cell(0, 1).c, ' ');
    assert_eq!(p.grid.cell(3, 2).c, 'A'); // after cursor unaffected
}

#[test]
fn erase_in_display_all() {
    let mut p = make_parser(5, 4);
    p.process(b"AAAAA");
    p.process(b"\x1b[2J");
    assert_eq!(p.grid.cell(0, 0).c, ' ');
}

// ── Erase in line ─────────────────────────────────────────────────────────────

#[test]
fn erase_in_line_before_cursor() {
    let mut p = make_parser(10, 5);
    p.process(b"hello");
    p.grid.cursor_col = 2;
    p.process(b"\x1b[1K");
    assert_eq!(p.grid.cell(0, 0).c, ' ');
    assert_eq!(p.grid.cell(1, 0).c, ' ');
    assert_eq!(p.grid.cell(2, 0).c, ' ');
    assert_eq!(p.grid.cell(3, 0).c, 'l');
}

#[test]
fn erase_in_line_entire() {
    let mut p = make_parser(10, 5);
    p.process(b"hello");
    p.grid.cursor_col = 2;
    p.process(b"\x1b[2K");
    for col in 0..5 {
        assert_eq!(p.grid.cell(col, 0).c, ' ');
    }
}

// ── SGR — untested branches ───────────────────────────────────────────────────

#[test]
fn sgr_bold_dim_off_code_22() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[1;2m"); // bold + dim
    p.process(b"\x1b[22m"); // turn both off
    assert!(!p.grid.bold);
    assert!(!p.grid.dim);
}

#[test]
fn sgr_underline_off_code_24() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[4m");
    p.process(b"\x1b[24m");
    assert!(!p.grid.underline);
}

#[test]
fn sgr_256_color_foreground() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[38;5;196m"); // color index 196
    p.process(b"X");
    // 196 = 16 + (5*36 + 0*6 + 0) = 16+180 = 196 → r=5,g=0,b=0 → rgb(215,0,0)
    let expected = color256(196, &[Color::BLACK; 16]);
    assert_eq!(p.grid.cell(0, 0).fg, expected);
}

#[test]
fn sgr_256_color_background() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[48;5;21m"); // color index 21
    p.process(b"X");
    let expected = color256(21, &[Color::BLACK; 16]);
    assert_eq!(p.grid.cell(0, 0).bg, expected);
}

#[test]
fn sgr_truecolor_foreground() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[38;2;255;128;0m");
    p.process(b"X");
    assert_eq!(p.grid.cell(0, 0).fg, Color::rgb(255, 128, 0));
}

#[test]
fn sgr_truecolor_background() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[48;2;10;20;30m");
    p.process(b"X");
    assert_eq!(p.grid.cell(0, 0).bg, Color::rgb(10, 20, 30));
}

#[test]
fn sgr_default_fg_code_39() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[31m"); // red fg
    p.process(b"\x1b[39m"); // reset to default
    assert_eq!(p.grid.fg, p.grid.default_fg);
}

#[test]
fn sgr_default_bg_code_49() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[41m"); // red bg
    p.process(b"\x1b[49m"); // reset to default
    assert_eq!(p.grid.bg, p.grid.default_bg);
}

#[test]
fn sgr_bright_foreground() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[90m"); // bright black fg
    assert_eq!(p.grid.fg, p.grid.palette[8]);
}

#[test]
fn sgr_bright_background() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[100m"); // bright black bg
    assert_eq!(p.grid.bg, p.grid.palette[8]);
}

// ── Scroll / insert / delete line ────────────────────────────────────────────

#[test]
fn csi_scroll_up_s() {
    let mut p = make_parser(5, 4);
    p.process(b"AAAAA");
    p.process(b"\x1b[2S"); // scroll up 2
    assert_eq!(p.grid.scrollback_len(), 2);
}

#[test]
fn csi_scroll_down_t() {
    let mut p = make_parser(5, 4);
    p.grid.cursor_row = 1;
    p.process(b"AAAAA");
    p.grid.cursor_row = 1;
    p.grid.cursor_col = 0;
    p.process(b"\x1b[1T"); // scroll down 1
    assert_eq!(p.grid.cell(0, 2).c, 'A');
}

#[test]
fn csi_insert_line_l() {
    let mut p = make_parser(5, 5);
    p.process(b"AAAAA\r\nBBBBB");
    p.grid.cursor_row = 0;
    p.grid.cursor_col = 0;
    p.process(b"\x1b[1L"); // insert 1 line at row 0
    assert_eq!(p.grid.cell(0, 0).c, ' ');
    assert_eq!(p.grid.cell(0, 1).c, 'A');
}

#[test]
fn csi_delete_line_m() {
    let mut p = make_parser(5, 5);
    p.process(b"AAAAA\r\nBBBBB");
    p.grid.cursor_row = 0;
    p.grid.cursor_col = 0;
    p.process(b"\x1b[1M"); // delete 1 line at row 0
    assert_eq!(p.grid.cell(0, 0).c, 'B');
}

// ── OSC 8 hyperlink ──────────────────────────────────────────────────────────

#[test]
fn osc8_hyperlink_sets_url_on_cells() {
    let mut p = make_parser(20, 5);
    p.process(b"\x1b]8;;https://example.com\x07");
    p.process(b"link");
    p.process(b"\x1b]8;;\x07"); // close
    assert!(p.grid.cell(0, 0).url.is_some());
    assert_eq!(
        p.grid.cell(0, 0).url.as_deref().map(|s| s.as_str()),
        Some("https://example.com")
    );
    assert!(p.grid.cell(4, 0).url.is_none());
}

// ── color256 rgb cube ────────────────────────────────────────────────────────

#[test]
fn color256_rgb_cube() {
    let palette = [Color::BLACK; 16];
    // index 16 = r=0,g=0,b=0 in the 6x6x6 cube → all zero → rgb(0,0,0)
    assert_eq!(color256(16, &palette), Color::rgb(0, 0, 0));
    // index 231 = last cube entry = r=5,g=5,b=5 → f(5)=55+5*40=255
    assert_eq!(color256(231, &palette), Color::rgb(255, 255, 255));
}

// ── SGR reset (code 0) inside multi-param sequence ───────────────────────────

#[test]
fn sgr_reset_code_zero_in_multi_param_sequence() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[1;0;4m"); // bold, then reset (0), then underline
    assert!(!p.grid.bold); // reset cleared bold
    assert!(p.grid.underline); // underline set after reset
}

// ── Scroll region with single param (p1 defaults to rows-1) ──────────────────

#[test]
fn scroll_region_single_param_uses_full_height() {
    let mut p = make_parser(10, 10);
    p.process(b"\x1b[3r"); // top=3, no bottom param → bottom=rows-1
    assert_eq!(p.grid.scroll_top, 2);
    assert_eq!(p.grid.scroll_bottom, 9);
}

// ── ESC M: reverse index ──────────────────────────────────────────────────────

#[test]
fn reverse_index_moves_cursor_up() {
    let mut p = make_parser(10, 5);
    p.grid.cursor_row = 2;
    p.process(b"\x1bM");
    assert_eq!(p.grid.cursor_row, 1);
}

#[test]
fn reverse_index_at_scroll_top_scrolls_content_down() {
    let mut p = make_parser(10, 5);
    p.process(b"AAAAA");
    p.grid.cursor_row = 0;
    p.grid.cursor_col = 0;
    p.process(b"\x1bM"); // at scroll_top → scroll_down(1)
    assert_eq!(p.grid.cell(0, 1).c, 'A');
    assert_eq!(p.grid.cell(0, 0).c, ' ');
}

// ── Additional edge cases ─────────────────────────────────────────────────────

#[test]
fn crlf_moves_to_col_zero_next_row() {
    let mut p = make_parser(10, 5);
    p.process(b"AB\r\n");
    assert_eq!(p.grid.cursor_col, 0);
    assert_eq!(p.grid.cursor_row, 1);
    assert_eq!(p.grid.cell(0, 0).c, 'A');
    assert_eq!(p.grid.cell(1, 0).c, 'B');
}

#[test]
fn cursor_position_clamps_to_grid_bounds() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[999;999H");
    assert_eq!(p.grid.cursor_row, 4);
    assert_eq!(p.grid.cursor_col, 9);
}

#[test]
fn nul_byte_is_ignored() {
    let mut p = make_parser(10, 5);
    p.process(b"A\x00B");
    assert_eq!(p.grid.cell(0, 0).c, 'A');
    assert_eq!(p.grid.cell(1, 0).c, 'B');
    assert_eq!(p.grid.cursor_col, 2);
}

#[test]
fn sgr_bold_persists_across_chars() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[1mAB\x1b[0mC");
    assert!(p.grid.cell(0, 0).bold);
    assert!(p.grid.cell(1, 0).bold);
    assert!(!p.grid.cell(2, 0).bold);
}

#[test]
fn cursor_up_clamps_at_scroll_top() {
    let mut p = make_parser(10, 5);
    p.grid.scroll_top = 2;
    p.grid.cursor_row = 2;
    p.process(b"\x1b[10A");
    assert_eq!(p.grid.cursor_row, 0);
}

#[test]
fn cursor_down_clamps_at_last_row() {
    let mut p = make_parser(10, 5);
    p.grid.cursor_row = 3;
    p.process(b"\x1b[10B");
    assert_eq!(p.grid.cursor_row, 4);
}

#[test]
fn cha_clamps_to_last_col() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[999G");
    assert_eq!(p.grid.cursor_col, 9);
}

#[test]
fn multiple_scroll_ups_accumulate_scrollback() {
    let mut p = make_parser(5, 3);
    p.process(b"line1\r\nline2\r\nline3\r\nline4\r\n");
    assert!(p.grid.scrollback_len() > 0);
}

#[test]
fn sgr_underline_off_code_24_clears_underline() {
    let mut p = make_parser(10, 5);
    p.process(b"\x1b[4m");
    assert!(p.grid.underline);
    p.process(b"\x1b[24m");
    assert!(!p.grid.underline);
}

#[test]
fn dcs_sequence_hook_put_unhook_do_not_panic() {
    // DCS: \x1bP<data>\x1b\\ — exercises hook(), put(), and unhook()
    let mut p = make_parser(10, 5);
    p.process(b"\x1bPhello\x1b\\");
    // No assertion beyond "did not panic"; parser should remain usable
    p.process(b"X");
    assert_eq!(p.grid.cell(0, 0).c, 'X');
}

#[test]
fn osc_title_empty_string_clears_title() {
    let mut p = make_parser(10, 5);
    // Set a title first
    p.process(b"\x1b]2;my title\x07");
    assert_eq!(p.grid.osc_title.as_deref(), Some("my title"));
    // Send OSC with empty title — should clear to None
    p.process(b"\x1b]2;\x07");
    assert!(p.grid.osc_title.is_none());
}
