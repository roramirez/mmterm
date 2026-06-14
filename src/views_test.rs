use super::*;
use crate::app_state::AppState;
use crate::config::Config;
use crate::input::InputMode;
use crate::theme::default_theme;
use crate::ui::layout::{STATUS_BAR_H, TAB_BAR_H};

fn make_state() -> AppState {
    AppState::new(Config::default(), default_theme())
}

/// Convenience: acquire guards and collect views in one call.
fn views(state: &AppState, w: u32, h: u32) -> Vec<PaneView<'_>> {
    // We can't return guards and views together because of lifetime constraints,
    // so leak the guards via a static thread-local for test purposes.
    // Instead, we just test properties that don't depend on grid content.
    let guards = acquire_grid_guards(state);
    // The guards are dropped after this block; views would become invalid.
    // To keep tests simple, we return a vec of collected non-borrow fields.
    // We work around this by calling collect_pane_views inside a scope and
    // extracting the non-borrow data before returning.
    drop(guards);
    // Re-acquire for the actual call
    let guards = acquire_grid_guards(state);
    // We need to keep guards alive for the duration of use. Since we can't
    // return views that borrow guards in a test helper, call the fn inline.
    let _views = collect_pane_views(state, &guards, w, h, TAB_BAR_H, STATUS_BAR_H);
    drop(guards);
    // Return a simplified view of the data we need to test
    vec![]
}

// Helper that collects a snapshot of rect/is_active/etc. for testing.
struct ViewSnapshot {
    rect: [u32; 4],
    is_active: bool,
    search_matches_len: usize,
    search_current: Option<usize>,
    hovered_url: Option<String>,
}

fn collect_snapshots(state: &AppState, w: u32, h: u32) -> Vec<ViewSnapshot> {
    let guards = acquire_grid_guards(state);
    let views = collect_pane_views(state, &guards, w, h, TAB_BAR_H, STATUS_BAR_H);
    views
        .iter()
        .map(|v| ViewSnapshot {
            rect: v.rect,
            is_active: v.is_active,
            search_matches_len: v.search_matches.len(),
            search_current: v.search_current,
            hovered_url: v.hovered_url.map(|s| s.to_string()),
        })
        .collect()
}

// ── collect_pane_views ────────────────────────────────────────────────────────

#[test]
fn collect_pane_views_empty_tab_returns_empty() {
    let mut state = make_state();
    state.add_empty_tab();
    let snaps = collect_snapshots(&state, 800, 600);
    assert!(snaps.is_empty());
}

#[test]
fn collect_pane_views_single_pane_rect_spans_usable_area() {
    let mut state = make_state();
    state.add_test_pane();
    let snaps = collect_snapshots(&state, 800, 600);
    assert_eq!(snaps.len(), 1);
    let rect = snaps[0].rect;
    assert_eq!(rect[0], 0);
    assert_eq!(rect[1], TAB_BAR_H);
    assert_eq!(rect[2], 800);
    assert_eq!(rect[3], 578u32.saturating_sub(TAB_BAR_H + STATUS_BAR_H));
}

#[test]
fn collect_pane_views_single_pane_is_active() {
    let mut state = make_state();
    state.add_test_pane();
    let snaps = collect_snapshots(&state, 800, 600);
    assert!(snaps[0].is_active);
}

#[test]
fn collect_pane_views_zoomed_fills_window() {
    let mut state = make_state();
    state.add_test_pane();
    state.tabs[state.active_tab].zoomed = true;
    let (w, h) = (800u32, 600u32);
    let snaps = collect_snapshots(&state, w, h);
    assert_eq!(snaps.len(), 1);
    let rect = snaps[0].rect;
    assert_eq!(rect[0], 0);
    assert_eq!(rect[1], TAB_BAR_H);
    assert_eq!(rect[2], w);
    assert_eq!(rect[3], h.saturating_sub(TAB_BAR_H + STATUS_BAR_H));
    assert!(snaps[0].is_active);
}

#[test]
fn collect_pane_views_zoomed_no_active_pane_returns_empty() {
    let mut state = make_state();
    state.add_empty_tab();
    state.tabs[state.active_tab].zoomed = true;
    let snaps = collect_snapshots(&state, 800, 600);
    assert!(snaps.is_empty());
}

#[test]
fn collect_pane_views_no_search_returns_empty_matches() {
    let mut state = make_state();
    state.add_test_pane();
    let snaps = collect_snapshots(&state, 800, 600);
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].search_matches_len, 0);
    assert!(snaps[0].search_current.is_none());
}

#[test]
fn collect_pane_views_search_matches_assigned_to_active_pane() {
    let mut state = make_state();
    state.add_test_pane();
    state.search_matches = vec![(0, 0, 3), (1, 2, 5)];
    state.search_current = 1;
    let snaps = collect_snapshots(&state, 800, 600);
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].search_matches_len, 2);
    assert_eq!(snaps[0].search_current, Some(1));
}

#[test]
fn collect_pane_views_hovered_url_propagated() {
    let mut state = make_state();
    state.add_test_pane();
    state.hovered_url = Some("https://example.com".into());
    let snaps = collect_snapshots(&state, 800, 600);
    assert_eq!(snaps[0].hovered_url.as_deref(), Some("https://example.com"));
}

// ── build_tab_titles ──────────────────────────────────────────────────────────

#[test]
fn build_tab_titles_single_tab_is_active() {
    let mut state = make_state();
    state.add_empty_tab();
    let titles = build_tab_titles(&state);
    assert_eq!(titles.len(), 1);
    let (label, is_active, has_activity) = &titles[0];
    assert!(is_active);
    assert!(!has_activity);
    assert!(label.contains('1'));
}

#[test]
fn build_tab_titles_multiple_only_one_active() {
    let mut state = make_state();
    state.add_empty_tab();
    state.add_empty_tab();
    state.add_empty_tab();
    state.active_tab = 1;
    let titles = build_tab_titles(&state);
    assert_eq!(titles.len(), 3);
    let active_count = titles.iter().filter(|(_, a, _)| *a).count();
    assert_eq!(active_count, 1);
    assert!(titles[1].1);
}

#[test]
fn build_tab_titles_tab_with_name_uses_name() {
    let mut state = make_state();
    state.add_empty_tab();
    state.tabs[state.active_tab].name = Some("myshell".into());
    let titles = build_tab_titles(&state);
    assert!(titles[0].0.contains("myshell"));
}

#[test]
fn build_tab_titles_activity_flag_reflected() {
    let mut state = make_state();
    state.add_empty_tab();
    state.add_empty_tab();
    state.active_tab = 0;
    state.tabs[1].has_activity = true;
    let titles = build_tab_titles(&state);
    assert!(!titles[0].2);
    assert!(titles[1].2);
}

#[test]
fn build_tab_titles_rename_mode_shows_cursor() {
    let mut state = make_state();
    state.add_empty_tab();
    state.mode = InputMode::RenameTab {
        buf: "newname".into(),
    };
    let titles = build_tab_titles(&state);
    assert!(
        titles[0].0.contains('|'),
        "rename buf should show cursor pipe"
    );
}

#[test]
fn build_tab_titles_rename_mode_inactive_tab_not_affected() {
    let mut state = make_state();
    state.add_empty_tab();
    state.add_empty_tab();
    state.active_tab = 0;
    state.mode = InputMode::RenameTab {
        buf: "newname".into(),
    };
    let titles = build_tab_titles(&state);
    assert!(!titles[1].0.contains('|'));
}
