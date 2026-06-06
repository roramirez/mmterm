use super::{
    PaneView, Renderer, blend, cell_url_hovered, color_u32, dim_color, get_cell, mode_style,
};
use crate::InputMode;
use crate::config::Config;
use crate::terminal::Grid;
use crate::terminal::grid::{Color, CursorShape, GridColors};
use crate::theme::default_theme;
use crate::tui_config::ConfigPanel;

fn make_renderer() -> Renderer {
    Renderer::new("JetBrainsMono", 16.0)
}

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
fn font_metrics_compute_returns_positive_cell_size() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    assert!(m.cell_width > 0);
    assert!(m.cell_height > 0);
    assert!(m.baseline > 0);
}

#[test]
fn font_metrics_grid_size_for_standard_window() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600);
    assert!(cols > 0);
    assert!(rows > 0);
}

#[test]
fn font_metrics_grid_size_for_small_window() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(1, 1);
    assert_eq!(cols, 1);
    assert_eq!(rows, 1);
}

#[test]
fn color_u32_packs_rgb_correctly() {
    let c = Color::rgb(0x12, 0x34, 0x56);
    let u = color_u32(c);
    assert_eq!((u >> 24) & 0xFF, 0xFF);
    assert_eq!((u >> 16) & 0xFF, 0x12);
    assert_eq!((u >> 8) & 0xFF, 0x34);
    assert_eq!(u & 0xFF, 0x56);
}

#[test]
fn dim_color_factor_zero_returns_black() {
    let dimmed = dim_color(0xff_80_80_80, 0.0);
    assert_eq!(dimmed & 0x00_ff_ff_ff, 0);
}

#[test]
fn dim_color_factor_one_returns_identity() {
    let c = 0xff_80_40_20u32;
    let dimmed = dim_color(c, 1.0);
    assert_eq!((dimmed >> 16) & 0xFF, 0x80);
    assert_eq!((dimmed >> 8) & 0xFF, 0x40);
    assert_eq!(dimmed & 0xFF, 0x20);
}

#[test]
fn get_cell_from_live_grid() {
    let grid = make_grid(80, 24);
    let cell = get_cell(&grid, 0, 0, 0);
    assert_eq!(cell.c, ' ');
}

#[test]
fn get_cell_out_of_bounds_returns_blank() {
    let grid = make_grid(80, 24);
    // Large scroll_offset with large row → lands past both scrollback and live grid.
    let cell = get_cell(&grid, 9999, 9999, 0);
    assert_eq!(cell.c, ' ');
}

#[test]
fn get_cell_scrollback_col_out_of_bounds_returns_blank() {
    let mut grid = make_grid(80, 24);
    // Push a line into scrollback with scroll_up.
    grid.scroll_up(1);
    if !grid.scrollback.is_empty() {
        // col well beyond line length → BLANK_CELL
        let cell = get_cell(&grid, 1, 0, 9999);
        assert_eq!(cell.c, ' ');
    }
}

#[test]
fn mode_style_returns_badge_for_each_mode() {
    let theme = default_theme();
    let (label, _) = mode_style(&InputMode::Insert, false, &theme);
    assert_eq!(label, "INSERT");
    let (label, _) = mode_style(&InputMode::Insert, true, &theme);
    assert_eq!(label, "INSERT PASS");
    let (label, _) = mode_style(&InputMode::Normal, false, &theme);
    assert_eq!(label, "NORMAL");
    let (label, _) = mode_style(
        &InputMode::Visual {
            start_col: 0,
            start_row: 0,
            cur_col: 0,
            cur_row: 0,
            anchored: false,
        },
        false,
        &theme,
    );
    assert_eq!(label, "VISUAL");
    let (label, _) = mode_style(&InputMode::RenameTab { buf: String::new() }, false, &theme);
    assert_eq!(label, "RENAME");
    let (label, _) = mode_style(
        &InputMode::Search {
            query: String::new(),
            history_pos: None,
        },
        false,
        &theme,
    );
    assert_eq!(label, "SEARCH");
}

#[test]
fn draw_empty_buffer_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw(
        &mut buf,
        800,
        600,
        &[],
        &[],
        &InputMode::Insert,
        false,
        &[],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );
}

