use super::*;
use crate::renderer::FontMetrics;
use crate::theme::default_theme;
use crate::ui::Layout;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use winit::event::Modifiers;
// Use the X11 extension to set `any_thread = true`. Both the X11 and
// Wayland Linux backends share the same underlying `any_thread` flag, so
// calling either extension is equivalent.
#[cfg(all(unix, not(target_os = "macos")))]
use winit::platform::x11::EventLoopBuilderExtX11;

// Create the EventLoop once per process and cache its proxy. We use
// `with_any_thread(true)` because cargo runs tests on non-main threads.
static TEST_PROXY: OnceLock<winit::event_loop::EventLoopProxy<()>> = OnceLock::new();

fn headless_proxy() -> winit::event_loop::EventLoopProxy<()> {
    TEST_PROXY
        .get_or_init(|| {
            #[cfg(all(unix, not(target_os = "macos")))]
            let el = winit::event_loop::EventLoop::builder()
                .with_any_thread(true)
                .build()
                .expect("test event loop");
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            let el = winit::event_loop::EventLoop::builder()
                .build()
                .expect("test event loop");
            let p = el.create_proxy();
            std::mem::forget(el);
            p
        })
        .clone()
}

impl App {
    fn new_headless() -> Self {
        let config = Config::default();
        let renderer = Renderer::new(&config.font.family, config.font.size);
        let theme = default_theme();
        Self {
            window: None,
            surface: None,
            renderer,
            tabs: Vec::new(),
            active_tab: 0,
            next_pane_id: 0,
            mode: InputMode::Insert,
            modifiers: Modifiers::default(),
            cursor_blink: true,
            blink_last: Instant::now(),
            ctrl_w_pending: false,
            config,
            config_panel: None,
            clipboard: None,
            mouse_pos: None,
            mouse_selecting: false,
            proxy: headless_proxy(),
            surface_size: (0, 0),
            search_matches: Vec::new(),
            search_current: 0,
            hovered_url: None,
            swallow_next_tab: false,
            wakeup_pending: Arc::new(AtomicBool::new(false)),
            theme,
        }
    }

    fn push_empty_tab(&mut self) {
        let metrics = FontMetrics {
            font_px: 16.0,
            cell_width: 8,
            cell_height: 16,
            baseline: 13,
        };
        self.tabs.push(TabState {
            panes: HashMap::new(),
            layout: Layout::new(0, 800, 600),
            active: 0,
            metrics,
            name: None,
            zoomed: false,
            has_activity: false,
            bell_flash_until: None,
        });
    }
}

// ── Smoke test ────────────────────────────────────────────────────────────────

#[test]
fn new_headless_does_not_panic() {
    let _ = App::new_headless();
}

// ── Tab navigation ────────────────────────────────────────────────────────────

#[test]
fn next_tab_with_single_tab_does_nothing() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.next_tab();
    assert_eq!(app.active_tab, 0);
}

#[test]
fn next_tab_wraps_around_to_first() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.push_empty_tab();
    app.active_tab = 1;
    app.next_tab();
    assert_eq!(app.active_tab, 0);
}

#[test]
fn next_tab_advances_to_next() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.push_empty_tab();
    app.active_tab = 0;
    app.next_tab();
    assert_eq!(app.active_tab, 1);
}

#[test]
fn prev_tab_with_single_tab_does_nothing() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.prev_tab();
    assert_eq!(app.active_tab, 0);
}

#[test]
fn prev_tab_wraps_around_to_last() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.push_empty_tab();
    app.active_tab = 0;
    app.prev_tab();
    assert_eq!(app.active_tab, 1);
}

#[test]
fn prev_tab_goes_to_previous() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.push_empty_tab();
    app.active_tab = 1;
    app.prev_tab();
    assert_eq!(app.active_tab, 0);
}

// ── Tab reordering ────────────────────────────────────────────────────────────

#[test]
fn move_tab_left_at_index_zero_does_nothing() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.push_empty_tab();
    app.active_tab = 0;
    app.move_tab_left();
    assert_eq!(app.active_tab, 0);
}

#[test]
fn move_tab_left_decrements_active_tab() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.push_empty_tab();
    app.active_tab = 1;
    app.move_tab_left();
    assert_eq!(app.active_tab, 0);
}

#[test]
fn move_tab_right_at_last_index_does_nothing() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.push_empty_tab();
    app.active_tab = 1;
    app.move_tab_right();
    assert_eq!(app.active_tab, 1);
}

