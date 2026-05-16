use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::source::SystemSource;
use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;
use std::ffi::CString;

use freetype::freetype as ft;

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct GlyphKey {
    pub c: char,
    pub px: u32,
    pub bold: bool,
    pub italic: bool,
}

/// Everything needed to place a glyph correctly in its cell.
pub struct GlyphInfo {
    /// Grayscale alpha (1 byte/pixel) when color=false; RGBA (4 bytes/pixel) when color=true.
    pub bitmap: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Pixels from baseline down to bottom of bitmap (negative = below baseline).
    pub ymin: i32,
    pub _advance: u32,
    /// True when bitmap is RGBA color (e.g. color emoji from FreeType); false for outline glyphs.
    pub color: bool,
}

/// Safe wrapper around a FreeType library + face pair for color emoji rendering.
struct ColorEmojiRenderer {
    library: ft::FT_Library,
    face: ft::FT_Face,
}

// GlyphCache lives on the render thread only; raw pointers are safe to send there.
unsafe impl Send for ColorEmojiRenderer {}

impl ColorEmojiRenderer {
    fn new(path: &std::path::Path) -> Option<Self> {
        unsafe {
            let mut library: ft::FT_Library = std::ptr::null_mut();
            if ft::FT_Init_FreeType(&mut library) != 0 {
                return None;
            }
            let path_cstr = CString::new(path.to_str()?).ok()?;
            let mut face: ft::FT_Face = std::ptr::null_mut();
            if ft::FT_New_Face(library, path_cstr.as_ptr(), 0, &mut face) != 0 {
                ft::FT_Done_FreeType(library);
                return None;
            }
            Some(Self { library, face })
        }
    }

    fn rasterize(&self, c: char, cell_px: u32) -> Option<GlyphInfo> {
        unsafe {
            // CBDT/CBLC fonts have fixed strikes; FT_Set_Pixel_Sizes returns empty bitmaps.
            // Select the only available strike (109ppem → 136×128 raw bitmap), then scale.
            if ft::FT_Select_Size(self.face, 0) != 0 {
                return None;
            }

            let flags = (ft::FT_LOAD_RENDER | ft::FT_LOAD_COLOR) as ft::FT_Int32;
            if ft::FT_Load_Char(self.face, c as ft::FT_ULong, flags) != 0 {
                return None;
            }

            let glyph = &*(*self.face).glyph;
            let bm = &glyph.bitmap;

            if bm.pixel_mode as u32 != ft::FT_Pixel_Mode_::FT_PIXEL_MODE_BGRA as u32 {
                return None;
            }

            let src_w = bm.width;
            let src_h = bm.rows;
            if src_w == 0 || src_h == 0 {
                return None;
            }

            let pitch = bm.pitch.unsigned_abs() as usize;
            let raw = std::slice::from_raw_parts(bm.buffer, src_h as usize * pitch);

            // Convert FreeType BGRA → RGBA
            let mut src_rgba = Vec::with_capacity((src_w * src_h * 4) as usize);
            for row in 0..src_h as usize {
                for col in 0..src_w as usize {
                    let base = row * pitch + col * 4;
                    src_rgba.push(raw[base + 2]); // R
                    src_rgba.push(raw[base + 1]); // G
                    src_rgba.push(raw[base]); // B
                    src_rgba.push(raw[base + 3]); // A
                }
            }

            // Scale native strike down to cell_px height.
            let scale = cell_px as f32 / src_h as f32;
            let dst_h = cell_px;
            let dst_w = ((src_w as f32 * scale).round() as u32).max(1);
            let scaled = scale_rgba_bilinear(&src_rgba, src_w, src_h, dst_w, dst_h);

            let bitmap_top = (glyph.bitmap_top as f32 * scale).round() as i32;
            let ymin = bitmap_top - dst_h as i32;

            Some(GlyphInfo {
                bitmap: scaled,
                width: dst_w,
                height: dst_h,
                ymin,
                _advance: dst_w,
                color: true,
            })
        }
    }
}

impl Drop for ColorEmojiRenderer {
    fn drop(&mut self) {
        unsafe {
            ft::FT_Done_Face(self.face);
            ft::FT_Done_FreeType(self.library);
        }
    }
}

pub struct GlyphCache {
    font: Font,
    bold_font: Font,
    italic_font: Font,
    bold_italic_font: Font,
    /// Outline fonts tried in order when the primary font lacks a glyph.
    fallbacks: Vec<Font>,
    /// FreeType renderer for Noto Color Emoji (CBDT/CBLC color bitmaps).
    ft_emoji: Option<ColorEmojiRenderer>,
    cache: HashMap<GlyphKey, GlyphInfo>,
}

