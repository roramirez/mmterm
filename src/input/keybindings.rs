use winit::event::{ElementState, KeyEvent, Modifiers};
use winit::keyboard::{Key, NamedKey};

use super::mode::InputMode;

pub enum Action {
    SendToPty(Vec<u8>),
    SetMode(InputMode),
    Paste,
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
    CtrlWPrefix,
    OpenConfig,
    NewTab,
    NextTab,
    PrevTab,
    CloseTab,
    RenameTab,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    Quit,
    None,
}

pub fn handle_key(
    event: &KeyEvent,
    modifiers: &Modifiers,
    mode: &InputMode,
    grid_cols: usize,
    grid_rows: usize,
) -> Action {
    if event.state != ElementState::Pressed {
        return Action::None;
    }

    let ctrl  = modifiers.state().control_key();
    let shift = modifiers.state().shift_key();

    // ── Global shortcuts (all modes, never sent to PTY) ──────────────────

    // Ctrl+W — pane management prefix
    if ctrl && !shift {
        if let Key::Character(s) = &event.logical_key {
            if s.eq_ignore_ascii_case("w") {
                return Action::CtrlWPrefix;
            }
        }
    }

    // Ctrl+. — cycle Insert → Normal → Visual
    if ctrl {
        if let Key::Character(s) = &event.logical_key {
            if s == "." {
                let next = match mode {
                    InputMode::Insert    => InputMode::Normal,
                    InputMode::Normal    => InputMode::Visual {
                        start_col: 0, start_row: 0, cur_col: 0, cur_row: 0,
                    },
                    InputMode::Visual { .. } | InputMode::RenameTab { .. } => InputMode::Insert,
                };
                return Action::SetMode(next);
            }
            // Ctrl+\ — also enters Normal mode (alternative)
            if s == "\\" || s == "|" {
                return Action::SetMode(InputMode::Normal);
            }
        }
    }

    if ctrl && shift {
        match &event.logical_key {
            Key::Character(s) if s.eq_ignore_ascii_case("v") => return Action::Paste,
            Key::Character(s) if s.eq_ignore_ascii_case("w") => return Action::CloseTab,
            Key::Character(s) if s.eq_ignore_ascii_case("r") => return Action::RenameTab,
            Key::Named(NamedKey::ArrowUp)   => return Action::ScrollUp(1),
            Key::Named(NamedKey::ArrowDown) => return Action::ScrollDown(1),
            Key::Named(NamedKey::PageUp)    => return Action::ScrollUp(grid_rows / 2),
            Key::Named(NamedKey::PageDown)  => return Action::ScrollDown(grid_rows / 2),
            Key::Named(NamedKey::Home)      => return Action::ScrollToTop,
            Key::Named(NamedKey::End)       => return Action::ScrollToBottom,
            _ => {}
        }
    }

    if ctrl && !shift {
        match &event.logical_key {
            Key::Character(s) if s.eq_ignore_ascii_case("q") => return Action::Quit,
            Key::Character(s) if s == ","                    => return Action::OpenConfig,
            Key::Character(s) if s.eq_ignore_ascii_case("t") => return Action::NewTab,
            // Ctrl++ / Ctrl+= — increase font size
            Key::Character(s) if s == "+" || s == "="        => return Action::IncreaseFontSize,
            // Ctrl+- — decrease font size
            Key::Character(s) if s == "-"                    => return Action::DecreaseFontSize,
            // Ctrl+0 — reset font size
            Key::Character(s) if s == "0"                    => return Action::ResetFontSize,
            _ => {}
        }
        // Ctrl+PageUp/Down → tab navigation
        if event.logical_key == Key::Named(NamedKey::PageUp) {
            return Action::PrevTab;
        }
        if event.logical_key == Key::Named(NamedKey::PageDown) {
            return Action::NextTab;
        }
    }

    if shift && !ctrl {
        match &event.logical_key {
            Key::Named(NamedKey::PageUp)   => return Action::ScrollUp(grid_rows / 2),
            Key::Named(NamedKey::PageDown) => return Action::ScrollDown(grid_rows / 2),
            _ => {}
        }
    }

    // ── Per-mode handling ────────────────────────────────────────────────
    match mode {
        InputMode::Insert => handle_insert(event, ctrl),
        InputMode::Normal => handle_normal(event, ctrl),
        InputMode::Visual { start_col, start_row, cur_col, cur_row } => {
            handle_visual(event, *start_col, *start_row, *cur_col, *cur_row, grid_cols, grid_rows)
        }
        InputMode::RenameTab { .. } => Action::None,
    }
}

pub fn handle_ctrl_w(event: &KeyEvent) -> Action {
    if event.state != ElementState::Pressed { return Action::None; }
    match &event.logical_key {
        Key::Character(s) => match s.to_lowercase().as_str() {
            "v" => Action::SplitH,
            "s" => Action::SplitV,
            "h" => Action::FocusLeft,
            "l" => Action::FocusRight,
            "k" => Action::FocusUp,
            "j" => Action::FocusDown,
            "w" => Action::FocusNext,
            "q" => Action::ClosePane,
            _   => Action::None,
        },
        Key::Named(NamedKey::ArrowLeft)  => Action::FocusLeft,
        Key::Named(NamedKey::ArrowRight) => Action::FocusRight,
        Key::Named(NamedKey::ArrowUp)    => Action::FocusUp,
        Key::Named(NamedKey::ArrowDown)  => Action::FocusDown,
        _ => Action::None,
    }
}

