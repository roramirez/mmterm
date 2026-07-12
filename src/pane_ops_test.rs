use std::collections::HashMap;

use crate::app_state::{AppState, TabState};
use crate::config::Config;
use crate::dpi::Logical;
use crate::renderer::FontMetrics;
use crate::ui::layout::{Layout, SplitDir};

use super::App;

/// Builds an EventLoop that works from any thread (needed for tests).
/// Falls back gracefully if no display is available. Mirrors `main_test.rs`.
fn make_event_loop() -> Option<winit::event_loop::EventLoop<()>> {
    #[cfg(target_os = "linux")]
    {
        use winit::event_loop::EventLoopBuilder;

        #[cfg(feature = "x11")]
        {
            use winit::platform::x11::EventLoopBuilderExtX11;
            if let Ok(el) = EventLoopBuilder::new().with_any_thread(true).build() {
                return Some(el);
            }
        }

        #[cfg(feature = "wayland")]
        {
            use winit::platform::wayland::EventLoopBuilderExtWayland;
            if let Ok(el) = EventLoopBuilder::new().with_any_thread(true).build() {
                return Some(el);
            }
        }

        {
            use winit::platform::x11::EventLoopBuilderExtX11;
            EventLoopBuilder::new().with_any_thread(true).build().ok()
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        winit::event_loop::EventLoop::new().ok()
    }
}

/// Constructs a headless App for testing — no window, no PTY spawned yet.
fn make_app() -> Option<App> {
    let el = make_event_loop()?;
    let proxy = el.create_proxy();
    std::mem::forget(el);
    Some(App::new(Config::default(), proxy, None))
}

/// Pushes a one-pane tab (id 1) onto `app` and focuses it, so split/close paths
/// have a valid active tab to operate on without spawning a real shell.
fn seed_one_pane_tab(app: &mut App) {
    let mut tab = empty_tab();
    tab.panes.insert(
        1,
        AppState::test_pane_entry(Logical(16.0), metrics(16.0, 8, 16)),
    );
    app.state.tabs.push(tab);
    app.state.active_tab = app.state.tabs.len() - 1;
    app.state.next_pane_id = 2;
}

fn metrics(font_px: f32, cw: u32, ch: u32) -> FontMetrics {
    FontMetrics {
        font_px,
        cell_width: cw,
        cell_height: ch,
        baseline: ch.saturating_sub(3),
    }
}

fn empty_tab() -> TabState {
    TabState {
        panes: HashMap::new(),
        layout: Layout::new(1, 800, 600),
        active: 1,
        name: None,
        zoomed: false,
        has_activity: false,
        bell_flash_start: None,
        bell_flash_until: None,
        bell_cooldown_until: None,
        passthrough: false,
        mode: crate::input::InputMode::Insert,
    }
}

#[test]
fn sync_uses_per_pane_metrics() {
    // Two panes side-by-side; pane 1 has half the cell size of pane 2,
    // so it must end up with more cols/rows after sizing.
    let mut tab = empty_tab();
    tab.layout.split(1, 2, SplitDir::H);
    tab.panes.insert(
        1,
        AppState::test_pane_entry(Logical(16.0), metrics(16.0, 8, 16)),
    );
    tab.panes.insert(
        2,
        AppState::test_pane_entry(Logical(32.0), metrics(32.0, 16, 32)),
    );

    App::sync_pane_sizes_tab(&mut tab, 22, 22, 0);

    // sync_pane_sizes_tab writes target dimensions to pending_resize; the parser
    // thread applies them asynchronously. Test the contract that sync_pane_sizes_tab
    // keeps: it must compute the correct (cols, rows) for each pane's metrics.
    let (c1, r1) = tab.panes[&1]
        .pending_resize
        .lock()
        .unwrap()
        .expect("pane 1 should have a pending resize");
    let (c2, r2) = tab.panes[&2]
        .pending_resize
        .lock()
        .unwrap()
        .expect("pane 2 should have a pending resize");
    assert!(c1 > c2, "smaller cells must yield more cols: {c1} vs {c2}");
    assert!(r1 > r2, "smaller cells must yield more rows: {r1} vs {r2}");
}

// ---- card 01: a failed PTY spawn must not create a ghost pane ----

#[test]
fn do_split_with_failed_spawn_is_inert() {
    let Some(mut app) = make_app() else {
        println!("skipped — no display available");
        return;
    };
    seed_one_pane_tab(&mut app);
    app.state.config.shell.program = Some("/does/not/exist".to_string());

    let before_active = app.tab().active;
    let before_panes = app.tab().panes.len();
    let before_leaves = app.tab().layout.leaves().len();

    app.do_split(SplitDir::H);

    assert_eq!(app.tab().active, before_active, "focus must not move");
    assert_eq!(app.tab().panes.len(), before_panes, "no pane added");
    assert_eq!(
        app.tab().layout.leaves().len(),
        before_leaves,
        "layout tree must be unchanged"
    );
    // Invariant: the active pane id exists in `panes`.
    assert!(app.tab().panes.contains_key(&app.tab().active));
}

#[test]
fn new_tab_with_failed_spawn_adds_no_tab() {
    let Some(mut app) = make_app() else {
        println!("skipped — no display available");
        return;
    };
    seed_one_pane_tab(&mut app);
    app.state.config.shell.program = Some("/does/not/exist".to_string());

    let before_tabs = app.state.tabs.len();
    let before_active_tab = app.state.active_tab;

    app.new_tab(800, 600);

    assert_eq!(app.state.tabs.len(), before_tabs, "no ghost tab created");
    assert_eq!(
        app.state.active_tab, before_active_tab,
        "active tab must not change"
    );
}

// ---- card 02: resolving the next active pane must never panic on empty ----

#[test]
fn next_active_after_remove_handles_empty() {
    let empty: HashMap<usize, crate::PaneEntry> = HashMap::new();
    // Both of these previously panicked via `keys().next().unwrap()`.
    assert_eq!(App::next_active_after_remove(&empty, None), None);
    assert_eq!(App::next_active_after_remove(&empty, Some(5)), None);
}

#[test]
fn next_active_after_remove_prefers_and_falls_back() {
    let mut panes: HashMap<usize, crate::PaneEntry> = HashMap::new();
    panes.insert(
        7,
        AppState::test_pane_entry(Logical(16.0), metrics(16.0, 8, 16)),
    );

    // Preferred id present → returned as-is.
    assert_eq!(App::next_active_after_remove(&panes, Some(7)), Some(7));
    // No preference → falls back to the only remaining pane.
    assert_eq!(App::next_active_after_remove(&panes, None), Some(7));
    // Preferred id absent → falls back to an existing key.
    assert_eq!(App::next_active_after_remove(&panes, Some(99)), Some(7));
}

#[test]
fn handlers_do_not_panic_with_empty_tabs() {
    // After a startup spawn failure the app has zero tabs and an exit is pending,
    // but winit may still deliver queued events. These handlers must not index
    // into the empty `tabs`.
    let Some(mut app) = make_app() else {
        println!("skipped — no display available");
        return;
    };
    assert!(app.state.tabs.is_empty());
    app.handle_focus_changed(true);
    app.handle_focus_changed(false);
    app.redraw();
}
