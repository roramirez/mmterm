use std::collections::HashSet;

use crate::config::{
    ColorsConfig, Config, FontConfig, GeneralConfig, LogConfig, ShellConfig, StatusBarConfig,
    TerminalConfig, ThemeConfig, WindowConfig,
};
use crate::theme::{list_themes, themes_dir};

// Field indices — keep in sync with from_config()
const F_RESTORE_SESSION: usize = 0;
const F_SCREENSHOT_DIR: usize = 1;
const F_VISUAL_BELL: usize = 2;
const F_FONT_FAMILY: usize = 3;
const F_FONT_SIZE: usize = 4;
const F_WIN_WIDTH: usize = 5;
const F_WIN_HEIGHT: usize = 6;
const F_WIN_TITLE: usize = 7;
const F_BLINK_MS: usize = 8;
const F_DIM: usize = 9;
const F_DETECT_URLS: usize = 10;
const F_SHELL: usize = 11;
const F_SCROLLBACK: usize = 12;
const F_LOG_AUTO: usize = 13;
const F_LOG_DIR: usize = 14;
const F_THEME_NAME: usize = 15;
const F_COLOR_BG: usize = 16;
const F_COLOR_FG: usize = 17;
const F_COLOR_CUR: usize = 18;
const F_COLOR_SEL: usize = 19;
const F_PALETTE: usize = 20; // F_PALETTE + 0..15
const F_STATUS_BAR_RIGHT: usize = 36;
const F_AUTO_UPDATE_CHECK: usize = 37;
const F_AUTO_UPDATE_INSTALL: usize = 38;

const PALETTE_LABELS: [&str; 16] = [
    "Palette 0  black",
    "Palette 1  red",
    "Palette 2  green",
    "Palette 3  yellow",
    "Palette 4  blue",
    "Palette 5  magenta",
    "Palette 6  cyan",
    "Palette 7  white",
    "Palette 8  br.blk",
    "Palette 9  br.red",
    "Palette 10 br.grn",
    "Palette 11 br.yel",
    "Palette 12 br.blu",
    "Palette 13 br.mag",
    "Palette 14 br.cyn",
    "Palette 15 br.wht",
];

#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    Text,
    Float,
    UInt,
    OptText,
    HexColor,            // #RRGGBB
    Bool,                // "true" | "false"
    Select(Vec<String>), // cyclic list; field.value holds the selected name
}

#[derive(Debug, Clone)]
pub struct Field {
    pub label: &'static str,
    pub hint: &'static str,
    pub value: String,
    pub kind: FieldKind,
    /// Visual section separator before this field
    pub section: Option<&'static str>,
}

pub enum ConfigAction {
    None,
    Save(Box<Config>),
    Cancel,
    PreviewTheme(String),
}

pub struct ConfigPanel {
    pub fields: Vec<Field>,
    pub selected: usize,
    pub editing: bool,
    pub edit_buf: String,
    pub status: Option<String>,
    /// Section names that are currently collapsed (body fields hidden).
    pub collapsed: HashSet<&'static str>,
}

