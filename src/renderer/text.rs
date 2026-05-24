use super::glyph::GlyphCache;
use crate::input::InputMode;
use crate::terminal::grid::{Cell, CursorShape};
use crate::terminal::sixel::SixelImage;
use crate::terminal::{Color, Grid};
use crate::theme::ResolvedTheme;
use crate::ui::layout::{PANE_PADDING, TAB_BAR_H};

pub(super) const STATUS_BAR_H: u32 = 22;
const BADGE_PAD_X: u32 = 8;
// Dark color for text rendered on bright-colored badges (readable on any saturated hue).
const BADGE_FG: u32 = 0xff_11_11_1d;
// Fixed search match foreground — dark enough for contrast on yellow/orange highlights.
const SEARCH_MATCH_FG: Color = Color::rgb(0x11, 0x11, 0x1d);

pub struct PaneView<'a> {
    pub grid: &'a Grid,
    pub rect: [u32; 4],
    pub scroll_offset: usize,
    pub is_active: bool,
    pub show_cursor: bool,
    /// When false, cells with `cell.blink` have their glyph hidden (off phase).
    pub blink_visible: bool,
    /// Match positions (abs_row, start_col, len) sorted by abs_row. abs_row = scrollback_len + grid_row.
    pub search_matches: &'a [(usize, usize, usize)],
    pub search_current: Option<usize>,
    /// URL currently hovered by the mouse; only cells with this URL get an underline.
    pub hovered_url: Option<&'a str>,
    /// Cursor shape requested by the running program via DECSCUSR.
    pub cursor_shape: CursorShape,
}

/// Cell layout metrics derived from a specific font size.
/// Stored per-tab so each tab can have its own font size without
/// affecting the global config or other tabs.
#[derive(Clone, Debug)]
pub struct FontMetrics {
    pub font_px: f32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub baseline: u32,
}

impl FontMetrics {
    pub fn compute(glyphs: &mut GlyphCache, font_px: f32) -> Self {
        let m = glyphs.metrics('M', font_px, false);
        let cell_width = m.advance_width.ceil() as u32;
        let ascender = m.height as u32;
        let g = glyphs.metrics('g', font_px, false);
        let descender = ((-g.ymin).max(0)) as u32;
        let cell_height = ascender + descender + 2;
        let baseline = ascender + 1;
        log::info!(
            "FontMetrics at {font_px}px: cell={}x{} baseline={}",
            cell_width,
            cell_height,
            baseline
        );
        Self {
            font_px,
            cell_width: cell_width.max(1),
            cell_height: cell_height.max(1),
            baseline,
        }
    }

    pub fn grid_size_for(&self, w: u32, h: u32) -> (usize, usize) {
        (
            (w / self.cell_width).max(1) as usize,
            (h / self.cell_height).max(1) as usize,
        )
    }
}

pub struct Renderer {
    pub font_px: f32, // default from config (reference only)
    pub(super) status_font_px: f32,
    pub glyphs: GlyphCache,
}

impl Renderer {
    pub fn new(font_family: &str, font_px: f32) -> Self {
        let glyphs = GlyphCache::new(font_family);
        Self {
            font_px,
            status_font_px: 13.0,
            glyphs,
        }
    }

