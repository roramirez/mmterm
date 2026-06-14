use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, RwLock};

use crossbeam_channel::{bounded, unbounded};

use crate::drain;
use crate::terminal::grid::{Grid, GridColors};
use crate::ui::tabs;
use crate::ui::{Layout, Pane, SplitDir};
use crate::{PaneEntry, TabState, logging, pty};
use winit::event_loop::ActiveEventLoop;

use super::App;

impl App {
    pub(crate) fn spawn_pane_into(
        &mut self,
        tab_idx: usize,
        rect: [u32; 4],
        cwd: Option<std::path::PathBuf>,
    ) -> usize {
        let id = self.state.next_pane_id;
        self.state.next_pane_id += 1;
        let [_, _, w, h] = rect;
        let logical = crate::dpi::Logical(self.state.config.font.size);
        let metrics = self.renderer.make_metrics(self.scale.px(logical));
        let pad2 = self.scale.chrome(crate::ui::layout::PANE_PADDING) * 2;
        let (cols, rows) = metrics.grid_size_for(w.saturating_sub(pad2), h.saturating_sub(pad2));
        let t = &self.state.theme;
        let grid = Arc::new(RwLock::new(Grid::with_colors(
            cols,
            rows,
            GridColors {
                fg: t.foreground,
                bg: t.background,
                cursor: t.cursor,
                selection: t.selection,
                palette: t.palette,
            },
            self.state.config.terminal.scrollback_lines,
        )));
        let pane = Pane::new(grid.clone(), rect);
        // Bounded channel caps PTY output backlog at ~1 MB (256 × 4 KB chunks),
        // matching WezTerm's socketpair size. Provides natural backpressure:
        // when full the PTY reader blocks, slowing the child process rather than
        // allowing unbounded memory growth during heavy output (e.g. find /).
        let (pty_tx, pty_rx) = bounded::<Vec<u8>>(256);
        let shell = self
            .state
            .config
            .shell
            .program
            .clone()
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/bash".to_string());
        // Wakeup fires from the parser thread after each parsed batch.
        // Uses the app-level wakeup_pending flag (same Arc the main thread reads)
        // so the parser can cooperatively yield when the render thread is behind.
        let proxy = self.proxy.clone();
        let app_wakeup_pending = Arc::clone(&self.wakeup_pending);
        let wakeup = Box::new(move || {
            if !app_wakeup_pending.swap(true, Ordering::AcqRel) {
                let _ = proxy.send_event(());
            }
        });
        match pty::PtySession::spawn_with_shell(
            cols as u16,
            rows as u16,
            pty_tx,
            &shell,
            cwd.as_ref(),
            // PTY reader thread no longer calls wakeup; parser thread handles it.
            Box::new(|| {}),
        ) {
            Ok(pty) => {
                let log_file_opt = if self.state.config.logging.auto_log {
                    open_log_file(id, &self.state.config.logging.log_dir)
                } else {
                    None
                };
                let log_file = Arc::new(Mutex::new(log_file_opt));
                let pending_resize: Arc<Mutex<Option<(usize, usize)>>> = Arc::new(Mutex::new(None));
                let (effects_tx, effects_rx) = unbounded::<drain::ParseEffect>();
                let parser_thread = drain::spawn_parser_thread(drain::ParserThreadArgs {
                    rx: pty_rx,
                    grid,
                    log_file: log_file.clone(),
                    effects_tx,
                    wakeup_pending: Arc::clone(&self.wakeup_pending),
                    pending_resize: pending_resize.clone(),
                    wakeup,
                });
                self.state.tabs[tab_idx].panes.insert(
                    id,
                    PaneEntry {
                        pane,
                        pty,
                        effects_rx,
                        log_file,
                        pending_resize,
                        _parser_thread: parser_thread,
                        logical_font_size: logical,
                        metrics,
                    },
                );
            }
            Err(e) => log::error!("PTY spawn failed: {e}"),
        }
        id
    }

    pub(crate) fn spawn_pane(&mut self, rect: [u32; 4]) -> usize {
        let cwd = self
            .state
            .tabs
            .get(self.state.active_tab)
            .and_then(|t| t.panes.get(&t.active))
            .and_then(|e| e.pty.cwd());
        self.spawn_pane_into(self.state.active_tab, rect, cwd)
    }

    pub(crate) fn new_tab(&mut self, win_w: u32, win_h: u32) {
        let cwd = self
            .state
            .tabs
            .get(self.state.active_tab)
            .and_then(|t| t.panes.get(&t.active))
            .and_then(|e| e.pty.cwd());
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let layout = Layout::new(0, win_w, win_h);
        let initial_rect = layout
            .rects_scaled(tab_h, status_h)
            .first()
            .map(|(_, r)| *r)
            .unwrap_or([0, tab_h, win_w, win_h]);
        let tab_idx = self.state.tabs.len();
        self.state.tabs.push(TabState {
            panes: HashMap::new(),
            layout: Layout::new(0, win_w, win_h),
            active: 0,
            name: None,
            zoomed: false,
            has_activity: false,
            bell_flash_start: None,
            bell_flash_until: None,
            bell_cooldown_until: None,
            passthrough: false,
        });
        let id = self.spawn_pane_into(tab_idx, initial_rect, cwd);
        self.state.tabs[tab_idx].layout = Layout::new(id, win_w, win_h);
        self.state.tabs[tab_idx].active = id;
        self.state.active_tab = tab_idx;
    }

