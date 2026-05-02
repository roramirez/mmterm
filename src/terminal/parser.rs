use super::grid::{Color, Grid};
use vte::{Params, Parser, Perform};

pub struct TerminalParser {
    pub grid: Grid,
    parser: Parser,
}

impl TerminalParser {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            grid: Grid::new(cols, rows),
            parser: Parser::new(),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        // vte requires a mutable performer; we route through a wrapper
        let mut performer = Performer { grid: &mut self.grid };
        for &byte in bytes {
            self.parser.advance(&mut performer, byte);
        }
    }
}

struct Performer<'a> {
    grid: &'a mut Grid,
}

impl Perform for Performer<'_> {
    fn print(&mut self, c: char) {
        self.grid.write_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.grid.cursor_col = 0,
            b'\n' => self.grid.advance_row(),
            0x08 => {
                // backspace
                if self.grid.cursor_col > 0 {
                    self.grid.cursor_col -= 1;
                }
            }
            0x07 => {} // bell — ignore
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: char) {
        let ps: Vec<u16> = params.iter().map(|p| p[0]).collect();
        let p0 = ps.first().copied().unwrap_or(0);
        let p1 = ps.get(1).copied().unwrap_or(0);

        match action {
            // Cursor movement
            'A' => {
                let n = p0.max(1) as usize;
                self.grid.cursor_row = self.grid.cursor_row.saturating_sub(n);
            }
            'B' => {
                let n = p0.max(1) as usize;
                self.grid.cursor_row = (self.grid.cursor_row + n).min(self.grid.rows - 1);
            }
            'C' => {
                let n = p0.max(1) as usize;
                self.grid.cursor_col = (self.grid.cursor_col + n).min(self.grid.cols - 1);
            }
            'D' => {
                let n = p0.max(1) as usize;
                self.grid.cursor_col = self.grid.cursor_col.saturating_sub(n);
            }
            // Cursor position (row;col, 1-indexed)
            'H' | 'f' => {
                let row = (p0.saturating_sub(1)) as usize;
                let col = (p1.saturating_sub(1)) as usize;
                self.grid.cursor_row = row.min(self.grid.rows - 1);
                self.grid.cursor_col = col.min(self.grid.cols - 1);
            }
            // Erase in display
            'J' => match p0 {
                0 => {
                    let row = self.grid.cursor_row;
                    let col = self.grid.cursor_col;
                    let cols = self.grid.cols;
                    let rows = self.grid.rows;
                    for c in col..cols {
                        self.grid.cells[row * cols + c] = Default::default();
                    }
                    for r in (row + 1)..rows {
                        self.grid.clear_line(r);
                    }
                }
                1 => {
                    let row = self.grid.cursor_row;
                    let col = self.grid.cursor_col;
                    let cols = self.grid.cols;
                    for c in 0..=col {
                        self.grid.cells[row * cols + c] = Default::default();
                    }
                    for r in 0..row {
                        self.grid.clear_line(r);
                    }
                }
                2 | 3 => self.grid.clear_screen(),
                _ => {}
            },
            // Erase in line
            'K' => {
                let row = self.grid.cursor_row;
                let col = self.grid.cursor_col;
                let cols = self.grid.cols;
                match p0 {
                    0 => {
                        for c in col..cols {
                            self.grid.cells[row * cols + c] = Default::default();
                        }
                    }
                    1 => {
                        for c in 0..=col {
                            self.grid.cells[row * cols + c] = Default::default();
                        }
                    }
                    2 => self.grid.clear_line(row),
                    _ => {}
                }
            }
            // SGR — Select Graphic Rendition
            'm' => {
                if ps.is_empty() || (ps.len() == 1 && ps[0] == 0) {
                    self.grid.fg = Color::WHITE;
                    self.grid.bg = Color::BLACK;
                    self.grid.bold = false;
                    return;
                }
                let mut i = 0;
                while i < ps.len() {
                    match ps[i] {
                        0 => {
                            self.grid.fg = Color::WHITE;
                            self.grid.bg = Color::BLACK;
                            self.grid.bold = false;
                        }
                        1 => self.grid.bold = true,
                        22 => self.grid.bold = false,
                        // Standard foreground colors 30-37
                        n @ 30..=37 => self.grid.fg = ansi_color(n - 30, self.grid.bold),
                        39 => self.grid.fg = Color::WHITE,
                        // Standard background colors 40-47
                        n @ 40..=47 => self.grid.bg = ansi_color(n - 40, false),
                        49 => self.grid.bg = Color::BLACK,
                        // Bright foreground 90-97
                        n @ 90..=97 => self.grid.fg = ansi_color(n - 90, true),
                        // Bright background 100-107
                        n @ 100..=107 => self.grid.bg = ansi_color(n - 100, true),
                        // 256-color and truecolor
                        38 => {
                            if i + 1 < ps.len() {
                                match ps[i + 1] {
                                    5 if i + 2 < ps.len() => {
                                        self.grid.fg = color256(ps[i + 2] as u8);
                                        i += 2;
                                    }
                                    2 if i + 4 < ps.len() => {
                                        self.grid.fg = Color::rgb(
                                            ps[i + 2] as u8,
                                            ps[i + 3] as u8,
                                            ps[i + 4] as u8,
                                        );
                                        i += 4;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        48 => {
                            if i + 1 < ps.len() {
                                match ps[i + 1] {
                                    5 if i + 2 < ps.len() => {
                                        self.grid.bg = color256(ps[i + 2] as u8);
                                        i += 2;
                                    }
                                    2 if i + 4 < ps.len() => {
                                        self.grid.bg = Color::rgb(
                                            ps[i + 2] as u8,
                                            ps[i + 3] as u8,
                                            ps[i + 4] as u8,
                                        );
                                        i += 4;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            // Scroll up
            'S' => self.grid.scroll_up(p0.max(1) as usize),
            // Set scroll region
            'r' => {
                let top = p0.saturating_sub(1) as usize;
                let bot = if p1 == 0 {
                    self.grid.rows - 1
                } else {
                    (p1 - 1) as usize
                };
                self.grid.scroll_top = top.min(self.grid.rows - 1);
                self.grid.scroll_bottom = bot.min(self.grid.rows - 1);
                self.grid.cursor_row = self.grid.scroll_top;
                self.grid.cursor_col = 0;
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'M' => {
                // Reverse index
                if self.grid.cursor_row > self.grid.scroll_top {
                    self.grid.cursor_row -= 1;
                }
            }
            _ => {}
        }
    }
}

fn ansi_color(index: u16, bright: bool) -> Color {
    match (index, bright) {
        (0, false) => Color::rgb(0x1e, 0x1e, 0x2e),
        (0, true) => Color::rgb(0x58, 0x5b, 0x70),
        (1, false) => Color::rgb(0xf3, 0x8b, 0xa8),
        (1, true) => Color::rgb(0xf3, 0x8b, 0xa8),
        (2, false) => Color::rgb(0xa6, 0xe3, 0xa1),
        (2, true) => Color::rgb(0xa6, 0xe3, 0xa1),
        (3, false) => Color::rgb(0xf9, 0xe2, 0xaf),
        (3, true) => Color::rgb(0xf9, 0xe2, 0xaf),
        (4, false) => Color::rgb(0x89, 0xb4, 0xfa),
        (4, true) => Color::rgb(0x89, 0xb4, 0xfa),
        (5, false) => Color::rgb(0xcb, 0xa6, 0xf7),
        (5, true) => Color::rgb(0xcb, 0xa6, 0xf7),
        (6, false) => Color::rgb(0x89, 0xdc, 0xeb),
        (6, true) => Color::rgb(0x89, 0xdc, 0xeb),
        (7, false) => Color::rgb(0xba, 0xc2, 0xde),
        (7, true) => Color::rgb(0xd8, 0xd8, 0xd8),
        _ => Color::WHITE,
    }
}

fn color256(n: u8) -> Color {
    if n < 16 {
        return ansi_color(n as u16 % 8, n >= 8);
    }
    if n >= 232 {
        let v = 8 + (n - 232) * 10;
        return Color::rgb(v, v, v);
    }
    let idx = n - 16;
    let b = idx % 6;
    let g = (idx / 6) % 6;
    let r = idx / 36;
    let f = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
    Color::rgb(f(r), f(g), f(b))
}
