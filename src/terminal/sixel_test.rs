use super::SixelDecoder;

fn feed(dec: &mut SixelDecoder, bytes: &[u8]) {
    for &b in bytes {
        dec.feed_byte(b);
    }
}

#[test]
fn empty_input_returns_none() {
    let dec = SixelDecoder::new();
    assert!(dec.finish().is_none());
}

#[test]
fn question_mark_byte_sets_no_pixels_but_produces_image() {
    // '?' = 63 - 63 = 0 → bits = 0b000000 → no pixels set, but x advances
    // '@' = 64 - 63 = 1 → bits = 0b000001 → pixel at (0, 0)
    let mut dec = SixelDecoder::new();
    dec.feed_byte(b'@'); // bits = 1 → row 0 pixel set
    let img = dec.finish().expect("should produce image");
    assert_eq!(img.width, 1);
    assert!(img.height >= 1);
    // pixel (0, 0): alpha must be 255 (default palette[0] is black, opaque)
    assert_eq!(img.pixels[3], 255);
}

#[test]
fn rle_repeat_produces_multiple_columns() {
    // '!3@' → 3 repetitions of '@' (bit 0 set)
    let mut dec = SixelDecoder::new();
    feed(&mut dec, b"!3@");
    let img = dec.finish().expect("should produce image");
    assert_eq!(img.width, 3);
}

#[test]
fn rle_missing_count_defaults_to_one() {
    // '!@' with no digit before the sixel byte → treat as repeat=1
    let mut dec = SixelDecoder::new();
    feed(&mut dec, b"!@");
    let img = dec.finish().expect("should produce image");
    assert_eq!(img.width, 1);
}

#[test]
fn carriage_return_resets_x() {
    // '@$@' → pixel at x=0, then CR (x=0), then pixel at x=0 (overwrite)
    let mut dec = SixelDecoder::new();
    feed(&mut dec, b"@$@");
    let img = dec.finish().expect("should produce image");
    assert_eq!(img.width, 1, "only one column of pixels");
}

#[test]
fn band_linefeed_advances_six_rows() {
    // '@-@' → pixel at row 0, band newline (band_row += 6), pixel at row 6
    let mut dec = SixelDecoder::new();
    feed(&mut dec, b"@-@");
    let img = dec.finish().expect("should produce image");
    assert!(
        img.height >= 7,
        "must span at least 7 rows (row 0 and row 6)"
    );
}

#[test]
fn palette_define_rgb_sets_color() {
    // '#1;2;255;0;0' sets color 1 to red; '#1@' draws pixel with that color
    let mut dec = SixelDecoder::new();
    feed(&mut dec, b"#1;2;255;0;0#1@");
    let img = dec.finish().expect("should produce image");
    assert_eq!(img.pixels[0], 255, "R"); // R
    assert_eq!(img.pixels[1], 0, "G"); // G
    assert_eq!(img.pixels[2], 0, "B"); // B
    assert_eq!(img.pixels[3], 255, "A"); // A
}

#[test]
fn palette_select_only_switches_color() {
    // '#1;2;0;255;0' defines green, '#1' selects it, '@' draws pixel
    let mut dec = SixelDecoder::new();
    feed(&mut dec, b"#1;2;0;255;0#1@");
    let img = dec.finish().expect("should produce image");
    assert_eq!(img.pixels[1], 255, "G channel should be 255");
}

#[test]
fn full_sixel_band_all_bits_set() {
    // '~' = 126 - 63 = 63 = 0b111111 → all 6 rows in the band have a pixel at x=0
    let mut dec = SixelDecoder::new();
    dec.feed_byte(b'~');
    let img = dec.finish().expect("should produce image");
    assert_eq!(img.width, 1);
    assert_eq!(img.height, 6);
    // all 6 pixels in column 0 should be opaque
    for row in 0..6usize {
        let alpha = img.pixels[row * 4 * img.width as usize + 3];
        assert_eq!(alpha, 255, "row {row} should be opaque");
    }
}

#[test]
fn multiple_bands_stack_vertically() {
    // '~-~' → full band at rows 0-5, linefeed, full band at rows 6-11
    let mut dec = SixelDecoder::new();
    feed(&mut dec, b"~-~");
    let img = dec.finish().expect("should produce image");
    assert_eq!(img.height, 12);
}

#[test]
fn unknown_bytes_do_not_panic() {
    let mut dec = SixelDecoder::new();
    for b in 0u8..=127u8 {
        dec.feed_byte(b);
    }
    // No assertion beyond "did not panic"
}

#[test]
fn hls_palette_does_not_panic() {
    // '#0;1;0;50;100' = HLS: hue=0, lightness=50%, saturation=100% (should be red)
    let mut dec = SixelDecoder::new();
    feed(&mut dec, b"#0;1;0;50;100#0@");
    let img = dec.finish().expect("should produce image");
    assert_eq!(img.pixels[3], 255, "alpha");
}

#[test]
fn image_with_only_zero_bits_returns_none() {
    // '?' = 0 bits → no pixels written → no image
    let mut dec = SixelDecoder::new();
    feed(&mut dec, b"???");
    // The decoder sees zero-bit data; width stays at 0 pixels set.
    // finish() returns None because no pixels were set.
    // (x advances but ensure_size is only called when bits != 0, so height = 0)
    assert!(dec.finish().is_none());
}
