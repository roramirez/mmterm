use crate::terminal::grid::Grid;

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn char_at(grid: &Grid, scroll_offset: usize, row: usize, col: usize) -> char {
    if col >= grid.cols {
        return ' ';
    }
    if scroll_offset == 0 {
        return if row < grid.rows {
            grid.cell(col, row).c
        } else {
            ' '
        };
    }
    let sb_len = grid.scrollback.len();
    let abs_row = sb_len.saturating_sub(scroll_offset) + row;
    if abs_row < sb_len {
        let line = &grid.scrollback[abs_row];
        if col < line.len() { line[col].c } else { ' ' }
    } else {
        let live = abs_row.saturating_sub(sb_len);
        if live < grid.rows {
            grid.cell(col, live).c
        } else {
            ' '
        }
    }
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
    loop {
        let next = step_forward(c, r, cols, rows);
        let Some((nc, nr)) = next else { break };
        let next_char = char_at(grid, scroll_offset, nr, nc);
        if cur_is_word && !is_word(next_char) {
            c = nc;
            r = nr;
            break;
        }
        if !cur_is_word && (next_char == ' ' || is_word(next_char)) {
            c = nc;
            r = nr;
            break;
        }
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
        if cur_is_word && !next_is_word {
            break;
        }
        if !cur_is_word {
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
}
