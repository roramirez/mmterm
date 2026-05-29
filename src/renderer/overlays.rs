use crate::theme::ResolvedTheme;
use crate::tui_config::ConfigPanel;

use super::text::{
    Renderer, STATUS_BAR_H, blend, color_u32, dim_buffer, draw_rect_border, fill_rect,
};

// ── Screenshot helpers ───────────────────────────────────────────────────────

fn dim_row_range(buf: &mut [u32], bw: u32, row: u32, col_start: u32, col_end: u32) {
    const VEIL: u32 = 0x99_00_00_00;
    for col in col_start..col_end {
        let idx = (row * bw + col) as usize;
        if idx < buf.len() {
            buf[idx] = blend(buf[idx], VEIL, 0x99);
        }
    }
}

fn dim_outside_rect(
    buf: &mut [u32],
    bw: u32,
    bh: u32,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
) {
    for row in 0..bh {
        if row < top || row >= bottom {
            dim_row_range(buf, bw, row, 0, bw);
        } else {
            dim_row_range(buf, bw, row, 0, left);
            dim_row_range(buf, bw, row, right, bw);
        }
    }
}

fn hint_text_y(top: u32, bottom: u32, line_h: u32, bh: u32) -> u32 {
    if bottom + 4 + line_h <= bh {
        bottom + 4
    } else if top >= line_h + 4 {
        top - line_h - 4
    } else {
        bh.saturating_sub(line_h + 4)
    }
}

struct FieldRowLayout {
    px: u32,
    panel_w: u32,
    pad: u32,
    cw: u32,
    fp: f32,
    row_h: u32,
    bg: u32,
    border: u32,
    sel: usize,
}

fn field_value_display(
    panel: &ConfigPanel,
    i: usize,
    is_select: bool,
    is_sel: bool,
    is_editing: bool,
) -> String {
    if is_select && is_sel {
        format!("\u{2190} {} \u{2192}", panel.display_value(i))
    } else {
        format!(
            "{}{}",
            panel.display_value(i),
            if is_editing { "_" } else { "" }
        )
    }
}

fn draw_hex_color_swatch(
    buf: &mut [u32],
    bw: u32,
    panel: &ConfigPanel,
    i: usize,
    draw_y: u32,
    l: &FieldRowLayout,
) {
    if !matches!(panel.fields[i].kind, crate::tui_config::FieldKind::HexColor) {
        return;
    }
    let hex = panel.display_value(i);
    let Ok(n) = u32::from_str_radix(hex.trim_start_matches('#'), 16) else {
        return;
    };
    fill_rect(
        buf,
        bw,
        l.px + l.panel_w - l.pad - 10,
        draw_y + 2,
        8,
        l.row_h - 4,
        0xff_00_00_00 | n,
    );
}

