use super::glyph::GlyphCache;
use crate::input::InputMode;
use crate::terminal::grid::Cell;
use crate::terminal::{Color, Grid};
use crate::tui_config::ConfigPanel;
use crate::ui::layout::TAB_BAR_H;

const STATUS_BAR_H: u32 = 22;
const BADGE_PAD_X: u32 = 8;
const SEP_COLOR: u32 = 0xFF_31_32_44;

pub struct PaneView<'a> {
    pub grid: &'a Grid,
    pub rect: [u32; 4],
    pub scroll_offset: usize,
    pub is_active: bool,
    pub show_cursor: bool,
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
            cell_width, cell_height, baseline
        );
        Self { font_px, cell_width: cell_width.max(1), cell_height: cell_height.max(1), baseline }
    }

    pub fn grid_size_for(&self, w: u32, h: u32) -> (usize, usize) {
        ((w / self.cell_width).max(1) as usize, (h / self.cell_height).max(1) as usize)
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
        Self { font_px, status_font_px: 13.0, glyphs }
    }

    /// Compute metrics for a given font size using the shared glyph cache.
    pub fn make_metrics(&mut self, font_px: f32) -> FontMetrics {
        FontMetrics::compute(&mut self.glyphs, font_px)
    }

    pub fn draw(
        &mut self,
        buf: &mut [u32],
        buf_width: u32,
        buf_height: u32,
        panes: &[PaneView],
        separators: &[[u32; 4]],
        mode: &InputMode,
        tab_titles: &[(String, bool)],
        metrics: &FontMetrics,
    ) {
        let bg_fill = panes.first().map(|p| p.grid.default_bg).unwrap_or(Color::BLACK);
        buf.fill(color_u32(bg_fill));

        for pane in panes {
            self.draw_pane(buf, buf_width, pane, mode, metrics);
        }

        // Draw separators
        for &[sx, sy, sw, sh] in separators {
            let color = SEP_COLOR;
            for dy in 0..sh {
                for dx in 0..sw {
                    let idx = ((sy + dy) * buf_width + sx + dx) as usize;
                    if idx < buf.len() {
                        buf[idx] = color;
                    }
                }
            }
        }

        // Active pane: highlight left edge of its separator(s)
        // (top-edge highlight removed — tab bar serves that role)

        self.draw_tab_bar(buf, buf_width, tab_titles);
        self.draw_status_bar(buf, buf_width, buf_height, mode);
    }

    fn draw_pane(&mut self, buf: &mut [u32], buf_width: u32, pane: &PaneView, mode: &InputMode, m: &FontMetrics) {
        let [rx, ry, rw, rh] = pane.rect;
        let grid = pane.grid;

        let selection_range = if pane.is_active {
            match mode {
                InputMode::Visual { start_col, start_row, cur_col, cur_row } => {
                    Some((*start_col, *start_row, *cur_col, *cur_row))
                }
                _ => None,
            }
        } else {
            None
        };

        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let cell = get_cell(grid, pane.scroll_offset, row, col);
                let px = rx + col as u32 * m.cell_width;
                let py = ry + row as u32 * m.cell_height;

                if px + m.cell_width > rx + rw || py + m.cell_height > ry + rh {
                    continue;
                }

                let is_cursor = pane.show_cursor
                    && col == grid.cursor_col
                    && row == grid.cursor_row;
                let is_selected = selection_range.map_or(false, |(sc, sr, ec, er)| {
                    let (r0, c0, r1, c1) =
                        if (sr, sc) <= (er, ec) { (sr, sc, er, ec) } else { (er, ec, sr, sc) };
                    (row > r0 || (row == r0 && col >= c0))
                        && (row < r1 || (row == r1 && col <= c1))
                });

                let bg = if is_cursor {
                    grid.cursor_color
                } else if is_selected {
                    grid.selection_color
                } else {
                    cell.bg
                };
                let fg = if is_cursor { Color::BLACK } else { cell.fg };
                let bg32 = color_u32(bg);

                for dy in 0..m.cell_height {
                    for dx in 0..m.cell_width {
                        let idx = ((py + dy) * buf_width + px + dx) as usize;
                        if idx < buf.len() {
                            buf[idx] = bg32;
                        }
                    }
                }

                if cell.c == ' ' {
                    continue;
                }

                let info = self.glyphs.get(cell.c, m.font_px, cell.bold);
                let (gw, gh) = (info.width, info.height);
                // Place glyph so its baseline aligns with the cell baseline.
                // ymin is pixels from baseline to bottom of bitmap.
                // top of glyph in cell = baseline - (height + ymin)  [ymin usually ≤ 0 for descenders]
                let glyph_top = m.baseline as i32 - (gh as i32 + info.ymin);
                let y_offset = glyph_top.max(0) as u32;
                let fg32 = color_u32(fg);

                for gy in 0..gh {
                    for gx in 0..gw {
                        let alpha = info.bitmap[(gy * gw + gx) as usize];
                        if alpha == 0 { continue; }
                        let sx = px + gx;
                        let sy = py + y_offset + gy;
                        if sx >= rx + rw || sy >= ry + rh { continue; }
                        let idx = (sy * buf_width + sx) as usize;
                        if idx < buf.len() {
                            buf[idx] = blend(bg32, fg32, alpha);
                        }
                    }
                }
            }
        }
    }

    fn draw_tab_bar(&mut self, buf: &mut [u32], width: u32, tabs: &[(String, bool)]) {
        let bar_bg  = 0xFF_11_11_1d_u32;
        let sep_col = 0xFF_31_32_44_u32;
        let fp = self.status_font_px;
        let cw = self.glyphs.rasterize('M', fp, false).1;

        // Background
        for y in 0..TAB_BAR_H {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                if idx < buf.len() { buf[idx] = bar_bg; }
            }
        }
        // Bottom separator
        let sep_y = TAB_BAR_H - 1;
        for x in 0..width {
            let idx = (sep_y * width + x) as usize;
            if idx < buf.len() { buf[idx] = sep_col; }
        }

        let mut cursor_x = 4u32;
        for (label, is_active) in tabs {
            let tab_w = label.len() as u32 * cw + 12;
            let (badge_bg, text_color) = if *is_active {
                (0xFF_89_b4_fa_u32, 0xFF_11_11_1d_u32)
            } else {
                (0xFF_24_25_3a_u32, 0xFF_58_5b_70_u32)
            };

            // Badge fill
            for dy in 2..TAB_BAR_H - 2 {
                for dx in 0..tab_w {
                    let idx = (dy * width + cursor_x + dx) as usize;
                    if idx < buf.len() { buf[idx] = badge_bg; }
                }
            }

            // Label
            self.draw_str(buf, width, TAB_BAR_H, cursor_x + 6, 2, label, fp, *is_active, text_color);
            cursor_x += tab_w + 2;
        }
    }

    pub fn draw_config_panel(&mut self, buf: &mut [u32], bw: u32, bh: u32, panel: &ConfigPanel) {
        // Semi-transparent overlay
        for p in buf.iter_mut() {
            let r = ((*p >> 16) & 0xFF) / 3;
            let g = ((*p >> 8) & 0xFF) / 3;
            let b = (*p & 0xFF) / 3;
            *p = 0xFF_00_00_00 | (r << 16) | (g << 8) | b;
        }

        let fp = self.status_font_px;
        let cw = self.glyphs.rasterize('M', fp, false).1;
        let row_h = (fp * 1.6) as u32 + 4;
        let section_h = row_h - 2;
        let pad = cw;

        let panel_w = (bw as f32 * 0.65) as u32;
        // Fixed panel height: title + footer + visible rows (fit inside window)
        let footer_rows = 2u32; // hint + status
        let max_visible = ((bh.saturating_sub(STATUS_BAR_H + row_h * 2 + row_h * footer_rows)) / row_h).max(4);
        let panel_h = row_h * (max_visible + 2 + footer_rows);
        let px = (bw - panel_w) / 2;
        let py = (bh.saturating_sub(panel_h)) / 2;

        let bg     = 0xFF_1a_1b_26_u32;
        let border = 0xFF_89_b4_fa_u32;

        // Background + border
        for dy in 0..panel_h {
            for dx in 0..panel_w {
                let idx = ((py + dy) * bw + px + dx) as usize;
                if idx < buf.len() { buf[idx] = bg; }
            }
        }
        for dx in 0..panel_w {
            let t = (py * bw + px + dx) as usize;
            let b = ((py + panel_h - 1) * bw + px + dx) as usize;
            if t < buf.len() { buf[t] = border; }
            if b < buf.len() { buf[b] = border; }
        }
        for dy in 0..panel_h {
            let l = ((py + dy) * bw + px) as usize;
            let r = ((py + dy) * bw + px + panel_w - 1) as usize;
            if l < buf.len() { buf[l] = border; }
            if r < buf.len() { buf[r] = border; }
        }

        // Title bar
        self.draw_str(buf, bw, bh, px + pad, py + 4, "CONFIGURATION", fp, true, 0xFF_cb_a6_f7);
        // scroll indicator
        let total = panel.fields.len();
        let scroll_info = format!("{}/{}", panel.selected + 1, total);
        let si_x = px + panel_w - cw * scroll_info.len() as u32 - pad;
        self.draw_str(buf, bw, bh, si_x, py + 4, &scroll_info, fp, false, 0xFF_58_5b_70);

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
            if draw_y + row_h > py + panel_h - row_h * footer_rows { break; }

            // Section header
            if let Some(sec) = field.section {
                // separator line
                for dx in 1..panel_w - 1 {
                    let idx = (draw_y * bw + px + dx) as usize;
                    if idx < buf.len() { buf[idx] = 0xFF_24_25_3a; }
                }
                let sec_label = format!("── {} ", sec);
                self.draw_str(buf, bw, bh, px + pad, draw_y + 1, &sec_label, fp, true, 0xFF_58_5b_70);
                draw_y += section_h;
                if draw_y + row_h > py + panel_h - row_h * footer_rows { break; }
            }

            let is_sel     = i == sel;
            let is_editing = panel.editing && is_sel;

            // Row background
            let row_bg = if is_sel { 0xFF_2a_2b_3d } else { bg };
            for dx in 1..panel_w - 1 {
                for dy in 0..row_h {
                    let idx = ((draw_y + dy) * bw + px + dx) as usize;
                    if idx < buf.len() { buf[idx] = row_bg; }
                }
            }
            if is_sel {
                // left accent bar
                for dy in 0..row_h {
                    let idx = ((draw_y + dy) * bw + px + 1) as usize;
                    if idx < buf.len() { buf[idx] = border; }
                }
            }

            // Color swatch for hex fields
            if field.kind == crate::tui_config::FieldKind::HexColor {
                let hex = panel.display_value(i);
                if let Ok(n) = u32::from_str_radix(hex.trim_start_matches('#'), 16) {
                    let swatch_color = 0xFF_00_00_00 | n;
                    for dy in 2..row_h - 2 {
                        for dx in 0..8 {
                            let sx = px + panel_w - pad - 10 + dx;
                            let sy = draw_y + dy;
                            let idx = (sy * bw + sx) as usize;
                            if idx < buf.len() { buf[idx] = swatch_color; }
                        }
                    }
                }
            }

            let label_color = if is_sel { 0xFF_f9_e2_af } else { 0xFF_ba_c2_de };
            let cursor_str  = if is_editing { "_" } else { "" };
            let text = format!("{:<18} {}{}", field.label, panel.display_value(i), cursor_str);
            self.draw_str(buf, bw, bh, px + pad + 4, draw_y + 2, &text, fp, false, label_color);

            if is_editing {
                let ex = px + panel_w - cw * 7 - pad;
                self.draw_str(buf, bw, bh, ex, draw_y + 2, "[editing]", fp, false, 0xFF_a6_e3_a1);
            }

            draw_y += row_h;
        }

        // Footer: hint + status/help
        let footer_y = py + panel_h - row_h * footer_rows;
        // divider
        for dx in 1..panel_w - 1 {
            let idx = (footer_y * bw + px + dx) as usize;
            if idx < buf.len() { buf[idx] = 0xFF_31_32_44; }
        }

        let hint = format!("hint: {}", panel.fields[panel.selected].hint);
        self.draw_str(buf, bw, bh, px + pad, footer_y + 2, &hint, fp, false, 0xFF_58_5b_70);

        let status_y = py + panel_h - row_h;
        let status = panel.status.as_deref()
            .unwrap_or("j/k: move  Enter/i: edit  Ctrl+S: save  q/Esc: cancel");
        let status_color = if panel.status.is_some() { 0xFF_f3_8b_a8 } else { 0xFF_58_5b_70 };
        self.draw_str(buf, bw, bh, px + pad, status_y, status, fp, false, status_color);
    }

    fn draw_str(
        &mut self, buf: &mut [u32], bw: u32, bh: u32,
        mut x: u32, y: u32, s: &str, px: f32, bold: bool, color: u32,
    ) {
        let m_metrics = self.glyphs.metrics('M', px, bold);
        let advance = m_metrics.advance_width.ceil() as u32;
        let ascender = m_metrics.height as u32;
        let baseline = ascender;
        for c in s.chars() {
            let info = self.glyphs.get(c, px, bold);
            let (gw, gh) = (info.width, info.height);
            let glyph_top = baseline as i32 - (gh as i32 + info.ymin);
            let cy = (y as i32 + glyph_top).max(0) as u32;
            for gy in 0..gh {
                for gx in 0..gw {
                    let alpha = info.bitmap[(gy * gw + gx) as usize];
                    if alpha == 0 { continue; }
                    let sx = x + gx;
                    let sy = cy + gy;
                    if sx >= bw || sy >= bh { continue; }
                    let idx = (sy * bw + sx) as usize;
                    if idx < buf.len() {
                        buf[idx] = blend(buf[idx], color, alpha);
                    }
                }
            }
            x += advance;
        }
    }

    fn draw_status_bar(&mut self, buf: &mut [u32], width: u32, height: u32, mode: &InputMode) {
        let bar_y = height.saturating_sub(STATUS_BAR_H);
        let bar_bg = 0xFF_18_18_25_u32;

        for y in bar_y..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                if idx < buf.len() { buf[idx] = bar_bg; }
            }
        }
        if bar_y > 0 {
            for x in 0..width {
                let idx = (bar_y * width + x) as usize;
                if idx < buf.len() { buf[idx] = 0xFF_31_32_44; }
            }
        }

        let (label, badge_color) = mode_style(mode);
        let badge_fg = 0xFF_11_11_1d_u32;
        let px = self.status_font_px;
        let char_w = self.glyphs.rasterize('M', px, true).1;
        let badge_w = label.len() as u32 * char_w + BADGE_PAD_X * 2;
        let badge_h = STATUS_BAR_H - 4;
        let badge_x = 8u32;
        let badge_y = bar_y + 2;

        for dy in 0..badge_h {
            for dx in 0..badge_w {
                let idx = ((badge_y + dy) * width + badge_x + dx) as usize;
                if idx < buf.len() { buf[idx] = badge_color; }
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
                    if alpha == 0 { continue; }
                    let sx = text_x + gx;
                    let sy = cy + gy;
                    if sx >= width || sy >= height { continue; }
                    let idx = (sy * width + sx) as usize;
                    if idx < buf.len() { buf[idx] = blend(badge_color, badge_fg, alpha); }
                }
            }
            text_x += char_w;
        }
    }
}

