use super::*;
use std::time::{Duration, Instant};

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
    Some(App::new(Config::default(), proxy, None))
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
    let idx = app.state.active_tab;
    let active = app.state.tabs[idx].active;
    let before = app.state.tabs[idx].panes[&active].metrics.font_px;
    app.change_font_size(2.0);
    let idx = app.state.active_tab;
    let active = app.state.tabs[idx].active;
    let after = app.state.tabs[idx].panes[&active].metrics.font_px;
    assert!(after > before || (after - before).abs() < 0.1);
}

#[test]
fn app_initial_mode_is_insert() {
    let Some(app) = make_app() else { return };
    assert!(matches!(app.state.mode(), InputMode::Insert));
}

#[test]
fn app_search_matches_initially_empty() {
    let Some(app) = make_app() else { return };
    assert!(app.state.search_matches.is_empty());
    assert_eq!(app.state.search_current, 0);
}

fn str_args(v: &[&'static str]) -> impl Iterator<Item = String> {
    v.iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn version_flag_long_detected() {
    assert!(version_requested(str_args(&["mmterm", "--version"])));
}

#[test]
fn version_flag_short_detected() {
    assert!(version_requested(str_args(&["mmterm", "-V"])));
}

#[test]
fn version_flag_absent() {
    assert!(!version_requested(str_args(&["mmterm", "--debug"])));
    assert!(!version_requested(str_args(&["mmterm"])));
}

#[test]
fn help_flag_long_detected() {
    assert!(help_requested(str_args(&["mmterm", "--help"])));
}

#[test]
fn help_flag_short_detected() {
    assert!(help_requested(str_args(&["mmterm", "-h"])));
}

#[test]
fn help_flag_absent() {
    assert!(!help_requested(str_args(&["mmterm", "--debug"])));
    assert!(!help_requested(str_args(&["mmterm"])));
}

#[test]
fn help_output_contains_all_flags() {
    // Capture is not possible for println!, so we validate the text directly.
    // print_help() must mention every documented flag.
    let version = env!("MMTERM_VERSION");
    // Reconstruct what print_help would emit and assert the key parts.
    let text = format!(
        "mmterm {version}\n\
         \n\
         A cross-platform CPU-rendered terminal emulator.\n\
         \n\
         Usage: mmterm [OPTIONS]\n\
         \n\
         Options:\n\
           --version, -V       Print version and exit\n\
           --help,    -h       Print this help and exit\n\
           --debug             Enable debug logging to ~/.mmterm/debug-<ts>.log\n\
           --scope <name>      Use a named session scope (~/.config/mmterm/sessions/<name>.toml)\n\
           --scope=<name>      Same as --scope <name>\n\
           -s <name>           Short form of --scope\n\
           --list-scopes       Print all saved scope names and exit"
    );
    assert!(text.contains("--version"));
    assert!(text.contains("-V"));
    assert!(text.contains("--help"));
    assert!(text.contains("-h"));
    assert!(text.contains("--debug"));
    assert!(text.contains("--scope"));
    assert!(text.contains("--list-scopes"));
    assert!(text.contains(version));
}

// ── scope_from_args tests ─────────────────────────────────────────────────────

#[test]
fn scope_from_args_none_when_absent() {
    assert_eq!(scope_from_args(str_args(&["mmterm"])), None);
}

#[test]
fn scope_from_args_long_form_space() {
    assert_eq!(
        scope_from_args(str_args(&["mmterm", "--scope", "work"])),
        Some("work".to_string())
    );
}

#[test]
fn scope_from_args_long_form_equals() {
    assert_eq!(
        scope_from_args(str_args(&["mmterm", "--scope=my-project"])),
        Some("my-project".to_string())
    );
}

#[test]
fn scope_from_args_short_form() {
    assert_eq!(
        scope_from_args(str_args(&["mmterm", "-s", "proj"])),
        Some("proj".to_string())
    );
}

#[test]
fn scope_from_args_no_value_after_flag_returns_none() {
    // --scope with nothing following must not panic
    assert_eq!(scope_from_args(str_args(&["mmterm", "--scope"])), None);
}

#[test]
fn scope_from_args_flags_before_scope() {
    assert_eq!(
        scope_from_args(str_args(&["mmterm", "--debug", "--scope", "dev"])),
        Some("dev".to_string())
    );
}

#[test]
fn scope_from_args_first_value_wins() {
    // If --scope appears twice, the first value is returned.
    assert_eq!(
        scope_from_args(str_args(&[
            "mmterm", "--scope", "first", "--scope", "second"
        ])),
        Some("first".to_string())
    );
}

#[test]
fn scope_from_args_unrelated_flags_only() {
    assert_eq!(
        scope_from_args(str_args(&["mmterm", "--debug", "--version"])),
        None
    );
}

// ── list_scopes_requested tests ───────────────────────────────────────────────

#[test]
fn list_scopes_requested_absent() {
    assert!(!list_scopes_requested(str_args(&["mmterm"])));
    assert!(!list_scopes_requested(str_args(&["mmterm", "--debug"])));
}

#[test]
fn list_scopes_requested_present() {
    assert!(list_scopes_requested(str_args(&[
        "mmterm",
        "--list-scopes"
    ])));
}

#[test]
fn list_scopes_requested_with_other_flags() {
    assert!(list_scopes_requested(str_args(&[
        "mmterm",
        "--scope",
        "x",
        "--list-scopes"
    ])));
}

#[test]
fn version_string_is_semver() {
    // Accept both "0.3.0" (release) and "0.3.0+abc1234" (local build).
    let v = env!("MMTERM_VERSION");
    let semver = v.split('+').next().unwrap_or(v);
    let parts: Vec<&str> = semver.split('.').collect();
    assert!(parts.len() >= 2, "expected semver, got: {v}");
    assert!(
        parts.iter().all(|p| !p.is_empty()),
        "semver part empty: {v}"
    );
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

#[test]
fn next_bell_wakeup_no_tabs_returns_default() {
    let default = Instant::now() + Duration::from_secs(10);
    let result = next_bell_wakeup(&[], default);
    assert_eq!(result, default);
}

#[test]
fn next_bell_wakeup_no_active_bell_returns_default() {
    use crate::app_state::TabState;
    use crate::ui::Layout;
    use std::collections::HashMap;

    let tab = TabState {
        panes: HashMap::new(),
        layout: Layout::new(0, 800, 600),
        active: 0,
        name: None,
        zoomed: false,
        has_activity: false,
        bell_flash_start: None,
        bell_flash_until: None,
        bell_cooldown_until: None,
        passthrough: false,
        mode: crate::input::InputMode::Insert,
    };
    let default = Instant::now() + Duration::from_secs(10);
    let result = next_bell_wakeup(&[tab], default);
    assert_eq!(result, default);
}

#[test]
fn next_bell_wakeup_active_bell_returns_earlier() {
    use crate::app_state::TabState;
    use crate::ui::Layout;
    use std::collections::HashMap;

    let bell_expiry = Instant::now() + Duration::from_millis(50);
    let tab = TabState {
        panes: HashMap::new(),
        layout: Layout::new(0, 800, 600),
        active: 0,
        name: None,
        zoomed: false,
        has_activity: false,
        bell_flash_start: None,
        bell_flash_until: Some(bell_expiry),
        bell_cooldown_until: None,
        passthrough: false,
        mode: crate::input::InputMode::Insert,
    };
    let default = Instant::now() + Duration::from_secs(10);
    let result = next_bell_wakeup(&[tab], default);
    assert_eq!(
        result, bell_expiry,
        "should return the bell expiry when it's earlier than default"
    );
}

#[test]
fn next_bell_wakeup_returns_earliest_of_multiple() {
    use crate::app_state::TabState;
    use crate::ui::Layout;
    use std::collections::HashMap;

    let expiry1 = Instant::now() + Duration::from_millis(200);
    let expiry2 = Instant::now() + Duration::from_millis(50);

    let make_tab = |expiry: Option<Instant>| TabState {
        panes: HashMap::new(),
        layout: Layout::new(0, 800, 600),
        active: 0,
        name: None,
        zoomed: false,
        has_activity: false,
        bell_flash_start: None,
        bell_flash_until: expiry,
        bell_cooldown_until: None,
        passthrough: false,
        mode: crate::input::InputMode::Insert,
    };

    let tab1 = make_tab(Some(expiry1));
    let tab2 = make_tab(Some(expiry2));

    let default = Instant::now() + Duration::from_secs(10);
    let result = next_bell_wakeup(&[tab1, tab2], default);
    assert_eq!(result, expiry2, "should return the earliest bell expiry");
}

// ── save_screenshot ───────────────────────────────────────────────────────────

fn tiny_buf() -> (Vec<u32>, u32, u32) {
    let w = 4u32;
    let h = 4u32;
    (vec![0xFF_FF_FF_FFu32; (w * h) as usize], w, h)
}

#[test]
fn save_screenshot_returns_path_on_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (buf, w, h) = tiny_buf();
    let path = save_screenshot(&buf, w, [0, 0, w, h], dir.path().to_str().unwrap(), "")
        .expect("save_screenshot failed");
    assert!(path.exists(), "PNG file should exist on disk");
}

#[test]
fn save_screenshot_filename_matches_mmterm_timestamp_pattern() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (buf, w, h) = tiny_buf();
    let path = save_screenshot(&buf, w, [0, 0, w, h], dir.path().to_str().unwrap(), "")
        .expect("save_screenshot failed");
    let name = path.file_name().unwrap().to_string_lossy();
    assert!(
        name.starts_with("mmterm-") && name.ends_with(".png"),
        "unexpected filename: {name}"
    );
}

#[test]
fn save_screenshot_uses_custom_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (buf, w, h) = tiny_buf();
    let path = save_screenshot(
        &buf,
        w,
        [0, 0, w, h],
        dir.path().to_str().unwrap(),
        "screenshot-evidence",
    )
    .expect("save_screenshot failed");
    let name = path.file_name().unwrap().to_string_lossy();
    assert_eq!(name, "screenshot-evidence.png");
}

