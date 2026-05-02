use crate::terminal::TerminalParser;

pub struct Pane {
    pub parser: TerminalParser,
    pub rect: [u32; 4],
    /// Lines scrolled up from the bottom (0 = live view)
    pub scroll_offset: usize,
}

impl Pane {
    pub fn new(cols: usize, rows: usize, rect: [u32; 4]) -> Self {
        Self { parser: TerminalParser::new(cols, rows), rect, scroll_offset: 0 }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
        // Any new output snaps back to live view
        self.scroll_offset = 0;
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
