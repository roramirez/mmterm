use super::{Logical, Physical, Scale};

#[test]
fn px_2x() {
    assert_eq!(Scale::new(2.0).px(Logical(16.0)), Physical(32.0));
}
#[test]
fn px_1x() {
    assert_eq!(Scale::new(1.0).px(Logical(16.0)), Physical(16.0));
}
#[test]
fn px_1_5x() {
    assert_eq!(Scale::new(1.5).px(Logical(16.0)), Physical(24.0));
}
#[test]
fn chrome_2x() {
    assert_eq!(Scale::new(2.0).chrome(22), 44);
}
#[test]
fn chrome_1x() {
    assert_eq!(Scale::new(1.0).chrome(22), 22);
}
#[test]
fn chrome_1_5x() {
    assert_eq!(Scale::new(1.5).chrome(22), 33);
}
#[test]
fn chrome_1_25x_rounds() {
    assert_eq!(Scale::new(1.25).chrome(22), 28);
}
#[test]
fn chrome_round_half_up() {
    assert_eq!(Scale::new(1.5).chrome(1), 2);
}
#[test]
fn scale_floor_sub_one() {
    assert_eq!(Scale::new(0.5).get(), 1.0);
}
#[test]
fn scale_floor_zero() {
    assert_eq!(Scale::new(0.0).get(), 1.0);
}
#[test]
fn scale_unity() {
    assert_eq!(Scale::new(1.0).get(), 1.0);
}
