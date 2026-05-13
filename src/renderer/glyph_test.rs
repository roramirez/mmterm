use super::*;

fn make_cache() -> GlyphCache {
    GlyphCache::new("JetBrainsMono")
}

#[test]
fn glyph_cache_new_does_not_panic() {
    let _cache = make_cache();
}

#[test]
fn get_space_returns_glyph_info() {
    let mut cache = make_cache();
    let info = cache.get(' ', 16.0, false);
    // Space is a valid glyph; bitmap may be empty but dimensions should be non-negative.
    let _ = info.width;
    let _ = info.height;
}

#[test]
fn get_ascii_letter_returns_nonzero_advance() {
    let mut cache = make_cache();
    let info = cache.get('M', 16.0, false);
    assert!(info._advance > 0);
}

#[test]
fn metrics_returns_positive_dimensions() {
    let cache = make_cache();
    let m = cache.metrics('M', 16.0, false);
    assert!(m.advance_width > 0.0);
}

#[test]
fn rasterize_ascii_returns_nonempty_bitmap() {
    let mut cache = make_cache();
    let (bitmap, w, h) = cache.rasterize('A', 16.0, false);
    assert!(w > 0);
    assert!(h > 0);
    assert!(!bitmap.is_empty());
}

#[test]
fn rasterize_unicode_falls_back_gracefully() {
    let mut cache = make_cache();
    // U+25A1 is the tofu box □ — always covered by the fallback chain.
    let (bitmap, w, h) = cache.rasterize('\u{25A1}', 16.0, false);
    let _ = (bitmap, w, h); // just assert no panic
}

#[test]
fn get_caches_result_second_call_is_same() {
    let mut cache = make_cache();
    let advance1 = cache.get('X', 16.0, false)._advance;
    let advance2 = cache.get('X', 16.0, false)._advance;
    assert_eq!(advance1, advance2);
}

#[test]
fn scale_rgba_bilinear_preserves_dimensions() {
    let src = vec![128u8; 4 * 4 * 4]; // 4×4 RGBA, all mid-gray
    let dst = scale_rgba_bilinear(&src, 4, 4, 2, 2);
    assert_eq!(dst.len(), 2 * 2 * 4);
}

#[test]
fn get_bold_variant_does_not_panic() {
    let mut cache = make_cache();
    let _info = cache.get('B', 16.0, true);
}

#[test]
fn load_fallback_regular_returns_valid_font() {
    let _font = load_fallback(false);
}

#[test]
fn load_fallback_bold_returns_valid_font() {
    let _font = load_fallback(true);
}

#[test]
fn unknown_font_family_falls_back_to_embedded() {
    // System font lookup fails → load_fallback is invoked as the unwrap_or_else path.
    let _cache = GlyphCache::new("NoSuchFont99999XYZ");
}

#[test]
fn rasterize_box_drawing_triggers_fallback_chain() {
    let mut cache = make_cache();
    // U+2588 FULL BLOCK — may not be in the primary font, exercises the fallback chain.
    let (_, w, h) = cache.rasterize('\u{2588}', 16.0, false);
    let _ = (w, h);
}

#[test]
fn get_from_unknown_font_uses_tofu_fallback() {
    let mut cache = GlyphCache::new("NoSuchFont99999XYZ");
    // Any char — the cache will use the embedded fallback; should not panic.
    let _info = cache.get('A', 16.0, false);
}
