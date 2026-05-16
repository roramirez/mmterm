/// Encodes a mouse event as bytes to send to the PTY.
///
/// `sgr=true`  → SGR extended encoding (`\x1b[<btn;col;rowM` / `m`)
/// `sgr=false` → X10/normal encoding (clamped to fit in a byte)
pub fn encode_mouse_event(btn: u8, col: usize, row: usize, release: bool, sgr: bool) -> Vec<u8> {
    if sgr {
        let suffix = if release { 'm' } else { 'M' };
        format!("\x1b[<{};{};{}{}", btn, col + 1, row + 1, suffix).into_bytes()
    } else {
        // X10 encoding: each coordinate is offset by 32 and clamped to a byte.
        let b = btn + 32;
        let c = (col + 1 + 32) as u8;
        let r = (row + 1 + 32) as u8;
        vec![0x1b, b'[', b'M', b, c, r]
    }
}

#[cfg(test)]
#[path = "mouse_test.rs"]
mod tests;
