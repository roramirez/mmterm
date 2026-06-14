use std::sync::RwLockReadGuard;

use crate::app_state::AppState;
use crate::input::InputMode;
use crate::renderer::PaneView;
use crate::terminal::grid::Grid;
use crate::ui::tabs;

#[cfg(test)]
#[path = "views_test.rs"]
mod tests;

fn search_args(
    is_active: bool,
    has_search: bool,
    matches: &[(usize, usize, usize)],
    current: usize,
) -> (&[(usize, usize, usize)], Option<usize>) {
    if is_active && has_search {
        (matches, Some(current))
    } else {
        (&[], None)
    }
}

/// Acquire read guards for all panes in the active tab.
/// Only used in tests; in production, callers clone Arcs first so guards are
/// independent of the AppState borrow.
#[cfg(test)]
pub(crate) fn acquire_grid_guards(state: &AppState) -> Vec<(usize, RwLockReadGuard<'_, Grid>)> {
    if state.tabs.is_empty() {
        return vec![];
    }
    let tab = &state.tabs[state.active_tab];
    tab.panes
        .iter()
        .map(|(id, e)| (*id, e.pane.grid.read().unwrap()))
        .collect()
}

pub fn collect_pane_views<'a>(
    state: &'a AppState,
    guards: &'a [(usize, RwLockReadGuard<'a, Grid>)],
    w: u32,
    h: u32,
    tab_h: u32,
    status_h: u32,
) -> Vec<PaneView<'a>> {
    if state.tabs.is_empty() {
        return vec![];
    }
    let tab = &state.tabs[state.active_tab];
    let active_id = tab.active;
    let has_search = !state.search_matches.is_empty();
    let search_matches = &state.search_matches;
    let search_current_val = state.search_current;
    let insert_mode = matches!(state.mode, InputMode::Insert);

    let guard_for = |id: usize| -> Option<&'a Grid> {
        guards.iter().find(|(gid, _)| *gid == id).map(|(_, g)| &**g)
    };

    if tab.zoomed {
        let Some(entry) = tab.panes.get(&active_id) else {
            return vec![];
        };
        let Some(grid) = guard_for(active_id) else {
            return vec![];
        };
        let show_cursor = tabs::should_show_cursor(
            true,
            insert_mode,
            state.cursor_blink,
            entry.pane.scroll_offset,
        );
        let (sm, sc) = search_args(true, has_search, search_matches, search_current_val);
        vec![PaneView {
            grid,
            rect: [0, tab_h, w, h.saturating_sub(tab_h + status_h)],
            scroll_offset: entry.pane.scroll_offset,
            is_active: true,
            show_cursor,
            blink_visible: state.cursor_blink,
            search_matches: sm,
            search_current: sc,
            hovered_url: state.hovered_url.as_deref(),
            cursor_shape: grid.cursor_shape,
            metrics: &entry.metrics,
        }]
    } else {
        let rects = tab.layout.rects_scaled(tab_h, status_h);
        rects
            .iter()
            .filter_map(|(id, rect)| {
                let entry = tab.panes.get(id)?;
                let grid = guard_for(*id)?;
                let is_active = *id == active_id;
                let show_cursor = tabs::should_show_cursor(
                    is_active,
                    insert_mode,
                    state.cursor_blink,
                    entry.pane.scroll_offset,
                );
                let (sm, sc) =
                    search_args(is_active, has_search, search_matches, search_current_val);
                Some(PaneView {
                    grid,
                    rect: *rect,
                    scroll_offset: entry.pane.scroll_offset,
                    is_active,
                    show_cursor,
                    blink_visible: state.cursor_blink,
                    search_matches: sm,
                    search_current: sc,
                    hovered_url: state.hovered_url.as_deref(),
                    cursor_shape: grid.cursor_shape,
                    metrics: &entry.metrics,
                })
            })
            .collect()
    }
}

pub fn build_tab_titles(state: &AppState) -> Vec<(String, bool, bool)> {
    state
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let is_active = i == state.active_tab;
            let osc_title = tab
                .panes
                .get(&tab.active)
                .and_then(|e| {
                    let g = e.pane.grid.read().unwrap();
                    g.osc_title.clone()
                })
                .filter(|t| !t.starts_with('/') && !t.starts_with('~'));
            let rename_buf = if is_active {
                if let InputMode::RenameTab { buf } = &state.mode {
                    Some(buf.as_str())
                } else {
                    None
                }
            } else {
                None
            };
            let label = tabs::tab_label(
                i,
                tab.name.as_deref(),
                osc_title.as_deref(),
                is_active,
                rename_buf,
            );
            (label, is_active, tab.has_activity)
        })
        .collect()
}
