use super::grid::{Color, Grid};
use vte::{Params, Parser, Perform};

pub struct TerminalParser {
    pub grid: Grid,
    parser: Parser,
}

impl TerminalParser {
    pub fn new_with_colors(
        cols: usize, rows: usize,
        fg: Color, bg: Color, cursor: Color, selection: Color,
        palette: [Color; 16],
    ) -> Self {
        Self {
            grid: Grid::with_colors(cols, rows, fg, bg, cursor, selection, palette),
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

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let ps: Vec<u16> = params.iter().map(|p| p[0]).collect();
        let p0 = ps.first().copied().unwrap_or(0);
        let p1 = ps.get(1).copied().unwrap_or(0);

        // DEC private modes: \e[?<n>h (set) / \e[?<n>l (reset)
        if intermediates == b"?" {
            match (action, p0) {
                ('h', 1) => { self.grid.application_cursor_keys = true; return; }
                ('l', 1) => { self.grid.application_cursor_keys = false; return; }
                _ => return,
            }
        }

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
                    let blank = self.grid.blank_cell();
                    for c in col..cols {
                        self.grid.cells[row * cols + c] = blank.clone();
                    }
                    for r in (row + 1)..rows {
                        self.grid.clear_line(r);
                    }
                }
                1 => {
                    let row = self.grid.cursor_row;
                    let col = self.grid.cursor_col;
                    let cols = self.grid.cols;
                    let blank = self.grid.blank_cell();
                    for c in 0..=col {
                        self.grid.cells[row * cols + c] = blank.clone();
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
                let blank = self.grid.blank_cell();
                match p0 {
                    0 => {
                        for c in col..cols {
                            self.grid.cells[row * cols + c] = blank.clone();
                        }
                    }
                    1 => {
                        for c in 0..=col {
                            self.grid.cells[row * cols + c] = blank.clone();
                        }
                    }
                    2 => self.grid.clear_line(row),
                    _ => {}
                }
            }
            // SGR — Select Graphic Rendition
            'm' => {
                if ps.is_empty() || (ps.len() == 1 && ps[0] == 0) {
                    self.grid.fg = self.grid.default_fg;
                    self.grid.bg = self.grid.default_bg;
                    self.grid.bold = false;
                    return;
                }
                let mut i = 0;
                while i < ps.len() {
                    match ps[i] {
                        0 => {
                            self.grid.fg = self.grid.default_fg;
                            self.grid.bg = self.grid.default_bg;
                            self.grid.bold = false;
                        }
                        1 => self.grid.bold = true,
                        22 => self.grid.bold = false,
                        // Standard foreground colors 30-37
                        n @ 30..=37 => self.grid.fg = self.grid.palette[(n - 30) as usize],
                        39 => self.grid.fg = self.grid.default_fg,
                        // Standard background colors 40-47
                        n @ 40..=47 => self.grid.bg = self.grid.palette[(n - 40) as usize],
                        49 => self.grid.bg = self.grid.default_bg,
                        // Bright foreground 90-97
                        n @ 90..=97 => self.grid.fg = self.grid.palette[(n - 90 + 8) as usize],
                        // Bright background 100-107
                        n @ 100..=107 => self.grid.bg = self.grid.palette[(n - 100 + 8) as usize],
                        // 256-color and truecolor
                        38 => {
                            if i + 1 < ps.len() {
                                match ps[i + 1] {
                                    5 if i + 2 < ps.len() => {
                                        self.grid.fg = color256(ps[i + 2] as u8, &self.grid.palette);
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
                                        self.grid.bg = color256(ps[i + 2] as u8, &self.grid.palette);
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

fn color256(n: u8, palette: &[Color; 16]) -> Color {
    if n < 16 {
        return palette[n as usize];
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
