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
- DEC Special Graphics character set (`ESC ( 0` / `ESC ( B`): maps `j`–`x` and
  surrounding chars to Unicode box-drawing glyphs (`┘┐┌└┼─│├┤┴┬` etc.);
  required by ncurses apps (`dialog`, `nmtui`, `mutt`, `alpine`).
- RIS (`ESC c`): full terminal reset — clears screen and scrollback, resets
  cursor, SGR, scroll region, alternate screen, and all mode flags to their
  initial state.
- Focus reporting (`?1004h/l`): sends `\e[I` on focus-in and `\e[O` on focus-out
  to the active pane; fires on OS window focus events, tab switches, and pane
  switches — neovim uses this to trigger `autoread` and `FocusGained` hooks.
- DECAWM (`?7h/l`): autowrap mode toggle; when disabled, characters written at
  the right margin overwrite the last cell and the cursor stays at the margin
  instead of advancing to the next line.

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

### Command Palette
- `Ctrl+Shift+P` — open the command palette overlay (works in all modes).
- Type to filter the list of all available actions by label or internal code (case-insensitive substring).
- `↑` / `↓` — navigate between matched entries.
- `Enter` — execute the selected action and close the palette.
- `Esc` — close without executing; returns to Insert mode.
- Each row shows the human-readable label on the left and the keyboard shortcut on the right.
- The filter resets the selection to the first entry on every keystroke.
- Rendered as a 62%-wide centered overlay with a dimmed background; disappears as soon as an action runs.
- Escape is always forwarded to the PTY — vim, less, etc. work as expected.
- Full function-key, arrow-key, and special-key forwarding in Insert mode.
- Ctrl+character encoding (Ctrl+A = 0x01 … Ctrl+Z = 0x1A).

### Split Panes
- Binary-tree layout: `Node::Leaf(id)` or `Node::Split { dir, ratio, a, b }`.
- Horizontal split: `Ctrl+W v` / Vertical split: `Ctrl+W s`.
- Auto-split: `Ctrl+W a` — splits along the longest dimension (H if wider, V if taller).
- Focus navigation: `Ctrl+W h/j/k/l` or arrow keys.
- Cycle focus: `Ctrl+W w`.
- Close pane: `Ctrl+W q` (closes tab when last pane).
- 50/50 initial split ratio; separator is 1 px wide.
- Resize by dragging the separator (4 px click margin); cursor changes to `ColResize`/`RowResize` on hover.
- Keyboard resize: `Ctrl+Shift+←/→` (horizontal) and `Ctrl+Shift+↑/↓` (vertical) move the nearest separator by 5% per keypress.
- Minimum pane size is 10% of the parent region (ratio clamped to 0.1–0.9).

### Screenshot Mode
Screenshot capture is a two-step flow: region selection followed by a name prompt.

**Step 1 — Region selector (`InputMode::Screenshot`)**
- Entered with `Ctrl+W p`; exits to Insert mode on `Esc`.
- `InputMode::Screenshot { cx, cy, half_w, half_h }` — rectangle centered at `(cx, cy)` with independent half-extents.
- Arrow keys translate the selection 20 px in the given direction.
- `Shift+→`/`Shift+←` grow or shrink only the right edge (left edge stays fixed); `Shift+↓`/`Shift+↑` grow or shrink only the bottom edge (top edge stays fixed); each step is 20 px.
- `Enter`/`Space` → transitions to `InputMode::ScreenshotName` (name prompt); capture is taken from the raw pixel buffer **before** overlays are drawn (no selector border in output).
- Overlay: pixels outside the rectangle are darkened (60 % veil); a 2-px white border frames the selection; a hint line shows key bindings.
- Status bar badge: `SHOT` (yellow) while the mode is active.

