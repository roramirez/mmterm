use crate::theme::ResolvedTheme;
use crate::tui_config::ConfigPanel;

use super::text::{
    Renderer, STATUS_BAR_H, blend, color_u32, dim_buffer, draw_rect_border, fill_rect,
};

// ── Overlay palette ──────────────────────────────────────────────────────────
// All color constants used by config panel and command palette overlays.

/// Panel background (dark blue).
const C_PANEL_BG: u32 = 0xff_1a_1b_26;
/// Panel border / selected-row left accent (bright blue).
const C_BORDER: u32 = 0xff_89_b4_fa;
/// Section separator / thin rule between sections.
const C_SECTION_RULE: u32 = 0xff_24_25_3a;
/// Footer separator line.
const C_FOOTER_SEP: u32 = 0xff_31_32_44;
/// Dimmed text: section headers, scroll indicator, hints, help.
const C_DIM: u32 = 0xff_58_5b_70;
/// Panel title (magenta).
const C_TITLE: u32 = 0xff_cb_a6_f7;
/// Selected row background.
const C_ROW_SEL_BG: u32 = 0xff_2a_2b_3d;
/// Label color when row is selected (yellow).
const C_LABEL_SEL: u32 = 0xff_f9_e2_af;
/// Label color when row is unselected (light blue-grey).
const C_LABEL_UNSEL: u32 = 0xff_ba_c2_de;
/// Error / invalid status text (pink).
const C_ERROR: u32 = 0xff_f3_8b_a8;
/// "[editing]" badge (green).
const C_EDITING: u32 = 0xff_a6_e3_a1;
/// Command palette query input text.
const C_QUERY_TEXT: u32 = 0xff_cb_d5_f5;

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

fn collapse_indicator(is_collapsed: bool) -> &'static str {
    if is_collapsed { "[+]" } else { "[-]" }
}

