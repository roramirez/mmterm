/// Integration scenarios: multi-step sequences that mirror real terminal sessions.
/// Each test feeds a stream of bytes that a real process would emit, then
/// asserts on the complete resulting state — not just one VT feature at a time.
use super::super::grid::{Color, Grid, GridColors};
use super::*;

struct TestParser {
    inner: TerminalParser,
    pub grid: Grid,
}

impl TestParser {
    fn process(&mut self, bytes: &[u8]) {
        self.inner.process(bytes, &mut self.grid);
    }
}

fn term(cols: usize, rows: usize) -> TestParser {
    TestParser {
        inner: TerminalParser::new(),
        grid: Grid::with_colors(
            cols,
            rows,
            GridColors {
                fg: Color::WHITE,
                bg: Color::BLACK,
                cursor: Color::CURSOR,
                selection: Color::SELECTION,
                palette: [Color::BLACK; 16],
            },
            10_000,
        ),
    }
}

// ── Scenario 1: coloured bash prompt ────────────────────────────────────────
//
// bash emits something like:
//   \e]2;user@host\a          OSC 2 window title
//   \e]7;file:///home/u\a     OSC 7 working directory
//   \e[1;32muser@host\e[0m:\e[1;34m~/code\e[0m$  (coloured prompt)
//
// After processing we expect:
//   - OSC title set
//   - CWD set
//   - prompt text in cells with correct bold+colour
//   - trailing "$ " in default colour

#[test]
fn scenario_bash_prompt_sets_title_cwd_and_color() {
    let mut p = term(40, 5);

    // OSC sequences come first (order matches real bash)
    p.process(b"\x1b]2;user@host\x07");
    p.process(b"\x1b]7;file:///home/user/code\x07");

    // Prompt: bold green "user@host", reset, ":", bold blue "~/code", reset, "$ "
    p.process(b"\x1b[1;32muser@host\x1b[0m:\x1b[1;34m~/code\x1b[0m$ ");

    assert_eq!(p.grid.osc_title.as_deref(), Some("user@host"));
    assert_eq!(p.grid.cwd.as_deref(), Some("/home/user/code"));

    // "u" — first char of "user@host" — bold, palette[2] (green)
    let u = p.grid.cell(0, 0);
    assert!(u.bold, "prompt should be bold");
    assert_eq!(u.fg, p.grid.palette[2], "prompt should use palette green");

    // ":" — separator after bold green section — default colour, not bold
    let colon = p.grid.cell(9, 0);
    assert!(!colon.bold);
    assert_eq!(colon.c, ':');

    // "~" — start of bold blue path
    let tilde = p.grid.cell(10, 0);
    assert!(tilde.bold);
    assert_eq!(tilde.fg, p.grid.palette[4], "path should use palette blue");

    // "$ " — plain suffix after reset
    let dollar = p.grid.cell(16, 0);
    assert!(!dollar.bold);
    assert_eq!(dollar.c, '$');
}

// ── Scenario 2: vim opens a file (alternate screen) ─────────────────────────
//
// vim (and most TUI apps) switch to alternate screen, paint their UI, then
// switch back. Primary screen content must survive intact.

#[test]
fn scenario_vim_alternate_screen_preserves_primary() {
    let mut p = term(20, 5);

    // Write something to the primary screen first
    p.process(b"primary content");

    // vim enters alternate screen, paints something, then exits
    p.process(b"\x1b[?1049h"); // enter alternate
    p.process(b"\x1b[2J"); // clear (alternate)
    p.process(b"\x1b[1;1H"); // home
    p.process(b"vim content");
    p.process(b"\x1b[?1049l"); // exit alternate → primary restored

    // Primary screen must have original content
    // "primary content": p=0 r=1 i=2 m=3 a=4 r=5 y=6 ' '=7 c=8
    assert_eq!(p.grid.cell(0, 0).c, 'p');
    assert_eq!(p.grid.cell(1, 0).c, 'r');
    assert_eq!(p.grid.cell(8, 0).c, 'c');

    // Cursor visibility restored by exit (vim re-enables it)
    p.process(b"\x1b[?25h");
    assert!(p.grid.cursor_visible);
}

// ── Scenario 3: ls --color output ───────────────────────────────────────────
//
// ls emits directory names in bold blue, regular files in default colour.