**Step 2 — Name prompt (`InputMode::ScreenshotName`)**
- `InputMode::ScreenshotName { cx, cy, half_w, half_h, name: String }` — carries the region from step 1 plus the name being typed.
- The selection is kept visible (dimmed veil + border). A centered input box near the bottom shows `Name: <typed>_`.
- Hint below the box: `Enter save  (empty = mmterm-<timestamp>.png)   Esc cancel`.
- Typing characters appends to `name`; `Backspace` removes the last character.
- `Enter` — saves the PNG and exits to Insert mode:
  - Non-empty input: filename is `<sanitized-name>.png` (path separators, colons, spaces → `-`).
  - Empty input: filename falls back to `mmterm-YYYYMMDDTHHMMSS.png`.
  - Saved to `config.general.screenshot_dir` (default `~/mmterm/shot`); `~` is expanded to `$HOME`. The absolute path is copied to the clipboard after a successful save.
- `Esc` — cancels without saving; returns to Insert mode.

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

### Search History
- Stored in `~/.config/mmterm/search_history` (plain text, zsh `EXTENDED_HISTORY` format).
- Each line: `: <unix_timestamp>:0;<query>` — compatible with standard history tools.
- `↑` in Search mode loads the previous query (most recent first); current draft saved in `AppState.search_before_history`.
- `↓` advances toward the newest entry; past the last entry restores the draft.
- Typing or `Backspace` while browsing history exits navigation (`history_pos` reset to `None`).
- History saved atomically (`.tmp` → rename) on `Escape` or `Enter` when query is non-empty.
- Deduplication: repeated queries move to the end (most recent wins).
- Cap: 50 entries; oldest discarded when limit is exceeded.
- `InputMode::Search { history_pos: Option<(usize, usize)> }` — `Some((idx, len))` while browsing.
- Status bar shows `[hist N/M]` alongside the match count while navigating history.
- `AppState.search_history: Vec<String>` populated from disk at startup via `history::load_search_history()`.

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
- **Left** — current input mode badge (`INSERT` / `INSERT PASS` / `NORMAL` / `VISUAL` / `SEARCH`); `INSERT PASS` indicates passthrough mode is active.
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

### Session Persistence

- On quit (`Ctrl+Q` or window close), if `general.restore_session = true`, a
  centered dialog is shown over the dimmed terminal:

  ```
  Save session before quitting?
  [s] Save and quit   [q] Quit   [Esc] Cancel
  ```

- `s` / `S` — saves the session file and exits.
- `q` / `Q` / `n` / `Enter` — exits without saving.
- `Esc` — cancels the quit and returns to normal input.

#### Named scopes

`--scope <name>` (also `--scope=<name>` / `-s <name>`) routes save/load to a
named workspace file, keeping it fully isolated from other scopes:

| Invocation | Session file |
|---|---|
| `mmterm` | `~/.config/mmterm/session.toml` (default) |
| `mmterm --scope work` | `~/.config/mmterm/sessions/work.toml` |
| `mmterm -s personal` | `~/.config/mmterm/sessions/personal.toml` |

`--list-scopes` prints all saved scope names (sorted) and exits without
launching the terminal.

#### What is saved

| Field | Description |
|---|---|
| `active_tab` | Index of the focused tab |
| Per tab: `name` | Tab name set by the user (if any) |
| Per tab: `active_pane` | DFS-order index of the focused pane |
| Per tab: `pane_cwds` | Working directory of each pane (via `/proc/<pid>/cwd`) |
| Per tab: `layout` | Full binary split tree with `dir` (`H`/`V`) and `ratio` per node |

#### What is NOT saved

- PTY content, scrollback, or cursor state — each pane opens a fresh shell.
- Per-tab font size, zoom, scroll offset — session-only state.

#### Restore

On launch, if the session file exists and `restore_session = true`, each saved
tab is recreated: panes are spawned in DFS leaf order with their saved CWDs,
and the split tree is reconstructed with the original ratios.

- CWD no longer exists → falls back to `$HOME`.
- File missing or corrupt → silently falls back to a blank tab.

#### Config (`[general]`)

```toml
[general]
restore_session = true   # set to false to disable the save dialog and always quit immediately
```

