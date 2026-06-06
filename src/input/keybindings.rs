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
    AutoSplit,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    FocusNext,
    ClosePane,
    ZoomPane,
    RotatePanesForward,
    RotatePanesBackward,
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
    TogglePassthrough,
    OpenCommandPalette,
    ResizePaneRight,
    ResizePaneLeft,
    ResizePaneDown,
    ResizePaneUp,
    ScreenshotOpen,
    /// Move only the right edge (dw) or bottom edge (dh); left/top stays fixed.
    /// Positive = grow, negative = shrink.
    ScreenshotEdgeResize(i32, i32),
    /// Move selection center by (dx, dy) pixels.
    ScreenshotMove(i32, i32),
    ScreenshotCapture,
    Quit,
    QuitSaveSession,
    QuitNoSave,
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
    if let Some(action) = handle_global_shortcuts(key, ctrl, shift, alt, mode, grid_rows) {
        return action;
    }
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
        InputMode::CommandPalette { .. } => Action::None,
        InputMode::ScreenshotName { .. } => Action::None,
        InputMode::QuitSave => handle_quit_save(key),
        InputMode::Screenshot { .. } => handle_screenshot(key, shift),
    }
}

// ── Global shortcut sub-handlers ────────────────────────────────────────────

fn ctrl_shift_char_action(s: &str) -> Option<Action> {
    match s.to_lowercase().as_str() {
        "v" => Some(Action::Paste),
        "w" => Some(Action::CloseTab),
        "r" => Some(Action::RenameTab),
        "k" => Some(Action::ClearScrollback),
        "l" => Some(Action::ToggleLog),
        "p" => Some(Action::OpenCommandPalette),
        _ => None,
    }
}

fn ctrl_shift_action(key: &Key) -> Option<Action> {
    match key {
        Key::Character(s) => ctrl_shift_char_action(s),
        Key::Named(NamedKey::ArrowUp) => Some(Action::ResizePaneUp),
        Key::Named(NamedKey::ArrowDown) => Some(Action::ResizePaneDown),
        Key::Named(NamedKey::ArrowRight) => Some(Action::ResizePaneRight),
        Key::Named(NamedKey::ArrowLeft) => Some(Action::ResizePaneLeft),
        Key::Named(NamedKey::PageUp) => Some(Action::MoveTabLeft),
        Key::Named(NamedKey::PageDown) => Some(Action::MoveTabRight),
        Key::Named(NamedKey::Home) => Some(Action::ScrollToTop),
        Key::Named(NamedKey::End) => Some(Action::ScrollToBottom),
        _ => None,
    }
}

fn ctrl_char_key_action(s: &str) -> Option<Action> {
    match s.to_lowercase().as_str() {
        "q" => Some(Action::Quit),
        "," => Some(Action::OpenConfig),
        "t" => Some(Action::NewTab),
        "b" => Some(Action::TogglePassthrough),
        "+" | "=" => Some(Action::IncreaseFontSize),
        "-" => Some(Action::DecreaseFontSize),
        "0" => Some(Action::ResetFontSize),
        _ => None,
    }
}

fn ctrl_char_action(key: &Key, alt: bool) -> Option<Action> {
    match key {
        Key::Character(s) => ctrl_char_key_action(s),
        Key::Named(NamedKey::PageUp) => Some(Action::PrevTab),
        Key::Named(NamedKey::PageDown) => Some(Action::NextTab),
        Key::Named(NamedKey::Enter) if !alt => Some(Action::ToggleFullscreen),
        _ => None,
    }
}

