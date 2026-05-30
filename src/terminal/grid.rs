use super::sixel::SixelImage;
use std::collections::VecDeque;
use std::sync::Arc;

/// DECSCUSR cursor shape (CSI Ps SP q).
/// Blinking variants use the existing blink_visible flag in PaneView.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Beam,
}

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

pub struct GridColors {
    pub fg: Color,
    pub bg: Color,
    pub cursor: Color,
    pub selection: Color,
    pub palette: [Color; 16],
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
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
            italic: false,
            underline: false,
            strikethrough: false,
            overline: false,
            reverse: false,
            blink: false,
            wide: false,
            wide_cont: false,
            url: None,
        }
    }
}

struct SavedCursor {
    col: usize,
    row: usize,
    fg: Color,
    bg: Color,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    overline: bool,
    reverse: bool,
    blink: bool,
}

struct SavedScreen {
    cells: Vec<Cell>,
    cols: usize,
    rows: usize,
    cursor_col: usize,
    cursor_row: usize,
    cursor_visible: bool,
    scroll_top: usize,
    scroll_bottom: usize,
    fg: Color,
    bg: Color,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    overline: bool,
    reverse: bool,
    blink: bool,
    scrollback: VecDeque<Vec<Cell>>,
    scrollback_wrapped: VecDeque<bool>,
    row_wrapped: Vec<bool>,
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
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub reverse: bool,
    pub blink: bool,
    // DECSC/DECRC saved cursor (ESC 7 / ESC 8)
    decsc: Option<SavedCursor>,
    // Scrollback: lines that have scrolled off the top (oldest first)
    pub scrollback: VecDeque<Vec<Cell>>,
    // Parallel to scrollback: true means the corresponding line soft-wrapped
    // into the next (autowrap at column boundary, not an explicit newline).
    pub scrollback_wrapped: VecDeque<bool>,
    // Per live-grid row: row_wrapped[r] = true when row r autowrapped into row r+1.
    // Shifts left (with a new false appended) each time scroll_up pushes row 0.
    row_wrapped: Vec<bool>,
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
    // DECSCUSR: cursor shape set by the running program
    pub cursor_shape: CursorShape,
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
    // Maximum number of scrollback lines
    pub scrollback_max: usize,
    // Response bytes to be written back to the PTY (DSR, DA replies)
    pub pending_responses: Vec<u8>,
    // OSC 52 clipboard operations: text to write, or true = read request
    pub pending_clipboard_write: Option<String>,
    pub pending_clipboard_read: bool,
    // DEC Special Graphics character set (ESC ( 0 = on, ESC ( B = off)
    pub charset_drawing: bool,
    // Focus reporting mode (?1004): send \e[I on focus-in, \e[O on focus-out
    pub focus_report: bool,
    // DECAWM (?7): autowrap — when false, chars at the right margin overwrite instead of wrapping
    pub autowrap: bool,
    // Sixel images anchored to live-grid cell coordinates; cleared on clear_screen
    // and alternate-screen transitions.
    pub images: Vec<SixelImage>,
}

impl Grid {
    pub fn with_colors(
        cols: usize,
        rows: usize,
        colors: GridColors,
        scrollback_max: usize,
    ) -> Self {
        let GridColors {
            fg: default_fg,
            bg: default_bg,
            cursor: cursor_color,
            selection: selection_color,
            palette,
        } = colors;
        let blank = Cell {
            c: ' ',
            fg: default_fg,
            bg: default_bg,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            strikethrough: false,
            overline: false,
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
            italic: false,
            underline: false,
            strikethrough: false,
            overline: false,
            reverse: false,
            blink: false,
            decsc: None,
            scrollback: VecDeque::new(),
            scrollback_wrapped: VecDeque::new(),
            row_wrapped: vec![false; rows],
            default_fg,
            default_bg,
            cursor_color,
            selection_color,
            palette,
            application_cursor_keys: false,
            cursor_visible: true,
            cursor_shape: CursorShape::Block,
            bracketed_paste: false,
            mouse_mode: 0,
            mouse_sgr: false,
            alternate_saved: None,
            current_url: None,
            cwd: None,
            osc_title: None,
            bell_pending: false,
            scrollback_max,
            pending_responses: Vec::new(),
            pending_clipboard_write: None,
            pending_clipboard_read: false,
            charset_drawing: false,
            focus_report: false,
            autowrap: true,
            images: Vec::new(),
        }
    }

