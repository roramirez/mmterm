use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_file(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[test]
fn load_missing_file_returns_empty() {
    let path = std::path::Path::new("/tmp/mmterm_history_nonexistent_xyz");
    assert!(load_from(path).is_empty());
}

#[test]
fn load_parses_valid_lines() {
    let f = write_file(": 1749240000:0;error\n: 1749240060:0;panic\n");
    let h = load_from(f.path());
    assert_eq!(h, vec!["error", "panic"]);
}

#[test]
fn load_skips_malformed_lines() {
    let f = write_file("not a history line\n: 1749240000:0;good\ngarbage\n");
    let h = load_from(f.path());
    assert_eq!(h, vec!["good"]);
}

#[test]
fn load_dedup_keeps_last_occurrence() {
    let f = write_file(": 1000:0;foo\n: 2000:0;bar\n: 3000:0;foo\n");
    let h = load_from(f.path());
    // "foo" appeared twice; only the last matters, order: bar then foo
    assert_eq!(h, vec!["bar", "foo"]);
}

#[test]
fn round_trip_single_entry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("search_history");
    save_to(&path, &["regex_test".to_string()]);
    let loaded = load_from(&path);
    assert_eq!(loaded, vec!["regex_test"]);
}

#[test]
fn round_trip_preserves_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("search_history");
    let history = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
    save_to(&path, &history);
    assert_eq!(load_from(&path), history);
}

#[test]
fn save_caps_at_max_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("search_history");
    let history: Vec<String> = (0..60).map(|i| format!("q{i}")).collect();
    save_to(&path, &history);
    let loaded = load_from(&path);
    assert_eq!(loaded.len(), MAX_ENTRIES);
    assert_eq!(loaded[0], "q0");
    assert_eq!(loaded[49], "q49");
}

#[test]
fn load_skips_empty_query() {
    let f = write_file(": 1000:0;\n: 2000:0;valid\n");
    let h = load_from(f.path());
    assert_eq!(h, vec!["valid"]);
}
