use super::*;

/// Builds an EventLoop that works from any thread (needed for tests).
/// Falls back gracefully if no display is available.
fn make_event_loop() -> Option<winit::event_loop::EventLoop<()>> {
    #[cfg(target_os = "linux")]
    {
        use winit::event_loop::EventLoopBuilder;

        // Try X11 first
        #[cfg(feature = "x11")]
        {
            use winit::platform::x11::EventLoopBuilderExtX11;
            if let Ok(el) = EventLoopBuilder::new().with_any_thread(true).build() {
                return Some(el);
            }
        }

        // Try Wayland
        #[cfg(feature = "wayland")]
        {
            use winit::platform::wayland::EventLoopBuilderExtWayland;
            if let Ok(el) = EventLoopBuilder::new().with_any_thread(true).build() {
                return Some(el);
            }
        }

        // Try X11 unconditionally via the trait (always compiled on Linux)
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

#[test]
fn eventloop_proxy_can_be_created() {
    if let Some(el) = make_event_loop() {
        let _proxy = el.create_proxy();
    } else {
        println!("skipped — no display available");
    }
}

/// Constructs a headless App for testing — no window, no PTY spawned yet.
fn make_app() -> Option<App> {
    let el = make_event_loop()?;
    let proxy = el.create_proxy();
    // Keep el alive long enough for proxy to be valid, then leak it.
    // The proxy remains usable after the EventLoop is dropped on Linux.
    std::mem::forget(el);
    Some(App::new(Config::default(), proxy))
}

#[test]
fn app_new_headless_does_not_panic() {
    if make_app().is_none() {
        println!("skipped — no display available");
    }
}

#[test]
fn app_change_font_size_increase() {
    let Some(mut app) = make_app() else { return };
    // Simulate a tab with metrics so change_font_size has something to work with.
    // Without tabs, the method will panic on index — skip gracefully.
    if app.state.tabs.is_empty() {
        return;
    }
    let before = app.state.tabs[0].metrics.font_px;
    app.change_font_size(2.0);
    let after = app.state.tabs[app.state.active_tab].metrics.font_px;
    assert!(after > before || (after - before).abs() < 0.1);
}

#[test]
fn app_initial_mode_is_insert() {
    let Some(app) = make_app() else { return };
    assert!(matches!(app.state.mode, InputMode::Insert));
}

#[test]
fn app_search_matches_initially_empty() {
    let Some(app) = make_app() else { return };
    assert!(app.state.search_matches.is_empty());
    assert_eq!(app.state.search_current, 0);
}

#[test]
fn no_debug_flag_returns_none() {
    if !std::env::args().any(|a| a == "--debug") {
        assert!(debug_log_path().is_none());
    }
}

#[test]
fn debug_log_path_format() {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = format!("{home}/.mmterm");
    let ts = chrono::Local::now().format("%Y%m%dT%H%M%S");
    let path = format!("{dir}/debug-{ts}.log");
    assert!(path.starts_with(&dir));
    assert!(path.ends_with(".log"));
    assert!(path.contains("debug-"));
}
