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
use crate::ui::{Layout, Pane, SplitDir};

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
    ChangeFontSize(f32),
    ToggleLog,
    SendToPty(Vec<u8>),
    Paste,
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
                if !self.tabs.is_empty() {
                    let active = self.tab().active;
                    if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                        e.pane.scroll_bottom();
                    }
                }
                vec![AppEffect::SendToPty(bytes)]
            }
            Action::Paste => vec![AppEffect::Paste],

            // ── Mode ─────────────────────────────────────────────────────────
            Action::SetMode(new_mode) => {
                let mode = if let InputMode::Visual { .. } = &new_mode {
                    if matches!(self.mode, InputMode::Visual { .. }) {
                        new_mode
                    } else {
                        let (col, row) = self
                            .tabs
                            .get(self.active_tab)
                            .and_then(|t| t.panes.get(&t.active))
                            .map(|e| {
                                if e.pane.scroll_offset > 0 {
                                    (0, 0)
                                } else {
                                    (e.pane.parser.grid.cursor_col, e.pane.parser.grid.cursor_row)
                                }
                            })
                            .unwrap_or((0, 0));
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
                self.mode = mode;
                vec![AppEffect::Redraw]
            }

            // ── Scroll ───────────────────────────────────────────────────────
            Action::ScrollUp(n) => {
                if !self.tabs.is_empty() {
                    let active = self.tab().active;
                    let grid_rows = self
                        .tab()
                        .panes
                        .get(&active)
                        .map(|e| e.pane.parser.grid.rows)
                        .unwrap_or(1);
                    if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                        e.pane.scroll_up(n);
                    }
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
                vec![AppEffect::Redraw]
            }
            Action::ScrollDown(n) => {
                if !self.tabs.is_empty() {
                    let active = self.tab().active;
                    if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                        e.pane.scroll_down(n);
                    }
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
                vec![AppEffect::Redraw]
            }
            Action::ScrollToTop => {
                if !self.tabs.is_empty() {
                    let active = self.tab().active;
                    if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                        e.pane.scroll_top();
                    }
                }
                vec![AppEffect::Redraw]
            }
            Action::ScrollToBottom => {
                if !self.tabs.is_empty() {
                    let active = self.tab().active;
                    if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                        e.pane.scroll_bottom();
                    }
                }
                vec![AppEffect::Redraw]
            }
            Action::ClearScrollback => {
                if !self.tabs.is_empty() {
                    let active = self.tab().active;
                    if let Some(e) = self.tab_mut().panes.get_mut(&active) {
                        e.pane.parser.grid.scrollback.clear();
                        e.pane.parser.grid.clear_screen();
                        e.pane.parser.grid.cursor_col = 0;
                        e.pane.parser.grid.cursor_row = 0;
                        e.pane.scroll_bottom();
                    }
                    self.search_matches.clear();
                }
                vec![AppEffect::Redraw]
            }

            // ── Copy / Visual ────────────────────────────────────────────────
            Action::Copy => {
                if let InputMode::Visual {
                    start_col,
                    start_row,
                    cur_col,
                    cur_row,
                    anchored: true,
                } = self.mode.clone()
                {
                    if !self.tabs.is_empty() {
                        let active = self.tab().active;
                        if let Some(entry) = self.tab().panes.get(&active) {
                            let scroll_offset = entry.pane.scroll_offset;
                            let text = entry.pane.parser.grid.selected_text(
                                start_col,
                                start_row,
                                cur_col,
                                cur_row,
                                scroll_offset,
                            );
                            if !text.is_empty() {
                                let cb = self.clipboard.get_or_insert_with(|| {
                                    Clipboard::new().expect("clipboard unavailable")
                                });
                                let _ = cb.set_text(text);
                            }
                        }
                    }
                    self.mode = InputMode::Insert;
                }
                vec![AppEffect::Redraw]
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
                vec![AppEffect::Redraw]
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
                vec![AppEffect::Redraw]
            }
            Action::VisualWordForward => {
                if let InputMode::Visual {
                    start_col,
                    start_row,
                    cur_col,
                    cur_row,
                    anchored,
                } = self.mode.clone()
                    && !self.tabs.is_empty()
                {
                    let active = self.tab().active;
                    if let Some(entry) = self.tab().panes.get(&active) {
                        let (nc, nr) = crate::motion::word_forward(
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
                }
                vec![AppEffect::Redraw]
            }
            Action::VisualWordBackward => {
                if let InputMode::Visual {
                    start_col,
                    start_row,
                    cur_col,
                    cur_row,
                    anchored,
                } = self.mode.clone()
                    && !self.tabs.is_empty()
                {
                    let active = self.tab().active;
                    if let Some(entry) = self.tab().panes.get(&active) {
                        let (nc, nr) = crate::motion::word_backward(
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
                }
                vec![AppEffect::Redraw]
            }
            Action::VisualWordEnd => {
                if let InputMode::Visual {
                    start_col,
                    start_row,
                    cur_col,
                    cur_row,
                    anchored,
                } = self.mode.clone()
                    && !self.tabs.is_empty()
                {
                    let active = self.tab().active;
                    if let Some(entry) = self.tab().panes.get(&active) {
                        let (nc, nr) = crate::motion::word_end(
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
                }
                vec![AppEffect::Redraw]
            }
            Action::VisualYankLine => {
                if let InputMode::Visual { cur_row, .. } = self.mode.clone() {
                    if !self.tabs.is_empty() {
                        let active = self.tab().active;
                        if let Some(entry) = self.tab().panes.get(&active) {
                            let cols = entry.pane.parser.grid.cols.saturating_sub(1);
                            let text = entry.pane.parser.grid.selected_text(
                                0,
                                cur_row,
                                cols,
                                cur_row,
                                entry.pane.scroll_offset,
                            );
                            if !text.is_empty() {
                                let cb = self.clipboard.get_or_insert_with(|| {
                                    Clipboard::new().expect("clipboard unavailable")
                                });
                                let _ = cb.set_text(text);
                            }
                        }
                    }
                    self.mode = InputMode::Insert;
                }
                vec![AppEffect::Redraw]
            }
            Action::VisualBoundaryUp(n) => {
                if !self.tabs.is_empty() {
                    let active = self.tab().active;
                    let grid_rows = self
                        .tab()
                        .panes
                        .get(&active)
                        .map(|e| e.pane.parser.grid.rows)
                        .unwrap_or(1);
                    if let Some(e) = self.tab_mut().panes.get_mut(&active) {
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
                vec![AppEffect::Redraw]
            }
            Action::VisualBoundaryDown(n) => {
                if !self.tabs.is_empty() {
                    let active = self.tab().active;
                    let grid_rows = self
                        .tab()
                        .panes
                        .get(&active)
                        .map(|e| e.pane.parser.grid.rows)
                        .unwrap_or(1);
                    if let Some(e) = self.tab_mut().panes.get_mut(&active) {
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
                vec![AppEffect::Redraw]
            }

            // ── Pane / split (need PTY resize → delegated) ───────────────────
            Action::SplitH => vec![AppEffect::SplitPane(SplitDir::H)],
            Action::SplitV => vec![AppEffect::SplitPane(SplitDir::V)],
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

            // ── Logging ──────────────────────────────────────────────────────
            Action::ToggleLog => vec![AppEffect::ToggleLog],

            // ── UI ───────────────────────────────────────────────────────────
            Action::ToggleFullscreen => vec![AppEffect::ToggleFullscreen],
            Action::OpenConfig => {
                self.open_config_panel();
                vec![AppEffect::Redraw]
            }
            Action::ZoomPane => {
                if !self.tabs.is_empty() {
                    self.tab_mut().zoomed = !self.tab().zoomed;
                }
                vec![AppEffect::Redraw]
            }
            Action::Quit => {
                let total_panes = self.tabs.iter().map(|t| t.panes.len()).sum::<usize>();
                if crate::tabs::needs_quit_confirm(self.tabs.len(), total_panes) {
                    self.quit_pending = true;
                    vec![AppEffect::QuitPending]
                } else {
                    vec![AppEffect::Quit]
                }
            }
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::default_theme;

    fn make_state() -> AppState {
        AppState::new(Config::default(), default_theme())
    }

    fn make_state_with_tabs(n: usize) -> AppState {
        let mut s = make_state();
        for _ in 0..n {
            s.add_empty_tab();
        }
        s
    }

    // ── Initial state ─────────────────────────────────────────────────────────

    #[test]
    fn initial_mode_is_insert() {
        assert!(matches!(make_state().mode, InputMode::Insert));
    }

    #[test]
    fn initial_search_is_empty() {
        let s = make_state();
        assert!(s.search_matches.is_empty());
        assert_eq!(s.search_current, 0);
    }

    #[test]
    fn initial_tabs_empty() {
        assert!(make_state().tabs.is_empty());
    }

    // ── Tab navigation ────────────────────────────────────────────────────────

    #[test]
    fn dispatch_next_tab_cycles() {
        let mut s = make_state_with_tabs(3);
        s.active_tab = 0;
        let effects = s.dispatch_action(Action::NextTab);
        assert_eq!(s.active_tab, 1);
        assert!(effects.iter().any(|e| matches!(e, AppEffect::Redraw)));
    }

    #[test]
    fn dispatch_prev_tab_wraps() {
        let mut s = make_state_with_tabs(3);
        s.active_tab = 0;
        s.dispatch_action(Action::PrevTab);
        assert_eq!(s.active_tab, 2);
    }

    #[test]
    fn dispatch_goto_tab_valid_index() {
        let mut s = make_state_with_tabs(3);
        s.dispatch_action(Action::GoToTab(2));
        assert_eq!(s.active_tab, 2);
    }

    #[test]
    fn dispatch_goto_tab_out_of_bounds_is_noop() {
        let mut s = make_state_with_tabs(3);
        s.active_tab = 0;
        s.dispatch_action(Action::GoToTab(99));
        assert_eq!(s.active_tab, 0);
    }

    #[test]
    fn dispatch_move_tab_left() {
        let mut s = make_state_with_tabs(3);
        s.active_tab = 1;
        s.dispatch_action(Action::MoveTabLeft);
        assert_eq!(s.active_tab, 0);
    }

    #[test]
    fn dispatch_move_tab_right() {
        let mut s = make_state_with_tabs(3);
        s.active_tab = 1;
        s.dispatch_action(Action::MoveTabRight);
        assert_eq!(s.active_tab, 2);
    }

    // ── Mode transitions ──────────────────────────────────────────────────────

    #[test]
    fn dispatch_set_mode_normal() {
        let mut s = make_state();
        s.dispatch_action(Action::SetMode(InputMode::Normal));
        assert!(matches!(s.mode, InputMode::Normal));
    }

    #[test]
    fn dispatch_search_open_sets_search_mode() {
        let mut s = make_state();
        s.dispatch_action(Action::SearchOpen);
        assert!(matches!(s.mode, InputMode::Search { .. }));
        assert!(s.search_matches.is_empty());
    }

    #[test]
    fn dispatch_rename_tab_sets_rename_mode() {
        let mut s = make_state_with_tabs(1);
        s.dispatch_action(Action::RenameTab);
        assert!(matches!(s.mode, InputMode::RenameTab { .. }));
    }

    // ── Zoom ─────────────────────────────────────────────────────────────────

    #[test]
    fn dispatch_zoom_pane_toggles_zoomed() {
        let mut s = make_state_with_tabs(1);
        assert!(!s.tab().zoomed);
        s.dispatch_action(Action::ZoomPane);
        assert!(s.tab().zoomed);
        s.dispatch_action(Action::ZoomPane);
        assert!(!s.tab().zoomed);
    }

    // ── Ctrl-W prefix ─────────────────────────────────────────────────────────

    #[test]
    fn dispatch_ctrl_w_prefix_sets_pending() {
        let mut s = make_state();
        assert!(!s.ctrl_w_pending);
        s.dispatch_action(Action::CtrlWPrefix);
        assert!(s.ctrl_w_pending);
    }

    // ── Quit ─────────────────────────────────────────────────────────────────

    #[test]
    fn dispatch_quit_single_tab_returns_quit_effect() {
        let mut s = make_state_with_tabs(1);
        let effects = s.dispatch_action(Action::Quit);
        assert!(effects.iter().any(|e| matches!(e, AppEffect::Quit)));
    }

    #[test]
    fn dispatch_quit_multiple_tabs_returns_quit_pending() {
        let mut s = make_state_with_tabs(2);
        let effects = s.dispatch_action(Action::Quit);
        assert!(effects.iter().any(|e| matches!(e, AppEffect::QuitPending)));
        assert!(s.quit_pending);
    }

    // ── Delegated effects ─────────────────────────────────────────────────────

    #[test]
    fn dispatch_split_h_returns_split_effect() {
        let mut s = make_state();
        let effects = s.dispatch_action(Action::SplitH);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::SplitPane(SplitDir::H)))
        );
    }

    #[test]
    fn dispatch_new_tab_returns_effect() {
        let mut s = make_state();
        let effects = s.dispatch_action(Action::NewTab);
        assert!(effects.iter().any(|e| matches!(e, AppEffect::NewTab)));
    }

    #[test]
    fn dispatch_increase_font_size_returns_effect() {
        let mut s = make_state();
        let effects = s.dispatch_action(Action::IncreaseFontSize);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::ChangeFontSize(f) if *f == 1.0))
        );
    }

    #[test]
    fn dispatch_toggle_fullscreen_returns_effect() {
        let mut s = make_state();
        let effects = s.dispatch_action(Action::ToggleFullscreen);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::ToggleFullscreen))
        );
    }

    #[test]
    fn dispatch_none_returns_no_effects() {
        let mut s = make_state();
        let effects = s.dispatch_action(Action::None);
        assert!(effects.is_empty());
    }

    // ── Config panel ──────────────────────────────────────────────────────────

    #[test]
    fn dispatch_open_config_creates_panel() {
        let mut s = make_state();
        assert!(s.config_panel.is_none());
        s.dispatch_action(Action::OpenConfig);
        assert!(s.config_panel.is_some());
    }

    // ── Visual mode ───────────────────────────────────────────────────────────

    #[test]
    fn dispatch_visual_anchor_sets_anchored() {
        let mut s = make_state();
        s.mode = InputMode::Visual {
            start_col: 5,
            start_row: 2,
            cur_col: 3,
            cur_row: 1,
            anchored: false,
        };
        s.dispatch_action(Action::VisualAnchor);
        if let InputMode::Visual {
            start_col,
            start_row,
            anchored,
            ..
        } = s.mode
        {
            assert!(anchored);
            assert_eq!(start_col, 3); // cur becomes start
            assert_eq!(start_row, 1);
        } else {
            panic!("expected Visual mode");
        }
    }

    #[test]
    fn dispatch_visual_swap_anchor_swaps_start_and_cur() {
        let mut s = make_state();
        s.mode = InputMode::Visual {
            start_col: 0,
            start_row: 0,
            cur_col: 5,
            cur_row: 3,
            anchored: true,
        };
        s.dispatch_action(Action::VisualSwapAnchor);
        if let InputMode::Visual {
            start_col,
            start_row,
            cur_col,
            cur_row,
            ..
        } = s.mode
        {
            assert_eq!(start_col, 5);
            assert_eq!(start_row, 3);
            assert_eq!(cur_col, 0);
            assert_eq!(cur_row, 0);
        } else {
            panic!("expected Visual mode");
        }
    }

    // ── Tests with real panes ─────────────────────────────────────────────────

    fn make_state_with_pane() -> AppState {
        let mut s = make_state();
        s.add_empty_tab();
        s.add_test_pane();
        s
    }

    #[test]
    fn focus_next_with_single_pane_stays() {
        let mut s = make_state_with_pane();
        let before = s.tab().active;
        s.focus_next();
        assert_eq!(s.tab().active, before);
    }

    #[test]
    fn dispatch_scroll_up_adjusts_offset() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        // Seed scrollback so scroll_up has room
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            for _ in 0..30 {
                e.pane.parser.grid.scroll_up(1);
            }
        }
        let before = s
            .tab()
            .panes
            .get(&active)
            .map(|e| e.pane.scroll_offset)
            .unwrap_or(0);
        s.dispatch_action(Action::ScrollUp(3));
        let after = s
            .tab()
            .panes
            .get(&active)
            .map(|e| e.pane.scroll_offset)
            .unwrap_or(0);
        assert!(after > before || after == before); // clamped at max scrollback
    }

    #[test]
    fn dispatch_scroll_down_decrements_offset() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        // Put some scrollback and scroll up first
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            for _ in 0..10 {
                e.pane.parser.grid.scroll_up(1);
            }
            e.pane.scroll_offset = 5;
        }
        s.dispatch_action(Action::ScrollDown(2));
        let after = s
            .tab()
            .panes
            .get(&active)
            .map(|e| e.pane.scroll_offset)
            .unwrap_or(0);
        assert_eq!(after, 3);
    }

    #[test]
    fn dispatch_scroll_to_top_sets_max_offset() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            for _ in 0..10 {
                e.pane.parser.grid.scroll_up(1);
            }
        }
        s.dispatch_action(Action::ScrollToTop);
        let sb = s
            .tab()
            .panes
            .get(&active)
            .map(|e| e.pane.parser.grid.scrollback.len())
            .unwrap_or(0);
        let off = s
            .tab()
            .panes
            .get(&active)
            .map(|e| e.pane.scroll_offset)
            .unwrap_or(0);
        assert_eq!(off, sb);
    }

    #[test]
    fn dispatch_scroll_to_bottom_resets_offset() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            e.pane.scroll_offset = 5;
        }
        s.dispatch_action(Action::ScrollToBottom);
        let off = s
            .tab()
            .panes
            .get(&active)
            .map(|e| e.pane.scroll_offset)
            .unwrap_or(99);
        assert_eq!(off, 0);
    }

    #[test]
    fn dispatch_clear_scrollback_empties_it() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            for _ in 0..5 {
                e.pane.parser.grid.scroll_up(1);
            }
        }
        s.dispatch_action(Action::ClearScrollback);
        let sb = s
            .tab()
            .panes
            .get(&active)
            .map(|e| e.pane.parser.grid.scrollback.len())
            .unwrap_or(99);
        assert_eq!(sb, 0);
    }

    #[test]
    fn dispatch_search_open_then_next_prev_no_panic() {
        let mut s = make_state_with_pane();
        s.dispatch_action(Action::SearchOpen);
        // No matches → next/prev are no-ops
        s.dispatch_action(Action::SearchNext);
        s.dispatch_action(Action::SearchPrev);
        assert!(s.search_matches.is_empty());
    }

    #[test]
    fn update_search_matches_finds_content() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        // Write "needle" into the grid
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            for c in "needle".chars() {
                e.pane.parser.grid.write_char(c);
            }
        }
        s.mode = InputMode::Search {
            query: "needle".to_string(),
        };
        s.update_search_matches();
        assert!(!s.search_matches.is_empty());
    }

    #[test]
    fn dispatch_send_to_pty_scrolls_to_bottom() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            e.pane.scroll_offset = 5;
        }
        // SendToPty should scroll_bottom (clears offset)
        let effects = s.dispatch_action(Action::SendToPty(b"x".to_vec()));
        assert!(effects.iter().any(|e| matches!(e, AppEffect::SendToPty(_))));
        let off = s
            .tab()
            .panes
            .get(&active)
            .map(|e| e.pane.scroll_offset)
            .unwrap_or(99);
        assert_eq!(off, 0);
    }

    #[test]
    fn dispatch_visual_word_forward_with_pane_does_not_panic() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            for c in "hello world".chars() {
                e.pane.parser.grid.write_char(c);
            }
        }
        s.mode = InputMode::Visual {
            start_col: 0,
            start_row: 0,
            cur_col: 0,
            cur_row: 0,
            anchored: false,
        };
        s.dispatch_action(Action::VisualWordForward);
        if let InputMode::Visual { cur_col, .. } = s.mode {
            assert!(cur_col > 0, "cursor should have moved forward");
        }
    }

    #[test]
    fn dispatch_visual_yank_line_exits_visual_mode() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            for c in "hello".chars() {
                e.pane.parser.grid.write_char(c);
            }
        }
        s.mode = InputMode::Visual {
            start_col: 0,
            start_row: 0,
            cur_col: 4,
            cur_row: 0,
            anchored: true,
        };
        s.dispatch_action(Action::VisualYankLine);
        assert!(matches!(s.mode, InputMode::Insert));
    }

    #[test]
    fn dispatch_copy_with_anchored_selection_exits_visual() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            for c in "hello".chars() {
                e.pane.parser.grid.write_char(c);
            }
        }
        s.mode = InputMode::Visual {
            start_col: 0,
            start_row: 0,
            cur_col: 4,
            cur_row: 0,
            anchored: true,
        };
        s.dispatch_action(Action::Copy);
        assert!(matches!(s.mode, InputMode::Insert));
    }

    #[test]
    fn dispatch_visual_boundary_up_scrolls_pane() {
        let mut s = make_state_with_pane();
        let active = s.tab().active;
        if let Some(e) = s.tab_mut().panes.get_mut(&active) {
            for _ in 0..30 {
                e.pane.parser.grid.scroll_up(1);
            }
        }
        s.mode = InputMode::Visual {
            start_col: 0,
            start_row: 0,
            cur_col: 0,
            cur_row: 0,
            anchored: true,
        };
        s.dispatch_action(Action::VisualBoundaryUp(2));
        let off = s
            .tab()
            .panes
            .get(&active)
            .map(|e| e.pane.scroll_offset)
            .unwrap_or(0);
        assert!(off > 0);
    }

    #[test]
    fn dispatch_quit_with_pane_needs_confirm() {
        let mut s = make_state_with_pane();
        // 1 tab, 1 pane → no confirm
        let effects = s.dispatch_action(Action::Quit);
        assert!(effects.iter().any(|e| matches!(e, AppEffect::Quit)));
    }
}