fn alt_action(key: &Key, ctrl: bool, shift: bool) -> Option<Action> {
    if !ctrl && *key == Key::Named(NamedKey::Tab) {
        return Some(Action::None);
    }
    if !ctrl
        && !shift
        && let Key::Character(s) = key
        && let Some(d) = s.chars().next().and_then(|c| c.to_digit(10))
        && d >= 1
    {
        return Some(Action::GoToTab((d - 1) as usize));
    }
    None
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

fn ctrl_dot_next_mode(mode: &InputMode) -> InputMode {
    match mode {
        InputMode::Insert => InputMode::Normal,
        InputMode::Normal => visual_mode_init(),
        _ => InputMode::Insert,
    }
}

fn ctrl_special_char_action(s: &str, mode: &InputMode) -> Option<Action> {
    if s == "." {
        return Some(Action::SetMode(ctrl_dot_next_mode(mode)));
    }
    if s == "\\" || s == "|" {
        return Some(Action::SetMode(InputMode::Normal));
    }
    None
}

fn shift_scroll_action(key: &Key, grid_rows: usize) -> Option<Action> {
    match key {
        Key::Named(NamedKey::PageUp) => Some(Action::ScrollUp(grid_rows)),
        Key::Named(NamedKey::PageDown) => Some(Action::ScrollDown(grid_rows)),
        _ => None,
    }
}

fn handle_ctrl_only(key: &Key, alt: bool, mode: &InputMode) -> Option<Action> {
    if let Key::Character(s) = key {
        if s.eq_ignore_ascii_case("w") {
            return Some(Action::CtrlWPrefix);
        }
        if let Some(a) = ctrl_special_char_action(s, mode) {
            return Some(a);
        }
        if s.eq_ignore_ascii_case("c") && matches!(mode, InputMode::Visual { .. }) {
            return Some(Action::Copy);
        }
    }
    ctrl_char_action(key, alt)
}

fn handle_global_shortcuts(
    key: &Key,
    ctrl: bool,
    shift: bool,
    alt: bool,
    mode: &InputMode,
    grid_rows: usize,
) -> Option<Action> {
    if ctrl && shift {
        return ctrl_shift_action(key);
    }
    if ctrl {
        return handle_ctrl_only(key, alt, mode);
    }
    if shift && let Some(a) = shift_scroll_action(key, grid_rows) {
        return Some(a);
    }
    if alt {
        return alt_action(key, false, shift);
    }
    None
}

pub(crate) fn ctrl_w_action(key: &Key) -> Action {
    match key {
        Key::Character(s) => {
            if s.as_str() == "R" {
                return Action::RotatePanesBackward;
            }
            match s.to_lowercase().as_str() {
                "v" => Action::SplitH,
                "s" => Action::SplitV,
                "a" => Action::AutoSplit,
                "h" => Action::FocusLeft,
                "l" => Action::FocusRight,
                "k" => Action::FocusUp,
                "j" => Action::FocusDown,
                "w" => Action::FocusNext,
                "q" => Action::ClosePane,
                "z" => Action::ZoomPane,
                "r" => Action::RotatePanesForward,
                "p" => Action::ScreenshotOpen,
                _ => Action::None,
            }
        }
        Key::Named(NamedKey::ArrowLeft) => Action::FocusLeft,
        Key::Named(NamedKey::ArrowRight) => Action::FocusRight,
        Key::Named(NamedKey::ArrowUp) => Action::FocusUp,
        Key::Named(NamedKey::ArrowDown) => Action::FocusDown,
        _ => Action::None,
    }
}

/// Bypasses all mmterm shortcuts and encodes the key as raw PTY bytes (Insert mode encoding).
/// Used when passthrough mode is active. Ctrl+B is NOT handled here — the caller must
/// intercept it to exit passthrough before calling this.
pub fn handle_key_passthrough(
    event: &KeyEvent,
    modifiers: &Modifiers,
    application_cursor_keys: bool,
) -> Action {
    if event.state != ElementState::Pressed {
        return Action::None;
    }
    let ctrl = modifiers.state().control_key();
    let shift = modifiers.state().shift_key();
    let alt = modifiers.state().alt_key();
    handle_insert(
        &event.logical_key,
        ctrl,
        shift,
        alt,
        application_cursor_keys,
    )
}

// ── Insert mode sub-handlers ─────────────────────────────────────────────────

fn encode_ctrl_key(key: &Key) -> Option<Action> {
    if let Key::Character(s) = key
        && let Some(c) = s.chars().next()
    {
        let raw = c as u32;
        if (1..=26).contains(&raw) {
            return Some(Action::SendToPty(vec![raw as u8]));
        }
        let lower = (c as u8).to_ascii_lowercase();
        if lower.is_ascii_alphabetic() {
            return Some(Action::SendToPty(vec![lower - b'a' + 1]));
        }
    }
    if *key == Key::Named(NamedKey::Enter) {
        return Some(Action::SendToPty(vec![b'\n']));
    }
    None
}

fn encode_alt_key(key: &Key) -> Option<Action> {
    let mut bytes = match key {
        Key::Named(NamedKey::Tab) => vec![b'\t'],
        Key::Named(NamedKey::Enter) => vec![b'\r'],
        Key::Named(NamedKey::Backspace) => vec![0x7f],
        Key::Character(s) => s.as_bytes().to_vec(),
        _ => return None,
    };
    bytes.insert(0, 0x1b);
    Some(Action::SendToPty(bytes))
}

fn pick_seq(app: bool, app_seq: &'static [u8], vt_seq: &'static [u8]) -> &'static [u8] {
    if app { app_seq } else { vt_seq }
}

fn cursor_seq(key: &Key, app: bool) -> Option<&'static [u8]> {
    Some(match key {
        Key::Named(NamedKey::ArrowUp) => pick_seq(app, b"\x1bOA", b"\x1b[A"),
        Key::Named(NamedKey::ArrowDown) => pick_seq(app, b"\x1bOB", b"\x1b[B"),
        Key::Named(NamedKey::ArrowRight) => pick_seq(app, b"\x1bOC", b"\x1b[C"),
        Key::Named(NamedKey::ArrowLeft) => pick_seq(app, b"\x1bOD", b"\x1b[D"),
        Key::Named(NamedKey::Home) => pick_seq(app, b"\x1bOH", b"\x1b[1~"),
        Key::Named(NamedKey::End) => pick_seq(app, b"\x1bOF", b"\x1b[4~"),
        _ => return None,
    })
}

