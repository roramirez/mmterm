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
    pub const CURSOR: Self = Self::rgb(0xcb, 0xa6, 0xf7);
    pub const SELECTION: Self = Self::rgb(0x45, 0x47, 0x5a);
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self { c: ' ', fg: Color::WHITE, bg: Color::BLACK, bold: false }
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
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![Cell::default(); cols * rows],
            cursor_col: 0,
            cursor_row: 0,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            fg: Color::WHITE,
            bg: Color::BLACK,
            bold: false,
            scrollback: Vec::new(),
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let mut new_cells = vec![Cell::default(); cols * rows];
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
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.advance_row();
        }
        let fg = self.fg;
        let bg = self.bg;
        let bold = self.bold;
        let cell = self.cell_mut(self.cursor_col, self.cursor_row);
        cell.c = c;
        cell.fg = fg;
        cell.bg = bg;
        cell.bold = bold;
        self.cursor_col += 1;
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
        for _ in 0..n {
            // Only push to scrollback when the full screen scrolls (scroll_top == 0)
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
                self.cells[bot * cols + c] = Cell::default();
            }
        }
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    pub fn clear_line(&mut self, row: usize) {
        let cols = self.cols;
        for c in 0..cols {
            self.cells[row * cols + c] = Cell::default();
        }
    }

    pub fn clear_screen(&mut self) {
        self.cells = vec![Cell::default(); self.cols * self.rows];
    }
}
