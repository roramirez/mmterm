use std::path::{Path, PathBuf};

use crate::config::parse_hex;
use crate::terminal::grid::Color;

const BUNDLED: &[(&str, &str)] = &[
    ("default", include_str!("themes/default.toml")),
    (
        "catppuccin-mocha",
        include_str!("themes/catppuccin-mocha.toml"),
    ),
    ("dracula", include_str!("themes/dracula.toml")),
    ("gruvbox-dark", include_str!("themes/gruvbox-dark.toml")),
    ("monokai", include_str!("themes/monokai.toml")),
    ("nord", include_str!("themes/nord.toml")),
    ("one-dark", include_str!("themes/one-dark.toml")),
    ("solarized-dark", include_str!("themes/solarized-dark.toml")),
    ("tokyo-night", include_str!("themes/tokyo-night.toml")),
];

/// All colors needed to render a mmterm session.
#[derive(Debug, Clone)]
pub struct ResolvedTheme {
    pub foreground: Color,
    pub background: Color,
    pub cursor: Color,
    pub selection: Color,
    pub palette: [Color; 16],
    // UI chrome
    pub search_match: Color,
    pub search_current: Color,
    pub scrollbar: Color,
    pub badge: Color,
    pub separator: Color,
}

/// Raw TOML shape of a theme file.
#[derive(serde::Deserialize)]
struct ThemeFile {
    foreground: String,
    background: String,
    color0: String,
    color1: String,
    color2: String,
    color3: String,
    color4: String,
    color5: String,
    color6: String,
    color7: String,
    color8: String,
    color9: String,
    color10: String,
    color11: String,
    color12: String,
    color13: String,
    color14: String,
    color15: String,
    // Optional UI fields — derived from palette if absent.
    cursor: Option<String>,
    selection: Option<String>,
    search_match: Option<String>,
    search_current: Option<String>,
    scrollbar: Option<String>,
    badge: Option<String>,
    separator: Option<String>,
}

fn resolve(tf: &ThemeFile) -> ResolvedTheme {
    let h = |s: &str| parse_hex(s);
    let palette = [
        h(&tf.color0),
        h(&tf.color1),
        h(&tf.color2),
        h(&tf.color3),
        h(&tf.color4),
        h(&tf.color5),
        h(&tf.color6),
        h(&tf.color7),
        h(&tf.color8),
        h(&tf.color9),
        h(&tf.color10),
        h(&tf.color11),
        h(&tf.color12),
        h(&tf.color13),
        h(&tf.color14),
        h(&tf.color15),
    ];
    ResolvedTheme {
        foreground: h(&tf.foreground),
        background: h(&tf.background),
        cursor: tf.cursor.as_deref().map(h).unwrap_or(palette[15]),
        selection: tf.selection.as_deref().map(h).unwrap_or(palette[0]),
        search_match: tf.search_match.as_deref().map(h).unwrap_or(palette[3]),
        search_current: tf.search_current.as_deref().map(h).unwrap_or(palette[1]),
        scrollbar: tf.scrollbar.as_deref().map(h).unwrap_or(palette[8]),
        badge: tf.badge.as_deref().map(h).unwrap_or(palette[2]),
        separator: tf.separator.as_deref().map(h).unwrap_or(palette[0]),
        palette,
    }
}

/// Returns the built-in default theme (catppuccin-mocha).
pub fn default_theme() -> ResolvedTheme {
    let tf: ThemeFile =
        toml::from_str(BUNDLED[0].1).expect("bundled catppuccin-mocha.toml is invalid");
    resolve(&tf)
}

/// Returns `~/.config/mmterm/themes/`.
pub fn themes_dir() -> PathBuf {
    dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mmterm")
        .join("themes")
}

/// Writes bundled theme files to `themes_dir` if they do not yet exist.
/// Never overwrites user edits.
pub fn install_bundled_themes(themes_dir: &Path) {
    if let Err(e) = std::fs::create_dir_all(themes_dir) {
        log::warn!("Could not create themes dir {}: {e}", themes_dir.display());
        return;
    }
    for (name, content) in BUNDLED {
        let path = themes_dir.join(format!("{name}.toml"));
        if !path.exists()
            && let Err(e) = std::fs::write(&path, content)
        {
            log::warn!("Could not write theme {name}: {e}");
        }
    }
}

/// Loads a theme by name from `themes_dir/<name>.toml`.
/// Returns an error string if the file is missing or malformed.
pub fn load_theme(name: &str, themes_dir: &Path) -> Result<ResolvedTheme, String> {
    let path = themes_dir.join(format!("{name}.toml"));
    let raw = std::fs::read_to_string(&path).map_err(|_| {
        format!(
            "theme \"{name}\" not found — check {}",
            themes_dir.display()
        )
    })?;
    let tf: ThemeFile =
        toml::from_str(&raw).map_err(|e| format!("invalid theme \"{name}\": {e}"))?;
    Ok(resolve(&tf))
}

/// Returns available theme names from `themes_dir`, sorted alphabetically.
pub fn list_themes(themes_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(themes_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("toml") {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

#[cfg(test)]
#[path = "theme_test.rs"]
mod tests;
