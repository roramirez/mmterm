use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crossbeam_channel::unbounded;

use crate::terminal::grid::GridColors;
use crate::ui::{Layout, Pane, SplitDir};
use crate::{PaneEntry, TabState, logging, pty, tabs};
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
        let pad2 = self.scale.chrome(crate::ui::layout::PANE_PADDING) * 2;
        let (cols, rows) = self.state.tabs[tab_idx]
            .metrics
            .grid_size_for(w.saturating_sub(pad2), h.saturating_sub(pad2));
        let t = &self.state.theme;
        let pane = Pane::new_with_colors(
            cols,
            rows,
            rect,
            GridColors {
                fg: t.foreground,
                bg: t.background,
                cursor: t.cursor,
                selection: t.selection,
                palette: t.palette,
            },
            self.state.config.terminal.scrollback_lines,
        );
        let (tx, rx) = unbounded::<Vec<u8>>();
        let shell = self
            .state
            .config
            .shell
            .program
            .clone()
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/bash".to_string());
        let proxy = self.proxy.clone();
        let wakeup_pending = Arc::clone(&self.wakeup_pending);
        let wakeup = Box::new(move || {
            if !wakeup_pending.swap(true, Ordering::AcqRel) {
                let _ = proxy.send_event(());
            }
        });
        match pty::PtySession::spawn_with_shell(
            cols as u16,
            rows as u16,
            tx,
            &shell,
            cwd.as_ref(),
            wakeup,
        ) {
            Ok(pty) => {
                let log_file = if self.state.config.logging.auto_log {
                    open_log_file(id, &self.state.config.logging.log_dir)
                } else {
                    None
                };
                self.state.tabs[tab_idx].panes.insert(
                    id,
                    PaneEntry {
                        pane,
                        pty,
                        rx,
                        log_file,
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
        let logical = crate::dpi::Logical(self.state.config.font.size);
        let metrics = self.renderer.make_metrics(self.scale.px(logical));
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
            metrics,
            logical_font_size: crate::dpi::Logical(self.state.config.font.size),
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
                let (cols, rows) = tab
                    .metrics
                    .grid_size_for(w.saturating_sub(pad2), h.saturating_sub(pad2));
                if entry.pane.parser.grid.cols != cols || entry.pane.parser.grid.rows != rows {
                    entry.pane.resize(cols, rows, rect);
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
