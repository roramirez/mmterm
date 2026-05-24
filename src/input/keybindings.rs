use winit::event::{ElementState, KeyEvent, Modifiers};
use winit::keyboard::{Key, NamedKey};

use super::mode::InputMode;

pub enum Action {
    SendToPty(Vec<u8>),
    SetMode(InputMode),
    Paste,
    Copy,
    ScrollUp(usize),
    ScrollDown(usize),
    ScrollToTop,
    ScrollToBottom,
    SplitH,
    SplitV,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    FocusNext,
    ClosePane,
    ZoomPane,
    ToggleFullscreen,
    CtrlWPrefix,
    OpenConfig,
    NewTab,
    NextTab,
    PrevTab,
    GoToTab(usize),
    MoveTabLeft,
    MoveTabRight,
    CloseTab,
    RenameTab,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    SearchOpen,
    SearchNext,
    SearchPrev,
    VisualSwapAnchor,
    VisualAnchor,
    VisualBoundaryUp(usize),
    VisualBoundaryDown(usize),
    VisualWordForward,
    VisualWordBackward,
    VisualWordEnd,
    VisualYankLine,
    ClearScrollback,
    ToggleLog,
    ResizePaneRight,
    ResizePaneLeft,
    ResizePaneDown,
    ResizePaneUp,
    Quit,
    None,
}

pub fn handle_key(
    event: &KeyEvent,
    modifiers: &Modifiers,
    mode: &InputMode,
    grid_cols: usize,
    grid_rows: usize,
    application_cursor_keys: bool,
) -> Action {
    if event.state != ElementState::Pressed {
        return Action::None;
    }
    let ctrl = modifiers.state().control_key();
    let shift = modifiers.state().shift_key();
    let alt = modifiers.state().alt_key();
    handle_key_inner(
        &event.logical_key,
        ctrl,
        shift,
        alt,
        mode,
        grid_cols,
        grid_rows,
        application_cursor_keys,
    )
}

