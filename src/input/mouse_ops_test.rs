use std::collections::HashMap;

use crate::app_state::{AppState, TabState};
use crate::config::Config;
use crate::dpi::Logical;
use crate::input::InputMode;
use crate::renderer::FontMetrics;
use crate::ui::layout::Layout;

use crate::App;

/// Builds an EventLoop that works from any thread (needed for tests). Falls back
/// gracefully if no display is available. Mirrors `pane_ops_test.rs`.
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
    Some(App::new(Config::default(), proxy, None, None))
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
        mode: InputMode::Insert,
    }
}

/// Pushes a one-pane tab (id 1, 8x16 cells) onto `app` and focuses it. The pane's
/// grid is 80x24 with rect `[0, 22, 800, 556]` (see `test_pane_entry`).
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

#[test]
fn select_line_at_anchors_full_row_visual() {
    let Some(mut app) = make_app() else {
        return; // headless CI without a display
    };
    seed_one_pane_tab(&mut app);

    // Pixel inside the pane: x=10 -> col 1, y=30 -> row 0 (rect y0 = 22, ch = 16).
    app.select_line_at(10.0, 30.0);

    match app.state.mode() {
        InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            cur_row,
            anchored,
        } => {
            assert_eq!(*start_col, 0, "line selection starts at column 0");
            assert_eq!(*cur_col, 79, "line selection ends at the last column");
            assert_eq!(*start_row, 0);
            assert_eq!(*cur_row, 0);
            assert!(*anchored, "line selection is anchored/highlighted");
        }
        other => panic!("expected anchored Visual selection, got {other:?}"),
    }
    assert!(
        !app.state.mouse_selecting,
        "triple-click ends any in-progress drag selection"
    );
    assert_eq!(app.tab().active, 1, "triple-click focuses the clicked pane");
}

#[test]
fn select_line_at_ignores_clicks_outside_any_pane() {
    let Some(mut app) = make_app() else {
        return;
    };
    seed_one_pane_tab(&mut app);

    // y=5 is in the tab bar (< tab_h = 22), so no pane is hit.
    app.select_line_at(10.0, 5.0);
    assert!(
        matches!(app.state.mode(), InputMode::Insert),
        "a click outside any pane leaves the mode unchanged"
    );
}
