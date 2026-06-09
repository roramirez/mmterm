use crate::input::InputMode;
use crate::terminal::Color;
use crate::terminal::grid::{Cell, CursorShape, Grid};
use crate::terminal::sixel::SixelImage;
use crate::theme::ResolvedTheme;

// Fixed search match foreground — dark enough for contrast on yellow/orange highlights.
pub(super) const SEARCH_MATCH_FG: Color = Color::rgb(0x11, 0x11, 0x1d);

// bg will differ per grid but this fallback is only hit for out-of-bounds scrollback
pub(super) static BLANK_CELL: Cell = Cell {
    c: ' ',
    fg: Color::WHITE,
    bg: Color::rgb(0x12, 0x12, 0x12),
    bold: false,
    dim: false,
    italic: false,
    underline: false,
    strikethrough: false,
    overline: false,
    reverse: false,
    blink: false,
    wide: false,
    wide_cont: false,
    url: None,
};

pub(super) fn get_cell(grid: &Grid, scroll_offset: usize, row: usize, col: usize) -> &Cell {
    if scroll_offset == 0 {
        return if col < grid.cols && row < grid.rows {
            grid.cell(col, row)
        } else {
            &BLANK_CELL
        };
    }
    let sb_len = grid.scrollback.len();
    let sb_row = sb_len.saturating_sub(scroll_offset) + row;
    if sb_row < sb_len {
        grid.scrollback[sb_row].get(col).unwrap_or(&BLANK_CELL)
    } else {
        let live_row = sb_row - sb_len;
        if live_row < grid.rows {
            grid.cell(col, live_row)
        } else {
            &BLANK_CELL
        }
    }
}

pub(super) fn mode_style(
    mode: &InputMode,
    passthrough: bool,
    theme: &ResolvedTheme,
) -> (&'static str, u32) {
    match mode {
        InputMode::Normal => ("NORMAL", color_u32(theme.palette[4])),
        InputMode::Insert => {
            let label = if passthrough { "INSERT PASS" } else { "INSERT" };
            (label, color_u32(theme.palette[2]))
        }
        InputMode::Visual { .. } => ("VISUAL", color_u32(theme.palette[5])),
        InputMode::RenameTab { .. } => ("RENAME", color_u32(theme.palette[3])),
        InputMode::Search { .. } => ("SEARCH", color_u32(theme.palette[3])),
        InputMode::CommandPalette { .. } => ("PALETTE", color_u32(theme.palette[6])),
        InputMode::QuitSave => ("INSERT", color_u32(theme.palette[2])),
        InputMode::Screenshot { .. } => ("SHOT", color_u32(theme.palette[3])),
        InputMode::ScreenshotName { .. } => ("SHOT", color_u32(theme.palette[3])),
    }
}