pub fn handle_ctrl_w(event: &KeyEvent) -> Action {
    if event.state != ElementState::Pressed {
        return Action::None;
    }
    ctrl_w_action(&event.logical_key)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_key_inner(
    key: &Key,
    ctrl: bool,
    shift: bool,
    alt: bool,
    mode: &InputMode,
    grid_cols: usize,
    grid_rows: usize,
    application_cursor_keys: bool,
) -> Action {
    // ── Global shortcuts (all modes, never sent to PTY) ──────────────────

    // Ctrl+W — pane management prefix
    if ctrl
        && !shift
        && let Key::Character(s) = key
        && s.eq_ignore_ascii_case("w")
    {
        return Action::CtrlWPrefix;
    }

    // Ctrl+. — cycle Insert → Normal → Visual
    if ctrl && let Key::Character(s) = key {
        if s == "." {
            let next = match mode {
                InputMode::Insert => InputMode::Normal,
                InputMode::Normal => InputMode::Visual {
                    start_col: 0,
                    start_row: 0,
                    cur_col: 0,
                    cur_row: 0,
                    anchored: false,
                },
                InputMode::Visual { .. }
                | InputMode::RenameTab { .. }
                | InputMode::Search { .. } => InputMode::Insert,
            };
            return Action::SetMode(next);
        }
        // Ctrl+\ — also enters Normal mode (alternative)
        if s == "\\" || s == "|" {
            return Action::SetMode(InputMode::Normal);
        }
    }

    if ctrl && shift {
        match key {
            Key::Character(s) if s.eq_ignore_ascii_case("v") => return Action::Paste,
            Key::Character(s) if s.eq_ignore_ascii_case("w") => return Action::CloseTab,
            Key::Character(s) if s.eq_ignore_ascii_case("r") => return Action::RenameTab,
            Key::Character(s) if s.eq_ignore_ascii_case("k") => return Action::ClearScrollback,
            Key::Character(s) if s.eq_ignore_ascii_case("l") => return Action::ToggleLog,
            Key::Named(NamedKey::ArrowUp) => return Action::ResizePaneUp,
            Key::Named(NamedKey::ArrowDown) => return Action::ResizePaneDown,
            Key::Named(NamedKey::ArrowRight) => return Action::ResizePaneRight,
            Key::Named(NamedKey::ArrowLeft) => return Action::ResizePaneLeft,
            Key::Named(NamedKey::PageUp) => return Action::MoveTabLeft,
            Key::Named(NamedKey::PageDown) => return Action::MoveTabRight,
            Key::Named(NamedKey::Home) => return Action::ScrollToTop,
            Key::Named(NamedKey::End) => return Action::ScrollToBottom,
            _ => {}
        }
    }

    if ctrl
        && !shift
        && let Key::Character(s) = key
        && s.eq_ignore_ascii_case("c")
        && matches!(mode, InputMode::Visual { .. })
    {
        return Action::Copy;
    }

    if ctrl && !shift {
        match key {
            Key::Character(s) if s.eq_ignore_ascii_case("q") => return Action::Quit,
            Key::Character(s) if s == "," => return Action::OpenConfig,
            Key::Character(s) if s.eq_ignore_ascii_case("t") => return Action::NewTab,
            // Ctrl++ / Ctrl+= — increase font size
            Key::Character(s) if s == "+" || s == "=" => return Action::IncreaseFontSize,
            // Ctrl+- — decrease font size
            Key::Character(s) if s == "-" => return Action::DecreaseFontSize,
            // Ctrl+0 — reset font size
            Key::Character(s) if s == "0" => return Action::ResetFontSize,
            _ => {}
        }
        // Ctrl+PageUp/Down → tab navigation
        if *key == Key::Named(NamedKey::PageUp) {
            return Action::PrevTab;
        }
        if *key == Key::Named(NamedKey::PageDown) {
            return Action::NextTab;
        }
    }

    if shift && !ctrl {
        match key {
            Key::Named(NamedKey::PageUp) => return Action::ScrollUp(grid_rows),
            Key::Named(NamedKey::PageDown) => return Action::ScrollDown(grid_rows),
            _ => {}
        }
    }

    // Ctrl+Enter — toggle fullscreen (all modes, like Ghostty on Linux/Windows)
    if ctrl && !shift && !alt && *key == Key::Named(NamedKey::Enter) {
        return Action::ToggleFullscreen;
    }

    // Alt+Tab / Alt+Shift+Tab — consumed silently so the keystroke isn't
    // forwarded to the PTY while the window manager switches focus.
    if alt && !ctrl && *key == Key::Named(NamedKey::Tab) {
        return Action::None;
    }

    // Alt+1..9 — jump to tab by position (1-indexed)
    if alt
        && !ctrl
        && !shift
        && let Key::Character(s) = key
        && let Some(d) = s.chars().next().and_then(|c| c.to_digit(10))
        && d >= 1
    {
        return Action::GoToTab((d - 1) as usize);
    }

    // ── Per-mode handling ────────────────────────────────────────────────
    match mode {
        InputMode::Insert => handle_insert(key, ctrl, shift, alt, application_cursor_keys),
        InputMode::Normal => handle_normal(key, grid_rows),
        InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            cur_row,
            anchored,
        } => handle_visual(
            key,
            (*start_col, *start_row),
            (*cur_col, *cur_row),
            *anchored,
            grid_cols,
            grid_rows,
        ),
        InputMode::RenameTab { .. } => Action::None,
        InputMode::Search { .. } => Action::None,
    }
}

pub(crate) fn ctrl_w_action(key: &Key) -> Action {
    match key {
        Key::Character(s) => match s.to_lowercase().as_str() {
            "v" => Action::SplitH,
            "s" => Action::SplitV,
            "h" => Action::FocusLeft,
            "l" => Action::FocusRight,
            "k" => Action::FocusUp,
            "j" => Action::FocusDown,
            "w" => Action::FocusNext,
            "q" => Action::ClosePane,
            "z" => Action::ZoomPane,
            _ => Action::None,
        },
        Key::Named(NamedKey::ArrowLeft) => Action::FocusLeft,
        Key::Named(NamedKey::ArrowRight) => Action::FocusRight,
        Key::Named(NamedKey::ArrowUp) => Action::FocusUp,
        Key::Named(NamedKey::ArrowDown) => Action::FocusDown,
        _ => Action::None,
    }
}

