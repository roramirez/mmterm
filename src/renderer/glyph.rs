use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::source::SystemSource;
use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct GlyphKey {
    pub c: char,
    pub px: u32,
    pub bold: bool,
}

/// Everything needed to place a glyph correctly in its cell.
pub struct GlyphInfo {
    pub bitmap: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Pixels from baseline down to bottom of bitmap (negative = below baseline)
    pub ymin: i32,
    pub _advance: u32,
}

pub struct GlyphCache {
    font: Font,
    bold_font: Font,
    /// Outline fonts tried in order when the primary font lacks a glyph.
    fallbacks: Vec<Font>,
    cache: HashMap<GlyphKey, GlyphInfo>,
}

impl GlyphCache {
    pub fn new(family: &str) -> Self {
        let font = load_system_font(family, false).unwrap_or_else(|| load_fallback(false));
        let bold_font = load_system_font(family, true).unwrap_or_else(|| load_fallback(true));
        let fallbacks = load_fallback_fonts();
        Self { font, bold_font, fallbacks, cache: HashMap::new() }
    }

    pub fn get(&mut self, c: char, px: f32, bold: bool) -> &GlyphInfo {
        let key = GlyphKey { c, px: px as u32, bold };
        if !self.cache.contains_key(&key) {
            // Pick primary font; if it lacks the glyph try the fallback chain.
            let (m, bitmap) = {
                let primary = if bold { &self.bold_font } else { &self.font };
                if primary.has_glyph(c) {
                    primary.rasterize(c, px)
                } else {
                    let mut found = None;
                    for fb in &self.fallbacks {
                        if fb.has_glyph(c) {
                            found = Some(fb.rasterize(c, px));
                            break;
                        }
                    }
                    found.unwrap_or_else(|| primary.rasterize(c, px))
                }
            };
            self.cache.insert(key.clone(), GlyphInfo {
                bitmap,
                width:   m.width as u32,
                height:  m.height as u32,
                ymin:    m.ymin,
                _advance: m.advance_width.ceil() as u32,
            });
        }
        self.cache.get(&key).unwrap()
    }

    /// Measure metrics for a character without caching the bitmap.
    pub fn metrics(&self, c: char, px: f32, bold: bool) -> Metrics {
        let font = if bold { &self.bold_font } else { &self.font };
        font.rasterize(c, px).0
    }

    // Keep old API for status bar rendering compatibility
    pub fn rasterize(&mut self, c: char, px: f32, bold: bool) -> (&[u8], u32, u32) {
        let info = self.get(c, px, bold);
        (info.bitmap.as_slice(), info.width, info.height)
    }
}

fn load_system_font(family: &str, bold: bool) -> Option<Font> {
    let source = SystemSource::new();
    let mut props = Properties::new();
    props.weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    props.style = Style::Normal;

    let handle = source.select_best_match(
        &[FamilyName::Title(family.to_string()), FamilyName::Monospace],
        &props,
    ).ok()?;
    let bytes = font_bytes(handle)?;
    let font = Font::from_bytes(bytes.as_slice(), FontSettings::default()).ok()?;
    log::info!("Loaded {} font: {}", if bold { "bold" } else { "regular" }, family);
    Some(font)
}

fn font_bytes(handle: Handle) -> Option<Vec<u8>> {
    match handle {
        Handle::Path { path, .. } => std::fs::read(&path).ok(),
        Handle::Memory { bytes, .. } => Some(bytes.to_vec()),
    }
}

fn load_fallback(bold: bool) -> Font {
    let data: &[u8] = if bold {
        include_bytes!("../../assets/JetBrainsMono-Bold.ttf")
    } else {
        include_bytes!("../../assets/JetBrainsMono-Regular.ttf")
    };
    Font::from_bytes(data, FontSettings::default()).expect("embedded fallback font failed")
}

/// Load outline system fonts used as glyph fallbacks (symbols, emoji outlines, CJK, etc.).
fn load_fallback_fonts() -> Vec<Font> {
    let source = SystemSource::new();
    let props = Properties::new();
    let families = [
        "Noto Sans Symbols 2",
        "Noto Sans Symbols",
        "DejaVu Sans",
        "Noto Sans",
    ];
    let mut fonts = Vec::new();
    for family in &families {
        let names = &[FamilyName::Title(family.to_string())];
        if let Ok(handle) = source.select_best_match(names, &props) {
            if let Some(bytes) = font_bytes(handle) {
                if let Ok(font) = Font::from_bytes(bytes.as_slice(), FontSettings::default()) {
                    log::info!("Loaded glyph fallback font: {}", family);
                    fonts.push(font);
                }
            }
        }
    }
    fonts
}
