use crate::app_state::AppState;
use crate::config::Config;
use crate::theme::default_theme;

fn make_state() -> AppState {
    AppState::new(Config::default(), default_theme())
}

#[test]
fn push_search_history_ignores_empty_query() {
    let mut s = make_state();
    s.push_search_history(String::new());
    assert!(s.search_history.is_empty());
}

#[test]
fn push_search_history_appends_in_order() {
    let mut s = make_state();
    s.push_search_history("foo".into());
    s.push_search_history("bar".into());
    assert_eq!(s.search_history, vec!["foo".to_string(), "bar".to_string()]);
}

#[test]
fn push_search_history_dedupes_moving_existing_to_end() {
    let mut s = make_state();
    s.push_search_history("foo".into());
    s.push_search_history("bar".into());
    s.push_search_history("foo".into());
    // "foo" must not appear twice; the re-search moves it to the most-recent slot.
    assert_eq!(s.search_history, vec!["bar".to_string(), "foo".to_string()]);
}

#[test]
fn push_search_history_caps_at_50_entries_dropping_oldest() {
    let mut s = make_state();
    for i in 0..60 {
        s.push_search_history(format!("q{i}"));
    }
    assert_eq!(
        s.search_history.len(),
        50,
        "history is capped at 50 entries"
    );
    // The 10 oldest (q0..q9) were dropped; q10 is now the oldest, q59 the newest.
    assert_eq!(s.search_history.first().unwrap(), "q10");
    assert_eq!(s.search_history.last().unwrap(), "q59");
}

#[test]
fn push_search_history_clears_pending_before_buffer() {
    let mut s = make_state();
    s.search_before_history = "draft".into();
    s.push_search_history("committed".into());
    assert!(
        s.search_before_history.is_empty(),
        "committing a search must clear the saved in-progress query"
    );
}

#[test]
fn click_is_forwarded_when_application_enabled_mouse_reporting() {
    assert!(super::forward_click_to_pty(1000, 0, false));
    assert!(super::forward_click_to_pty(1002, 1, false));
    assert!(super::forward_click_to_pty(1006, 2, false));
}

#[test]
fn click_is_handled_locally_without_mouse_reporting() {
    assert!(!super::forward_click_to_pty(0, 0, false));
}

#[test]
fn shift_click_bypasses_mouse_reporting() {
    // Shift must reach the terminal's own selection / link handling even while
    // a full-screen application (Claude Code, vim, tmux) grabs the mouse.
    assert!(!super::forward_click_to_pty(1000, 0, true));
    assert!(!super::forward_click_to_pty(1002, 1, true));
    assert!(!super::forward_click_to_pty(1006, 2, true));
}

#[test]
fn buttons_beyond_right_are_never_forwarded() {
    assert!(!super::forward_click_to_pty(1000, 3, false));
    assert!(!super::forward_click_to_pty(1000, 3, true));
}
