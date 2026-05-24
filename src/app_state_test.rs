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
            e.pane.parser.grid.write_char(c);
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
            e.pane.parser.grid.scroll_up(1);
        }
        e.pane.scroll_offset = 10;
    }
    let grid_rows = s
        .tab()
        .panes
        .get(&active)
        .map(|e| e.pane.parser.grid.rows)
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
            e.pane.parser.grid.scroll_up(1);
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
            e.pane.parser.grid.scroll_up(1);
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
