use winit::event::{ElementState, KeyEvent, Modifiers};
use winit::keyboard::{Key, NamedKey};

use super::mode::InputMode;
use crate::input::keymap::{
    BindingKey, DispatchCtx, InputModeKind, KeyMap, ModeClass, Mods, action_from_name,
    token_from_key,
};

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
    keymap: &KeyMap,
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
    let st = modifiers.state();
    handle_key_modified(
        keymap,
        &event.logical_key,
        st.control_key(),
        st.shift_key(),
        st.alt_key(),
        st.super_key(),
        mode,
        grid_cols,
        grid_rows,
        application_cursor_keys,
    )
}

/// Routes a key by its modifier flags. The keymap is consulted FIRST; on a miss
/// we fall through to the encoding/mode handlers. Split from `handle_key` so the
/// dispatch is unit-testable without constructing winit `Modifiers`/`KeyEvent`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_key_modified(
    keymap: &KeyMap,
    key: &Key,
    ctrl: bool,
    shift: bool,
    alt: bool,
    cmd: bool,
    mode: &InputMode,
    grid_cols: usize,
    grid_rows: usize,
    application_cursor_keys: bool,
) -> Action {
    let mods = Mods {
        ctrl,
        shift,
        alt,
        cmd,
    };
    if let Some(token) = token_from_key(key, /* lowercase */ true) {
        let bkey = BindingKey {
            mods,
            token,
            chord_tail: None,
        };
        if let Some(name) = keymap.lookup(ModeClass::Global, &bkey) {
            let ctx = DispatchCtx {
                grid_rows,
                mode: InputModeKind::of(mode),
            };
            if let Some(action) = action_from_name(name, ctx) {
                return action;
            }
        }
    }

    // Keymap miss. When ⌘/Super is held, swallow so a bare ⌘<key> never leaks
    // to the PTY (matches prior behavior). Otherwise fall through to encoding.
    if cmd {
        return Action::None;
    }
    handle_key_inner(
        key,
        ctrl,
        shift,
        alt,
        mode,
        grid_cols,
        grid_rows,
        application_cursor_keys,
    )
}

pub fn handle_ctrl_w(keymap: &KeyMap, event: &KeyEvent) -> Action {
    if event.state != ElementState::Pressed {
        return Action::None;
    }
    handle_ctrl_w_keymap(keymap, &event.logical_key)
}

/// Resolve a `Ctrl+W <tail>` chord against the keymap. Chord tails are stored
/// case-preserved, but only `R` differs from `r` (rotate backward vs forward).
/// We look up the case-preserved tail FIRST so `R` resolves to its own binding,
/// then retry with the lowercased tail so shifted tails like `Ctrl+W V` / `S`
/// still match the lowercase `v` / `s` chords.
pub(crate) fn handle_ctrl_w_keymap(keymap: &KeyMap, key: &Key) -> Action {
    let make_bkey = |tail| BindingKey {
        mods: Mods {
            ctrl: true,
            shift: false,
            alt: false,
            cmd: false,
        },
        token: crate::input::keymap::KeyToken::Char("w".into()),
        chord_tail: Some((Mods::default(), tail)),
    };

    let name = token_from_key(key, /* lowercase */ false)
        .and_then(|tail| keymap.lookup(ModeClass::Global, &make_bkey(tail)))
        .or_else(|| {
            token_from_key(key, /* lowercase */ true)
                .and_then(|tail| keymap.lookup(ModeClass::Global, &make_bkey(tail)))
        });

    match name {
        Some(name) => action_from_name(
            name,
            DispatchCtx {
                grid_rows: 0,
                mode: InputModeKind::Insert,
            },
        )
        .unwrap_or(Action::None),
        None => Action::None,
    }
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
    // Alt+Tab is swallowed (never leaks ESC-Tab to the PTY), matching pre-keymap
    // behavior. Passthrough mode bypasses handle_key_inner, so it still sends raw.
    if alt && !ctrl && matches!(key, Key::Named(NamedKey::Tab)) {
        return Action::None;
    }

    // Ctrl+C copies the selection while in Visual mode (else falls through to
    // raw 0x03 in Insert). Not a Global keymap row because that would also
    // intercept Ctrl+C in Insert.
    if ctrl
        && !shift
        && !alt
        && matches!(mode, InputMode::Visual { .. })
        && matches!(key, Key::Character(s) if s.eq_ignore_ascii_case("c"))
    {
        return Action::Copy;
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

fn visual_mode_init() -> InputMode {
    InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 0,
        cur_row: 0,
        anchored: false,
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
