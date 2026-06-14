use crate::terminal::grid::Grid;

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn char_at(grid: &Grid, scroll_offset: usize, row: usize, col: usize) -> char {
    grid.cell_char_at(row, col, scroll_offset).unwrap_or(' ')
}

/// Advance one step in linear viewport order, wrapping across rows.
fn step_forward(col: usize, row: usize, cols: usize, rows: usize) -> Option<(usize, usize)> {
    if col + 1 < cols {
        Some((col + 1, row))
    } else if row + 1 < rows {
        Some((0, row + 1))
    } else {
        None
    }
}

/// Retreat one step in linear viewport order, wrapping across rows.
fn step_back(col: usize, row: usize, cols: usize) -> Option<(usize, usize)> {
    if col > 0 {
        Some((col - 1, row))
    } else if row > 0 {
        Some((cols - 1, row - 1))
    } else {
        None
    }
}

/// Move cursor to the start of the next word (`w`).
/// Skips the rest of the current word, then skips whitespace.
pub fn word_forward(grid: &Grid, scroll_offset: usize, col: usize, row: usize) -> (usize, usize) {
    let cols = grid.cols;
    let rows = grid.rows;
    let mut c = col;
    let mut r = row;

    let cur_is_word = is_word(char_at(grid, scroll_offset, r, c));

    // Skip the current run (word or non-word non-space).
    while let Some((nc, nr)) = step_forward(c, r, cols, rows) {
        let next_char = char_at(grid, scroll_offset, nr, nc);
        c = nc;
        r = nr;
        if (cur_is_word && !is_word(next_char))
            || (!cur_is_word && (next_char == ' ' || is_word(next_char)))
        {
            break;
        }
    }

    // Skip whitespace.
    while char_at(grid, scroll_offset, r, c) == ' ' {
        let Some((nc, nr)) = step_forward(c, r, cols, rows) else {
            break;
        };
        c = nc;
        r = nr;
    }

    (c, r)
}

/// Move cursor to the start of the previous word (`b`).
pub fn word_backward(grid: &Grid, scroll_offset: usize, col: usize, row: usize) -> (usize, usize) {
    let cols = grid.cols;
    let mut c = col;
    let mut r = row;

    // Step back at least once.
    let Some((sc, sr)) = step_back(c, r, cols) else {
        return (c, r);
    };
    c = sc;
    r = sr;

    // Skip whitespace going backwards.
    while char_at(grid, scroll_offset, r, c) == ' ' {
        let Some((nc, nr)) = step_back(c, r, cols) else {
            break;
        };
        c = nc;
        r = nr;
    }

    // Skip word chars going backwards (or non-word non-space).
    let target_is_word = is_word(char_at(grid, scroll_offset, r, c));
    while let Some((nc, nr)) = step_back(c, r, cols) {
        let prev = char_at(grid, scroll_offset, nr, nc);
        if target_is_word != is_word(prev) || prev == ' ' {
            break;
        }
        c = nc;
        r = nr;
    }

    (c, r)
}