Toggling via the TUI config panel (`Ctrl+,`) under the **General** section.

### Visual Bell

BEL (`0x07`) received from the PTY triggers a brief visual indicator.

**Behaviour:**
- A yellow `●` dot (U+25CF, `theme.palette[3]`) is rendered next to the mode badge in the status bar for 150 ms with a quadratic ease-out fade (`intensity = 1 - t²`).
- A 500 ms cooldown starts when the flash fires; any BEL received while the cooldown is active is silently dropped. This prevents tab-completion spam from re-triggering the indicator.
- Optionally, `visual_bell = true` also blends the theme foreground color over the content area at up to ~22 % opacity for the same 150 ms duration.

**Implementation fields (per `TabState`):**
- `bell_flash_start: Option<Instant>` — set when bell fires; drives intensity calculation.
- `bell_flash_until: Option<Instant>` — expiry used by `next_bell_wakeup` to schedule the next frame wakeup.
- `bell_cooldown_until: Option<Instant>` — guards against repeated firings.

**Config (`[general]`):**

```toml
[general]
visual_bell = false   # set to true to also flash the screen background
```

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

### Configurable Keymap

mmterm's modifier shortcuts are a single data-driven table (`src/input/keymap.rs`).
`default_keymap()` defines all built-in Global-scope shortcuts as data; the user's
`[keybindings]` config overlays the same table.

**Config format**

```toml
[keybindings]
"cmd+v"     = "paste"      # override / add a binding
"cmd+k"     = "none"       # disable a built-in default
"ctrl+e"    = "new_tab"    # add a new shortcut
"ctrl+w x"  = "close_pane" # chord (space-separated tail key)
```

**Grammar**
- Modifiers (order-insensitive, `+`-joined): `ctrl` `shift` `alt` `cmd`.
  `cmd` resolves to Command (⌘) on macOS and Super on Linux/Windows.
- Key token: a single character (`v` `,` `+` `=`) or a named key
  (`enter` `escape` `tab` `space` `backspace` `delete` `pageup` `pagedown`
  `home` `end` `arrowup/down/left/right` `f1`..`f12`); letters are case-insensitive.
- Chord: a space-separated tail key after a prefix (today only `ctrl+w`).
- `"none"` disables the matching built-in (returns the key to its raw terminal meaning).

**Merge semantics**
- Defaults always load; a valid entry inserts/replaces; `"none"` removes; an empty
  table = full defaults.
- Invalid entries are skipped, `log::warn!`-ed, counted, and surfaced as a transient
  status-bar notice `"N keybindings invalid — see log"`. The app always starts.
- Validation rejects: unparseable bindings, unknown action names, and **bare
  unmodified-character** bindings in Global scope (they would shadow literal typing).

**Bindable action names**

| Name | Action |
|---|---|
| `paste`, `copy` | clipboard |
| `new_tab`, `close_tab`, `next_tab`, `prev_tab`, `move_tab_left`, `move_tab_right`, `rename_tab` | tabs |
| `go_to_tab_1`..`go_to_tab_9` | select tab N |
| `split_horizontal`, `split_vertical`, `auto_split`, `close_pane` | splits |
| `focus_left/right/up/down`, `focus_next` | pane focus |
| `zoom_pane`, `rotate_panes_forward`, `rotate_panes_backward` | pane layout |
| `resize_pane_left/right/up/down` | pane resize |
| `scroll_page_up`, `scroll_page_down`, `scroll_to_top`, `scroll_to_bottom`, `clear_scrollback` | scroll |
| `search_open`, `search_next`, `search_prev` | search |
| `increase_font_size`, `decrease_font_size`, `reset_font_size` | font |
| `open_config`, `open_command_palette`, `toggle_fullscreen`, `toggle_log`, `toggle_passthrough`, `screenshot_open`, `quit` | app / ui |
| `cycle_mode`, `enter_normal_mode` | mode |
| `ctrl_w_prefix` | start a `Ctrl+W` chord |

