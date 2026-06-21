use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub active_tab: usize,
    pub tabs: Vec<SavedTab>,
    /// Theme override for this scope; absent means use the global config theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedTab {
    pub name: Option<String>,
    /// Index into `pane_cwds` that was the active pane (DFS leaf order).
    pub active_pane: usize,
    /// CWD per pane in DFS leaf order; empty = fall back to $HOME.
    pub pane_cwds: Vec<PathBuf>,
    pub layout: SavedNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SavedNode {
    Leaf {
        slot: usize,
    },
    Split {
        dir: SavedSplitDir,
        ratio: f32,
        a: Box<SavedNode>,
        b: Box<SavedNode>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SavedSplitDir {
    H,
    V,
}

/// Returns the session file path for the given scope.
///
/// - `None`  → `~/.config/mmterm/session.toml` (default)
/// - `Some(name)` → `~/.config/mmterm/sessions/<name>.toml`
pub fn session_path_for(scope: Option<&str>) -> PathBuf {
    let base = dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mmterm");
    match scope {
        None => base.join("session.toml"),
        Some(name) => base.join("sessions").join(format!("{name}.toml")),
    }
}

/// Returns a sorted list of saved scope names (stems of `*.toml` in the
/// `sessions/` sub-directory of the mmterm config dir).
pub fn list_scopes() -> Vec<String> {
    list_scopes_in(&{
        dirs_next::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mmterm")
            .join("sessions")
    })
}

pub(crate) fn list_scopes_in(dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            let raw = e.file_name();
            let s = raw.to_string_lossy();
            s.strip_suffix(".toml").map(|stem| stem.to_string())
        })
        .collect();
    names.sort();
    names
}

/// Directory that holds per-pane scrollback text files for the given scope.
/// - `None`  → `~/.mmterm/default/`
/// - `Some(name)` → `~/.mmterm/<name>/`
///
/// Each scope always gets its own sub-directory so sessions with identical
/// tab/pane counts never share files.
pub fn scrollback_dir_for(scope: Option<&str>) -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mmterm")
        .join(scope.unwrap_or("default"))
}

/// Path to the scrollback file for one pane slot within a tab.
pub fn scrollback_path_for(scope: Option<&str>, tab: usize, slot: usize) -> PathBuf {
    scrollback_dir_for(scope).join(format!("tab-{tab}-pane-{slot}.txt"))
}

/// Write scrollback lines to a file atomically (`.tmp` → rename).
pub(crate) fn save_scrollback(path: &Path, lines: &[String]) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let content = lines.join("\n");
    let tmp = path.with_extension("txt.tmp");
    std::fs::write(&tmp, content.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Read scrollback lines from a file. Returns an empty vec if the file is missing.
pub(crate) fn load_scrollback(path: &Path) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.is_empty() => s.lines().map(|l| l.to_string()).collect(),
        _ => vec![],
    }
}

pub(crate) fn save_to(path: &std::path::Path, session: &SavedSession) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let content =
        toml::to_string_pretty(session).map_err(|e| anyhow::anyhow!("serialize session: {e}"))?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &content)?;
    std::fs::rename(&tmp, path)?;
    log::info!("Session saved to {}", path.display());
    Ok(())
}

pub(crate) fn load_from(path: &std::path::Path) -> Option<SavedSession> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return None,
    };
    match toml::from_str::<SavedSession>(&raw) {
        Ok(session) => {
            log::info!("Session loaded from {}", path.display());
            Some(session)
        }
        Err(e) => {
            log::warn!("Failed to parse session {}: {e}", path.display());
            None
        }
    }
}

#[cfg(test)]
#[path = "session_test.rs"]
mod tests;
