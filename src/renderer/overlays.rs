use crate::config::tui_config::ConfigPanel;
use crate::theme::ResolvedTheme;
use crate::ui::layout::STATUS_BAR_H;

use super::text::{Renderer, blend, color_u32, dim_buffer, draw_rect_border, fill_rect};

// ── Overlay palette ──────────────────────────────────────────────────────────
// Colors used by the config panel and command palette overlays, all resolved
// from the active `ResolvedTheme` so the overlays track the current theme.
// Only the two panel backgrounds are dedicated theme fields; every other color
// maps to an existing chrome field or palette slot (see `OverlayColors`).

#[derive(Clone, Copy)]
struct OverlayColors {
    /// Panel background.
    panel_bg: u32,
    /// Panel border / selected-row left accent.
    border: u32,
    /// Thin rule between sections / under the palette query row.
    section_rule: u32,
    /// Footer separator line.
    footer_sep: u32,
    /// Dimmed text: section headers, scroll indicator, hints, help.
    dim: u32,
    /// Panel title.
    title: u32,
    /// Selected row background.
    row_sel_bg: u32,
    /// Label color when a row is selected.
    label_sel: u32,
    /// Label color when a row is unselected.
    label_unsel: u32,
    /// Error / invalid status text.
    error: u32,
    /// "[editing]" badge.
    editing: u32,
    /// Command palette query input text.
    query_text: u32,
}

