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
        window_state: None,
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
        window_state: None,
    };
    assert!(app.restore_session(saved, 800, 600));
    assert_eq!(app.state.tabs[0].panes.len(), 1);
}

#[test]
fn build_saved_session_without_window_yields_no_window_state() {
    let Some(mut app) = make_app() else {
        return; // no display — skip
    };
    // Seed one tab/pane so build_saved_session has something to walk.
    app.new_tab(800, 600);
    // No window has been created on the headless App, so the geometry is unknown.
    let saved = app.build_saved_session();
    assert!(
        saved.window_state.is_none(),
        "window_state must be None when there is no window to read geometry from"
    );
}

/// Unique scope name per test process so we never collide with a real
/// user scope; the caller is responsible for removing what it writes.
fn unique_scope(tag: &str) -> String {
    format!("__mmterm_shutdown_test_{tag}_{}", std::process::id())
}

fn cleanup_scope(scope: &str) {
    let _ = std::fs::remove_file(crate::session::session_path_for(Some(scope)));
    let _ = std::fs::remove_dir_all(crate::session::scrollback_dir_for(Some(scope)));
}

#[test]
fn save_session_on_shutdown_writes_when_restore_enabled() {
    let Some(mut app) = make_app() else {
        return; // no display — skip
    };
    let scope = unique_scope("enabled");
    cleanup_scope(&scope);
    app.scope = Some(scope.clone());
    app.state.config.general.restore_session = true;
    app.new_tab(800, 600);

    app.save_session_on_shutdown();

    let path = crate::session::session_path_for(Some(&scope));
    assert!(path.exists(), "session file written on shutdown");
    let loaded = crate::session::load_from(&path).expect("session round-trips");
    assert_eq!(loaded.tabs.len(), 1, "the single tab was persisted");
    cleanup_scope(&scope);
}

#[test]
fn save_session_on_shutdown_is_noop_when_restore_disabled() {
    let Some(mut app) = make_app() else {
        return; // no display — skip
    };
    let scope = unique_scope("disabled");
    cleanup_scope(&scope);
    app.scope = Some(scope.clone());
    app.state.config.general.restore_session = false;
    app.new_tab(800, 600);

    app.save_session_on_shutdown();

    let path = crate::session::session_path_for(Some(&scope));
    assert!(
        !path.exists(),
        "nothing must be written when restore_session is disabled"
    );
    cleanup_scope(&scope);
}

#[test]
fn save_session_on_shutdown_honors_scope_path() {
    let Some(mut app) = make_app() else {
        return; // no display — skip
    };
    let scope = unique_scope("scoped");
    cleanup_scope(&scope);
    app.scope = Some(scope.clone());
    app.state.config.general.restore_session = true;
    app.new_tab(800, 600);

    app.save_session_on_shutdown();

    // The save must land at the scoped path, not the default session file.
    let scoped = crate::session::session_path_for(Some(&scope));
    assert!(scoped.exists(), "scoped session file written");
    assert_ne!(
        scoped,
        crate::session::session_path_for(None),
        "scoped path differs from the default session path"
    );
    cleanup_scope(&scope);
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
        window_state: None,
    };
    // Nothing to restore: returns false and leaves the app with no tabs.
    assert!(!app.restore_session(saved, 800, 600));
    assert!(app.state.tabs.is_empty());
}
