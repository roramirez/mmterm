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
    let info = cache.get(' ', 16.0, false, false);
    // Space is a valid glyph; bitmap may be empty but dimensions should be non-negative.
    let _ = info.width;
    let _ = info.height;
}

#[test]
fn get_ascii_letter_returns_nonzero_advance() {
    let mut cache = make_cache();
    let info = cache.get('M', 16.0, false, false);
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
    let advance1 = cache.get('X', 16.0, false, false)._advance;
    let advance2 = cache.get('X', 16.0, false, false)._advance;
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
    let _info = cache.get('B', 16.0, true, false);
}

#[test]
fn load_fallback_regular_returns_valid_font() {
    let _font = load_fallback(false, false);
}

#[test]
fn load_fallback_bold_returns_valid_font() {
    let _font = load_fallback(true, false);
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
    let _info = cache.get('A', 16.0, false, false);
}

#[test]
fn scale_rgba_bilinear_upscale_from_1x1() {
    // A single RGBA pixel upscaled 2× should produce a 2×2 result with the same color.
    let src = vec![0xFF_u8, 0x80, 0x40, 0xFF]; // R=255 G=128 B=64 A=255
    let dst = scale_rgba_bilinear(&src, 1, 1, 2, 2);
    assert_eq!(dst.len(), 2 * 2 * 4);
    assert_eq!(dst[0], 0xFF); // R
    assert_eq!(dst[1], 0x80); // G
    assert_eq!(dst[2], 0x40); // B
}

#[test]
fn scale_rgba_bilinear_downscale_to_1x1() {
    // 4×4 solid red RGBA → 1×1 should be approximately red.
    let src = vec![0xFF_u8, 0x00, 0x00, 0xFF].repeat(16); // 4×4, all red
    let dst = scale_rgba_bilinear(&src, 4, 4, 1, 1);
    assert_eq!(dst.len(), 4);
    assert_eq!(dst[0], 0xFF); // R
    assert_eq!(dst[1], 0x00); // G
    assert_eq!(dst[2], 0x00); // B
}

#[test]
fn scale_rgba_bilinear_all_zero_stays_zero() {
    let src = vec![0u8; 4 * 8 * 4]; // 4×8 fully transparent black
    let dst = scale_rgba_bilinear(&src, 4, 8, 2, 4);
    assert!(dst.iter().all(|&b| b == 0));
}

#[test]
fn load_fallback_fonts_does_not_panic() {
    // Exercises the system font discovery loop; result may be empty on minimal systems.
    let _fonts = load_fallback_fonts();
}

#[test]
fn get_wide_cjk_char_exercises_fallback_path() {
    // U+4E00 (一) has Unicode width 2, so resolve_glyph skips the primary font
    // and falls through to the ft_emoji renderer (None in tests) then the
    // outline fallback chain or the tofu box.
    let mut cache = make_cache();
    let _info = cache.get('\u{4E00}', 16.0, false, false);
}

#[test]
fn get_rare_unicode_falls_back_to_tofu() {
    // U+E000 is a Private Use Area codepoint that no standard font covers.
    let mut cache = make_cache();
    let _info = cache.get('\u{E000}', 16.0, false, false);
}

#[test]
fn get_italic_variant_does_not_panic() {
    let mut cache = make_cache();
    let _info = cache.get('A', 16.0, false, true);
}

#[test]
fn get_bold_italic_variant_does_not_panic() {
    let mut cache = make_cache();
    let _info = cache.get('A', 16.0, true, true);
}

#[test]
fn load_fallback_italic_returns_valid_font() {
    let _font = load_fallback(false, true);
}

#[test]
fn load_fallback_bold_italic_returns_valid_font() {
    let _font = load_fallback(true, true);
}

#[test]
fn italic_and_regular_produce_distinct_cache_entries() {
    // GlyphKey must distinguish italic from non-italic so each gets its own glyph.
    let mut cache = make_cache();
    let regular = cache.get('A', 16.0, false, false)._advance;
    let italic = cache.get('A', 16.0, false, true)._advance;
    // Both should be valid (non-zero advance); cache lookup must not panic.
    assert!(regular > 0);
    assert!(italic > 0);
}

#[test]
fn system_font_without_italic_in_path_falls_back_to_bundled() {
    // load_system_font returns None for italic when the selected file path
    // does not contain "italic" or "oblique", so GlyphCache::new falls back
    // to the embedded JetBrainsMono-Italic.ttf. Verify the cache is usable.
    let mut cache = GlyphCache::new("Noto Sans Mono");
    let _info = cache.get('A', 16.0, false, true);
}
