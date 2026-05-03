const SCROLLBACK_MAX: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
    pub const WHITE: Self = Self::rgb(0xd8, 0xd8, 0xd8);
    pub const BLACK: Self = Self::rgb(0x1e, 0x1e, 0x2e);
    #[allow(dead_code)]
    pub const CURSOR: Self = Self::rgb(0xcb, 0xa6, 0xf7);
    #[allow(dead_code)]
    pub const SELECTION: Self = Self::rgb(0x45, 0x47, 0x5a);
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    /// True when this is the left half of a 2-column wide character.
    pub wide: bool,
    /// True when this is the right (placeholder) half of a wide character.
    pub wide_cont: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self { c: ' ', fg: Color::WHITE, bg: Color::BLACK, bold: false, wide: false, wide_cont: false }
    }
}

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Cell>,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub scroll_top: usize,
    pub scroll_bottom: usize,
    // SGR state
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    // Scrollback: lines that have scrolled off the top (oldest first)
    pub scrollback: Vec<Vec<Cell>>,
    // Theme colors
    pub default_fg: Color,
    pub default_bg: Color,
    pub cursor_color: Color,
    pub selection_color: Color,
    pub palette: [Color; 16],
    // DECCKM: when true, arrow keys send SS3 sequences (\eOA) instead of CSI (\e[A)
    pub application_cursor_keys: bool,
}

impl Grid {
    pub fn with_colors(
        cols: usize,
        rows: usize,
        default_fg: Color,
        default_bg: Color,
        cursor_color: Color,
        selection_color: Color,
        palette: [Color; 16],
    ) -> Self {
        let blank = Cell { c: ' ', fg: default_fg, bg: default_bg, bold: false, wide: false, wide_cont: false };
        Self {
            cols,
            rows,
            cells: vec![blank; cols * rows],
            cursor_col: 0,
            cursor_row: 0,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            fg: default_fg,
            bg: default_bg,
            bold: false,
            scrollback: Vec::new(),
            default_fg,
            default_bg,
            cursor_color,
            selection_color,
            palette,
            application_cursor_keys: false,
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let blank = self.blank_cell();
        let mut new_cells = vec![blank; cols * rows];
        let copy_rows = self.rows.min(rows);
        let copy_cols = self.cols.min(cols);
        for r in 0..copy_rows {
            for c in 0..copy_cols {
                new_cells[r * cols + c] = self.cells[r * self.cols + c].clone();
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.cells = new_cells;
        self.scroll_bottom = rows - 1;
        self.cursor_col = self.cursor_col.min(cols - 1);
        self.cursor_row = self.cursor_row.min(rows - 1);
    }

    pub fn cell_mut(&mut self, col: usize, row: usize) -> &mut Cell {
        &mut self.cells[row * self.cols + col]
    }

    pub fn cell(&self, col: usize, row: usize) -> &Cell {
        &self.cells[row * self.cols + col]
    }

    pub fn write_char(&mut self, c: char) {
        use unicode_width::UnicodeWidthChar;
        let char_cols = UnicodeWidthChar::width(c).unwrap_or(1).max(1);

        if self.cursor_col + char_cols > self.cols {
            self.cursor_col = 0;
            self.advance_row();
        }

        let fg = self.fg;
        let bg = self.bg;
        let bold = self.bold;
        let wide = char_cols == 2;

        let cell = self.cell_mut(self.cursor_col, self.cursor_row);
        cell.c = c;
        cell.fg = fg;
        cell.bg = bg;
        cell.bold = bold;
        cell.wide = wide;
        cell.wide_cont = false;
        self.cursor_col += 1;

        if wide && self.cursor_col < self.cols {
            let blank = self.blank_cell();
            let cont = self.cell_mut(self.cursor_col, self.cursor_row);
            *cont = Cell { wide_cont: true, ..blank };
            self.cursor_col += 1;
        }
    }

    pub fn advance_row(&mut self) {
        if self.cursor_row >= self.scroll_bottom {
            self.scroll_up(1);
        } else {
            self.cursor_row += 1;
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        let top = self.scroll_top;
        let bot = self.scroll_bottom;
        let cols = self.cols;
        let blank = self.blank_cell();
        for _ in 0..n {
            if top == 0 {
                let line: Vec<Cell> = (0..cols)
                    .map(|c| self.cells[top * cols + c].clone())
                    .collect();
                self.scrollback.push(line);
                if self.scrollback.len() > SCROLLBACK_MAX {
                    self.scrollback.remove(0);
                }
            }
            for r in top..bot {
                for c in 0..cols {
                    self.cells[r * cols + c] = self.cells[(r + 1) * cols + c].clone();
                }
            }
            for c in 0..cols {
                self.cells[bot * cols + c] = blank.clone();
            }
        }
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    pub fn selected_text(&self, sc: usize, sr: usize, ec: usize, er: usize) -> String {
        let (r0, c0, r1, c1) = if (sr, sc) <= (er, ec) {
            (sr, sc, er, ec)
        } else {
            (er, ec, sr, sc)
        };
        let mut result = String::new();
        for row in r0..=r1 {
            let col_start = if row == r0 { c0 } else { 0 };
            let col_end = if row == r1 { c1 } else { self.cols.saturating_sub(1) };
            let mut line = String::new();
            for col in col_start..=col_end {
                if col < self.cols && row < self.rows {
                    line.push(self.cell(col, row).c);
                }
            }
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(line.trim_end_matches(' '));
        }
        result
    }

    pub fn blank_cell(&self) -> Cell {
        Cell { c: ' ', fg: self.default_fg, bg: self.default_bg, bold: false, wide: false, wide_cont: false }
    }

    pub fn clear_line(&mut self, row: usize) {
        let cols = self.cols;
        let blank = self.blank_cell();
        for c in 0..cols {
            self.cells[row * cols + c] = blank.clone();
        }
    }

    pub fn clear_screen(&mut self) {
        let blank = self.blank_cell();
        self.cells = vec![blank; self.cols * self.rows];
    }
}