fn handle_insert(
    key: &Key,
    ctrl: bool,
    shift: bool,
    alt: bool,
    application_cursor_keys: bool,
) -> Action {
    // Escape is forwarded to PTY — vim / other TUI apps need it
    if ctrl {
        if let Key::Character(s) = key
            && let Some(c) = s.chars().next()
        {
            let raw = c as u32;
            if (1..=26).contains(&raw) {
                return Action::SendToPty(vec![raw as u8]);
            }
            let lower = (c as u8).to_ascii_lowercase();
            if lower.is_ascii_alphabetic() {
                return Action::SendToPty(vec![lower - b'a' + 1]);
            }
        }
        if *key == Key::Named(NamedKey::Enter) {
            return Action::SendToPty(vec![b'\n']);
        }
    }

    // Alt+key: prefix with ESC (standard xterm Alt encoding)
    if alt && !ctrl {
        let inner = match key {
            Key::Named(NamedKey::Tab) => Some(vec![b'\t']),
            Key::Named(NamedKey::Enter) => Some(vec![b'\r']),
            Key::Named(NamedKey::Backspace) => Some(vec![0x7f]),
            Key::Character(s) => Some(s.as_bytes().to_vec()),
            _ => None,
        };
        if let Some(mut bytes) = inner {
            bytes.insert(0, 0x1b);
            return Action::SendToPty(bytes);
        }
    }

    match key {
        Key::Named(NamedKey::Escape) => Action::SendToPty(vec![0x1b]),
        Key::Named(NamedKey::Space) => Action::SendToPty(vec![b' ']),
        Key::Named(NamedKey::Enter) => Action::SendToPty(vec![b'\r']),
        Key::Named(NamedKey::Backspace) => Action::SendToPty(vec![0x7f]),
        Key::Named(NamedKey::Tab) if shift => Action::SendToPty(b"\x1b[Z".to_vec()),
        Key::Named(NamedKey::Tab) => Action::SendToPty(vec![b'\t']),
        Key::Named(NamedKey::ArrowUp) => Action::SendToPty(if application_cursor_keys {
            b"\x1bOA".to_vec()
        } else {
            b"\x1b[A".to_vec()
        }),
        Key::Named(NamedKey::ArrowDown) => Action::SendToPty(if application_cursor_keys {
            b"\x1bOB".to_vec()
        } else {
            b"\x1b[B".to_vec()
        }),
        Key::Named(NamedKey::ArrowRight) => Action::SendToPty(if application_cursor_keys {
            b"\x1bOC".to_vec()
        } else {
            b"\x1b[C".to_vec()
        }),
        Key::Named(NamedKey::ArrowLeft) => Action::SendToPty(if application_cursor_keys {
            b"\x1bOD".to_vec()
        } else {
            b"\x1b[D".to_vec()
        }),
        Key::Named(NamedKey::Home) => Action::SendToPty(if application_cursor_keys {
            b"\x1bOH".to_vec()
        } else {
            b"\x1b[1~".to_vec()
        }),
        Key::Named(NamedKey::End) => Action::SendToPty(if application_cursor_keys {
            b"\x1bOF".to_vec()
        } else {
            b"\x1b[4~".to_vec()
        }),
        Key::Named(NamedKey::PageUp) => Action::SendToPty(b"\x1b[5~".to_vec()),
        Key::Named(NamedKey::PageDown) => Action::SendToPty(b"\x1b[6~".to_vec()),
        Key::Named(NamedKey::Delete) => Action::SendToPty(b"\x1b[3~".to_vec()),
        Key::Named(NamedKey::F1) => Action::SendToPty(b"\x1bOP".to_vec()),
        Key::Named(NamedKey::F2) => Action::SendToPty(b"\x1bOQ".to_vec()),
        Key::Named(NamedKey::F3) => Action::SendToPty(b"\x1bOR".to_vec()),
        Key::Named(NamedKey::F4) => Action::SendToPty(b"\x1bOS".to_vec()),
        Key::Named(NamedKey::F5) => Action::SendToPty(b"\x1b[15~".to_vec()),
        Key::Named(NamedKey::F6) => Action::SendToPty(b"\x1b[17~".to_vec()),
        Key::Named(NamedKey::F7) => Action::SendToPty(b"\x1b[18~".to_vec()),
        Key::Named(NamedKey::F8) => Action::SendToPty(b"\x1b[19~".to_vec()),
        Key::Named(NamedKey::F9) => Action::SendToPty(b"\x1b[20~".to_vec()),
        Key::Named(NamedKey::F10) => Action::SendToPty(b"\x1b[21~".to_vec()),
        Key::Named(NamedKey::F11) => Action::SendToPty(b"\x1b[23~".to_vec()),
        Key::Named(NamedKey::F12) => Action::SendToPty(b"\x1b[24~".to_vec()),
        Key::Character(s) => Action::SendToPty(s.as_bytes().to_vec()),
        _ => Action::None,
    }
}