fn handle_insert(
    key: &Key,
    ctrl: bool,
    shift: bool,
    alt: bool,
    application_cursor_keys: bool,
) -> Action {
    if ctrl && let Some(action) = encode_ctrl_key(key) {
        return action;
    }
    if alt
        && !ctrl
        && let Some(action) = encode_alt_key(key)
    {
        return action;
    }
    if let Some(seq) = cursor_seq(key, application_cursor_keys) {
        return Action::SendToPty(seq.to_vec());
    }
    match key {
        Key::Named(NamedKey::Escape) => Action::SendToPty(vec![0x1b]),
        Key::Named(NamedKey::Space) => Action::SendToPty(vec![b' ']),
        Key::Named(NamedKey::Enter) => Action::SendToPty(vec![b'\r']),
        Key::Named(NamedKey::Backspace) => Action::SendToPty(vec![0x7f]),
        Key::Named(NamedKey::Tab) if shift => Action::SendToPty(b"\x1b[Z".to_vec()),
        Key::Named(NamedKey::Tab) => Action::SendToPty(vec![b'\t']),
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
            "v" => Action::SetMode(visual_mode_init()),
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

fn visual_up_action(
    cur_col: usize,
    cur_row: usize,
    move_to: &impl Fn(usize, usize) -> Action,
) -> Action {
    if cur_row == 0 {
        Action::VisualBoundaryUp(1)
    } else {
        move_to(cur_col, cur_row - 1)
    }
}

fn visual_down_action(
    cur_col: usize,
    cur_row: usize,
    rows: usize,
    move_to: &impl Fn(usize, usize) -> Action,
) -> Action {
    if cur_row == rows {
        Action::VisualBoundaryDown(1)
    } else {
        move_to(cur_col, cur_row + 1)
    }
}

fn visual_char_action(
    s: &str,
    cur_col: usize,
    cur_row: usize,
    cols: usize,
    rows: usize,
    move_to: &impl Fn(usize, usize) -> Action,
) -> Action {
    match s {
        "h" => move_to(cur_col.saturating_sub(1), cur_row),
        "l" => move_to((cur_col + 1).min(cols), cur_row),
        "k" => visual_up_action(cur_col, cur_row, move_to),
        "j" => visual_down_action(cur_col, cur_row, rows, move_to),
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
        "v" => Action::VisualAnchor,
        "q" => Action::SetMode(InputMode::Insert),
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
        Key::Named(NamedKey::ArrowUp) => visual_up_action(cur_col, cur_row, &move_to),
        Key::Named(NamedKey::ArrowDown) => visual_down_action(cur_col, cur_row, rows, &move_to),
        Key::Named(NamedKey::Home) => move_to(0, cur_row),
        Key::Named(NamedKey::End) => move_to(cols, cur_row),
        Key::Named(NamedKey::PageUp) => Action::VisualBoundaryUp(rows + 1),
        Key::Named(NamedKey::PageDown) => Action::VisualBoundaryDown(rows + 1),
        Key::Character(s) => visual_char_action(s, cur_col, cur_row, cols, rows, &move_to),
        _ => Action::None,
    }
}

fn handle_quit_save(key: &Key) -> Action {
    match key {
        Key::Character(s) if s.eq_ignore_ascii_case("s") => Action::QuitSaveSession,
        Key::Character(s)
            if s.eq_ignore_ascii_case("q")
                || s.eq_ignore_ascii_case("n")
                || s.eq_ignore_ascii_case("y") =>
        {
            Action::QuitNoSave
        }
        Key::Named(NamedKey::Enter) => Action::QuitNoSave,
        Key::Named(NamedKey::Escape) => Action::SetMode(InputMode::Insert),
        _ => Action::None,
    }
}

const SCREENSHOT_MOVE_STEP: i32 = 20;

fn handle_screenshot(key: &Key, shift: bool) -> Action {
    if shift {
        return match key {
            Key::Named(NamedKey::ArrowRight) => Action::ScreenshotEdgeResize(1, 0),
            Key::Named(NamedKey::ArrowLeft) => Action::ScreenshotEdgeResize(-1, 0),
            Key::Named(NamedKey::ArrowDown) => Action::ScreenshotEdgeResize(0, 1),
            Key::Named(NamedKey::ArrowUp) => Action::ScreenshotEdgeResize(0, -1),
            _ => Action::None,
        };
    }
    match key {
        Key::Named(NamedKey::ArrowRight) => Action::ScreenshotMove(SCREENSHOT_MOVE_STEP, 0),
        Key::Named(NamedKey::ArrowLeft) => Action::ScreenshotMove(-SCREENSHOT_MOVE_STEP, 0),
        Key::Named(NamedKey::ArrowDown) => Action::ScreenshotMove(0, SCREENSHOT_MOVE_STEP),
        Key::Named(NamedKey::ArrowUp) => Action::ScreenshotMove(0, -SCREENSHOT_MOVE_STEP),
        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => Action::ScreenshotCapture,
        Key::Named(NamedKey::Escape) => Action::SetMode(InputMode::Insert),
        _ => Action::None,
    }
}

#[cfg(test)]
#[path = "keybindings_test.rs"]
mod tests;
