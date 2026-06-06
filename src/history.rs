use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ENTRIES: usize = 50;

pub fn history_path() -> PathBuf {
    dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mmterm")
        .join("search_history")
}

/// Reads the history file and returns queries oldest-first.
/// Lines that don't match the `: <ts>:0;<query>` format are silently skipped.
pub fn load_search_history() -> Vec<String> {
    load_from(&history_path())
}

pub(crate) fn load_from(path: &std::path::Path) -> Vec<String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut entries: Vec<String> = raw
        .lines()
        .filter(|l| l.starts_with(": "))
        .filter_map(|l| {
            let after_colon = l.strip_prefix(": ")?;
            let (_meta, query) = after_colon.split_once(';')?;
            let q = query.to_string();
            if q.is_empty() { None } else { Some(q) }
        })
        .collect();
    // dedup: keep last occurrence of each query
    let mut seen = std::collections::HashSet::new();
    let mut deduped: Vec<String> = Vec::new();
    for q in entries.iter().rev() {
        if seen.insert(q.clone()) {
            deduped.push(q.clone());
        }
    }
    deduped.reverse();
    entries = deduped;
    entries.truncate(MAX_ENTRIES);
    entries
}

/// Atomically rewrites the history file with the current in-memory history.
/// Uses the zsh EXTENDED_HISTORY format: `: <unix_ts>:0;<query>`.
pub fn save_search_history(history: &[String]) {
    save_to(&history_path(), history);
}

pub(crate) fn save_to(path: &std::path::Path, history: &[String]) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let entries: Vec<String> = history
        .iter()
        .take(MAX_ENTRIES)
        .map(|q| format!(": {ts}:0;{q}"))
        .collect();
    let content = entries.join("\n") + "\n";
    let tmp = path.with_file_name("search_history.tmp");
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        log::warn!("search history: cannot create dir: {e}");
        return;
    }
    if let Err(e) = std::fs::write(&tmp, &content) {
        log::warn!("search history: write failed: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        log::warn!("search history: rename failed: {e}");
    }
}

#[cfg(test)]
#[path = "history_test.rs"]
mod tests;
