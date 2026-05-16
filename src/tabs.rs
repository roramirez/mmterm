/// Returns the index of the next tab, wrapping around.
/// Returns `active` unchanged when there is only one tab.
pub fn next_tab_index(active: usize, count: usize) -> usize {
    if count <= 1 {
        active
    } else {
        (active + 1) % count
    }
}

/// Returns the index of the previous tab, wrapping around.
/// Returns `active` unchanged when there is only one tab.
pub fn prev_tab_index(active: usize, count: usize) -> usize {
    if count <= 1 {
        active
    } else {
        active.checked_sub(1).unwrap_or(count - 1)
    }
}

/// Returns the new active-tab index after the tab at `active` has been
/// removed from a list of `count` tabs.
///
/// Precondition: `count >= 2` (caller must handle the single-tab case).
pub fn close_tab_index(active: usize, count: usize) -> usize {
    // After removal the list has count-1 entries. If active was the last,
    // clamp to the new last index.
    if active >= count - 1 {
        count - 2
    } else {
        active
    }
}

/// Returns the new active index after swapping the active tab one step to
/// the left (`move_left`) or to the right (`!move_left`).
/// Returns `active` unchanged when the move is out of bounds.
pub fn move_tab_index(active: usize, count: usize, move_left: bool) -> usize {
    if move_left {
        if count > 1 && active > 0 {
            active - 1
        } else {
            active
        }
    } else {
        if count > 1 && active + 1 < count {
            active + 1
        } else {
            active
        }
    }
}

/// Returns the next active pane id by cycling through `leaves` after `active`.
/// Returns `active` unchanged when `active` is not in `leaves`.
pub fn next_pane_in_layout(leaves: &[usize], active: usize) -> usize {
    if let Some(pos) = leaves.iter().position(|&id| id == active) {
        leaves[(pos + 1) % leaves.len()]
    } else {
        active
    }
}

/// Returns `true` when quitting requires user confirmation.
/// Confirmation is needed whenever more than one tab or pane is open.
pub fn needs_quit_confirm(tab_count: usize, total_pane_count: usize) -> bool {
    tab_count > 1 || total_pane_count > 1
}

#[cfg(test)]
#[path = "tabs_test.rs"]
mod tests;
