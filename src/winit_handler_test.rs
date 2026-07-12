use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::next_bell_wakeup;
use crate::app_state::TabState;
use crate::ui::layout::Layout;

/// Builds a bare tab whose only meaningful field for these tests is
/// `bell_flash_until`. No panes, no PTYs — `next_bell_wakeup` only reads the
/// bell expiry.
fn tab_with_bell(bell_until: Option<Instant>) -> TabState {
    TabState {
        panes: HashMap::new(),
        layout: Layout::new(0, 800, 600),
        active: 0,
        name: None,
        zoomed: false,
        has_activity: false,
        bell_flash_start: None,
        bell_flash_until: bell_until,
        bell_cooldown_until: None,
        passthrough: false,
        mode: crate::input::InputMode::Insert,
    }
}

#[test]
fn next_bell_wakeup_empty_tabs_returns_default() {
    let default = Instant::now() + Duration::from_secs(10);
    assert_eq!(next_bell_wakeup(&[], default), default);
}

#[test]
fn next_bell_wakeup_ignores_tabs_without_bell() {
    let default = Instant::now() + Duration::from_secs(10);
    let tabs = [tab_with_bell(None), tab_with_bell(None)];
    assert_eq!(next_bell_wakeup(&tabs, default), default);
}

#[test]
fn next_bell_wakeup_ignores_expired_bell() {
    let default = Instant::now() + Duration::from_secs(10);
    // A bell that already expired must not pull the wakeup into the past.
    let past = Instant::now() - Duration::from_millis(1);
    let tabs = [tab_with_bell(Some(past))];
    assert_eq!(next_bell_wakeup(&tabs, default), default);
}

#[test]
fn next_bell_wakeup_picks_earliest_future_bell() {
    let default = Instant::now() + Duration::from_secs(10);
    let soon = Instant::now() + Duration::from_millis(200);
    let later = Instant::now() + Duration::from_secs(5);
    let tabs = [tab_with_bell(Some(later)), tab_with_bell(Some(soon))];
    assert_eq!(
        next_bell_wakeup(&tabs, default),
        soon,
        "must wake at the soonest pending bell, earlier than the default"
    );
}

#[test]
fn next_bell_wakeup_future_bell_after_default_is_ignored() {
    // A bell later than the default deadline must not delay the wakeup.
    let default = Instant::now() + Duration::from_millis(100);
    let far = Instant::now() + Duration::from_secs(30);
    let tabs = [tab_with_bell(Some(far))];
    assert_eq!(next_bell_wakeup(&tabs, default), default);
}