impl GlyphCache {
    pub fn new(family: &str) -> Self {
        let font =
            load_system_font(family, false, false).unwrap_or_else(|| load_fallback(false, false));
        let bold_font =
            load_system_font(family, true, false).unwrap_or_else(|| load_fallback(true, false));
        let italic_font =
            load_system_font(family, false, true).unwrap_or_else(|| load_fallback(false, true));
        let bold_italic_font =
            load_system_font(family, true, true).unwrap_or_else(|| load_fallback(true, true));
        let fallbacks = load_fallback_fonts();
        let ft_emoji = load_color_emoji_renderer();
        Self {
            font,
            bold_font,
            italic_font,
            bold_italic_font,
            fallbacks,
            ft_emoji,
            cache: HashMap::new(),
        }
    }

    pub fn get(&mut self, c: char, px: f32, bold: bool, italic: bool) -> &GlyphInfo {
        let key = GlyphKey {
            c,
            px: px as u32,
            bold,
            italic,
        };
        if !self.cache.contains_key(&key) {
            let info = self.resolve_glyph(c, px, bold, italic);
            self.cache.insert(key.clone(), info);
        }
        self.cache.get(&key).unwrap()
    }

    /// Measure metrics for a character without caching the bitmap.
    pub fn metrics(&self, c: char, px: f32, bold: bool) -> Metrics {
        let font = if bold { &self.bold_font } else { &self.font };
        font.rasterize(c, px).0
    }

    // Keep old API for status bar rendering compatibility (always returns grayscale alpha).
    pub fn rasterize(&mut self, c: char, px: f32, bold: bool) -> (&[u8], u32, u32) {
        let info = self.get(c, px, bold, false);
        (info.bitmap.as_slice(), info.width, info.height)
    }

    fn resolve_glyph(&mut self, c: char, px: f32, bold: bool, italic: bool) -> GlyphInfo {
        use unicode_width::UnicodeWidthChar;

        let make_outline = |m: fontdue::Metrics, bitmap: Vec<u8>| GlyphInfo {
            width: m.width as u32,
            height: m.height as u32,
            ymin: m.ymin,
            _advance: m.advance_width.ceil() as u32,
            bitmap,
            color: false,
        };

        let is_wide = UnicodeWidthChar::width(c).unwrap_or(1) >= 2;

        // Wide chars (emoji, CJK) skip the primary font entirely: no monospace font
        // covers emoji well, and intercepting them here would block color rendering.
        if !is_wide {
            // 1. Primary font: covers ASCII and most monospace glyphs.
            let primary = match (bold, italic) {
                (true, true) => &self.bold_italic_font,
                (true, false) => &self.bold_font,
                (false, true) => &self.italic_font,
                (false, false) => &self.font,
            };
            if primary.has_glyph(c) {
                let (m, bitmap) = primary.rasterize(c, px);
                // Return even when bitmap is empty (e.g. space): empty = invisible, not missing.
                return make_outline(m, bitmap);
            }
        }

        // 2. FreeType color emoji — first for wide chars, fallback for narrow ones.
        if let Some(renderer) = &self.ft_emoji
            && let Some(info) = renderer.rasterize(c, px as u32)
        {
            return info;
        }

        // 3. Outline fallback chain for symbols, CJK, box-drawing, etc.
        for fb in &self.fallbacks {
            if fb.has_glyph(c) {
                let (m, bitmap) = fb.rasterize(c, px);
                if !bitmap.is_empty() {
                    return make_outline(m, bitmap);
                }
            }
        }

        // 4. Tofu box □ so the cell is visibly non-empty.
        let primary = if bold { &self.bold_font } else { &self.font };
        let (m, bm) = primary.rasterize('\u{25A1}', px);
        make_outline(m, bm)
    }
}

fn load_color_emoji_renderer() -> Option<ColorEmojiRenderer> {
    let source = SystemSource::new();
    let names = &[FamilyName::Title("Noto Color Emoji".to_string())];
    let handle = source.select_best_match(names, &Properties::new()).ok()?;
    let path = match handle {
        Handle::Path { path, .. } => path,
        _ => return None,
    };
    let renderer = ColorEmojiRenderer::new(&path)?;
    log::info!("Loaded color emoji renderer: {}", path.display());
    Some(renderer)
}

