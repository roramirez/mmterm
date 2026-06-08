mod app_event;
mod app_state;
mod cli;
mod command_palette;
mod config;
mod drain;
mod font;
mod geometry;
mod history;
mod input;
mod input_ops;
mod logging;
mod motion;
mod mouse;
mod mouse_ops;
mod pane_ops;
mod pty;
mod render_ops;
mod renderer;
mod restore;
mod screenshot;
mod search;
mod session;
mod statusbar;
mod tabs;
mod terminal;
mod theme;
mod tui_config;
mod ui;
mod update;
mod views;
mod winit_handler;

pub use app_state::{AppEffect, AppState, PaneEntry, TabState};
pub(crate) use cli::{
    debug_log_path, help_requested, list_scopes_requested, print_help, scope_from_args,
    version_requested,
};
pub use input::InputMode;

#[cfg(test)]
pub(crate) use render_ops::bell_flash_intensity;
#[cfg(test)]
pub(crate) use screenshot::{sanitize_screenshot_name, save_screenshot};
#[cfg(test)]
pub(crate) use winit_handler::next_bell_wakeup;

#[cfg(test)]
#[path = "main_test.rs"]
mod tests;

use config::Config;
use renderer::Renderer;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use winit::event::Modifiers;
use winit::event_loop::{EventLoop, EventLoopProxy};
use winit::window::Window;

use crate::theme::{default_theme, install_bundled_themes, load_theme, themes_dir};

// ── App ──────────────────────────────────────────────────────────────────────

struct App {
    state: AppState,
    // ── winit / rendering infrastructure ────────────────────────────────────
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    renderer: Renderer,
    modifiers: Modifiers,
    proxy: EventLoopProxy<()>,
    surface_size: (u32, u32),
    wakeup_pending: Arc<AtomicBool>,
    /// Timestamp of the last frame where PTY data was actually consumed.
    /// Used to drive a vsync-style render loop while output is flowing.
    last_pty_data: Option<Instant>,
    /// Pending screenshot crop [x, y, w, h]; captured in redraw() before overlays are drawn.
    pending_screenshot: Option<([u32; 4], String)>,
    /// Named session scope from `--scope <name>`; `None` means the default session.
    scope: Option<String>,
    /// Receives the daily update-check outcome.
    update_rx: Option<std::sync::mpsc::Receiver<crate::update::CheckOutcome>>,
    /// Receives the result of a Linux background self-apply (Ok(version) on success).
    update_apply_rx: Option<std::sync::mpsc::Receiver<std::io::Result<crate::update::Version>>>,
}

impl App {
    fn new(config: Config, proxy: EventLoopProxy<()>, scope: Option<String>) -> Self {
        let renderer = Renderer::new(&config.font.family, config.font.size);
        let td = themes_dir();
        install_bundled_themes(&td);
        let theme = load_theme(&config.theme.name, &td).unwrap_or_else(|e| {
            log::warn!("{e} — using default theme");
            default_theme()
        });
        let wakeup_pending = Arc::new(AtomicBool::new(false));
        let mut state = AppState::new(config, theme);
        state.search_history = history::load_search_history();

        let mut update_rx = None;
        if state.config.general.auto_update_check
            && let Some(current) = crate::update::Version::parse(env!("MMTERM_VERSION"))
        {
            let (tx, rx) = std::sync::mpsc::channel();
            crate::update::spawn_check(current, crate::update::state_path(), tx);
            update_rx = Some(rx);
        }

        Self {
            state,
            window: None,
            surface: None,
            renderer,
            modifiers: Modifiers::default(),
            proxy,
            surface_size: (0, 0),
            wakeup_pending,
            last_pty_data: None,
            pending_screenshot: None,
            scope,
            update_rx,
            update_apply_rx: None,
        }
    }

