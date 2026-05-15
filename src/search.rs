use crate::terminal::grid::Grid;

/// Scans `grid` (scrollback + live rows) for all matches of `query` (treated as a regex).
/// Returns `(abs_row, start_col, match_len)` tuples sorted by ascending `abs_row`.
/// Returns an empty vec when `query` is empty or not a valid regex.
pub fn compute_search_matches(grid: &Grid, query: &str) -> Vec<(usize, usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let re = match regex::Regex::new(query) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let sb_len = grid.scrollback.len();
    let mut matches = Vec::new();

    for (abs_row, line) in grid.scrollback.iter().enumerate() {
        let text: String = line.iter().map(|c| c.c).collect();
        for mat in re.find_iter(&text) {
            let col = text[..mat.start()].chars().count();
            let len = mat.as_str().chars().count();
            matches.push((abs_row, col, len));
        }
    }

    for row in 0..grid.rows {
        let abs_row = sb_len + row;
        let text: String = (0..grid.cols).map(|c| grid.cell(c, row).c).collect();
        for mat in re.find_iter(&text) {
            let col = text[..mat.start()].chars().count();
            let len = mat.as_str().chars().count();
            matches.push((abs_row, col, len));
        }
    }

    matches
}

/// Computes the scroll offset needed to center `abs_row` in the viewport.
/// Returns 0 when `abs_row` is a live grid row (i.e. `abs_row >= sb_len`).
pub fn compute_scroll_offset(abs_row: usize, sb_len: usize, grid_rows: usize) -> usize {
    if abs_row >= sb_len {
        0
    } else {
        let target_row = grid_rows / 2;
        (sb_len + target_row).saturating_sub(abs_row).min(sb_len)
    }
}

#[cfg(test)]
#[path = "search_test.rs"]
mod tests;