fn config_panel_hint(panel: &ConfigPanel) -> String {
    match panel.fields[panel.selected].section {
        Some(sec) => {
            if panel.collapsed.contains(sec) {
                "Space: expand section  ]: next section  [: prev section".to_string()
            } else {
                "Space: collapse section  ]: next section  [: prev section".to_string()
            }
        }
        None => format!("hint: {}", panel.fields[panel.selected].hint),
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
    fn panel_font_metrics(&mut self) -> (f32, u32, u32) {
        let fp = self.status_font_px;
        let cw = self.glyphs.rasterize('M', fp, false).1;
        let row_h = (fp * 1.6) as u32 + 4;
        (fp, cw, row_h)
    }

    /// Draws the section separator rule and label/indicator row.
    /// Returns `Some(new_draw_y)` to continue the field loop, or `None` if
    /// the panel viewport is exhausted and the caller should break.
    #[allow(clippy::too_many_arguments)]
    fn draw_config_section_header(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        panel: &ConfigPanel,
        sec: &'static str,
        i: usize,
        draw_y: u32,
        clip_y: u32,
        l: &FieldRowLayout,
    ) -> Option<u32> {
        let section_h = l.row_h - 2;
        fill_rect(buf, bw, l.px + 1, draw_y, l.panel_w - 2, 1, C_SECTION_RULE);
        let is_collapsed = panel.collapsed.contains(sec);
        let count = panel.collapsed_count(sec);
        let sec_label = if is_collapsed {
            format!("── {} ({}) ", sec, count)
        } else {
            format!("── {} ", sec)
        };
        self.draw_str(
            buf,
            bw,
            bh,
            l.px + l.pad,
            draw_y + 1,
            &sec_label,
            l.fp,
            true,
            C_DIM,
        );
        let indicator = collapse_indicator(is_collapsed);
        let ind_color = if i == l.sel { C_LABEL_SEL } else { C_DIM };
        let ind_x = l.px + l.panel_w - l.cw * indicator.len() as u32 - l.pad;
        self.draw_str(
            buf,
            bw,
            bh,
            ind_x,
            draw_y + 1,
            indicator,
            l.fp,
            false,
            ind_color,
        );
        let new_y = draw_y + section_h;
        if new_y + l.row_h > clip_y {
            None
        } else {
            Some(new_y)
        }
    }

    pub fn draw_config_panel(&mut self, buf: &mut [u32], bw: u32, bh: u32, panel: &ConfigPanel) {
        dim_buffer(buf);

        let (fp, cw, row_h) = self.panel_font_metrics();
        let pad = cw;

        let panel_w = (bw as f32 * 0.65) as u32;
        // Fixed panel height: title + footer + visible rows (fit inside window)
        let footer_rows = 2u32; // hint + status
        let max_visible =
            ((bh.saturating_sub(STATUS_BAR_H + row_h * 2 + row_h * footer_rows)) / row_h).max(4);
        let panel_h = row_h * (max_visible + 2 + footer_rows);
        let px = (bw - panel_w) / 2;
        let py = (bh.saturating_sub(panel_h)) / 2;

        fill_rect(buf, bw, px, py, panel_w, panel_h, C_PANEL_BG);
        draw_rect_border(buf, bw, px, py, panel_w, panel_h, C_BORDER);

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
            C_TITLE,
        );

        // Scroll window: keep selected in view (using visible indices)
        let vis = panel.visible_indices();
        let sel = panel.selected;
        let sel_pos = vis.iter().position(|&i| i == sel).unwrap_or(0);
        let scroll_start = sel_pos.saturating_sub(max_visible as usize - 1);

        // Scroll indicator shows position within visible set
        let scroll_info = format!("{}/{}", sel_pos + 1, vis.len());
        let si_x = px + panel_w - cw * scroll_info.len() as u32 - pad;
        self.draw_str(buf, bw, bh, si_x, py + 4, &scroll_info, fp, false, C_DIM);

        let layout = FieldRowLayout {
            px,
            panel_w,
            pad,
            cw,
            fp,
            row_h,
            bg: C_PANEL_BG,
            border: C_BORDER,
            sel,
        };
        let clip_y = py + panel_h - row_h * footer_rows;

        let content_y = py + row_h * 2;
        let mut draw_y = content_y;

        for &i in vis.iter().skip(scroll_start) {
            if draw_y + row_h > clip_y {
                break;
            }
            match self.draw_config_panel_row(buf, bw, bh, panel, i, draw_y, clip_y, &layout) {
                Some(new_y) => draw_y = new_y,
                None => break,
            }
        }

        self.draw_config_panel_footer(
            buf,
            bw,
            bh,
            panel,
            px,
            panel_w,
            pad,
            fp,
            row_h,
            footer_rows,
            panel_h,
            py,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_config_panel_footer(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        panel: &ConfigPanel,
        px: u32,
        panel_w: u32,
        pad: u32,
        fp: f32,
        row_h: u32,
        footer_rows: u32,
        panel_h: u32,
        py: u32,
    ) {
        let footer_y = py + panel_h - row_h * footer_rows;
        fill_rect(buf, bw, px + 1, footer_y, panel_w - 2, 1, C_FOOTER_SEP);
        let hint = config_panel_hint(panel);
        self.draw_str(buf, bw, bh, px + pad, footer_y + 2, &hint, fp, false, C_DIM);
        let status_y = py + panel_h - row_h;
        let status = panel.status.as_deref().unwrap_or(
            "j/k: move  Space: collapse  ]/[: section  Enter/i: edit  Ctrl+S: save  q: cancel",
        );
        let status_color = if panel.status.is_some() {
            C_ERROR
        } else {
            C_DIM
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
    #[allow(clippy::too_many_arguments)]
    fn draw_config_panel_row(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        panel: &ConfigPanel,
        i: usize,
        draw_y: u32,
        clip_y: u32,
        l: &FieldRowLayout,
    ) -> Option<u32> {
        if let Some(sec) = panel.fields[i].section {
            let new_y =
                self.draw_config_section_header(buf, bw, bh, panel, sec, i, draw_y, clip_y, l)?;
            self.draw_config_field_row(buf, bw, bh, panel, i, new_y, l);
            Some(new_y + l.row_h)
        } else {
            self.draw_config_field_row(buf, bw, bh, panel, i, draw_y, l);
            Some(draw_y + l.row_h)
        }
    }

    fn draw_editing_badge(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        l: &FieldRowLayout,
        y: u32,
    ) {
        let ex = l.px + l.panel_w - l.cw * 7 - l.pad;
        self.draw_str(buf, bw, bh, ex, y + 2, "[editing]", l.fp, false, C_EDITING);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_section_collapse_badge(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        sec: &'static str,
        panel: &ConfigPanel,
        l: &FieldRowLayout,
        y: u32,
    ) {
        let ind = collapse_indicator(panel.collapsed.contains(sec));
        let bx = l.px + l.panel_w - l.cw * ind.len() as u32 - l.pad;
        self.draw_str(buf, bw, bh, bx, y + 2, ind, l.fp, false, C_LABEL_SEL);
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

        let row_bg = if is_sel { C_ROW_SEL_BG } else { l.bg };
        fill_rect(buf, bw, l.px + 1, draw_y, l.panel_w - 2, l.row_h, row_bg);
        if is_sel {
            fill_rect(buf, bw, l.px + 1, draw_y, 1, l.row_h, l.border);
        }

        draw_hex_color_swatch(buf, bw, panel, i, draw_y, l);

        let label_color = if is_sel { C_LABEL_SEL } else { C_LABEL_UNSEL };
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
            self.draw_editing_badge(buf, bw, bh, l, draw_y);
        }

        // For section-header fields show [+]/[-] badge on the row itself so the
        // collapse affordance is visible on the highlighted row, not just above it.
        if let Some(sec) = field.section.filter(|_| is_sel) {
            self.draw_section_collapse_badge(buf, bw, bh, sec, panel, l, draw_y);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_palette_entry(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        px: u32,
        panel_w: u32,
        pad: u32,
        cw: u32,
        fp: f32,
        row_h: u32,
        row_y: u32,
        label: &str,
        shortcut: &str,
        is_sel: bool,
    ) {
        let row_bg = if is_sel { C_ROW_SEL_BG } else { C_PANEL_BG };
        fill_rect(buf, bw, px + 1, row_y, panel_w - 2, row_h, row_bg);
        if is_sel {
            fill_rect(buf, bw, px + 1, row_y, 1, row_h, C_BORDER);
        }
        let label_color = if is_sel { C_LABEL_SEL } else { C_LABEL_UNSEL };
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
            C_DIM,
        );
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

        let (fp, cw, row_h) = self.panel_font_metrics();

        const MAX_VISIBLE: usize = 10;
        let visible = entries.len().min(MAX_VISIBLE);
        let panel_h = row_h * (1 + visible as u32 + 1);
        let panel_w = (bw as f32 * 0.62) as u32;
        let px = (bw - panel_w) / 2;
        let py = bh / 4;

        let pad = cw;

        fill_rect(buf, bw, px, py, panel_w, panel_h, C_PANEL_BG);
        draw_rect_border(buf, bw, px, py, panel_w, panel_h, C_BORDER);

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
            C_QUERY_TEXT,
        );

        // Entry count indicator
        let count_str = format!("{}/{}", entries.len(), crate::command_palette::total());
        let count_x = px + panel_w - cw * count_str.len() as u32 - pad;
        self.draw_str(buf, bw, bh, count_x, py + 4, &count_str, fp, false, C_DIM);

        // Separator line under query row
        let sep_y = py + row_h;
        fill_rect(buf, bw, px + 1, sep_y, panel_w - 2, 1, C_SECTION_RULE);

        // Scroll window: keep selected visible
        let scroll_start = selected.saturating_sub(MAX_VISIBLE - 1);

        let content_y = py + row_h + 1;
        for (list_i, &(label, shortcut)) in entries
            .iter()
            .enumerate()
            .skip(scroll_start)
            .take(MAX_VISIBLE)
        {
            let row_y = content_y + (list_i - scroll_start) as u32 * row_h;
            self.draw_palette_entry(
                buf,
                bw,
                bh,
                px,
                panel_w,
                pad,
                cw,
                fp,
                row_h,
                row_y,
                label,
                shortcut,
                list_i == selected,
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
            C_DIM,
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

    #[allow(clippy::too_many_arguments)]
    pub fn draw_screenshot_name_input(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        cx: u32,
        cy: u32,
        half_w: u32,
        half_h: u32,
        name: &str,
    ) {
        let left = cx.saturating_sub(half_w);
        let top = cy.saturating_sub(half_h);
        let right = (cx + half_w).min(bw);
        let bottom = (cy + half_h).min(bh);
        let sel_w = right.saturating_sub(left);
        let sel_h = bottom.saturating_sub(top);

        dim_outside_rect(buf, bw, bh, left, top, right, bottom);
        self.draw_selection_border(buf, bw, left, top, sel_w, sel_h);

        let (fp, cw, row_h) = self.panel_font_metrics();
        let pad = cw * 2;

        let label = "Name: ";
        let display = format!("{}{}_", label, name);
        let box_w = (display.chars().count() as u32).max(30) * cw + pad * 2;
        let box_h = row_h + pad;
        let bx = bw.saturating_sub(box_w) / 2;
        let by = bh.saturating_sub(box_h + 8);

        fill_rect(buf, bw, bx, by, box_w, box_h, C_PANEL_BG);
        draw_rect_border(buf, bw, bx, by, box_w, box_h, C_BORDER);
        self.draw_str(
            buf,
            bw,
            bh,
            bx + pad,
            by + pad / 2,
            &display,
            fp,
            false,
            C_QUERY_TEXT,
        );

        let hint = "Enter save  (empty = mmterm-<timestamp>.png)   Esc cancel";
        let hint_w = hint.chars().count() as u32 * cw;
        let hint_x = bw.saturating_sub(hint_w) / 2;
        let hint_y = by.saturating_sub(row_h + 2);
        self.draw_str(buf, bw, bh, hint_x, hint_y, hint, fp, false, C_DIM);
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