fn handle_normal(key: &Key, grid_rows: usize) -> Action {
    match key {
        Key::Named(NamedKey::Escape) => Action::SetMode(InputMode::Insert),
        Key::Named(NamedKey::PageUp) => Action::ScrollUp(grid_rows),
        Key::Named(NamedKey::PageDown) => Action::ScrollDown(grid_rows),
        Key::Character(s) => match s.as_str() {
            "i" => Action::SetMode(InputMode::Insert),
            "v" => Action::SetMode(InputMode::Visual {
                start_col: 0,
                start_row: 0,
                cur_col: 0,
                cur_row: 0,
                anchored: false,
            }),
            "q" => Action::ClosePane,
            "/" => Action::SearchOpen,
            "n" => Action::SearchNext,
            "N" => Action::SearchPrev,
            "j" => Action::ScrollDown(3),
            "k" => Action::ScrollUp(3),
            _ => Action::None,
        },
        _ => Action::None,
    }
}

fn handle_visual(
    key: &Key,
    (start_col, start_row): (usize, usize),
    (cur_col, cur_row): (usize, usize),
    anchored: bool,
    cols: usize,
    rows: usize,
) -> Action {
    let rows = rows.saturating_sub(1);
    let cols = cols.saturating_sub(1);

    let move_to = |nc: usize, nr: usize| {
        Action::SetMode(InputMode::Visual {
            start_col,
            start_row,
            cur_col: nc.min(cols),
            cur_row: nr.min(rows),
            anchored,
        })
    };

    match key {
        Key::Named(NamedKey::Escape) => Action::SetMode(InputMode::Insert),
        Key::Named(NamedKey::ArrowLeft) => move_to(cur_col.saturating_sub(1), cur_row),
        Key::Named(NamedKey::ArrowRight) => move_to((cur_col + 1).min(cols), cur_row),
        Key::Named(NamedKey::ArrowUp) => {
            if cur_row == 0 {
                Action::VisualBoundaryUp(1)
            } else {
                move_to(cur_col, cur_row - 1)
            }
        }
        Key::Named(NamedKey::ArrowDown) => {
            if cur_row == rows {
                Action::VisualBoundaryDown(1)
            } else {
                move_to(cur_col, cur_row + 1)
            }
        }
        Key::Named(NamedKey::Home) => move_to(0, cur_row),
        Key::Named(NamedKey::End) => move_to(cols, cur_row),
        Key::Character(s) => match s.as_str() {
            "h" => move_to(cur_col.saturating_sub(1), cur_row),
            "l" => move_to((cur_col + 1).min(cols), cur_row),
            "k" => {
                if cur_row == 0 {
                    Action::VisualBoundaryUp(1)
                } else {
                    move_to(cur_col, cur_row - 1)
                }
            }
            "j" => {
                if cur_row == rows {
                    Action::VisualBoundaryDown(1)
                } else {
                    move_to(cur_col, cur_row + 1)
                }
            }
            "0" => move_to(0, cur_row),
            "$" => move_to(cols, cur_row),
            "g" => move_to(cur_col, 0),
            "G" => move_to(cur_col, rows),
            "w" => Action::VisualWordForward,
            "b" => Action::VisualWordBackward,
            "e" => Action::VisualWordEnd,
            "y" => Action::Copy,
            "Y" => Action::VisualYankLine,
            "o" => Action::VisualSwapAnchor,
            // set anchor at current cursor position (start marking from here)
            "v" => Action::VisualAnchor,
            "q" => Action::SetMode(InputMode::Insert),
            _ => Action::None,
        },
        _ => Action::None,
    }
}

#[cfg(test)]
#[path = "keybindings_test.rs"]
mod tests;
