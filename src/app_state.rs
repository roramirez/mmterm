use arboard::Clipboard;
use crossbeam_channel::Receiver;
use std::collections::HashMap;
use std::time::Instant;

use crate::config::Config;
use crate::input::InputMode;
use crate::input::keybindings::Action;
use crate::renderer::FontMetrics;
use crate::theme::ResolvedTheme;
use crate::tui_config::ConfigPanel;
use crate::ui::{Layout, Pane, SeparatorHandle, SplitDir};

// ── Re-exports so main.rs can still use these types ─────────────────────────

pub struct PaneEntry {
    pub pane: Pane,
    pub pty: crate::pty::PtySession,
    pub rx: Receiver<Vec<u8>>,
    pub log_file: Option<std::fs::File>,
}

pub struct TabState {
    pub panes: HashMap<usize, PaneEntry>,
    pub layout: Layout,
    pub active: usize,
    pub metrics: FontMetrics,
    pub name: Option<String>,
    pub zoomed: bool,
    pub has_activity: bool,
    pub bell_flash_until: Option<Instant>,
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
    SaveSessionAndQuit,
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
    pub hovered_url: Option<String>,
    pub swallow_next_tab: bool,
    pub theme: ResolvedTheme,
    pub drag_separator: Option<SeparatorHandle>,
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
            hovered_url: None,
            swallow_next_tab: false,
            theme,
            config,
            drag_separator: None,
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
        self.active_tab = crate::tabs::next_tab_index(self.active_tab, self.tabs.len());
    }

    pub fn prev_tab(&mut self) {
        self.active_tab = crate::tabs::prev_tab_index(self.active_tab, self.tabs.len());
    }

    pub fn move_tab_left(&mut self) {
        let new = crate::tabs::move_tab_index(self.active_tab, self.tabs.len(), true);
        if new != self.active_tab {
            self.tabs.swap(self.active_tab, new);
            self.active_tab = new;
        }
    }

    pub fn move_tab_right(&mut self) {
        let new = crate::tabs::move_tab_index(self.active_tab, self.tabs.len(), false);
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
        self.tab_mut().active = crate::tabs::next_pane_in_layout(&leaves, active);
    }

    // ── Search ───────────────────────────────────────────────────────────────

    pub fn update_search_matches(&mut self) {
        let query = match &self.mode {
            InputMode::Search { query } => query.clone(),
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
            self.search_matches =
                crate::search::compute_search_matches(&entry.pane.parser.grid, &query);
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
            .map(|e| (e.pane.parser.grid.scrollback.len(), e.pane.parser.grid.rows))
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
            .map(|e| e.pane.parser.grid.rows)
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
            &entry.pane.parser.grid,
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

    fn copy_text_to_clipboard(&mut self, text: String) {
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
                start_row: (start_row + n).min(grid_rows.saturating_sub(1)),
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
                    (e.pane.parser.grid.cursor_col, e.pane.parser.grid.cursor_row)
                }
            })
            .unwrap_or((0, 0))
    }

    // ── Visual sub-dispatch ──────────────────────────────────────────────────

    fn dispatch_visual_action(&mut self, action: Action) -> Vec<AppEffect> {
        match action {
            Action::Copy => {
                if let InputMode::Visual {
                    start_col,
                    start_row,
                    cur_col,
                    cur_row,
                    anchored: true,
                } = self.mode.clone()
                {
                    if let Some(entry) = self.active_entry() {
                        let text = entry.pane.parser.grid.selected_text(
                            start_col,
                            start_row,
                            cur_col,
                            cur_row,
                            entry.pane.scroll_offset,
                        );
                        self.copy_text_to_clipboard(text);
                    }
                    self.mode = InputMode::Insert;
                }
            }
            Action::VisualSwapAnchor => {
                if let InputMode::Visual {
                    start_col,
                    start_row,
                    cur_col,
                    cur_row,
                    anchored,
                } = self.mode.clone()
                {
                    self.mode = InputMode::Visual {
                        start_col: cur_col,
                        start_row: cur_row,
                        cur_col: start_col,
                        cur_row: start_row,
                        anchored,
                    };
                }
            }
            Action::VisualAnchor => {
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
            Action::VisualWordForward => self.move_visual_cursor(crate::motion::word_forward),
            Action::VisualWordBackward => self.move_visual_cursor(crate::motion::word_backward),
            Action::VisualWordEnd => self.move_visual_cursor(crate::motion::word_end),
            Action::VisualYankLine => {
                if let InputMode::Visual { cur_row, .. } = self.mode.clone() {
                    if let Some(entry) = self.active_entry() {
                        let cols = entry.pane.parser.grid.cols.saturating_sub(1);
                        let text = entry.pane.parser.grid.selected_text(
                            0,
                            cur_row,
                            cols,
                            cur_row,
                            entry.pane.scroll_offset,
                        );
                        self.copy_text_to_clipboard(text);
                    }
                    self.mode = InputMode::Insert;
                }
            }
            _ => {}
        }
        vec![AppEffect::Redraw]
    }

    fn visual_boundary_scroll_up(&mut self, n: usize) {
        let grid_rows = self.active_grid_rows();
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
                start_row: (start_row + n).min(grid_rows.saturating_sub(1)),
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
                if let Some(e) = self.active_entry_mut() {
                    e.pane.scroll_bottom();
                }
                vec![AppEffect::SendToPty(bytes)]
            }
            Action::Paste => vec![AppEffect::Paste],

            // ── Mode ─────────────────────────────────────────────────────────
            Action::SetMode(new_mode) => {
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

            // ── Scroll ───────────────────────────────────────────────────────
            Action::ScrollUp(n) => {
                let grid_rows = self.active_grid_rows();
                if let Some(e) = self.active_entry_mut() {
                    e.pane.scroll_up(n);
                }
                self.adjust_visual_scroll_up(n, grid_rows);
                vec![AppEffect::Redraw]
            }
            Action::ScrollDown(n) => {
                if let Some(e) = self.active_entry_mut() {
                    e.pane.scroll_down(n);
                }
                self.adjust_visual_scroll_down(n);
                vec![AppEffect::Redraw]
            }
            Action::ScrollToTop => {
                if let Some(e) = self.active_entry_mut() {
                    e.pane.scroll_top();
                }
                vec![AppEffect::Redraw]
            }
            Action::ScrollToBottom => {
                if let Some(e) = self.active_entry_mut() {
                    e.pane.scroll_bottom();
                }
                vec![AppEffect::Redraw]
            }
            Action::ClearScrollback => {
                if let Some(e) = self.active_entry_mut() {
                    e.pane.parser.grid.scrollback.clear();
                    e.pane.parser.grid.clear_screen();
                    e.pane.parser.grid.cursor_col = 0;
                    e.pane.parser.grid.cursor_row = 0;
                    e.pane.scroll_bottom();
                }
                if !self.tabs.is_empty() {
                    self.search_matches.clear();
                }
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
            Action::GoToTab(idx) => {
                if idx < self.tabs.len() {
                    self.active_tab = idx;
                    vec![AppEffect::Redraw]
                } else {
                    vec![]
                }
            }
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
                let current = self
                    .tabs
                    .get(self.active_tab)
                    .and_then(|t| t.name.clone())
                    .unwrap_or_default();
                self.mode = InputMode::RenameTab { buf: current };
                vec![AppEffect::Redraw]
            }

            // ── Search ───────────────────────────────────────────────────────
            Action::SearchOpen => {
                self.search_matches.clear();
                self.search_current = 0;
                self.mode = InputMode::Search {
                    query: String::new(),
                };
                vec![AppEffect::Redraw]
            }
            Action::SearchNext => {
                if !self.search_matches.is_empty() {
                    let next = (self.search_current + 1) % self.search_matches.len();
                    self.scroll_to_match(next);
                }
                vec![AppEffect::Redraw]
            }
            Action::SearchPrev => {
                if !self.search_matches.is_empty() {
                    let prev = if self.search_current == 0 {
                        self.search_matches.len() - 1
                    } else {
                        self.search_current - 1
                    };
                    self.scroll_to_match(prev);
                }
                vec![AppEffect::Redraw]
            }

            // ── Font size (needs renderer → delegated) ───────────────────────
            Action::IncreaseFontSize => vec![AppEffect::ChangeFontSize(1.0)],
            Action::DecreaseFontSize => vec![AppEffect::ChangeFontSize(-1.0)],
            Action::ResetFontSize => {
                let default = self.config.font.size;
                let current = self
                    .tabs
                    .get(self.active_tab)
                    .map(|t| t.metrics.font_px)
                    .unwrap_or(default);
                vec![AppEffect::ChangeFontSize(default - current)]
            }

            // ── Pane resize ──────────────────────────────────────────────────
            Action::ResizePaneRight => {
                vec![AppEffect::ResizePane {
                    split_h: true,
                    delta: crate::ui::layout::NUDGE_STEP,
                }]
            }
            Action::ResizePaneLeft => {
                vec![AppEffect::ResizePane {
                    split_h: true,
                    delta: -crate::ui::layout::NUDGE_STEP,
                }]
            }
            Action::ResizePaneDown => {
                vec![AppEffect::ResizePane {
                    split_h: false,
                    delta: crate::ui::layout::NUDGE_STEP,
                }]
            }
            Action::ResizePaneUp => {
                vec![AppEffect::ResizePane {
                    split_h: false,
                    delta: -crate::ui::layout::NUDGE_STEP,
                }]
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
                if !self.tabs.is_empty() {
                    let zoomed = self.tab().zoomed;
                    self.tab_mut().zoomed = !zoomed;
                }
                vec![AppEffect::Redraw]
            }
            Action::Quit => {
                if self.config.general.restore_session {
                    self.mode = crate::input::InputMode::QuitSave;
                    vec![AppEffect::Redraw]
                } else {
                    let total_panes = self.tabs.iter().map(|t| t.panes.len()).sum::<usize>();
                    if crate::tabs::needs_quit_confirm(self.tabs.len(), total_panes) {
                        self.quit_pending = true;
                        vec![AppEffect::QuitPending]
                    } else {
                        vec![AppEffect::Quit]
                    }
                }
            }
            Action::QuitSaveSession => vec![AppEffect::SaveSessionAndQuit],
            Action::QuitNoSave => vec![AppEffect::Quit],
            Action::None => vec![],
        }
    }

    // ── Test helpers ─────────────────────────────────────────────────────────

    #[cfg(test)]
    pub fn add_empty_tab(&mut self) {
        use crate::ui::layout::Layout;
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        let metrics = crate::renderer::FontMetrics {
            font_px: 16.0,
            cell_width: 8,
            cell_height: 16,
            baseline: 13,
        };
        self.tabs.push(TabState {
            panes: HashMap::new(),
            layout: Layout::new(id, 800, 600),
            active: id,
            metrics,
            name: None,
            zoomed: false,
            has_activity: false,
            bell_flash_until: None,
        });
        self.active_tab = self.tabs.len() - 1;
    }

    /// Creates a tab with one real pane (PTY: /bin/true, grid: 80×24).
    /// The PTY exits immediately — use for grid/state tests only, not I/O.
    #[cfg(test)]
    pub fn add_test_pane(&mut self) {
        use crate::terminal::grid::{Color, GridColors};
        use crate::ui::layout::Layout;
        use crossbeam_channel::unbounded;

        if self.tabs.is_empty() {
            self.add_empty_tab();
        }
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        let (tx, rx) = unbounded();
        let pty = crate::pty::PtySession::spawn_with_shell(
            80,
            24,
            tx,
            "/bin/true",
            None,
            Box::new(|| {}),
        )
        .expect("PTY spawn failed");
        let pane = Pane::new_with_colors(
            80,
            24,
            [0, 22, 800, 556],
            GridColors {
                fg: Color::WHITE,
                bg: Color::BLACK,
                cursor: Color::WHITE,
                selection: Color::WHITE,
                palette: [Color::BLACK; 16],
            },
            1000,
        );
        let tab_idx = self.active_tab;
        self.tabs[tab_idx].panes.insert(
            id,
            PaneEntry {
                pane,
                pty,
                rx,
                log_file: None,
            },
        );
        self.tabs[tab_idx].active = id;
        self.tabs[tab_idx].layout = Layout::new(id, 800, 578);
    }
}

#[cfg(test)]
#[path = "app_state_test.rs"]
mod tests;
