use chrono::{DateTime, Local};

/// Builds the right-side status bar text from a list of segment tokens.
///
/// Supported tokens:
/// - `%date{fmt}` — current time formatted with strftime-style `fmt`
/// - `%pwd`       — replaced with `cwd` when present
/// - anything else — rendered verbatim
///
/// Returns `None` when `segments` is empty or all segments resolve to nothing.
pub fn resolve(segments: &[String], cwd: Option<&str>, now: &DateTime<Local>) -> Option<String> {
    if segments.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for seg in segments {
        if let Some(fmt) = seg.strip_prefix("%date{").and_then(|s| s.strip_suffix('}')) {
            parts.push(now.format(fmt).to_string());
        } else if seg == "%pwd" {
            if let Some(p) = cwd {
                parts.push(p.to_string());
            }
        } else {
            parts.push(seg.clone());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("  "))
    }
}

/// Replaces a leading `$HOME` prefix in `path` with `~`.
/// Returns `path` unchanged when `home` is empty or `path` does not start with it.
pub fn shorten_home(path: &str, home: &str) -> String {
    if !home.is_empty() && path.starts_with(home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    }
}

/// Returns the pane OSC title to display in the status bar, or `None` when it
/// should be suppressed.
///
/// The title is suppressed when `pwd_in_right` is true and the title matches
/// the current working directory (which is already shown on the right side).
pub fn pane_title_for_display<'a>(
    pane_title: Option<&'a str>,
    pwd_in_right: bool,
    cwd: Option<&str>,
) -> Option<&'a str> {
    pane_title.filter(|t| !(pwd_in_right && cwd.is_some_and(|c| *t == c)))
}

#[cfg(test)]
#[path = "statusbar_test.rs"]
mod tests;
