//! Single source of truth for shortcut bindings.
//!
//! `default_keymap()` returns all built-in modifier/chord shortcuts as data.
//! `KeyMap::from_config` overlays the user's `[keybindings]` config onto a copy
//! of the defaults (insert/replace; `"none"` removes; invalid entries skipped +
//! collected as `KeymapError`). Dispatch consults `lookup` first; a miss falls
//! through to the literal/PTY encoding handlers in `keybindings.rs`.
//
use std::collections::HashMap;

use winit::keyboard::{Key, NamedKey};

use crate::config::KeybindingsConfig;
use crate::input::keybindings::Action;
use crate::input::mode::InputMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyToken {
    /// Single character as winit reports it in `Key::Character`. Primary tokens
    /// are lowercased; chord-tail tokens keep their original case (so `Ctrl+W R`
    /// stays distinct from `Ctrl+W r`).
    Char(String),
    Named(NamedKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModeClass {
    Global,
    Normal,
    Visual,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindingKey {
    pub mods: Mods,
    pub token: KeyToken,
    pub chord_tail: Option<(Mods, KeyToken)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeymapError {
    Parse { raw: String, reason: String },
    UnknownAction { raw: String, name: String },
    ShadowsInput { raw: String },
}

/// Parse a single binding string into its mode scope + `BindingKey`.
/// Grammar: `[normal:|visual:] mods*('+') key [SPACE tail-mods*('+') tail-key]`.
pub fn parse_binding(raw: &str) -> Result<(ModeClass, BindingKey), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty binding".into());
    }

    // Mode prefix.
    let (scope, rest) = if let Some(r) = trimmed.strip_prefix("normal:") {
        (ModeClass::Normal, r)
    } else if let Some(r) = trimmed.strip_prefix("visual:") {
        (ModeClass::Visual, r)
    } else {
        (ModeClass::Global, trimmed)
    };

    // Chord: split on the first ASCII space into head + tail.
    let mut parts = rest.splitn(2, ' ');
    let head = parts.next().unwrap_or("").trim();
    let tail = parts.next().map(str::trim).filter(|s| !s.is_empty());

    let (mods, token) = parse_combo(head, /* lowercase */ true)?;
    let chord_tail = match tail {
        Some(t) => {
            let (tmods, ttoken) = parse_combo(t, /* lowercase */ false)?;
            Some((tmods, ttoken))
        }
        None => None,
    };

    Ok((
        scope,
        BindingKey {
            mods,
            token,
            chord_tail,
        },
    ))
}

/// Parse `ctrl+shift+v` style combo. The final segment is the key; the rest are
/// modifiers. `lowercase` controls whether a single-letter key is folded.
fn parse_combo(s: &str, lowercase: bool) -> Result<(Mods, KeyToken), String> {
    if s.is_empty() {
        return Err("empty key combo".into());
    }
    // A bare "+" or "=" key with no modifiers.
    if s == "+" {
        return Ok((Mods::default(), KeyToken::Char("+".into())));
    }

    let mut mods = Mods::default();
    // Determine the key segment and the modifier prefix. A combo ending in `++`
    // means the key itself is `+`, e.g. `cmd++`: the modifiers are everything up
    // to that doubled `+`. A combo ending in a single `+` (e.g. `ctrl+`) is a
    // dangling separator with no key and is rejected.
    let (modifier_str, key_seg): (&str, String) = if let Some(mods_str) = s.strip_suffix("++") {
        (mods_str, "+".to_string())
    } else if s.ends_with('+') {
        return Err("binding ends with a trailing `+` separator".into());
    } else {
        match s.rsplit_once('+') {
            Some((m, k)) => (m, k.to_string()),
            None => ("", s.to_string()),
        }
    };

    if !modifier_str.is_empty() {
        for m in modifier_str.split('+') {
            apply_modifier(&mut mods, m)?;
        }
    }

    let token = parse_key_token(&key_seg, lowercase)?;
    Ok((mods, token))
}

fn apply_modifier(mods: &mut Mods, m: &str) -> Result<(), String> {
    match m.to_lowercase().as_str() {
        "ctrl" | "control" => mods.ctrl = true,
        "shift" => mods.shift = true,
        "alt" | "option" => mods.alt = true,
        "cmd" | "super" | "win" | "meta" => mods.cmd = true,
        other => return Err(format!("unknown modifier `{other}`")),
    }
    Ok(())
}

fn parse_key_token(s: &str, lowercase: bool) -> Result<KeyToken, String> {
    if let Some(named) = named_key_from_name(s) {
        return Ok(KeyToken::Named(named));
    }
    // Single grapheme / char key.
    let chars: Vec<char> = s.chars().collect();
    if chars.len() != 1 {
        return Err(format!("unknown key `{s}`"));
    }
    let c = chars[0];
    let stored = if lowercase {
        c.to_lowercase().to_string()
    } else {
        c.to_string()
    };
    Ok(KeyToken::Char(stored))
}

fn named_key_from_name(s: &str) -> Option<NamedKey> {
    let n = s.to_lowercase();
    Some(match n.as_str() {
        "enter" | "return" => NamedKey::Enter,
        "escape" | "esc" => NamedKey::Escape,
        "tab" => NamedKey::Tab,
        "space" => NamedKey::Space,
        "backspace" => NamedKey::Backspace,
        "delete" | "del" => NamedKey::Delete,
        "pageup" => NamedKey::PageUp,
        "pagedown" => NamedKey::PageDown,
        "home" => NamedKey::Home,
        "end" => NamedKey::End,
        "arrowup" | "up" => NamedKey::ArrowUp,
        "arrowdown" | "down" => NamedKey::ArrowDown,
        "arrowleft" | "left" => NamedKey::ArrowLeft,
        "arrowright" | "right" => NamedKey::ArrowRight,
        "f1" => NamedKey::F1,
        "f2" => NamedKey::F2,
        "f3" => NamedKey::F3,
        "f4" => NamedKey::F4,
        "f5" => NamedKey::F5,
        "f6" => NamedKey::F6,
        "f7" => NamedKey::F7,
        "f8" => NamedKey::F8,
        "f9" => NamedKey::F9,
        "f10" => NamedKey::F10,
        "f11" => NamedKey::F11,
        "f12" => NamedKey::F12,
        _ => return None,
    })
}

/// Build a `KeyToken` from a live winit `Key`, applying the same lowercase rule
/// used for primary tokens. Returns `None` for keys we never bind.
pub fn token_from_key(key: &Key, lowercase: bool) -> Option<KeyToken> {
    match key {
        Key::Character(s) => {
            let stored = if lowercase {
                s.to_lowercase()
            } else {
                s.to_string()
            };
            Some(KeyToken::Char(stored))
        }
        Key::Named(n) => Some(KeyToken::Named(*n)),
        _ => None,
    }
}

/// Runtime context a few actions need at build time.
#[derive(Debug, Clone, Copy)]
pub struct DispatchCtx {
    pub grid_rows: usize,
    /// Used by `cycle_mode` (Ctrl+.) to pick the next mode.
    pub mode: InputModeKind,
}

/// A `Copy` projection of `InputMode` carrying only what the registry needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputModeKind {
    Insert,
    Normal,
    Visual,
    Other,
}

impl InputModeKind {
    pub fn of(mode: &InputMode) -> Self {
        match mode {
            InputMode::Insert => InputModeKind::Insert,
            InputMode::Normal => InputModeKind::Normal,
            InputMode::Visual { .. } => InputModeKind::Visual,
            _ => InputModeKind::Other,
        }
    }
}

fn visual_mode_init() -> InputMode {
    InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 0,
        cur_row: 0,
        anchored: false,
    }
}

