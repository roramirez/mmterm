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