    pub fn enter_alternate_screen(&mut self) {
        if self.alternate_saved.is_some() {
            return;
        }
        let blank = self.blank_cell();
        self.alternate_saved = Some(SavedScreen {
            cells: std::mem::replace(&mut self.cells, vec![blank; self.cols * self.rows]),
            cols: self.cols,
            rows: self.rows,
            cursor_col: self.cursor_col,
            cursor_row: self.cursor_row,
            cursor_visible: self.cursor_visible,
            scroll_top: self.scroll_top,
            scroll_bottom: self.scroll_bottom,
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
            dim: self.dim,
            italic: self.italic,
            underline: self.underline,
            strikethrough: self.strikethrough,
            overline: self.overline,
            reverse: self.reverse,
            blink: self.blink,
            scrollback: std::mem::take(&mut self.scrollback),
            scrollback_wrapped: std::mem::take(&mut self.scrollback_wrapped),
            row_wrapped: std::mem::replace(&mut self.row_wrapped, vec![false; self.rows]),
            current_url: self.current_url.take(),
        });
        self.cursor_col = 0;
        self.cursor_row = 0;
        self.cursor_visible = true;
        self.cursor_shape = CursorShape::Block;
        self.scroll_top = 0;
        self.scroll_bottom = self.rows - 1;
        self.fg = self.default_fg;
        self.bg = self.default_bg;
        self.bold = false;
        self.dim = false;
        self.italic = false;
        self.underline = false;
        self.strikethrough = false;
        self.overline = false;
        self.reverse = false;
        self.blink = false;
        self.charset_drawing = false;
        self.images.clear();
    }

    pub fn exit_alternate_screen(&mut self) {
        if let Some(saved) = self.alternate_saved.take() {
            // The terminal may have been resized while in alternate screen.
            // Refit the saved main-screen cells to the current dimensions.
            if saved.cols == self.cols && saved.rows == self.rows {
                self.cells = saved.cells;
            } else {
                let blank = self.blank_cell();
                let mut cells = vec![blank; self.cols * self.rows];
                let copy_cols = saved.cols.min(self.cols);
                let copy_rows = saved.rows.min(self.rows);
                for r in 0..copy_rows {
                    for c in 0..copy_cols {
                        cells[r * self.cols + c] = saved.cells[r * saved.cols + c].clone();
                    }
                }
                self.cells = cells;
            }
            self.cursor_col = saved.cursor_col.min(self.cols.saturating_sub(1));
            self.cursor_row = saved.cursor_row.min(self.rows.saturating_sub(1));
            self.cursor_visible = saved.cursor_visible;
            self.scroll_top = saved.scroll_top.min(self.rows.saturating_sub(1));
            self.scroll_bottom = saved.scroll_bottom.min(self.rows.saturating_sub(1));
            self.fg = saved.fg;
            self.bg = saved.bg;
            self.bold = saved.bold;
            self.dim = saved.dim;
            self.italic = saved.italic;
            self.underline = saved.underline;
            self.strikethrough = saved.strikethrough;
            self.overline = saved.overline;
            self.reverse = saved.reverse;
            self.blink = saved.blink;
            self.scrollback = saved.scrollback;
            self.scrollback_wrapped = saved.scrollback_wrapped;
            self.row_wrapped = saved.row_wrapped;
            self.row_wrapped.resize(self.rows, false);
            self.current_url = saved.current_url;
            self.images.clear();
        }
    }