fn load_system_font(family: &str, bold: bool, italic: bool) -> Option<Font> {
    let source = SystemSource::new();
    let mut props = Properties::new();
    props.weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    props.style = if italic { Style::Italic } else { Style::Normal };

    let handle = source
        .select_best_match(
            &[FamilyName::Title(family.to_string()), FamilyName::Monospace],
            &props,
        )
        .ok()?;

    // font_kit returns the closest match even when the exact style is unavailable.
    // Verify the selected file is actually italic by inspecting its path; if not,
    // return None so the caller falls back to the bundled italic font.
    if italic {
        if let Handle::Path { ref path, .. } = handle {
            let name = path.to_string_lossy().to_lowercase();
            if !name.contains("italic") && !name.contains("oblique") {
                return None;
            }
        }
    }

    let bytes = font_bytes(handle)?;
    let font = Font::from_bytes(bytes.as_slice(), FontSettings::default()).ok()?;
    log::info!(
        "Loaded {}{} font: {}",
        if bold { "bold" } else { "regular" },
        if italic { " italic" } else { "" },
        family
    );
    Some(font)
}

fn font_bytes(handle: Handle) -> Option<Vec<u8>> {
    match handle {
        Handle::Path { path, .. } => std::fs::read(&path).ok(),
        Handle::Memory { bytes, .. } => Some(bytes.to_vec()),
    }
}

fn load_fallback(bold: bool, italic: bool) -> Font {
    let data: &[u8] = match (bold, italic) {
        (true, true) => include_bytes!("../../assets/JetBrainsMono-BoldItalic.ttf"),
        (true, false) => include_bytes!("../../assets/JetBrainsMono-Bold.ttf"),
        (false, true) => include_bytes!("../../assets/JetBrainsMono-Italic.ttf"),
        (false, false) => include_bytes!("../../assets/JetBrainsMono-Regular.ttf"),
    };
    Font::from_bytes(data, FontSettings::default()).expect("embedded fallback font failed")
}

/// Bilinear downscale for RGBA bitmaps. Works in straight-alpha space.
fn scale_rgba_bilinear(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let mut dst = vec![0u8; (dst_w * dst_h * 4) as usize];
    let x_ratio = src_w as f32 / dst_w as f32;
    let y_ratio = src_h as f32 / dst_h as f32;

    for dy in 0..dst_h {
        for dx in 0..dst_w {
            let sx = (dx as f32 + 0.5) * x_ratio - 0.5;
            let sy = (dy as f32 + 0.5) * y_ratio - 0.5;
            let x0 = (sx.floor() as i32).clamp(0, src_w as i32 - 1) as u32;
            let y0 = (sy.floor() as i32).clamp(0, src_h as i32 - 1) as u32;
            let x1 = (x0 + 1).min(src_w - 1);
            let y1 = (y0 + 1).min(src_h - 1);
            let fx = (sx - sx.floor()).clamp(0.0, 1.0);
            let fy = (sy - sy.floor()).clamp(0.0, 1.0);

            let get = |x: u32, y: u32, ch: usize| -> f32 {
                src[((y * src_w + x) * 4) as usize + ch] as f32
            };
            let dst_idx = ((dy * dst_w + dx) * 4) as usize;
            for ch in 0..4usize {
                let v = (1.0 - fy) * ((1.0 - fx) * get(x0, y0, ch) + fx * get(x1, y0, ch))
                    + fy * ((1.0 - fx) * get(x0, y1, ch) + fx * get(x1, y1, ch));
                dst[dst_idx + ch] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    dst
}

/// Outline system fonts tried in order when the primary font lacks a glyph.
fn load_fallback_fonts() -> Vec<Font> {
    let source = SystemSource::new();
    let props = Properties::new();
    let families = [
        "Noto Sans Symbols2", // fc-list: "Noto Sans Symbols2" (no space before 2)
        "Noto Sans Symbols",
        "DejaVu Sans",
        "Noto Sans",
    ];
    let mut fonts = Vec::new();
    for family in &families {
        let names = &[FamilyName::Title(family.to_string())];
        if let Ok(handle) = source.select_best_match(names, &props)
            && let Some(bytes) = font_bytes(handle)
            && let Ok(font) = Font::from_bytes(bytes.as_slice(), FontSettings::default())
        {
            log::info!("Loaded glyph fallback font: {}", family);
            fonts.push(font);
        }
    }
    fonts
}

#[cfg(test)]
#[path = "glyph_test.rs"]
mod tests;