impl ConfigPanel {
    pub fn from_config(cfg: &Config) -> Self {
        let hex = |s: &str| s.to_string();
        let mut fields = vec![
            // ── General ─────────────────────────────────────────────────────
            Field {
                label: "Restore Session",
                hint: "restore tabs, splits, and CWDs on next launch",
                value: cfg.general.restore_session.to_string(),
                kind: FieldKind::Bool,
                section: Some("General"),
            },
            Field {
                label: "Screenshot Dir",
                hint: "directory for screenshots; ~ expands to home (e.g. ~/mmterm/shot)",
                value: cfg.general.screenshot_dir.clone(),
                kind: FieldKind::OptText,
                section: None,
            },
            Field {
                label: "Visual Bell",
                hint: "flash the screen on BEL in addition to the status bar dot",
                value: cfg.general.visual_bell.to_string(),
                kind: FieldKind::Bool,
                section: None,
            },
            // ── Font ────────────────────────────────────────────────────────
            Field {
                label: "Font Family",
                hint: "system font name, e.g. 'JetBrains Mono'",
                value: cfg.font.family.clone(),
                kind: FieldKind::Text,
                section: Some("Font"),
            },
            Field {
                label: "Font Size",
                hint: "pixels, e.g. 16.0",
                value: cfg.font.size.to_string(),
                kind: FieldKind::Float,
                section: None,
            },
            // ── Window ──────────────────────────────────────────────────────
            Field {
                label: "Window Width",
                hint: "pixels",
                value: cfg.window.width.to_string(),
                kind: FieldKind::UInt,
                section: Some("Window"),
            },
            Field {
                label: "Window Height",
                hint: "pixels",
                value: cfg.window.height.to_string(),
                kind: FieldKind::UInt,
                section: None,
            },
            Field {
                label: "Window Title",
                hint: "title bar text",
                value: cfg.window.title.clone(),
                kind: FieldKind::Text,
                section: None,
            },
            Field {
                label: "Cursor Blink",
                hint: "milliseconds per half-cycle, e.g. 500",
                value: cfg.window.cursor_blink_ms.to_string(),
                kind: FieldKind::UInt,
                section: None,
            },
            Field {
                label: "Inactive Dim",
                hint: "brightness of unfocused panes (0.0–1.0, e.g. 0.55)",
                value: cfg.window.inactive_dim.to_string(),
                kind: FieldKind::Float,
                section: None,
            },
            Field {
                label: "Detect URLs",
                hint: "true or false — auto-detect http(s):// links",
                value: cfg.window.detect_urls.to_string(),
                kind: FieldKind::Bool,
                section: None,
            },
            // ── Shell ───────────────────────────────────────────────────────
            Field {
                label: "Shell",
                hint: "empty = use $SHELL",
                value: cfg.shell.program.clone().unwrap_or_default(),
                kind: FieldKind::OptText,
                section: Some("Shell"),
            },
            // ── Terminal ────────────────────────────────────────────────────
            Field {
                label: "Scrollback Lines",
                hint: "max lines kept in scrollback, e.g. 10000",
                value: cfg.terminal.scrollback_lines.to_string(),
                kind: FieldKind::UInt,
                section: Some("Terminal"),
            },
            // ── Logging ─────────────────────────────────────────────────────
            Field {
                label: "Auto Log",
                hint: "true or false — start logging automatically for each new pane",
                value: cfg.logging.auto_log.to_string(),
                kind: FieldKind::Bool,
                section: Some("Logging"),
            },
            Field {
                label: "Log Dir",
                hint: "directory for log files; empty = $HOME",
                value: cfg.logging.log_dir.clone(),
                kind: FieldKind::OptText,
                section: None,
            },
            // ── Theme ───────────────────────────────────────────────────────
            Field {
                label: "Theme",
                hint: "← / → to cycle  (changes apply immediately as preview)",
                value: cfg.theme.name.clone(),
                kind: FieldKind::Select(list_themes(&themes_dir())),
                section: Some("Theme"),
            },
            // ── Colors ──────────────────────────────────────────────────────
            Field {
                label: "Background",
                hint: "#RRGGBB",
                value: hex(&cfg.colors.background),
                kind: FieldKind::HexColor,
                section: Some("Colors"),
            },
            Field {
                label: "Foreground",
                hint: "#RRGGBB",
                value: hex(&cfg.colors.foreground),
                kind: FieldKind::HexColor,
                section: None,
            },
            Field {
                label: "Cursor",
                hint: "#RRGGBB",
                value: hex(&cfg.colors.cursor),
                kind: FieldKind::HexColor,
                section: None,
            },
            Field {
                label: "Selection",
                hint: "#RRGGBB",
                value: hex(&cfg.colors.selection),
                kind: FieldKind::HexColor,
                section: None,
            },
        ];

        // 16 palette entries
        let palette = cfg.colors.palette.clone();
        for (i, label) in PALETTE_LABELS.iter().enumerate() {
            let value = palette
                .get(i)
                .cloned()
                .unwrap_or_else(|| "#000000".to_string());
            fields.push(Field {
                label,
                hint: "#RRGGBB",
                value,
                kind: FieldKind::HexColor,
                section: if i == 0 { Some("Palette") } else { None },
            });
        }

        // ── Status Bar ──────────────────────────────────────────────────────
        fields.push(Field {
            label: "Status Bar Right",
            hint: "format string: %pwd  %date{%H:%M:%S}",
            value: cfg.status_bar.right.clone(),
            kind: FieldKind::OptText,
            section: Some("Status Bar"),
        });

        // ── Updates ─────────────────────────────────────────────────────────
        fields.push(Field {
            label: "Auto Update Check",
            hint: "check GitHub once per day for a newer release",
            value: cfg.general.auto_update_check.to_string(),
            kind: FieldKind::Bool,
            section: Some("Updates"),
        });
        fields.push(Field {
            label: "Auto Update Install",
            hint: "Linux only: silently self-replace binary when update found",
            value: cfg.general.auto_update_install.to_string(),
            kind: FieldKind::Bool,
            section: None,
        });

        let mut collapsed = HashSet::new();
        collapsed.insert("Palette");

        Self {
            fields,
            selected: 0,
            editing: false,
            edit_buf: String::new(),
            status: None,
            collapsed,
        }
    }