Modal (`normal:` / `visual:`) remapping is a future phase; literal text and cursor/PTY
encoding are never bindable — they are the fallback when no binding matches.

### Global (all modes)

| Binding | Action |
|---|---|
| `Ctrl+Q` | Quit — shows save-session dialog when `restore_session = true`; otherwise confirmation overlay when multiple tabs/panes are open |
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
| `Ctrl+Shift+P` | Open command palette |
| `Ctrl+Shift+L` | Toggle session logging (active pane) |
| `Ctrl+Shift+V` | Paste from clipboard |
| `Ctrl+Shift+C` | Copy selection |
| `Ctrl+Shift+K` | Clear scrollback |
| `Ctrl+Shift+PgUp/PgDn` | Scroll half screen |
| `Ctrl+Shift+Home/End` | Scroll to top / bottom |
| `Shift+PgUp/PgDn` | Scroll half screen |
| `Ctrl+.` | Cycle mode (Insert → Normal → Visual → Insert) |
| `Ctrl+\` | Enter Normal mode |
| `Ctrl+B` | Toggle passthrough mode (see below) |

### macOS Command (⌘) / Super

The platform-standard ⌘ shortcuts are routed when the Super modifier is held
(macOS Command; Linux/Windows Super key). They take priority over mode dispatch;
while Super is held an unmapped key is swallowed (never sent to the PTY). Inactive
in passthrough mode.

| Binding | Action |
|---|---|
| `⌘V` | Paste |
| `⌘C` | Copy selection (Visual mode) |
| `⌘N` / `⌘T` | New tab |
| `⌘W` | Close tab |
| `⌘1`..`⌘9` | Jump to tab by position |
| `⌘Q` | Quit |
| `⌘,` | Open config panel |
| `⌘F` | Open scrollback search |
| `⌘K` | Clear scrollback |
| `⌘+` | Increase font size |
| `⌘-` | Decrease font size |
| `⌘=` / `⌘0` | Reset font size |

### Pane Management (`Ctrl+W` prefix)

| Binding | Action |
|---|---|
| `Ctrl+W v` | Split horizontally (side by side) |
| `Ctrl+W s` | Split vertically (top / bottom) |
| `Ctrl+W a` | Auto-split along longest dimension |
| `Ctrl+W h` / `←` | Focus left pane |
| `Ctrl+W l` / `→` | Focus right pane |
| `Ctrl+W k` / `↑` | Focus pane above |
| `Ctrl+W j` / `↓` | Focus pane below |
| `Ctrl+W w` | Cycle focus to next pane |
| `Ctrl+W q` | Close active pane |
| `Ctrl+W z` | Toggle pane zoom (full-window focus) |
| `Ctrl+W p` | Enter screenshot mode |
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

### Passthrough Mode

Passthrough mode suspends all mmterm keybindings and forwards every keystroke
directly to the PTY. It solves the conflict where mmterm intercepts shortcuts
(e.g. `Ctrl+W`) before they reach a program running inside the terminal (vim,
tmux, another terminal multiplexer).

| Key | Action |
|---|---|
| `Ctrl+B` | Toggle passthrough on / off |

**Behaviour:**
- Active only in `Insert` mode (the mode where keystrokes normally go to the PTY anyway).
- While active the status bar badge changes from `INSERT` to `INSERT PASS`.
- `Ctrl+B` is the only key mmterm intercepts while passthrough is active; all other keys, including `Ctrl+W` chords, are forwarded as raw bytes.
- Passthrough state is per-tab and session-only — it resets to off on tab close or restart.
- The flag lives on `TabState.passthrough` and is never persisted to config or session files.

**Implementation:** `handle_keyboard_input` in `src/app_event.rs` checks
`tab.passthrough` before the normal dispatch path. If active, `Ctrl+B` clears
the flag; all other pressed events are encoded by `handle_key_passthrough`
(Insert-mode encoding, bypassing global shortcuts) and sent to the PTY.

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
