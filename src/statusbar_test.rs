use super::*;
use chrono::TimeZone;

fn fixed_now() -> chrono::DateTime<chrono::Local> {
    chrono::Local
        .with_ymd_and_hms(2026, 1, 15, 12, 30, 0)
        .unwrap()
}

#[test]
fn empty_segments_returns_none() {
    assert!(resolve(&[], None, &fixed_now()).is_none());
}

#[test]
fn literal_segment_rendered_verbatim() {
    let segs = vec!["hello".to_string()];
    assert_eq!(
        resolve(&segs, None, &fixed_now()),
        Some("hello".to_string())
    );
}

#[test]
fn pwd_segment_with_cwd_present() {
    let segs = vec!["%pwd".to_string()];
    assert_eq!(
        resolve(&segs, Some("/home/user"), &fixed_now()),
        Some("/home/user".to_string())
    );
}

#[test]
fn pwd_segment_without_cwd_produces_nothing() {
    let segs = vec!["%pwd".to_string()];
    assert!(resolve(&segs, None, &fixed_now()).is_none());
}

#[test]
fn date_segment_formats_correctly() {
    let segs = vec!["%date{%Y-%m-%d}".to_string()];
    assert_eq!(
        resolve(&segs, None, &fixed_now()),
        Some("2026-01-15".to_string())
    );
}

#[test]
fn date_time_segment_formats_time() {
    let segs = vec!["%date{%H:%M}".to_string()];
    assert_eq!(
        resolve(&segs, None, &fixed_now()),
        Some("12:30".to_string())
    );
}

#[test]
fn multiple_segments_joined_with_double_space() {
    let segs = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    assert_eq!(
        resolve(&segs, None, &fixed_now()),
        Some("a  b  c".to_string())
    );
}

#[test]
fn pwd_and_literal_combined() {
    let segs = vec!["%pwd".to_string(), "branch".to_string()];
    assert_eq!(
        resolve(&segs, Some("/src"), &fixed_now()),
        Some("/src  branch".to_string())
    );
}

#[test]
fn all_segments_resolve_to_nothing_returns_none() {
    // Only %pwd with no cwd → nothing
    let segs = vec!["%pwd".to_string()];
    assert!(resolve(&segs, None, &fixed_now()).is_none());
}
