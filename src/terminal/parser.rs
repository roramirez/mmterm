use super::grid::{Color, CursorShape, Grid};

fn param_or_one(p: u16) -> usize {
    p.max(1) as usize
}
use super::sixel::SixelDecoder;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use vte::{Params, Parser, Perform};

pub struct TerminalParser {
    parser: Parser,
}

impl Default for TerminalParser {
    fn default() -> Self {
        Self {
            parser: Parser::new(),
        }
    }
}

impl TerminalParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process(&mut self, bytes: &[u8], grid: &mut Grid) {
        let mut performer = Performer {
            grid,
            dcs_kind: None,
            sixel_decoder: None,
            sixel_col: 0,
            sixel_row: 0,
        };
        for &byte in bytes {
            self.parser.advance(&mut performer, byte);
        }
    }
}

enum DcsKind {
    Sixel,
    Unknown,
}

fn cursor_shape_from_param(p0: u16) -> CursorShape {
    match p0 {
        3 | 4 => CursorShape::Underline,
        5 | 6 => CursorShape::Beam,
        _ => CursorShape::Block,
    }
}

fn sgr_should_reset(ps: &[u16]) -> bool {
    ps.is_empty() || (ps.len() == 1 && ps[0] == 0)
}

struct Performer<'a> {
    grid: &'a mut Grid,
    dcs_kind: Option<DcsKind>,
    sixel_decoder: Option<SixelDecoder>,
    sixel_col: usize,
    sixel_row: usize,
}

