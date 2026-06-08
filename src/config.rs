use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::terminal::grid::Color;

const DEFAULT_CONFIG: &str = include_str!("../assets/config.toml");

fn default_config() -> Config {
    toml::from_str(DEFAULT_CONFIG).expect("assets/config.toml is invalid")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub font: FontConfig,

    #[serde(default)]
    pub window: WindowConfig,

    #[serde(default)]
    pub shell: ShellConfig,

    #[serde(default)]
    pub terminal: TerminalConfig,

    #[serde(default)]
    pub logging: LogConfig,

    #[serde(default)]
    pub colors: ColorsConfig,

    #[serde(default)]
    pub theme: ThemeConfig,

    #[serde(default)]
    pub status_bar: StatusBarConfig,

    #[serde(default)]
    pub general: GeneralConfig,
}

fn default_true() -> bool {
    true
}

fn default_screenshot_dir() -> String {
    "~/mmterm/shot".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub restore_session: bool,
    #[serde(default = "default_screenshot_dir")]
    pub screenshot_dir: String,
    #[serde(default)]
    pub visual_bell: bool,
    #[serde(default = "default_true")]
    pub auto_update_check: bool,
    #[serde(default = "default_true")]
    pub auto_update_install: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            restore_session: default_true(),
            screenshot_dir: default_screenshot_dir(),
            visual_bell: false,
            auto_update_check: default_true(),
            auto_update_install: default_true(),
        }
    }
}

fn default_status_bar_right() -> String {
    "%pwd".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusBarConfig {
    #[serde(default = "default_status_bar_right")]
    pub right: String,
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            right: default_status_bar_right(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_theme_name")]
    pub name: String,
}

fn default_theme_name() -> String {
    "default".into()
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: default_theme_name(),
        }
    }
}

fn default_scrollback_lines() -> usize {
    10_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    #[serde(default = "default_scrollback_lines")]
    pub scrollback_lines: usize,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            scrollback_lines: default_scrollback_lines(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
}

fn default_inactive_dim() -> f32 {
    0.55
}
fn default_detect_urls() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub cursor_blink_ms: u32,
    #[serde(default = "default_inactive_dim")]
    pub inactive_dim: f32,
    #[serde(default = "default_detect_urls")]
    pub detect_urls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    pub program: Option<String>,
}

fn default_auto_log() -> bool {
    false
}

fn default_log_dir() -> String {
    String::new()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogConfig {
    /// Start logging automatically for every new pane.
    #[serde(default = "default_auto_log")]
    pub auto_log: bool,
    /// Directory for log files. Empty string means ~/.mmterm.
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorsConfig {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
    /// 16-color ANSI palette: [black, red, green, yellow, blue, magenta, cyan, white,
    ///                          bright variants of each]
    pub palette: Vec<String>,
}

#[allow(dead_code)]
impl ColorsConfig {
    pub fn bg(&self) -> Color {
        parse_hex(&self.background)
    }
    pub fn fg(&self) -> Color {
        parse_hex(&self.foreground)
    }
    pub fn cursor(&self) -> Color {
        parse_hex(&self.cursor)
    }
    pub fn selection(&self) -> Color {
        parse_hex(&self.selection)
    }

    pub fn palette_colors(&self) -> [Color; 16] {
        let mut out = [Color::rgb(0, 0, 0); 16];
        for (i, hex) in self.palette.iter().enumerate().take(16) {
            out[i] = parse_hex(hex);
        }
        out
    }
}

pub fn parse_hex(s: &str) -> Color {
    let s = s.trim_start_matches('#');
    let n = u32::from_str_radix(s, 16).unwrap_or(0);
    Color::rgb(
        ((n >> 16) & 0xFF) as u8,
        ((n >> 8) & 0xFF) as u8,
        (n & 0xFF) as u8,
    )
}

impl Default for FontConfig {
    fn default() -> Self {
        default_config().font
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        default_config().window
    }
}

impl Default for ShellConfig {
    fn default() -> Self {
        default_config().shell
    }
}

impl Default for ColorsConfig {
    fn default() -> Self {
        default_config().colors
    }
}

impl Default for Config {
    fn default() -> Self {
        default_config()
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(raw) => match toml::from_str::<Config>(&raw) {
                Ok(mut cfg) => {
                    cfg.terminal.scrollback_lines = cfg.terminal.scrollback_lines.max(100);
                    log::info!("Loaded config from {}", path.display());
                    cfg
                }
                Err(e) => {
                    log::warn!("Invalid config at {}: {e} — using defaults", path.display());
                    Self::default()
                }
            },
            Err(_) => {
                log::info!("No config at {} — using defaults", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match toml::to_string_pretty(self) {
            Ok(content) => match std::fs::write(&path, content) {
                Ok(_) => log::info!("Config saved to {}", path.display()),
                Err(e) => log::error!("Failed to save config: {e}"),
            },
            Err(e) => log::error!("Failed to serialize config: {e}"),
        }
    }

    pub fn write_default_if_missing() {
        let path = config_path();
        if path.exists() {
            return;
        }
        Self::default().save();
        log::info!("Created default config at {}", path.display());
    }
}

fn config_path() -> PathBuf {
    dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mmterm")
        .join("config.toml")
}

#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
