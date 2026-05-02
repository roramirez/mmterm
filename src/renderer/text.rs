use super::glyph::GlyphCache;
use crate::input::InputMode;
use crate::terminal::{Color, Grid};
use crate::terminal::grid::Cell;

// Height of the status bar in pixels
const STATUS_BAR_H: u32 = 22;
// Padding inside the pill badge
const BADGE_PAD_X: u32 = 8;

pub struct Renderer {
    pub cell_width: u32,
    pub cell_height: u32,
    font_px: f32,
    status_font_px: f32,
    glyphs: GlyphCache,
}

impl Renderer {
    pub fn new(font_px: f32) -> Self {
        let mut glyphs = GlyphCache::new();
        let (_, w, _) = glyphs.rasterize('M', font_px, false);
        let cell_width = w.max(1);
        let cell_height = (font_px * 1.4) as u32;
        Self {
            cell_width,
            cell_height,
            font_px,
            status_font_px: 13.0,
            glyphs,
        }
    }

    /// Returns (cols, rows) that fit in the given pixel dimensions,
    /// reserving space for the status bar.
    pub fn grid_size(&self, width: u32, height: u32) -> (usize, usize) {
        let usable_h = height.saturating_sub(STATUS_BAR_H);
        let cols = (width / self.cell_width).max(1) as usize;
        let rows = (usable_h / self.cell_height).max(1) as usize;
        (cols, rows)
    }

