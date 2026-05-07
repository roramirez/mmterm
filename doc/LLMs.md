# mmterm — LLM Quick Reference

Cross-platform CPU-rendered terminal emulator in Rust.
Full spec: `doc/SPEC.md`. This file is the dense implementation reference.

## File Map

| File | What lives here |
|---|---|
| `src/main.rs` | `App`, `TabState`, `PaneEntry`; event loop; `match action` dispatch |
| `src/input/keybindings.rs` | `Action` enum; `handle_key`, `handle_ctrl_w` |
| `src/input/mode.rs` | `InputMode` enum (Insert / Normal / Visual) |
| `src/pty/session.rs` | `PtySession` — fork PTY, spawn shell, read/write bytes |
| `src/terminal/grid.rs` | `Grid`, `Cell`, `Color` — VT cell grid + scrollback |
| `src/terminal/parser.rs` | `vte` performer → `Grid` mutations |
| `src/renderer/text.rs` | `Renderer`, `FontMetrics`, `PaneView`; all pixel writes |
| `src/renderer/glyph.rs` | `GlyphCache` — fontdue rasterization |
| `src/ui/layout.rs` | `Layout`, `SplitDir`, `Node` (binary-tree pane splits) |
| `src/ui/pane.rs` | `Pane` — scroll offset, selection, cursor |
| `src/config.rs` | `Config`, `*Config` structs — TOML load/save |
| `src/tui_config.rs` | `ConfigPanel`, `Field` — in-process config editor |

Constants in `src/ui/layout.rs`: `TAB_BAR_H = 22`, `STATUS_BAR_H = 22`.

## Core Types (abbreviated)

```rust
struct App {
    renderer: Renderer,           // shared glyph cache + default font_px
    tabs: Vec<TabState>,
    active_tab: usize,
    next_pane_id: usize,
    mode: InputMode,
    modifiers: Modifiers,
    cursor_blink: bool,
    blink_last: Instant,
    ctrl_w_pending: bool,
    config: Config,
    config_panel: Option<ConfigPanel>,
}

struct TabState {
    panes: HashMap<usize, PaneEntry>,
    layout: Layout,
    active: usize,              // active pane id
    metrics: FontMetrics,       // session-only, NOT saved to config
    name: Option<String>,
}

struct PaneEntry {
    pane: Pane,
    pty: PtySession,
    rx: Receiver<Vec<u8>>,      // PTY → render thread channel
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
    Quit, None,
}
```

## Adding a Feature — 4-Step Checklist

### 1. Keybinding
- Add variant to `Action` in `src/input/keybindings.rs`
- Return it from `handle_key` or `handle_ctrl_w`
- Handle it in the `match action` block(s) in `src/main.rs` (two blocks: `ctrl_w_pending` path and normal path)

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
renderer.draw(buf, buf_width, buf_height, panes: &[PaneView], separators, ...)
renderer.draw_config_panel(buf, bw, bh, panel)
```

## Critical Invariants

- `renderer.font_px` is the config default reference, not the active size — always use `tab.metrics.font_px`
- Never borrow `self.renderer` and `self.tabs[i]` mutably in the same expression — bind `let idx = self.active_tab` first
- Clamp font size: `.clamp(6.0, 72.0)`, guard with epsilon before recomputing metrics
- Bounds-check every pixel write: `if idx < buf.len()`
- Use `Instant` for time-sensitive behaviour (blink, key repeat) — never frame counts
- `F_*` constants in `tui_config.rs` must be contiguous and match the `fields` vec order exactly
- Do not `unwrap()` on paths reachable at runtime — use `?`, `if let`, or a logged fallback
- Do not persist session-only state (per-tab font size, scroll offset) to config

## Logging

`log::info!` / `log::warn!` — activated with `RUST_LOG=info mmterm`.
