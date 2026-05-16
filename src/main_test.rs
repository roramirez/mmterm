use super::debug_log_path;

#[test]
fn no_debug_flag_returns_none() {
    if !std::env::args().any(|a| a == "--debug") {
        assert!(debug_log_path().is_none());
    }
}

#[test]
fn debug_log_path_format() {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = format!("{home}/.mmterm");
    let ts = chrono::Local::now().format("%Y%m%dT%H%M%S");
    let path = format!("{dir}/debug-{ts}.log");
    assert!(path.starts_with(&dir));
    assert!(path.ends_with(".log"));
    assert!(path.contains("debug-"));
}
