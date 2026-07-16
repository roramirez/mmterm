use std::collections::HashMap;

use crate::App;
use crate::app_state::{AppState, TabState};
use crate::config::Config;
use crate::dpi::Logical;
use crate::input::InputMode;
use crate::renderer::FontMetrics;
use crate::ui::layout::Layout;

/// Builds an EventLoop that works from any thread (needed for tests).
/// Falls back gracefully if no display is available. Mirrors `pane_ops_test.rs`.
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

/// Headless App with a single pane (id 1) whose grid contains the word "hi"
/// starting at (row 0, col 0). Returns `None` when no display is available.
fn app_with_word() -> Option<App> {
    let el = make_event_loop()?;
    let proxy = el.create_proxy();
    std::mem::forget(el);
    let mut app = App::new(Config::default(), proxy, None, None);

    let mut tab = empty_tab();
    let entry = AppState::test_pane_entry(Logical(16.0), metrics(16.0, 8, 16));
    if let Some(mut grid) = entry.pane.grid_write() {
        grid.write_char('h');
        grid.write_char('i');
    }
    tab.panes.insert(1, entry);
    app.state.tabs.push(tab);
    app.state.active_tab = app.state.tabs.len() - 1;
    app.state.next_pane_id = 2;
    // Start from a clean slate so `get_or_insert_with` is an accurate
    // "did the copy path run" signal.
    app.state.clipboard = None;
    Some(app)
}

fn set_drag_selection(app: &mut App) {
    app.state.tab_mut().mode = InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 1,
        cur_row: 0,
        anchored: true,
    };
}

// ── finish_mouse_selection (drag release) ──────────────────────────────────────

#[test]
fn finish_selection_does_not_copy_when_disabled() {
    let Some(mut app) = app_with_word() else {
        return;
    };
    app.state.config.window.copy_on_select = false;
    set_drag_selection(&mut app);
    app.finish_mouse_selection();
    // Copy path never reached → clipboard was not lazily created.
    assert!(app.state.clipboard.is_none());
    assert!(matches!(app.state.mode(), InputMode::Insert));
}

#[test]
fn finish_selection_copies_when_enabled() {
    // Skip when the system clipboard is unavailable (e.g. headless CI).
    if arboard::Clipboard::new().is_err() {
        return;
    }
    let Some(mut app) = app_with_word() else {
        return;
    };
    app.state.config.window.copy_on_select = true;
    set_drag_selection(&mut app);
    app.finish_mouse_selection();
    // Copy path reached → clipboard lazily created for the write.
    assert!(app.state.clipboard.is_some());
    assert!(matches!(app.state.mode(), InputMode::Insert));
}

// ── select_word_at (double-click) ──────────────────────────────────────────────

#[test]
fn word_select_does_not_copy_when_disabled() {
    let Some(mut app) = app_with_word() else {
        return;
    };
    app.state.config.window.copy_on_select = false;
    // Pixel (1, 23) lands on grid cell (col 0, row 0) with 8x16 cells and a
    // 22px tab bar → selects the word "hi".
    app.select_word_at(1.0, 23.0);
    assert!(app.state.clipboard.is_none());
    // The word stays highlighted in anchored Visual mode.
    assert!(matches!(
        app.state.mode(),
        InputMode::Visual { anchored: true, .. }
    ));
}

#[test]
fn word_select_copies_when_enabled() {
    if arboard::Clipboard::new().is_err() {
        return;
    }
    let Some(mut app) = app_with_word() else {
        return;
    };
    app.state.config.window.copy_on_select = true;
    app.select_word_at(1.0, 23.0);
    assert!(app.state.clipboard.is_some());
}
