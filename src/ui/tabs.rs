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
    if count <= 1 {
        return active;
    }
    if move_left {
        active.saturating_sub(1)
    } else {
        (active + 1).min(count - 1)
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

/// Builds the display label for a tab entry in the tab bar.
///
/// `rename_buf` — `Some(buf)` when the tab is being renamed (overrides all other names).
/// `osc_title`  — OSC 0/2 title set by the running process (pre-filtered: no `/` or `~` prefix).
/// `name`       — user-assigned tab name.
/// Falls back to `" {index+1} "` when no name is available.
pub fn tab_label(
    index: usize,
    name: Option<&str>,
    osc_title: Option<&str>,
    is_active: bool,
    rename_buf: Option<&str>,
) -> String {
    if is_active && let Some(buf) = rename_buf {
        return format!(" {}| ", buf);
    }
    name.or(osc_title)
        .map(|n| format!(" {} ", n))
        .unwrap_or_else(|| format!(" {} ", index + 1))
}

/// Returns `true` when the cursor should be drawn in this pane.
/// The cursor is only visible in Insert mode, on the live view (not scrolled),
/// during the blink-on phase.
pub fn should_show_cursor(
    is_active: bool,
    in_insert_mode: bool,
    blink_visible: bool,
    scroll_offset: usize,
) -> bool {
    is_active && in_insert_mode && blink_visible && scroll_offset == 0
}

#[cfg(test)]
#[path = "tabs_test.rs"]
mod tests;
