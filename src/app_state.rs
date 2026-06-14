use arboard::Clipboard;
use crossbeam_channel::Receiver;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::config::Config;
use crate::config::tui_config::ConfigPanel;
use crate::dpi::Logical;
use crate::input::InputMode;
use crate::input::keybindings::Action;
use crate::renderer::FontMetrics;
use crate::theme::ResolvedTheme;
use crate::ui::{Layout, Pane, SeparatorHandle, SplitDir};

// ── Screenshot helpers ───────────────────────────────────────────────────────

// ── Re-exports so main.rs can still use these types ─────────────────────────

pub struct PaneEntry {
    pub pane: Pane,
    pub pty: crate::pty::PtySession,
    pub effects_rx: Receiver<crate::drain::ParseEffect>,
    pub log_file: Arc<Mutex<Option<std::fs::File>>>,
    /// Non-blocking resize request: main thread writes Some((cols, rows));
    /// parser thread applies grid.resize() within its existing write lock and
    /// clears the Option. Avoids blocking the event loop on grid.write().
    pub pending_resize: Arc<Mutex<Option<(usize, usize)>>>,
    pub(crate) _parser_thread: std::thread::JoinHandle<()>,
    /// Per-pane density-independent font size. Physical px = scale.px(logical_font_size).
    /// Mutated by Ctrl±/reset; re-derived (not persisted) on ScaleFactorChanged.
    pub logical_font_size: Logical,
    /// Cell layout derived from `scale.px(logical_font_size)`.
    pub metrics: FontMetrics,
}

pub struct TabState {
    pub panes: HashMap<usize, PaneEntry>,
    pub layout: Layout,
    pub active: usize,
    pub name: Option<String>,
    pub zoomed: bool,
    pub has_activity: bool,
    pub bell_flash_start: Option<Instant>,
    pub bell_flash_until: Option<Instant>,
    pub bell_cooldown_until: Option<Instant>,
    pub passthrough: bool,
}

// ── Side-effects reported back to the winit App ──────────────────────────────

/// Actions that AppState cannot perform itself (they need winit, renderer, or
/// PTY) are returned as `AppEffect` values for `App` to execute.
#[derive(Debug)]
pub enum AppEffect {
    Redraw,
    Quit,
    QuitPending,
    ToggleFullscreen,
    NewTab,
    ClosePane,
    CloseTab,
    SplitPane(SplitDir),
    AutoSplitPane,
    ChangeFontSize(f32),
    ToggleLog,
    SendToPty(Vec<u8>),
    Paste,
    ResizePane { split_h: bool, delta: f32 },
    RotatePanes(bool),
    SaveSessionAndQuit,
    ScreenshotOpen,
}

// ── AppState ─────────────────────────────────────────────────────────────────

pub struct AppState {
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
    pub next_pane_id: usize,
    pub mode: InputMode,
    pub cursor_blink: bool,
    pub blink_last: Instant,
    pub ctrl_w_pending: bool,
    pub quit_pending: bool,
    pub config: Config,
    pub config_panel: Option<ConfigPanel>,
    pub clipboard: Option<Clipboard>,
    pub mouse_pos: Option<(f64, f64)>,
    pub mouse_selecting: bool,
    pub search_matches: Vec<(usize, usize, usize)>,
    pub search_current: usize,
    pub search_history: Vec<String>,
    pub search_before_history: String,
    pub hovered_url: Option<String>,
    pub swallow_next_tab: bool,
    pub theme: ResolvedTheme,
    pub drag_separator: Option<SeparatorHandle>,
    /// A newer version is available (drives the status-bar badge).
    pub available_update: Option<crate::update::Version>,
    /// An update was self-applied this session (Linux) — drives the "restart" badge.
    pub update_applied: Option<crate::update::Version>,
}