impl Performer<'_> {
    fn handle_dec_private_modes(&mut self, action: char, p0: u16) {
        match (action, p0) {
            ('h', 1) => self.grid.application_cursor_keys = true,
            ('l', 1) => self.grid.application_cursor_keys = false,
            ('h', 7) => self.grid.autowrap = true,
            ('l', 7) => self.grid.autowrap = false,
            ('h', 25) => self.grid.cursor_visible = true,
            ('l', 25) => self.grid.cursor_visible = false,
            ('h', 1000) => self.grid.mouse_mode = 1000,
            ('l', 1000) => self.grid.mouse_mode = 0,
            ('h', 1002) => self.grid.mouse_mode = 1002,
            ('l', 1002) => self.grid.mouse_mode = 0,
            ('h', 1003) => self.grid.mouse_mode = 1003,
            ('l', 1003) => self.grid.mouse_mode = 0,
            ('h', 1004) => self.grid.focus_report = true,
            ('l', 1004) => self.grid.focus_report = false,
            ('h', 1006) => self.grid.mouse_sgr = true,
            ('l', 1006) => self.grid.mouse_sgr = false,
            ('h', 1049) => self.grid.enter_alternate_screen(),
            ('l', 1049) => self.grid.exit_alternate_screen(),
            ('h', 2004) => self.grid.bracketed_paste = true,
            ('l', 2004) => self.grid.bracketed_paste = false,
            _ => {}
        }
    }

    fn erase_partial_row(&mut self, row: usize, col_start: usize, col_end: usize) {
        let cols = self.grid.cols;
        let blank = self.grid.erase_cell();
        self.grid.cells[row * cols + col_start..row * cols + col_end].fill(blank);
    }

    fn handle_erase_display(&mut self, p0: u16) {
        let row = self.grid.cursor_row;
        let col = self.grid.cursor_col;
        match p0 {
            0 => {
                self.erase_partial_row(row, col, self.grid.cols);
                for r in (row + 1)..self.grid.rows {
                    self.grid.clear_line(r);
                }
            }
            1 => {
                self.erase_partial_row(row, 0, col + 1);
                for r in 0..row {
                    self.grid.clear_line(r);
                }
            }
            2 | 3 => self.grid.clear_screen(),
            _ => {}
        }
    }

    fn handle_insert_line(&mut self, n: usize) {
        let saved_top = self.grid.scroll_top;
        self.grid.scroll_top = self.grid.cursor_row;
        self.grid.scroll_down(n);
        self.grid.scroll_top = saved_top;
        self.grid.cursor_col = 0;
    }

    fn handle_delete_line(&mut self, n: usize) {
        let saved_top = self.grid.scroll_top;
        self.grid.scroll_top = self.grid.cursor_row;
        self.grid.scroll_up(n);
        self.grid.scroll_top = saved_top;
        self.grid.cursor_col = 0;
    }

    fn handle_erase_line(&mut self, p0: u16) {
        let row = self.grid.cursor_row;
        let col = self.grid.cursor_col;
        match p0 {
            0 => self.erase_partial_row(row, col, self.grid.cols),
            1 => self.erase_partial_row(row, 0, col + 1),
            2 => self.grid.clear_line(row),
            _ => {}
        }
    }

    fn apply_sgr_color_param(&mut self, ps: &[u16], i: usize, fg: bool) -> usize {
        if let Some((color, skip)) = parse_color_from_params(ps, i, &self.grid.palette) {
            if fg {
                self.grid.fg = color;
            } else {
                self.grid.bg = color;
            }
            skip
        } else {
            0
        }
    }

    fn handle_sgr(&mut self, ps: &[u16]) {
        if sgr_should_reset(ps) {
            self.grid.reset_sgr();
            return;
        }
        let mut i = 0;
        while i < ps.len() {
            match ps[i] {
                0 => self.grid.reset_sgr(),
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
                53 => self.grid.overline = true,
                55 => self.grid.overline = false,
                n @ 30..=37 => self.grid.fg = self.grid.palette[(n - 30) as usize],
                39 => self.grid.fg = self.grid.default_fg,
                n @ 40..=47 => self.grid.bg = self.grid.palette[(n - 40) as usize],
                49 => self.grid.bg = self.grid.default_bg,
                n @ 90..=97 => self.grid.fg = self.grid.palette[(n - 90 + 8) as usize],
                n @ 100..=107 => self.grid.bg = self.grid.palette[(n - 100 + 8) as usize],
                38 => i += self.apply_sgr_color_param(ps, i, true),
                48 => i += self.apply_sgr_color_param(ps, i, false),
                _ => {}
            }
            i += 1;
        }
    }

    fn handle_char_ops(&mut self, action: char, p0: u16) {
        let n = param_or_one(p0);
        let row = self.grid.cursor_row;
        let col = self.grid.cursor_col;
        let cols = self.grid.cols;
        let blank = self.grid.erase_cell();
        match action {
            'P' => char_delete_n(&mut self.grid.cells, row, col, cols, n, blank),
            '@' => char_insert_n(&mut self.grid.cells, row, col, cols, n, blank),
            'X' => {
                for c in col..(col + n).min(cols) {
                    self.grid.cells[row * cols + c] = blank.clone();
                }
            }
            _ => {}
        }
    }

    fn handle_scroll_region(&mut self, p0: u16, p1: u16) {
        let top = (p0.saturating_sub(1) as usize).min(self.grid.max_row());
        let bot = if p1 == 0 {
            self.grid.max_row()
        } else {
            (p1 - 1) as usize
        }
        .min(self.grid.max_row());
        self.grid.scroll_top = top;
        self.grid.scroll_bottom = bot;
        self.grid.cursor_row = top;
        self.grid.cursor_col = 0;
    }
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
                self.grid.cursor_col = next.min(self.grid.max_col());
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let ps: Vec<u16> = params.iter().map(|p| p[0]).collect();
        let p0 = ps.first().copied().unwrap_or(0);
        let p1 = ps.get(1).copied().unwrap_or(0);

        if intermediates == b"?" {
            self.handle_dec_private_modes(action, p0);
            return;
        }

        match action {
            'A' => self.grid.cursor_row = self.grid.cursor_row.saturating_sub(param_or_one(p0)),
            'B' => {
                self.grid.cursor_row =
                    (self.grid.cursor_row + param_or_one(p0)).min(self.grid.max_row());
            }
            'C' => {
                self.grid.cursor_col =
                    (self.grid.cursor_col + param_or_one(p0)).min(self.grid.max_col());
            }
            'D' => self.grid.cursor_col = self.grid.cursor_col.saturating_sub(param_or_one(p0)),
            // Cursor position (row;col, 1-indexed)
            'H' | 'f' => {
                self.grid.cursor_row = (p0.saturating_sub(1) as usize).min(self.grid.max_row());
                self.grid.cursor_col = (p1.saturating_sub(1) as usize).min(self.grid.max_col());
            }
            'J' => self.handle_erase_display(p0),
            'K' => self.handle_erase_line(p0),
            'm' => self.handle_sgr(&ps),
            'S' => self.grid.scroll_up(param_or_one(p0)),
            'T' => self.grid.scroll_down(param_or_one(p0)),
            // Insert Line
            'L' => self.handle_insert_line(param_or_one(p0)),
            // Delete Line
            'M' => self.handle_delete_line(param_or_one(p0)),
            'P' | '@' | 'X' => self.handle_char_ops(action, p0),
            // CHA: cursor horizontal absolute (1-indexed)
            'G' => self.grid.cursor_col = (p0.saturating_sub(1) as usize).min(self.grid.max_col()),
            // VPA: vertical position absolute (1-indexed)
            'd' => self.grid.cursor_row = (p0.saturating_sub(1) as usize).min(self.grid.max_row()),
            // DSR: respond with cursor position (CSI 6 n → CSI row;col R)
            'n' if p0 == 6 => {
                let resp = format!(
                    "\x1b[{};{}R",
                    self.grid.cursor_row + 1,
                    self.grid.cursor_col + 1
                );
                self.grid
                    .pending_responses
                    .extend_from_slice(resp.as_bytes());
            }
            // DA: device attributes (CSI c → CSI ?1;0c)
            'c' if p0 == 0 => self.grid.pending_responses.extend_from_slice(b"\x1b[?1;0c"),
            // DECSCUSR: cursor shape (CSI Ps SP q)
            'q' if intermediates == b" " => {
                self.grid.cursor_shape = cursor_shape_from_param(p0);
            }
            // Set scroll region
            'r' => self.handle_scroll_region(p0, p1),
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        osc_set_title(self.grid, params);
        osc_set_cwd(self.grid, params);
        osc_set_hyperlink(self.grid, params);
        osc_clipboard(self.grid, params);
    }
    fn hook(&mut self, _params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        // Sixel graphics: DCS P...q (final char = 'q', no intermediates)
        if action == 'q' && intermediates.is_empty() {
            self.dcs_kind = Some(DcsKind::Sixel);
            self.sixel_decoder = Some(SixelDecoder::new());
            self.sixel_col = self.grid.cursor_col;
            self.sixel_row = self.grid.cursor_row;
        } else {
            self.dcs_kind = Some(DcsKind::Unknown);
        }
    }

    fn put(&mut self, byte: u8) {
        if let (Some(DcsKind::Sixel), Some(dec)) = (&self.dcs_kind, &mut self.sixel_decoder) {
            dec.feed_byte(byte);
        }
        // Unknown DCS: silently discard bytes.
    }

    fn unhook(&mut self) {
        if let (Some(DcsKind::Sixel), Some(mut img)) = (
            self.dcs_kind.take(),
            self.sixel_decoder.take().and_then(|d| d.finish()),
        ) {
            img.col = self.sixel_col;
            img.row = self.sixel_row;
            self.grid.images.push(img);
            // Cursor advance is intentionally omitted: the parser has no
            // access to FontMetrics. Real sixel producers (chafa, libsixel,
            // img2sixel) always send an explicit cursor-positioning escape
            // after the image.
        }
        self.sixel_decoder = None;
        self.dcs_kind = None;
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            // DEC Special Graphics (line drawing): ESC ( 0 = on, ESC ( B = ASCII
            (b"(", b'0') => self.grid.charset_drawing = true,
            (b"(", b'B') => self.grid.charset_drawing = false,
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
            ([], b'c') => {
                // RIS: Full Reset — clear all state as if freshly opened
                self.grid.reset();
            }
            _ => {}
        }
    }
}

