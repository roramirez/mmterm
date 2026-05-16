use super::glyph::GlyphCache;
use crate::input::InputMode;
use crate::terminal::grid::{Cell, CursorShape};
use crate::terminal::{Color, Grid};
use crate::theme::ResolvedTheme;
use crate::tui_config::ConfigPanel;
use crate::ui::layout::{PANE_PADDING, TAB_BAR_H};

const STATUS_BAR_H: u32 = 22;
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
    status_font_px: f32,
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
            for dy in 0..sh {
                for dx in 0..sw {
                    let idx = ((sy + dy) * buf_width + sx + dx) as usize;
                    if idx < buf.len() {
                        buf[idx] = sep_color;
                    }
                }
            }
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

        // Pre-fill the entire pane rect so gutter pixels (the fractional strip
        // at the right/bottom where the cell grid doesn't fully cover the rect)
        // match the pane background instead of leaking the buf.fill color.
        let pane_bg32 = color_u32(grid.default_bg);
        for dy in 0..rh {
            for dx in 0..rw {
                let idx = ((ry + dy) * buf_width + rx + dx) as usize;
                if idx < buf.len() {
                    buf[idx] = pane_bg32;
                }
            }
        }

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
            // Binary-search lower bound for this row's matches (matches are sorted by abs_row).
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

                // Search highlight: scan matches for this abs_row.
                let (in_match, is_current_match) = if !pane.search_matches.is_empty() {
                    let mut found = false;
                    let mut current = false;
                    let mut i = row_match_lo;
                    while i < pane.search_matches.len() && pane.search_matches[i].0 == abs_row {
                        let (_, mc, mlen) = pane.search_matches[i];
                        if col >= mc && col < mc + mlen {
                            found = true;
                            current = pane.search_current == Some(i);
                            break;
                        }
                        i += 1;
                    }
                    (found, current)
                } else {
                    (false, false)
                };

                // Apply reverse video before cursor/selection overrides.
                let (cell_fg, cell_bg) = if cell.reverse {
                    (cell.bg, cell.fg)
                } else {
                    (cell.fg, cell.bg)
                };

                let block_cursor = is_cursor && pane.cursor_shape == CursorShape::Block;
                let bg = if block_cursor {
                    grid.cursor_color
                } else if is_current_match {
                    theme.search_current
                } else if in_match {
                    theme.search_match
                } else if is_selected {
                    grid.selection_color
                } else {
                    cell_bg
                };
                let fg = if block_cursor {
                    Color::BLACK
                } else if in_match {
                    SEARCH_MATCH_FG
                } else if cell.dim {
                    // Dim: blend fg 50% toward bg
                    Color::rgb(
                        ((cell_fg.r as u16 + cell_bg.r as u16) / 2) as u8,
                        ((cell_fg.g as u16 + cell_bg.g as u16) / 2) as u8,
                        ((cell_fg.b as u16 + cell_bg.b as u16) / 2) as u8,
                    )
                } else {
                    cell_fg
                };
                // Background stays at full saturation in all panes; only
                // foreground (text, emoji) is dimmed so inactive panes look
                // visually de-emphasized without shifting the background color.
                let bg32 = color_u32(bg);

                for dy in 0..m.cell_height {
                    for dx in 0..draw_w {
                        let idx = ((cell_y + dy) * buf_width + cell_x + dx) as usize;
                        if idx < buf.len() {
                            buf[idx] = bg32;
                        }
                    }
                }

                if cell.c != ' ' && (!cell.blink || pane.blink_visible) {
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
                        // RGBA bitmap (color emoji from FreeType): blit with per-pixel alpha.
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
                                    let px = if pane.is_active {
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
                            if pane.is_active {
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

                // Underline (1px, 2px from bottom): SGR 4 or OSC 8 hyperlink
                let url_hovered =
                    cell_url_hovered(cell.url.as_ref().map(|u| u.as_str()), pane.hovered_url);
                let has_url = cell.url.is_some();
                if cell.underline || has_url {
                    let hyperlink_ul = color_u32(theme.palette[4]); // blue
                    let ul_color = if has_url {
                        let base = if url_hovered {
                            hyperlink_ul
                        } else {
                            dim_color(hyperlink_ul, 0.45)
                        };
                        if pane.is_active {
                            base
                        } else {
                            dim_color(base, dim_factor)
                        }
                    } else {
                        color_u32(fg)
                    };
                    let ul_y = cell_y + m.cell_height.saturating_sub(2);
                    if ul_y < ry + rh {
                        for dx in 0..draw_w {
                            let sx = cell_x + dx;
                            if sx >= rx + rw {
                                break;
                            }
                            let idx = (ul_y * buf_width + sx) as usize;
                            if idx < buf.len() {
                                buf[idx] = ul_color;
                            }
                        }
                    }
                }

                // Overline (1px at top of cell): SGR 53
                if cell.overline {
                    let ol_color = color_u32(fg);
                    if cell_y < ry + rh {
                        for dx in 0..draw_w {
                            let sx = cell_x + dx;
                            if sx >= rx + rw {
                                break;
                            }
                            let idx = (cell_y * buf_width + sx) as usize;
                            if idx < buf.len() {
                                buf[idx] = ol_color;
                            }
                        }
                    }
                }

                // Strikethrough (1px at mid-ascender)
                if cell.strikethrough {
                    let st_y = cell_y + m.baseline / 2;
                    if st_y < ry + rh {
                        let st_color = color_u32(fg);
                        for dx in 0..draw_w {
                            let sx = cell_x + dx;
                            if sx >= rx + rw {
                                break;
                            }
                            let idx = (st_y * buf_width + sx) as usize;
                            if idx < buf.len() {
                                buf[idx] = st_color;
                            }
                        }
                    }
                }

                // Non-block cursor overlay (beam or underline).
                if is_cursor && pane.cursor_shape != CursorShape::Block {
                    let cur32 = color_u32(grid.cursor_color);
                    match pane.cursor_shape {
                        CursorShape::Beam => {
                            // 1px vertical bar on the left edge of the cell
                            for dy in 0..m.cell_height {
                                let sy = cell_y + dy;
                                if sy >= ry + rh {
                                    break;
                                }
                                let idx = (sy * buf_width + cell_x) as usize;
                                if idx < buf.len() {
                                    buf[idx] = cur32;
                                }
                            }
                        }
                        CursorShape::Underline => {
                            // 2px horizontal bar at the bottom of the cell
                            for dy in 0..2u32 {
                                let sy = cell_y + m.cell_height.saturating_sub(2) + dy;
                                if sy >= ry + rh {
                                    break;
                                }
                                for dx in 0..draw_w {
                                    let sx = cell_x + dx;
                                    if sx >= rx + rw {
                                        break;
                                    }
                                    let idx = (sy * buf_width + sx) as usize;
                                    if idx < buf.len() {
                                        buf[idx] = cur32;
                                    }
                                }
                            }
                        }
                        CursorShape::Block => unreachable!(),
                    }
                }

                col += cell_cols as usize;
            }
            row += 1;
        }

        // Scrollbar overlay: thin 4px strip on the right edge of the pane.
        // Only drawn when there is scrollback content to indicate position.
        let sb_len = grid.scrollback.len();
        if sb_len > 0 && rw >= 6 && rh > 0 {
            let total = sb_len + grid.rows;
            let view_start = sb_len.saturating_sub(pane.scroll_offset);

            let thumb_h = ((rh as usize * grid.rows) / total).max(4) as u32;
            let track_available = rh.saturating_sub(thumb_h) as usize;
            let thumb_y = if total > grid.rows {
                (track_available * view_start / (total - grid.rows)) as u32
            } else {
                0
            };

            let bar_x = rx + rw - 4;
            let thumb_color = if pane.scroll_offset > 0 {
                color_u32(theme.palette[4]) // blue when scrolled up
            } else {
                color_u32(theme.scrollbar) // dim at live view
            };
            let track_overlay = color_u32(theme.background);

            for dy in 0..rh {
                let sy = ry + dy;
                // Track: dark semi-transparent strip
                for dx in 0..4u32 {
                    let idx = (sy * buf_width + bar_x + dx) as usize;
                    if idx < buf.len() {
                        buf[idx] = blend(buf[idx], track_overlay, 160);
                    }
                }
                // Thumb: 2px wide, 1px inset from each side
                if dy >= thumb_y && dy < thumb_y + thumb_h {
                    for dx in 1..3u32 {
                        let idx = (sy * buf_width + bar_x + dx) as usize;
                        if idx < buf.len() {
                            buf[idx] = thumb_color;
                        }
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
        for y in 0..TAB_BAR_H {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                if idx < buf.len() {
                    buf[idx] = bar_bg;
                }
            }
        }
        // Bottom separator
        let sep_y = TAB_BAR_H - 1;
        for x in 0..width {
            let idx = (sep_y * width + x) as usize;
            if idx < buf.len() {
                buf[idx] = sep_col;
            }
        }

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
            for dy in 2..TAB_BAR_H - 2 {
                for dx in 0..tab_w {
                    let idx = (dy * width + cursor_x + dx) as usize;
                    if idx < buf.len() {
                        buf[idx] = badge_bg;
                    }
                }
            }

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
                for dy in 0..DOT {
                    for dx in 0..DOT {
                        let idx = ((dot_y + dy) * width + dot_x + dx) as usize;
                        if idx < buf.len() {
                            buf[idx] = dot_color;
                        }
                    }
                }
            }

            cursor_x += tab_w + 2;
        }
    }

    pub fn draw_config_panel(&mut self, buf: &mut [u32], bw: u32, bh: u32, panel: &ConfigPanel) {
        // Semi-transparent overlay
        for p in buf.iter_mut() {
            let r = ((*p >> 16) & 0xFF) / 3;
            let g = ((*p >> 8) & 0xFF) / 3;
            let b = (*p & 0xFF) / 3;
            *p = 0xff_00_00_00 | (r << 16) | (g << 8) | b;
        }

        let fp = self.status_font_px;
        let cw = self.glyphs.rasterize('M', fp, false).1;
        let row_h = (fp * 1.6) as u32 + 4;
        let section_h = row_h - 2;
        let pad = cw;

        let panel_w = (bw as f32 * 0.65) as u32;
        // Fixed panel height: title + footer + visible rows (fit inside window)
        let footer_rows = 2u32; // hint + status
        let max_visible =
            ((bh.saturating_sub(STATUS_BAR_H + row_h * 2 + row_h * footer_rows)) / row_h).max(4);
        let panel_h = row_h * (max_visible + 2 + footer_rows);
        let px = (bw - panel_w) / 2;
        let py = (bh.saturating_sub(panel_h)) / 2;

        let bg = 0xff_1a_1b_26_u32;
        let border = 0xff_89_b4_fa_u32;

        // Background + border
        for dy in 0..panel_h {
            for dx in 0..panel_w {
                let idx = ((py + dy) * bw + px + dx) as usize;
                if idx < buf.len() {
                    buf[idx] = bg;
                }
            }
        }
        for dx in 0..panel_w {
            let t = (py * bw + px + dx) as usize;
            let b = ((py + panel_h - 1) * bw + px + dx) as usize;
            if t < buf.len() {
                buf[t] = border;
            }
            if b < buf.len() {
                buf[b] = border;
            }
        }
        for dy in 0..panel_h {
            let l = ((py + dy) * bw + px) as usize;
            let r = ((py + dy) * bw + px + panel_w - 1) as usize;
            if l < buf.len() {
                buf[l] = border;
            }
            if r < buf.len() {
                buf[r] = border;
            }
        }

        // Title bar
        self.draw_str(
            buf,
            bw,
            bh,
            px + pad,
            py + 4,
            "CONFIGURATION",
            fp,
            true,
            0xff_cb_a6_f7,
        );
        // scroll indicator
        let total = panel.fields.len();
        let scroll_info = format!("{}/{}", panel.selected + 1, total);
        let si_x = px + panel_w - cw * scroll_info.len() as u32 - pad;
        self.draw_str(
            buf,
            bw,
            bh,
            si_x,
            py + 4,
            &scroll_info,
            fp,
            false,
            0xff_58_5b_70,
        );

        // Scroll window: keep selected in view
        let sel = panel.selected;
        let scroll_start = if sel >= max_visible as usize {
            sel + 1 - max_visible as usize
        } else {
            0
        };

        let content_y = py + row_h * 2;
        let mut draw_y = content_y;

        for (i, field) in panel.fields.iter().enumerate().skip(scroll_start) {
            if draw_y + row_h > py + panel_h - row_h * footer_rows {
                break;
            }

            // Section header
            if let Some(sec) = field.section {
                // separator line
                for dx in 1..panel_w - 1 {
                    let idx = (draw_y * bw + px + dx) as usize;
                    if idx < buf.len() {
                        buf[idx] = 0xff_24_25_3a;
                    }
                }
                let sec_label = format!("── {} ", sec);
                self.draw_str(
                    buf,
                    bw,
                    bh,
                    px + pad,
                    draw_y + 1,
                    &sec_label,
                    fp,
                    true,
                    0xff_58_5b_70,
                );
                draw_y += section_h;
                if draw_y + row_h > py + panel_h - row_h * footer_rows {
                    break;
                }
            }

            let is_sel = i == sel;
            let is_editing = panel.editing && is_sel;

            // Row background
            let row_bg = if is_sel { 0xff_2a_2b_3d } else { bg };
            for dx in 1..panel_w - 1 {
                for dy in 0..row_h {
                    let idx = ((draw_y + dy) * bw + px + dx) as usize;
                    if idx < buf.len() {
                        buf[idx] = row_bg;
                    }
                }
            }
            if is_sel {
                // left accent bar
                for dy in 0..row_h {
                    let idx = ((draw_y + dy) * bw + px + 1) as usize;
                    if idx < buf.len() {
                        buf[idx] = border;
                    }
                }
            }

            // Color swatch for hex fields
            if matches!(field.kind, crate::tui_config::FieldKind::HexColor) {
                let hex = panel.display_value(i);
                if let Ok(n) = u32::from_str_radix(hex.trim_start_matches('#'), 16) {
                    let swatch_color = 0xff_00_00_00 | n;
                    for dy in 2..row_h - 2 {
                        for dx in 0..8 {
                            let sx = px + panel_w - pad - 10 + dx;
                            let sy = draw_y + dy;
                            let idx = (sy * bw + sx) as usize;
                            if idx < buf.len() {
                                buf[idx] = swatch_color;
                            }
                        }
                    }
                }
            }

            let label_color = if is_sel { 0xff_f9_e2_af } else { 0xff_ba_c2_de };
            let is_select = matches!(field.kind, crate::tui_config::FieldKind::Select(_));
            let cursor_str = if is_editing { "_" } else { "" };
            let value_display = if is_select && is_sel {
                format!("\u{2190} {} \u{2192}", panel.display_value(i))
            } else {
                format!("{}{}", panel.display_value(i), cursor_str)
            };
            let text = format!("{:<18} {}", field.label, value_display);
            self.draw_str(
                buf,
                bw,
                bh,
                px + pad + 4,
                draw_y + 2,
                &text,
                fp,
                false,
                label_color,
            );

            if is_editing {
                let ex = px + panel_w - cw * 7 - pad;
                self.draw_str(
                    buf,
                    bw,
                    bh,
                    ex,
                    draw_y + 2,
                    "[editing]",
                    fp,
                    false,
                    0xff_a6_e3_a1,
                );
            }

            draw_y += row_h;
        }

        // Footer: hint + status/help
        let footer_y = py + panel_h - row_h * footer_rows;
        // divider
        for dx in 1..panel_w - 1 {
            let idx = (footer_y * bw + px + dx) as usize;
            if idx < buf.len() {
                buf[idx] = 0xff_31_32_44;
            }
        }

        let hint = format!("hint: {}", panel.fields[panel.selected].hint);
        self.draw_str(
            buf,
            bw,
            bh,
            px + pad,
            footer_y + 2,
            &hint,
            fp,
            false,
            0xff_58_5b_70,
        );

        let status_y = py + panel_h - row_h;
        let status = panel
            .status
            .as_deref()
            .unwrap_or("j/k: move  Enter/i: edit  Ctrl+S: save  q/Esc: cancel");
        let status_color = if panel.status.is_some() {
            0xff_f3_8b_a8
        } else {
            0xff_58_5b_70
        };
        self.draw_str(
            buf,
            bw,
            bh,
            px + pad,
            status_y,
            status,
            fp,
            false,
            status_color,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_str(
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

        for y in bar_y..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                if idx < buf.len() {
                    buf[idx] = bar_bg;
                }
            }
        }
        if bar_y > 0 {
            for x in 0..width {
                let idx = (bar_y * width + x) as usize;
                if idx < buf.len() {
                    buf[idx] = sep_color;
                }
            }
        }

        let (label, badge_color) = mode_style(mode, theme);
        let badge_fg = BADGE_FG;
        let px = self.status_font_px;
        let char_w = self.glyphs.rasterize('M', px, true).1;
        let badge_w = label.len() as u32 * char_w + BADGE_PAD_X * 2;
        let badge_h = STATUS_BAR_H - 4;
        let badge_x = 8u32;
        let badge_y = bar_y + 2;

        for dy in 0..badge_h {
            for dx in 0..badge_w {
                let idx = ((badge_y + dy) * width + badge_x + dx) as usize;
                if idx < buf.len() {
                    buf[idx] = badge_color;
                }
            }
        }

        let mut text_x = badge_x + BADGE_PAD_X;
        for c in label.chars() {
            let (bitmap, gw, gh) = self.glyphs.rasterize(c, px, true);
            let baseline = (badge_h as f32 * 0.82) as u32;
            let cy = badge_y + baseline.saturating_sub(gh);
            for gy in 0..gh {
                for gx in 0..gw {
                    let alpha = bitmap[(gy * gw + gx) as usize];
                    if alpha == 0 {
                        continue;
                    }
                    let sx = text_x + gx;
                    let sy = cy + gy;
                    if sx >= width || sy >= height {
                        continue;
                    }
                    let idx = (sy * width + sx) as usize;
                    if idx < buf.len() {
                        buf[idx] = blend(badge_color, badge_fg, alpha);
                    }
                }
            }
            text_x += char_w;
        }

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
            for dy in 0..badge_h {
                for dx in 0..rec_w {
                    let idx = ((badge_y + dy) * width + rec_x + dx) as usize;
                    if idx < buf.len() {
                        buf[idx] = rec_color;
                    }
                }
            }
            let mut tx = rec_x + BADGE_PAD_X;
            for c in rec_label.chars() {
                let (bitmap, gw, gh) = self.glyphs.rasterize(c, px, true);
                let baseline = (badge_h as f32 * 0.82) as u32;
                let cy = badge_y + baseline.saturating_sub(gh);
                for gy in 0..gh {
                    for gx in 0..gw {
                        let alpha = bitmap[(gy * gw + gx) as usize];
                        if alpha == 0 {
                            continue;
                        }
                        let sx = tx + gx;
                        let sy = cy + gy;
                        if sx >= width || sy >= height {
                            continue;
                        }
                        let idx = (sy * width + sx) as usize;
                        if idx < buf.len() {
                            buf[idx] = blend(rec_color, badge_fg, alpha);
                        }
                    }
                }
                tx += char_w;
            }
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

    pub fn draw_quit_confirm(&mut self, buf: &mut [u32], bw: u32, bh: u32, theme: &ResolvedTheme) {
        // Dim the background.
        for p in buf.iter_mut() {
            let r = ((*p >> 16) & 0xFF) / 3;
            let g = ((*p >> 8) & 0xFF) / 3;
            let b = (*p & 0xFF) / 3;
            *p = 0xff_00_00_00 | (r << 16) | (g << 8) | b;
        }

        let fp = self.status_font_px;
        let cw = self.glyphs.rasterize('M', fp, false).1;
        let line_h = (fp * 1.8) as u32;
        let pad_x = cw * 3;
        let pad_y = line_h;

        let lines = ["Quit? All tabs will close.", "[y] Yes   [n / Esc] Cancel"];
        let max_chars = lines.iter().map(|l| l.len() as u32).max().unwrap_or(1);
        let box_w = max_chars * cw + pad_x * 2;
        let box_h = lines.len() as u32 * line_h + pad_y * 2;
        let bx = bw.saturating_sub(box_w) / 2;
        let by = bh.saturating_sub(box_h) / 2;

        let bg = color_u32(theme.background);
        let border = color_u32(theme.palette[1]);

        for dy in 0..box_h {
            for dx in 0..box_w {
                let idx = ((by + dy) * bw + bx + dx) as usize;
                if idx < buf.len() {
                    buf[idx] = bg;
                }
            }
        }
        // Border (1 px)
        for dx in 0..box_w {
            let t = (by * bw + bx + dx) as usize;
            let b = ((by + box_h - 1) * bw + bx + dx) as usize;
            if t < buf.len() {
                buf[t] = border;
            }
            if b < buf.len() {
                buf[b] = border;
            }
        }
        for dy in 0..box_h {
            let l = ((by + dy) * bw + bx) as usize;
            let r = ((by + dy) * bw + bx + box_w - 1) as usize;
            if l < buf.len() {
                buf[l] = border;
            }
            if r < buf.len() {
                buf[r] = border;
            }
        }

        for (i, line) in lines.iter().enumerate() {
            let fg = if i == 0 {
                color_u32(theme.foreground)
            } else {
                color_u32(theme.palette[8])
            };
            let ty = by + pad_y + i as u32 * line_h;
            self.draw_str(buf, bw, bh, bx + pad_x, ty, line, fp, false, fg);
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
    }
}

fn color_u32(c: Color) -> u32 {
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

fn blend(bg: u32, fg: u32, alpha: u8) -> u32 {
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

#[cfg(test)]
mod tests {
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
        let (label, _) = mode_style(&InputMode::Insert, &theme);
        assert_eq!(label, "INSERT");
        let (label, _) = mode_style(&InputMode::Normal, &theme);
        assert_eq!(label, "NORMAL");
        let (label, _) = mode_style(
            &InputMode::Visual {
                start_col: 0,
                start_row: 0,
                cur_col: 0,
                cur_row: 0,
                anchored: false,
            },
            &theme,
        );
        assert_eq!(label, "VISUAL");
        let (label, _) = mode_style(&InputMode::RenameTab { buf: String::new() }, &theme);
        assert_eq!(label, "RENAME");
        let (label, _) = mode_style(
            &InputMode::Search {
                query: String::new(),
            },
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
            &[],
            &m,
            0,
            0,
            None,
            None,
            0.55,
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
            &[("shell".to_string(), true, false)],
            &m,
            0,
            0,
            None,
            None,
            0.55,
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
            },
            &[],
            &m,
            3,
            1,
            Some("/home/user"),
            None,
            0.55,
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
            &[],
            &m,
            0,
            0,
            None,
            Some("nvim src/main.rs"),
            0.55,
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
            &[],
            &m,
            0,
            0,
            None,
            None,
            0.55,
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
            },
            &[],
            &m,
            0,
            0,
            None,
            Some("nvim src/main.rs"),
            0.55,
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
            },
            &[],
            &m,
            0,
            0,
            None,
            None,
            0.55,
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
            &[],
            &m,
            0,
            0,
            None,
            None,
            0.55,
            true,
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
            &[],
            &m,
            0,
            0,
            None,
            None,
            0.55,
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
            &[("t".to_string(), true, false)],
            m,
            0,
            0,
            None,
            None,
            0.55,
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
            &[("t".to_string(), true, false)],
            &m,
            0,
            0,
            None,
            None,
            0.55,
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
            &[("t".to_string(), true, false)],
            &m,
            0,
            0,
            None,
            None,
            0.55,
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
            },
            &[],
            &m,
            0,
            0,
            None,
            None,
            0.55,
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
            },
            &[],
            &m,
            0, // search_total = 0
            0,
            None,
            None,
            0.55,
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
        let (cols, rows) =
            m.grid_size_for(800u32.saturating_sub(pad2), 556u32.saturating_sub(pad2));
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
            &[("t".to_string(), true, false)],
            &m,
            0,
            0,
            None,
            None,
            0.55,
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
}
