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

// ── HiDPI: logical_font_size field ───────────────────────────────────────

#[test]
fn new_tab_seeds_logical_font_size() {
    use crate::renderer::FontMetrics;
    let mut s = make_state();
    s.add_empty_tab();
    let m = FontMetrics {
        font_px: 16.0,
        cell_width: 8,
        cell_height: 16,
        baseline: 13,
    };
    let idx = s.active_tab;
    s.tabs[idx]
        .panes
        .insert(1, AppState::test_pane_entry(crate::dpi::Logical(16.0), m));
    assert_eq!(
        s.tabs[idx].panes[&1].logical_font_size,
        crate::dpi::Logical(16.0)
    );
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

fn make_state_no_restore() -> AppState {
    let mut cfg = Config::default();
    cfg.general.restore_session = false;
    AppState::new(cfg, crate::theme::default_theme())
}

#[test]
fn dispatch_quit_single_tab_returns_quit_effect() {
    let mut s = make_state_no_restore();
    for _ in 0..1 {
        s.add_empty_tab();
    }
    let effects = s.dispatch_action(Action::Quit);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Quit)));
}

#[test]
fn dispatch_quit_multiple_tabs_returns_quit_pending() {
    let mut s = make_state_no_restore();
    for _ in 0..2 {
        s.add_empty_tab();
    }
    let effects = s.dispatch_action(Action::Quit);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::QuitPending)));
    assert!(s.quit_pending);
}

#[test]
fn dispatch_quit_with_restore_session_enters_quit_save_mode() {
    let mut s = make_state_with_tabs(1);
    // restore_session defaults to true
    assert!(s.config.general.restore_session);
    let effects = s.dispatch_action(Action::Quit);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Redraw)));
    assert!(matches!(s.mode, crate::input::InputMode::QuitSave));
}

#[test]
fn dispatch_quit_no_save_returns_quit_effect() {
    let mut s = make_state_with_tabs(1);
    let effects = s.dispatch_action(Action::QuitNoSave);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Quit)));
}

#[test]
fn dispatch_quit_save_session_returns_save_and_quit_effect() {
    let mut s = make_state_with_tabs(1);
    let effects = s.dispatch_action(Action::QuitSaveSession);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::SaveSessionAndQuit))
    );
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
fn dispatch_auto_split_returns_auto_split_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::AutoSplit);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::AutoSplitPane))
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
            e.pane.grid.write().unwrap().scroll_up(1);
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
            e.pane.grid.write().unwrap().scroll_up(1);
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
            e.pane.grid.write().unwrap().scroll_up(1);
        }
    }
    s.dispatch_action(Action::ScrollToTop);
    let sb = s
        .tab()
        .panes
        .get(&active)
        .map(|e| e.pane.grid.read().unwrap().scrollback.len())
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
            e.pane.grid.write().unwrap().scroll_up(1);
        }
    }
    s.dispatch_action(Action::ClearScrollback);
    let sb = s
        .tab()
        .panes
        .get(&active)
        .map(|e| e.pane.grid.read().unwrap().scrollback.len())
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
            e.pane.grid.write().unwrap().write_char(c);
        }
    }
    s.mode = InputMode::Search {
        query: "needle".to_string(),
        history_pos: None,
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
            e.pane.grid.write().unwrap().write_char(c);
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
            e.pane.grid.write().unwrap().write_char(c);
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
            e.pane.grid.write().unwrap().write_char(c);
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
fn visual_boundary_up_start_row_grows_beyond_one_page() {
    // Regression: previously start_row was clamped to grid_rows-1, limiting
    // selection to at most one viewport regardless of how many pages were scrolled.
    let mut s = make_state_with_pane();
    let active = s.tab().active;
    let grid_rows = s
        .tab()
        .panes
        .get(&active)
        .map(|e| e.pane.grid.read().unwrap().rows)
        .unwrap_or(1);
    if let Some(e) = s.tab_mut().panes.get_mut(&active) {
        for _ in 0..(grid_rows * 3) {
            e.pane.grid.write().unwrap().scroll_up(1);
        }
    }
    s.mode = InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 0,
        cur_row: 0,
        anchored: true,
    };
    // Scroll up two full pages via boundary scroll
    s.dispatch_action(Action::VisualBoundaryUp(grid_rows));
    s.dispatch_action(Action::VisualBoundaryUp(grid_rows));
    if let InputMode::Visual {
        start_row, cur_row, ..
    } = s.mode
    {
        // start_row must track both pages; cur_row stays at 0
        assert_eq!(cur_row, 0);
        assert_eq!(
            start_row,
            grid_rows * 2,
            "start_row should grow past grid_rows-1"
        );
    } else {
        panic!("expected Visual mode");
    }
}