/// Move cursor to the end of the current or next word (`e`).
pub fn word_end(grid: &Grid, scroll_offset: usize, col: usize, row: usize) -> (usize, usize) {
    let cols = grid.cols;
    let rows = grid.rows;
    let mut c = col;
    let mut r = row;

    // If already at end of a word, move forward first.
    let at_word_end = {
        let cur = char_at(grid, scroll_offset, r, c);
        let next = step_forward(c, r, cols, rows)
            .map(|(nc, nr)| char_at(grid, scroll_offset, nr, nc))
            .unwrap_or(' ');
        is_word(cur) && !is_word(next)
    };
    if at_word_end {
        let Some((nc, nr)) = step_forward(c, r, cols, rows) else {
            return (c, r);
        };
        c = nc;
        r = nr;
    }

    // Skip whitespace.
    while char_at(grid, scroll_offset, r, c) == ' ' {
        let Some((nc, nr)) = step_forward(c, r, cols, rows) else {
            break;
        };
        c = nc;
        r = nr;
    }

    // Advance to end of word.
    loop {
        let cur_is_word = is_word(char_at(grid, scroll_offset, r, c));
        let Some((nc, nr)) = step_forward(c, r, cols, rows) else {
            break;
        };
        let next_is_word = is_word(char_at(grid, scroll_offset, nr, nc));
        if !cur_is_word || !next_is_word {
            break;
        }
        c = nc;
        r = nr;
    }

    (c, r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::grid::{Color, Grid, GridColors};

    fn make_grid(text: &str) -> Grid {
        let cols = text.len().max(10);
        let mut g = Grid::with_colors(
            cols,
            5,
            GridColors {
                fg: Color::WHITE,
                bg: Color::BLACK,
                cursor: Color::WHITE,
                selection: Color::WHITE,
                palette: [Color::BLACK; 16],
            },
            100,
        );
        for c in text.chars() {
            g.write_char(c);
        }
        g
    }

    #[test]
    fn word_forward_skips_to_next_word() {
        let g = make_grid("hello world");
        let (c, r) = word_forward(&g, 0, 0, 0);
        assert_eq!(r, 0);
        assert_eq!(c, 6); // 'w' in "world"
    }

    #[test]
    fn word_forward_from_space_skips_to_word() {
        let g = make_grid("hello world");
        let (c, r) = word_forward(&g, 0, 5, 0); // starting at the space
        assert_eq!(r, 0);
        assert_eq!(c, 6);
    }

    #[test]
    fn word_backward_lands_on_word_start() {
        let g = make_grid("hello world");
        let (c, r) = word_backward(&g, 0, 8, 0); // inside "world"
        assert_eq!(r, 0);
        assert_eq!(c, 6); // start of "world"
    }

    #[test]
    fn word_backward_from_start_of_word_jumps_to_prev_word() {
        let g = make_grid("hello world");
        let (c, r) = word_backward(&g, 0, 6, 0); // at 'w'
        assert_eq!(r, 0);
        assert_eq!(c, 0); // start of "hello"
    }

    #[test]
    fn word_end_finds_end_of_current_word() {
        let g = make_grid("hello world");
        let (c, r) = word_end(&g, 0, 0, 0); // at 'h'
        assert_eq!(r, 0);
        assert_eq!(c, 4); // 'o' in "hello"
    }

    #[test]
    fn word_end_from_space_finds_end_of_next_word() {
        let g = make_grid("hello world");
        let (c, r) = word_end(&g, 0, 5, 0); // at space
        assert_eq!(r, 0);
        assert_eq!(c, 10); // 'd' in "world"
    }

    #[test]
    fn word_backward_at_origin_stays_at_origin() {
        // line 104: step_back returns None at (0,0) → early return
        let g = make_grid("hello");
        let (c, r) = word_backward(&g, 0, 0, 0);
        assert_eq!((c, r), (0, 0));
    }

    #[test]
    fn word_end_at_end_of_word_advances_to_next() {
        // lines 148-152: already at word end → step forward first
        let g = make_grid("hello world");
        let (c, r) = word_end(&g, 0, 4, 0); // at 'o', end of "hello"
        assert_eq!(r, 0);
        assert_eq!(c, 10); // 'd', end of "world"
    }

    #[test]
    fn word_forward_at_last_cell_stays() {
        // line 40: step_forward returns None at last cell
        let g = make_grid("hi");
        let cols = g.cols;
        let (c, r) = word_forward(&g, 0, cols - 1, g.rows - 1);
        assert_eq!(r, g.rows - 1);
        assert_eq!(c, cols - 1);
    }

    #[test]
    fn char_at_col_out_of_bounds_returns_space() {
        // line 9: col >= grid.cols → ' '
        let g = make_grid("hi");
        assert_eq!(char_at(&g, 0, 0, g.cols + 10), ' ');
    }

    #[test]
    fn char_at_row_out_of_bounds_returns_space() {
        // line 15: row >= grid.rows → ' '
        let g = make_grid("hi");
        assert_eq!(char_at(&g, 0, g.rows + 5, 0), ' ');
    }

    #[test]
    fn word_forward_with_scroll_offset_reads_scrollback() {
        // lines 18-22: scroll_offset > 0 path in char_at
        let mut g = make_grid("hello world and more text here now ");
        // push content into scrollback by writing more lines
        for _ in 0..g.rows + 2 {
            for c in "next line     ".chars() {
                g.write_char(c);
            }
        }
        // With scroll_offset > 0 the function should not panic
        let (_, _) = word_forward(&g, 1, 0, 0);
    }
}
