# mmterm — Architecture & Feature Specification

## Overview

**mmterm** is a cross-platform GPU-free terminal emulator written in Rust.
It targets Linux (X11/Wayland) and macOS (Cocoa) via the `winit` windowing
abstraction and renders entirely with a CPU-based pixel buffer (softbuffer).
The design goal is a small, auditable, dependency-minimal emulator with
vim-style modal input, split panes, and multi-tab sessions.

---

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                        main.rs                       │
│  App (ApplicationHandler)                            │
│  ├── Vec<TabState>                                   │
│  │     ├── HashMap<id, PaneEntry>                    │
│  │     ├── Layout (binary-tree split)                │
│  │     ├── FontMetrics (per-tab, session-only)       │
│  │     └── active pane id                            │
│  ├── Renderer  (shared glyph cache)                  │
│  ├── Config    (loaded from TOML)                    │
│  └── InputMode (Insert / Normal / Visual / Search)   │
└────────────┬─────────────────────────────────────────┘
             │ winit events
    ┌────────▼────────┐     ┌──────────────────────────┐
    │  input/         │     │  pty/                    │
    │  keybindings.rs │     │  session.rs              │
    │  mode.rs        │     │  (portable-pty thread)   │
    └────────┬────────┘     └────────────┬─────────────┘
             │ Action enum               │ crossbeam channel
    ┌────────▼───────────────────────────▼─────────────┐
    │  terminal/                                        │
    │  parser.rs  (vte performer → grid mutations)      │
    │  grid.rs    (cells, scrollback, palette)          │
    └────────────────────────┬─────────────────────────┘
                             │ &Grid
    ┌────────────────────────▼─────────────────────────┐
    │  renderer/                                        │
    │  text.rs   (draw_pane, tab bar, status bar,       │
    │              TUI config panel)                    │
    │  glyph.rs  (GlyphCache via fontdue + font-kit)   │
    └────────────────────────┬─────────────────────────┘
                             │ u32 pixel buffer
                   softbuffer surface → window
