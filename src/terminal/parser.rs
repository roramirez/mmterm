use super::grid::{Color, Grid, GridColors};
use vte::{Params, Parser, Perform};

pub struct TerminalParser {
    pub grid: Grid,
    parser: Parser,
}

impl TerminalParser {
    pub fn new_with_colors(
        cols: usize,
        rows: usize,
        colors: GridColors,
        scrollback_max: usize,
    ) -> Self {
        Self {
            grid: Grid::with_colors(cols, rows, colors, scrollback_max),
            parser: Parser::new(),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        // vte requires a mutable performer; we route through a wrapper
        let mut performer = Performer {
            grid: &mut self.grid,
        };
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
            0x08 if self.grid.cursor_col > 0 => {
                self.grid.cursor_col -= 1;
            }
            0x07 => self.grid.bell_pending = true,
            0x09 => {
                // tab: advance to next 8-column boundary
                let next = (self.grid.cursor_col / 8 + 1) * 8;
                self.grid.cursor_col = next.min(self.grid.cols - 1);
            }
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
                ('h', 1) => self.grid.application_cursor_keys = true,
                ('l', 1) => self.grid.application_cursor_keys = false,
                ('h', 25) => self.grid.cursor_visible = true,
                ('l', 25) => self.grid.cursor_visible = false,
                ('h', 1000) => self.grid.mouse_mode = 1000,
                ('l', 1000) => self.grid.mouse_mode = 0,
                ('h', 1002) => self.grid.mouse_mode = 1002,
                ('l', 1002) => self.grid.mouse_mode = 0,
                ('h', 1003) => self.grid.mouse_mode = 1003,
                ('l', 1003) => self.grid.mouse_mode = 0,
                ('h', 1006) => self.grid.mouse_sgr = true,
                ('l', 1006) => self.grid.mouse_sgr = false,
                ('h', 1049) => self.grid.enter_alternate_screen(),
                ('l', 1049) => self.grid.exit_alternate_screen(),
                ('h', 2004) => self.grid.bracketed_paste = true,
                ('l', 2004) => self.grid.bracketed_paste = false,
                _ => {}
            }
            return;
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
                    let blank = self.grid.erase_cell();
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
                    let blank = self.grid.erase_cell();
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
                let blank = self.grid.erase_cell();
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
                    self.grid.dim = false;
                    self.grid.italic = false;
                    self.grid.underline = false;
                    self.grid.strikethrough = false;
                    self.grid.reverse = false;
                    self.grid.blink = false;
                    return;
                }
                let mut i = 0;
                while i < ps.len() {
                    match ps[i] {
                        0 => {
                            self.grid.fg = self.grid.default_fg;
                            self.grid.bg = self.grid.default_bg;
                            self.grid.bold = false;
                            self.grid.dim = false;
                            self.grid.italic = false;
                            self.grid.underline = false;
                            self.grid.strikethrough = false;
                            self.grid.reverse = false;
                            self.grid.blink = false;
                        }
                        1 => self.grid.bold = true,
                        2 => self.grid.dim = true,
                        3 => self.grid.italic = true,
                        4 => self.grid.underline = true,
                        5 => self.grid.blink = true,
                        7 => self.grid.reverse = true,
                        9 => self.grid.strikethrough = true,
                        22 => {
                            self.grid.bold = false;
                            self.grid.dim = false;
                        }
                        23 => self.grid.italic = false,
                        24 => self.grid.underline = false,
                        25 => self.grid.blink = false,
                        27 => self.grid.reverse = false,
                        29 => self.grid.strikethrough = false,
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
                        38 if i + 1 < ps.len() => match ps[i + 1] {
                            5 if i + 2 < ps.len() => {
                                self.grid.fg = color256(ps[i + 2] as u8, &self.grid.palette);
                                i += 2;
                            }
                            2 if i + 4 < ps.len() => {
                                self.grid.fg =
                                    Color::rgb(ps[i + 2] as u8, ps[i + 3] as u8, ps[i + 4] as u8);
                                i += 4;
                            }
                            _ => {}
                        },
                        48 if i + 1 < ps.len() => match ps[i + 1] {
                            5 if i + 2 < ps.len() => {
                                self.grid.bg = color256(ps[i + 2] as u8, &self.grid.palette);
                                i += 2;
                            }
                            2 if i + 4 < ps.len() => {
                                self.grid.bg =
                                    Color::rgb(ps[i + 2] as u8, ps[i + 3] as u8, ps[i + 4] as u8);
                                i += 4;
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                    i += 1;
                }
            }
            // Scroll up / scroll down
            'S' => self.grid.scroll_up(p0.max(1) as usize),
            'T' => self.grid.scroll_down(p0.max(1) as usize),
            // Insert Line: insert n blank lines at cursor, shifting rest of scroll region down
            'L' => {
                let n = p0.max(1) as usize;
                let saved_top = self.grid.scroll_top;
                self.grid.scroll_top = self.grid.cursor_row;
                self.grid.scroll_down(n);
                self.grid.scroll_top = saved_top;
                self.grid.cursor_col = 0;
            }
            // Delete Line: delete n lines at cursor, shifting rest of scroll region up
            'M' => {
                let n = p0.max(1) as usize;
                let saved_top = self.grid.scroll_top;
                self.grid.scroll_top = self.grid.cursor_row;
                self.grid.scroll_up(n);
                self.grid.scroll_top = saved_top;
                self.grid.cursor_col = 0;
            }
            // DCH: delete n characters at cursor, shift line left, fill end with blanks
            'P' => {
                let n = p0.max(1) as usize;
                let row = self.grid.cursor_row;
                let col = self.grid.cursor_col;
                let cols = self.grid.cols;
                let blank = self.grid.erase_cell();
                let n = n.min(cols - col);
                for c in col..cols {
                    self.grid.cells[row * cols + c] = if c + n < cols {
                        self.grid.cells[row * cols + c + n].clone()
                    } else {
                        blank.clone()
                    };
                }
            }
            // ICH: insert n blank characters at cursor, shift line right, drop overflow
            '@' => {
                let n = p0.max(1) as usize;
                let row = self.grid.cursor_row;
                let col = self.grid.cursor_col;
                let cols = self.grid.cols;
                let blank = self.grid.erase_cell();
                let n = n.min(cols - col);
                for c in (col..cols).rev() {
                    self.grid.cells[row * cols + c] = if c >= col + n {
                        self.grid.cells[row * cols + c - n].clone()
                    } else {
                        blank.clone()
                    };
                }
            }
            // ECH: erase n characters at cursor (replace with blanks, no shift)
            'X' => {
                let n = p0.max(1) as usize;
                let row = self.grid.cursor_row;
                let col = self.grid.cursor_col;
                let cols = self.grid.cols;
                let blank = self.grid.erase_cell();
                for c in col..(col + n).min(cols) {
                    self.grid.cells[row * cols + c] = blank.clone();
                }
            }
            // CHA: cursor horizontal absolute (move to column, 1-indexed)
            'G' => {
                let col = p0.saturating_sub(1) as usize;
                self.grid.cursor_col = col.min(self.grid.cols - 1);
            }
            // VPA: vertical position absolute (move to row, 1-indexed)
            'd' => {
                let row = p0.saturating_sub(1) as usize;
                self.grid.cursor_row = row.min(self.grid.rows - 1);
            }
            // DSR: Device Status Report — respond with active cursor position
            // CSI 6 n  →  CSI row ; col R  (1-indexed)
            'n' if p0 == 6 => {
                let row = self.grid.cursor_row + 1;
                let col = self.grid.cursor_col + 1;
                let resp = format!("\x1b[{row};{col}R");
                self.grid
                    .pending_responses
                    .extend_from_slice(resp.as_bytes());
            }
            // DA: Device Attributes — report as VT100 with no options
            // CSI c  or  CSI 0 c  →  CSI ? 1 ; 0 c
            'c' if p0 == 0 => {
                self.grid.pending_responses.extend_from_slice(b"\x1b[?1;0c");
            }
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

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC 0/1/2: set window title (0 and 2 = window title, 1 = icon name)
        if let [code, title] = params
            && matches!(*code, b"0" | b"1" | b"2")
            && let Ok(s) = std::str::from_utf8(title)
        {
            let t = s.trim();
            self.grid.osc_title = if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            };
        }
        // OSC 7: current working directory reported by the shell
        if let [b"7", uri] = params
            && let Ok(s) = std::str::from_utf8(uri)
        {
            self.grid.cwd = parse_osc7_uri(s);
        }
        // OSC 8 hyperlink: \e]8;params;uri\e\\  (empty uri = end link)
        if let [osc, _, uri, ..] = params
            && *osc == b"8"
        {
            if uri.is_empty() {
                self.grid.current_url = None;
            } else if let Ok(s) = std::str::from_utf8(uri) {
                self.grid.current_url = Some(std::sync::Arc::new(s.to_string()));
            }
        }
    }
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            ([], b'M') => {
                // Reverse index: move up, or scroll content down if at top of scroll region
                if self.grid.cursor_row > self.grid.scroll_top {
                    self.grid.cursor_row -= 1;
                } else {
                    self.grid.scroll_down(1);
                }
            }
            ([], b'7') => {
                // DECSC: save cursor position and SGR attributes
                self.grid.save_cursor();
            }
            ([], b'8') => {
                // DECRC: restore cursor position and SGR attributes
                self.grid.restore_cursor();
            }
            _ => {}
        }
    }
}

fn parse_osc7_uri(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    if rest.starts_with('/') {
        Some(rest.to_string())
    } else {
        rest.find('/').map(|i| rest[i..].to_string())
    }
}

#[cfg(test)]
#[path = "parser_test.rs"]
mod tests;

#[cfg(test)]
#[path = "scenario_test.rs"]
mod scenarios;

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