fn cycle_mode_next(kind: InputModeKind) -> InputMode {
    match kind {
        InputModeKind::Insert => InputMode::Normal,
        InputModeKind::Normal => visual_mode_init(),
        _ => InputMode::Insert,
    }
}

/// Map a user-bindable action name to a concrete `Action`. Returns `None` for
/// unknown names (the validator turns that into `KeymapError::UnknownAction`).
/// `"none"` is intentionally NOT here — it is the reserved disable keyword.
pub fn action_from_name(name: &str, ctx: DispatchCtx) -> Option<Action> {
    let a = match name {
        // clipboard
        "paste" => Action::Paste,
        "copy" => Action::Copy,
        // tabs
        "new_tab" => Action::NewTab,
        "close_tab" => Action::CloseTab,
        "next_tab" => Action::NextTab,
        "prev_tab" => Action::PrevTab,
        "move_tab_left" => Action::MoveTabLeft,
        "move_tab_right" => Action::MoveTabRight,
        "rename_tab" => Action::RenameTab,
        "go_to_tab_1" => Action::GoToTab(0),
        "go_to_tab_2" => Action::GoToTab(1),
        "go_to_tab_3" => Action::GoToTab(2),
        "go_to_tab_4" => Action::GoToTab(3),
        "go_to_tab_5" => Action::GoToTab(4),
        "go_to_tab_6" => Action::GoToTab(5),
        "go_to_tab_7" => Action::GoToTab(6),
        "go_to_tab_8" => Action::GoToTab(7),
        "go_to_tab_9" => Action::GoToTab(8),
        // panes
        "split_horizontal" => Action::SplitH,
        "split_vertical" => Action::SplitV,
        "auto_split" => Action::AutoSplit,
        "close_pane" => Action::ClosePane,
        "focus_left" => Action::FocusLeft,
        "focus_right" => Action::FocusRight,
        "focus_up" => Action::FocusUp,
        "focus_down" => Action::FocusDown,
        "focus_next" => Action::FocusNext,
        "zoom_pane" => Action::ZoomPane,
        "rotate_panes_forward" => Action::RotatePanesForward,
        "rotate_panes_backward" => Action::RotatePanesBackward,
        "resize_pane_right" => Action::ResizePaneRight,
        "resize_pane_left" => Action::ResizePaneLeft,
        "resize_pane_down" => Action::ResizePaneDown,
        "resize_pane_up" => Action::ResizePaneUp,
        // scroll
        "scroll_page_up" => Action::ScrollUp(ctx.grid_rows),
        "scroll_page_down" => Action::ScrollDown(ctx.grid_rows),
        "scroll_to_top" => Action::ScrollToTop,
        "scroll_to_bottom" => Action::ScrollToBottom,
        "clear_scrollback" => Action::ClearScrollback,
        // search
        "search_open" => Action::SearchOpen,
        "search_next" => Action::SearchNext,
        "search_prev" => Action::SearchPrev,
        // font size
        "increase_font_size" => Action::IncreaseFontSize,
        "decrease_font_size" => Action::DecreaseFontSize,
        "reset_font_size" => Action::ResetFontSize,
        // ui / app
        "open_config" => Action::OpenConfig,
        "open_command_palette" => Action::OpenCommandPalette,
        "toggle_fullscreen" => Action::ToggleFullscreen,
        "toggle_log" => Action::ToggleLog,
        "toggle_passthrough" => Action::TogglePassthrough,
        "screenshot_open" => Action::ScreenshotOpen,
        "quit" => Action::Quit,
        // modes
        "cycle_mode" => Action::SetMode(cycle_mode_next(ctx.mode)),
        "enter_normal_mode" => Action::SetMode(InputMode::Normal),
        // pane chord prefix
        "ctrl_w_prefix" => Action::CtrlWPrefix,
        _ => return None,
    };
    Some(a)
}