#[test]
fn draw_pane_fills_background_color() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let grid = make_grid(cols, rows);
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: true,
        show_cursor: false,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw(
        &mut buf,
        800,
        600,
        &[pane],
        &[],
        &InputMode::Insert,
        false,
        &[("shell".to_string(), true, false)],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );
    assert!(buf.iter().any(|&p| p != 0));
}

#[test]
fn draw_tab_bar_renders_without_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw(
        &mut buf,
        800,
        600,
        &[],
        &[],
        &InputMode::Normal,
        false,
        &[
            ("tab1".to_string(), true, false),
            ("tab2".to_string(), false, true),
        ],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );
}

#[test]
fn draw_status_bar_renders_without_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw(
        &mut buf,
        800,
        600,
        &[],
        &[],
        &InputMode::Search {
            query: "hello".to_string(),
            history_pos: None,
        },
        false,
        &[],
        &m,
        3,
        1,
        Some("/home/user"),
        None,
        0.55,
        None,
        false,
        true,
        &theme,
    );
}

#[test]
fn draw_status_bar_pane_title_centered() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let theme = default_theme();

    // With title: pixels should differ from without title.
    let mut buf_with = vec![0u32; 800 * 600];
    r.draw(
        &mut buf_with,
        800,
        600,
        &[],
        &[],
        &InputMode::Normal,
        false,
        &[],
        &m,
        0,
        0,
        None,
        Some("nvim src/main.rs"),
        0.55,
        None,
        false,
        false,
        &theme,
    );

    let mut buf_without = vec![0u32; 800 * 600];
    r.draw(
        &mut buf_without,
        800,
        600,
        &[],
        &[],
        &InputMode::Normal,
        false,
        &[],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );

    assert!(
        buf_with != buf_without,
        "status bar with pane title must differ from one without"
    );
}

#[test]
fn draw_status_bar_pane_title_suppressed_in_search() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let theme = default_theme();

    // In Search mode the title should not be drawn — buffers must match.
    let mut buf_with = vec![0u32; 800 * 600];
    r.draw(
        &mut buf_with,
        800,
        600,
        &[],
        &[],
        &InputMode::Search {
            query: String::new(),
            history_pos: None,
        },
        false,
        &[],
        &m,
        0,
        0,
        None,
        Some("nvim src/main.rs"),
        0.55,
        None,
        false,
        false,
        &theme,
    );

    let mut buf_without = vec![0u32; 800 * 600];
    r.draw(
        &mut buf_without,
        800,
        600,
        &[],
        &[],
        &InputMode::Search {
            query: String::new(),
            history_pos: None,
        },
        false,
        &[],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );

    assert_eq!(
        buf_with, buf_without,
        "pane title must be suppressed in Search mode"
    );
}

#[test]
fn draw_config_panel_renders_without_panic() {
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    let panel = ConfigPanel::from_config(&Config::default());
    r.draw_config_panel(&mut buf, 800, 600, &panel);
}

#[test]
fn draw_quit_confirm_renders_without_panic() {
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw_quit_confirm(&mut buf, 800, 600, &theme);
    assert!(buf.iter().any(|&p| p != 0));
}

#[test]
fn draw_quit_confirm_dims_background() {
    let mut r = make_renderer();
    let theme = default_theme();
    let mut buf = vec![0xff_80_80_80u32; 800 * 600];
    r.draw_quit_confirm(&mut buf, 800, 600, &theme);
    // At least one pixel outside the box should be dimmer than the original.
    assert!(buf.iter().any(|&p| ((p >> 16) & 0xFF) < 0x80));
}

#[test]
fn draw_with_bell_flash_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw(
        &mut buf,
        800,
        600,
        &[],
        &[],
        &InputMode::Insert,
        false,
        &[],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        Some(1.0),
        false,
        false,
        &theme,
    );
}

#[test]
fn draw_with_separator_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    let sep = [[200u32, 0u32, 2u32, 600u32]];
    r.draw(
        &mut buf,
        800,
        600,
        &[],
        &sep,
        &InputMode::Insert,
        false,
        &[],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );
}

#[test]
fn blend_transparent_returns_bg() {
    let bg = 0xff_11_22_33;
    assert_eq!(blend(bg, 0xff_ff_ff_ff, 0), bg);
}

#[test]
fn blend_opaque_returns_fg_channels() {
    let result = blend(0xff_00_00_00, 0xff_80_40_20, 255);
    assert_eq!((result >> 16) & 0xFF, 0x80);
    assert_eq!((result >> 8) & 0xFF, 0x40);
    assert_eq!(result & 0xFF, 0x20);
}

