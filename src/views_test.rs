use super::*;
use crate::app_state::AppState;
use crate::config::Config;
use crate::input::InputMode;
use crate::theme::default_theme;
use crate::ui::layout::{STATUS_BAR_H, TAB_BAR_H};

fn make_state() -> AppState {
    AppState::new(Config::default(), default_theme())
}

// ── collect_pane_views ────────────────────────────────────────────────────────

#[test]
fn collect_pane_views_empty_tab_returns_empty() {
    let mut state = make_state();
    state.add_empty_tab();
    let views = collect_pane_views(&state, 800, 600, TAB_BAR_H, STATUS_BAR_H);
    assert!(views.is_empty());
}

#[test]
fn collect_pane_views_single_pane_rect_spans_usable_area() {
    let mut state = make_state();
    state.add_test_pane();
    // add_test_pane() creates a layout with Layout::new(id, 800, 578).
    // The rect comes from the stored layout, not from the (w,h) argument
    // (which only affects the zoomed full-window path).
    // Usable height = layout_h - TAB_BAR_H - STATUS_BAR_H = 578 - 22 - 22 = 534.
    let views = collect_pane_views(&state, 800, 600, TAB_BAR_H, STATUS_BAR_H);
    assert_eq!(views.len(), 1);
    let rect = views[0].rect;
    assert_eq!(rect[0], 0);
    assert_eq!(rect[1], TAB_BAR_H);
    assert_eq!(rect[2], 800);
    assert_eq!(rect[3], 578u32.saturating_sub(TAB_BAR_H + STATUS_BAR_H));
}

#[test]
fn collect_pane_views_single_pane_is_active() {
    let mut state = make_state();
    state.add_test_pane();
    let views = collect_pane_views(&state, 800, 600, TAB_BAR_H, STATUS_BAR_H);
    assert!(views[0].is_active);
}

#[test]
fn collect_pane_views_zoomed_fills_window() {
    let mut state = make_state();
    state.add_test_pane();
    state.tabs[state.active_tab].zoomed = true;
    let (w, h) = (800u32, 600u32);
    let views = collect_pane_views(&state, w, h, TAB_BAR_H, STATUS_BAR_H);
    assert_eq!(views.len(), 1);
    let rect = views[0].rect;
    assert_eq!(rect[0], 0);
    assert_eq!(rect[1], TAB_BAR_H);
    assert_eq!(rect[2], w);
    assert_eq!(rect[3], h.saturating_sub(TAB_BAR_H + STATUS_BAR_H));
    assert!(views[0].is_active);
}

#[test]
fn collect_pane_views_zoomed_no_active_pane_returns_empty() {
    let mut state = make_state();
    state.add_empty_tab();
    // active points to an id not in panes
    state.tabs[state.active_tab].zoomed = true;
    let views = collect_pane_views(&state, 800, 600, TAB_BAR_H, STATUS_BAR_H);
    assert!(views.is_empty());
}

#[test]
fn collect_pane_views_no_search_returns_empty_matches() {
    let mut state = make_state();
    state.add_test_pane();
    // search_matches is empty by default
    let views = collect_pane_views(&state, 800, 600, TAB_BAR_H, STATUS_BAR_H);
    assert_eq!(views.len(), 1);
    assert!(views[0].search_matches.is_empty());
    assert!(views[0].search_current.is_none());
}

#[test]
fn collect_pane_views_search_matches_assigned_to_active_pane() {
    let mut state = make_state();
    state.add_test_pane();
    state.search_matches = vec![(0, 0, 3), (1, 2, 5)];
    state.search_current = 1;
    let views = collect_pane_views(&state, 800, 600, TAB_BAR_H, STATUS_BAR_H);
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].search_matches.len(), 2);
    assert_eq!(views[0].search_current, Some(1));
}

#[test]
fn collect_pane_views_hovered_url_propagated() {
    let mut state = make_state();
    state.add_test_pane();
    state.hovered_url = Some("https://example.com".into());
    let views = collect_pane_views(&state, 800, 600, TAB_BAR_H, STATUS_BAR_H);
    assert_eq!(views[0].hovered_url, Some("https://example.com"));
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
    // inactive tab (index 1) should not show rename cursor
    assert!(!titles[1].0.contains('|'));
}