fn handle_insert(event: &KeyEvent, ctrl: bool) -> Action {
    // Escape is forwarded to PTY — vim / other TUI apps need it
    if ctrl {
        if let Key::Character(s) = &event.logical_key {
            if let Some(c) = s.chars().next() {
                let raw = c as u32;
                if raw >= 1 && raw <= 26 {
                    return Action::SendToPty(vec![raw as u8]);
                }
                let lower = (c as u8).to_ascii_lowercase();
                if lower.is_ascii_alphabetic() {
                    return Action::SendToPty(vec![lower - b'a' + 1]);
                }
            }
        }
        if event.logical_key == Key::Named(NamedKey::Enter) {
            return Action::SendToPty(vec![b'\n']);
        }
    }

    match &event.logical_key {
        Key::Named(NamedKey::Escape)    => Action::SendToPty(vec![0x1b]),
        Key::Named(NamedKey::Space)     => Action::SendToPty(vec![b' ']),
        Key::Named(NamedKey::Enter)     => Action::SendToPty(vec![b'\r']),
        Key::Named(NamedKey::Backspace) => Action::SendToPty(vec![0x7f]),
        Key::Named(NamedKey::Tab)       => Action::SendToPty(vec![b'\t']),
        Key::Named(NamedKey::ArrowUp)   => Action::SendToPty(b"\x1b[A".to_vec()),
        Key::Named(NamedKey::ArrowDown) => Action::SendToPty(b"\x1b[B".to_vec()),
        Key::Named(NamedKey::ArrowRight)=> Action::SendToPty(b"\x1b[C".to_vec()),
        Key::Named(NamedKey::ArrowLeft) => Action::SendToPty(b"\x1b[D".to_vec()),
        Key::Named(NamedKey::Home)      => Action::SendToPty(b"\x1b[H".to_vec()),
        Key::Named(NamedKey::End)       => Action::SendToPty(b"\x1b[F".to_vec()),
        Key::Named(NamedKey::PageUp)    => Action::SendToPty(b"\x1b[5~".to_vec()),
        Key::Named(NamedKey::PageDown)  => Action::SendToPty(b"\x1b[6~".to_vec()),
        Key::Named(NamedKey::Delete)    => Action::SendToPty(b"\x1b[3~".to_vec()),
        Key::Named(NamedKey::F1)        => Action::SendToPty(b"\x1bOP".to_vec()),
        Key::Named(NamedKey::F2)        => Action::SendToPty(b"\x1bOQ".to_vec()),
        Key::Named(NamedKey::F3)        => Action::SendToPty(b"\x1bOR".to_vec()),
        Key::Named(NamedKey::F4)        => Action::SendToPty(b"\x1bOS".to_vec()),
        Key::Character(s)               => Action::SendToPty(s.as_bytes().to_vec()),
        _ => Action::None,
    }
}

fn handle_normal(event: &KeyEvent, _ctrl: bool) -> Action {
    // Escape or i → return to Insert (and send Escape to PTY)
    match &event.logical_key {
        Key::Named(NamedKey::Escape) => {
            return Action::SetMode(InputMode::Insert);
        }
        Key::Character(s) => match s.as_str() {
            "i" => return Action::SetMode(InputMode::Insert),
            "v" => return Action::SetMode(InputMode::Visual {
                start_col: 0, start_row: 0, cur_col: 0, cur_row: 0,
            }),
            "q" => return Action::Quit,
            _ => {}
        },
        _ => {}
    }
    Action::None
}

fn handle_visual(
    event: &KeyEvent,
    start_col: usize,
    start_row: usize,
    cur_col: usize,
    cur_row: usize,
    cols: usize,
    rows: usize,
) -> Action {
    let rows = rows.saturating_sub(1);
    let cols = cols.saturating_sub(1);

    let move_to = |nc: usize, nr: usize| Action::SetMode(InputMode::Visual {
        start_col,
        start_row,
        cur_col: nc.min(cols),
        cur_row: nr.min(rows),
    });

    match &event.logical_key {
        Key::Named(NamedKey::Escape)     => Action::SetMode(InputMode::Insert),
        Key::Named(NamedKey::ArrowLeft)  => move_to(cur_col.saturating_sub(1), cur_row),
        Key::Named(NamedKey::ArrowRight) => move_to((cur_col + 1).min(cols), cur_row),
        Key::Named(NamedKey::ArrowUp)    => move_to(cur_col, cur_row.saturating_sub(1)),
        Key::Named(NamedKey::ArrowDown)  => move_to(cur_col, (cur_row + 1).min(rows)),
        Key::Named(NamedKey::Home)       => move_to(0, cur_row),
        Key::Named(NamedKey::End)        => move_to(cols, cur_row),
        Key::Character(s) => match s.as_str() {
            "h" => move_to(cur_col.saturating_sub(1), cur_row),
            "l" => move_to((cur_col + 1).min(cols), cur_row),
            "k" => move_to(cur_col, cur_row.saturating_sub(1)),
            "j" => move_to(cur_col, (cur_row + 1).min(rows)),
            "0" => move_to(0, cur_row),
            "$" => move_to(cols, cur_row),
            "g" => move_to(cur_col, 0),
            "G" => move_to(cur_col, rows),
            "v" | "q" => Action::SetMode(InputMode::Insert),
            _ => Action::None,
        },
        _ => Action::None,
    }
}
