/// Resolves the directory used for log files.
/// Falls back to `~/.mmterm` when `log_dir` is empty.
pub fn resolve_log_dir(log_dir: &str) -> String {
    if log_dir.is_empty() {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{home}/.mmterm")
    } else {
        log_dir.to_string()
    }
}

/// Constructs the log file path for a given pane.
pub fn log_file_path(dir: &str, ts: u64, pane_id: usize) -> String {
    format!("{dir}/mmterm-{ts}-pane{pane_id}.log")
}

#[cfg(test)]
#[path = "logging_test.rs"]
mod tests;