pub(super) fn color_u32(c: Color) -> u32 {
    (0xFF << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}

pub(super) fn dim_color(c: u32, factor: f32) -> u32 {
    let r = (((c >> 16) & 0xFF) as f32 * factor) as u32;
    let g = (((c >> 8) & 0xFF) as f32 * factor) as u32;
    let b = ((c & 0xFF) as f32 * factor) as u32;
    (0xFF << 24) | (r << 16) | (g << 8) | b
}

#[allow(clippy::too_many_arguments)]
pub(super) fn cell_out_of_pane_bounds(
    cell_x: u32,
    cell_y: u32,
    draw_w: u32,
    cell_height: u32,
    rx: u32,
    ry: u32,
    rw: u32,
    rh: u32,
    padding: u32,
) -> bool {
    cell_x + draw_w > rx + rw.saturating_sub(padding)
        || cell_y + cell_height > ry + rh.saturating_sub(padding)
}

pub(super) fn should_draw_glyph(cell: &Cell, blink_visible: bool) -> bool {
    cell.c != ' ' && (!cell.blink || blink_visible)
}

pub(super) fn cell_url_hovered(cell_url: Option<&str>, hovered_url: Option<&str>) -> bool {
    match (cell_url, hovered_url) {
        (Some(cu), Some(hu)) => cu == hu,
        _ => false,
    }
}

pub(super) fn blend(bg: u32, fg: u32, alpha: u8) -> u32 {
    let a = alpha as u32;
    let inv = 255 - a;
    // Blend in linear light (gamma-2 approximation: encode=square, decode=sqrt).
    // Avoids the sRGB-space error that makes antialiased glyphs look washed out.
    let blend_ch = |b: u32, f: u32| {
        let mixed = f * f * a + b * b * inv; // [0, 255^2 * 255] fits in u32
        ((mixed as f32 / 255.0).sqrt().round() as u32).min(255)
    };
    (0xFF << 24)
        | (blend_ch((bg >> 16) & 0xFF, (fg >> 16) & 0xFF) << 16)
        | (blend_ch((bg >> 8) & 0xFF, (fg >> 8) & 0xFF) << 8)
        | blend_ch(bg & 0xFF, fg & 0xFF)
}

pub(super) fn fill_pane_background(buf: &mut [u32], buf_width: u32, rect: [u32; 4], color: u32) {
    let [rx, ry, rw, rh] = rect;
    fill_rect(buf, buf_width, rx, ry, rw, rh, color);
}

pub(super) fn dim_buffer(buf: &mut [u32]) {
    for p in buf.iter_mut() {
        let r = ((*p >> 16) & 0xFF) / 3;
        let g = ((*p >> 8) & 0xFF) / 3;
        let b = (*p & 0xFF) / 3;
        *p = 0xff_00_00_00 | (r << 16) | (g << 8) | b;
    }
}

pub(super) fn fill_rect(buf: &mut [u32], bw: u32, x: u32, y: u32, w: u32, h: u32, color: u32) {
    for dy in 0..h {
        for dx in 0..w {
            let idx = ((y + dy) * bw + x + dx) as usize;
            if idx < buf.len() {
                buf[idx] = color;
            }
        }
    }
}

fn write_pixel(buf: &mut [u32], idx: usize, color: u32) {
    if idx < buf.len() {
        buf[idx] = color;
    }
}

pub(super) fn draw_rect_border(
    buf: &mut [u32],
    bw: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: u32,
) {
    for dx in 0..w {
        write_pixel(buf, (y * bw + x + dx) as usize, color);
        write_pixel(buf, ((y + h - 1) * bw + x + dx) as usize, color);
    }
    for dy in 0..h {
        write_pixel(buf, ((y + dy) * bw + x) as usize, color);
        write_pixel(buf, ((y + dy) * bw + x + w - 1) as usize, color);
    }
}

pub(super) fn search_highlight(
    matches: &[(usize, usize, usize)],
    current: Option<usize>,
    abs_row: usize,
    col: usize,
    row_match_lo: usize,
) -> (bool, bool) {
    let mut in_match = false;
    let mut is_current_match = false;
    for (i, &(mr, mc, ml)) in matches.iter().enumerate().skip(row_match_lo) {
        if mr != abs_row {
            break;
        }
        if col >= mc && col < mc + ml {
            in_match = true;
            is_current_match = current == Some(i);
        }
    }
    (in_match, is_current_match)
}

#[allow(clippy::too_many_arguments)]
fn resolve_bg_color(
    is_cursor: bool,
    cursor_shape: CursorShape,
    is_selected: bool,
    is_current_match: bool,
    in_match: bool,
    bg: Color,
    cursor_color: Color,
    selection_color: Color,
    theme: &ResolvedTheme,
) -> u32 {
    if is_cursor && cursor_shape == CursorShape::Block {
        color_u32(cursor_color)
    } else if is_selected {
        color_u32(selection_color)
    } else if is_current_match {
        color_u32(theme.search_current)
    } else if in_match {
        color_u32(theme.search_match)
    } else {
        color_u32(bg)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_cell_colors(
    cell: &Cell,
    is_cursor: bool,
    is_selected: bool,
    in_match: bool,
    is_current_match: bool,
    cursor_shape: CursorShape,
    cursor_color: Color,
    selection_color: Color,
    theme: &ResolvedTheme,
) -> (u32, Color) {
    let (fg, bg) = if cell.reverse {
        (cell.bg, cell.fg)
    } else {
        (cell.fg, cell.bg)
    };
    let fg = if cell.dim {
        Color::rgb(
            (fg.r as f32 * 0.5) as u8,
            (fg.g as f32 * 0.5) as u8,
            (fg.b as f32 * 0.5) as u8,
        )
    } else {
        fg
    };
    let bg32 = resolve_bg_color(
        is_cursor,
        cursor_shape,
        is_selected,
        is_current_match,
        in_match,
        bg,
        cursor_color,
        selection_color,
        theme,
    );
    let fg = if (in_match || is_current_match) && !is_cursor {
        SEARCH_MATCH_FG
    } else {
        fg
    };
    (bg32, fg)
}

pub(super) fn is_cell_selected(
    selection_range: Option<(usize, usize, usize, usize)>,
    col: usize,
    row: usize,
) -> bool {
    selection_range.is_some_and(|(sc, sr, ec, er)| {
        let (r0, c0, r1, c1) = if (sr, sc) <= (er, ec) {
            (sr, sc, er, ec)
        } else {
            (er, ec, sr, sc)
        };
        (row > r0 || (row == r0 && col >= c0)) && (row < r1 || (row == r1 && col <= c1))
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_clipped_hline(
    buf: &mut [u32],
    buf_width: u32,
    y: u32,
    x: u32,
    w: u32,
    clip: [u32; 4],
    color: u32,
) {
    let [rx, ry, rw, rh] = clip;
    if y < ry || y >= ry + rh {
        return;
    }
    let x_end = (x + w).min(rx + rw);
    for sx in x..x_end {
        let idx = (y * buf_width + sx) as usize;
        if idx < buf.len() {
            buf[idx] = color;
        }
    }
}

pub(super) fn draw_clipped_vline(
    buf: &mut [u32],
    buf_width: u32,
    x: u32,
    y: u32,
    h: u32,
    clip: [u32; 4],
    color: u32,
) {
    let [rx, ry, rw, rh] = clip;
    if x < rx || x >= rx + rw {
        return;
    }
    let y_end = (y + h).min(ry + rh);
    for sy in y.max(ry)..y_end {
        let idx = (sy * buf_width + x) as usize;
        if idx < buf.len() {
            buf[idx] = color;
        }
    }
}

pub(super) fn link_underline_color(
    theme: &ResolvedTheme,
    is_hovered: bool,
    pane_is_active: bool,
    dim_factor: f32,
) -> u32 {
    let base = color_u32(theme.palette[4]);
    if is_hovered {
        base
    } else if pane_is_active {
        dim_color(base, 0.65)
    } else {
        dim_color(base, dim_factor)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_scrollbar(
    buf: &mut [u32],
    buf_width: u32,
    rect: [u32; 4],
    grid_rows: usize,
    sb_len: usize,
    scroll_offset: usize,
    theme: &ResolvedTheme,
    scale: crate::dpi::Scale,
) {
    if sb_len == 0 {
        return;
    }
    let thumb_w = scale.chrome(2);
    let min_thumb_h = scale.chrome(4) as f32;
    let [rx, ry, rw, rh] = rect;
    let scrollbar_x = rx + rw.saturating_sub(thumb_w);
    let total = sb_len + grid_rows;
    let thumb_h = ((grid_rows as f32 / total as f32) * rh as f32).max(min_thumb_h) as u32;
    let scroll_pos = sb_len.saturating_sub(scroll_offset);
    let thumb_y = ry + ((scroll_pos as f32 / total as f32) * rh as f32) as u32;
    let color = if scroll_offset == 0 {
        color_u32(theme.scrollbar)
    } else {
        color_u32(theme.palette[4])
    };
    let clamped_h = thumb_h.min((ry + rh).saturating_sub(thumb_y));
    fill_rect(
        buf,
        buf_width,
        scrollbar_x,
        thumb_y,
        thumb_w,
        clamped_h,
        color,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn blit_sixel_pixel(
    buf: &mut [u32],
    buf_width: u32,
    img: &SixelImage,
    px_i: u32,
    py: u32,
    sx: u32,
    sy: u32,
    clip: [u32; 4],
) {
    let [rx, ry, rw, rh] = clip;
    let base = ((py * img.width + px_i) * 4) as usize;
    if base + 3 >= img.pixels.len() {
        return;
    }
    let a = img.pixels[base + 3];
    if a == 0 || sx >= rx + rw || sy >= ry + rh {
        return;
    }
    let idx = (sy * buf_width + sx) as usize;
    if idx < buf.len() {
        let r = img.pixels[base] as u32;
        let g = img.pixels[base + 1] as u32;
        let b = img.pixels[base + 2] as u32;
        let src = (0xff_u32 << 24) | (r << 16) | (g << 8) | b;
        buf[idx] = blend(buf[idx], src, a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Grid;
    use crate::terminal::grid::GridColors;

    fn default_colors() -> GridColors {
        GridColors {
            fg: Color::WHITE,
            bg: Color::BLACK,
            cursor: Color::rgb(0, 128, 255),
            selection: Color::rgb(64, 64, 128),
            palette: [Color::BLACK; 16],
        }
    }

    #[test]
    fn color_u32_packs_rgb() {
        let c = Color::rgb(0x12, 0x34, 0x56);
        assert_eq!(color_u32(c), 0xFF_12_34_56);
    }

    #[test]
    fn dim_color_reduces_brightness() {
        let c = 0xFF_80_80_80u32;
        let d = dim_color(c, 0.5);
        let r = (d >> 16) & 0xFF;
        let g = (d >> 8) & 0xFF;
        let b = d & 0xFF;
        assert!(r <= 0x40);
        assert!(g <= 0x40);
        assert!(b <= 0x40);
    }

    #[test]
    fn fill_rect_writes_color_in_bounds() {
        let mut buf = vec![0u32; 10 * 10];
        fill_rect(&mut buf, 10, 1, 1, 3, 3, 0xFF_FF_FF_FF);
        assert_eq!(buf[1 * 10 + 1], 0xFF_FF_FF_FF);
        assert_eq!(buf[3 * 10 + 3], 0xFF_FF_FF_FF);
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn get_cell_live_view_no_scroll() {
        let mut g = Grid::with_colors(4, 4, default_colors(), 100);
        g.cursor_col = 0;
        g.cursor_row = 0;
        g.write_char('A');
        let cell = get_cell(&g, 0, 0, 0);
        assert_eq!(cell.c, 'A');
    }

    #[test]
    fn search_highlight_finds_match_on_row() {
        let matches = vec![(5, 2, 3)];
        let (in_m, is_cur) = search_highlight(&matches, Some(0), 5, 2, 0);
        assert!(in_m);
        assert!(is_cur);
    }

    #[test]
    fn search_highlight_misses_different_row() {
        let matches = vec![(5, 2, 3)];
        let (in_m, _) = search_highlight(&matches, Some(0), 6, 2, 0);
        assert!(!in_m);
    }

    #[test]
    fn is_cell_selected_inside_range() {
        assert!(is_cell_selected(Some((1, 0, 1, 5)), 1, 3));
    }

    #[test]
    fn is_cell_selected_outside_range() {
        assert!(!is_cell_selected(Some((1, 0, 1, 5)), 1, 6));
    }

    #[test]
    fn blend_opaque_uses_fg() {
        let fg = 0xFF_FF_00_00u32;
        let bg = 0xFF_00_FF_00u32;
        let result = blend(bg, fg, 255);
        let r = (result >> 16) & 0xFF;
        let g = (result >> 8) & 0xFF;
        assert!(r > 200);
        assert!(g < 50);
    }

    // ── Task 21 proxy tests — pane padding scaling ───────────────────────────

    #[test]
    fn pane_padding_scales() {
        use crate::dpi::Scale;
        assert_eq!(Scale::new(2.0).chrome(crate::ui::layout::PANE_PADDING), 8);
        assert_eq!(Scale::new(1.0).chrome(crate::ui::layout::PANE_PADDING), 4);
    }

    // ── Task 23 proxy tests — pure arithmetic, no rendering ──────────────────

    #[test]
    fn scrollbar_dims_scale() {
        use crate::dpi::Scale;
        assert_eq!(Scale::new(2.0).chrome(2), 4);
        assert_eq!(Scale::new(2.0).chrome(4), 8);
        assert_eq!(Scale::new(1.0).chrome(2), 2);
    }

    #[test]
    fn blend_transparent_uses_bg() {
        let fg = 0xFF_FF_00_00u32;
        let bg = 0xFF_00_FF_00u32;
        let result = blend(bg, fg, 0);
        let r = (result >> 16) & 0xFF;
        let g = (result >> 8) & 0xFF;
        assert!(r < 50);
        assert!(g > 200);
    }
}
