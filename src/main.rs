mod app_event;
mod app_state;
mod cli;
mod config;
mod dpi;
mod drain;
mod font;
mod geometry;
mod history;
mod input;
mod input_ops;
mod logging;
mod pane_ops;
mod pty;
mod renderer;
mod restore;
mod scaling;
mod search;
mod session;
mod terminal;
mod theme;
mod ui;
mod update;
mod winit_handler;

pub use app_state::{AppEffect, AppState, PaneEntry, TabState};
pub(crate) use cli::{
    debug_log_path, help_requested, list_scopes_requested, print_help, scope_from_args,
    version_requested,
};
pub use input::InputMode;

#[cfg(test)]
pub(crate) use renderer::render_ops::bell_flash_intensity;
#[cfg(test)]
pub(crate) use renderer::screenshot::{sanitize_screenshot_name, save_screenshot};
#[cfg(test)]
pub(crate) use winit_handler::next_bell_wakeup;

#[cfg(test)]
#[path = "main_test.rs"]
mod tests;

use config::Config;
use renderer::Renderer;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
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
    /// Pending screenshot crop [x, y, w, h]; captured in redraw() before overlays are drawn.
    pending_screenshot: Option<([u32; 4], String)>,
    /// Named session scope from `--scope <name>`; `None` means the default session.
    scope: Option<String>,
    /// Receives the daily update-check outcome.
    update_rx: Option<std::sync::mpsc::Receiver<crate::update::CheckOutcome>>,
    /// Receives the result of a Linux background self-apply (Ok(version) on success).
    update_apply_rx: Option<std::sync::mpsc::Receiver<std::io::Result<crate::update::Version>>>,
    /// Current-monitor scale (floored >= 1.0). Single source of truth — set in
    /// resumed() before the first tab and in handle_scale_changed(). No other code
    /// calls window.scale_factor() (keeps logic unit-testable; spec §5.1).
    scale: crate::dpi::Scale,
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
            pending_screenshot: None,
            scope,
            update_rx,
            update_apply_rx: None,
            scale: crate::dpi::Scale::new(1.0),
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
            if self.state.config.general.auto_update_install
                && let Ok(exe) = std::env::current_exe()
                && let crate::update::InstallTarget::Writable(path) =
                    crate::update::detect_install_target(&exe, env!("MMTERM_VERSION"))
            {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::update::apply_linux_update(&path).map(|_| v));
                });
                self.update_apply_rx = Some(rx);
                return;
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

    pub(crate) fn tab_h(&self) -> u32 {
        self.scale.chrome(crate::ui::layout::TAB_BAR_H)
    }

    pub(crate) fn status_h(&self) -> u32 {
        self.scale.chrome(crate::ui::layout::STATUS_BAR_H)
    }

    pub(crate) fn pane_padding(&self) -> u32 {
        self.scale.chrome(crate::ui::layout::PANE_PADDING)
    }

    fn handle_resize(&mut self, w: u32, h: u32) {
        for tab in &mut self.state.tabs {
            tab.layout.resize(w, h);
        }
        self.sync_all_pane_sizes();
    }

    /// Window moved to a different-DPI monitor. Ordering (spec §5.5/§5.7):
    /// update scale + every tab's metrics + sync_all_pane_sizes (a guard for the
    /// case where the physical size is unchanged and no Resized follows); do NOT
    /// layout/redraw here — the Resized event that winit emits next does that at
    /// the new physical size.
    fn handle_scale_changed(&mut self, new_scale: f64) {
        self.scale = crate::dpi::Scale::new(new_scale);
        self.renderer.scale = self.scale;
        crate::scaling::recompute_metrics_for_scale(
            &mut self.state.tabs,
            self.scale,
            &mut self.renderer,
        );
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
