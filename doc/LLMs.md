# mmterm — LLM Quick Reference

Cross-platform CPU-rendered terminal emulator in Rust.
Full spec: `doc/SPEC.md`. This file is the dense implementation reference.

## File Map

| File | What lives here |
|---|---|
| `src/main.rs` | `App`; event loop; winit wiring; `wakeup_pending` |
| `src/app_state.rs` | `AppState`, `TabState`, `PaneEntry`, `AppEffect`; action dispatch |
| `src/drain.rs` | `ParseEffect` enum; `spawn_parser_thread`; `drain_effects` |
| `src/input/keybindings.rs` | `Action` enum; literal/PTY encoding + not-yet-tabled per-mode handlers (Insert/Normal/Visual/QuitSave/Screenshot) |
| `src/input/keymap.rs` | `KeyMap`, `BindingKey`, `Mods`, `KeyToken`, `ModeClass`, `default_keymap()`, binding parser, `action_from_name`/`name_of_action` registry — single source of truth for shortcut bindings |
| `src/input/mode.rs` | `InputMode` enum (Insert / Normal / Visual / QuitSave) |
| `src/pty/session.rs` | `PtySession` — fork PTY, spawn shell, read/write bytes |
| `src/terminal/grid.rs` | `Grid`, `Cell`, `Color` — VT cell grid + scrollback |
| `src/terminal/parser.rs` | `vte` performer → `Grid` mutations |
| `src/renderer/text.rs` | `Renderer`, `FontMetrics`, `PaneView`; all pixel writes |
| `src/renderer/glyph.rs` | `GlyphCache` — fontdue rasterization |
| `src/ui/layout.rs` | `Layout`, `SplitDir`, `Node` (binary-tree pane splits) |
| `src/ui/pane.rs` | `Pane` — scroll offset, selection, cursor |
| `src/config/mod.rs` | `Config`, `*Config` structs — TOML load/save |
| `src/config/tui_config.rs` | `ConfigPanel`, `Field` — in-process config editor |
| `src/session/mod.rs` | `SavedSession`, `SavedTab`, `SavedNode` — session persistence save/load |
| `src/theme/mod.rs` | `ResolvedTheme`, `load_theme()`, `install_bundled_themes()` |
| `src/theme/themes/*.toml` | 9 bundled theme files embedded via `include_str!` |
| `src/input/motion.rs` | `word_forward`, `word_backward`, `word_end` — Visual mode `w`/`b`/`e` |
| `src/input/mouse.rs` | SGR/X10 mouse event encoding helper |
| `src/input/mouse_ops.rs` | `impl App` — mouse hit-testing, selection, URL open |
| `src/ui/command_palette.rs` | static command palette entries; filter and action dispatch |
| `src/ui/statusbar.rs` | right-side status bar template resolution (`%pwd`, `%date`) |
| `src/ui/tabs.rs` | tab index arithmetic (next/prev/move/close) |
| `src/renderer/views.rs` | `collect_pane_views`, `build_tab_titles` — pane view assembly |
| `src/renderer/render_ops.rs` | `impl App` — `redraw()`, focus, frame orchestration |
| `src/renderer/screenshot.rs` | screenshot save to PNG |

Constants in `src/ui/layout.rs`: `TAB_BAR_H = 22`, `STATUS_BAR_H = 22`.

## Visual Mode Selection (implemented)

