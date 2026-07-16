use super::*;
use crate::terminal::grid::{Color, Grid, GridColors};

fn make_grid(cols: usize, rows: usize) -> Grid {
    Grid::with_colors(
        cols,
        rows,
        GridColors {
            fg: Color::WHITE,
            bg: Color::BLACK,
            cursor: Color::CURSOR,
            selection: Color::SELECTION,
            palette: [Color::BLACK; 16],
        },
        1000,
    )
}

#[test]
fn empty_query_returns_empty() {
    let g = make_grid(10, 5);
    assert!(compute_search_matches(&g, "", false).is_empty());
}

#[test]
fn invalid_regex_returns_empty() {
    let g = make_grid(10, 5);
    assert!(compute_search_matches(&g, "[invalid", false).is_empty());
}

#[test]
fn finds_match_in_live_grid() {
    let mut g = make_grid(10, 3);
    for c in "hello".chars() {
        g.write_char(c);
    }
    let matches = compute_search_matches(&g, "hello", false);
    assert_eq!(matches.len(), 1);
    let sb_len = g.scrollback.len();
    assert_eq!(matches[0].0, sb_len); // first live row
    assert_eq!(matches[0].1, 0);
    assert_eq!(matches[0].2, 5);
}

#[test]
fn finds_match_in_scrollback() {
    // 10-col, 2-row grid: writing 21+ chars pushes the first row into scrollback.
    let mut g = make_grid(10, 2);
    for c in "hello     ".chars() {
        g.write_char(c);
    }
    // Fill the second row and trigger one scroll_up (21 chars total)
    for c in "           ".chars() {
        g.write_char(c);
    }
    assert!(!g.scrollback.is_empty());
    let matches = compute_search_matches(&g, "hello", false);
    assert!(!matches.is_empty());
    assert_eq!(matches[0].0, 0); // scrollback row 0
    assert_eq!(matches[0].1, 0);
}

#[test]
fn no_match_returns_empty() {
    let mut g = make_grid(10, 3);
    g.write_char('a');
    assert!(compute_search_matches(&g, "xyz", false).is_empty());
}

#[test]
fn multiple_matches_same_row() {
    let mut g = make_grid(20, 3);
    for c in "ab_ab_ab            ".chars() {
        g.write_char(c);
    }
    let matches = compute_search_matches(&g, "ab", false);
    assert_eq!(matches.len(), 3);
}

#[test]
fn regex_pattern_works() {
    let mut g = make_grid(20, 3);
    for c in "foo123bar           ".chars() {
        g.write_char(c);
    }
    let matches = compute_search_matches(&g, r"\d+", false);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].1, 3); // col 3
    assert_eq!(matches[0].2, 3); // len 3
}

#[test]
fn case_sensitive_misses_differing_case() {
    let mut g = make_grid(20, 3);
    for c in "Hello hello HELLO   ".chars() {
        g.write_char(c);
    }
    // Case-sensitive: only the exact lowercase "hello" matches.
    assert_eq!(compute_search_matches(&g, "hello", false).len(), 1);
}

#[test]
fn case_insensitive_matches_all_cases() {
    let mut g = make_grid(20, 3);
    for c in "Hello hello HELLO   ".chars() {
        g.write_char(c);
    }
    // Case-insensitive: all three variants match.
    assert_eq!(compute_search_matches(&g, "hello", true).len(), 3);
}

#[test]
fn scroll_offset_live_row_returns_zero() {
    assert_eq!(compute_scroll_offset(10, 10, 24), 0);
    assert_eq!(compute_scroll_offset(15, 10, 24), 0);
}

#[test]
fn scroll_offset_top_of_scrollback_clamps_to_sb_len() {
    // abs_row=0, sb_len=100, grid_rows=24 → (100+12-0).min(100) = 100
    let offset = compute_scroll_offset(0, 100, 24);
    assert_eq!(offset, 100);
}

#[test]
fn scroll_offset_does_not_exceed_sb_len() {
    let offset = compute_scroll_offset(0, 5, 24);
    assert!(offset <= 5);
}

#[test]
fn scroll_offset_mid_scrollback() {
    // abs_row=50, sb_len=100, grid_rows=24 → (100+12-50).min(100) = 62
    let offset = compute_scroll_offset(50, 100, 24);
    assert_eq!(offset, 62);
}

#[test]
fn scroll_offset_exact_boundary() {
    // abs_row == sb_len → live row → 0
    assert_eq!(compute_scroll_offset(50, 50, 24), 0);
}

// ── extract_match_text ────────────────────────────────────────────────────────

#[test]
fn extract_from_live_grid() {
    let mut g = make_grid(10, 3);
    for c in "hello".chars() {
        g.write_char(c);
    }
    let sb_len = g.scrollback.len();
    // abs_row = sb_len → live row 0; col=0, len=5
    assert_eq!(extract_match_text(&g, sb_len, 0, 5), "hello");
}

#[test]
fn extract_from_scrollback() {
    let mut g = make_grid(10, 2);
    for c in "hello     ".chars() {
        g.write_char(c);
    }
    // push hello into scrollback
    for c in "           ".chars() {
        g.write_char(c);
    }
    assert!(!g.scrollback.is_empty());
    let text = extract_match_text(&g, 0, 0, 5);
    assert_eq!(text, "hello");
}

#[test]
fn extract_partial_match() {
    let mut g = make_grid(10, 3);
    for c in "foobar".chars() {
        g.write_char(c);
    }
    let sb_len = g.scrollback.len();
    assert_eq!(extract_match_text(&g, sb_len, 3, 3), "bar");
}

#[test]
fn extract_clamps_to_grid_cols() {
    let mut g = make_grid(5, 3);
    for c in "abcde".chars() {
        g.write_char(c);
    }
    let sb_len = g.scrollback.len();
    // len goes beyond cols — should clamp
    let text = extract_match_text(&g, sb_len, 3, 10);
    assert_eq!(text.len(), 2); // cols 3 and 4
}
