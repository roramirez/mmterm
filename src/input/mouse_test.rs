use super::*;

// ── SGR encoding ─────────────────────────────────────────────────────────────

#[test]
fn sgr_press_produces_capital_m() {
    let bytes = encode_mouse_event(0, 0, 0, false, true);
    assert!(bytes.ends_with(b"M"));
    assert!(bytes.starts_with(b"\x1b[<"));
}

#[test]
fn sgr_release_produces_lowercase_m() {
    let bytes = encode_mouse_event(0, 0, 0, true, true);
    assert!(bytes.ends_with(b"m"));
}

#[test]
fn sgr_encodes_1_indexed_col_and_row() {
    // col=2, row=3 → ";3;4M"
    let s = String::from_utf8(encode_mouse_event(0, 2, 3, false, true)).unwrap();
    assert!(s.contains(";3;4M"), "got: {s}");
}

#[test]
fn sgr_encodes_button() {
    // btn=2 → "\x1b[<2;..."
    let s = String::from_utf8(encode_mouse_event(2, 0, 0, false, true)).unwrap();
    assert!(s.starts_with("\x1b[<2;"), "got: {s}");
}

// ── X10 encoding ─────────────────────────────────────────────────────────────

#[test]
fn x10_starts_with_csi_m() {
    let bytes = encode_mouse_event(0, 0, 0, false, false);
    assert_eq!(&bytes[..3], b"\x1b[M");
    assert_eq!(bytes.len(), 6);
}

#[test]
fn x10_button_offset_by_32() {
    let bytes = encode_mouse_event(1, 0, 0, false, false);
    assert_eq!(bytes[3], 1 + 32);
}

#[test]
fn x10_col_offset_by_33() {
    // col=0 → byte = 0 + 1 + 32 = 33
    let bytes = encode_mouse_event(0, 0, 0, false, false);
    assert_eq!(bytes[4], 33);
}

#[test]
fn x10_row_offset_by_33() {
    let bytes = encode_mouse_event(0, 0, 0, false, false);
    assert_eq!(bytes[5], 33);
}

#[test]
fn x10_col_and_row_incremented() {
    // col=2, row=3 → col_byte = 2+1+32=35, row_byte = 3+1+32=36
    let bytes = encode_mouse_event(0, 2, 3, false, false);
    assert_eq!(bytes[4], 35);
    assert_eq!(bytes[5], 36);
}

#[test]
fn x10_release_ignored_for_encoding() {
    // X10 doesn't encode release differently — same byte sequence.
    let press = encode_mouse_event(0, 0, 0, false, false);
    let release = encode_mouse_event(0, 0, 0, true, false);
    assert_eq!(press, release);
}