impl AppState {
    pub fn new(config: Config, theme: ResolvedTheme) -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: 0,
            next_pane_id: 0,
            mode: InputMode::Insert,
            cursor_blink: true,
            blink_last: Instant::now(),
            ctrl_w_pending: false,
            quit_pending: false,
            config_panel: None,
            clipboard: Clipboard::new().ok(),
            mouse_pos: None,
            mouse_selecting: false,
            search_matches: Vec::new(),
            search_current: 0,
            search_history: Vec::new(),
            search_before_history: String::new(),
            hovered_url: None,
            swallow_next_tab: false,
            theme,
            config,
            drag_separator: None,
            available_update: None,
            update_applied: None,
        }
    }

    // ── Tab helpers ──────────────────────────────────────────────────────────

    pub fn tab(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    pub fn tab_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    pub fn next_tab(&mut self) {
        self.active_tab = crate::ui::tabs::next_tab_index(self.active_tab, self.tabs.len());
    }

    pub fn prev_tab(&mut self) {
        self.active_tab = crate::ui::tabs::prev_tab_index(self.active_tab, self.tabs.len());
    }

    pub fn move_tab_left(&mut self) {
        let new = crate::ui::tabs::move_tab_index(self.active_tab, self.tabs.len(), true);
        if new != self.active_tab {
            self.tabs.swap(self.active_tab, new);
            self.active_tab = new;
        }
    }

    pub fn move_tab_right(&mut self) {
        let new = crate::ui::tabs::move_tab_index(self.active_tab, self.tabs.len(), false);
        if new != self.active_tab {
            self.tabs.swap(self.active_tab, new);
            self.active_tab = new;
        }
    }

    pub fn focus_dir(&mut self, dx: i32, dy: i32) {
        let active = self.tab().active;
        if let Some(id) = self.tab().layout.focus_dir(active, dx, dy) {
            self.tab_mut().active = id;
        }
    }

    pub fn focus_next(&mut self) {
        let active = self.tab().active;
        let leaves = self.tab().layout.leaves();
        self.tab_mut().active = crate::ui::tabs::next_pane_in_layout(&leaves, active);
    }

    // ── Search ───────────────────────────────────────────────────────────────

    pub fn push_search_history(&mut self, query: String) {
        if query.is_empty() {
            return;
        }
        self.search_history.retain(|q| q != &query);
        self.search_history.push(query);
        if self.search_history.len() > 50 {
            self.search_history.remove(0);
        }
        self.search_before_history.clear();
    }

    pub fn update_search_matches(&mut self) {
        let query = match &self.mode {
            InputMode::Search { query, .. } => query.clone(),
            _ => return,
        };
        self.search_matches.clear();
        self.search_current = 0;
        if self.tabs.is_empty() {
            return;
        }
        let tab_idx = self.active_tab;
        let active = self.tabs[tab_idx].active;
        if let Some(entry) = self.tabs[tab_idx].panes.get(&active) {
            let grid = entry.pane.grid.read().unwrap();
            self.search_matches = crate::search::compute_search_matches(&grid, &query);
        }
        if !self.search_matches.is_empty() {
            self.scroll_to_match(0);
        }
    }

    pub fn scroll_to_match(&mut self, idx: usize) {
        if idx >= self.search_matches.len() {
            return;
        }
        self.search_current = idx;
        let (abs_row, _, _) = self.search_matches[idx];
        if self.tabs.is_empty() {
            return;
        }
        let tab_idx = self.active_tab;
        let active = self.tabs[tab_idx].active;
        let (sb_len, grid_rows) = self.tabs[tab_idx]
            .panes
            .get(&active)
            .map(|e| {
                let g = e.pane.grid.read().unwrap();
                (g.scrollback.len(), g.rows)
            })
            .unwrap_or((0, 24));
        let new_offset = crate::search::compute_scroll_offset(abs_row, sb_len, grid_rows);
        if let Some(entry) = self.tabs[tab_idx].panes.get_mut(&active) {
            entry.pane.scroll_offset = new_offset;
        }
    }

    pub fn open_config_panel(&mut self) {
        self.config_panel = Some(ConfigPanel::from_config(&self.config));
    }

    // ── Active pane accessors ────────────────────────────────────────────────

    fn active_entry(&self) -> Option<&PaneEntry> {
        if self.tabs.is_empty() {
            return None;
        }
        let active = self.tab().active;
        self.tab().panes.get(&active)
    }

    fn active_entry_mut(&mut self) -> Option<&mut PaneEntry> {
        if self.tabs.is_empty() {
            return None;
        }
        let active = self.tab().active;
        self.tab_mut().panes.get_mut(&active)
    }

    fn active_grid_rows(&self) -> usize {
        self.active_entry()
            .map(|e| e.pane.grid.read().unwrap().rows)
            .unwrap_or(1)
    }

    // ── Visual motion helper ─────────────────────────────────────────────────

    fn move_visual_cursor(
        &mut self,
        motion: impl Fn(&crate::terminal::grid::Grid, usize, usize, usize) -> (usize, usize),
    ) {
        let InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            cur_row,
            anchored,
        } = self.mode.clone()
        else {
            return;
        };
        let Some(entry) = self.active_entry() else {
            return;
        };
        let (nc, nr) = motion(
            &entry.pane.grid.read().unwrap(),
            entry.pane.scroll_offset,
            cur_col,
            cur_row,
        );
        self.mode = InputMode::Visual {
            start_col,
            start_row,
            cur_col: nc,
            cur_row: nr,
            anchored,
        };
    }

    // ── Clipboard helper ─────────────────────────────────────────────────────

    pub(crate) fn copy_text_to_clipboard(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        if self.clipboard.is_none() {
            self.clipboard = Clipboard::new().ok();
        }
        if let Some(cb) = self.clipboard.as_mut() {
            let _ = cb.set_text(text);
        }
    }

    // ── Visual scroll adjustment ─────────────────────────────────────────────

    fn adjust_visual_scroll_up(&mut self, n: usize, grid_rows: usize) {
        if let InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            cur_row,
            anchored,
        } = self.mode.clone()
        {
            self.mode = InputMode::Visual {
                start_col,
                start_row: start_row + n,
                cur_col,
                cur_row: (cur_row + n).min(grid_rows.saturating_sub(1)),
                anchored,
            };
        }
    }

    fn adjust_visual_scroll_down(&mut self, n: usize) {
        if let InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            cur_row,
            anchored,
        } = self.mode.clone()
        {
            self.mode = InputMode::Visual {
                start_col,
                start_row: start_row.saturating_sub(n),
                cur_col,
                cur_row: cur_row.saturating_sub(n),
                anchored,
            };
        }
    }

    // ── Cursor-position for entering Visual mode ─────────────────────────────

    fn visual_start_pos(&self) -> (usize, usize) {
        self.active_entry()
            .map(|e| {
                if e.pane.scroll_offset > 0 {
                    (0, 0)
                } else {
                    let g = e.pane.grid.read().unwrap();
                    (g.cursor_col, g.cursor_row)
                }
            })
            .unwrap_or((0, 0))
    }

    // ── Visual sub-dispatch ──────────────────────────────────────────────────

    fn do_visual_copy(&mut self) {
        if let InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            cur_row,
            anchored: true,
        } = self.mode.clone()
        {
            let text = self.active_entry().map(|entry| {
                entry.pane.grid.read().unwrap().selected_text(
                    start_col,
                    start_row,
                    cur_col,
                    cur_row,
                    entry.pane.scroll_offset,
                )
            });
            if let Some(text) = text {
                self.copy_text_to_clipboard(text);
            }
            self.mode = InputMode::Insert;
        }
    }

    fn do_visual_yank_line(&mut self) {
        if let InputMode::Visual { cur_row, .. } = self.mode.clone() {
            let text = self.active_entry().map(|entry| {
                let grid = entry.pane.grid.read().unwrap();
                let cols = grid.cols.saturating_sub(1);
                grid.selected_text(0, cur_row, cols, cur_row, entry.pane.scroll_offset)
            });
            if let Some(text) = text {
                self.copy_text_to_clipboard(text);
            }
            self.mode = InputMode::Insert;
        }
    }

    fn swap_visual_anchor(&mut self) {
        if let InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            cur_row,
            anchored,
        } = self.mode.clone()
        {
            let grid_rows = self.active_grid_rows();
            self.mode = InputMode::Visual {
                start_col: cur_col,
                start_row: cur_row,
                cur_col: start_col,
                cur_row: start_row.min(grid_rows.saturating_sub(1)),
                anchored,
            };
        }
    }

    fn set_visual_anchor(&mut self) {
        if let InputMode::Visual {
            cur_col, cur_row, ..
        } = self.mode.clone()
        {
            self.mode = InputMode::Visual {
                start_col: cur_col,
                start_row: cur_row,
                cur_col,
                cur_row,
                anchored: true,
            };
        }
    }

    fn dispatch_visual_action(&mut self, action: Action) -> Vec<AppEffect> {
        match action {
            Action::Copy => self.do_visual_copy(),
            Action::VisualSwapAnchor => self.swap_visual_anchor(),
            Action::VisualAnchor => self.set_visual_anchor(),
            Action::VisualWordForward => {
                self.move_visual_cursor(crate::input::motion::word_forward)
            }
            Action::VisualWordBackward => {
                self.move_visual_cursor(crate::input::motion::word_backward)
            }
            Action::VisualWordEnd => self.move_visual_cursor(crate::input::motion::word_end),
            Action::VisualYankLine => self.do_visual_yank_line(),
            _ => {}
        }
        vec![AppEffect::Redraw]
    }

    fn visual_boundary_scroll_up(&mut self, n: usize) {
        if let Some(e) = self.active_entry_mut() {
            e.pane.scroll_up(n);
        }
        if let InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            anchored,
            ..
        } = self.mode.clone()
        {
            self.mode = InputMode::Visual {
                start_col,
                start_row: start_row + n,
                cur_col,
                cur_row: 0,
                anchored,
            };
        }
    }

    fn visual_boundary_scroll_down(&mut self, n: usize) {
        let grid_rows = self.active_grid_rows();
        if let Some(e) = self.active_entry_mut() {
            e.pane.scroll_down(n);
        }
        if let InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            anchored,
            ..
        } = self.mode.clone()
        {
            self.mode = InputMode::Visual {
                start_col,
                start_row: start_row.saturating_sub(n),
                cur_col,
                cur_row: grid_rows.saturating_sub(1),
                anchored,
            };
        }
    }

    fn do_scroll_up(&mut self, n: usize) {
        let grid_rows = self.active_grid_rows();
        if let Some(e) = self.active_entry_mut() {
            e.pane.scroll_up(n);
        }
        self.adjust_visual_scroll_up(n, grid_rows);
    }

    fn do_scroll_down(&mut self, n: usize) {
        if let Some(e) = self.active_entry_mut() {
            e.pane.scroll_down(n);
        }
        self.adjust_visual_scroll_down(n);
    }

    /// Scroll the active pane's viewport by `lines` (positive = up, negative = down),
    /// adjusting any active Visual selection so it tracks the content.
    /// Called by mouse-wheel events; tested separately from dispatch_action.
    pub(crate) fn viewport_scroll(&mut self, lines: f32) {
        let n = lines.abs().ceil() as usize;
        if lines > 0.0 {
            self.do_scroll_up(n);
        } else {
            self.do_scroll_down(n);
        }
    }

    fn do_scroll_top(&mut self) {
        if let Some(e) = self.active_entry_mut() {
            e.pane.scroll_top();
        }
    }

    fn do_scroll_bottom(&mut self) {
        if let Some(e) = self.active_entry_mut() {
            e.pane.scroll_bottom();
        }
    }

    fn do_go_to_tab(&mut self, idx: usize) -> Vec<AppEffect> {
        if idx < self.tabs.len() {
            self.active_tab = idx;
            vec![AppEffect::Redraw]
        } else {
            vec![]
        }
    }

    fn do_zoom_pane(&mut self) {
        if !self.tabs.is_empty() {
            let zoomed = self.tab().zoomed;
            self.tab_mut().zoomed = !zoomed;
        }
    }

    fn do_reset_font_size(&self) -> Vec<AppEffect> {
        let default_logical = self.config.font.size;
        let current = self
            .tabs
            .get(self.active_tab)
            .and_then(|t| t.panes.get(&t.active))
            .map(|e| e.logical_font_size.0)
            .unwrap_or(default_logical);
        vec![AppEffect::ChangeFontSize(default_logical - current)]
    }

    // ── Action dispatch ──────────────────────────────────────────────────────
    //
    // Handles all actions that are pure state mutations. Returns `AppEffect`s
    // for anything that requires winit, the renderer, or the PTY.

    pub fn dispatch_action(&mut self, action: Action) -> Vec<AppEffect> {
        match action {
            // ── Ctrl-W prefix ────────────────────────────────────────────────
            Action::CtrlWPrefix => {
                self.ctrl_w_pending = true;
                vec![]
            }

            // ── PTY / IO (delegated to App) ──────────────────────────────────
            Action::SendToPty(bytes) => {
                self.do_scroll_bottom();
                vec![AppEffect::SendToPty(bytes)]
            }
            Action::Paste => vec![AppEffect::Paste],

            // ── Mode ─────────────────────────────────────────────────────────
            Action::SetMode(new_mode) => self.do_set_mode(new_mode),

            // ── Scroll ───────────────────────────────────────────────────────
            Action::ScrollUp(n) => {
                self.do_scroll_up(n);
                vec![AppEffect::Redraw]
            }
            Action::ScrollDown(n) => {
                self.do_scroll_down(n);
                vec![AppEffect::Redraw]
            }
            Action::ScrollToTop => {
                self.do_scroll_top();
                vec![AppEffect::Redraw]
            }
            Action::ScrollToBottom => {
                self.do_scroll_bottom();
                vec![AppEffect::Redraw]
            }
            Action::ClearScrollback => {
                self.do_clear_scrollback();
                vec![AppEffect::Redraw]
            }

            // ── Copy / Visual ────────────────────────────────────────────────
            Action::Copy
            | Action::VisualSwapAnchor
            | Action::VisualAnchor
            | Action::VisualWordForward
            | Action::VisualWordBackward
            | Action::VisualWordEnd
            | Action::VisualYankLine => self.dispatch_visual_action(action),
            Action::VisualBoundaryUp(n) => {
                self.visual_boundary_scroll_up(n);
                vec![AppEffect::Redraw]
            }
            Action::VisualBoundaryDown(n) => {
                self.visual_boundary_scroll_down(n);
                vec![AppEffect::Redraw]
            }

            // ── Pane / split (need PTY resize → delegated) ───────────────────
            Action::SplitH => vec![AppEffect::SplitPane(SplitDir::H)],
            Action::SplitV => vec![AppEffect::SplitPane(SplitDir::V)],
            Action::AutoSplit => vec![AppEffect::AutoSplitPane],
            Action::ClosePane => vec![AppEffect::ClosePane],
            Action::FocusLeft => {
                self.focus_dir(-1, 0);
                vec![AppEffect::Redraw]
            }
            Action::FocusRight => {
                self.focus_dir(1, 0);
                vec![AppEffect::Redraw]
            }
            Action::FocusUp => {
                self.focus_dir(0, -1);
                vec![AppEffect::Redraw]
            }
            Action::FocusDown => {
                self.focus_dir(0, 1);
                vec![AppEffect::Redraw]
            }
            Action::FocusNext => {
                self.focus_next();
                vec![AppEffect::Redraw]
            }

            // ── Tab management ───────────────────────────────────────────────
            Action::NewTab => vec![AppEffect::NewTab],
            Action::NextTab => {
                self.next_tab();
                vec![AppEffect::Redraw]
            }
            Action::PrevTab => {
                self.prev_tab();
                vec![AppEffect::Redraw]
            }
            Action::GoToTab(idx) => self.do_go_to_tab(idx),
            Action::MoveTabLeft => {
                self.move_tab_left();
                vec![AppEffect::Redraw]
            }
            Action::MoveTabRight => {
                self.move_tab_right();
                vec![AppEffect::Redraw]
            }
            Action::CloseTab => vec![AppEffect::CloseTab],
            Action::RenameTab => {
                self.do_rename_tab();
                vec![AppEffect::Redraw]
            }

            // ── Search ───────────────────────────────────────────────────────
            Action::SearchOpen => {
                self.search_matches.clear();
                self.search_current = 0;
                self.search_before_history.clear();
                self.mode = InputMode::Search {
                    query: String::new(),
                    history_pos: None,
                };
                vec![AppEffect::Redraw]
            }
            Action::SearchNext => {
                self.do_search_next();
                vec![AppEffect::Redraw]
            }
            Action::SearchPrev => {
                self.do_search_prev();
                vec![AppEffect::Redraw]
            }

            // ── Font size (needs renderer → delegated) ───────────────────────
            Action::IncreaseFontSize => vec![AppEffect::ChangeFontSize(1.0)],
            Action::DecreaseFontSize => vec![AppEffect::ChangeFontSize(-1.0)],
            Action::ResetFontSize => self.do_reset_font_size(),

            // ── Pane resize ──────────────────────────────────────────────────
            Action::ResizePaneRight => vec![AppEffect::ResizePane {
                split_h: true,
                delta: crate::ui::layout::NUDGE_STEP,
            }],
            Action::ResizePaneLeft => vec![AppEffect::ResizePane {
                split_h: true,
                delta: -crate::ui::layout::NUDGE_STEP,
            }],
            Action::ResizePaneDown => vec![AppEffect::ResizePane {
                split_h: false,
                delta: crate::ui::layout::NUDGE_STEP,
            }],
            Action::ResizePaneUp => vec![AppEffect::ResizePane {
                split_h: false,
                delta: -crate::ui::layout::NUDGE_STEP,
            }],

            // ── Passthrough ──────────────────────────────────────────────────
            Action::TogglePassthrough => {
                self.tab_mut().passthrough = !self.tab_mut().passthrough;
                vec![AppEffect::Redraw]
            }

            // ── Logging ──────────────────────────────────────────────────────
            Action::ToggleLog => vec![AppEffect::ToggleLog],

            // ── UI ───────────────────────────────────────────────────────────
            Action::ToggleFullscreen => vec![AppEffect::ToggleFullscreen],
            Action::OpenCommandPalette => {
                self.mode = InputMode::CommandPalette {
                    query: String::new(),
                    selected: 0,
                };
                vec![AppEffect::Redraw]
            }
            Action::OpenConfig => {
                self.open_config_panel();
                vec![AppEffect::Redraw]
            }
            Action::ZoomPane => {
                self.do_zoom_pane();
                vec![AppEffect::Redraw]
            }
            Action::RotatePanesForward => {
                self.tabs[self.active_tab].zoomed = false;
                vec![AppEffect::RotatePanes(true)]
            }
            Action::RotatePanesBackward => {
                self.tabs[self.active_tab].zoomed = false;
                vec![AppEffect::RotatePanes(false)]
            }
            Action::ScreenshotOpen => vec![AppEffect::ScreenshotOpen],
            Action::ScreenshotEdgeResize(dw, dh) => {
                self.do_screenshot_edge_resize(dw, dh);
                vec![AppEffect::Redraw]
            }
            Action::ScreenshotMove(dx, dy) => {
                self.do_screenshot_move(dx, dy);
                vec![AppEffect::Redraw]
            }
            Action::ScreenshotCapture => self.do_screenshot_capture(),
            Action::Quit => self.do_quit(),
            Action::QuitSaveSession => vec![AppEffect::SaveSessionAndQuit],
            Action::QuitNoSave => vec![AppEffect::Quit],
            Action::None => vec![],
        }
    }

    fn do_set_mode(&mut self, new_mode: InputMode) -> Vec<AppEffect> {
        self.mode = if let InputMode::Visual { .. } = &new_mode {
            if matches!(self.mode, InputMode::Visual { .. }) {
                new_mode
            } else {
                let (col, row) = self.visual_start_pos();
                InputMode::Visual {
                    start_col: col,
                    start_row: row,
                    cur_col: col,
                    cur_row: row,
                    anchored: false,
                }
            }
        } else {
            new_mode
        };
        vec![AppEffect::Redraw]
    }

    fn do_clear_scrollback(&mut self) {
        if let Some(e) = self.active_entry_mut() {
            let mut g = e.pane.grid.write().unwrap();
            g.scrollback.clear();
            g.clear_screen();
            g.cursor_col = 0;
            g.cursor_row = 0;
            drop(g);
            e.pane.scroll_bottom();
        }
        if !self.tabs.is_empty() {
            self.search_matches.clear();
        }
    }

    fn do_rename_tab(&mut self) {
        let current = self
            .tabs
            .get(self.active_tab)
            .and_then(|t| t.name.clone())
            .unwrap_or_default();
        self.mode = InputMode::RenameTab { buf: current };
    }

    fn do_search_next(&mut self) {
        if !self.search_matches.is_empty() {
            let next = (self.search_current + 1) % self.search_matches.len();
            self.scroll_to_match(next);
        }
    }

    fn do_search_prev(&mut self) {
        if !self.search_matches.is_empty() {
            let prev = if self.search_current == 0 {
                self.search_matches.len() - 1
            } else {
                self.search_current - 1
            };
            self.scroll_to_match(prev);
        }
    }

    fn do_screenshot_edge_resize(&mut self, dw: i32, dh: i32) {
        let InputMode::Screenshot {
            cx,
            cy,
            half_w,
            half_h,
        } = self.mode
        else {
            return;
        };
        const MIN_FULL: i32 = 40;
        const STEP: i32 = 20;

        let left = cx.saturating_sub(half_w) as i32;
        let top = cy.saturating_sub(half_h) as i32;

        let new_right = ((cx + half_w) as i32 + dw * STEP).max(left + MIN_FULL);
        let new_bottom = ((cy + half_h) as i32 + dh * STEP).max(top + MIN_FULL);

        self.mode = InputMode::Screenshot {
            cx: (left + (new_right - left) / 2) as u32,
            cy: (top + (new_bottom - top) / 2) as u32,
            half_w: ((new_right - left) / 2) as u32,
            half_h: ((new_bottom - top) / 2) as u32,
        };
    }

    fn do_screenshot_move(&mut self, dx: i32, dy: i32) {
        let InputMode::Screenshot {
            cx,
            cy,
            half_w,
            half_h,
        } = self.mode
        else {
            return;
        };
        self.mode = InputMode::Screenshot {
            cx: (cx as i32 + dx).max(0) as u32,
            cy: (cy as i32 + dy).max(0) as u32,
            half_w,
            half_h,
        };
    }

    fn do_screenshot_capture(&mut self) -> Vec<AppEffect> {
        let InputMode::Screenshot {
            cx,
            cy,
            half_w,
            half_h,
        } = self.mode
        else {
            return vec![];
        };
        self.mode = InputMode::ScreenshotName {
            cx,
            cy,
            half_w,
            half_h,
            name: String::new(),
        };
        vec![AppEffect::Redraw]
    }

    fn do_quit(&mut self) -> Vec<AppEffect> {
        if self.config.general.restore_session {
            self.mode = crate::input::InputMode::QuitSave;
            return vec![AppEffect::Redraw];
        }
        let total_panes = self.tabs.iter().map(|t| t.panes.len()).sum::<usize>();
        if crate::ui::tabs::needs_quit_confirm(self.tabs.len(), total_panes) {
            self.quit_pending = true;
            vec![AppEffect::QuitPending]
        } else {
            vec![AppEffect::Quit]
        }
    }

    // ── Test helpers ─────────────────────────────────────────────────────────

    #[cfg(test)]
    pub fn add_empty_tab(&mut self) {
        use crate::ui::layout::Layout;
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        self.tabs.push(TabState {
            panes: HashMap::new(),
            layout: Layout::new(id, 800, 600),
            active: id,
            name: None,
            zoomed: false,
            has_activity: false,
            bell_flash_start: None,
            bell_flash_until: None,
            bell_cooldown_until: None,
            passthrough: false,
        });
        self.active_tab = self.tabs.len() - 1;
    }

    /// Build a real `PaneEntry` (throwaway `/bin/true` PTY) carrying the given
    /// per-pane font size + metrics. For window-free tests of per-pane sizing.
    #[cfg(test)]
    pub(crate) fn test_pane_entry(
        logical: Logical,
        metrics: crate::renderer::FontMetrics,
    ) -> PaneEntry {
        use crate::drain;
        use crate::terminal::grid::{Color, Grid, GridColors};
        use crate::ui::Pane;
        use crossbeam_channel::unbounded;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex, RwLock};

        let (pty_tx, pty_rx) = unbounded::<Vec<u8>>();
        let pty = crate::pty::PtySession::spawn_with_shell(
            80,
            24,
            pty_tx,
            "/bin/true",
            None,
            Box::new(|| {}),
        )
        .expect("PTY spawn failed");
        let grid = Arc::new(RwLock::new(Grid::with_colors(
            80,
            24,
            GridColors {
                fg: Color::WHITE,
                bg: Color::BLACK,
                cursor: Color::WHITE,
                selection: Color::WHITE,
                palette: [Color::BLACK; 16],
            },
            1000,
        )));
        let pane = Pane::new(grid.clone(), [0, 22, 800, 556]);
        let log_file = Arc::new(Mutex::new(None));
        let pending_resize = Arc::new(Mutex::new(None));
        let (effects_tx, effects_rx) = unbounded::<drain::ParseEffect>();
        let parser_thread = drain::spawn_parser_thread(drain::ParserThreadArgs {
            rx: pty_rx,
            grid,
            log_file: log_file.clone(),
            effects_tx,
            wakeup_pending: Arc::new(AtomicBool::new(false)),
            pending_resize: pending_resize.clone(),
            wakeup: Box::new(|| {}),
        });
        PaneEntry {
            pane,
            pty,
            effects_rx,
            log_file,
            pending_resize,
            _parser_thread: parser_thread,
            logical_font_size: logical,
            metrics,
        }
    }

    /// Creates a tab with one real pane (PTY: /bin/true, grid: 80×24).
    /// The PTY exits immediately — use for grid/state tests only, not I/O.
    #[cfg(test)]
    pub fn add_test_pane(&mut self) {
        use crate::drain;
        use crate::terminal::grid::{Color, Grid, GridColors};
        use crate::ui::layout::Layout;
        use crossbeam_channel::unbounded;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex, RwLock};

        if self.tabs.is_empty() {
            self.add_empty_tab();
        }
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        let (pty_tx, pty_rx) = unbounded::<Vec<u8>>();
        let pty = crate::pty::PtySession::spawn_with_shell(
            80,
            24,
            pty_tx,
            "/bin/true",
            None,
            Box::new(|| {}),
        )
        .expect("PTY spawn failed");
        let grid = Arc::new(RwLock::new(Grid::with_colors(
            80,
            24,
            GridColors {
                fg: Color::WHITE,
                bg: Color::BLACK,
                cursor: Color::WHITE,
                selection: Color::WHITE,
                palette: [Color::BLACK; 16],
            },
            1000,
        )));
        let pane = Pane::new(grid.clone(), [0, 22, 800, 556]);
        let log_file = Arc::new(Mutex::new(None));
        let pending_resize = Arc::new(Mutex::new(None));
        let (effects_tx, effects_rx) = unbounded::<drain::ParseEffect>();
        let parser_thread = drain::spawn_parser_thread(drain::ParserThreadArgs {
            rx: pty_rx,
            grid,
            log_file: log_file.clone(),
            effects_tx,
            wakeup_pending: Arc::new(AtomicBool::new(false)),
            pending_resize: pending_resize.clone(),
            wakeup: Box::new(|| {}),
        });
        let tab_idx = self.active_tab;
        self.tabs[tab_idx].panes.insert(
            id,
            PaneEntry {
                pane,
                pty,
                effects_rx,
                log_file,
                pending_resize,
                _parser_thread: parser_thread,
                logical_font_size: Logical(16.0),
                metrics: crate::renderer::FontMetrics {
                    font_px: 16.0,
                    cell_width: 8,
                    cell_height: 16,
                    baseline: 13,
                },
            },
        );
        self.tabs[tab_idx].active = id;
        self.tabs[tab_idx].layout = Layout::new(id, 800, 578);
    }
}

#[cfg(test)]
#[path = "app_state_test.rs"]
mod tests;