    /// Compute metrics for a given font size using the shared glyph cache.
    pub fn make_metrics(&mut self, font_px: f32) -> FontMetrics {
        FontMetrics::compute(&mut self.glyphs, font_px)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        buf: &mut [u32],
        buf_width: u32,
        buf_height: u32,
        panes: &[PaneView],
        separators: &[[u32; 4]],
        mode: &InputMode,
        tab_titles: &[(String, bool, bool)],
        metrics: &FontMetrics,
        search_total: usize,
        search_current: usize,
        right_text: Option<&str>,
        pane_title: Option<&str>,
        inactive_dim: f32,
        bell_flash: bool,
        is_logging: bool,
        theme: &ResolvedTheme,
    ) {
        let bg_fill = panes
            .first()
            .map(|p| p.grid.default_bg)
            .unwrap_or(theme.background);
        buf.fill(color_u32(bg_fill));

        for pane in panes {
            self.draw_pane(buf, buf_width, pane, mode, metrics, inactive_dim, theme);
        }

        // Draw separators
        let sep_color = color_u32(theme.separator);
        for &[sx, sy, sw, sh] in separators {
            fill_rect(buf, buf_width, sx, sy, sw, sh, sep_color);
        }

        if bell_flash {
            let flash_color = 0xff_ff_ff_ff_u32;
            let content_top = TAB_BAR_H;
            let content_bot = buf_height.saturating_sub(STATUS_BAR_H);
            for y in content_top..content_bot {
                for x in 0..buf_width {
                    let idx = (y * buf_width + x) as usize;
                    if idx < buf.len() {
                        buf[idx] = blend(buf[idx], flash_color, 38);
                    }
                }
            }
        }

        self.draw_tab_bar(buf, buf_width, tab_titles, theme);
        self.draw_status_bar(
            buf,
            buf_width,
            buf_height,
            mode,
            search_total,
            search_current,
            right_text,
            pane_title,
            is_logging,
            theme,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_pane(
        &mut self,
        buf: &mut [u32],
        buf_width: u32,
        pane: &PaneView,
        mode: &InputMode,
        m: &FontMetrics,
        dim_factor: f32,
        theme: &ResolvedTheme,
    ) {
        let [rx, ry, rw, rh] = pane.rect;
        let grid = pane.grid;

        // Pre-fill gutter pixels so they match the pane background.
        fill_pane_background(buf, buf_width, pane.rect, color_u32(grid.default_bg));

        let selection_range = if pane.is_active {
            match mode {
                InputMode::Visual {
                    start_col,
                    start_row,
                    cur_col,
                    cur_row,
                    anchored: true,
                } => Some((*start_col, *start_row, *cur_col, *cur_row)),
                _ => None,
            }
        } else {
            None
        };

        let sb_len = grid.scrollback.len();

        let mut row = 0usize;
        while row < grid.rows {
            // Absolute row in the combined scrollback+grid address space.
            let abs_row = sb_len.saturating_sub(pane.scroll_offset) + row;
            // Binary-search lower bound for this row's matches (sorted by abs_row).
            let row_match_lo = pane
                .search_matches
                .partition_point(|&(r, _, _)| r < abs_row);

            let mut col = 0usize;
            while col < grid.cols {
                let cell = get_cell(grid, pane.scroll_offset, row, col);

                // Continuation cells are rendered as part of the preceding wide char.
                if cell.wide_cont {
                    col += 1;
                    continue;
                }

                let cell_cols = if cell.wide { 2u32 } else { 1u32 };
                let draw_w = cell_cols * m.cell_width;
                let cell_x = rx + PANE_PADDING + col as u32 * m.cell_width;
                let cell_y = ry + PANE_PADDING + row as u32 * m.cell_height;

                if cell_x + draw_w > rx + rw.saturating_sub(PANE_PADDING)
                    || cell_y + m.cell_height > ry + rh.saturating_sub(PANE_PADDING)
                {
                    col += cell_cols as usize;
                    continue;
                }

                let is_cursor = match mode {
                    InputMode::Visual {
                        cur_col: vc,
                        cur_row: vr,
                        ..
                    } if pane.is_active => col == *vc && row == *vr && pane.blink_visible,
                    _ => pane.show_cursor && col == grid.cursor_col && row == grid.cursor_row,
                };
                let is_selected = selection_range.is_some_and(|(sc, sr, ec, er)| {
                    let (r0, c0, r1, c1) = if (sr, sc) <= (er, ec) {
                        (sr, sc, er, ec)
                    } else {
                        (er, ec, sr, sc)
                    };
                    (row > r0 || (row == r0 && col >= c0)) && (row < r1 || (row == r1 && col <= c1))
                });

                let (in_match, is_current_match) = search_highlight(
                    pane.search_matches,
                    pane.search_current,
                    abs_row,
                    col,
                    row_match_lo,
                );

                // Background stays at full saturation; only foreground is dimmed
                // so inactive panes look de-emphasized without shifting bg color.
                let (bg32, fg) = resolve_cell_colors(
                    cell,
                    is_cursor,
                    is_selected,
                    in_match,
                    is_current_match,
                    pane.cursor_shape,
                    grid.cursor_color,
                    grid.selection_color,
                    theme,
                );

                fill_rect(buf, buf_width, cell_x, cell_y, draw_w, m.cell_height, bg32);

                if cell.c != ' ' && (!cell.blink || pane.blink_visible) {
                    self.draw_glyph(
                        buf,
                        buf_width,
                        cell,
                        cell_x,
                        cell_y,
                        draw_w,
                        fg,
                        bg32,
                        pane.is_active,
                        dim_factor,
                        m,
                        pane.rect,
                    );
                }

                draw_cell_decorations(
                    buf,
                    buf_width,
                    cell,
                    cell_x,
                    cell_y,
                    draw_w,
                    fg,
                    m,
                    pane.rect,
                    theme,
                    pane.is_active,
                    dim_factor,
                    pane.hovered_url,
                );

                if is_cursor && pane.cursor_shape != CursorShape::Block {
                    draw_cursor_overlay(
                        buf,
                        buf_width,
                        pane.cursor_shape,
                        cell_x,
                        cell_y,
                        draw_w,
                        color_u32(grid.cursor_color),
                        m,
                        pane.rect,
                    );
                }

                col += cell_cols as usize;
            }
            row += 1;
        }

        draw_scrollbar(
            buf,
            buf_width,
            pane.rect,
            grid.rows,
            sb_len,
            pane.scroll_offset,
            theme,
        );

        // Images are only shown at live view; col/row coords are meaningless when scrolled.
        if pane.scroll_offset == 0 {
            draw_images(buf, buf_width, pane.rect, &grid.images, m);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_glyph(
        &mut self,
        buf: &mut [u32],
        buf_width: u32,
        cell: &Cell,
        cell_x: u32,
        cell_y: u32,
        draw_w: u32,
        fg: Color,
        bg32: u32,
        pane_is_active: bool,
        dim_factor: f32,
        m: &FontMetrics,
        clip: [u32; 4],
    ) {
        let [rx, ry, rw, rh] = clip;
        let info = self.glyphs.get(cell.c, m.font_px, cell.bold, cell.italic);
        let (gw, gh) = (info.width, info.height);
        let glyph_top = m.baseline as i32 - (gh as i32 + info.ymin);
        let y_offset = glyph_top.max(0) as u32;
        // Center color emoji horizontally within the cell area.
        let x_base = if info.color && gw < draw_w {
            cell_x + (draw_w - gw) / 2
        } else {
            cell_x
        };

        if info.color {
            // RGBA bitmap (color emoji): blit with per-pixel alpha.
            for gy in 0..gh {
                for gx in 0..gw {
                    let base = ((gy * gw + gx) * 4) as usize;
                    let a = info.bitmap[base + 3];
                    if a == 0 {
                        continue;
                    }
                    let r = info.bitmap[base] as u32;
                    let g = info.bitmap[base + 1] as u32;
                    let b = info.bitmap[base + 2] as u32;
                    let sx = x_base + gx;
                    let sy = cell_y + y_offset + gy;
                    if sx >= rx + rw || sy >= ry + rh {
                        continue;
                    }
                    let idx = (sy * buf_width + sx) as usize;
                    if idx < buf.len() {
                        let px = (0xff_u32 << 24) | (r << 16) | (g << 8) | b;
                        let px = if pane_is_active {
                            px
                        } else {
                            dim_color(px, dim_factor)
                        };
                        buf[idx] = blend(bg32, px, a);
                    }
                }
            }
        } else {
            // Grayscale alpha bitmap: blend fg color with background.
            let fg32 = {
                let c = color_u32(fg);
                if pane_is_active {
                    c
                } else {
                    dim_color(c, dim_factor)
                }
            };
            for gy in 0..gh {
                for gx in 0..gw {
                    let alpha = info.bitmap[(gy * gw + gx) as usize];
                    if alpha == 0 {
                        continue;
                    }
                    let sx = x_base + gx;
                    let sy = cell_y + y_offset + gy;
                    if sx >= rx + rw || sy >= ry + rh {
                        continue;
                    }
                    let idx = (sy * buf_width + sx) as usize;
                    if idx < buf.len() {
                        buf[idx] = blend(bg32, fg32, alpha);
                    }
                }
            }
        }
    }

    fn draw_tab_bar(
        &mut self,
        buf: &mut [u32],
        width: u32,
        tabs: &[(String, bool, bool)],
        theme: &ResolvedTheme,
    ) {
        let bar_bg = dim_color(color_u32(theme.background), 0.75);
        let sep_col = color_u32(theme.separator);
        let fp = self.status_font_px;
        let cw = self.glyphs.rasterize('M', fp, false).1;

        // Background
        fill_rect(buf, width, 0, 0, width, TAB_BAR_H, bar_bg);
        // Bottom separator
        fill_rect(buf, width, 0, TAB_BAR_H - 1, width, 1, sep_col);

        let mut cursor_x = 4u32;
        for (label, is_active, has_activity) in tabs {
            let tab_w = label.len() as u32 * cw + 12;
            let (badge_bg, text_color) = if *is_active {
                (color_u32(theme.badge), BADGE_FG)
            } else {
                (
                    dim_color(color_u32(theme.background), 0.85),
                    color_u32(theme.palette[8]),
                )
            };

            // Badge fill
            fill_rect(buf, width, cursor_x, 2, tab_w, TAB_BAR_H - 4, badge_bg);

            // Label
            self.draw_str(
                buf,
                width,
                TAB_BAR_H,
                cursor_x + 6,
                2,
                label,
                fp,
                *is_active,
                text_color,
            );

            // Activity dot: small filled square in the top-right corner of the badge
            if *has_activity && !*is_active {
                const DOT: u32 = 4;
                let dot_color = color_u32(theme.palette[1]); // red/pink
                let dot_x = cursor_x + tab_w.saturating_sub(DOT + 3);
                let dot_y = 4u32;
                fill_rect(buf, width, dot_x, dot_y, DOT, DOT, dot_color);
            }

            cursor_x += tab_w + 2;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_str(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        mut x: u32,
        y: u32,
        s: &str,
        px: f32,
        bold: bool,
        color: u32,
    ) {
        let m_metrics = self.glyphs.metrics('M', px, bold);
        let advance = m_metrics.advance_width.ceil() as u32;
        let ascender = m_metrics.height as u32;
        let baseline = ascender;
        for c in s.chars() {
            let info = self.glyphs.get(c, px, bold, false);
            let (gw, gh) = (info.width, info.height);
            let glyph_top = baseline as i32 - (gh as i32 + info.ymin);
            let cy = (y as i32 + glyph_top).max(0) as u32;
            for gy in 0..gh {
                for gx in 0..gw {
                    let alpha = info.bitmap[(gy * gw + gx) as usize];
                    if alpha == 0 {
                        continue;
                    }
                    let sx = x + gx;
                    let sy = cy + gy;
                    if sx >= bw || sy >= bh {
                        continue;
                    }
                    let idx = (sy * bw + sx) as usize;
                    if idx < buf.len() {
                        buf[idx] = blend(buf[idx], color, alpha);
                    }
                }
            }
            x += advance;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_status_bar(
        &mut self,
        buf: &mut [u32],
        width: u32,
        height: u32,
        mode: &InputMode,
        search_total: usize,
        search_current: usize,
        right_text: Option<&str>,
        pane_title: Option<&str>,
        is_logging: bool,
        theme: &ResolvedTheme,
    ) {
        let bar_y = height.saturating_sub(STATUS_BAR_H);
        let bar_bg = dim_color(color_u32(theme.background), 0.85);
        let sep_color = color_u32(theme.separator);

        fill_rect(buf, width, 0, bar_y, width, height - bar_y, bar_bg);
        if bar_y > 0 {
            fill_rect(buf, width, 0, bar_y, width, 1, sep_color);
        }

        let (label, badge_color) = mode_style(mode, theme);
        let badge_fg = BADGE_FG;
        let px = self.status_font_px;
        let char_w = self.glyphs.rasterize('M', px, true).1;
        let badge_w = label.len() as u32 * char_w + BADGE_PAD_X * 2;
        let badge_h = STATUS_BAR_H - 4;
        let badge_x = 8u32;
        let badge_y = bar_y + 2;

        fill_rect(buf, width, badge_x, badge_y, badge_w, badge_h, badge_color);

        self.draw_badge_label(
            buf,
            width,
            height,
            badge_x + BADGE_PAD_X,
            badge_y,
            badge_h,
            char_w,
            label,
            badge_color,
            badge_fg,
            px,
        );

        // Show search query and match count next to the badge.
        if let InputMode::Search { query } = mode {
            let info = if query.is_empty() {
                "/".to_string()
            } else if search_total == 0 {
                format!("/{query}  [no matches]")
            } else {
                format!("/{query}  [{}/{}]", search_current + 1, search_total)
            };
            self.draw_str(
                buf,
                width,
                height,
                badge_x + badge_w + 10,
                badge_y + 2,
                &info,
                px,
                false,
                color_u32(theme.palette[15]),
            );
        }

        // Show ● REC badge when session logging is active.
        if is_logging {
            let rec_label = "\u{25cf} REC";
            let rec_w = rec_label.len() as u32 * char_w + BADGE_PAD_X * 2;
            let rec_color = color_u32(theme.palette[1]); // red/pink
            let rec_x = badge_x + badge_w + 8;
            fill_rect(buf, width, rec_x, badge_y, rec_w, badge_h, rec_color);
            self.draw_badge_label(
                buf,
                width,
                height,
                rec_x + BADGE_PAD_X,
                badge_y,
                badge_h,
                char_w,
                rec_label,
                rec_color,
                badge_fg,
                px,
            );
        }

        // Show pane OSC title centered in the status bar (suppressed during search).
        if !matches!(mode, InputMode::Search { .. })
            && let Some(title) = pane_title
        {
            let title_w = title.len() as u32 * char_w;
            if title_w < width {
                let title_x = (width - title_w) / 2;
                self.draw_str(
                    buf,
                    width,
                    height,
                    title_x,
                    badge_y + 2,
                    title,
                    px,
                    false,
                    color_u32(theme.palette[8]),
                );
            }
        }

        // Show right-aligned status bar segments (pwd, date, etc.).
        if let Some(text) = right_text {
            let text_w = text.len() as u32 * char_w;
            if let Some(text_x) = width.checked_sub(text_w + 10) {
                self.draw_str(
                    buf,
                    width,
                    height,
                    text_x,
                    badge_y + 2,
                    text,
                    px,
                    false,
                    color_u32(theme.palette[8]),
                );
            }
        }
    }
}

fn get_cell(grid: &Grid, scroll_offset: usize, row: usize, col: usize) -> &Cell {
    if scroll_offset == 0 {
        return grid.cell(col, row);
    }
    let sb_len = grid.scrollback.len();
    let sb_start = sb_len.saturating_sub(scroll_offset);
    let sb_row = sb_start + row;
    if sb_row < sb_len {
        let line = &grid.scrollback[sb_row];
        if col < line.len() {
            &line[col]
        } else {
            &BLANK_CELL
        }
    } else {
        let live_row = sb_row.saturating_sub(sb_len);
        if live_row < grid.rows {
            grid.cell(col, live_row)
        } else {
            &BLANK_CELL
        }
    }
}

// bg will differ per grid but this fallback is only hit for out-of-bounds scrollback
static BLANK_CELL: Cell = Cell {
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

fn mode_style(mode: &InputMode, theme: &ResolvedTheme) -> (&'static str, u32) {
    match mode {
        InputMode::Normal => ("NORMAL", color_u32(theme.palette[4])),
        InputMode::Insert => ("INSERT", color_u32(theme.palette[2])),
        InputMode::Visual { .. } => ("VISUAL", color_u32(theme.palette[5])),
        InputMode::RenameTab { .. } => ("RENAME", color_u32(theme.palette[3])),
        InputMode::Search { .. } => ("SEARCH", color_u32(theme.palette[3])),
        InputMode::CommandPalette { .. } => ("PALETTE", color_u32(theme.palette[6])),
        InputMode::QuitSave => ("INSERT", color_u32(theme.palette[2])),
    }
}

pub(super) fn color_u32(c: Color) -> u32 {
    (0xFF << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}

fn dim_color(c: u32, factor: f32) -> u32 {
    let r = (((c >> 16) & 0xFF) as f32 * factor) as u32;
    let g = (((c >> 8) & 0xFF) as f32 * factor) as u32;
    let b = ((c & 0xFF) as f32 * factor) as u32;
    (0xFF << 24) | (r << 16) | (g << 8) | b
}

fn cell_url_hovered(cell_url: Option<&str>, hovered_url: Option<&str>) -> bool {
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

fn fill_pane_background(buf: &mut [u32], buf_width: u32, rect: [u32; 4], color: u32) {
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
        let t = (y * bw + x + dx) as usize;
        let b = ((y + h - 1) * bw + x + dx) as usize;
        if t < buf.len() {
            buf[t] = color;
        }
        if b < buf.len() {
            buf[b] = color;
        }
    }
    for dy in 0..h {
        let l = ((y + dy) * bw + x) as usize;
        let r = ((y + dy) * bw + x + w - 1) as usize;
        if l < buf.len() {
            buf[l] = color;
        }
        if r < buf.len() {
            buf[r] = color;
        }
    }
}

fn search_highlight(
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
fn resolve_cell_colors(
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
    let bg32 = if is_cursor && cursor_shape == CursorShape::Block {
        color_u32(cursor_color)
    } else if is_selected {
        color_u32(selection_color)
    } else if is_current_match {
        color_u32(theme.search_current)
    } else if in_match {
        color_u32(theme.search_match)
    } else {
        color_u32(bg)
    };
    let fg = if (in_match || is_current_match) && !is_cursor {
        SEARCH_MATCH_FG
    } else {
        fg
    };
    (bg32, fg)
}

#[allow(clippy::too_many_arguments)]
fn draw_cell_decorations(
    buf: &mut [u32],
    buf_width: u32,
    cell: &Cell,
    cell_x: u32,
    cell_y: u32,
    draw_w: u32,
    fg: Color,
    m: &FontMetrics,
    clip: [u32; 4],
    theme: &ResolvedTheme,
    pane_is_active: bool,
    dim_factor: f32,
    hovered_url: Option<&str>,
) {
    let [rx, ry, rw, rh] = clip;
    let fg32 = if pane_is_active {
        color_u32(fg)
    } else {
        dim_color(color_u32(fg), dim_factor)
    };

    let draw_hline = |buf: &mut [u32], y: u32, color: u32| {
        if y < ry || y >= ry + rh {
            return;
        }
        for dx in 0..draw_w {
            let sx = cell_x + dx;
            if sx >= rx + rw {
                break;
            }
            let idx = (y * buf_width + sx) as usize;
            if idx < buf.len() {
                buf[idx] = color;
            }
        }
    };

    if cell.underline {
        draw_hline(buf, cell_y + m.cell_height.saturating_sub(2), fg32);
    }
    if cell.strikethrough {
        draw_hline(buf, cell_y + m.cell_height / 2, fg32);
    }
    if cell.overline {
        draw_hline(buf, cell_y, fg32);
    }

    // OSC 8 hyperlink underline — always shown, brightens on hover.
    if cell.url.is_some() {
        let is_hovered = cell_url_hovered(cell.url.as_ref().map(|s| s.as_str()), hovered_url);
        let link_color = {
            let base = color_u32(theme.palette[4]);
            if is_hovered {
                base
            } else if pane_is_active {
                dim_color(base, 0.65)
            } else {
                dim_color(base, dim_factor)
            }
        };
        draw_hline(buf, cell_y + m.cell_height.saturating_sub(2), link_color);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_cursor_overlay(
    buf: &mut [u32],
    buf_width: u32,
    cursor_shape: CursorShape,
    cell_x: u32,
    cell_y: u32,
    draw_w: u32,
    cur32: u32,
    m: &FontMetrics,
    clip: [u32; 4],
) {
    let [rx, ry, rw, rh] = clip;
    match cursor_shape {
        CursorShape::Underline => {
            let ul_y = cell_y + m.cell_height.saturating_sub(2);
            if ul_y < ry + rh {
                for dx in 0..draw_w {
                    let sx = cell_x + dx;
                    if sx >= rx + rw {
                        break;
                    }
                    let idx = (ul_y * buf_width + sx) as usize;
                    if idx < buf.len() {
                        buf[idx] = cur32;
                    }
                }
            }
        }
        CursorShape::Beam => {
            for dy in 0..m.cell_height {
                let sy = cell_y + dy;
                if sy >= ry + rh {
                    break;
                }
                let sx = cell_x;
                if sx >= rx + rw {
                    break;
                }
                let idx = (sy * buf_width + sx) as usize;
                if idx < buf.len() {
                    buf[idx] = cur32;
                }
            }
        }
        CursorShape::Block => {}
    }
}

fn draw_scrollbar(
    buf: &mut [u32],
    buf_width: u32,
    rect: [u32; 4],
    grid_rows: usize,
    sb_len: usize,
    scroll_offset: usize,
    theme: &ResolvedTheme,
) {
    if sb_len == 0 {
        return;
    }
    let [rx, ry, rw, rh] = rect;
    let scrollbar_x = rx + rw.saturating_sub(2);
    let total = sb_len + grid_rows;
    let thumb_h = ((grid_rows as f32 / total as f32) * rh as f32).max(4.0) as u32;
    let scroll_pos = sb_len.saturating_sub(scroll_offset);
    let thumb_y = ry + ((scroll_pos as f32 / total as f32) * rh as f32) as u32;
    let color = if scroll_offset == 0 {
        color_u32(theme.scrollbar)
    } else {
        color_u32(theme.palette[4])
    };
    let clamped_h = thumb_h.min((ry + rh).saturating_sub(thumb_y));
    fill_rect(buf, buf_width, scrollbar_x, thumb_y, 2, clamped_h, color);
}

fn draw_images(
    buf: &mut [u32],
    buf_width: u32,
    rect: [u32; 4],
    images: &[SixelImage],
    m: &FontMetrics,
) {
    let [rx, ry, rw, rh] = rect;
    for img in images {
        let img_x = rx + PANE_PADDING + img.col as u32 * m.cell_width;
        let img_y = ry + PANE_PADDING + img.row as u32 * m.cell_height;
        for py in 0..img.height {
            for px_i in 0..img.width {
                let base = ((py * img.width + px_i) * 4) as usize;
                if base + 3 >= img.pixels.len() {
                    continue;
                }
                let a = img.pixels[base + 3];
                if a == 0 {
                    continue;
                }
                let r = img.pixels[base] as u32;
                let g = img.pixels[base + 1] as u32;
                let b = img.pixels[base + 2] as u32;
                let sx = img_x + px_i;
                let sy = img_y + py;
                if sx >= rx + rw || sy >= ry + rh {
                    continue;
                }
                let idx = (sy * buf_width + sx) as usize;
                if idx < buf.len() {
                    let src = (0xff_u32 << 24) | (r << 16) | (g << 8) | b;
                    buf[idx] = blend(buf[idx], src, a);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "text_test.rs"]
mod tests;