#[test]
fn move_tab_right_increments_active_tab() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.push_empty_tab();
    app.active_tab = 0;
    app.move_tab_right();
    assert_eq!(app.active_tab, 1);
}

#[test]
fn move_tab_left_single_tab_does_nothing() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.move_tab_left();
    assert_eq!(app.active_tab, 0);
}

#[test]
fn move_tab_right_single_tab_does_nothing() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.move_tab_right();
    assert_eq!(app.active_tab, 0);
}

// ── Config panel ──────────────────────────────────────────────────────────────

#[test]
fn open_config_panel_creates_panel() {
    let mut app = App::new_headless();
    assert!(app.config_panel.is_none());
    app.open_config_panel();
    assert!(app.config_panel.is_some());
}

#[test]
fn open_config_panel_twice_replaces_panel() {
    let mut app = App::new_headless();
    app.open_config_panel();
    app.open_config_panel();
    assert!(app.config_panel.is_some());
}

// ── Theme / palette ───────────────────────────────────────────────────────────

#[test]
fn reseed_pane_palettes_with_no_tabs_does_not_panic() {
    let mut app = App::new_headless();
    app.reseed_pane_palettes();
}

#[test]
fn reseed_pane_palettes_with_empty_tabs_does_not_panic() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.reseed_pane_palettes();
}

// ── Search state ──────────────────────────────────────────────────────────────

#[test]
fn update_search_matches_non_search_mode_returns_early() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.mode = InputMode::Insert;
    app.update_search_matches();
    assert!(app.search_matches.is_empty());
    assert_eq!(app.search_current, 0);
}

#[test]
fn update_search_matches_normal_mode_returns_early() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.mode = InputMode::Normal;
    app.search_matches = vec![(1, 2, 3)]; // pre-populate to show they aren't cleared
    app.update_search_matches();
    // Not in Search mode → returns without touching search_matches
    assert!(!app.search_matches.is_empty());
}

#[test]
fn update_search_matches_empty_query_clears_matches() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.mode = InputMode::Search {
        query: String::new(),
    };
    app.search_matches = vec![(0, 0, 3)];
    app.update_search_matches();
    assert!(app.search_matches.is_empty());
}

#[test]
fn scroll_to_match_empty_list_does_not_panic() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.search_matches.clear();
    app.scroll_to_match(0);
    assert_eq!(app.search_current, 0);
}

#[test]
fn scroll_to_match_out_of_bounds_does_not_panic() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    app.search_matches.clear();
    app.scroll_to_match(99);
    assert_eq!(app.search_current, 0);
}

// ── Pane hit testing ──────────────────────────────────────────────────────────

#[test]
fn pane_at_pixel_inside_content_area_returns_pane_id() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    // Layout::new(0, 800, 600) gives rect [0, 22, 800, 556]
    // (400, 300) is inside the content area
    let result = app.pane_at_pixel(400.0, 300.0);
    assert_eq!(result, Some(0));
}

#[test]
fn pane_at_pixel_in_tab_bar_returns_none() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    // y=5 is in the tab bar (y < TAB_BAR_H=22), outside any pane rect
    let result = app.pane_at_pixel(400.0, 5.0);
    assert!(result.is_none());
}

#[test]
fn pane_at_pixel_outside_window_returns_none() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    let result = app.pane_at_pixel(99999.0, 99999.0);
    assert!(result.is_none());
}

#[test]
fn pane_at_pixel_negative_coords_returns_none() {
    let mut app = App::new_headless();
    app.push_empty_tab();
    let result = app.pane_at_pixel(-1.0, -1.0);
    assert!(result.is_none());
}

// ── Miscellaneous ─────────────────────────────────────────────────────────────

#[test]
fn initial_state_has_insert_mode() {
    let app = App::new_headless();
    assert!(matches!(app.mode, InputMode::Insert));
}

#[test]
fn initial_state_search_is_empty() {
    let app = App::new_headless();
    assert!(app.search_matches.is_empty());
    assert_eq!(app.search_current, 0);
}

#[test]
fn initial_state_no_config_panel() {
    let app = App::new_headless();
    assert!(app.config_panel.is_none());
}

#[test]
fn initial_state_no_hovered_url() {
    let app = App::new_headless();
    assert!(app.hovered_url.is_none());
}
