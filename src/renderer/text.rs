use super::blit::{blit_color_glyph, blit_glyph_pixels, blit_gray_glyph};
pub(super) use super::draw_fns::*;
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

fn apply_bell_flash(buf: &mut [u32], buf_width: u32, buf_height: u32, color: u32, intensity: f32) {
    let alpha = (intensity * 55.0).round().clamp(0.0, 255.0) as u8;
    let content_top = TAB_BAR_H;
    let content_bot = buf_height.saturating_sub(STATUS_BAR_H);
    for y in content_top..content_bot {
        for x in 0..buf_width {
            let idx = (y * buf_width + x) as usize;
            if idx < buf.len() {
                buf[idx] = blend(buf[idx], color, alpha);
            }
        }
    }
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
        passthrough: bool,
        tab_titles: &[(String, bool, bool)],
        metrics: &FontMetrics,
        search_total: usize,
        search_current: usize,
        right_text: Option<&str>,
        pane_title: Option<&str>,
        inactive_dim: f32,
        bell_flash_intensity: Option<f32>,
        visual_bell: bool,
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

        if visual_bell && let Some(intensity) = bell_flash_intensity {
            apply_bell_flash(
                buf,
                buf_width,
                buf_height,
                color_u32(theme.foreground),
                intensity,
            );
        }

        self.draw_tab_bar(buf, buf_width, tab_titles, theme);
        self.draw_status_bar(
            buf,
            buf_width,
            buf_height,
            mode,
            passthrough,
            search_total,
            search_current,
            right_text,
            pane_title,
            bell_flash_intensity.is_some(),
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

        for row in 0..grid.rows {
            self.render_row(
                buf,
                buf_width,
                pane,
                mode,
                m,
                dim_factor,
                theme,
                sb_len,
                selection_range,
                row,
            );
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
        let info = self.glyphs.get(cell.c, m.font_px, cell.bold, cell.italic);
        let glyph_top = m.baseline as i32 - (info.height as i32 + info.ymin);
        let y_offset = glyph_top.max(0) as u32;
        let x_base = if info.color && info.width < draw_w {
            cell_x + (draw_w - info.width) / 2
        } else {
            cell_x
        };
        if info.color {
            blit_color_glyph(
                buf,
                buf_width,
                &info.bitmap,
                info.width,
                info.height,
                x_base,
                cell_y,
                y_offset,
                bg32,
                pane_is_active,
                dim_factor,
                clip,
            );
        } else {
            let fg32 = if pane_is_active {
                color_u32(fg)
            } else {
                dim_color(color_u32(fg), dim_factor)
            };
            blit_gray_glyph(
                buf,
                buf_width,
                &info.bitmap,
                info.width,
                info.height,
                x_base,
                cell_y,
                y_offset,
                bg32,
                fg32,
                clip,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_row(
        &mut self,
        buf: &mut [u32],
        buf_width: u32,
        pane: &PaneView,
        mode: &InputMode,
        m: &FontMetrics,
        dim_factor: f32,
        theme: &ResolvedTheme,
        sb_len: usize,
        selection_range: Option<(usize, usize, usize, usize)>,
        row: usize,
    ) {
        let [rx, ry, rw, rh] = pane.rect;
        let grid = pane.grid;
        let abs_row = sb_len.saturating_sub(pane.scroll_offset) + row;
        let row_match_lo = pane
            .search_matches
            .partition_point(|&(r, _, _)| r < abs_row);
        // Precompute row-invariant values used in the tight cell loop.
        let cell_y = ry + PANE_PADDING + row as u32 * m.cell_height;
        let base_x = rx + PANE_PADDING;
        let cursor_color_u32 = color_u32(grid.cursor_color);

        let mut col = 0usize;
        while col < grid.cols {
            let cell = get_cell(grid, pane.scroll_offset, row, col);

            if cell.wide_cont {
                col += 1;
                continue;
            }

            let cell_cols = if cell.wide { 2u32 } else { 1u32 };
            let draw_w = cell_cols * m.cell_width;
            let cell_x = base_x + col as u32 * m.cell_width;

            if cell_out_of_pane_bounds(cell_x, cell_y, draw_w, m.cell_height, rx, ry, rw, rh) {
                col += cell_cols as usize;
                continue;
            }

            let is_cursor = is_cell_cursor(pane, mode, col, row, grid);
            let is_selected = is_cell_selected(selection_range, col, row);

            let (in_match, is_current_match) = search_highlight(
                pane.search_matches,
                pane.search_current,
                abs_row,
                col,
                row_match_lo,
            );

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

            self.draw_cell(
                buf,
                buf_width,
                pane,
                cell,
                cell_x,
                cell_y,
                draw_w,
                is_cursor,
                bg32,
                fg,
                m,
                cursor_color_u32,
                dim_factor,
                theme,
            );

            col += cell_cols as usize;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_cell(
        &mut self,
        buf: &mut [u32],
        buf_width: u32,
        pane: &PaneView,
        cell: &Cell,
        cell_x: u32,
        cell_y: u32,
        draw_w: u32,
        is_cursor: bool,
        bg32: u32,
        fg: Color,
        m: &FontMetrics,
        cursor_color: u32,
        dim_factor: f32,
        theme: &ResolvedTheme,
    ) {
        fill_rect(buf, buf_width, cell_x, cell_y, draw_w, m.cell_height, bg32);
        if should_draw_glyph(cell, pane.blink_visible) {
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
                cursor_color,
                m,
                pane.rect,
            );
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

        let inactive_bg = dim_color(color_u32(theme.background), 0.85);
        let inactive_text = color_u32(theme.palette[8]);
        let active_bg = color_u32(theme.badge);
        let mut cursor_x = 4u32;
        for (label, is_active, has_activity) in tabs {
            let tab_w = label.len() as u32 * cw + 12;
            let (badge_bg, text_color) = if *is_active {
                (active_bg, BADGE_FG)
            } else {
                (inactive_bg, inactive_text)
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
            blit_glyph_pixels(buf, bw, bh, x, cy, gw, gh, &info.bitmap, color);
            x += advance;
        }
    }

    /// Draws a filled badge rectangle followed by a label glyphs inside it.
    #[allow(clippy::too_many_arguments)]
    fn draw_status_badge(
        &mut self,
        buf: &mut [u32],
        width: u32,
        height: u32,
        x: u32,
        y: u32,
        badge_w: u32,
        badge_h: u32,
        label: &str,
        badge_color: u32,
        fg: u32,
        char_w: u32,
        px: f32,
    ) {
        fill_rect(buf, width, x, y, badge_w, badge_h, badge_color);
        self.draw_badge_label(
            buf,
            width,
            height,
            x + BADGE_PAD_X,
            y,
            badge_h,
            char_w,
            label,
            badge_color,
            fg,
            px,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_status_bar(
        &mut self,
        buf: &mut [u32],
        width: u32,
        height: u32,
        mode: &InputMode,
        passthrough: bool,
        search_total: usize,
        search_current: usize,
        right_text: Option<&str>,
        pane_title: Option<&str>,
        bell_active: bool,
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

        let (label, badge_color) = mode_style(mode, passthrough, theme);
        let badge_fg = BADGE_FG;
        let px = self.status_font_px;
        let char_w = self.glyphs.rasterize('M', px, true).1;
        let badge_w = label.len() as u32 * char_w + BADGE_PAD_X * 2;
        let badge_h = STATUS_BAR_H - 4;
        let badge_x = 8u32;
        let badge_y = bar_y + 2;

        self.draw_status_badge(
            buf,
            width,
            height,
            badge_x,
            badge_y,
            badge_w,
            badge_h,
            label,
            badge_color,
            badge_fg,
            char_w,
            px,
        );

        // Show search query and match count next to the badge.
        self.draw_search_info(
            buf,
            width,
            height,
            mode,
            search_total,
            search_current,
            badge_x + badge_w + 10,
            badge_y + 2,
            px,
            color_u32(theme.palette[15]),
        );

        // Show ● REC badge when session logging is active.
        if is_logging {
            let rec_label = "\u{25cf} REC";
            let rec_w = rec_label.len() as u32 * char_w + BADGE_PAD_X * 2;
            let rec_color = color_u32(theme.palette[1]); // red/pink
            let rec_x = badge_x + badge_w + 8;
            self.draw_status_badge(
                buf, width, height, rec_x, badge_y, rec_w, badge_h, rec_label, rec_color, badge_fg,
                char_w, px,
            );
        }

        // Show "●" bell indicator when BEL was recently received.
        if bell_active {
            let dot = "\u{25cf}";
            let dot_x = badge_x + badge_w + 4;
            let dot_y = badge_y + 2;
            self.draw_str(
                buf,
                width,
                height,
                dot_x,
                dot_y,
                dot,
                px,
                false,
                color_u32(theme.palette[3]),
            );
        }

        // Show pane OSC title centered in the status bar (suppressed during search).
        self.draw_pane_title_centered(
            buf,
            width,
            height,
            mode,
            pane_title,
            badge_y,
            char_w,
            px,
            color_u32(theme.palette[8]),
        );

        // Show right-aligned status bar segments (pwd, date, etc.).
        self.draw_right_status(
            buf,
            width,
            height,
            right_text,
            badge_y,
            char_w,
            px,
            color_u32(theme.palette[8]),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_search_info(
        &mut self,
        buf: &mut [u32],
        width: u32,
        height: u32,
        mode: &InputMode,
        search_total: usize,
        search_current: usize,
        x: u32,
        y: u32,
        px: f32,
        color: u32,
    ) {
        if let InputMode::Search { query, history_pos } = mode {
            let base = if query.is_empty() {
                "/".to_string()
            } else if search_total == 0 {
                format!("/{query}  [no matches]")
            } else {
                format!("/{query}  [{}/{}]", search_current + 1, search_total)
            };
            let info = if let Some((pos, len)) = history_pos {
                format!("{base}  [hist {}/{}]", pos + 1, len)
            } else {
                base
            };
            self.draw_str(buf, width, height, x, y, &info, px, false, color);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_pane_title_centered(
        &mut self,
        buf: &mut [u32],
        width: u32,
        height: u32,
        mode: &InputMode,
        pane_title: Option<&str>,
        badge_y: u32,
        char_w: u32,
        px: f32,
        color: u32,
    ) {
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
                    color,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_right_status(
        &mut self,
        buf: &mut [u32],
        width: u32,
        height: u32,
        right_text: Option<&str>,
        badge_y: u32,
        char_w: u32,
        px: f32,
        color: u32,
    ) {
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
                    color,
                );
            }
        }
    }
}

fn is_cell_cursor(pane: &PaneView, mode: &InputMode, col: usize, row: usize, grid: &Grid) -> bool {
    match mode {
        InputMode::Visual {
            cur_col: vc,
            cur_row: vr,
            ..
        } if pane.is_active => col == *vc && row == *vr && pane.blink_visible,
        _ => pane.show_cursor && col == grid.cursor_col && row == grid.cursor_row,
    }
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
    let fg32 = if pane_is_active {
        color_u32(fg)
    } else {
        dim_color(color_u32(fg), dim_factor)
    };

    if cell.underline {
        draw_clipped_hline(
            buf,
            buf_width,
            cell_y + m.cell_height.saturating_sub(2),
            cell_x,
            draw_w,
            clip,
            fg32,
        );
    }
    if cell.strikethrough {
        draw_clipped_hline(
            buf,
            buf_width,
            cell_y + m.cell_height / 2,
            cell_x,
            draw_w,
            clip,
            fg32,
        );
    }
    if cell.overline {
        draw_clipped_hline(buf, buf_width, cell_y, cell_x, draw_w, clip, fg32);
    }
    if cell.url.is_some() {
        let is_hovered = cell_url_hovered(cell.url.as_ref().map(|s| s.as_str()), hovered_url);
        let link_color = link_underline_color(theme, is_hovered, pane_is_active, dim_factor);
        draw_clipped_hline(
            buf,
            buf_width,
            cell_y + m.cell_height.saturating_sub(2),
            cell_x,
            draw_w,
            clip,
            link_color,
        );
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
    match cursor_shape {
        CursorShape::Underline => {
            let ul_y = cell_y + m.cell_height.saturating_sub(2);
            draw_clipped_hline(buf, buf_width, ul_y, cell_x, draw_w, clip, cur32);
        }
        CursorShape::Beam => {
            draw_clipped_vline(buf, buf_width, cell_x, cell_y, m.cell_height, clip, cur32);
        }
        CursorShape::Block => {}
    }
}

fn draw_images(
    buf: &mut [u32],
    buf_width: u32,
    rect: [u32; 4],
    images: &[SixelImage],
    m: &FontMetrics,
) {
    let [rx, ry, ..] = rect;
    for img in images {
        let img_x = rx + PANE_PADDING + img.col as u32 * m.cell_width;
        let img_y = ry + PANE_PADDING + img.row as u32 * m.cell_height;
        for py in 0..img.height {
            for px_i in 0..img.width {
                blit_sixel_pixel(
                    buf,
                    buf_width,
                    img,
                    px_i,
                    py,
                    img_x + px_i,
                    img_y + py,
                    rect,
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "text_test.rs"]
mod tests;