/// Reverse map: the canonical name for an `Action`, for docs / validation
/// messages. Parameterized + internal actions return `None` (they are not a
/// single stable user name) except where a fixed name exists.
#[cfg(test)]
pub fn name_of_action(action: &Action) -> Option<&'static str> {
    Some(match action {
        Action::Paste => "paste",
        Action::Copy => "copy",
        Action::NewTab => "new_tab",
        Action::CloseTab => "close_tab",
        Action::NextTab => "next_tab",
        Action::PrevTab => "prev_tab",
        Action::MoveTabLeft => "move_tab_left",
        Action::MoveTabRight => "move_tab_right",
        Action::RenameTab => "rename_tab",
        Action::SplitH => "split_horizontal",
        Action::SplitV => "split_vertical",
        Action::AutoSplit => "auto_split",
        Action::ClosePane => "close_pane",
        Action::FocusLeft => "focus_left",
        Action::FocusRight => "focus_right",
        Action::FocusUp => "focus_up",
        Action::FocusDown => "focus_down",
        Action::FocusNext => "focus_next",
        Action::ZoomPane => "zoom_pane",
        Action::RotatePanesForward => "rotate_panes_forward",
        Action::RotatePanesBackward => "rotate_panes_backward",
        Action::ResizePaneRight => "resize_pane_right",
        Action::ResizePaneLeft => "resize_pane_left",
        Action::ResizePaneDown => "resize_pane_down",
        Action::ResizePaneUp => "resize_pane_up",
        Action::ScrollToTop => "scroll_to_top",
        Action::ScrollToBottom => "scroll_to_bottom",
        Action::ClearScrollback => "clear_scrollback",
        Action::SearchOpen => "search_open",
        Action::SearchNext => "search_next",
        Action::SearchPrev => "search_prev",
        Action::IncreaseFontSize => "increase_font_size",
        Action::DecreaseFontSize => "decrease_font_size",
        Action::ResetFontSize => "reset_font_size",
        Action::OpenConfig => "open_config",
        Action::OpenCommandPalette => "open_command_palette",
        Action::ToggleFullscreen => "toggle_fullscreen",
        Action::ToggleLog => "toggle_log",
        Action::TogglePassthrough => "toggle_passthrough",
        Action::ScreenshotOpen => "screenshot_open",
        Action::Quit => "quit",
        Action::CtrlWPrefix => "ctrl_w_prefix",
        _ => return None,
    })
}

