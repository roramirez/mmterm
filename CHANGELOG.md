# Changelog

All notable changes to mmterm are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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