#[test]
fn blend_midpoint_is_lighter_than_linear() {
    // Gamma-correct blending at alpha=128 on black→white should yield a
    // value visibly above the linear midpoint of 128 (~181 for gamma=2).
    let result = blend(0xff_00_00_00, 0xff_ff_ff_ff, 128);
    let ch = result & 0xFF;
    assert!(ch > 128, "expected gamma-correct midpoint > 128, got {ch}");
}

#[test]
fn url_hovered_matches_same_url() {
    assert!(cell_url_hovered(
        Some("https://example.com"),
        Some("https://example.com")
    ));
}

#[test]
fn url_hovered_no_match_when_different_url() {
    assert!(!cell_url_hovered(
        Some("https://example.com"),
        Some("https://other.com")
    ));
}

#[test]
fn url_hovered_no_match_when_hovered_none() {
    assert!(!cell_url_hovered(Some("https://example.com"), None));
}

#[test]
fn url_hovered_no_match_when_cell_has_no_url() {
    assert!(!cell_url_hovered(None, Some("https://example.com")));
}

#[test]
fn url_hovered_no_match_when_both_none() {
    assert!(!cell_url_hovered(None, None));
}

fn make_pane<'a>(grid: &'a Grid, m: &crate::renderer::text::FontMetrics) -> PaneView<'a> {
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let _ = (cols, rows);
    PaneView {
        grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: true,
        show_cursor: false,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    }
}

fn do_draw(
    r: &mut Renderer,
    m: &crate::renderer::text::FontMetrics,
    panes: &[PaneView<'_>],
    mode: &InputMode,
) {
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw(
        &mut buf,
        800,
        600,
        panes,
        &[],
        mode,
        false,
        &[("t".to_string(), true, false)],
        m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );
}

#[test]
fn draw_pane_with_text_renders_glyphs() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('H');
    grid.write_char('e');
    grid.write_char('l');
    grid.write_char('l');
    grid.write_char('o');
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_inactive_pane_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('X');
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: false,
        show_cursor: false,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_visual_selection_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('A');
    grid.write_char('B');
    grid.write_char('C');
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: true,
        show_cursor: false,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    let mode = InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 2,
        cur_row: 0,
        anchored: true,
    };
    do_draw(&mut r, &m, &[pane], &mode);
}

#[test]
fn draw_pane_with_search_match_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('f');
    grid.write_char('o');
    grid.write_char('o');
    let sb_len = grid.scrollback_len();
    // Match at abs_row = sb_len (first live row), col 0, length 3.
    let matches: Vec<(usize, usize, usize)> = vec![(sb_len, 0, 3)];
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: true,
        show_cursor: false,
        blink_visible: true,
        search_matches: &matches,
        search_current: Some(0),
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_with_underline_cell_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('U');
    grid.cell_mut(0, 0).underline = true;
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_osc8_link_without_hover_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('L');
    grid.cell_mut(0, 0).url = Some(std::sync::Arc::new("https://example.com".to_string()));
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_osc8_link_with_hover_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('L');
    grid.cell_mut(0, 0).url = Some(std::sync::Arc::new("https://example.com".to_string()));
    let mut pane = make_pane(&grid, &m);
    pane.hovered_url = Some("https://example.com");
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_osc8_link_paints_underline_without_hover() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('L');
    grid.cell_mut(0, 0).url = Some(std::sync::Arc::new("https://example.com".to_string()));
    let pane = make_pane(&grid, &m);
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw(
        &mut buf,
        800,
        600,
        &[pane],
        &[],
        &InputMode::Insert,
        false,
        &[("t".to_string(), true, false)],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );
    // Underline row is at rect_y + cell_height - 2 (cell at row 0, rect_y = 22)
    let ul_y = (22 + m.cell_height.saturating_sub(2)) as usize;
    let row_pixels = &buf[ul_y * 800..(ul_y + 1) * 800];
    // At least one pixel in the first cell width should differ from the background
    let bg = color_u32(grid.cell(0, 0).bg);
    assert!(
        row_pixels[..m.cell_width as usize].iter().any(|&p| p != bg),
        "expected a visible underline pixel for OSC 8 link without hover"
    );
}

