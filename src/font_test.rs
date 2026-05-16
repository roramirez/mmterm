use super::*;

#[test]
fn positive_delta_increases_size() {
    let result = apply_delta(16.0, 1.0).unwrap();
    assert!((result - 17.0).abs() < 0.01);
}

#[test]
fn negative_delta_decreases_size() {
    let result = apply_delta(16.0, -1.0).unwrap();
    assert!((result - 15.0).abs() < 0.01);
}

#[test]
fn delta_clamped_at_minimum() {
    // Already at min; negative delta → None (no change).
    assert!(apply_delta(6.0, -1.0).is_none());
}

#[test]
fn delta_clamped_at_maximum() {
    assert!(apply_delta(72.0, 1.0).is_none());
}

#[test]
fn delta_clamps_into_range() {
    // From 70, +10 would be 80 but is clamped to 72.
    let result = apply_delta(70.0, 10.0).unwrap();
    assert!((result - 72.0).abs() < 0.01);
}

#[test]
fn tiny_delta_below_epsilon_returns_none() {
    // Change < 0.1 → no-op.
    assert!(apply_delta(16.0, 0.05).is_none());
}

#[test]
fn zero_delta_returns_none() {
    assert!(apply_delta(16.0, 0.0).is_none());
}

#[test]
fn reset_to_default_from_large_size() {
    // Simulates Ctrl+0 passing delta = default - current.
    let result = apply_delta(24.0, 16.0 - 24.0).unwrap();
    assert!((result - 16.0).abs() < 0.01);
}