/// The merged binding table. Values are registry action names (`&'static str`).
#[derive(Debug, Clone, Default)]
pub struct KeyMap {
    map: HashMap<(ModeClass, BindingKey), &'static str>,
}

impl KeyMap {
    fn insert(&mut self, scope: ModeClass, key: BindingKey, name: &'static str) {
        self.map.insert((scope, key), name);
    }

    pub fn lookup(&self, scope: ModeClass, key: &BindingKey) -> Option<&'static str> {
        self.map.get(&(scope, key.clone())).copied()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl KeyMap {
    /// Build the merged keymap: defaults overlaid with the user's config.
    /// Returns the map + any per-entry errors (skipped entries).
    pub fn from_config(cfg: &KeybindingsConfig) -> (KeyMap, Vec<KeymapError>) {
        let mut km = default_keymap();
        let mut errors = Vec::new();

        for (raw, action) in &cfg.0 {
            let (scope, key) = match parse_binding(raw) {
                Ok(v) => v,
                Err(reason) => {
                    errors.push(KeymapError::Parse {
                        raw: raw.clone(),
                        reason,
                    });
                    continue;
                }
            };

            // "none" disables a default → remove the entry.
            if action == "none" {
                km.map.remove(&(scope, key));
                continue;
            }

            // Shadows-input guard: bare unmodified single-char in Global scope.
            if scope == ModeClass::Global
                && key.mods == Mods::default()
                && key.chord_tail.is_none()
                && matches!(key.token, KeyToken::Char(_))
            {
                errors.push(KeymapError::ShadowsInput { raw: raw.clone() });
                continue;
            }

            // Validate the action name against the registry. Use a probe ctx;
            // names that depend on ctx still resolve to Some(_) here.
            let probe = DispatchCtx {
                grid_rows: 1,
                mode: InputModeKind::Insert,
            };
            if action_from_name(action, probe).is_none() {
                errors.push(KeymapError::UnknownAction {
                    raw: raw.clone(),
                    name: action.clone(),
                });
                continue;
            }

            // The validator guaranteed `action_from_name` returned `Some`, so the
            // name is one of the known registry names and `intern_known_name`
            // resolves it to a `&'static str` (kept in sync with the registry).
            let static_name = intern_known_name(action)
                .expect("validated name must be in NAMES — keep intern_known_name in sync");
            km.insert(scope, key, static_name);
        }

        (km, errors)
    }
}

/// Intern a validated action name to `&'static str`. Covers every name accepted
/// by `action_from_name`, including the parameterized ones that `name_of_action`
/// cannot reverse. Keep in sync with `action_from_name`.
fn intern_known_name(name: &str) -> Option<&'static str> {
    const NAMES: &[&str] = &[
        "paste",
        "copy",
        "new_tab",
        "close_tab",
        "next_tab",
        "prev_tab",
        "move_tab_left",
        "move_tab_right",
        "rename_tab",
        "go_to_tab_1",
        "go_to_tab_2",
        "go_to_tab_3",
        "go_to_tab_4",
        "go_to_tab_5",
        "go_to_tab_6",
        "go_to_tab_7",
        "go_to_tab_8",
        "go_to_tab_9",
        "split_horizontal",
        "split_vertical",
        "auto_split",
        "close_pane",
        "focus_left",
        "focus_right",
        "focus_up",
        "focus_down",
        "focus_next",
        "zoom_pane",
        "rotate_panes_forward",
        "rotate_panes_backward",
        "resize_pane_right",
        "resize_pane_left",
        "resize_pane_down",
        "resize_pane_up",
        "scroll_page_up",
        "scroll_page_down",
        "scroll_to_top",
        "scroll_to_bottom",
        "clear_scrollback",
        "search_open",
        "search_next",
        "search_prev",
        "increase_font_size",
        "decrease_font_size",
        "reset_font_size",
        "open_config",
        "open_command_palette",
        "toggle_fullscreen",
        "toggle_log",
        "toggle_passthrough",
        "screenshot_open",
        "quit",
        "cycle_mode",
        "enter_normal_mode",
        "ctrl_w_prefix",
    ];
    NAMES.iter().copied().find(|&n| n == name)
}

