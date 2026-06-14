use chrono::{DateTime, Local};

/// Builds the right-side status bar text from a format string.
///
/// Supported tokens (substituted in place):
/// - `%date{fmt}` — current time formatted with strftime-style `fmt`
/// - `%pwd`       — replaced with `cwd` when present, or removed (with surrounding space) when absent
///
/// Literal text and spaces between tokens are preserved as-is.
/// Returns `None` when `template` is empty or resolves to an empty string.
pub(crate) fn apply_pwd_token(result: &mut String, cwd: Option<&str>) {
    match cwd {
        Some(p) => result.push_str(p),
        None => {
            if result.ends_with(' ') {
                result.pop();
            }
        }
    }
}

pub(crate) fn apply_date_token<'a>(
    result: &mut String,
    inner: &'a str,
    now: &DateTime<Local>,
) -> &'a str {
    if let Some(close) = inner.find('}') {
        result.push_str(&now.format(&inner[..close]).to_string());
        &inner[close + 1..]
    } else {
        result.push_str("%date{");
        inner
    }
}

pub fn resolve(template: &str, cwd: Option<&str>, now: &DateTime<Local>) -> Option<String> {
    if template.is_empty() {
        return None;
    }
    let mut result = String::with_capacity(template.len());
    let mut rest = template;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("%pwd") {
            apply_pwd_token(&mut result, cwd);
            rest = after;
        } else if let Some(inner) = rest.strip_prefix("%date{") {
            rest = apply_date_token(&mut result, inner, now);
        } else {
            let next = rest.find('%').unwrap_or(rest.len());
            result.push_str(&rest[..next]);
            rest = &rest[next..];
        }
    }
    let trimmed = result.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
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