    fn poll_update(&mut self) {
        let mut changed = false;

        // Drain into locals first so we can mutate `self` without holding a
        // borrow on the channel fields.
        let mut newer = Vec::new();
        if let Some(rx) = &self.update_rx {
            while let Ok(outcome) = rx.try_recv() {
                if let crate::update::CheckOutcome::Newer(v) = outcome {
                    newer.push(v);
                }
            }
        }
        for v in newer {
            self.on_update_available(v);
            changed = true;
        }

        let mut applied = Vec::new();
        if let Some(rx) = &self.update_apply_rx {
            while let Ok(res) = rx.try_recv() {
                applied.push(res);
            }
        }
        // Err results are silent no-ops, leaving the running binary untouched.
        for v in applied.into_iter().flatten() {
            self.state.update_applied = Some(v);
            self.state.available_update = None;
            changed = true;
        }

        if changed {
            self.request_redraw();
        }
    }

    fn on_update_available(&mut self, v: crate::update::Version) {
        #[cfg(target_os = "linux")]
        {
            if self.state.config.general.auto_update_install {
                if let Ok(exe) = std::env::current_exe() {
                    if let crate::update::InstallTarget::Writable(path) =
                        crate::update::detect_install_target(&exe, env!("MMTERM_VERSION"))
                    {
                        let (tx, rx) = std::sync::mpsc::channel();
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::update::apply_linux_update(&path).map(|_| v));
                        });
                        self.update_apply_rx = Some(rx);
                        return;
                    }
                }
            }
            self.state.available_update = Some(v); // not eligible -> notify only
        }
        #[cfg(target_os = "macos")]
        {
            self.state.available_update = Some(v);
        }
    }

    fn tab(&self) -> &TabState {
        self.state.tab()
    }

    fn tab_mut(&mut self) -> &mut TabState {
        self.state.tab_mut()
    }

    fn session_path(&self) -> std::path::PathBuf {
        session::session_path_for(self.scope.as_deref())
    }

    fn handle_resize(&mut self, w: u32, h: u32) {
        for tab in &mut self.state.tabs {
            tab.layout.resize(w, h);
        }
        self.sync_all_pane_sizes();
    }
}

fn init_logging(log_path: Option<&str>) {
    let level = if log_path.is_some() {
        log::LevelFilter::Debug
    } else {
        std::env::var("RUST_LOG")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(log::LevelFilter::Warn)
    };

    let mut dispatch = fern::Dispatch::new().level(level).chain(
        fern::Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!("[{}] {}", record.level(), message))
            })
            .chain(std::io::stderr()),
    );

    if let Some(path) = log_path {
        match fern::log_file(path) {
            Ok(file) => {
                dispatch = dispatch.chain(
                    fern::Dispatch::new()
                        .format(|out, message, record| {
                            out.finish(format_args!(
                                "{ts} [{level}] {target} — {msg}",
                                ts = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f"),
                                level = record.level(),
                                target = record.target(),
                                msg = message
                            ))
                        })
                        .chain(file),
                );
            }
            Err(e) => {
                eprintln!("mmterm: could not open debug log {path}: {e}");
            }
        }
    }

    if let Err(e) = dispatch.apply() {
        eprintln!("mmterm: logging init failed: {e}");
    }
}

fn main() {
    if version_requested(std::env::args()) {
        println!("mmterm {}", env!("MMTERM_VERSION"));
        return;
    }

    if help_requested(std::env::args()) {
        print_help();
        return;
    }

    let log_path = debug_log_path();
    init_logging(log_path.as_deref());

    if let Some(ref path) = log_path {
        let p = path.clone();
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log::error!("panic: {info}");
            default_hook(info);
            eprintln!("\nmmterm: debug log saved to {p}");
        }));
        log::info!("debug logging enabled → {path}");
    }

    if list_scopes_requested(std::env::args()) {
        for name in session::list_scopes() {
            println!("{name}");
        }
        return;
    }

    let scope = scope_from_args(std::env::args());

    Config::write_default_if_missing();
    let config = Config::load();
    let event_loop = EventLoop::new().unwrap();
    let proxy = event_loop.create_proxy();
    let mut app = App::new(config, proxy, scope);
    event_loop.run_app(&mut app).unwrap();
}