    /// Draw the terminal grid + status bar into a pixel buffer (0xAARRGGBB u32).
    pub fn draw(
        &mut self,
        buf: &mut [u32],
        buf_width: u32,
        buf_height: u32,
        grid: &Grid,
        scroll_offset: usize,
        mode: &InputMode,
        show_cursor: bool,
    ) {
        buf.fill(color_u32(Color::BLACK));

        let selection_range = match mode {
            InputMode::Visual { start_col, start_row, cur_col, cur_row } => {
                Some((*start_col, *start_row, *cur_col, *cur_row))
            }
            _ => None,
        };

        // ── Terminal grid ────────────────────────────────────────────────────
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                // When scrolled, pull from scrollback; otherwise from live grid
                let cell = get_cell(grid, scroll_offset, row, col);
                let px = col as u32 * self.cell_width;
                let py = row as u32 * self.cell_height;

                if px + self.cell_width > buf_width || py + self.cell_height > buf_height {
                    continue;
                }

                let is_cursor =
                    show_cursor && col == grid.cursor_col && row == grid.cursor_row;
                let is_selected = selection_range.map_or(false, |(sc, sr, ec, er)| {
                    let (r0, c0, r1, c1) = if (sr, sc) <= (er, ec) {
                        (sr, sc, er, ec)
                    } else {
                        (er, ec, sr, sc)
                    };
                    (row > r0 || (row == r0 && col >= c0))
                        && (row < r1 || (row == r1 && col <= c1))
                });

                let bg = if is_cursor {
                    Color::CURSOR
                } else if is_selected {
                    Color::SELECTION
                } else {
                    cell.bg
                };
                let fg = if is_cursor { Color::BLACK } else { cell.fg };

                let bg32 = color_u32(bg);
                for dy in 0..self.cell_height {
                    for dx in 0..self.cell_width {
                        let idx = ((py + dy) * buf_width + px + dx) as usize;
                        if idx < buf.len() {
                            buf[idx] = bg32;
                        }
                    }
                }

                if cell.c == ' ' {
                    continue;
                }

                let (bitmap, gw, gh) =
                    self.glyphs.rasterize(cell.c, self.font_px, cell.bold);
                let baseline = (self.cell_height as f32 * 0.8) as u32;
                let y_offset = baseline.saturating_sub(gh);
                let fg32 = color_u32(fg);

                for gy in 0..gh {
                    for gx in 0..gw {
                        let alpha = bitmap[(gy * gw + gx) as usize];
                        if alpha == 0 {
                            continue;
                        }
                        let sx = px + gx;
                        let sy = py + y_offset + gy;
                        if sx >= buf_width || sy >= buf_height {
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

        // ── Status bar ───────────────────────────────────────────────────────
        self.draw_status_bar(buf, buf_width, buf_height, mode);
    }

    fn draw_status_bar(&mut self, buf: &mut [u32], width: u32, height: u32, mode: &InputMode) {
        let bar_y = height.saturating_sub(STATUS_BAR_H);

        // Background strip
        let bar_bg = 0xFF_18_18_25_u32; // slightly lighter than terminal bg
        for y in bar_y..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                if idx < buf.len() {
                    buf[idx] = bar_bg;
                }
            }
        }

        // Thin separator line
        if bar_y > 0 {
            for x in 0..width {
                let idx = (bar_y * width + x) as usize;
                if idx < buf.len() {
                    buf[idx] = 0xFF_31_32_44;
                }
            }
        }

        // Mode badge
        let (label, badge_color) = mode_style(mode);
        let badge_fg = 0xFF_11_11_1d_u32; // near-black text on colored badge

        let px = self.status_font_px;
        // Measure badge width
        let char_w = self.glyphs.rasterize('M', px, true).1;
        let badge_w = label.len() as u32 * char_w + BADGE_PAD_X * 2;
        let badge_h = STATUS_BAR_H - 4;
        let badge_x = 8u32;
        let badge_y = bar_y + 2;

        // Fill badge rectangle with rounded feel (just fill for now)
        for dy in 0..badge_h {
            for dx in 0..badge_w {
                let sx = badge_x + dx;
                let sy = badge_y + dy;
                let idx = (sy * width + sx) as usize;
                if idx < buf.len() {
                    buf[idx] = badge_color;
                }
            }
        }

        // Render label text on badge
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
    }
}

fn get_cell<'a>(grid: &'a Grid, scroll_offset: usize, row: usize, col: usize) -> &'a Cell {
    if scroll_offset == 0 {
        return grid.cell(col, row);
    }
    let sb_len = grid.scrollback.len();
    // Index into scrollback: the topmost visible line is at sb_len - scroll_offset
    let sb_start = sb_len.saturating_sub(scroll_offset);
    let sb_row = sb_start + row;
    if sb_row < sb_len {
        let line = &grid.scrollback[sb_row];
        if col < line.len() { &line[col] } else { &BLANK_CELL }
    } else {
        // sb exhausted — map remaining rows onto the live screen
        let live_row = sb_row.saturating_sub(sb_len);
        if live_row < grid.rows {
            grid.cell(col, live_row)
        } else {
            &BLANK_CELL
        }
    }
}

static BLANK_CELL: Cell = Cell { c: ' ', fg: Color::WHITE, bg: Color::BLACK, bold: false };

fn mode_style(mode: &InputMode) -> (&'static str, u32) {
    match mode {
        InputMode::Normal => ("NORMAL", 0xFF_89_b4_fa),   // blue
        InputMode::Insert => ("INSERT", 0xFF_a6_e3_a1),   // green
        InputMode::Visual { .. } => ("VISUAL", 0xFF_cb_a6_f7), // purple
    }
}

fn color_u32(c: Color) -> u32 {
    (0xFF << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}

fn blend(bg: u32, fg: u32, alpha: u8) -> u32 {
    let a = alpha as u32;
    let inv = 255 - a;
    let blend_ch = |bg_ch: u32, fg_ch: u32| (fg_ch * a + bg_ch * inv) / 255;
    let br = (bg >> 16) & 0xFF;
    let bg_g = (bg >> 8) & 0xFF;
    let bb = bg & 0xFF;
    let fr = (fg >> 16) & 0xFF;
    let fg_g = (fg >> 8) & 0xFF;
    let fb = fg & 0xFF;
    (0xFF << 24) | (blend_ch(br, fr) << 16) | (blend_ch(bg_g, fg_g) << 8) | blend_ch(bb, fb)
}