    #[allow(dead_code)]
    pub fn in_alternate_screen(&self) -> bool {
        self.alternate_saved.is_some()
    }

    /// Remove trailing default-background spaces from a logical line.
    /// Wide-continuation placeholders are never trimmed (they pair with the
    /// preceding wide cell).
    fn trim_line(line: &mut Vec<Cell>, default_bg: Color) {
        while matches!(line.last(), Some(c) if c.c == ' ' && c.bg == default_bg && !c.wide_cont) {
            line.pop();
        }
    }

    /// Join soft-wrapped physical rows into logical lines and trim each.
    fn join_logical_lines(
        rows: impl Iterator<Item = (Vec<Cell>, bool)>,
        default_bg: Color,
    ) -> Vec<Vec<Cell>> {
        let mut lines: Vec<Vec<Cell>> = Vec::new();
        let mut current: Vec<Cell> = Vec::new();
        for (cells, soft_wrapped) in rows {
            current.extend(cells);
            if !soft_wrapped {
                Self::trim_line(&mut current, default_bg);
                lines.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            Self::trim_line(&mut current, default_bg);
            lines.push(current);
        }
        lines
    }

    /// Split a logical line into physical chunks of `new_cols`, never splitting
    /// a wide character across rows. Each chunk is padded to `new_cols`.
    /// Returns `(chunk, soft_wrapped)` pairs.
    fn split_logical_line(line: &[Cell], new_cols: usize, blank: &Cell) -> Vec<(Vec<Cell>, bool)> {
        if line.is_empty() {
            return vec![(vec![blank.clone(); new_cols], false)];
        }
        let mut chunks = Vec::new();
        let mut pos = 0;
        while pos < line.len() {
            let end_raw = (pos + new_cols).min(line.len());
            // Step back one if the split lands on the left half of a wide char.
            let end = if end_raw < line.len() && end_raw > 0 && line[end_raw - 1].wide {
                end_raw - 1
            } else {
                end_raw
            };
            let is_last = end >= line.len();
            let mut chunk = line[pos..end].to_vec();
            chunk.resize(new_cols, blank.clone());
            chunks.push((chunk, !is_last));
            pos = end;
        }
        chunks
    }

    /// Push a row into scrollback, applying the scrollback-max cap.
    fn push_scrollback(&mut self, row: Vec<Cell>, soft_wrap: bool) {
        self.scrollback.push_back(row);
        self.scrollback_wrapped.push_back(soft_wrap);
        if self.scrollback.len() > self.scrollback_max {
            self.scrollback.pop_front();
            self.scrollback_wrapped.pop_front();
        }
    }

    /// Reflow scrollback lines to fit `new_cols`. Returns the signed change in
    /// scrollback line count so callers can adjust their `scroll_offset`.
    fn reflow_scrollback(&mut self, new_cols: usize) -> isize {
        let old_len = self.scrollback.len() as isize;
        let blank = self.blank_cell();
        let default_bg = self.default_bg;

        let rows = self
            .scrollback
            .drain(..)
            .zip(self.scrollback_wrapped.drain(..));
        for line in Self::join_logical_lines(rows, default_bg) {
            for (chunk, soft_wrap) in Self::split_logical_line(&line, new_cols, &blank) {
                self.push_scrollback(chunk, soft_wrap);
            }
        }
        self.scrollback.len() as isize - old_len
    }

    /// Find which logical line and offset within it the cursor occupies.
    /// Scans rows linearly, tracking the current logical-line index and how
    /// many physical rows deep into it we are.
    fn cursor_logical_pos(
        row_wrapped: &[bool],
        cursor_row: usize,
        cursor_col: usize,
        old_cols: usize,
    ) -> (usize, usize) {
        let mut li = 0usize;
        let mut depth = 0usize; // physical rows into the current logical line
        for (r, &wrapped) in row_wrapped.iter().enumerate() {
            if r == cursor_row {
                return (li, depth * old_cols + cursor_col);
            }
            if wrapped {
                depth += 1;
            } else {
                li += 1;
                depth = 0;
            }
        }
        (li.saturating_sub(1), cursor_col)
    }

    /// Reflow the live grid to new dimensions. Logical lines are re-split at
    /// `new_cols`; rows that no longer fit spill into scrollback.
    /// Returns the signed change in scrollback length.
    fn reflow_live_grid(&mut self, new_cols: usize, new_rows: usize) -> isize {
        let old_cols = self.cols;
        let old_rows = self.rows;
        let blank = self.blank_cell();
        let default_bg = self.default_bg;

        let (cursor_ll, cursor_lc) = Self::cursor_logical_pos(
            &self.row_wrapped,
            self.cursor_row,
            self.cursor_col,
            old_cols,
        );

        let rows = (0..old_rows).map(|r| {
            (
                self.cells[r * old_cols..(r + 1) * old_cols].to_vec(),
                self.row_wrapped[r],
            )
        });
        let logical_lines = Self::join_logical_lines(rows, default_bg);

        // Re-split and locate new cursor position.
        let mut new_rows_data: Vec<(Vec<Cell>, bool)> = Vec::new();
        let mut new_cursor = (
            new_rows_data.len(),
            self.cursor_col.min(new_cols.saturating_sub(1)),
        );
        for (li, line) in logical_lines.iter().enumerate() {
            let first_row = new_rows_data.len();
            let chunks = Self::split_logical_line(line, new_cols, &blank);
            if li == cursor_ll {
                let ci = (cursor_lc / new_cols).min(chunks.len().saturating_sub(1));
                new_cursor = (first_row + ci, (cursor_lc % new_cols).min(new_cols - 1));
            }
            new_rows_data.extend(chunks);
        }

        let spill_count = new_rows_data.len().saturating_sub(new_rows);
        for (row, soft_wrap) in new_rows_data.drain(..spill_count) {
            self.push_scrollback(row, soft_wrap);
        }
        new_cursor.0 = new_cursor.0.saturating_sub(spill_count);

        new_rows_data.resize_with(new_rows, || (vec![blank.clone(); new_cols], false));

        let mut new_cells = vec![blank.clone(); new_cols * new_rows];
        let mut new_row_wrapped = vec![false; new_rows];
        for (r, (row, soft_wrap)) in new_rows_data.iter().enumerate() {
            let n = row.len().min(new_cols);
            new_cells[r * new_cols..r * new_cols + n].clone_from_slice(&row[..n]);
            new_row_wrapped[r] = *soft_wrap;
        }
        self.cells = new_cells;
        self.row_wrapped = new_row_wrapped;
        self.cursor_row = new_cursor.0.min(new_rows.saturating_sub(1));
        self.cursor_col = new_cursor.1;

        spill_count as isize
    }

    pub fn resize(&mut self, cols: usize, rows: usize) -> isize {
        // Reflow scrollback first (reads self.cols for trimming logic).
        let mut delta = if cols != self.cols {
            self.reflow_scrollback(cols)
        } else {
            0
        };

        // Reflow the live grid when dimensions change, the scroll region is
        // the full screen (scroll_top == 0), and we are not in alternate screen
        // (interactive apps like vim manage their own layout).
        if (cols != self.cols || rows != self.rows)
            && self.scroll_top == 0
            && self.alternate_saved.is_none()
        {
            delta += self.reflow_live_grid(cols, rows);
            // self.cells, row_wrapped, cursor already updated by reflow_live_grid.
        } else {
            // Naive resize: copy cells, truncate or extend.
            let blank = self.blank_cell();
            let mut new_cells = vec![blank; cols * rows];
            let copy_rows = self.rows.min(rows);
            let copy_cols = self.cols.min(cols);
            for r in 0..copy_rows {
                for c in 0..copy_cols {
                    new_cells[r * cols + c] = self.cells[r * self.cols + c].clone();
                }
            }
            self.cells = new_cells;
            self.row_wrapped.resize(rows, false);
            self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
            self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        }

        self.cols = cols;
        self.rows = rows;
        self.scroll_bottom = rows - 1;

        delta
    }

    pub fn cell_mut(&mut self, col: usize, row: usize) -> &mut Cell {
        &mut self.cells[row * self.cols + col]
    }

    pub fn cell(&self, col: usize, row: usize) -> &Cell {
        &self.cells[row * self.cols + col]
    }

    fn make_char_cell(&self, c: char, wide: bool) -> Cell {
        Cell {
            c,
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
            dim: self.dim,
            italic: self.italic,
            underline: self.underline,
            strikethrough: self.strikethrough,
            overline: self.overline,
            reverse: self.reverse,
            blink: self.blink,
            wide,
            wide_cont: false,
            url: self.current_url.clone(),
        }
    }

    pub fn write_char(&mut self, c: char) {
        use unicode_width::UnicodeWidthChar;
        let char_cols = UnicodeWidthChar::width(c).unwrap_or(1).max(1);

        if self.cursor_col + char_cols > self.cols {
            if self.autowrap {
                self.row_wrapped[self.cursor_row] = true;
                self.cursor_col = 0;
                self.advance_row();
            } else {
                self.cursor_col = self.cols - char_cols;
            }
        }

        let wide = char_cols == 2;
        let c = if self.charset_drawing {
            dec_line_drawing(c)
        } else {
            c
        };
        let new_cell = self.make_char_cell(c, wide);
        *self.cell_mut(self.cursor_col, self.cursor_row) = new_cell;
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

        if !self.autowrap {
            self.cursor_col = self.cursor_col.min(self.cols - 1);
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
            // Save the top row to scrollback before it is overwritten.
            if top == 0 {
                let line = self.cells[..cols].to_vec();
                // Shift row_wrapped left: the wrap status of row 0 becomes the
                // scrollback flag; new bottom row starts as hard-wrapped.
                let soft_wrap = if self.row_wrapped.is_empty() {
                    false
                } else {
                    self.row_wrapped.remove(0)
                };
                self.row_wrapped.push(false);
                self.scrollback.push_back(line);
                self.scrollback_wrapped.push_back(soft_wrap);
                if self.scrollback.len() > self.scrollback_max {
                    self.scrollback.pop_front();
                    self.scrollback_wrapped.pop_front();
                }
            }
            // Shift rows top..bot upward by one using a rotation. rotate_left
            // moves elements without cloning (ptr::copy internally), so the
            // only Clone calls are the 'cols' blank fills below.
            self.cells[top * cols..(bot + 1) * cols].rotate_left(cols);
            // Clear the bottom row (now holds stale content after rotation).
            for cell in &mut self.cells[bot * cols..(bot + 1) * cols] {
                *cell = blank.clone();
            }
        }
    }

    pub fn scroll_down(&mut self, n: usize) {
        let top = self.scroll_top;
        let bot = self.scroll_bottom;
        let cols = self.cols;
        let blank = self.erase_cell();
        for _ in 0..n {
            // Shift rows top..bot downward by one using a rotation.
            self.cells[top * cols..(bot + 1) * cols].rotate_right(cols);
            // Clear the top row (now holds stale content after rotation).
            for cell in &mut self.cells[top * cols..(top + 1) * cols] {
                *cell = blank.clone();
            }
        }
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    pub fn selected_text(
        &self,
        sc: usize,
        sr: usize,
        ec: usize,
        er: usize,
        scroll_offset: usize,
    ) -> String {
        let (r0, c0, r1, c1) = if (sr, sc) <= (er, ec) {
            (sr, sc, er, ec)
        } else {
            (er, ec, sr, sc)
        };
        let mut result = String::new();
        for row in r0..=r1 {
            let (col_start, col_end) = row_col_range(r0, r1, c0, c1, row, self.cols);
            let line = self.collect_row_text(row, col_start, col_end, scroll_offset);
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(line.trim_end_matches(' '));
        }
        result
    }

    fn collect_row_text(
        &self,
        row: usize,
        col_start: usize,
        col_end: usize,
        scroll_offset: usize,
    ) -> String {
        let mut line = String::new();
        for col in col_start..=col_end {
            if let Some(c) = self.cell_char_at(row, col, scroll_offset) {
                line.push(c);
            }
        }
        line
    }

    fn scrollback_char_at(&self, abs_row: usize, col: usize) -> Option<char> {
        let sb_len = self.scrollback.len();
        if abs_row < sb_len {
            let line = &self.scrollback[abs_row];
            Some(if col < line.len() { line[col].c } else { ' ' })
        } else {
            let live = abs_row.saturating_sub(sb_len);
            (live < self.rows).then(|| self.cell(col, live).c)
        }
    }

    pub(crate) fn cell_char_at(
        &self,
        row: usize,
        col: usize,
        scroll_offset: usize,
    ) -> Option<char> {
        if col >= self.cols {
            return None;
        }
        let sb_start = self.scrollback.len().saturating_sub(scroll_offset);
        if scroll_offset > 0 {
            self.scrollback_char_at(sb_start + row, col)
        } else if row < self.rows {
            Some(self.cell(col, row).c)
        } else {
            None
        }
    }

    pub fn blank_cell(&self) -> Cell {
        Cell {
            c: ' ',
            fg: self.default_fg,
            bg: self.default_bg,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            strikethrough: false,
            overline: false,
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
            italic: false,
            underline: false,
            strikethrough: false,
            overline: false,
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
        self.images.clear();
    }

    /// Scan all live grid rows for plain-text `http(s)://` URLs and stamp
    /// matching cells with `cell.url`.  OSC 8 cells (already non-None) are
    /// left untouched.  Call this after each PTY-data batch.
    pub fn scan_urls(&mut self) {
        for row in 0..self.rows {
            let chars: Vec<char> = (0..self.cols).map(|c| self.cell(c, row).c).collect();
            let cols = self.cols;
            let row_cells = &mut self.cells[row * cols..(row + 1) * cols];
            let mut col = 0;
            while col < chars.len() {
                if let Some(span) = url_span_at(&chars, col) {
                    let url_arc = Arc::new(chars[col..col + span].iter().collect::<String>());
                    stamp_url_span(row_cells, col, span, &url_arc);
                    col += span;
                } else {
                    col += 1;
                }
            }
        }
    }

    pub fn reset_sgr(&mut self) {
        self.fg = self.default_fg;
        self.bg = self.default_bg;
        self.bold = false;
        self.dim = false;
        self.italic = false;
        self.underline = false;
        self.strikethrough = false;
        self.overline = false;
        self.reverse = false;
        self.blink = false;
    }

    pub fn save_cursor(&mut self) {
        self.decsc = Some(SavedCursor {
            col: self.cursor_col,
            row: self.cursor_row,
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
            dim: self.dim,
            italic: self.italic,
            underline: self.underline,
            strikethrough: self.strikethrough,
            overline: self.overline,
            reverse: self.reverse,
            blink: self.blink,
        });
    }

    pub fn restore_cursor(&mut self) {
        if let Some(s) = &self.decsc {
            self.cursor_col = s.col.min(self.cols - 1);
            self.cursor_row = s.row.min(self.rows - 1);
            self.fg = s.fg;
            self.bg = s.bg;
            self.bold = s.bold;
            self.dim = s.dim;
            self.italic = s.italic;
            self.underline = s.underline;
            self.strikethrough = s.strikethrough;
            self.overline = s.overline;
            self.reverse = s.reverse;
            self.blink = s.blink;
        }
    }

    /// RIS — Full Reset (ESC c).  Clears all state as if the terminal were freshly opened.
    pub fn reset(&mut self) {
        // Exit alternate screen first so we reset the primary buffer.
        self.alternate_saved = None;

        let blank = Cell {
            c: ' ',
            fg: self.default_fg,
            bg: self.default_bg,
            ..Cell::default()
        };
        self.cells = vec![blank; self.cols * self.rows];
        self.scrollback.clear();
        self.scrollback_wrapped.clear();
        self.row_wrapped = vec![false; self.rows];

        self.cursor_col = 0;
        self.cursor_row = 0;
        self.scroll_top = 0;
        self.scroll_bottom = self.rows - 1;

        self.fg = self.default_fg;
        self.bg = self.default_bg;
        self.bold = false;
        self.dim = false;
        self.italic = false;
        self.underline = false;
        self.strikethrough = false;
        self.overline = false;
        self.reverse = false;
        self.blink = false;

        self.decsc = None;
        self.current_url = None;
        self.osc_title = None;
        self.cursor_visible = true;
        self.cursor_shape = CursorShape::Block;
        self.bracketed_paste = false;
        self.mouse_mode = 0;
        self.mouse_sgr = false;
        self.application_cursor_keys = false;
        self.charset_drawing = false;
        self.focus_report = false;
    }
}

fn stamp_url_span(row_cells: &mut [Cell], col: usize, span: usize, url_arc: &Arc<String>) {
    for cell in &mut row_cells[col..col + span] {
        if cell.url.is_none() {
            cell.url = Some(url_arc.clone());
        }
    }
}

fn row_col_range(
    r0: usize,
    r1: usize,
    c0: usize,
    c1: usize,
    row: usize,
    cols: usize,
) -> (usize, usize) {
    let col_start = if row == r0 { c0 } else { 0 };
    let col_end = if row == r1 {
        c1
    } else {
        cols.saturating_sub(1)
    };
    (col_start, col_end)
}

/// Strip trailing punctuation from a URL body.  Returns the adjusted length.
/// For `)` only strips if there is no matching `(` inside the URL body.
fn strip_trailing_punct(tail: &[char], prefix_len: usize, mut len: usize) -> usize {
    const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', '\'', '"', ']', '>'];
    while len > prefix_len {
        let c = tail[len - 1];
        if TRAILING.contains(&c) {
            len -= 1;
        } else if c == ')' {
            let open = tail[prefix_len..len].iter().filter(|&&x| x == '(').count();
            let close = tail[prefix_len..len].iter().filter(|&&x| x == ')').count();
            if close > open {
                len -= 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    len
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
    if len <= prefix_len {
        return None;
    }
    let len = strip_trailing_punct(tail, prefix_len, len);
    if len > prefix_len { Some(len) } else { None }
}

/// Map a character through the DEC Special Graphics character set (G0).
/// Only the 32 printable chars in the 0x60–0x7e range are remapped;
/// everything else passes through unchanged.
fn dec_line_drawing(c: char) -> char {
    match c {
        '`' => '◆',  // diamond
        'a' => '▒',  // checkerboard
        'b' => '␉',  // HT
        'c' => '␌',  // FF
        'd' => '\r', // CR
        'e' => '␊',  // LF
        'f' => '°',  // degree
        'g' => '±',  // plus-minus
        'h' => '␤',  // NL
        'i' => '␋',  // VT
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺', // scan line 1
        'p' => '⎻', // scan line 3
        'q' => '─',
        'r' => '⎼', // scan line 7
        's' => '⎽', // scan line 9
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        'y' => '≤',
        'z' => '≥',
        '{' => 'π',
        '|' => '≠',
        '}' => '£',
        '~' => '·', // middle dot
        _ => c,
    }
}

#[cfg(test)]
#[path = "grid_test.rs"]
mod tests;