fn badge_pixel(buf: &mut [u32], bw: u32, bh: u32, sx: u32, sy: u32, color: u32) {
    if sx >= bw || sy >= bh {
        return;
    }
    let idx = (sy * bw + sx) as usize;
    if idx < buf.len() {
        buf[idx] = color;
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_glyph_badge(
    buf: &mut [u32],
    bw: u32,
    bh: u32,
    ox: u32,
    oy: u32,
    gw: u32,
    gh: u32,
    bitmap: &[u8],
    badge_color: u32,
    fg: u32,
) {
    for gy in 0..gh {
        for gx in 0..gw {
            let alpha = bitmap[(gy * gw + gx) as usize];
            if alpha == 0 {
                continue;
            }
            badge_pixel(buf, bw, bh, ox + gx, oy + gy, blend(badge_color, fg, alpha));
        }
    }
}

impl Renderer {
    pub fn draw_config_panel(&mut self, buf: &mut [u32], bw: u32, bh: u32, panel: &ConfigPanel) {
        dim_buffer(buf);

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

        fill_rect(buf, bw, px, py, panel_w, panel_h, bg);
        draw_rect_border(buf, bw, px, py, panel_w, panel_h, border);

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
        let scroll_start = sel.saturating_sub(max_visible as usize - 1);
        let layout = FieldRowLayout {
            px,
            panel_w,
            pad,
            cw,
            fp,
            row_h,
            bg,
            border,
            sel,
        };
        let clip_y = py + panel_h - row_h * footer_rows;

        let content_y = py + row_h * 2;
        let mut draw_y = content_y;

        for (i, field) in panel.fields.iter().enumerate().skip(scroll_start) {
            if draw_y + row_h > clip_y {
                break;
            }

            // Section header
            if let Some(sec) = field.section {
                fill_rect(buf, bw, px + 1, draw_y, panel_w - 2, 1, 0xff_24_25_3a);
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
                if draw_y + row_h > clip_y {
                    break;
                }
            }

            self.draw_config_field_row(buf, bw, bh, panel, i, draw_y, &layout);
            draw_y += row_h;
        }

        // Footer: hint + status/help
        let footer_y = py + panel_h - row_h * footer_rows;
        fill_rect(buf, bw, px + 1, footer_y, panel_w - 2, 1, 0xff_31_32_44);

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
    fn draw_config_field_row(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        panel: &ConfigPanel,
        i: usize,
        draw_y: u32,
        l: &FieldRowLayout,
    ) {
        let field = &panel.fields[i];
        let is_sel = i == l.sel;
        let is_editing = panel.editing && is_sel;

        let row_bg = if is_sel { 0xff_2a_2b_3d } else { l.bg };
        fill_rect(buf, bw, l.px + 1, draw_y, l.panel_w - 2, l.row_h, row_bg);
        if is_sel {
            fill_rect(buf, bw, l.px + 1, draw_y, 1, l.row_h, l.border);
        }

        draw_hex_color_swatch(buf, bw, panel, i, draw_y, l);

        let label_color = if is_sel { 0xff_f9_e2_af } else { 0xff_ba_c2_de };
        let is_select = matches!(field.kind, crate::tui_config::FieldKind::Select(_));
        let value_display = field_value_display(panel, i, is_select, is_sel, is_editing);
        let text = format!("{:<18} {}", field.label, value_display);
        self.draw_str(
            buf,
            bw,
            bh,
            l.px + l.pad + 4,
            draw_y + 2,
            &text,
            l.fp,
            false,
            label_color,
        );

        if is_editing {
            let ex = l.px + l.panel_w - l.cw * 7 - l.pad;
            self.draw_str(
                buf,
                bw,
                bh,
                ex,
                draw_y + 2,
                "[editing]",
                l.fp,
                false,
                0xff_a6_e3_a1,
            );
        }
    }

    /// `entries` is a slice of `(label, shortcut)` pairs — e.g. `("Split Vertical", "Ctrl+W s")`.
    pub fn draw_command_palette(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        query: &str,
        entries: &[(&str, &str)],
        selected: usize,
    ) {
        dim_buffer(buf);

        let fp = self.status_font_px;
        let cw = self.glyphs.rasterize('M', fp, false).1;
        let row_h = (fp * 1.6) as u32 + 4;

        const MAX_VISIBLE: usize = 10;
        let visible = entries.len().min(MAX_VISIBLE);
        let panel_h = row_h * (1 + visible as u32 + 1);
        let panel_w = (bw as f32 * 0.62) as u32;
        let px = (bw - panel_w) / 2;
        let py = bh / 4;

        let bg = 0xff_1a_1b_26_u32;
        let border = 0xff_89_b4_fa_u32;
        let pad = cw;

        fill_rect(buf, bw, px, py, panel_w, panel_h, bg);
        draw_rect_border(buf, bw, px, py, panel_w, panel_h, border);

        // Query input row
        let query_display = format!("> {query}_");
        self.draw_str(
            buf,
            bw,
            bh,
            px + pad,
            py + 4,
            &query_display,
            fp,
            false,
            0xff_cb_d5_f5,
        );

        // Entry count indicator
        let count_str = format!("{}/{}", entries.len(), crate::command_palette::total());
        let count_x = px + panel_w - cw * count_str.len() as u32 - pad;
        self.draw_str(
            buf,
            bw,
            bh,
            count_x,
            py + 4,
            &count_str,
            fp,
            false,
            0xff_58_5b_70,
        );

        // Separator line under query row
        let sep_y = py + row_h;
        fill_rect(buf, bw, px + 1, sep_y, panel_w - 2, 1, 0xff_24_25_3a);

        // Scroll window: keep selected visible
        let scroll_start = selected.saturating_sub(MAX_VISIBLE - 1);

        let content_y = py + row_h + 1;
        for (list_i, &(label, code)) in entries
            .iter()
            .enumerate()
            .skip(scroll_start)
            .take(MAX_VISIBLE)
        {
            let row_y = content_y + (list_i - scroll_start) as u32 * row_h;
            let is_sel = list_i == selected;

            let row_bg = if is_sel { 0xff_2a_2b_3d } else { bg };
            fill_rect(buf, bw, px + 1, row_y, panel_w - 2, row_h, row_bg);
            if is_sel {
                fill_rect(buf, bw, px + 1, row_y, 1, row_h, border);
            }

            // Label (left, bold when selected)
            let label_color = if is_sel { 0xff_f9_e2_af } else { 0xff_ba_c2_de };
            self.draw_str(
                buf,
                bw,
                bh,
                px + pad + 4,
                row_y + 2,
                label,
                fp,
                is_sel,
                label_color,
            );

            // Shortcut hint (right-aligned, dimmed)
            let shortcut = code; // parameter name reused — holds the shortcut string
            let shortcut_x = px + panel_w - cw * shortcut.len() as u32 - pad;
            self.draw_str(
                buf,
                bw,
                bh,
                shortcut_x,
                row_y + 2,
                shortcut,
                fp,
                false,
                0xff_58_5b_70,
            );
        }

        // Footer hint
        let footer_y = py + panel_h - row_h + 4;
        self.draw_str(
            buf,
            bw,
            bh,
            px + pad,
            footer_y,
            "↑↓ navigate   Enter execute   Esc close",
            fp,
            false,
            0xff_58_5b_70,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_screenshot_selector(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        cx: u32,
        cy: u32,
        half_w: u32,
        half_h: u32,
    ) {
        let left = cx.saturating_sub(half_w);
        let top = cy.saturating_sub(half_h);
        let right = (cx + half_w).min(bw);
        let bottom = (cy + half_h).min(bh);
        let sel_w = right.saturating_sub(left);
        let sel_h = bottom.saturating_sub(top);

        dim_outside_rect(buf, bw, bh, left, top, right, bottom);
        self.draw_selection_border(buf, bw, left, top, sel_w, sel_h);
        self.draw_selector_hint(buf, bw, bh, left, top, bottom, sel_w);
    }

    fn draw_selection_border(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        left: u32,
        top: u32,
        sel_w: u32,
        sel_h: u32,
    ) {
        if sel_w == 0 || sel_h == 0 {
            return;
        }
        draw_rect_border(buf, bw, left, top, sel_w, sel_h, 0xFF_FF_FF_FF);
        if sel_w > 2 && sel_h > 2 {
            draw_rect_border(
                buf,
                bw,
                left + 1,
                top + 1,
                sel_w - 2,
                sel_h - 2,
                0xFF_FF_FF_FF,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_selector_hint(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        left: u32,
        top: u32,
        bottom: u32,
        sel_w: u32,
    ) {
        let hint = "\u{2191}\u{2193}\u{2190}\u{2192} resize   Shift+\u{2191}\u{2193}\u{2190}\u{2192} move   Enter capture   Esc cancel";
        let fp = self.status_font_px;
        let cw = self.glyphs.rasterize('M', fp, false).1;
        let line_h = (fp * 1.6) as u32;
        let text_y = hint_text_y(top, bottom, line_h, bh);
        let text_w = hint.chars().count() as u32 * cw;
        let text_x = left
            .saturating_add(sel_w / 2)
            .saturating_sub(text_w / 2)
            .min(bw.saturating_sub(text_w));
        self.draw_str(buf, bw, bh, text_x, text_y, hint, fp, false, 0xFF_FF_FF_FF);
    }

    pub fn draw_quit_confirm(&mut self, buf: &mut [u32], bw: u32, bh: u32, theme: &ResolvedTheme) {
        dim_buffer(buf);
        let lines = ["Quit? All tabs will close.", "[y] Yes   [n / Esc] Cancel"];
        let fg = [color_u32(theme.foreground), color_u32(theme.palette[8])];
        self.draw_confirm_dialog(
            buf,
            bw,
            bh,
            &lines,
            &fg,
            color_u32(theme.background),
            color_u32(theme.palette[1]),
        );
    }

    pub fn draw_save_session_confirm(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        theme: &ResolvedTheme,
    ) {
        dim_buffer(buf);
        let lines = [
            "Save session before quitting?",
            "[s] Save and quit   [q] Quit   [Esc] Cancel",
        ];
        let fg = [color_u32(theme.foreground), color_u32(theme.palette[8])];
        self.draw_confirm_dialog(
            buf,
            bw,
            bh,
            &lines,
            &fg,
            color_u32(theme.background),
            color_u32(theme.palette[3]),
        );
    }

    /// Renders a string of glyphs blended onto a colored background.
    /// Used for mode and REC badges in the status bar.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_badge_label(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        mut x: u32,
        y: u32,
        badge_h: u32,
        char_w: u32,
        label: &str,
        badge_color: u32,
        fg: u32,
        fp: f32,
    ) {
        let baseline = (badge_h as f32 * 0.82) as u32;
        for c in label.chars() {
            let (bitmap, gw, gh) = self.glyphs.rasterize(c, fp, true);
            let cy = y + baseline.saturating_sub(gh);
            blit_glyph_badge(buf, bw, bh, x, cy, gw, gh, bitmap, badge_color, fg);
            x += char_w;
        }
    }

    /// Draws a centered dialog box with the given text lines over a dimmed background.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_confirm_dialog(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        lines: &[&str],
        fg_colors: &[u32],
        bg: u32,
        border: u32,
    ) {
        let fp = self.status_font_px;
        let cw = self.glyphs.rasterize('M', fp, false).1;
        let line_h = (fp * 1.8) as u32;
        let pad_x = cw * 3;
        let pad_y = line_h;
        let max_chars = lines.iter().map(|l| l.len() as u32).max().unwrap_or(1);
        let box_w = max_chars * cw + pad_x * 2;
        let box_h = lines.len() as u32 * line_h + pad_y * 2;
        let bx = bw.saturating_sub(box_w) / 2;
        let by = bh.saturating_sub(box_h) / 2;
        fill_rect(buf, bw, bx, by, box_w, box_h, bg);
        draw_rect_border(buf, bw, bx, by, box_w, box_h, border);
        for (i, line) in lines.iter().enumerate() {
            let ty = by + pad_y + i as u32 * line_h;
            let fg = fg_colors.get(i).copied().unwrap_or(0xFF_FF_FF_FF);
            self.draw_str(buf, bw, bh, bx + pad_x, ty, line, fp, false, fg);
        }
    }
}

#[cfg(test)]
#[path = "overlays_test.rs"]
mod tests;
