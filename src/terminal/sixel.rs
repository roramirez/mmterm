/// Maximum image dimension (pixels) in each axis to bound memory use.
const MAX_DIM: u32 = 4096;

/// Decoded sixel image anchored to a grid cell position.
#[derive(Clone, Debug)]
pub struct SixelImage {
    /// Grid column at which the image was emitted (cursor_col at DCS hook time).
    pub col: usize,
    /// Grid row at which the image was emitted (cursor_row at DCS hook time).
    pub row: usize,
    pub width: u32,
    pub height: u32,
    /// RGBA pixel data, row-major, 4 bytes per pixel.
    pub pixels: Vec<u8>,
}

/// 256-entry ARGB palette with DEC VT300 default colors.
struct Palette([u32; 256]);

impl Palette {
    fn new() -> Self {
        let mut p = [0xFF_00_00_00u32; 256]; // default: opaque black
        // DEC VT300 default palette entries 0-7 (RGB values).
        let defaults: [(u8, u8, u8); 8] = [
            (0, 0, 0),       // 0: black
            (0, 0, 255),     // 1: blue
            (255, 0, 0),     // 2: red
            (0, 255, 0),     // 3: green
            (255, 0, 255),   // 4: magenta
            (0, 255, 255),   // 5: cyan
            (255, 255, 0),   // 6: yellow
            (255, 255, 255), // 7: white
        ];
        for (i, (r, g, b)) in defaults.iter().enumerate() {
            p[i] = 0xFF_00_00_00 | ((*r as u32) << 16) | ((*g as u32) << 8) | (*b as u32);
        }
        Self(p)
    }

    fn set_rgb(&mut self, n: usize, r: u8, g: u8, b: u8) {
        if n < 256 {
            self.0[n] = 0xFF_00_00_00 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        }
    }

