# Changelog

All notable changes to mmterm are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed
- reverse video (`\e[7m`) was invisible: `write_char` pre-swapped fg/bg and the renderer swapped again, cancelling the effect; now only the renderer swaps based on `cell.reverse`

### Added
- SGR overline (`\e[53m` / `\e[55m`): parse, store on cells, and render as a 1px line at the top of the cell
- OSC 52 clipboard sync: write (`OSC 52;c;<base64> ST`) copies decoded text to the host clipboard; read (`OSC 52;c;? ST`) replies with current clipboard content encoded as base64 — enables copy/paste from remote SSH sessions
- DECSCUSR (`CSI Ps SP q`): cursor shape control — block (0–2), underline (3–4), beam (5–6); shape resets to block on alternate screen entry; fish, zsh vi-mode, and Neovim now change the cursor shape automatically
- 4 px inner padding on all pane edges so text no longer touches the border
- DSR (`CSI 6 n`) and DA (`CSI c`) query-response: terminal now replies with cursor position and VT100 device attributes, fixing hangs and layout errors in vim, less, and other TUI apps that probe the terminal on startup
- DECSC/DECRC (`ESC 7` / `ESC 8`) now save and restore SGR attributes (colors, bold, dim, underline, reverse, blink, strikethrough) in addition to cursor position
- SGR italic (`\e[3m` / `\e[23m`): parse, store on cells, and render using italic font variant; bundles JetBrainsMono Italic and Bold Italic as fallback when the system font has no italic
- visual mode: vim-style scrollback selection — navigate freely with `hjkl`/arrows (scrolls at boundaries), press `v` to set the anchor, extend with `w`/`b`/`e` word motions, then `y`/`Ctrl+C` to copy; `Y` yanks the current line, `o` swaps anchor and cursor
- configurable status bar right segments via `[status_bar] right` in config; supports `%pwd` (OSC 7 cwd) and `%date{fmt}` (strftime) tokens
- `Ctrl+Q` shows a confirmation overlay when multiple tabs or panes are open; single-pane sessions exit immediately
- `Alt+1`..`Alt+9` jump directly to a tab by position (1-indexed); intercepted globally so Insert mode does not forward the sequence to the PTY
- active pane OSC title shown centered in the status bar; suppressed during search mode
- `src/search.rs` and `src/geometry.rs` extract pure functions from `App` for testability; covered by 21 new unit tests
- 23 additional tests across `renderer/glyph`, `renderer/text`, `input/keybindings`, and `tui_config` raising coverage from 63 % to ~65 %
- `src/tabs.rs` and `src/mouse.rs` extract pure functions from `App` (tab index arithmetic and mouse event encoding); covered by 25 new unit tests
- `src/statusbar.rs`, `src/font.rs` extract status bar rendering and font-size clamping; `extract_match_text` and `cell_url_at_scroll` added to `search.rs`/`geometry.rs`; covered by 27 new unit tests
- `Ctrl+Enter` toggles borderless fullscreen (all modes); inspired by Ghostty's Linux/Windows shortcut
- Dependabot configuration for automated Cargo and GitHub Actions dependency updates
- `--debug` flag: writes `DEBUG`-level logs to `~/.mmterm/debug-<timestamp>.log`; panic hook prints the log path on crash

### Fixed
- Shift+Tab now sends the correct backtab sequence (`\x1b[Z`) to the PTY instead of a plain tab byte
- OSC 8 hyperlinks always show a dim blue underline; underline brightens on hover
- config panel navigation and edits now redraw immediately instead of waiting for the cursor-blink timer

## [0.2.0] - 2026-05-14

### Added
- configurable scrollback buffer size via `scrollback_lines` in `[terminal]` config section (default 10 000, minimum 100)
- headless renderer tests covering `FontMetrics`, `color_u32`, `dim_color`, `get_cell`, `mode_style`, and all draw paths
- glyph cache tests covering cache hit, fallback rendering, and bilinear scaling
- PTY session tests for spawn, write, and resize
- additional unit tests for config defaults, theme listing, tui_config Select cycling, and keybinding alt-modifier encoding
- renderer tests covering visual selection, search match highlighting, inactive pane dimming, underline, strikethrough, dim, reverse-video, cursor, and scrollbar rendering paths
- glyph cache tests for embedded fallback font (regular and bold) and fallback chain for unknown font families
- PTY session tests for spawn-with-cwd, default SHELL spawn, and cwd() readback

- theme system: 9 built-in themes (default, catppuccin-mocha, dracula, gruvbox-dark, monokai, nord, one-dark, solarized-dark, tokyo-night) installed to `~/.config/mmterm/themes/` on first launch
- theme selector in the config panel (← / → to cycle with live preview)
- `[theme] name` field in `config.toml`; custom themes can be added as `.toml` files in `~/.config/mmterm/themes/`
- tab bar, status bar, scrollbar, search highlights, and pane separators now use theme colors

### Performance

- cap PTY bytes parsed per frame to 256 KB and coalesce wakeup events so high-throughput commands (e.g. `find .`) render progressively in both normal and maximized windows