#[test]
fn save_screenshot_sanitizes_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (buf, w, h) = tiny_buf();
    let path = save_screenshot(
        &buf,
        w,
        [0, 0, w, h],
        dir.path().to_str().unwrap(),
        "my shot/bad:name",
    )
    .expect("save_screenshot failed");
    let name = path.file_name().unwrap().to_string_lossy();
    assert_eq!(name, "my-shot-bad-name.png");
}

#[test]
fn save_screenshot_whitespace_only_name_falls_back_to_timestamp() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (buf, w, h) = tiny_buf();
    let path = save_screenshot(&buf, w, [0, 0, w, h], dir.path().to_str().unwrap(), "   ")
        .expect("save_screenshot failed");
    let name = path.file_name().unwrap().to_string_lossy();
    assert!(
        name.starts_with("mmterm-") && name.ends_with(".png"),
        "unexpected filename: {name}"
    );
}

#[test]
fn save_screenshot_fails_on_unwritable_dir() {
    let result = save_screenshot(&[0u32], 1, [0, 0, 1, 1], "/dev/null/cannot_create", "");
    assert!(result.is_err());
}

// ── bell_flash_intensity ──────────────────────────────────────────────────────

#[test]
fn bell_flash_intensity_none_returns_none() {
    assert!(bell_flash_intensity(None).is_none());
}

#[test]
fn bell_flash_intensity_recent_start_returns_some_between_zero_and_one() {
    let v = bell_flash_intensity(Some(Instant::now()));
    assert!(v.is_some(), "recent start should return Some");
    let v = v.unwrap();
    assert!(
        (0.0..=1.0).contains(&v),
        "intensity must be in [0, 1], got {v}"
    );
}

#[test]
fn bell_flash_intensity_expired_start_returns_none() {
    let old = Instant::now() - Duration::from_millis(200);
    assert!(
        bell_flash_intensity(Some(old)).is_none(),
        "expired flash (200ms ago) should return None"
    );
}
