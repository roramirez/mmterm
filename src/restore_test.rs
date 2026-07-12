use crate::config::Config;
use crate::session::{SavedNode, SavedSession, SavedTab};

use super::App;

/// Builds an EventLoop that works from any thread (needed for tests).
/// Returns `None` when no display is available (headless CI) so callers skip.
/// Mirrors the helper in `pane_ops_test.rs` / `main_test.rs`.
fn make_event_loop() -> Option<winit::event_loop::EventLoop<()>> {
    #[cfg(target_os = "linux")]
    {
        use winit::event_loop::EventLoopBuilder;
        use winit::platform::x11::EventLoopBuilderExtX11;
        EventLoopBuilder::new().with_any_thread(true).build().ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        winit::event_loop::EventLoop::new().ok()
    }
}

/// Headless App with no window; `None` if no display is available.
fn make_app() -> Option<App> {
    let el = make_event_loop()?;
    let proxy = el.create_proxy();
    std::mem::forget(el);
    Some(App::new(Config::default(), proxy, None, None))
}

fn one_pane_tab(cwd: std::path::PathBuf) -> SavedTab {
    SavedTab {
        name: Some("t".into()),
        active_pane: 0,
        pane_cwds: vec![cwd],
        layout: SavedNode::Leaf { slot: 0 },
    }
}

#[test]
fn restore_session_falls_back_to_home_for_missing_cwd() {
    let Some(mut app) = make_app() else {
        return; // no display — skip
    };
    let missing = std::path::PathBuf::from("/nonexistent/path/should/not/exist");
    let saved = SavedSession {
        active_tab: 0,
        tabs: vec![one_pane_tab(missing)],
        theme: None,
    };
    // A non-existent CWD must not abort the restore; the pane spawns in $HOME.
    let ok = app.restore_session(saved, 800, 600);
    assert!(ok, "restore should succeed despite the missing CWD");
    assert_eq!(app.state.tabs.len(), 1, "the saved tab was restored");
    assert_eq!(
        app.state.tabs[0].panes.len(),
        1,
        "the pane spawned via the $HOME fallback"
    );
}

#[test]
fn restore_session_handles_empty_cwd_as_home() {
    let Some(mut app) = make_app() else {
        return; // no display — skip
    };
    // An empty CWD string is the documented "fall back to $HOME" sentinel.
    let saved = SavedSession {
        active_tab: 0,
        tabs: vec![one_pane_tab(std::path::PathBuf::new())],
        theme: None,
    };
    assert!(app.restore_session(saved, 800, 600));
    assert_eq!(app.state.tabs[0].panes.len(), 1);
}

#[test]
fn restore_session_empty_tabs_is_noop() {
    let Some(mut app) = make_app() else {
        return; // no display — skip
    };
    let saved = SavedSession {
        active_tab: 0,
        tabs: vec![],
        theme: None,
    };
    // Nothing to restore: returns false and leaves the app with no tabs.
    assert!(!app.restore_session(saved, 800, 600));
    assert!(app.state.tabs.is_empty());
}