- `InputMode::Visual { start_col, start_row, cur_col, cur_row, anchored: bool }`
- `anchored: false` — cursor navigates freely, no selection highlight shown
- `anchored: true` — selection highlighted from `(start_col, start_row)` to `(cur_col, cur_row)`
- `v` in Visual mode → `Action::VisualAnchor` → sets `start = cur, anchored = true`
- `o` → `Action::VisualSwapAnchor` → swaps start ↔ cur
- `w`/`b`/`e` → `Action::VisualWordForward/Backward/End` → handled in `main.rs` via `motion::word_*`
- `y` → `Action::Copy`; `Y` → `Action::VisualYankLine` (copies `cur_row`, exits to Insert)
- `k`/↑ at row 0 → `Action::VisualBoundaryUp(1)` → scrolls viewport, cursor stays at row 0, anchor shifts +1
- `j`/↓ at last row → `Action::VisualBoundaryDown(1)` → scrolls viewport, cursor stays at last row, anchor shifts -1
- `ScrollUp`/`ScrollDown` in Visual mode adjust both `start_row` and `cur_row` to track content
- Entering Visual from Normal while `scroll_offset > 0` starts cursor at `(0, 0)` of viewport
- Renderer `is_cursor`: in Visual mode uses `cur_col/cur_row` with `blink_visible` (not PTY cursor)
- Renderer `selection_range`: only `Some(...)` when `anchored == true`
- Coordinates are viewport-relative (0..grid.rows); `selected_text` uses `scroll_offset` to read scrollback

## Scrollback Search (implemented)

- `InputMode::Search { query: String }` — entered from Normal with `/`
- `App.search_matches: Vec<(usize, usize)>` — `(abs_row, start_col)` sorted by abs_row
- `App.search_current: usize` — index of highlighted match
- `abs_row` coordinate space: `0..sb_len` = scrollback, `sb_len..sb_len+grid.rows` = live grid
- `App::update_search_matches` recomputes on every query keystroke (char-based windows scan)
- `App::scroll_to_match(idx)` sets `pane.scroll_offset = (sb_len + grid_rows/2).saturating_sub(abs_row).min(sb_len)`
- `PaneView` carries `search_matches`, `search_match_len`, `search_current` to the renderer
- `draw_pane` uses binary-search (`partition_point`) to find the row's match range, then linear scan per cell
- Match highlight: yellow bg `#f9e2af`; current match: orange bg `#fe640d`; both use dark fg `#11111d`
- Status bar shows `/query  [current/total]` when in Search mode

## Pane Zoom (implemented)

- `TabState.zoomed: bool` — session-only flag, never persisted
- `Ctrl-W z` toggles zoom; handled in the `ctrl_w_pending` dispatch block in `src/main.rs`
- When zoomed, `redraw()` builds a single `PaneView` at `[0, TAB_BAR_H, w, h - TAB_BAR_H - STATUS_BAR_H]` instead of iterating the layout tree; separators are suppressed
- `do_split` and `do_close_pane` set `zoomed = false` before acting — layout is never modified
- Mouse hit-testing (`pane_at_pixel`, `pixel_to_cell`) uses the original layout rects while zoomed; cell coordinates may be slightly off in zoomed mode

## Session Persistence (implemented)

- Triggered by `Ctrl+Q` (or window close) when `config.general.restore_session = true`
- `InputMode::QuitSave` — new mode; `renderer.draw_save_session_confirm()` draws a centered dialog (same style as `draw_quit_confirm`) over the dimmed terminal
- Keys in `QuitSave`: `s`/`S` → `Action::QuitSaveSession` → `AppEffect::SaveSessionAndQuit`; `q`/`n`/`Enter` → `Action::QuitNoSave` → `AppEffect::Quit`; `Esc` → `SetMode(Insert)`
- `App::build_saved_session()` collects tab names, active panes, CWDs (via `pty.cwd()` → `/proc/<pid>/cwd`), and layout trees
- `Layout::to_saved_node()` walks the `Node` tree in DFS order, returns `(SavedNode, Vec<pane_id>)` with serial slot indices for leaves
- `Layout::from_saved_node(node, slot_to_id, w, h)` reconstructs the tree substituting slot indices with new pane IDs
- **Named scopes** — `--scope <name>` (also `--scope=<name>` / `-s <name>`) routes save/load to `~/.config/mmterm/sessions/<name>.toml`; omitting the flag uses the default `~/.config/mmterm/session.toml`
- `App.scope: Option<String>` holds the active scope; passed to `App::new()` from `main()`
- `session::session_path_for(scope: Option<&str>) -> PathBuf` — `None` → default path, `Some(name)` → scoped path under `sessions/`
- Save uses `session::save_to(&path, &s)` where `path = session_path_for(self.scope.as_deref())`; atomic write (`.tmp` → rename)
- On startup `resumed()`: if `restore_session = true`, calls `session::load_from(&session_path_for(scope))` and `App::restore_session()`; CWDs that no longer exist fall back to `$HOME`
- `--list-scopes` prints sorted scope names (`*.toml` stems from `sessions/` dir) and exits — implemented via `session::list_scopes()` / `list_scopes_in(dir)` and `list_scopes_requested()` in `main.rs`
- What is saved: `active_tab`, per-tab `name`, `active_pane` slot, `pane_cwds`, `layout` tree with `dir`/`ratio`
- What is NOT saved: PTY content, scrollback, per-tab font size, zoom, scroll offset