fn char_delete_n(
    cells: &mut [super::grid::Cell],
    row: usize,
    col: usize,
    cols: usize,
    n: usize,
    blank: super::grid::Cell,
) {
    let n = n.min(cols - col);
    for c in col..cols {
        cells[row * cols + c] = if c + n < cols {
            cells[row * cols + c + n].clone()
        } else {
            blank.clone()
        };
    }
}

fn char_insert_n(
    cells: &mut [super::grid::Cell],
    row: usize,
    col: usize,
    cols: usize,
    n: usize,
    blank: super::grid::Cell,
) {
    let n = n.min(cols - col);
    for c in (col..cols).rev() {
        cells[row * cols + c] = if c >= col + n {
            cells[row * cols + c - n].clone()
        } else {
            blank.clone()
        };
    }
}

fn osc_set_title(grid: &mut Grid, params: &[&[u8]]) {
    if let [code, title] = params
        && matches!(*code, b"0" | b"1" | b"2")
        && let Ok(s) = std::str::from_utf8(title)
    {
        let t = s.trim();
        grid.osc_title = if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        };
    }
}

fn osc_set_cwd(grid: &mut Grid, params: &[&[u8]]) {
    if let [b"7", uri] = params
        && let Ok(s) = std::str::from_utf8(uri)
    {
        grid.cwd = parse_osc7_uri(s);
    }
}

fn osc_set_hyperlink(grid: &mut Grid, params: &[&[u8]]) {
    if let [osc, _, uri, ..] = params
        && *osc == b"8"
    {
        if uri.is_empty() {
            grid.current_url = None;
        } else if let Ok(s) = std::str::from_utf8(uri) {
            grid.current_url = Some(std::sync::Arc::new(s.to_string()));
        }
    }
}

fn osc_clipboard(grid: &mut Grid, params: &[&[u8]]) {
    if let [b"52", _sel, data] = params {
        if *data == b"?" {
            grid.pending_clipboard_read = true;
        } else if let Ok(decoded) = BASE64.decode(data)
            && let Ok(text) = std::str::from_utf8(&decoded)
        {
            grid.pending_clipboard_write = Some(text.to_string());
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

/// Parse an SGR 38/48 extended color from `ps[i..]`.
/// Returns `Some((color, skip))` where `skip` is the number of extra indices consumed.
fn parse_color_from_params(ps: &[u16], i: usize, palette: &[Color; 16]) -> Option<(Color, usize)> {
    let sub = ps.get(i + 1)?;
    match sub {
        5 => ps.get(i + 2).map(|&n| (color256(n as u8, palette), 2)),
        2 => {
            let r = *ps.get(i + 2)? as u8;
            let g = *ps.get(i + 3)? as u8;
            let b = *ps.get(i + 4)? as u8;
            Some((Color::rgb(r, g, b), 4))
        }
        _ => None,
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