```

### Module Responsibilities

| Module | File(s) | Role |
|---|---|---|
| `main` | `main.rs` | App state, event loop, tab/pane orchestration |
| `input` | `keybindings.rs`, `mode.rs` | Key → `Action` mapping, modal state |
| `pty` | `session.rs` | Fork PTY, spawn shell, read/write bytes |
| `terminal` | `grid.rs`, `parser.rs` | VT/ANSI parsing, cell grid, scrollback |
| `ui` | `layout.rs`, `pane.rs` | Binary-tree split layout, pane struct |
| `renderer` | `text.rs`, `glyph.rs` | Pixel rendering, glyph cache, UI chrome |
| `config` | `config.rs` | TOML config load/save, defaults |
| `tui_config` | `tui_config.rs` | In-process config editor panel |

---

## Features

### Terminal Emulation
- PTY backend via `portable-pty` (forkpty/posix_openpt on POSIX).
- ANSI/VT escape code parsing via `vte` (SGR colors, cursor movement,
  erase sequences, scrolling regions, OSC, etc.).
- 10 000-line scrollback buffer per pane (configurable via `scrollback_lines`).
- 16-color ANSI palette fully configurable per profile.
- True foreground/background/cursor/selection colors.
- Bracketed paste mode (`Ctrl+Shift+V`).
- DSR (`CSI 6 n`) and DA (`CSI c`) query-response: replies with cursor position
  and VT100 device attributes — fixes hangs in vim, less, and other TUI apps.
- DECSC/DECRC (`ESC 7` / `ESC 8`): save and restore cursor position **and** all
  SGR attributes (colors, bold, dim, underline, reverse, blink, strikethrough).
- SGR italic (`\e[3m` / `\e[23m`): stored per-cell and rendered with an italic
  font variant; JetBrainsMono Italic and Bold Italic are bundled as fallback.

### Rendering
- CPU-only pixel buffer (no GPU, no OpenGL, no Vulkan).
- Glyph rasterization via `fontdue`; system font discovery via `font-kit`
  (fontconfig on Linux, CoreText on macOS).
- Bundled fallback font (JetBrains Mono Regular/Bold/Italic/Bold Italic) for
  zero-config startup.
- Per-character bold and italic rendering using separate font faces.
- Correct advance-width cell sizing: `cell_width = M.advance_width.ceil()`.
- Baseline alignment per glyph using fontdue `ymin` metric.
- SGR overline (`\e[53m` / `\e[55m`): rendered as a 1 px line at the top of
  the cell; cleared with `\e[55m`.
- 4 px inner padding on all pane edges so text never touches the border.

### Input
- Four modal modes: **Insert** (default), **Normal**, **Visual**, **Search**.
- Mode cycle: `Ctrl+.` (Insert → Normal → Visual → Insert).
- `Ctrl+\` as alternative entry to Normal mode.
- Escape is always forwarded to the PTY — vim, less, etc. work as expected.
- Full function-key, arrow-key, and special-key forwarding in Insert mode.
- Ctrl+character encoding (Ctrl+A = 0x01 … Ctrl+Z = 0x1A).

### Split Panes
- Binary-tree layout: `Node::Leaf(id)` or `Node::Split { dir, ratio, a, b }`.
- Horizontal split: `Ctrl+W v` / Vertical split: `Ctrl+W s`.
- Focus navigation: `Ctrl+W h/j/k/l` or arrow keys.
- Cycle focus: `Ctrl+W w`.
- Close pane: `Ctrl+W q` (closes tab when last pane).
- 50/50 initial split ratio; separator is 1 px wide.
- Resize by dragging the separator (4 px click margin); cursor changes to `ColResize`/`RowResize` on hover.
- Keyboard resize: `Ctrl+Shift+←/→` (horizontal) and `Ctrl+Shift+↑/↓` (vertical) move the nearest separator by 5% per keypress.
- Minimum pane size is 10% of the parent region (ratio clamped to 0.1–0.9).

### Tabs
- `Ctrl+T` — new tab.
- `Ctrl+PageUp` / `Ctrl+PageDown` — previous / next tab.
- `Ctrl+Shift+W` — close active tab.
- Tab bar drawn at the top (22 px). Active tab is visually highlighted.
- Each tab has its own pane tree, layout, and per-session font metrics.

### Font Size (per-tab, session-only)
- `Ctrl++` or `Ctrl+=` — increase font size by 1 px.
- `Ctrl+-` — decrease font size by 1 px.
- `Ctrl+0` — reset to config default.
- Changes are scoped to the active tab and are not persisted to config.

### Scrollback Navigation
- `Shift+PageUp` / `Shift+PageDown` — scroll half a screen.
- `Ctrl+Shift+Home` / `Ctrl+Shift+End` — jump to top / bottom.

### Scrollback Search
- Enter Search mode from Normal mode with `/`.
- Type a pattern to search; matches update live as you type.
- All matches are highlighted in yellow; the current match is highlighted in orange.
- The status bar shows the query and match count: `/pattern  [2/12]`.
- `Enter` — navigate to the next match (wraps around).
- `Escape` — return to Normal mode; matches remain visible for `n`/`N` navigation.
- `Backspace` — delete the last character of the query.
- `n` (Normal mode) — next match.
- `N` (Normal mode) — previous match.
- The view scrolls automatically to center the current match.
- Search covers the full 10 000-line scrollback buffer and the live screen.
- Match coordinates are stored as `(abs_row, start_col)` where
  `abs_row ∈ [0, scrollback_len)` is a scrollback line and
  `abs_row ∈ [scrollback_len, scrollback_len + grid.rows)` is a live grid row.

### Configuration
- TOML file at `$XDG_CONFIG_HOME/mmterm/config.toml`
  (created with defaults on first run).
- Sections: `[font]`, `[window]`, `[shell]`, `[terminal]`, `[logging]`,
  `[status_bar]`, `[colors]`, `[theme]`.
- In-process TUI config panel: `Ctrl+,` (editable fields, saved on Enter).

| Section | Key | Type | Default |
|---|---|---|---|
| font | family | string | `"Noto Sans Mono"` |
| font | size | float | `16.0` |
| window | width | uint | `800` |
| window | height | uint | `600` |
| window | title | string | `"mmterm"` |
| window | cursor_blink_ms | uint | `500` |
| window | inactive_dim | float | `0.55` |
| window | detect_urls | bool | `true` |
| terminal | scrollback_lines | uint | `10000` (min 100) |
| shell | program | string? | `$SHELL` |
| logging | auto_log | bool | `false` |
| logging | log_dir | string | `""` (→ `~/.mmterm`) |
| status_bar | right | string | `""` |
| theme | name | string | `"default"` |

### Themes

Themes define all terminal and UI colors in a single `.toml` file.

**Built-in themes** (installed to `~/.config/mmterm/themes/` on first launch):
`default`, `catppuccin-mocha`, `dracula`, `gruvbox-dark`, `monokai`, `nord`,
`one-dark`, `solarized-dark`, `tokyo-night`.

**Selecting a theme** — edit `config.toml`:
```toml
[theme]
name = "dracula"
```
Or use the config panel (`Ctrl+,`), navigate to the **Theme** field, and press
`←` / `→` to cycle through available themes with a live preview.

**Creating a custom theme** — place a `.toml` file in
`~/.config/mmterm/themes/`:
```toml
# ~/.config/mmterm/themes/my-theme.toml

foreground  = "#c0c0c0"
background  = "#1c1c1c"

# 16-color ANSI palette (indices 0–15)
color0      = "#1c1c1c"   # black
color1      = "#cc0000"   # red
color2      = "#4e9a06"   # green
color3      = "#c4a000"   # yellow
color4      = "#3465a4"   # blue
color5      = "#75507b"   # magenta
color6      = "#06989a"   # cyan
color7      = "#d3d7cf"   # white
color8      = "#555753"   # bright black
color9      = "#ef2929"   # bright red
color10     = "#8ae234"   # bright green
color11     = "#fce94f"   # bright yellow
color12     = "#729fcf"   # bright blue
color13     = "#ad7fa8"   # bright magenta
color14     = "#34e2e2"   # bright cyan
color15     = "#eeeeec"   # bright white

# UI colors (all optional — derived from the palette if omitted)
cursor          = "#c0c0c0"   # block cursor color
selection       = "#555753"   # visual selection background
search_match    = "#c4a000"   # search highlight background
search_current  = "#cc0000"   # current search match background
scrollbar       = "#555753"   # scrollbar thumb at live view
badge           = "#4e9a06"   # active tab badge
separator       = "#333333"   # pane and bar separator line
```

**Required fields:** `foreground`, `background`, `color0`–`color15`.

**Optional UI fields** — if omitted, defaults are derived from the palette:

| Field | Fallback |
|---|---|
| `cursor` | `color15` (bright white) |
| `selection` | `color0` (black) |
| `search_match` | `color3` (yellow) |
| `search_current` | `color1` (red) |
| `scrollbar` | `color8` (bright black) |
| `badge` | `color2` (green) |
| `separator` | `color0` (black) |

The file name (without `.toml`) becomes the theme name used in `config.toml`
and shown in the config panel selector.

**Note:** built-in theme files are written to disk on first launch and can be
edited as a starting point. mmterm never overwrites user edits to existing
theme files.

### Status Bar

The status bar (22 px, bottom of window) shows:
- **Left** — current input mode badge (`INSERT` / `NORMAL` / `VISUAL` / `SEARCH`).
- **Center** — active pane OSC title (set via `\e]0;title\e\\` or `\e]2;...`);
  suppressed during Search mode (which shows the query and match count instead).
- **Right** — configurable segments via `[status_bar] right` in config.

**Right segment syntax** — a space-separated list of tokens:

| Token | Output |
|---|---|
| `%pwd` | Current working directory (updated via OSC 7 `file://host/path` notifications) |
| `%date{fmt}` | Current date/time formatted with `strftime`-style `fmt` (e.g. `%date{%H:%M}`) |
| Any literal text | Rendered verbatim |

Example:

```toml
[status_bar]
right = "%pwd  %date{%Y-%m-%d %H:%M}"
```

### Session Logging
- `Ctrl+Shift+L` — toggle PTY output capture for the active pane.
- Raw bytes (including ANSI sequences) are written to
  `<log_dir>/mmterm-<unix_timestamp>-pane<id>.log`.
- Default directory is `~/.mmterm`, created automatically on first use.
- Override with `log_dir` in `[logging]`; set `auto_log = true` to start
  logging automatically for every new pane.
- The active pane shows a `● REC` badge in the status bar while recording.
- Log file is closed (and flushed) when logging is toggled off or the pane closes.

### Clipboard
- `Ctrl+Shift+V` — paste from host clipboard (bracketed paste).
- `Ctrl+Shift+C` — copy selection.
- OSC 52 clipboard sync:
  - **Write** (`OSC 52;c;<base64> ST`) — decodes the payload and copies it to
    the host clipboard; enables `pbcopy`/`xclip`-equivalent operation from
    remote SSH sessions (e.g. Neovim `"+y`).
  - **Read** (`OSC 52;c;? ST`) — replies with the current host clipboard content
    encoded as base64, allowing remote apps to paste from the host.

### Debug Logging

`mmterm --debug` activates `DEBUG`-level logging and writes all output to
`~/.mmterm/debug-<timestamp>.log`. The log path is printed to stderr on
startup, and the panic hook prints it again on crash so it is easy to find.

`RUST_LOG=info mmterm` routes `INFO`-and-above log lines to stderr without
writing a file.

### Cursor
- Block cursor (inverted fg/bg) on the active pane.
- Blink driven by wall-clock time (`Instant`), not frame count — rate is
  identical in debug and release builds.
- Blink half-period configurable via `cursor_blink_ms`.
- DECSCUSR (`CSI Ps SP q`): cursor shape control.
  - `0`–`2` → block (blinking/steady)
  - `3`–`4` → underline (blinking/steady)
  - `5`–`6` → beam / bar (blinking/steady)
  - Shape resets to block on alternate screen entry.
  - fish, zsh vi-mode, and Neovim change the cursor shape automatically via
    this sequence.

---

## Key Bindings Reference

### Global (all modes)

| Binding | Action |
|---|---|
| `Ctrl+Q` | Quit (confirmation overlay when multiple tabs/panes are open) |
| `Ctrl+Enter` | Toggle borderless fullscreen |
| `Ctrl+T` | New tab |
| `Ctrl+PageUp` | Previous tab |
| `Ctrl+PageDown` | Next tab |
| `Ctrl+Shift+PageUp` | Move tab left |
| `Ctrl+Shift+PageDown` | Move tab right |
| `Ctrl+Shift+W` | Close tab |
| `Ctrl+Shift+R` | Rename tab |
| `Alt+1`..`Alt+9` | Jump to tab by position (1-indexed) |
| `Ctrl++` / `Ctrl+=` | Increase font size (active tab) |
| `Ctrl+-` | Decrease font size (active tab) |
| `Ctrl+0` | Reset font size (active tab) |
| `Ctrl+,` | Open config panel |
| `Ctrl+Shift+L` | Toggle session logging (active pane) |
| `Ctrl+Shift+V` | Paste from clipboard |
| `Ctrl+Shift+C` | Copy selection |
| `Ctrl+Shift+K` | Clear scrollback |
| `Ctrl+Shift+PgUp/PgDn` | Scroll half screen |
| `Ctrl+Shift+Home/End` | Scroll to top / bottom |
| `Shift+PgUp/PgDn` | Scroll half screen |
| `Ctrl+.` | Cycle mode (Insert → Normal → Visual → Insert) |
| `Ctrl+\` | Enter Normal mode |

### Pane Management (`Ctrl+W` prefix)

| Binding | Action |
|---|---|
| `Ctrl+W v` | Split horizontally (side by side) |
| `Ctrl+W s` | Split vertically (top / bottom) |
| `Ctrl+W h` / `←` | Focus left pane |
| `Ctrl+W l` / `→` | Focus right pane |
| `Ctrl+W k` / `↑` | Focus pane above |
| `Ctrl+W j` / `↓` | Focus pane below |
| `Ctrl+W w` | Cycle focus to next pane |
| `Ctrl+W q` | Close active pane |
| `Ctrl+W z` | Toggle pane zoom (full-window focus) |
| `Ctrl+Shift+←/→` | Resize active pane horizontally (5% step) |
| `Ctrl+Shift+↑/↓` | Resize active pane vertically (5% step) |
| drag separator | Drag the 1 px line between panes to resize |

### Normal Mode

| Key | Action |
|---|---|
| `i` / `Escape` | Return to Insert mode |
| `v` | Enter Visual mode |
| `/` | Enter Search mode |
| `n` | Next search match |
| `N` | Previous search match |
| `q` | Quit |

### Search Mode

| Key | Action |
|---|---|
| _any character_ | Append to search query (live search) |
| `Backspace` | Delete last character |
| `Enter` | Next match |
| `Escape` | Return to Normal mode (matches remain for `n`/`N`) |

### Visual Mode

Visual mode uses a two-phase model: first navigate freely to position the cursor
(no selection is highlighted), then press `v` to place the anchor and extend the
selection by moving the cursor. `k`/↑ at the top row and `j`/↓ at the bottom
row scroll the viewport, making the entire scrollback buffer reachable. Scroll
actions shift the anchor coordinates so the selected content stays stable.

| Key | Action |
|---|---|
| `h/j/k/l` or arrows | Move cursor (scrolls viewport when at boundary) |
| `w` / `b` / `e` | Start of next word / start of prev word / end of word |
| `0` / `$` | Start / end of line |
| `g` / `G` | Top / bottom of viewport |
| `v` | Set selection anchor at cursor (activates highlight) |
| `o` | Swap anchor and cursor (extend from either end) |
| `y` / `Ctrl+C` | Copy selection to clipboard, return to Insert mode |
| `Y` | Yank (copy) the entire line at the cursor, return to Insert mode |
| `q` / `Escape` | Exit to Insert mode |

Word boundary detection (`w`/`b`/`e`) is implemented in `src/motion.rs`.
A character is a word char if it is alphanumeric or `_`; everything else is
punctuation or whitespace.

---

## Design Guidelines

### Adding a New Feature

1. **Keybinding** — Add a variant to `Action` in `keybindings.rs`, return it
   from `handle_key` or `handle_ctrl_w`, then handle it in the `match action`
   block in `main.rs`. An unused `Action` variant is a compile warning.
2. **Config option** — Add the field to the appropriate `*Config` struct with
   `#[serde(default = "fn_name")]`. Update `Default`, the TOML template in
   `save()`, and the TUI panel (`F_*` index constants, `from_config`,
   `build_config`). The `F_*` constants must stay contiguous and in the same
   order as the `fields` vec.
3. **Rendering** — Keep all pixel writes inside `renderer/text.rs`. Pass data
   via `PaneView` or a dedicated `draw_*` call; do not reach into `App` from
   the renderer.
4. **Per-tab vs global state** — Session-only state (e.g. font size) lives in
   `TabState`. Persistent state lives in `Config` and is saved to TOML.

### Dos

- Use `FontMetrics::grid_size_for(w, h)` to compute `(cols, rows)` from a
  pixel rect — never compute them inline.
- Use `grid.blank_cell()` for erase operations — it respects the configured
  background color.
- Use `grid.palette[n]` for ANSI color indices 0–15 — do not hardcode RGB.
- Measure time with `Instant` for anything rate-limited (blink, key repeat).
- Clamp font sizes with `.clamp(6.0, 72.0)` and guard with an epsilon check
  before recomputing metrics.
- Bounds-check every pixel write: `if idx < buf.len()`.

### Don'ts

- Do not use `renderer.font_px` for layout — it is the config default
  reference only. Use `tab.metrics.font_px`.
- Do not mutably borrow `self.renderer` and `self.tabs[i]` in the same
  expression — split with `let idx = self.active_tab` first.
- Do not use frame-count ticks for time-sensitive behaviour — frame rate
  differs between debug and release builds.
- Do not persist session-only state (per-tab font size, scroll offset) to
  the config file.
- Do not `unwrap()` on paths reachable at runtime; use `?`, `if let`, or a
  logged fallback.

### Performance Guidelines

**Benchmarks** — run `cargo bench` before and after any change to the hot paths (`terminal/grid.rs`, `terminal/parser.rs`, `renderer/text.rs`, `pty/session.rs`). Save a baseline first:

```sh
cargo bench -- --save-baseline before
# make change
cargo bench -- --baseline before
```

End-to-end benchmarks (vtebench, termbench, I/O timing) require a running mmterm session — see `bench/run_inside_mmterm.sh`.

**Known hot paths and their constraints:**

| Path | Constraint |
|---|---|
| `Grid::scroll_up` | Called on every line feed; uses `rotate_left` — avoid anything that forces per-cell heap operations |
| `TerminalParser::process` | Called on every PTY read; byte-by-byte `vte::advance` — avoid extra allocations inside the parse loop |
| `Renderer::draw_pane` | Runs every frame; pixel writes must stay O(visible cells) — no scrollback scans |
| `Grid::scan_urls` | O(visible rows × cols); only call when rows are dirty — never unconditionally on every PTY chunk |

**Don't measure performance in debug builds.** `opt-level=0` makes tight loops 5–10× slower than release; always use `cargo build --release` or `cargo bench` for performance evaluation.

### Code Conventions

- Modules are flat files; sub-directories only when a module has ≥ 2 files.
- Public API is minimal — prefer `pub(crate)` or private where possible.
- Logging via `log::info!` / `log::warn!` — activated with `RUST_LOG=info`.
- No comments on obvious code; one-line comments only for non-obvious
  invariants or workarounds (the *why*, never the *what*).
- All code must be formatted with `cargo fmt` before committing. Never commit
  manually-aligned or otherwise unformatted Rust code.

---

## Dependency Rationale

| Crate | Why |
|---|---|
| `winit 0.30` | Cross-platform window + event loop (X11/Wayland/Cocoa) |
| `softbuffer 0.4` | Minimal CPU pixel buffer tied to a winit window |
| `fontdue 0.9` | Pure-Rust glyph rasterization with exact metrics |
| `font-kit 0.14` | System font discovery (fontconfig / CoreText) |
| `vte 0.13` | Correct ANSI/VT parser (same as Alacritty) |
| `portable-pty 0.8` | Cross-platform PTY abstraction |
| `crossbeam-channel` | MPSC channel between PTY thread and render loop |
| `arboard 3` | Clipboard read/write for bracketed paste |
| `toml + serde` | Config file serialization |
| `dirs-next 2` | XDG / platform config directory lookup |
| `anyhow 1` | Error propagation in startup paths |
| `log + env_logger` | Structured logging gated by `RUST_LOG` |
