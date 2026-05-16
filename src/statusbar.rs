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

#[cfg(test)]
#[path = "statusbar_test.rs"]
mod tests;