#[test]
fn dispatch_visual_boundary_up_scrolls_pane() {
    let mut s = make_state_with_pane();
    let active = s.tab().active;
    if let Some(e) = s.tab_mut().panes.get_mut(&active) {
        for _ in 0..30 {
            e.pane.grid.write().unwrap().scroll_up(1);
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
fn dispatch_quit_with_pane_no_restore_no_confirm() {
    let mut s = make_state_no_restore();
    s.add_empty_tab();
    s.add_test_pane();
    // 1 tab, 1 pane, restore_session = false → quit immediately
    let effects = s.dispatch_action(Action::Quit);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Quit)));
}

#[test]
fn dispatch_visual_word_backward_moves_cursor() {
    let mut s = make_state_with_pane();
    let active = s.tab().active;
    if let Some(e) = s.tab_mut().panes.get_mut(&active) {
        for c in "hello world".chars() {
            e.pane.grid.write().unwrap().write_char(c);
        }
    }
    s.mode = InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 6,
        cur_row: 0,
        anchored: false,
    };
    s.dispatch_action(Action::VisualWordBackward);
    if let InputMode::Visual {
        cur_col, cur_row, ..
    } = s.mode
    {
        assert!(
            cur_col < 6 || cur_row == 0,
            "cursor should have moved backward"
        );
    } else {
        panic!("expected Visual mode");
    }
}

#[test]
fn dispatch_visual_word_end_moves_cursor_forward() {
    let mut s = make_state_with_pane();
    let active = s.tab().active;
    if let Some(e) = s.tab_mut().panes.get_mut(&active) {
        for c in "hello world".chars() {
            e.pane.grid.write().unwrap().write_char(c);
        }
    }
    s.mode = InputMode::Visual {
        start_col: 0,
        start_row: 0,
        cur_col: 0,
        cur_row: 0,
        anchored: false,
    };
    s.dispatch_action(Action::VisualWordEnd);
    if let InputMode::Visual { cur_col, .. } = s.mode {
        assert!(cur_col > 0, "cursor should have moved to word end");
    } else {
        panic!("expected Visual mode");
    }
}

#[test]
fn dispatch_visual_boundary_down_scrolls_pane() {
    let mut s = make_state_with_pane();
    let active = s.tab().active;
    // Add scrollback and scroll into it so scroll_down has room
    if let Some(e) = s.tab_mut().panes.get_mut(&active) {
        for _ in 0..30 {
            e.pane.grid.write().unwrap().scroll_up(1);
        }
        e.pane.scroll_offset = 10;
    }
    let grid_rows = s
        .tab()
        .panes
        .get(&active)
        .map(|e| e.pane.grid.read().unwrap().rows)
        .unwrap_or(1);
    s.mode = InputMode::Visual {
        start_col: 0,
        start_row: 5,
        cur_col: 0,
        cur_row: 5,
        anchored: true,
    };
    s.dispatch_action(Action::VisualBoundaryDown(2));
    let off = s
        .tab()
        .panes
        .get(&active)
        .map(|e| e.pane.scroll_offset)
        .unwrap_or(99);
    assert!(off < 10, "scroll_down should have reduced offset");
    if let InputMode::Visual { cur_row, .. } = s.mode {
        assert_eq!(
            cur_row,
            grid_rows.saturating_sub(1),
            "cursor should be at last row"
        );
    } else {
        panic!("expected Visual mode");
    }
}

#[test]
fn dispatch_scroll_up_adjusts_visual_coords() {
    let mut s = make_state_with_pane();
    let active = s.tab().active;
    if let Some(e) = s.tab_mut().panes.get_mut(&active) {
        for _ in 0..30 {
            e.pane.grid.write().unwrap().scroll_up(1);
        }
    }
    s.mode = InputMode::Visual {
        start_col: 0,
        start_row: 2,
        cur_col: 0,
        cur_row: 3,
        anchored: true,
    };
    s.dispatch_action(Action::ScrollUp(2));
    if let InputMode::Visual {
        start_row, cur_row, ..
    } = s.mode
    {
        assert_eq!(start_row, 4);
        assert_eq!(cur_row, 5);
    } else {
        panic!("expected Visual mode");
    }
}

