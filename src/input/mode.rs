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
    /// Scrollback search: query holds the pattern being typed
    Search {
        query: String,
    },
    /// Command palette: query filters actions; selected is index into filtered list
    CommandPalette {
        query: String,
        selected: usize,
    },
}
