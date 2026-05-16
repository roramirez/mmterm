const FONT_SIZE_MIN: f32 = 6.0;
const FONT_SIZE_MAX: f32 = 72.0;
const FONT_SIZE_EPSILON: f32 = 0.1;

/// Applies `delta` to `current` font size, clamped to `[6, 72]`.
/// Returns `Some(new_size)` when the change is large enough to matter,
/// or `None` when the result would differ by less than 0.1 px (no-op).
pub fn apply_delta(current: f32, delta: f32) -> Option<f32> {
    let new_size = (current + delta).clamp(FONT_SIZE_MIN, FONT_SIZE_MAX);
    if (new_size - current).abs() < FONT_SIZE_EPSILON {
        None
    } else {
        Some(new_size)
    }
}

#[cfg(test)]
#[path = "font_test.rs"]
mod tests;