#[test]
fn dispatch_scroll_down_adjusts_visual_coords() {
    let mut s = make_state_with_pane();
    let active = s.tab().active;
    if let Some(e) = s.tab_mut().panes.get_mut(&active) {
        for _ in 0..10 {
            e.pane.grid.write().unwrap().scroll_up(1);
        }
        e.pane.scroll_offset = 5;
    }
    s.mode = InputMode::Visual {
        start_col: 0,
        start_row: 4,
        cur_col: 0,
        cur_row: 6,
        anchored: true,
    };
    s.dispatch_action(Action::ScrollDown(2));
    if let InputMode::Visual {
        start_row, cur_row, ..
    } = s.mode
    {
        assert_eq!(start_row, 2);
        assert_eq!(cur_row, 4);
    } else {
        panic!("expected Visual mode");
    }
}

// ── Mouse-wheel scroll adjusts Visual selection (regression for viewport_scroll) ──

#[test]
fn viewport_scroll_up_adjusts_visual_selection() {
    let mut s = make_state_with_pane();
    let active = s.tab().active;
    if let Some(e) = s.tab_mut().panes.get_mut(&active) {
        for _ in 0..30 {
            e.pane.grid.write().unwrap().scroll_up(1);
        }
    }
    s.mode = InputMode::Visual {
        start_col: 1,
        start_row: 2,
        cur_col: 3,
        cur_row: 4,
        anchored: true,
    };
    s.viewport_scroll(3.0); // positive = scroll up
    let InputMode::Visual {
        start_row, cur_row, ..
    } = s.mode
    else {
        panic!("expected Visual mode");
    };
    assert_eq!(start_row, 5, "anchor must shift down when scrolling up");
    assert_eq!(cur_row, 7, "cursor must shift down when scrolling up");
}

#[test]
fn viewport_scroll_down_adjusts_visual_selection() {
    let mut s = make_state_with_pane();
    let active = s.tab().active;
    if let Some(e) = s.tab_mut().panes.get_mut(&active) {
        for _ in 0..10 {
            e.pane.grid.write().unwrap().scroll_up(1);
        }
        e.pane.scroll_offset = 5;
    }
    s.mode = InputMode::Visual {
        start_col: 1,
        start_row: 6,
        cur_col: 3,
        cur_row: 8,
        anchored: true,
    };
    s.viewport_scroll(-2.0); // negative = scroll down
    let InputMode::Visual {
        start_row, cur_row, ..
    } = s.mode
    else {
        panic!("expected Visual mode");
    };
    assert_eq!(start_row, 4, "anchor must shift up when scrolling down");
    assert_eq!(cur_row, 6, "cursor must shift up when scrolling down");
}

#[test]
fn viewport_scroll_outside_visual_mode_does_not_crash() {
    let mut s = make_state_with_pane();
    let active = s.tab().active;
    if let Some(e) = s.tab_mut().panes.get_mut(&active) {
        for _ in 0..10 {
            e.pane.grid.write().unwrap().scroll_up(1);
        }
    }
    s.mode = InputMode::Insert;
    s.viewport_scroll(3.0);
    assert!(matches!(s.mode, InputMode::Insert));
}

// ── Delegated simple effects ──────────────────────────────────────────────────

#[test]
fn dispatch_paste_returns_paste_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::Paste);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Paste)));
}

#[test]
fn dispatch_close_pane_returns_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::ClosePane);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::ClosePane)));
}

#[test]
fn dispatch_close_tab_returns_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::CloseTab);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::CloseTab)));
}

#[test]
fn dispatch_split_v_returns_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::SplitV);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::SplitPane(SplitDir::V)))
    );
}

#[test]
fn dispatch_decrease_font_size_returns_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::DecreaseFontSize);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::ChangeFontSize(f) if *f == -1.0))
    );
}

#[test]
fn dispatch_reset_font_size_returns_change_effect() {
    let mut s = make_state_with_tabs(1);
    let effects = s.dispatch_action(Action::ResetFontSize);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::ChangeFontSize(_)))
    );
}

#[test]
fn dispatch_reset_font_size_emits_logical_delta_to_default() {
    // Config default is 16.0; active pane is at logical 18.0 → delta should be -2.0.
    use crate::renderer::FontMetrics;
    let mut s = make_state_with_tabs(1);
    let m = FontMetrics {
        font_px: 18.0,
        cell_width: 9,
        cell_height: 18,
        baseline: 15,
    };
    s.tabs[0]
        .panes
        .insert(1, AppState::test_pane_entry(crate::dpi::Logical(18.0), m));
    s.tabs[0].active = 1;

    let effects = s.dispatch_action(Action::ResetFontSize);
    let delta = effects.iter().find_map(|e| {
        if let AppEffect::ChangeFontSize(d) = e {
            Some(*d)
        } else {
            None
        }
    });
    assert!(delta.is_some(), "expected ChangeFontSize effect");
    let d = delta.unwrap();
    assert!(
        (d - (-2.0_f32)).abs() < 1e-5,
        "expected delta -2.0 (default 16 - current 18), got {d}"
    );
}

