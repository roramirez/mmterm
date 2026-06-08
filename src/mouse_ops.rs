use arboard::Clipboard;

use crate::input::InputMode;
use crate::{geometry, mouse};

use super::App;

pub(super) fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", url])
            .spawn();
    }
}

impl App {
    pub(crate) fn active_mouse_mode(&self) -> (u16, bool) {
        let active = self.tab().active;
        self.tab()
            .panes
            .get(&active)
            .map(|e| (e.pane.parser.grid.mouse_mode, e.pane.parser.grid.mouse_sgr))
            .unwrap_or((0, false))
    }

    pub(crate) fn send_mouse_event(
        &mut self,
        btn: u8,
        col: usize,
        row: usize,
        release: bool,
        sgr: bool,
    ) {
        let active = self.tab().active;
        let data = mouse::encode_mouse_event(btn, col, row, release, sgr);
        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
            let _ = entry.pty.write_input(&data);
        }
    }

    pub(crate) fn pane_at_pixel(&self, px: f64, py: f64) -> Option<usize> {
        let (tab_h, status_h) = (self.tab_h(), self.status_h());
        let rects = self.tab().layout.rects_scaled(tab_h, status_h);
        geometry::pane_at_pixel(&rects, px, py)
    }

    pub(crate) fn pixel_to_cell(&self, pane_id: usize, px: f64, py: f64) -> Option<(usize, usize)> {
        let tab = self.tab();
        let entry = tab.panes.get(&pane_id)?;
        let m = &tab.metrics;
        geometry::pixel_to_cell(
            entry.pane.rect,
            m.cell_width,
            m.cell_height,
            entry.pane.parser.grid.cols,
            entry.pane.parser.grid.rows,
            px,
            py,
        )
    }

    pub(crate) fn url_at_pixel(&self, px: f64, py: f64) -> Option<String> {
        let pane_id = self.pane_at_pixel(px, py)?;
        let (col, row) = self.pixel_to_cell(pane_id, px, py)?;
        let tab = self.tab();
        let entry = tab.panes.get(&pane_id)?;
        let url = geometry::cell_url_at_scroll(
            &entry.pane.parser.grid,
            entry.pane.scroll_offset,
            col,
            row,
        )?;
        Some(url.as_ref().clone())
    }

    pub(crate) fn start_mouse_selection(&mut self, px: f64, py: f64) {
        if let Some(pane_id) = self.pane_at_pixel(px, py) {
            self.tab_mut().active = pane_id;
            if let Some((col, row)) = self.pixel_to_cell(pane_id, px, py) {
                self.state.mode = InputMode::Visual {
                    start_col: col,
                    start_row: row,
                    cur_col: col,
                    cur_row: row,
                    anchored: true,
                };
                self.state.mouse_selecting = true;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
    }

    pub(crate) fn update_mouse_selection(&mut self, px: f64, py: f64) {
        if let InputMode::Visual {
            start_col,
            start_row,
            ..
        } = self.state.mode.clone()
        {
            let active = self.tab().active;
            if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                self.state.mode = InputMode::Visual {
                    start_col,
                    start_row,
                    cur_col: col,
                    cur_row: row,
                    anchored: true,
                };
            }
        }
    }

    pub(crate) fn finish_mouse_selection(&mut self) {
        if let InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            cur_row,
            ..
        } = self.state.mode.clone()
        {
            self.state.mode = InputMode::Insert;
            if start_col == cur_col && start_row == cur_row {
                if let Some(url) = self.state.hovered_url.clone() {
                    open_url(&url);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
                return;
            }
            self.copy_selection_to_clipboard(start_col, start_row, cur_col, cur_row);
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    pub(crate) fn copy_selection_to_clipboard(
        &mut self,
        start_col: usize,
        start_row: usize,
        cur_col: usize,
        cur_row: usize,
    ) {
        let active = self.tab().active;
        if let Some(entry) = self.tab().panes.get(&active) {
            let scroll_offset = entry.pane.scroll_offset;
            let text = entry.pane.parser.grid.selected_text(
                start_col,
                start_row,
                cur_col,
                cur_row,
                scroll_offset,
            );
            if !text.is_empty() {
                let cb = self
                    .state
                    .clipboard
                    .get_or_insert_with(|| Clipboard::new().expect("clipboard unavailable"));
                match cb.set_text(text) {
                    Ok(()) => log::info!("Copied mouse selection to clipboard"),
                    Err(e) => log::warn!("Clipboard write failed: {e}"),
                }
            }
        }
    }
}
