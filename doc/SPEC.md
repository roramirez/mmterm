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
│  └── InputMode (Insert / Normal / Visual)            │
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
- 10 000-line scrollback buffer per pane.
- 16-color ANSI palette fully configurable per profile.
- True foreground/background/cursor/selection colors.
- Bracketed paste mode (`Ctrl+Shift+V`).

### Rendering
- CPU-only pixel buffer (no GPU, no OpenGL, no Vulkan).
- Glyph rasterization via `fontdue`; system font discovery via `font-kit`
  (fontconfig on Linux, CoreText on macOS).
- Bundled fallback font (JetBrains Mono Regular/Bold) for zero-config startup.
- Per-character bold rendering using a separate bold font face.
- Correct advance-width cell sizing: `cell_width = M.advance_width.ceil()`.
- Baseline alignment per glyph using fontdue `ymin` metric.

### Input
- Three modal modes: **Insert** (default), **Normal**, **Visual**.
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
- `Ctrl+Shift+↑` / `Ctrl+Shift+↓` — scroll one line.
- `Ctrl+Shift+Home` / `Ctrl+Shift+End` — jump to top / bottom.

### Configuration
- TOML file at `$XDG_CONFIG_HOME/mmterm/config.toml`
  (created with defaults on first run).
- Sections: `[font]`, `[window]`, `[shell]`, `[colors]`.
- In-process TUI config panel: `Ctrl+,` (editable fields, saved on Enter).

| Section | Key | Type | Default |
|---|---|---|---|
| font | family | string | `"Noto Sans Mono"` |
| font | size | float | `16.0` |
| window | width | uint | `800` |
| window | height | uint | `600` |
| window | title | string | `"mmterm"` |
| window | cursor_blink_ms | uint | `500` |
| shell | program | string? | `$SHELL` |
| colors | background | #RRGGBB | `#121212` |
| colors | foreground | #RRGGBB | `#a0a0a0` |
| colors | cursor | #RRGGBB | `#bbbbbb` |
| colors | selection | #RRGGBB | `#3d3d3d` |
| colors | palette | [#RRGGBB ×16] | Monokai/Hardcore |

### Cursor
- Block cursor (inverted fg/bg) on the active pane.
- Blink driven by wall-clock time (`Instant`), not frame count — rate is
  identical in debug and release builds.
- Blink half-period configurable via `cursor_blink_ms`.

---

## Key Bindings Reference

### Global (all modes)

| Binding | Action |
|---|---|
| `Ctrl+Q` | Quit |
| `Ctrl+T` | New tab |
| `Ctrl+PageUp` | Previous tab |
| `Ctrl+PageDown` | Next tab |
| `Ctrl+Shift+W` | Close tab |
| `Ctrl++` / `Ctrl+=` | Increase font size (active tab) |
| `Ctrl+-` | Decrease font size (active tab) |
| `Ctrl+0` | Reset font size (active tab) |
| `Ctrl+,` | Open config panel |
| `Ctrl+Shift+V` | Paste from clipboard |
| `Ctrl+Shift+↑/↓` | Scroll one line |
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

### Normal Mode

| Key | Action |
|---|---|
| `i` / `Escape` | Return to Insert mode |
| `v` | Enter Visual mode |
| `q` | Quit |

### Visual Mode

| Key | Action |
|---|---|
| `h/j/k/l` or arrows | Move selection cursor |
| `0` / `$` | Start / end of line |
| `g` / `G` | Top / bottom of screen |
| `v` / `q` / `Escape` | Exit to Insert mode |

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

### Code Conventions

- Modules are flat files; sub-directories only when a module has ≥ 2 files.
- Public API is minimal — prefer `pub(crate)` or private where possible.
- Logging via `log::info!` / `log::warn!` — activated with `RUST_LOG=info`.
- No comments on obvious code; one-line comments only for non-obvious
  invariants or workarounds (the *why*, never the *what*).

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