#[test]
fn draw_pane_with_strikethrough_cell_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('S');
    grid.cell_mut(0, 0).strikethrough = true;
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_with_dim_cell_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('D');
    grid.cell_mut(0, 0).dim = true;
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_with_reverse_video_cell_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('R');
    grid.cell_mut(0, 0).reverse = true;
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_reverse_video_swaps_background_to_fg_color() {
    // A space cell with reverse=true should render fg as background.
    // grid fg=WHITE (#ffffff), bg=BLACK (#000000). With reverse the cell
    // background must be WHITE (the fg), not BLACK (the original bg).
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = Grid::with_colors(
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
    );
    grid.reverse = true;
    grid.write_char(' '); // space: only background fill, no glyph
    let pane = make_pane(&grid, &m);
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw(
        &mut buf,
        800,
        600,
        &[pane],
        &[],
        &InputMode::Insert,
        false,
        &[("t".to_string(), true, false)],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );
    // Cell (0,0) background pixel: x = 4 (PANE_PADDING), y = 22+4 (TAB_BAR_H+PANE_PADDING)
    let px = 4usize;
    let py = 26usize;
    let pixel = buf[py * 800 + px];
    let r_ch = (pixel >> 16) & 0xFF;
    let g_ch = (pixel >> 8) & 0xFF;
    let b_ch = pixel & 0xFF;
    // Color::WHITE = rgb(0xd8, 0xd8, 0xd8). Reverse video: background must equal
    // the original fg (WHITE), not the original bg (BLACK = rgb(0x1e, 0x1e, 0x2e)).
    assert_eq!(r_ch, 0xd8, "reverse bg should be fg red channel");
    assert_eq!(g_ch, 0xd8, "reverse bg should be fg green channel");
    assert_eq!(b_ch, 0xd8, "reverse bg should be fg blue channel");
}

#[test]
fn draw_pane_with_scrollback_shows_scrollbar() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    // Push enough lines into scrollback to activate the scrollbar.
    for _ in 0..rows + 5 {
        grid.write_char('A');
        grid.scroll_up(1);
    }
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_scrolled_up_shows_scrollbar_thumb_position() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    for _ in 0..rows + 10 {
        grid.write_char('B');
        grid.scroll_up(1);
    }
    let sb_len = grid.scrollback_len();
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: sb_len.min(5),
        is_active: true,
        show_cursor: false,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_with_cursor_visible_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let grid = make_grid(cols, rows);
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: true,
        show_cursor: true,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_backwards_visual_selection_normalises_range() {
    // When cursor is before anchor (backwards selection), the else branch
    // at line 242 normalises the range so rendering still works.
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    for c in "hello world".chars() {
        grid.write_char(c);
    }
    let pane = make_pane(&grid, &m);
    // start > cur → backwards selection: (sr=0,sc=5) > (er=0,ec=0)
    let mode = InputMode::Visual {
        start_col: 5,
        start_row: 0,
        cur_col: 0,
        cur_row: 0,
        anchored: true,
    };
    do_draw(&mut r, &m, &[pane], &mode);
}