### Fixed

- copy selection now reads the correct content when scrolled into scrollback
- cursor is now always visible in Insert mode regardless of `?25l` sent by TUI apps (Ink, Claude Code); apps that hide the terminal cursor during rendering no longer leave it permanently invisible
- `cursor_visible` state is now saved and restored when entering/exiting the alternate screen
- OSC 8 hyperlink underline now only renders when the mouse hovers over the link, not on every cell with a URL
- default log directory is now `~/.mmterm` (created automatically) instead of `$HOME`
- glyph antialiasing now blends in linear light (gamma-2 approximation) instead of sRGB space, producing sharper text
- inactive split panes no longer shift the background color: only foreground text is dimmed; gutter pixels now correctly match the pane background

### Documentation

- add animated demo GIF to README showcasing split panes, tabs, tab rename, search, zoom, and 256-color output
- document session logging, `inactive_dim`, `detect_urls`, `Ctrl+Shift+L`, and `Ctrl+W z` in README and SPEC


## [0.1.0] - 2026-05-09

### Added

#### Terminal emulation
- SGR blink attribute (codes 5/25)
- SGR text attributes: dim, underline, strikethrough, reverse video
- DCH, ICH, ECH character editing sequences
- CHA and VPA cursor positioning sequences
- DECTCEM (cursor visibility), alternate screen, and bracketed paste
- DECSC/DECRC save/restore cursor, SGR reverse video, tab stops
- BCE — erase sequences respect current SGR background color
- Scroll down, IL/DL insert/delete lines, ESC M reverse index
- F5–F12 function key support
- DECCKM mode for readline / history-substring-search compatibility
- OSC 0/2 window title reflected in tab bar
- OSC 7 current working directory shown in status bar
- OSC 8 clickable hyperlinks with blue underline and pointer cursor
- Visual bell flash on BEL (0x07)
- Mouse reporting and selection rendering
- Plain-text URL auto-detection and click-to-open

#### Search
- Scrollback search with live match highlighting (`/` in Normal mode)
- Regex search support
- Copy matched text to clipboard
- Clear scrollback action

#### UI / panes
- Split-pane support with binary-tree layout (horizontal and vertical)
- Vim-style pane focus navigation (`Ctrl+W h/j/k/l`)
- Pane zoom — toggle full-window focus (`Ctrl+W z`)
- Scrollbar and improved keyboard scroll bindings
- Configurable inactive pane dimming
- Tab activity dot indicator

#### Tabs
- Multi-tab support with tab bar (`Ctrl+T`, `Ctrl+PageUp/Down`)
- Tab renaming with inline edit in tab bar
- Tab reordering with `Ctrl+Shift+PageUp/Down`
- New tabs and split panes inherit the current working directory

#### Fonts and rendering
- Color emoji support via FreeType CBDT/CBLC
- Wide-character and glyph fallback font support
- Per-tab font size (`Ctrl++` / `Ctrl+-` / `Ctrl+0`)
- Cursor blink resets on keypress

#### Configuration
- System font loading with bundled JetBrains Mono fallback
- Full 16-color ANSI palette theming
- TUI config panel (`Ctrl+,`) — edit settings in-process
- Configurable cursor blink interval
- Session logging: `Ctrl+Shift+L` toggles PTY output capture per-pane; active pane shows a `● REC` badge in the status bar; configurable via `[logging]` section (`auto_log`, `log_dir`)

#### Input
- Modal input: Insert, Normal, Visual, Search modes (vim-style)
- Mouse selection and copy/paste support
- `Ctrl+C` copy in Visual mode
- `Ctrl+Shift+C` / `Ctrl+Shift+V` clipboard bindings

#### Infrastructure
- GitHub Actions CI (fmt, clippy -D warnings, test)
- Release workflow: multi-platform binaries (Linux x86_64/aarch64, macOS x86_64/aarch64), tar.gz archives, SHA256 checksums
- Desktop install script
- App icon with MM branding

### Fixed

- Tab keystroke from Alt+Tab window switching no longer leaks into the PTY; modifier state is also reset when mmterm loses focus
- plain-text URL detection no longer includes trailing `)`, `.`, `,`, and similar punctuation in the link
- Alt modifier propagation to prevent bare tab on `Alt+Tab`
- `ZoomPane` dispatch missing in normal key handler
- `q` in Normal mode closed the app instead of the pane
- Auto-close pane when shell exits via `exit` or `Ctrl+D`
- Home/End keys now send standard sequences for readline compatibility
- Font metrics for correct cell sizing and glyph placement
- `Escape` passes through to PTY; `Ctrl+.` cycles input mode
- Tab bar spacing corrupted after color emoji rendering
- CPU busy-loop in event loop causing unnecessary load
- Redraw not requested immediately on tab switch and `Ctrl+W` actions

### Performance

- Fixed CPU busy-loop in PTY event loop

[Unreleased]: https://github.com/roramirez/mmterm/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/roramirez/mmterm/releases/tag/v0.1.0
