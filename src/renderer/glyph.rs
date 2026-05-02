use fontdue::{Font, FontSettings};
use std::collections::HashMap;

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct GlyphKey {
    pub c: char,
    pub px: u32,
    pub bold: bool,
}

pub struct GlyphCache {
    font: Font,
    bold_font: Font,
    cache: HashMap<GlyphKey, (Vec<u8>, u32, u32)>,
}

impl GlyphCache {
    pub fn new() -> Self {
        let font_data = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");
        let bold_data = include_bytes!("../../assets/JetBrainsMono-Bold.ttf");
        let font = Font::from_bytes(font_data as &[u8], FontSettings::default())
            .expect("Failed to load regular font");
        let bold_font = Font::from_bytes(bold_data as &[u8], FontSettings::default())
            .expect("Failed to load bold font");
        Self { font, bold_font, cache: HashMap::new() }
    }

    /// Returns (bitmap, width, height) for a glyph.
    pub fn rasterize(&mut self, c: char, px: f32, bold: bool) -> (&[u8], u32, u32) {
        let key = GlyphKey { c, px: px as u32, bold };
        if !self.cache.contains_key(&key) {
            let font = if bold { &self.bold_font } else { &self.font };
            let (metrics, bitmap) = font.rasterize(c, px);
            self.cache.insert(key.clone(), (bitmap, metrics.width as u32, metrics.height as u32));
        }
        let (bmp, w, h) = self.cache.get(&key).unwrap();
        (bmp.as_slice(), *w, *h)
    }
}
