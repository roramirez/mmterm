use super::bracketed_paste_encode;

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

#[test]
fn bracketed_paste_wraps_text_in_markers() {
    let out = bracketed_paste_encode("hi", true);
    let mut expected = Vec::new();
    expected.extend_from_slice(PASTE_START);
    expected.extend_from_slice(b"hi");
    expected.extend_from_slice(PASTE_END);
    assert_eq!(out, expected);
}

#[test]
fn non_bracketed_paste_passes_text_through_unchanged() {
    let out = bracketed_paste_encode("hi", false);
    assert_eq!(out, b"hi");
}

#[test]
fn bracketed_paste_empty_text_still_emits_markers() {
    // An empty paste in bracketed mode is start+end with nothing between, so a
    // program in bracketed-paste mode still sees a (zero-length) paste event.
    let out = bracketed_paste_encode("", true);
    let mut expected = Vec::new();
    expected.extend_from_slice(PASTE_START);
    expected.extend_from_slice(PASTE_END);
    assert_eq!(out, expected);
}

#[test]
fn non_bracketed_empty_text_is_empty() {
    assert!(bracketed_paste_encode("", false).is_empty());
}

#[test]
fn bracketed_paste_preserves_inner_bytes_including_newlines() {
    let out = bracketed_paste_encode("a\nb", true);
    // The payload between the markers must be byte-for-byte the original text.
    assert_eq!(
        &out[PASTE_START.len()..out.len() - PASTE_END.len()],
        b"a\nb"
    );
}