    pub(crate) fn close_tab(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.tabs.len() == 1 {
            event_loop.exit();
            return;
        }
        let old_active = self.state.active_tab;
        let old_count = self.state.tabs.len();
        self.state.tabs.remove(old_active);
        self.state.active_tab = tabs::close_tab_index(old_active, old_count);
    }

    pub(crate) fn close_pane_on_tab(
        &mut self,
        tab_idx: usize,
        pane_id: usize,
        event_loop: &ActiveEventLoop,
    ) {
        if tab_idx >= self.state.tabs.len() {
            return;
        }
        if self.state.tabs[tab_idx].panes.len() == 1 {
            if self.state.tabs.len() == 1 {
                event_loop.exit();
                return;
            }
            self.state.tabs.remove(tab_idx);
            if self.state.active_tab >= self.state.tabs.len() {
                self.state.active_tab = self.state.tabs.len() - 1;
            }
            return;
        }
        let tab = &mut self.state.tabs[tab_idx];
        let new_focus = tab.layout.remove(pane_id);
        tab.panes.remove(&pane_id);
        if tab.active == pane_id {
            tab.active = new_focus.unwrap_or_else(|| *tab.panes.keys().next().unwrap());
        }
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let pane_padding = self.pane_padding();
        Self::sync_pane_sizes_tab(&mut self.state.tabs[tab_idx], tab_h, status_h, pane_padding);
    }

    pub(crate) fn sync_pane_sizes_tab(
        tab: &mut TabState,
        tab_h: u32,
        status_h: u32,
        pane_padding: u32,
    ) {
        // rows*cell_height may be < pane_h by up to (cell_height-1)px — intentional bottom gutter; do not force equality.
        let rects = tab.layout.rects_scaled(tab_h, status_h);
        for (id, rect) in rects {
            if let Some(entry) = tab.panes.get_mut(&id) {
                let [_, _, w, h] = rect;
                let pad2 = pane_padding * 2;
                let (cols, rows) = entry
                    .metrics
                    .grid_size_for(w.saturating_sub(pad2), h.saturating_sub(pad2));
                let (grid_cols, grid_rows) = {
                    let g = entry.pane.grid.read().unwrap();
                    (g.cols, g.rows)
                };
                if grid_cols != cols || grid_rows != rows {
                    // Update rect immediately so the renderer uses the new layout
                    // on the very next frame (no lock needed — rect is main-thread state).
                    entry.pane.rect = rect;
                    // Signal the parser thread to apply grid.resize() within its
                    // existing write lock. This avoids blocking the event loop on
                    // grid.write() while the parser holds it (up to ~36 ms), keeping
                    // resize fluid even during heavy output.
                    *entry.pending_resize.lock().unwrap() = Some((cols, rows));
                    let _ = entry.pty.resize(cols as u16, rows as u16);
                }
            }
        }
    }

    pub(crate) fn sync_all_pane_sizes(&mut self) {
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let pane_padding = self.pane_padding();
        for tab in &mut self.state.tabs {
            Self::sync_pane_sizes_tab(tab, tab_h, status_h, pane_padding);
        }
    }

    pub(crate) fn do_split(&mut self, dir: SplitDir) {
        self.tab_mut().zoomed = false;
        let active = self.tab().active;
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let active_rect = self
            .tab()
            .layout
            .rects_scaled(tab_h, status_h)
            .into_iter()
            .find(|(id, _)| *id == active)
            .map(|(_, r)| r)
            .unwrap_or([0, tab_h, 100, 100]);

        let new_rect = match dir {
            SplitDir::H => {
                let hw = active_rect[2] / 2;
                [active_rect[0] + hw, active_rect[1], hw, active_rect[3]]
            }
            SplitDir::V => {
                let hh = active_rect[3] / 2;
                [active_rect[0], active_rect[1] + hh, active_rect[2], hh]
            }
        };

        let new_id = self.spawn_pane(new_rect);
        let tab = self.tab_mut();
        tab.layout.split(active, new_id, dir);
        tab.active = new_id;
        let idx = self.state.active_tab;
        let pane_padding = self.pane_padding();
        Self::sync_pane_sizes_tab(&mut self.state.tabs[idx], tab_h, status_h, pane_padding);
    }

    pub(crate) fn do_close_pane(&mut self, event_loop: &ActiveEventLoop) {
        self.tab_mut().zoomed = false;
        let tab = self.tab_mut();
        if tab.panes.len() == 1 {
            let _ = tab;
            self.close_tab(event_loop);
            return;
        }
        let active = tab.active;
        let new_focus = tab.layout.remove(active);
        tab.panes.remove(&active);
        tab.active = new_focus.unwrap_or_else(|| *tab.panes.keys().next().unwrap());
        let idx = self.state.active_tab;
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let pane_padding = self.pane_padding();
        Self::sync_pane_sizes_tab(&mut self.state.tabs[idx], tab_h, status_h, pane_padding);
    }
}

pub(super) fn open_log_file(pane_id: usize, log_dir: &str) -> Option<std::fs::File> {
    let dir = logging::resolve_log_dir(log_dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Failed to create log directory {dir}: {e}");
        return None;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = logging::log_file_path(&dir, ts, pane_id);
    match std::fs::File::create(&path) {
        Ok(f) => {
            log::info!("Logging started: {path}");
            Some(f)
        }
        Err(e) => {
            log::warn!("Failed to open log file {path}: {e}");
            None
        }
    }
}

#[cfg(test)]
#[path = "pane_ops_test.rs"]
mod tests;
