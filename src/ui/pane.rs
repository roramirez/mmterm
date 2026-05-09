use crate::terminal::TerminalParser;
use crate::terminal::grid::Color;

pub struct Pane {
    pub parser: TerminalParser,
    pub rect: [u32; 4],
    pub scroll_offset: usize,
}

impl Pane {
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_colors(
        cols: usize,
        rows: usize,
        rect: [u32; 4],
        fg: Color,
        bg: Color,
        cursor: Color,
        selection: Color,
        palette: [Color; 16],
    ) -> Self {
        Self {
            parser: TerminalParser::new_with_colors(cols, rows, fg, bg, cursor, selection, palette),
            rect,
            scroll_offset: 0,
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        let sb_before = self.parser.grid.scrollback_len();
        self.parser.process(bytes);
        let sb_after = self.parser.grid.scrollback_len();
        if self.scroll_offset > 0 {
            // Compensate for lines pushed into scrollback so the view stays
            // pinned to the same content. Clamp in case scrollback shrank
            // (e.g. alternate screen entered).
            let added = sb_after.saturating_sub(sb_before);
            self.scroll_offset = (self.scroll_offset + added).min(sb_after);
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize, rect: [u32; 4]) {
        self.parser.grid.resize(cols, rows);
        self.rect = rect;
    }

    pub fn scroll_up(&mut self, n: usize) {
        let max = self.parser.grid.scrollback_len();
        self.scroll_offset = (self.scroll_offset + n).min(max);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    pub fn scroll_top(&mut self) {
        self.scroll_offset = self.parser.grid.scrollback_len();
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll_offset = 0;
    }
}

#[cfg(test)]
#[path = "pane_test.rs"]
mod tests;