    pub fn handle_char(&mut self, c: char) -> ConfigAction {
        if self.editing {
            match c {
                '\r' | '\n' => {
                    self.confirm_edit();
                    ConfigAction::None
                }
                '\x1b' => {
                    self.cancel_edit();
                    ConfigAction::None
                }
                '\x7f' | '\x08' => {
                    self.edit_buf.pop();
                    ConfigAction::None
                }
                _ => {
                    self.edit_buf.push(c);
                    ConfigAction::None
                }
            }
        } else {
            match c {
                'j' | 'J' => {
                    self.move_down();
                    ConfigAction::None
                }
                'k' | 'K' => {
                    self.move_up();
                    ConfigAction::None
                }
                'i' | '\r' | '\n' => {
                    self.start_edit();
                    ConfigAction::None
                }
                ' ' => {
                    self.toggle_collapse();
                    ConfigAction::None
                }
                ']' => {
                    self.jump_section_forward();
                    ConfigAction::None
                }
                '[' => {
                    self.jump_section_backward();
                    ConfigAction::None
                }
                'q' | '\x1b' => ConfigAction::Cancel,
                _ => ConfigAction::None,
            }
        }
    }

    pub fn handle_backspace(&mut self) -> ConfigAction {
        if self.editing {
            self.edit_buf.pop();
        }
        ConfigAction::None
    }

    pub fn handle_up(&mut self) -> ConfigAction {
        self.move_up();
        ConfigAction::None
    }
    pub fn handle_down(&mut self) -> ConfigAction {
        self.move_down();
        ConfigAction::None
    }
    pub fn handle_left(&mut self) -> ConfigAction {
        self.cycle_select(-1)
    }
    pub fn handle_right(&mut self) -> ConfigAction {
        self.cycle_select(1)
    }

    pub fn handle_escape(&mut self) -> ConfigAction {
        if self.editing {
            self.cancel_edit();
            ConfigAction::None
        } else {
            ConfigAction::Cancel
        }
    }

    pub fn save(&mut self) -> ConfigAction {
        if self.editing {
            self.confirm_edit();
        }
        match self.build_config() {
            Ok(cfg) => {
                self.status = Some("Saved. Font/color changes apply on restart.".into());
                ConfigAction::Save(Box::new(cfg))
            }
            Err(e) => {
                self.status = Some(format!("Error: {e}"));
                ConfigAction::None
            }
        }
    }

    fn move_up(&mut self) {
        let vis = self.visible_indices();
        if let Some(pos) = vis
            .iter()
            .position(|&i| i == self.selected)
            .filter(|&p| p > 0)
        {
            self.selected = vis[pos - 1];
        }
    }

    fn move_down(&mut self) {
        let vis = self.visible_indices();
        if let Some(pos) = vis
            .iter()
            .position(|&i| i == self.selected)
            .filter(|&p| p + 1 < vis.len())
        {
            self.selected = vis[pos + 1];
        }
    }

    /// Field indices that are currently visible (section headers always shown;
    /// body fields hidden when their section is collapsed).
    pub fn visible_indices(&self) -> Vec<usize> {
        let mut vis = Vec::new();
        let mut current_section: Option<&'static str> = None;
        for (i, field) in self.fields.iter().enumerate() {
            if let Some(sec) = field.section {
                current_section = Some(sec);
                vis.push(i);
            } else {
                let hidden = current_section
                    .map(|s| self.collapsed.contains(s))
                    .unwrap_or(false);
                if !hidden {
                    vis.push(i);
                }
            }
        }
        vis
    }

