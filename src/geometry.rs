/// Returns the id of the first pane whose pixel rect contains `(px, py)`.
/// Right and bottom edges are exclusive (half-open interval).
pub fn pane_at_pixel(rects: &[(usize, [u32; 4])], px: f64, py: f64) -> Option<usize> {
    for &(id, [rx, ry, rw, rh]) in rects {
        if px >= rx as f64 && py >= ry as f64 && px < (rx + rw) as f64 && py < (ry + rh) as f64 {
            return Some(id);
        }
    }
    None
}

/// Converts a pixel coordinate to a grid cell `(col, row)` inside `rect`.
/// Returns `None` when `(px, py)` is outside `rect`.
/// The result is clamped to `[0, cols-1] × [0, rows-1]`.
pub fn pixel_to_cell(
    rect: [u32; 4],
    cell_w: u32,
    cell_h: u32,
    cols: usize,
    rows: usize,
    px: f64,
    py: f64,
) -> Option<(usize, usize)> {
    let [rx, ry, rw, rh] = rect;
    if px < rx as f64 || py < ry as f64 || px >= (rx + rw) as f64 || py >= (ry + rh) as f64 {
        return None;
    }
    let col = ((px - rx as f64) / cell_w as f64) as usize;
    let row = ((py - ry as f64) / cell_h as f64) as usize;
    Some((
        col.min(cols.saturating_sub(1)),
        row.min(rows.saturating_sub(1)),
    ))
}

/// Returns the OSC 8 URL of the cell at grid position `(col, row)` accounting
/// for scroll offset. Returns `None` when the coordinates are out of bounds or
/// the cell carries no URL.
pub fn cell_url_at_scroll(
    grid: &crate::terminal::grid::Grid,
    scroll_offset: usize,
    col: usize,
    row: usize,
) -> Option<std::sync::Arc<String>> {
    let cell = if scroll_offset == 0 {
        grid.cell(col, row)
    } else {
        let sb_len = grid.scrollback.len();
        let sb_start = sb_len.saturating_sub(scroll_offset);
        let sb_row = sb_start + row;
        if sb_row < sb_len {
            let line = &grid.scrollback[sb_row];
            if col < line.len() {
                &line[col]
            } else {
                return None;
            }
        } else {
            let live_row = sb_row.saturating_sub(sb_len);
            if live_row < grid.rows {
                grid.cell(col, live_row)
            } else {
                return None;
            }
        }
    };
    cell.url.clone()
}

#[cfg(test)]
#[path = "geometry_test.rs"]
mod tests;