    /// Convert HLS (hue 0-360, lightness 0-100, saturation 0-100) to RGB.
    fn set_hls(&mut self, n: usize, h: u16, l: u16, s: u16) {
        if n >= 256 {
            return;
        }
        let l = l as f32 / 100.0;
        let s = s as f32 / 100.0;
        if s == 0.0 {
            let v = (l * 255.0) as u8;
            self.set_rgb(n, v, v, v);
            return;
        }
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        let h = h as f32 / 360.0;
        let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
        let g = hue_to_rgb(p, q, h);
        let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
        self.set_rgb(n, (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
    }

    fn get_rgba(&self, n: usize) -> (u8, u8, u8, u8) {
        let v = self.0[n.min(255)];
        (
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
            ((v >> 24) & 0xFF) as u8,
        )
    }
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

#[derive(Debug, PartialEq)]
enum DecoderState {
    Normal,
    ColorIntro,
    Rle,
}

/// Streaming sixel decoder.  Feed bytes via [`SixelDecoder::feed_byte`] then
/// call [`SixelDecoder::finish`] to obtain the decoded image.
pub struct SixelDecoder {
    palette: Palette,
    current_color: usize,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    x: u32,
    band_row: u32,
    state: DecoderState,
    param_buf: Vec<u8>,
    truncated: bool,
}

impl Default for SixelDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl SixelDecoder {
    pub fn new() -> Self {
        Self {
            palette: Palette::new(),
            current_color: 0,
            pixels: Vec::new(),
            width: 0,
            height: 0,
            x: 0,
            band_row: 0,
            state: DecoderState::Normal,
            param_buf: Vec::new(),
            truncated: false,
        }
    }

    pub fn feed_byte(&mut self, byte: u8) {
        match self.state {
            DecoderState::Normal => self.handle_normal(byte),
            DecoderState::ColorIntro => self.handle_color_intro(byte),
            DecoderState::Rle => self.handle_rle(byte),
        }
    }

    fn handle_normal(&mut self, byte: u8) {
        match byte {
            b'#' => {
                self.param_buf.clear();
                self.state = DecoderState::ColorIntro;
            }
            b'!' => {
                self.param_buf.clear();
                self.state = DecoderState::Rle;
            }
            b'$' => {
                self.x = 0;
            }
            b'-' => {
                self.band_row += 6;
                self.x = 0;
            }
            b'?'..=b'~' => {
                let bits = byte - b'?';
                self.plot_sixel(bits, 1);
            }
            _ => {}
        }
    }

    fn handle_color_intro(&mut self, byte: u8) {
        match byte {
            b'0'..=b'9' | b';' => {
                self.param_buf.push(byte);
            }
            _ => {
                self.apply_color_params();
                self.state = DecoderState::Normal;
                self.handle_normal(byte);
            }
        }
    }

    fn handle_rle(&mut self, byte: u8) {
        match byte {
            b'0'..=b'9' => {
                self.param_buf.push(byte);
            }
            b'?'..=b'~' => {
                let count = parse_u32(&self.param_buf).max(1);
                let bits = byte - b'?';
                self.plot_sixel(bits, count);
                self.state = DecoderState::Normal;
            }
            _ => {
                self.state = DecoderState::Normal;
            }
        }
    }

    fn apply_color_params(&mut self) {
        let params = parse_params(&self.param_buf);
        if params.is_empty() {
            return;
        }
        let n = params[0] as usize % 256;
        self.current_color = n;
        if params.len() >= 5 {
            match params[1] {
                1 => self
                    .palette
                    .set_hls(n, params[2] as u16, params[3] as u16, params[4] as u16),
                2 => self
                    .palette
                    .set_rgb(n, params[2] as u8, params[3] as u8, params[4] as u8),
                _ => {}
            }
        }
    }

    fn plot_sixel(&mut self, bits: u8, repeat: u32) {
        if bits == 0 {
            // No pixels set; advance x without allocating buffer space.
            self.x = self.x.saturating_add(repeat);
            return;
        }
        let (r, g, b, a) = self.palette.get_rgba(self.current_color);
        for _ in 0..repeat {
            if self.truncated {
                break;
            }
            let px = self.x;
            // Ensure the buffer covers this band's 6 rows.
            if !self.ensure_size(px + 1, self.band_row + 6) {
                break;
            }
            for bit_idx in 0..6u32 {
                if bits & (1 << bit_idx) != 0 {
                    let py = self.band_row + bit_idx;
                    if py < self.height && px < self.width {
                        let base = ((py * self.width + px) * 4) as usize;
                        self.pixels[base] = r;
                        self.pixels[base + 1] = g;
                        self.pixels[base + 2] = b;
                        self.pixels[base + 3] = a;
                    }
                }
            }
            self.x += 1;
        }
    }

    /// Grow the pixel buffer to accommodate at least `w` × `h` pixels.
    /// Returns false (and sets `truncated`) if the requested size exceeds MAX_DIM.
    fn ensure_size(&mut self, w: u32, h: u32) -> bool {
        if w > MAX_DIM || h > MAX_DIM {
            self.truncated = true;
            return false;
        }
        if w <= self.width && h <= self.height {
            return true;
        }
        let new_w = w.max(self.width);
        let new_h = h.max(self.height);
        let new_len = (new_w * new_h * 4) as usize;
        let mut new_buf = vec![0u8; new_len];
        // Copy existing rows into the wider buffer.
        for row in 0..self.height {
            let src_start = (row * self.width * 4) as usize;
            let src_end = src_start + (self.width * 4) as usize;
            if src_end <= self.pixels.len() {
                let dst_start = (row * new_w * 4) as usize;
                new_buf[dst_start..dst_start + (self.width * 4) as usize]
                    .copy_from_slice(&self.pixels[src_start..src_end]);
            }
        }
        self.pixels = new_buf;
        self.width = new_w;
        self.height = new_h;
        true
    }

    /// Finalise decoding and return the image.  Returns `None` if no pixels
    /// were decoded (e.g. the DCS sequence contained no sixel data bytes).
    pub fn finish(self) -> Option<SixelImage> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        Some(SixelImage {
            col: 0,
            row: 0,
            width: self.width,
            height: self.height,
            pixels: self.pixels,
        })
    }
}

/// Parse a semicolon-separated list of ASCII decimal integers.
fn parse_params(buf: &[u8]) -> Vec<u32> {
    buf.split(|&b| b == b';')
        .filter(|s| !s.is_empty())
        .map(parse_u32)
        .collect()
}

fn parse_u32(buf: &[u8]) -> u32 {
    buf.iter().fold(0u32, |acc, &b| {
        if b.is_ascii_digit() {
            acc.saturating_mul(10).saturating_add((b - b'0') as u32)
        } else {
            acc
        }
    })
}

#[cfg(test)]
#[path = "sixel_test.rs"]
mod tests;
