use super::*;

// ── resolve_log_dir ───────────────────────────────────────────────────────────

#[test]
fn empty_log_dir_falls_back_to_home_mmterm() {
    let dir = resolve_log_dir("");
    assert!(dir.ends_with("/.mmterm"), "got: {dir}");
}

#[test]
fn non_empty_log_dir_is_returned_unchanged() {
    assert_eq!(resolve_log_dir("/tmp/logs"), "/tmp/logs");
    assert_eq!(resolve_log_dir("relative/path"), "relative/path");
}

#[test]
fn empty_log_dir_contains_mmterm_suffix() {
    let dir = resolve_log_dir("");
    assert!(dir.contains(".mmterm"));
}

// ── log_file_path ─────────────────────────────────────────────────────────────

#[test]
fn log_file_path_format() {
    let path = log_file_path("/tmp/logs", 1_000_000, 3);
    assert_eq!(path, "/tmp/logs/mmterm-1000000-pane3.log");
}

#[test]
fn log_file_path_pane_zero() {
    let path = log_file_path("/tmp", 0, 0);
    assert_eq!(path, "/tmp/mmterm-0-pane0.log");
}

#[test]
fn log_file_path_ends_with_dot_log() {
    let path = log_file_path("/logs", 42, 1);
    assert!(path.ends_with(".log"));
}

#[test]
fn log_file_path_contains_pane_id() {
    let path = log_file_path("/d", 0, 7);
    assert!(path.contains("pane7"));
}