impl OverlayColors {
    fn from_theme(t: &ResolvedTheme) -> Self {
        Self {
            panel_bg: color_u32(t.overlay_bg),
            border: color_u32(t.badge),
            section_rule: color_u32(t.separator),
            footer_sep: color_u32(t.separator),
            dim: color_u32(t.palette[8]),
            title: color_u32(t.palette[5]),
            row_sel_bg: color_u32(t.overlay_bg_sel),
            label_sel: color_u32(t.search_match),
            label_unsel: color_u32(t.foreground),
            error: color_u32(t.palette[1]),
            editing: color_u32(t.palette[2]),
            query_text: color_u32(t.foreground),
        }
    }
}

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
    c: OverlayColors,
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
    if !matches!(
        panel.fields[i].kind,
        crate::config::tui_config::FieldKind::HexColor
    ) {
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
        let fp = self.status_font_px();
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
        fill_rect(
            buf,
            bw,
            l.px + 1,
            draw_y,
            l.panel_w - 2,
            1,
            l.c.section_rule,
        );
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
            l.c.dim,
        );
        let indicator = collapse_indicator(is_collapsed);
        let ind_color = if i == l.sel { l.c.label_sel } else { l.c.dim };
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

    pub fn draw_config_panel(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        panel: &ConfigPanel,
        theme: &ResolvedTheme,
    ) {
        dim_buffer(buf);
        let c = OverlayColors::from_theme(theme);

        let (fp, cw, row_h) = self.panel_font_metrics();
        let pad = cw;

        let panel_w = (bw as f32 * 0.65) as u32;
        // Fixed panel height: title + footer + visible rows (fit inside window)
        let footer_rows = 2u32; // hint + status
        let max_visible = ((bh
            .saturating_sub(self.scale.chrome(STATUS_BAR_H) + row_h * 2 + row_h * footer_rows))
            / row_h)
            .max(4);
        let panel_h = row_h * (max_visible + 2 + footer_rows);
        let px = (bw - panel_w) / 2;
        let py = (bh.saturating_sub(panel_h)) / 2;

        fill_rect(buf, bw, px, py, panel_w, panel_h, c.panel_bg);
        draw_rect_border(buf, bw, px, py, panel_w, panel_h, c.border);

        // Title bar: "CONFIGURATION" in title color, version dimmed right after
        let title = "CONFIGURATION";
        self.draw_str(buf, bw, bh, px + pad, py + 4, title, fp, true, c.title);
        let ver = format!("  v{}", panel.version);
        let ver_x = px + pad + cw * title.len() as u32;
        self.draw_str(buf, bw, bh, ver_x, py + 4, &ver, fp, false, c.dim);

        // Scroll window: keep selected in view (using visible indices)
        let vis = panel.visible_indices();
        let sel = panel.selected;
        let sel_pos = vis.iter().position(|&i| i == sel).unwrap_or(0);
        let scroll_start = sel_pos.saturating_sub(max_visible as usize - 1);

        // Scroll indicator shows position within visible set
        let scroll_info = format!("{}/{}", sel_pos + 1, vis.len());
        let si_x = px + panel_w - cw * scroll_info.len() as u32 - pad;
        self.draw_str(buf, bw, bh, si_x, py + 4, &scroll_info, fp, false, c.dim);

        let layout = FieldRowLayout {
            px,
            panel_w,
            pad,
            cw,
            fp,
            row_h,
            c,
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
            &c,
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
        c: &OverlayColors,
    ) {
        let footer_y = py + panel_h - row_h * footer_rows;
        fill_rect(buf, bw, px + 1, footer_y, panel_w - 2, 1, c.footer_sep);
        let hint = config_panel_hint(panel);
        self.draw_str(buf, bw, bh, px + pad, footer_y + 2, &hint, fp, false, c.dim);
        let status_y = py + panel_h - row_h;
        let status = panel.status.as_deref().unwrap_or(
            "j/k: move  Space: collapse  ]/[: section  Enter/i: edit  Ctrl+S: save  q: cancel",
        );
        let status_color = if panel.status.is_some() {
            c.error
        } else {
            c.dim
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
        self.draw_str(
            buf,
            bw,
            bh,
            ex,
            y + 2,
            "[editing]",
            l.fp,
            false,
            l.c.editing,
        );
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
        self.draw_str(buf, bw, bh, bx, y + 2, ind, l.fp, false, l.c.label_sel);
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

        let row_bg = if is_sel { l.c.row_sel_bg } else { l.c.panel_bg };
        fill_rect(buf, bw, l.px + 1, draw_y, l.panel_w - 2, l.row_h, row_bg);
        if is_sel {
            fill_rect(buf, bw, l.px + 1, draw_y, 1, l.row_h, l.c.border);
        }

        draw_hex_color_swatch(buf, bw, panel, i, draw_y, l);

        let label_color = if is_sel {
            l.c.label_sel
        } else {
            l.c.label_unsel
        };
        let is_select = matches!(field.kind, crate::config::tui_config::FieldKind::Select(_));
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
        c: &OverlayColors,
    ) {
        let row_bg = if is_sel { c.row_sel_bg } else { c.panel_bg };
        fill_rect(buf, bw, px + 1, row_y, panel_w - 2, row_h, row_bg);
        if is_sel {
            fill_rect(buf, bw, px + 1, row_y, 1, row_h, c.border);
        }
        let label_color = if is_sel { c.label_sel } else { c.label_unsel };
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
            c.dim,
        );
    }

    /// `entries` is a slice of `(label, shortcut)` pairs — e.g. `("Split Vertical", "Ctrl+W s")`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_command_palette(
        &mut self,
        buf: &mut [u32],
        bw: u32,
        bh: u32,
        query: &str,
        entries: &[(&str, &str)],
        selected: usize,
        theme: &ResolvedTheme,
    ) {
        dim_buffer(buf);
        let c = OverlayColors::from_theme(theme);

        let (fp, cw, row_h) = self.panel_font_metrics();

        const MAX_VISIBLE: usize = 10;
        let visible = entries.len().min(MAX_VISIBLE);
        let panel_h = row_h * (1 + visible as u32 + 1);
        let panel_w = (bw as f32 * 0.62) as u32;
        let px = (bw - panel_w) / 2;
        let py = bh / 4;

        let pad = cw;

        fill_rect(buf, bw, px, py, panel_w, panel_h, c.panel_bg);
        draw_rect_border(buf, bw, px, py, panel_w, panel_h, c.border);

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
            c.query_text,
        );

        // Entry count indicator
        let count_str = format!("{}/{}", entries.len(), crate::ui::command_palette::total());
        let count_x = px + panel_w - cw * count_str.len() as u32 - pad;
        self.draw_str(buf, bw, bh, count_x, py + 4, &count_str, fp, false, c.dim);

        // Separator line under query row
        let sep_y = py + row_h;
        fill_rect(buf, bw, px + 1, sep_y, panel_w - 2, 1, c.section_rule);

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
                &c,
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
            c.dim,
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
        let fp = self.status_font_px();
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
        theme: &ResolvedTheme,
    ) {
        let c = OverlayColors::from_theme(theme);
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

        fill_rect(buf, bw, bx, by, box_w, box_h, c.panel_bg);
        draw_rect_border(buf, bw, bx, by, box_w, box_h, c.border);
        self.draw_str(
            buf,
            bw,
            bh,
            bx + pad,
            by + pad / 2,
            &display,
            fp,
            false,
            c.query_text,
        );

        let hint = "Enter save  (empty = mmterm-<timestamp>.png)   Esc cancel";
        let hint_w = hint.chars().count() as u32 * cw;
        let hint_x = bw.saturating_sub(hint_w) / 2;
        let hint_y = by.saturating_sub(row_h + 2);
        self.draw_str(buf, bw, bh, hint_x, hint_y, hint, fp, false, c.dim);
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
        let fp = self.status_font_px();
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