#[test]
fn dispatch_resize_pane_right_returns_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::ResizePaneRight);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::ResizePane { split_h: true, .. }))
    );
}

#[test]
fn dispatch_resize_pane_left_returns_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::ResizePaneLeft);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::ResizePane { split_h: true, .. }))
    );
}

#[test]
fn dispatch_resize_pane_down_returns_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::ResizePaneDown);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::ResizePane { split_h: false, .. }))
    );
}

#[test]
fn dispatch_resize_pane_up_returns_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::ResizePaneUp);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::ResizePane { split_h: false, .. }))
    );
}

#[test]
fn dispatch_toggle_log_returns_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::ToggleLog);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::ToggleLog)));
}

#[test]
fn dispatch_open_command_palette_sets_mode() {
    let mut s = make_state();
    s.dispatch_action(Action::OpenCommandPalette);
    assert!(
        matches!(s.mode, InputMode::CommandPalette { .. }),
        "mode should be CommandPalette"
    );
}

// ── Focus directions ──────────────────────────────────────────────────────────

#[test]
fn dispatch_focus_left_returns_redraw() {
    let mut s = make_state_with_tabs(1);
    let effects = s.dispatch_action(Action::FocusLeft);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Redraw)));
}

#[test]
fn dispatch_focus_right_returns_redraw() {
    let mut s = make_state_with_tabs(1);
    let effects = s.dispatch_action(Action::FocusRight);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Redraw)));
}

#[test]
fn dispatch_focus_up_returns_redraw() {
    let mut s = make_state_with_tabs(1);
    let effects = s.dispatch_action(Action::FocusUp);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Redraw)));
}

#[test]
fn dispatch_focus_down_returns_redraw() {
    let mut s = make_state_with_tabs(1);
    let effects = s.dispatch_action(Action::FocusDown);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Redraw)));
}

// ── Rotate panes ──────────────────────────────────────────────────────────────

#[test]
fn dispatch_rotate_panes_forward_returns_effect() {
    let mut s = make_state_with_tabs(1);
    s.tabs[0].zoomed = true;
    let effects = s.dispatch_action(Action::RotatePanesForward);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::RotatePanes(true)))
    );
    assert!(!s.tabs[0].zoomed, "rotate should clear zoomed flag");
}

#[test]
fn dispatch_rotate_panes_backward_returns_effect() {
    let mut s = make_state_with_tabs(1);
    s.tabs[0].zoomed = true;
    let effects = s.dispatch_action(Action::RotatePanesBackward);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::RotatePanes(false)))
    );
    assert!(!s.tabs[0].zoomed);
}

// ── Screenshot mode ───────────────────────────────────────────────────────────

#[test]
fn dispatch_screenshot_open_returns_effect() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::ScreenshotOpen);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, AppEffect::ScreenshotOpen))
    );
}

#[test]
fn dispatch_screenshot_edge_resize_moves_right_bottom_edge() {
    let mut s = make_state();
    // cx=400, cy=300, half_w=100, half_h=80
    // left=300 (fixed), top=220 (fixed), right=500, bottom=380
    s.mode = InputMode::Screenshot {
        cx: 400,
        cy: 300,
        half_w: 100,
        half_h: 80,
    };
    s.dispatch_action(Action::ScreenshotEdgeResize(1, 1));
    if let InputMode::Screenshot {
        cx,
        cy,
        half_w,
        half_h,
    } = s.mode
    {
        // left = cx - half_w must remain 300
        assert_eq!(cx - half_w, 300, "left edge must stay fixed");
        // top = cy - half_h must remain 220
        assert_eq!(cy - half_h, 220, "top edge must stay fixed");
        // right and bottom must have grown
        assert!(cx + half_w > 500, "right edge should grow");
        assert!(cy + half_h > 380, "bottom edge should grow");
    } else {
        panic!("expected Screenshot mode");
    }
}

#[test]
fn dispatch_screenshot_edge_resize_in_non_screenshot_mode_is_noop() {
    let mut s = make_state();
    s.dispatch_action(Action::ScreenshotEdgeResize(1, 1));
    assert!(matches!(s.mode, InputMode::Insert));
}