## OSC 8 Hyperlinks (implemented)

- `Grid.active_url: Option<Arc<String>>` — set by the parser on `\e]8;;uri\e\\` open, cleared on `\e]8;;\e\\` close
- `Cell.url: Option<Arc<String>>` — stamped on every cell written while a URL is active
- Renderer draws cells with a non-None URL with blue underline (`#89b4fa`)
- `App.hovered_url: Option<String>` — updated on `CursorMoved`; cursor switches to `Pointer` when hovering a link
- Left-click on a linked cell opens the URL via `xdg-open` (Linux) / `open` (macOS)

## Configurable Keymap (implemented)

- Single source of truth: `src/input/keymap.rs`. `default_keymap()` returns all built-in Global-scope shortcuts (Ctrl, Ctrl+Shift, Alt, ⌘/Super, `Ctrl+W` chords) AS DATA.
- `KeyMap::from_config(&KeybindingsConfig)` overlays the user's `[keybindings]` table onto a copy of the defaults: valid `binding=action` inserts/replaces; `binding="none"` removes; invalid entries are skipped + collected as `KeymapError` (Parse / UnknownAction / ShadowsInput).
- Dispatch: `handle_key` → `handle_key_modified(keymap, ...)` builds `(Mods, KeyToken)`, calls `keymap.lookup(ModeClass::Global, &bkey)` FIRST. Hit → `action_from_name(name, DispatchCtx { grid_rows, mode })`. Miss → if `cmd` held, swallow (`Action::None`); else fall through to `handle_key_inner` (encoding/per-mode fallback).
- `Ctrl+W` chords: `handle_ctrl_w(keymap, event)` → `handle_ctrl_w_keymap` looks up a `BindingKey { mods: ctrl, token: 'w', chord_tail: Some((no_mods, tail)) }`. Tail token is NOT lowercased, so `Ctrl+W R` (rotate backward) stays distinct from `Ctrl+W r` (forward).
- `cmd` token = ⌘ (macOS) / Super (Linux/Win); winit reports both via `super_key()` → the `cmd` flag.
- Map stores `&'static str` action names (not `Action` — several variants aren't `Clone` and some need runtime context). `intern_known_name` interns validated names; keep it in sync with `action_from_name`.
- `AppState` builds the merged `KeyMap` once in `new()` and stores `keymap`, `keymap_invalid_count`, `keymap_notice_until`. `render_ops` surfaces `keymap_notice()` into the status bar `right_text` (transient ~6 s).
- Config: `KeybindingsConfig(BTreeMap<String,String>)` newtype, `#[serde(default)]` field on `Config`; documented example block lives in `assets/config.toml`. No `F_*`/`Field` in `tui_config.rs` — a dynamic map has no Field representation; keybindings are file-configured.
- Out of scope (Phase 2): `normal:` / `visual:` modal remapping (rows not yet populated); arbitrary new chord prefixes.

## Core Types (abbreviated)

```rust
struct App {
    state: AppState,              // all mutable terminal state (tabs, mode, config…)
    renderer: Renderer,           // shared glyph cache + default font_px
    proxy: EventLoopProxy<()>,
    wakeup_pending: Arc<AtomicBool>, // set by parser threads; cleared in user_event
    window: Option<Arc<Window>>,
    surface_size: (u32, u32),
    scope: Option<String>,
}

struct TabState {
    panes: HashMap<usize, PaneEntry>,
    layout: Layout,
    active: usize,              // active pane id
    metrics: FontMetrics,       // session-only, NOT saved to config
    name: Option<String>,
    zoomed: bool,               // session-only, NOT saved to config
    bell_flash_start: Option<Instant>,   // set on BEL; drives fade intensity
    bell_flash_until: Option<Instant>,   // expiry for next_bell_wakeup scheduling
    bell_cooldown_until: Option<Instant>, // 500 ms guard against repeated BELs
}

struct PaneEntry {
    pane: Pane,
    pty: PtySession,
    effects_rx: Receiver<ParseEffect>,           // parser thread → main thread
    log_file: Arc<Mutex<Option<File>>>,
    pending_resize: Arc<Mutex<Option<(usize, usize)>>>, // main → parser (non-blocking)
    _parser_thread: JoinHandle<()>,
}

// Side-effects produced by each pane's parser thread and consumed on the main thread.
enum ParseEffect {
    PtyResponse(Vec<u8>),    // write back to PTY (OSC responses, etc.)
    ClipboardWrite(String),
    ClipboardRead,
    Bell,
    ScrollbackChanged { old: usize, new: usize },
    Resized { delta: isize, new_sb: usize },
    Disconnected,
}

pub struct FontMetrics {
    pub font_px: f32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub baseline: u32,
}

pub struct PaneView<'a> {
    pub grid: &'a Grid,
    pub rect: [u32; 4],         // [x, y, w, h] in pixels
    pub scroll_offset: usize,
    pub is_active: bool,
    pub show_cursor: bool,
}

pub struct Renderer {
    pub font_px: f32,           // config default — reference only, NOT for layout
    pub glyphs: GlyphCache,
}
```

## Action Enum (all variants)

```rust
pub enum Action {
    SendToPty(Vec<u8>), SetMode(InputMode), Paste, Copy,
    ScrollUp(usize), ScrollDown(usize), ScrollToTop, ScrollToBottom,
    SplitH, SplitV,
    FocusLeft, FocusRight, FocusUp, FocusDown, FocusNext, ClosePane,
    CtrlWPrefix, OpenConfig,
    NewTab, NextTab, PrevTab, CloseTab, RenameTab,
    IncreaseFontSize, DecreaseFontSize, ResetFontSize,
    SearchOpen, SearchNext, SearchPrev,
    VisualAnchor, VisualSwapAnchor,
    VisualWordForward, VisualWordBackward, VisualWordEnd,
    VisualYankLine,
    VisualBoundaryUp(usize), VisualBoundaryDown(usize),
    ZoomPane,
    Quit, QuitSaveSession, QuitNoSave, None,
}
```

## Adding a Feature — 4-Step Checklist

Every commit must include an entry in `CHANGELOG.md` under
`## [Unreleased]`. Use the appropriate section:

| Section | When |
|---|---|
| `Added` | New user-visible feature or capability |
| `Changed` | Modified behaviour of an existing feature |
| `Fixed` | Bug fix |
| `Removed` | Feature removed |
| `Security` | Security fix |
| `Documentation` | Changes to `doc/`, `README.md`, or other user-facing docs |

One line per entry, imperative mood, lowercase first letter. Example:

```markdown
## [Unreleased]

### Added
- regex search with copy-match and clear-scrollback actions
```

### 1. Keybinding / Action
- Add a variant to `Action` in `src/input/keybindings.rs` (only if a new action type is needed)
- Add a `name ↔ Action` entry to the registry in `src/input/keymap.rs` (`action_from_name` + `name_of_action`)
- Add a default row to `default_keymap()` in `src/input/keymap.rs`
- Handle the `Action` in `AppState::dispatch_action` (`src/app_state.rs`) if not already handled
- Literal text / cursor / PTY-byte encoding is NOT a keymap binding — it stays in `handle_insert`/`cursor_seq`/`encode_*` (the fallback)

### 2. Config option
- Add field to the right `*Config` struct in `src/config.rs` with `#[serde(default = "fn_name")]`
- Update `Default` impl and the TOML template string in `save()`
- Add `F_*` constant in `src/tui_config.rs` (must be contiguous, same order as `fields` vec)
- Update `from_config`, `build_config`, and the `fields` vec in `src/tui_config.rs`

### 3. Session-only (per-tab) state
- Add field to `TabState` in `src/main.rs`
- Initialize in `new_tab()` (or wherever `TabState` is constructed)
- Do NOT persist to config

### 4. Rendering
- All pixel writes go in `src/renderer/text.rs`
- Pass data via `PaneView` or a dedicated `draw_*` method on `Renderer`
- Do NOT reach into `App` from the renderer

## Key API: Grid (`src/terminal/grid.rs`)

```rust
grid.cell(col, row) -> &Cell
grid.cell_mut(col, row) -> &mut Cell
grid.write_char(c: char)          // writes at cursor, advances
grid.blank_cell() -> Cell         // respects bg color — use for erase
grid.erase_cell() -> Cell         // same but foreground cleared too
grid.clear_line(row)
grid.clear_screen()
grid.scroll_up(n)
grid.scroll_down(n)
grid.scrollback_len() -> usize
grid.resize(cols, rows)
grid.enter_alternate_screen()
grid.exit_alternate_screen()
grid.selected_text(sc, sr, ec, er) -> String
grid.palette[n]                   // ANSI color 0–15 — never hardcode RGB
grid.cols, grid.rows
```

## Key API: FontMetrics (`src/renderer/text.rs`)

```rust
FontMetrics::compute(glyphs, font_px) -> FontMetrics
metrics.grid_size_for(w, h) -> (cols, rows)  // ALWAYS use this, never inline
```

Use `tab.metrics` (session-scoped). Never use `renderer.font_px` for layout.

## Key API: Renderer (`src/renderer/text.rs`)

```rust
renderer.make_metrics(font_px) -> FontMetrics
renderer.draw(buf, buf_width, buf_height, panes: &[PaneView], separators, ..., theme: &ResolvedTheme)
renderer.draw_config_panel(buf, bw, bh, panel)
```

## Theme System (`src/theme.rs`)

`ResolvedTheme` is the single source of truth for all colors at runtime.
It is stored on `App` and passed to the renderer on every frame.

```rust
pub struct ResolvedTheme {
    pub foreground:     Color,   // terminal default fg
    pub background:     Color,   // terminal default bg
    pub cursor:         Color,   // block cursor
    pub selection:      Color,   // visual selection bg
    pub palette:        [Color; 16],  // ANSI 0–15
    // UI chrome
    pub search_match:   Color,   // search highlight bg
    pub search_current: Color,   // current match bg
    pub scrollbar:      Color,   // scrollbar thumb at live view
    pub badge:          Color,   // active tab badge
    pub separator:      Color,   // pane + bar separator line
}
```

Key functions:
```rust
load_theme(name: &str, themes_dir: &Path) -> Result<ResolvedTheme, String>
install_bundled_themes(themes_dir: &Path)   // writes .toml files if missing
list_themes(themes_dir: &Path) -> Vec<String>  // sorted names
default_theme() -> ResolvedTheme            // BUNDLED[0] = "default"
themes_dir() -> PathBuf                     // ~/.config/mmterm/themes/
```

Color sources in the renderer:
- Tab bar badge (active): `theme.badge`
- Tab bar badge (inactive): dimmed `theme.background`
- Tab bar / status bar separator: `theme.separator`
- Mode badges (NORMAL/INSERT/VISUAL/SEARCH): `theme.palette[4/2/5/3]`
- Scrollbar thumb (scrolled / live): `theme.palette[4]` / `theme.scrollbar`
- Hyperlink underline: `theme.palette[4]`
- Activity dot, REC badge: `theme.palette[1]`

Adding a new theme-driven color: add the field to `ResolvedTheme`,
add it to `ThemeFile` (optional), provide a palette-derived default in
`resolve()`, add to all 9 bundled `.toml` files, update `draw()`.

## Code Quality Gates (kimun)

`km` (`cargo install --git https://github.com/lnds/kimun`) — static + git analysis.
Config: `.kimun.toml` at repo root. Current score target: see `fail_below` there.

Before every commit:
```
km score --trend origin/main --fail-if-worse
```

Before touching a file:
```
km hotspots    # high churn × complexity files
km knowledge   # bus-factor risk (>80% single author)
```

In PR context:
```
km score diff main   # per-dimension delta; negative deltas need justification
```

## Critical Invariants

- `renderer.font_px` is the config default reference, not the active size — always use `tab.metrics.font_px`
- Never borrow `self.renderer` and `self.tabs[i]` mutably in the same expression — bind `let idx = self.active_tab` first
- Clamp font size: `.clamp(6.0, 72.0)`, guard with epsilon before recomputing metrics
- Bounds-check every pixel write: `if idx < buf.len()`
- Use `Instant` for time-sensitive behaviour (blink, key repeat) — never frame counts
- `F_*` constants in `tui_config.rs` must be contiguous and match the `fields` vec order exactly
- Do not `unwrap()` on paths reachable at runtime — use `?`, `if let`, or a logged fallback
- Do not persist session-only state (per-tab font size, scroll offset, zoom) to config

## Code Style

- Run `cargo fmt` before every commit — all code in the repo must be `rustfmt`-clean.
- Never manually align `match` arms, function arguments, or struct fields; let `rustfmt` decide.
- All code must pass `cargo clippy --locked -- -D warnings` with zero errors.

## Testing

- Every feature or bug fix must be accompanied by tests covering the new or changed behavior.
- Tests live in separate `*_test.rs` files alongside the source file, linked with `#[cfg(test)] #[path = "..."] mod tests;`.
- Run `cargo test` before reporting a task as complete.
- For keybinding changes: test modifier combinations beyond the happy path — e.g. if adding Shift+X, also test Ctrl+Shift+X, Alt+Shift+X, and the same key in every `InputMode`. Modifier interactions often produce surprising fall-through behavior that only a combined test will catch.

## Benchmarking

Criterion micro-benchmarks live in `benches/` and measure the hot paths in isolation — no GUI required.

```sh
cargo bench                                  # run all benchmarks
cargo bench parser_throughput                # parser only
cargo bench scroll_throughput                # scroll_up/scroll_down only
cargo bench -- --save-baseline before        # snapshot before an optimization
cargo bench -- --baseline before             # compare against snapshot
```

| Benchmark group | What it measures | Key bottleneck |
|---|---|---|
| `parser/realistic_256kb` | bytes/s through VTE parser on typical ANSI output | byte-by-byte `advance` loop |
| `parser/ascii_256kb` | bytes/s on pure printable ASCII | same — no SIMD fast-path yet |
| `parser/dense_sgr_64kb` | bytes/s on escape-sequence-heavy output | CSI dispatch overhead |
| `scroll_up/220x50` | µs per `scroll_up(1)` call on a standard grid | `Cell` clone cost |
| `seq_simulation/seq_1_100000` | end-to-end parse time for `seq 1 100000` (588 KB, ~100 K scrolls) | combined parser + scroll |

**Baseline numbers (as of 2026-05-16, after `rotate_left` fix):**

| Benchmark | Throughput / time |
|---|---|
| `parser/realistic_256kb` | ~885 KiB/s |
| `parser/ascii_256kb` | ~848 KiB/s |
| `scroll_up/220x50` | 15 µs / call (~22 GiB/s) |
| `seq_simulation/seq_1_100000` | ~1.4 s |

End-to-end benchmarks (must be run **inside a running mmterm session**):

```sh
bash bench/run_inside_mmterm.sh   # runs vtebench + termbench + plain I/O + memory
# results written to /tmp/mmterm_bench_results.txt
```

**When to run benchmarks:** before and after any change to `terminal/grid.rs`, `terminal/parser.rs`, `renderer/text.rs`, or `pty/session.rs`. Save a baseline before the change; compare after.

## Logging

`log::info!` / `log::warn!` — activated with `RUST_LOG=info mmterm`.