    /// Section name that field `idx` belongs to (walks backwards to the nearest header).
    pub fn section_of(&self, idx: usize) -> Option<&'static str> {
        for i in (0..=idx).rev() {
            if let Some(sec) = self.fields[i].section {
                return Some(sec);
            }
        }
        None
    }

    /// Toggle the collapsed state of the section whose header is at `selected`.
    /// No-op if the selected field is not a section header.
    pub fn toggle_collapse(&mut self) {
        let Some(sec) = self.fields[self.selected].section else {
            return;
        };
        if self.collapsed.contains(sec) {
            self.collapsed.remove(sec);
        } else {
            self.collapsed.insert(sec);
        }
        // Clamp selected to remain within the (updated) visible set.
        let vis = self.visible_indices();
        if !vis.contains(&self.selected) {
            self.selected = *vis.last().unwrap_or(&0);
        }
    }

    fn jump_section(&mut self, forward: bool) {
        let vis = self.visible_indices();
        let headers: Vec<usize> = vis
            .iter()
            .copied()
            .filter(|&i| self.fields[i].section.is_some())
            .collect();
        if headers.len() <= 1 {
            return;
        }
        let cur_sec = self.section_of(self.selected);
        let cur_pos = headers
            .iter()
            .position(|&i| self.fields[i].section == cur_sec)
            .unwrap_or(0);
        self.selected = if forward {
            headers[(cur_pos + 1) % headers.len()]
        } else {
            headers[cur_pos.checked_sub(1).unwrap_or(headers.len() - 1)]
        };
    }

    /// Jump to the next section header in visible order, wrapping around.
    pub fn jump_section_forward(&mut self) {
        self.jump_section(true);
    }

    /// Jump to the previous section header in visible order, wrapping around.
    pub fn jump_section_backward(&mut self) {
        self.jump_section(false);
    }

    /// Number of body fields hidden when `sec` is collapsed (excludes the header itself).
    pub fn collapsed_count(&self, sec: &str) -> usize {
        let mut in_sec = false;
        let mut count = 0;
        for field in &self.fields {
            if let Some(s) = field.section {
                in_sec = s == sec;
            } else if in_sec {
                count += 1;
            }
        }
        count
    }

    fn start_edit(&mut self) {
        if matches!(self.fields[self.selected].kind, FieldKind::Select(_)) {
            return;
        }
        self.edit_buf = self.fields[self.selected].value.clone();
        self.editing = true;
        self.status = None;
    }

    fn confirm_edit(&mut self) {
        let val = self.edit_buf.clone();
        if self.validate(&val) {
            let normalized = if matches!(self.fields[self.selected].kind, FieldKind::HexColor) {
                normalize_hex(&val)
            } else {
                val
            };
            self.fields[self.selected].value = normalized;
            self.editing = false;
        } else {
            self.status = Some(format!(
                "Invalid value for {} ({})",
                self.fields[self.selected].label, self.fields[self.selected].hint,
            ));
        }
    }

    fn cancel_edit(&mut self) {
        self.edit_buf.clear();
        self.editing = false;
    }

    fn cycle_select(&mut self, delta: i32) -> ConfigAction {
        let field = &mut self.fields[self.selected];
        let FieldKind::Select(options) = &field.kind else {
            return ConfigAction::None;
        };
        if options.is_empty() {
            return ConfigAction::None;
        }
        let cur = options.iter().position(|o| o == &field.value).unwrap_or(0);
        let len = options.len();
        let next = ((cur as i32 + delta).rem_euclid(len as i32)) as usize;
        let name = options[next].clone();
        field.value = name.clone();
        ConfigAction::PreviewTheme(name)
    }

    fn validate(&self, val: &str) -> bool {
        match &self.fields[self.selected].kind {
            FieldKind::Text => !val.is_empty(),
            FieldKind::OptText => true,
            FieldKind::Float => val.parse::<f32>().is_ok_and(|v| v > 0.0),
            FieldKind::UInt => val.parse::<u32>().is_ok_and(|v| v > 0),
            FieldKind::HexColor => is_valid_hex(val),
            FieldKind::Bool => val == "true" || val == "false",
            FieldKind::Select(_) => true,
        }
    }

    fn build_config(&self) -> Result<Config, String> {
        let get = |i: usize| self.fields[i].value.clone();

        let family = get(F_FONT_FAMILY);
        if family.is_empty() {
            return Err("Font family cannot be empty".into());
        }

        let size = get(F_FONT_SIZE)
            .parse::<f32>()
            .map_err(|_| "Invalid font size")?;
        if size <= 0.0 {
            return Err("Font size must be > 0".into());
        }

        let width = get(F_WIN_WIDTH)
            .parse::<u32>()
            .map_err(|_| "Invalid window width")?;
        let height = get(F_WIN_HEIGHT)
            .parse::<u32>()
            .map_err(|_| "Invalid window height")?;
        let title = get(F_WIN_TITLE);
        let blink_ms = get(F_BLINK_MS)
            .parse::<u32>()
            .map_err(|_| "Invalid cursor blink ms")?;
        let inactive_dim = get(F_DIM)
            .parse::<f32>()
            .map_err(|_| "Invalid inactive dim")?
            .clamp(0.0, 1.0);
        let detect_urls = get(F_DETECT_URLS)
            .parse::<bool>()
            .map_err(|_| "Invalid detect_urls — use true or false")?;
        let shell = {
            let s = get(F_SHELL);
            if s.is_empty() { None } else { Some(s) }
        };

        let scrollback_lines = get(F_SCROLLBACK)
            .parse::<usize>()
            .map_err(|_| "Invalid scrollback_lines")?;
        if scrollback_lines == 0 {
            return Err("Scrollback lines must be > 0".into());
        }

        let auto_log = get(F_LOG_AUTO)
            .parse::<bool>()
            .map_err(|_| "Invalid auto_log — use true or false")?;
        let log_dir = get(F_LOG_DIR);

        let theme_name = get(F_THEME_NAME);

        let background = get(F_COLOR_BG);
        let foreground = get(F_COLOR_FG);
        let cursor = get(F_COLOR_CUR);
        let selection = get(F_COLOR_SEL);

        let palette: Vec<String> = (0..16).map(|i| get(F_PALETTE + i)).collect();

        let status_bar_right = get(F_STATUS_BAR_RIGHT);

        let restore_session = get(F_RESTORE_SESSION)
            .parse::<bool>()
            .map_err(|_| "Invalid restore_session — use true or false")?;

        let screenshot_dir = get(F_SCREENSHOT_DIR);

        let visual_bell = get(F_VISUAL_BELL)
            .parse::<bool>()
            .map_err(|_| "Invalid visual_bell — use true or false")?;

        let auto_update_check = get(F_AUTO_UPDATE_CHECK)
            .parse::<bool>()
            .map_err(|_| "Invalid auto_update_check — use true or false")?;

        let auto_update_install = get(F_AUTO_UPDATE_INSTALL)
            .parse::<bool>()
            .map_err(|_| "Invalid auto_update_install — use true or false")?;

        Ok(Config {
            font: FontConfig { family, size },
            window: WindowConfig {
                width,
                height,
                title,
                cursor_blink_ms: blink_ms,
                inactive_dim,
                detect_urls,
            },
            shell: ShellConfig { program: shell },
            terminal: TerminalConfig { scrollback_lines },
            logging: LogConfig { auto_log, log_dir },
            colors: ColorsConfig {
                background,
                foreground,
                cursor,
                selection,
                palette,
            },
            theme: ThemeConfig { name: theme_name },
            status_bar: StatusBarConfig {
                right: status_bar_right,
            },
            general: GeneralConfig {
                restore_session,
                screenshot_dir,
                visual_bell,
                auto_update_check,
                auto_update_install,
            },
        })
    }

    pub fn display_value(&self, idx: usize) -> &str {
        if self.editing && idx == self.selected {
            &self.edit_buf
        } else {
            &self.fields[idx].value
        }
    }
}

fn is_valid_hex(s: &str) -> bool {
    let s = s.trim_start_matches('#');
    s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn normalize_hex(s: &str) -> String {
    let s = s.trim_start_matches('#');
    format!("#{}", s.to_uppercase())
}

#[cfg(test)]
#[path = "tui_config_test.rs"]
mod tests;
