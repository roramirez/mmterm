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

/// The contiguous range of tabs to draw when the strip is scrolled, plus
/// whether overflow chevrons are needed on each side.
pub struct TabWindow {
    /// Index of the first visible tab.
    pub first: usize,
    /// Index of the last visible tab (inclusive).
    pub last: usize,
    /// Whether tabs are hidden to the left (draw a `‹` chevron).
    pub left_chevron: bool,
    /// Whether tabs are hidden to the right (draw a `›` chevron).
    pub right_chevron: bool,
}

/// Computes which contiguous range of tabs is visible so that the active tab is
/// always shown, scrolling at whole-tab granularity.
///
/// `widths` is each tab's pixel width, `avail` the available pixel width for the
/// strip, and `chevron_w` the width reserved for an overflow chevron on a side
/// that has hidden tabs. The returned range always contains `active`.
pub fn visible_tab_window(widths: &[u32], active: usize, avail: u32, chevron_w: u32) -> TabWindow {
    let n = widths.len();
    if n == 0 {
        return TabWindow {
            first: 0,
            last: 0,
            left_chevron: false,
            right_chevron: false,
        };
    }
    let active = active.min(n - 1);
    let total: u32 = widths.iter().sum();
    if total <= avail {
        return TabWindow {
            first: 0,
            last: n - 1,
            left_chevron: false,
            right_chevron: false,
        };
    }

    // Overflow: anchor on the active tab and grow the window outward (left then
    // right) while it keeps fitting, reserving chevron space on whichever side
    // still has hidden tabs.
    let used = |first: usize, last: usize| -> u32 {
        let w: u32 = widths[first..=last].iter().sum();
        let lc = if first > 0 { chevron_w } else { 0 };
        let rc = if last < n - 1 { chevron_w } else { 0 };
        w + lc + rc
    };

    let mut first = active;
    let mut last = active;
    loop {
        let mut grew = false;
        if first > 0 && used(first - 1, last) <= avail {
            first -= 1;
            grew = true;
        }
        if last < n - 1 && used(first, last + 1) <= avail {
            last += 1;
            grew = true;
        }
        if !grew {
            break;
        }
    }

    TabWindow {
        first,
        last,
        left_chevron: first > 0,
        right_chevron: last < n - 1,
    }
}

#[cfg(test)]
#[path = "tabs_test.rs"]
mod tests;