fn get_cell<'a>(grid: &'a Grid, scroll_offset: usize, row: usize, col: usize) -> &'a Cell {
    if scroll_offset == 0 {
        return grid.cell(col, row);
    }
    let sb_len = grid.scrollback.len();
    let sb_start = sb_len.saturating_sub(scroll_offset);
    let sb_row = sb_start + row;
    if sb_row < sb_len {
        let line = &grid.scrollback[sb_row];
        if col < line.len() { &line[col] } else { &BLANK_CELL }
    } else {
        let live_row = sb_row.saturating_sub(sb_len);
        if live_row < grid.rows { grid.cell(col, live_row) } else { &BLANK_CELL }
    }
}

// bg will differ per grid but this fallback is only hit for out-of-bounds scrollback
static BLANK_CELL: Cell = Cell { c: ' ', fg: Color::WHITE, bg: Color::rgb(0x12, 0x12, 0x12), bold: false };

fn mode_style(mode: &InputMode) -> (&'static str, u32) {
    match mode {
        InputMode::Normal => ("NORMAL", 0xFF_89_b4_fa),
        InputMode::Insert => ("INSERT", 0xFF_a6_e3_a1),
        InputMode::Visual { .. } => ("VISUAL", 0xFF_cb_a6_f7),
        InputMode::RenameTab { .. } => ("RENAME", 0xFF_f9_e2_af),
    }
}

fn color_u32(c: Color) -> u32 {
    (0xFF << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}

fn blend(bg: u32, fg: u32, alpha: u8) -> u32 {
    let a = alpha as u32;
    let inv = 255 - a;
    let blend_ch = |b: u32, f: u32| (f * a + b * inv) / 255;
    (0xFF << 24)
        | (blend_ch((bg >> 16) & 0xFF, (fg >> 16) & 0xFF) << 16)
        | (blend_ch((bg >> 8) & 0xFF, (fg >> 8) & 0xFF) << 8)
        | blend_ch(bg & 0xFF, fg & 0xFF)
}