#[test]
fn scenario_ls_color_output_stamps_correct_attributes() {
    let mut p = term(40, 5);

    // Regular file (default colour)
    p.process(b"Cargo.toml");
    p.process(b"\r\n");

    // Directory (bold blue: \e[1;34m)
    p.process(b"\x1b[1;34msrc\x1b[0m");
    p.process(b"\r\n");

    // Executable (bold green: \e[1;32m)
    p.process(b"\x1b[1;32mmyapp\x1b[0m");

    // "Cargo.toml" — row 0, no colour, not bold
    let c = p.grid.cell(0, 0);
    assert_eq!(c.c, 'C');
    assert!(!c.bold);

    // "s" of "src" — row 1, bold blue
    let s = p.grid.cell(0, 1);
    assert_eq!(s.c, 's');
    assert!(s.bold);
    assert_eq!(s.fg, p.grid.palette[4]);

    // "m" of "myapp" — row 2, bold green
    let m = p.grid.cell(0, 2);
    assert_eq!(m.c, 'm');
    assert!(m.bold);
    assert_eq!(m.fg, p.grid.palette[2]);
}

// ── Scenario 4: scrollback + search ─────────────────────────────────────────
//
// A command produces many lines of output that push content into scrollback.
// Then a search query should find matches both in scrollback and in the live grid.

#[test]
fn scenario_output_fills_scrollback_and_search_finds_matches() {
    use crate::search::compute_search_matches;

    let cols = 20;
    let rows = 5;
    let mut p = term(cols, rows);

    // Emit 8 lines (> rows=5) so the first 3 go into scrollback
    for i in 0..8 {
        let line = format!("line{i:02} needle text \r\n");
        p.process(line.as_bytes());
    }

    assert!(
        p.grid.scrollback_len() > 0,
        "scrollback should have content"
    );

    // "needle" should appear in both scrollback and live grid rows
    let matches = compute_search_matches(&p.grid, "needle");
    assert!(
        matches.len() >= 8,
        "expected at least 8 matches, got {}",
        matches.len()
    );

    // Verify scrollback matches have abs_row < sb_len
    let sb_len = p.grid.scrollback_len();
    let sb_matches: Vec<_> = matches.iter().filter(|(r, _, _)| *r < sb_len).collect();
    assert!(!sb_matches.is_empty(), "should find matches in scrollback");

    // Verify live-grid matches have abs_row >= sb_len
    let live_matches: Vec<_> = matches.iter().filter(|(r, _, _)| *r >= sb_len).collect();
    assert!(!live_matches.is_empty(), "should find matches in live grid");
}

// ── Scenario 5: mouse reporting enable → click → disable ────────────────────
//
// Apps that use mouse (e.g. tmux, vim) enable mouse reporting, expect the
// terminal to respond to click events, then disable it on exit.

#[test]
fn scenario_mouse_reporting_enable_and_disable() {
    let mut p = term(80, 24);

    // App enables SGR mouse + any-event mode
    p.process(b"\x1b[?1003h"); // any-event
    p.process(b"\x1b[?1006h"); // SGR encoding

    assert_eq!(p.grid.mouse_mode, 1003);
    assert!(p.grid.mouse_sgr);

    // App exits and disables mouse
    p.process(b"\x1b[?1003l");
    p.process(b"\x1b[?1006l");

    assert_eq!(p.grid.mouse_mode, 0);
    assert!(!p.grid.mouse_sgr);
}

// ── Scenario 6: OSC 8 hyperlink session ─────────────────────────────────────
//
// A tool like `ls --hyperlink` emits OSC 8 links. We verify the URL is stamped
// on exactly the linked cells and cleared after the closing sequence.

#[test]
fn scenario_osc8_link_spans_correct_cells() {
    let mut p = term(40, 5);

    p.process(b"Visit ");
    // Open link
    p.process(b"\x1b]8;;https://mmterm.dev\x07");
    p.process(b"mmterm");
    // Close link
    p.process(b"\x1b]8;;\x07");
    p.process(b" for more.");

    // "Visit " → no URL
    assert!(p.grid.cell(0, 0).url.is_none());
    assert!(p.grid.cell(5, 0).url.is_none());

    // "mmterm" (cols 6..12) → has URL
    for col in 6..12 {
        let url = p.grid.cell(col, 0).url.as_deref();
        assert_eq!(
            url.map(|s| s.as_str()),
            Some("https://mmterm.dev"),
            "col {col} should carry the URL"
        );
    }

    // " for more." → no URL
    assert!(p.grid.cell(12, 0).url.is_none());
}