fn ch(c: &str) -> KeyToken {
    KeyToken::Char(c.into())
}

fn nk(n: NamedKey) -> KeyToken {
    KeyToken::Named(n)
}

const M_CTRL: Mods = Mods {
    ctrl: true,
    shift: false,
    alt: false,
    cmd: false,
};
const M_CTRL_SHIFT: Mods = Mods {
    ctrl: true,
    shift: true,
    alt: false,
    cmd: false,
};
const M_ALT: Mods = Mods {
    ctrl: false,
    shift: false,
    alt: true,
    cmd: false,
};
const M_SHIFT: Mods = Mods {
    ctrl: false,
    shift: true,
    alt: false,
    cmd: false,
};
const M_CMD: Mods = Mods {
    ctrl: false,
    shift: false,
    alt: false,
    cmd: true,
};

/// All built-in Global-scope shortcut bindings, as data. Mirrors today's
/// `handle_global_shortcuts` + `ctrl_w_action` + the curated `cmd` set.
///
/// Intentionally left to the `keybindings.rs` fallthrough (NOT in this table):
/// - `Ctrl+C` copy-in-Visual: mode-conditional, handled by `handle_ctrl_only`.
/// - Chord tails are stored case-preserved (only `R` differs from `r`). The
///   Task-4 dispatcher MUST lowercase the live tail token before lookup so that
///   shifted tails like `Ctrl+W V` still match `Ctrl+W v`, while passing the
///   exact glyph for the `R`/`r` distinction. (Design decision #4 in the plan.)
pub fn default_keymap() -> KeyMap {
    let mut km = KeyMap::default();
    {
        let mut g = |m: Mods, t: KeyToken, name: &'static str| {
            km.insert(
                ModeClass::Global,
                BindingKey {
                    mods: m,
                    token: t,
                    chord_tail: None,
                },
                name,
            )
        };

        // ── Ctrl (handle_ctrl_only + ctrl_char_action) ───────────────────────
        g(M_CTRL, ch("w"), "ctrl_w_prefix");
        g(M_CTRL, ch("q"), "quit");
        g(M_CTRL, ch(","), "open_config");
        g(M_CTRL, ch("t"), "new_tab");
        g(M_CTRL, ch("b"), "toggle_passthrough");
        g(M_CTRL, ch("+"), "increase_font_size");
        g(M_CTRL, ch("="), "increase_font_size");
        g(M_CTRL, ch("-"), "decrease_font_size");
        g(M_CTRL, ch("0"), "reset_font_size");
        g(M_CTRL, nk(NamedKey::PageUp), "prev_tab");
        g(M_CTRL, nk(NamedKey::PageDown), "next_tab");
        g(M_CTRL, nk(NamedKey::Enter), "toggle_fullscreen");
        // Ctrl+. cycles mode; Ctrl+\ and Ctrl+| enter Normal.
        g(M_CTRL, ch("."), "cycle_mode");
        g(M_CTRL, ch("\\"), "enter_normal_mode");
        g(M_CTRL, ch("|"), "enter_normal_mode");

        // ── Ctrl+Shift (ctrl_shift_action) ───────────────────────────────────────
        g(M_CTRL_SHIFT, ch("v"), "paste");
        g(M_CTRL_SHIFT, ch("w"), "close_tab");
        g(M_CTRL_SHIFT, ch("r"), "rename_tab");
        g(M_CTRL_SHIFT, ch("k"), "clear_scrollback");
        g(M_CTRL_SHIFT, ch("l"), "toggle_log");
        g(M_CTRL_SHIFT, ch("p"), "open_command_palette");
        g(M_CTRL_SHIFT, nk(NamedKey::ArrowUp), "resize_pane_up");
        g(M_CTRL_SHIFT, nk(NamedKey::ArrowDown), "resize_pane_down");
        g(M_CTRL_SHIFT, nk(NamedKey::ArrowRight), "resize_pane_right");
        g(M_CTRL_SHIFT, nk(NamedKey::ArrowLeft), "resize_pane_left");
        g(M_CTRL_SHIFT, nk(NamedKey::PageUp), "move_tab_left");
        g(M_CTRL_SHIFT, nk(NamedKey::PageDown), "move_tab_right");
        g(M_CTRL_SHIFT, nk(NamedKey::Home), "scroll_to_top");
        g(M_CTRL_SHIFT, nk(NamedKey::End), "scroll_to_bottom");

        // ── Shift (shift_scroll_action) ──────────────────────────────────────────
        g(M_SHIFT, nk(NamedKey::PageUp), "scroll_page_up");
        g(M_SHIFT, nk(NamedKey::PageDown), "scroll_page_down");

        // ── Alt (alt_action: Alt+1..9 → go_to_tab) ───────────────────────────────
        g(M_ALT, ch("1"), "go_to_tab_1");
        g(M_ALT, ch("2"), "go_to_tab_2");
        g(M_ALT, ch("3"), "go_to_tab_3");
        g(M_ALT, ch("4"), "go_to_tab_4");
        g(M_ALT, ch("5"), "go_to_tab_5");
        g(M_ALT, ch("6"), "go_to_tab_6");
        g(M_ALT, ch("7"), "go_to_tab_7");
        g(M_ALT, ch("8"), "go_to_tab_8");
        g(M_ALT, ch("9"), "go_to_tab_9");

        // ── Cmd / Super (cmd_char_action) ────────────────────────────────────────
        g(M_CMD, ch("v"), "paste");
        g(M_CMD, ch("c"), "copy");
        g(M_CMD, ch("n"), "new_tab");
        g(M_CMD, ch("t"), "new_tab");
        g(M_CMD, ch("w"), "close_tab");
        g(M_CMD, ch("q"), "quit");
        g(M_CMD, ch(","), "open_config");
        g(M_CMD, ch("f"), "search_open");
        g(M_CMD, ch("k"), "clear_scrollback");
        g(M_CMD, ch("+"), "increase_font_size");
        g(M_CMD, ch("-"), "decrease_font_size");
        g(M_CMD, ch("="), "reset_font_size");
        g(M_CMD, ch("0"), "reset_font_size");
        g(M_CMD, ch("1"), "go_to_tab_1");
        g(M_CMD, ch("2"), "go_to_tab_2");
        g(M_CMD, ch("3"), "go_to_tab_3");
        g(M_CMD, ch("4"), "go_to_tab_4");
        g(M_CMD, ch("5"), "go_to_tab_5");
        g(M_CMD, ch("6"), "go_to_tab_6");
        g(M_CMD, ch("7"), "go_to_tab_7");
        g(M_CMD, ch("8"), "go_to_tab_8");
        g(M_CMD, ch("9"), "go_to_tab_9");
    }

    // ── Ctrl+W chords (ctrl_w_action) ────────────────────────────────────────
    {
        let no_mods = Mods::default();
        let mut chord = |tail: KeyToken, name: &'static str| {
            km.insert(
                ModeClass::Global,
                BindingKey {
                    mods: M_CTRL,
                    token: ch("w"),
                    chord_tail: Some((no_mods, tail)),
                },
                name,
            )
        };
        chord(ch("v"), "split_horizontal");
        chord(ch("s"), "split_vertical");
        chord(ch("a"), "auto_split");
        chord(ch("h"), "focus_left");
        chord(ch("l"), "focus_right");
        chord(ch("k"), "focus_up");
        chord(ch("j"), "focus_down");
        chord(ch("w"), "focus_next");
        chord(ch("q"), "close_pane");
        chord(ch("z"), "zoom_pane");
        chord(ch("r"), "rotate_panes_forward");
        chord(ch("R"), "rotate_panes_backward"); // uppercase, case-preserved tail
        chord(ch("p"), "screenshot_open");
        chord(nk(NamedKey::ArrowLeft), "focus_left");
        chord(nk(NamedKey::ArrowRight), "focus_right");
        chord(nk(NamedKey::ArrowUp), "focus_up");
        chord(nk(NamedKey::ArrowDown), "focus_down");
    }

    km
}

#[cfg(test)]
#[path = "keymap_test.rs"]
mod tests;
