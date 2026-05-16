use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn install_bundled_themes_creates_nine_files() {
    let dir = tempdir().unwrap();
    install_bundled_themes(dir.path());
    let count = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("toml"))
        .count();
    assert_eq!(count, BUNDLED.len());
}

#[test]
fn install_bundled_themes_does_not_overwrite_existing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("catppuccin-mocha.toml");
    fs::write(&path, "sentinel").unwrap();
    install_bundled_themes(dir.path());
    assert_eq!(fs::read_to_string(&path).unwrap(), "sentinel");
}

#[test]
fn load_theme_catppuccin_mocha_parses() {
    let dir = tempdir().unwrap();
    install_bundled_themes(dir.path());
    let theme = load_theme("catppuccin-mocha", dir.path()).unwrap();
    assert_eq!(theme.background.r, 0x1e);
    assert_eq!(theme.background.g, 0x1e);
    assert_eq!(theme.background.b, 0x2e);
    assert_eq!(theme.foreground.r, 0xcd);
}

#[test]
fn load_theme_unknown_name_returns_error() {
    let dir = tempdir().unwrap();
    let result = load_theme("nonexistent", dir.path());
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("nonexistent"),
        "error should name the theme: {msg}"
    );
}

#[test]
fn load_theme_without_ui_fields_uses_palette_defaults() {
    let dir = tempdir().unwrap();
    let minimal = concat!(
        "foreground = \"#ffffff\"\n",
        "background = \"#000000\"\n",
        "color0  = \"#111111\"\n",
        "color1  = \"#ff0000\"\n",
        "color2  = \"#00ff00\"\n",
        "color3  = \"#ffff00\"\n",
        "color4  = \"#0000ff\"\n",
        "color5  = \"#ff00ff\"\n",
        "color6  = \"#00ffff\"\n",
        "color7  = \"#cccccc\"\n",
        "color8  = \"#888888\"\n",
        "color9  = \"#ff5555\"\n",
        "color10 = \"#55ff55\"\n",
        "color11 = \"#ffff55\"\n",
        "color12 = \"#5555ff\"\n",
        "color13 = \"#ff55ff\"\n",
        "color14 = \"#55ffff\"\n",
        "color15 = \"#eeeeee\"\n",
    );
    fs::write(dir.path().join("minimal.toml"), minimal).unwrap();
    let theme = load_theme("minimal", dir.path()).unwrap();
    // search_match defaults to palette[3] = yellow
    assert_eq!(theme.search_match.r, 0xff);
    assert_eq!(theme.search_match.g, 0xff);
    assert_eq!(theme.search_match.b, 0x00);
    // scrollbar defaults to palette[8] = gray
    assert_eq!(theme.scrollbar.r, 0x88);
}

#[test]
fn list_themes_returns_sorted_names() {
    let dir = tempdir().unwrap();
    install_bundled_themes(dir.path());
    let names = list_themes(dir.path());
    assert!(!names.is_empty());
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn list_themes_empty_dir_returns_empty() {
    let dir = tempdir().unwrap();
    let names = list_themes(dir.path());
    assert!(names.is_empty());
}

#[test]
fn default_theme_has_default_background() {
    let theme = default_theme();
    // default background is #121212
    assert_eq!(theme.background.r, 0x12);
    assert_eq!(theme.background.g, 0x12);
    assert_eq!(theme.background.b, 0x12);
}

#[test]
fn all_bundled_themes_parse_without_error() {
    let dir = tempdir().unwrap();
    install_bundled_themes(dir.path());
    for (name, _) in BUNDLED {
        let result = load_theme(name, dir.path());
        assert!(result.is_ok(), "theme {name} failed to parse: {:?}", result);
    }
}

#[test]
fn palette_has_16_entries() {
    let theme = default_theme();
    assert_eq!(theme.palette.len(), 16);
}

#[test]
fn list_themes_nonexistent_dir_returns_empty() {
    let names = list_themes(std::path::Path::new("/does/not/exist/at/all/0xdeadbeef"));
    assert!(names.is_empty());
}

#[test]
fn list_themes_dir_with_non_toml_files_skips_them() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("readme.txt"), "not a theme").unwrap();
    fs::write(dir.path().join("theme.toml.bak"), "also not").unwrap();
    let names = list_themes(dir.path());
    assert!(names.is_empty());
}

#[test]
fn load_theme_malformed_toml_returns_error() {
    // Covers the parse-error map_err branch in load_theme.
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("broken.toml"), "not valid toml ][[[").unwrap();
    let result = load_theme("broken", dir.path());
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("broken"), "error should name theme: {msg}");
}

#[cfg(unix)]
#[test]
fn install_bundled_themes_write_failure_is_silent() {
    // Make the themes dir read-only so writes fail → covers the
    // log::warn inside the write-error branch (line 129).
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().unwrap();
    let mut perms = fs::metadata(dir.path()).unwrap().permissions();
    perms.set_mode(0o555); // r-xr-xr-x — no write
    fs::set_permissions(dir.path(), perms.clone()).unwrap();
    // Should not panic even though all writes will fail.
    install_bundled_themes(dir.path());
    // Restore so tempdir cleanup works.
    perms.set_mode(0o755);
    fs::set_permissions(dir.path(), perms).unwrap();
}

#[cfg(unix)]
#[test]
fn install_bundled_themes_bad_path_is_silent() {
    // Pass a path whose parent is a regular file — create_dir_all fails →
    // covers lines 121-122 (log::warn + return).
    let dir = tempdir().unwrap();
    let file = dir.path().join("regular_file");
    fs::write(&file, "content").unwrap();
    // Subdirectory of a file can't be created.
    install_bundled_themes(&file.join("subdir"));
}