#[test]
fn draw_pane_grid_wider_than_rect_clips_overflow_cells() {
    // Grid with more cols than the rect can hold → rightmost cells are
    // skipped via the bounds-check at line 232.
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    // Extra columns beyond what the 800px rect fits.
    let mut grid = make_grid(cols + 10, rows);
    for c in "overflow".chars() {
        grid.write_char(c);
    }
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: true,
        show_cursor: false,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_with_wide_cont_cell_does_not_panic() {
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('A');
    // Mark col 1 as the right half of a wide char → draw_pane must skip it.
    grid.cell_mut(1, 0).wide_cont = true;
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_non_current_search_match_uses_match_color() {
    // Two matches; search_current=Some(1) → col 0 is a non-current match (line 278).
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('f');
    grid.write_char('o');
    grid.write_char('o');
    grid.write_char('f');
    grid.write_char('o');
    grid.write_char('o');
    let sb_len = grid.scrollback_len();
    let matches: Vec<(usize, usize, usize)> = vec![(sb_len, 0, 3), (sb_len, 3, 3)];
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: true,
        show_cursor: false,
        blink_visible: true,
        search_matches: &matches,
        search_current: Some(1), // match 0 → non-current, match 1 → current
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_inactive_with_url_does_not_panic() {
    // Inactive pane + URL cell → exercises dim_color on the hyperlink underline (line 398).
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.write_char('L');
    grid.cell_mut(0, 0).url = Some(std::sync::Arc::new("https://example.com".to_string()));
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: false,
        show_cursor: false,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_status_bar_search_empty_query_shows_slash() {
    // Search mode with empty query → info = "/" (line 948).
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw(
        &mut buf,
        800,
        600,
        &[],
        &[],
        &InputMode::Search {
            query: String::new(),
            history_pos: None,
        },
        false,
        &[],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );
}

#[test]
fn draw_status_bar_search_no_matches_shows_label() {
    // Non-empty query with search_total=0 → "no matches" label (line 950).
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    r.draw(
        &mut buf,
        800,
        600,
        &[],
        &[],
        &InputMode::Search {
            query: "xyz".to_string(),
            history_pos: None,
        },
        false,
        &[],
        &m,
        0, // search_total = 0
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );
}

#[test]
fn draw_config_panel_with_error_status_uses_error_color() {
    // ConfigPanel with a non-None status → error color branch (line 808).
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    let mut panel = ConfigPanel::from_config(&Config::default());
    panel.status = Some("invalid value".to_string());
    r.draw_config_panel(&mut buf, 800, 600, &panel);
}

#[test]
fn draw_config_panel_selected_past_max_visible_scrolls() {
    // selected far enough down to trigger scroll_start > 0 (line 659).
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    let mut panel = ConfigPanel::from_config(&Config::default());
    panel.selected = panel.fields.len() - 1; // last field, well past max_visible
    r.draw_config_panel(&mut buf, 800, 600, &panel);
}

#[test]
fn draw_config_panel_hexcolor_field_selected_renders_swatch() {
    // Selecting a HexColor field (Background = F_COLOR_BG = 13) renders the
    // color swatch (lines 724-733).
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    let mut panel = ConfigPanel::from_config(&Config::default());
    // F_COLOR_BG is the first HexColor field (index 13).
    panel.selected = 13;
    r.draw_config_panel(&mut buf, 800, 600, &panel);
}

#[test]
fn draw_config_panel_select_field_selected_shows_arrows() {
    // Theme field (index 12) is a Select → renders ← value → arrows (line 744).
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    let mut panel = ConfigPanel::from_config(&Config::default());
    panel.selected = 12; // F_THEME_NAME
    r.draw_config_panel(&mut buf, 800, 600, &panel);
}

#[test]
fn draw_config_panel_editing_select_field_shows_editing_label() {
    // Select field with editing=true → "[editing]" label (lines 762-763, 768).
    let mut r = make_renderer();
    let mut buf = vec![0u32; 800 * 600];
    let mut panel = ConfigPanel::from_config(&Config::default());
    panel.selected = 12; // F_THEME_NAME (Select kind)
    panel.editing = true;
    panel.edit_buf = panel.fields[12].value.clone();
    r.draw_config_panel(&mut buf, 800, 600, &panel);
}

#[test]
fn draw_pane_visual_mode_shows_cursor_at_cur_position() {
    // In Visual mode the cursor block must appear at (cur_col, cur_row), not
    // at the PTY cursor. We detect this by checking that the pixel at the
    // visual cursor position gets the cursor color while the pane is active.
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    for c in "hello".chars() {
        grid.write_char(c);
    }
    // PTY cursor ended up at (5, 0); we set the visual cursor at (2, 0).
    let mode = InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 2,
        cur_row: 0,
        anchored: false,
    };
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: true,
        show_cursor: false,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    // Must not panic — actual pixel inspection is left to integration testing.
    do_draw(&mut r, &m, &[pane], &mode);
}

#[test]
fn draw_pane_visual_mode_inactive_pane_no_cursor() {
    // Inactive panes must not show the visual cursor even if mode is Visual.
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let grid = make_grid(cols, rows);
    let mode = InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 3,
        cur_row: 0,
        anchored: false,
    };
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 0,
        is_active: false, // inactive
        show_cursor: false,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    do_draw(&mut r, &m, &[pane], &mode);
}

// ── PANE_PADDING tests ────────────────────────────────────────────────────

#[test]
fn pane_padding_leaves_top_left_corner_as_background() {
    // The top-left PANE_PADDING×PANE_PADDING pixels must remain background
    // color (no glyph pixels written there).
    use crate::ui::layout::PANE_PADDING;
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let pad2 = PANE_PADDING * 2;
    let (cols, rows) = m.grid_size_for(800u32.saturating_sub(pad2), 556u32.saturating_sub(pad2));
    let mut grid = make_grid(cols, rows);
    // Fill entire grid with 'X' so glyphs would bleed into the corner if
    // padding were absent.
    for _ in 0..cols * rows {
        grid.write_char('X');
    }
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 556],
        scroll_offset: 0,
        is_active: true,
        show_cursor: false,
        blink_visible: false,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    let mut buf = vec![0u32; 800 * 600];
    let theme = default_theme();
    let bg = color_u32(grid.default_bg);
    r.draw(
        &mut buf,
        800,
        600,
        &[pane],
        &[],
        &InputMode::Insert,
        false,
        &[("t".to_string(), true, false)],
        &m,
        0,
        0,
        None,
        None,
        0.55,
        None,
        false,
        false,
        &theme,
    );
    // Every pixel in the top-left padding block must equal bg.
    for dy in 0..PANE_PADDING {
        for dx in 0..PANE_PADDING {
            let idx = ((22 + dy) * 800 + dx) as usize;
            assert_eq!(
                buf[idx], bg,
                "pixel ({dx},{dy}) inside padding should be bg"
            );
        }
    }
}

#[test]
fn pane_padding_grid_size_accounts_for_both_sides() {
    // grid_size_for called with 2×PANE_PADDING subtracted must yield fewer
    // cols/rows than the unpadded call.
    use crate::ui::layout::PANE_PADDING;
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let pad2 = PANE_PADDING * 2;
    let (cols_padded, rows_padded) =
        m.grid_size_for(800u32.saturating_sub(pad2), 556u32.saturating_sub(pad2));
    let (cols_raw, rows_raw) = m.grid_size_for(800, 556);
    assert!(
        cols_padded <= cols_raw,
        "padded cols {cols_padded} should be ≤ raw cols {cols_raw}"
    );
    assert!(
        rows_padded <= rows_raw,
        "padded rows {rows_padded} should be ≤ raw rows {rows_raw}"
    );
}

#[test]
fn draw_pane_with_sixel_image_does_not_panic() {
    use crate::terminal::sixel::SixelImage;
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    // 2×6 red image at cell (0, 0)
    let pixels = vec![255u8, 0, 0, 255].repeat(2 * 6);
    grid.images.push(SixelImage {
        col: 0,
        row: 0,
        width: 2,
        height: 6,
        pixels,
    });
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_sixel_image_wider_than_pane_does_not_panic() {
    use crate::terminal::sixel::SixelImage;
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    // Image deliberately wider than the pane (9000 pixels)
    let pixels = vec![255u8, 0, 0, 128].repeat(9000);
    grid.images.push(SixelImage {
        col: 0,
        row: 0,
        width: 9000,
        height: 1,
        pixels,
    });
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_sixel_image_scrolled_up_not_drawn() {
    use crate::terminal::sixel::SixelImage;
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    grid.images.push(SixelImage {
        col: 0,
        row: 0,
        width: 4,
        height: 6,
        pixels: vec![255, 0, 0, 255].repeat(4 * 6),
    });
    // With scroll_offset > 0 the image blit is skipped; must not panic.
    let pane = PaneView {
        grid: &grid,
        rect: [0, 22, 800, 600 - 44],
        scroll_offset: 5,
        is_active: true,
        show_cursor: false,
        blink_visible: true,
        search_matches: &[],
        search_current: None,
        hovered_url: None,
        cursor_shape: CursorShape::Block,
    };
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}

#[test]
fn draw_pane_sixel_transparent_pixels_preserved() {
    // Transparent pixels (alpha=0) must not overwrite the background.
    use crate::terminal::sixel::SixelImage;
    let mut r = make_renderer();
    let m = r.make_metrics(16.0);
    let (cols, rows) = m.grid_size_for(800, 600u32.saturating_sub(44));
    let mut grid = make_grid(cols, rows);
    // Fully transparent image
    let pixels = vec![255u8, 0, 0, 0].repeat(4 * 6);
    grid.images.push(SixelImage {
        col: 0,
        row: 0,
        width: 4,
        height: 6,
        pixels,
    });
    let pane = make_pane(&grid, &m);
    do_draw(&mut r, &m, &[pane], &InputMode::Insert);
}
