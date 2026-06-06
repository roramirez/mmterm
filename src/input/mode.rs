#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Insert,
    /// Visual mode: cursor navigates freely; selection only active when `anchored` is true.
    Visual {
        start_col: usize,
        start_row: usize,
        cur_col: usize,
        cur_row: usize,
        /// False while navigating (no highlight); true after the user presses `v` to set the anchor.
        anchored: bool,
    },
    /// Inline tab-rename: buf holds the name being typed
    RenameTab {
        buf: String,
    },
    /// Scrollback search: query holds the pattern being typed.
    /// `history_pos` is `Some((idx, len))` while the user browses previous queries with ↑/↓.
    Search {
        query: String,
        history_pos: Option<(usize, usize)>,
    },
    /// Command palette: query filters actions; selected is index into filtered list
    CommandPalette {
        query: String,
        selected: usize,
    },
    /// Quit was triggered; waiting for save-session decision.
    QuitSave,
    /// Screenshot region selector: rectangle centered at (cx, cy).
    /// `half_w`/`half_h` are half the width and height of the selection.
    Screenshot {
        cx: u32,
        cy: u32,
        half_w: u32,
        half_h: u32,
    },
    /// User is typing a name for the screenshot before it is saved.
    ScreenshotName {
        cx: u32,
        cy: u32,
        half_w: u32,
        half_h: u32,
        name: String,
    },
}