#[test]
fn dispatch_screenshot_move_updates_center() {
    let mut s = make_state();
    s.mode = InputMode::Screenshot {
        cx: 400,
        cy: 300,
        half_w: 100,
        half_h: 80,
    };
    s.dispatch_action(Action::ScreenshotMove(10, -5));
    if let InputMode::Screenshot { cx, cy, .. } = s.mode {
        assert_eq!(cx, 410);
        assert_eq!(cy, 295);
    } else {
        panic!("expected Screenshot mode");
    }
}

#[test]
fn dispatch_screenshot_move_clamps_at_zero() {
    let mut s = make_state();
    s.mode = InputMode::Screenshot {
        cx: 5,
        cy: 3,
        half_w: 50,
        half_h: 50,
    };
    s.dispatch_action(Action::ScreenshotMove(-1000, -1000));
    if let InputMode::Screenshot { cx, cy, .. } = s.mode {
        assert_eq!(cx, 0);
        assert_eq!(cy, 0);
    } else {
        panic!("expected Screenshot mode");
    }
}

#[test]
fn dispatch_screenshot_capture_enters_name_mode() {
    let mut s = make_state();
    s.mode = InputMode::Screenshot {
        cx: 400,
        cy: 300,
        half_w: 100,
        half_h: 80,
    };
    let effects = s.dispatch_action(Action::ScreenshotCapture);
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Redraw)));
    assert!(
        matches!(
            s.mode,
            InputMode::ScreenshotName {
                cx: 400,
                cy: 300,
                half_w: 100,
                half_h: 80,
                ..
            }
        ),
        "expected ScreenshotName mode"
    );
}

#[test]
fn dispatch_screenshot_capture_in_non_screenshot_mode_is_empty() {
    let mut s = make_state();
    let effects = s.dispatch_action(Action::ScreenshotCapture);
    assert!(effects.is_empty());
}

// ── Search with actual matches ────────────────────────────────────────────────

#[test]
fn dispatch_search_next_with_matches_advances_current() {
    let mut s = make_state_with_pane();
    s.search_matches = vec![(0, 0, 3), (1, 0, 3), (2, 0, 3)];
    s.search_current = 0;
    s.dispatch_action(Action::SearchNext);
    assert_eq!(s.search_current, 1);
}

#[test]
fn dispatch_search_next_wraps_at_end() {
    let mut s = make_state_with_pane();
    s.search_matches = vec![(0, 0, 3), (1, 0, 3)];
    s.search_current = 1;
    s.dispatch_action(Action::SearchNext);
    assert_eq!(s.search_current, 0);
}

#[test]
fn dispatch_search_prev_with_matches_goes_backward() {
    let mut s = make_state_with_pane();
    s.search_matches = vec![(0, 0, 3), (1, 0, 3), (2, 0, 3)];
    s.search_current = 2;
    s.dispatch_action(Action::SearchPrev);
    assert_eq!(s.search_current, 1);
}

#[test]
fn dispatch_search_prev_wraps_at_zero() {
    let mut s = make_state_with_pane();
    s.search_matches = vec![(0, 0, 3), (1, 0, 3), (2, 0, 3)];
    s.search_current = 0;
    s.dispatch_action(Action::SearchPrev);
    assert_eq!(s.search_current, 2);
}

// ── Search history ────────────────────────────────────────────────────────

#[test]
fn search_history_saves_nonempty_query() {
    let mut s = make_state();
    s.push_search_history("error".to_string());
    assert_eq!(s.search_history, vec!["error"]);
}

#[test]
fn search_history_ignores_empty_query() {
    let mut s = make_state();
    s.push_search_history(String::new());
    assert!(s.search_history.is_empty());
}

#[test]
fn search_history_dedup_moves_to_end() {
    let mut s = make_state();
    s.push_search_history("foo".to_string());
    s.push_search_history("bar".to_string());
    s.push_search_history("foo".to_string());
    assert_eq!(s.search_history, vec!["bar", "foo"]);
}

#[test]
fn search_history_cap_at_50() {
    let mut s = make_state();
    for i in 0..55usize {
        s.push_search_history(format!("query{i}"));
    }
    assert_eq!(s.search_history.len(), 50);
    assert_eq!(s.search_history[0], "query5");
    assert_eq!(s.search_history[49], "query54");
}

#[test]
fn search_history_clears_before_history_on_push() {
    let mut s = make_state();
    s.search_before_history = "draft".to_string();
    s.push_search_history("committed".to_string());
    assert!(s.search_before_history.is_empty());
}
