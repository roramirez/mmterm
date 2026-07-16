use std::collections::HashMap;

use super::App;
use super::bracketed_paste_encode;
use crate::app_state::{AppState, TabState};
use crate::config::Config;
use crate::dpi::Logical;
use crate::renderer::FontMetrics;
use crate::ui::layout::Layout;

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// Builds an EventLoop usable from a test thread; None when no display exists.
/// Mirrors `pane_ops_test.rs`.
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

fn make_app(cfg: Config) -> Option<App> {
    let el = make_event_loop()?;
    let proxy = el.create_proxy();
    std::mem::forget(el);
    Some(App::new(cfg, proxy, None, None))
}

fn metrics() -> FontMetrics {
    FontMetrics {
        font_px: 16.0,
        cell_width: 8,
        cell_height: 16,
        baseline: 13,
    }
}

/// Pushes a one-pane tab (id 1) with a real PTY so the immediate-send path has a
/// valid active pane to write to.
fn seed_one_pane_tab(app: &mut App) {
    let mut tab = TabState {
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
    };
    tab.panes
        .insert(1, AppState::test_pane_entry(Logical(16.0), metrics()));
    app.state.tabs.push(tab);
    app.state.active_tab = app.state.tabs.len() - 1;
    app.state.next_pane_id = 2;
}

#[test]
fn paste_text_gates_multiline_paste_when_threshold_met() {
    let mut cfg = Config::default();
    cfg.window.paste_confirm_lines = 2;
    let Some(mut app) = make_app(cfg) else {
        return;
    };
    // 3 lines => 2 newlines >= threshold 2: must stash, not send.
    app.paste_text("a\nb\nc".to_string(), true);
    assert_eq!(app.state.pending_paste.as_deref(), Some("a\nb\nc"));
}

#[test]
fn paste_text_disabled_by_default_sends_immediately() {
    // Default threshold is 0 (disabled): even a multi-line paste sends directly.
    let Some(mut app) = make_app(Config::default()) else {
        return;
    };
    seed_one_pane_tab(&mut app);
    app.paste_text("a\nb\nc".to_string(), true);
    assert!(app.state.pending_paste.is_none());
}

#[test]
fn paste_text_below_threshold_sends_immediately() {
    let mut cfg = Config::default();
    cfg.window.paste_confirm_lines = 5;
    let Some(mut app) = make_app(cfg) else {
        return;
    };
    seed_one_pane_tab(&mut app);
    // 2 newlines < threshold 5: sends without confirmation.
    app.paste_text("a\nb\nc".to_string(), false);
    assert!(app.state.pending_paste.is_none());
}

#[test]
fn pending_paste_is_cleared_when_taken() {
    let mut cfg = Config::default();
    cfg.window.paste_confirm_lines = 2;
    let Some(mut app) = make_app(cfg) else {
        return;
    };
    app.paste_text("x\ny\nz".to_string(), true);
    assert!(app.state.pending_paste.is_some());
    // The overlay confirm/cancel branch takes the text, leaving None.
    let taken = app.state.pending_paste.take();
    assert_eq!(taken.as_deref(), Some("x\ny\nz"));
    assert!(app.state.pending_paste.is_none());
}

#[test]
fn bracketed_paste_wraps_text_in_markers() {
    let out = bracketed_paste_encode("hi", true);
    let mut expected = Vec::new();
    expected.extend_from_slice(PASTE_START);
    expected.extend_from_slice(b"hi");
    expected.extend_from_slice(PASTE_END);
    assert_eq!(out, expected);
}

#[test]
fn non_bracketed_paste_passes_text_through_unchanged() {
    let out = bracketed_paste_encode("hi", false);
    assert_eq!(out, b"hi");
}

#[test]
fn bracketed_paste_empty_text_still_emits_markers() {
    // An empty paste in bracketed mode is start+end with nothing between, so a
    // program in bracketed-paste mode still sees a (zero-length) paste event.
    let out = bracketed_paste_encode("", true);
    let mut expected = Vec::new();
    expected.extend_from_slice(PASTE_START);
    expected.extend_from_slice(PASTE_END);
    assert_eq!(out, expected);
}

#[test]
fn non_bracketed_empty_text_is_empty() {
    assert!(bracketed_paste_encode("", false).is_empty());
}

#[test]
fn bracketed_paste_preserves_inner_bytes_including_newlines() {
    let out = bracketed_paste_encode("a\nb", true);
    // The payload between the markers must be byte-for-byte the original text.
    assert_eq!(
        &out[PASTE_START.len()..out.len() - PASTE_END.len()],
        b"a\nb"
    );
}
