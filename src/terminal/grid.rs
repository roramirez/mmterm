use std::collections::VecDeque;
use std::sync::Arc;

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
    pub dim: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub reverse: bool,
    pub blink: bool,
    /// True when this is the left half of a 2-column wide character.
    pub wide: bool,
    /// True when this is the right (placeholder) half of a wide character.
    pub wide_cont: bool,
    /// OSC 8 hyperlink URI, shared across all cells in the same link span.
    pub url: Option<Arc<String>>,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: Color::WHITE,
            bg: Color::BLACK,
            bold: false,
            dim: false,
            underline: false,
            strikethrough: false,
            reverse: false,
            blink: false,
            wide: false,
            wide_cont: false,
            url: None,
        }
    }
}

struct SavedScreen {
    cells: Vec<Cell>,
    cursor_col: usize,
    cursor_row: usize,
    scroll_top: usize,
    scroll_bottom: usize,
    fg: Color,
    bg: Color,
    bold: bool,
    dim: bool,
    underline: bool,
    strikethrough: bool,
    reverse: bool,
    blink: bool,
    scrollback: VecDeque<Vec<Cell>>,
    current_url: Option<Arc<String>>,
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
    pub dim: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub reverse: bool,
    pub blink: bool,
    // DECSC/DECRC saved cursor
    pub saved_cursor_col: usize,
    pub saved_cursor_row: usize,
    // Scrollback: lines that have scrolled off the top (oldest first)
    pub scrollback: VecDeque<Vec<Cell>>,
    // Theme colors
    pub default_fg: Color,
    pub default_bg: Color,
    pub cursor_color: Color,
    pub selection_color: Color,
    pub palette: [Color; 16],
    // DECCKM: when true, arrow keys send SS3 sequences (\eOA) instead of CSI (\e[A)
    pub application_cursor_keys: bool,
    // DECTCEM: cursor visibility
    pub cursor_visible: bool,
    // Bracketed paste mode (?2004)
    pub bracketed_paste: bool,
    // Mouse reporting mode: 0=off, 1000=click, 1002=button-motion, 1003=any-motion
    pub mouse_mode: u16,
    // SGR extended mouse encoding (?1006)
    pub mouse_sgr: bool,
    // Alternate screen buffer (?1049): holds saved primary screen while in alt screen
    alternate_saved: Option<SavedScreen>,
    // OSC 8 hyperlink: URI for cells written while non-None
    pub current_url: Option<Arc<String>>,
    // OSC 7: current working directory reported by the shell
    pub cwd: Option<String>,
    // OSC 0/1/2: window title set by the running program
    pub osc_title: Option<String>,
    // Set by BEL (0x07); consumed by App to trigger a visual flash
    pub bell_pending: bool,
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
        let blank = Cell {
            c: ' ',
            fg: default_fg,
            bg: default_bg,
            bold: false,
            dim: false,
            underline: false,
            strikethrough: false,
            reverse: false,
            blink: false,
            wide: false,
            wide_cont: false,
            url: None,
        };
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
            dim: false,
            underline: false,
            strikethrough: false,
            reverse: false,
            blink: false,
            saved_cursor_col: 0,
            saved_cursor_row: 0,
            scrollback: VecDeque::new(),
            default_fg,
            default_bg,
            cursor_color,
            selection_color,
            palette,
            application_cursor_keys: false,
            cursor_visible: true,
            bracketed_paste: false,
            mouse_mode: 0,
            mouse_sgr: false,
            alternate_saved: None,
            current_url: None,
            cwd: None,
            osc_title: None,
            bell_pending: false,
        }
    }

    pub fn enter_alternate_screen(&mut self) {
        if self.alternate_saved.is_some() {
            return;
        }
        let blank = self.blank_cell();
        self.alternate_saved = Some(SavedScreen {
            cells: std::mem::replace(&mut self.cells, vec![blank; self.cols * self.rows]),
            cursor_col: self.cursor_col,
            cursor_row: self.cursor_row,
            scroll_top: self.scroll_top,
            scroll_bottom: self.scroll_bottom,
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
            dim: self.dim,
            underline: self.underline,
            strikethrough: self.strikethrough,
            reverse: self.reverse,
            blink: self.blink,
            scrollback: std::mem::take(&mut self.scrollback),
            current_url: self.current_url.take(),
        });
        self.cursor_col = 0;
        self.cursor_row = 0;
        self.scroll_top = 0;
        self.scroll_bottom = self.rows - 1;
        self.fg = self.default_fg;
        self.bg = self.default_bg;
        self.bold = false;
        self.dim = false;
        self.underline = false;
        self.strikethrough = false;
        self.reverse = false;
        self.blink = false;
    }

    pub fn exit_alternate_screen(&mut self) {
        if let Some(saved) = self.alternate_saved.take() {
            self.cells = saved.cells;
            self.cursor_col = saved.cursor_col;
            self.cursor_row = saved.cursor_row;
            self.scroll_top = saved.scroll_top;
            self.scroll_bottom = saved.scroll_bottom;
            self.fg = saved.fg;
            self.bg = saved.bg;
            self.bold = saved.bold;
            self.dim = saved.dim;
            self.underline = saved.underline;
            self.strikethrough = saved.strikethrough;
            self.reverse = saved.reverse;
            self.blink = saved.blink;
            self.scrollback = saved.scrollback;
            self.current_url = saved.current_url;
        }
    }

    #[allow(dead_code)]
    pub fn in_alternate_screen(&self) -> bool {
        self.alternate_saved.is_some()
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

        let (fg, bg) = if self.reverse {
            (self.bg, self.fg)
        } else {
            (self.fg, self.bg)
        };
        let bold = self.bold;
        let dim = self.dim;
        let underline = self.underline;
        let strikethrough = self.strikethrough;
        let reverse = self.reverse;
        let blink = self.blink;
        let wide = char_cols == 2;
        let url = self.current_url.clone();

        let cell = self.cell_mut(self.cursor_col, self.cursor_row);
        cell.c = c;
        cell.fg = fg;
        cell.bg = bg;
        cell.bold = bold;
        cell.dim = dim;
        cell.underline = underline;
        cell.strikethrough = strikethrough;
        cell.reverse = reverse;
        cell.blink = blink;
        cell.wide = wide;
        cell.wide_cont = false;
        cell.url = url;
        self.cursor_col += 1;

        if wide && self.cursor_col < self.cols {
            let blank = self.erase_cell();
            let cont = self.cell_mut(self.cursor_col, self.cursor_row);
            *cont = Cell {
                wide_cont: true,
                ..blank
            };
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
        let blank = self.erase_cell();
        for _ in 0..n {
            if top == 0 {
                let line: Vec<Cell> = (0..cols)
                    .map(|c| self.cells[top * cols + c].clone())
                    .collect();
                self.scrollback.push_back(line);
                if self.scrollback.len() > SCROLLBACK_MAX {
                    self.scrollback.pop_front();
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

    pub fn scroll_down(&mut self, n: usize) {
        let top = self.scroll_top;
        let bot = self.scroll_bottom;
        let cols = self.cols;
        let blank = self.erase_cell();
        for _ in 0..n {
            for r in (top..bot).rev() {
                for c in 0..cols {
                    self.cells[(r + 1) * cols + c] = self.cells[r * cols + c].clone();
                }
            }
            for c in 0..cols {
                self.cells[top * cols + c] = blank.clone();
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
            let col_end = if row == r1 {
                c1
            } else {
                self.cols.saturating_sub(1)
            };
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
        Cell {
            c: ' ',
            fg: self.default_fg,
            bg: self.default_bg,
            bold: false,
            dim: false,
            underline: false,
            strikethrough: false,
            reverse: false,
            blink: false,
            wide: false,
            wide_cont: false,
            url: None,
        }
    }

    // Erase operations (ED, EL, scroll blank rows) use the current SGR background,
    // not the default — this is the BCE (Background Color Erase) behaviour that
    // xterm and most terminals implement.
    pub fn erase_cell(&self) -> Cell {
        Cell {
            c: ' ',
            fg: self.default_fg,
            bg: self.bg,
            bold: false,
            dim: false,
            underline: false,
            strikethrough: false,
            reverse: false,
            blink: false,
            wide: false,
            wide_cont: false,
            url: None,
        }
    }

    pub fn clear_line(&mut self, row: usize) {
        let cols = self.cols;
        let blank = self.erase_cell();
        for c in 0..cols {
            self.cells[row * cols + c] = blank.clone();
        }
    }

    pub fn clear_screen(&mut self) {
        let blank = self.erase_cell();
        self.cells = vec![blank; self.cols * self.rows];
    }

    /// Scan all live grid rows for plain-text `http(s)://` URLs and stamp
    /// matching cells with `cell.url`.  OSC 8 cells (already non-None) are
    /// left untouched.  Call this after each PTY-data batch.
    pub fn scan_urls(&mut self) {
        for row in 0..self.rows {
            let chars: Vec<char> = (0..self.cols).map(|c| self.cell(c, row).c).collect();
            let mut col = 0;
            while col < chars.len() {
                if let Some(span) = url_span_at(&chars, col) {
                    let url_arc = Arc::new(chars[col..col + span].iter().collect::<String>());
                    for c in col..col + span {
                        let cell = self.cell_mut(c, row);
                        if cell.url.is_none() {
                            cell.url = Some(url_arc.clone());
                        }
                    }
                    col += span;
                } else {
                    col += 1;
                }
            }
        }
    }
}

/// Returns the length (in chars) of a URL starting at `chars[start]`, or
/// `None` if there is no URL there.  Matches `http://…` and `https://…`
/// up to the first whitespace or C0 control character.
fn url_span_at(chars: &[char], start: usize) -> Option<usize> {
    let tail = &chars[start..];
    let prefix_len = if tail.starts_with(&['h', 't', 't', 'p', 's', ':', '/', '/']) {
        8
    } else if tail.starts_with(&['h', 't', 't', 'p', ':', '/', '/']) {
        7
    } else {
        return None;
    };
    let mut len = prefix_len;
    while len < tail.len() {
        let c = tail[len];
        if c <= ' ' {
            break;
        }
        len += 1;
    }
    if len > prefix_len { Some(len) } else { None }
}

#[cfg(test)]
#[path = "grid_tests.rs"]
mod tests;
