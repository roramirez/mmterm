#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Insert,
    /// Visual mode: selection from (start_col, start_row) to (cur_col, cur_row)
    Visual {
        start_col: usize,
        start_row: usize,
        cur_col: usize,
        cur_row: usize,
    },
    /// Inline tab-rename: buf holds the name being typed
    RenameTab { buf: String },
}
