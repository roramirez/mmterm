# Changelog

All notable changes to mmterm are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed
- visual mode selection spanning multiple pages now copies all selected lines; previously `start_row` was clamped to the viewport height, so only the last page of a multi-page selection was copied

### Changed
- refactor grid.rs: add HTTPS_PREFIX_LEN/HTTP_PREFIX_LEN constants; replace remaining `rows-1`/`cols-1` literals with max_row()/max_col(); refactor clear_line() to use slice fill
- refactor app_state.rs: extract swap_visual_anchor() and set_visual_anchor() helpers; flatten dispatch_visual_action() nesting from 5 to 2 levels
- refactor renderer/text.rs: extract draw_status_badge() to eliminate duplicated fill_rect+draw_badge_label pattern in draw_status_bar()
- refactor layout.rs: extract `split_dimension()` helper and `RATIO_MIN`/`RATIO_MAX` constants; eliminate 6 repeated split-dimension calculations in `compute_rects`, `separators`, `find_sep_at_pixel`
- refactor grid.rs: remove duplicated scrollback-max cap in `scroll_up()` by calling `push_scrollback()`
- refactor app_event.rs: merge duplicate `if let Some(entry)` blocks in `viewport_scroll()`
- refactor keybindings.rs: extract `visual_mode_init()` to deduplicate repeated `InputMode::Visual` construction
- refactor grid.rs: extract `cell_with_colors`, `max_row`/`max_col`, `reposition_cursor_after_reflow`; replace duplicate SGR reset blocks with `reset_sgr()` calls to reduce Halstead effort
- refactor parser.rs: extract `param_or_one` helper; use `grid.max_row()`/`max_col()` in `csi_dispatch` and scroll-region handling
- refactor app_event.rs: eliminate double `command_palette::filter` lookup; simplify screenshot name mode reconstruction
- refactor draw_fns.rs: simplify `get_cell` fallback paths from 4 returns to 2
- refactor: extract `collapse_indicator`, `config_panel_hint`, `panel_font_metrics`, and `draw_config_section_header` helpers in `renderer/overlays.rs` to reduce cognitive complexity (D+ → B)
- refactor: unify `jump_section_forward`/`jump_section_backward` into a shared private `jump_section` method in `tui_config.rs`
- refactor: move CLI argument-parsing functions to `src/cli.rs`; extract `session_path()` method and `bell_flash_intensity()` free function from `main.rs`
- copy screenshot file path to clipboard after a successful capture
- config panel: palette section collapsed by default; `Space` on a section header toggles collapse; `]`/`[` and `Tab`/`Shift+Tab` jump between sections
- page up / page down now scroll the viewport in visual mode, extending the selection

### Added
- screenshot name prompt: after selecting a region, a text input asks for a filename; Enter saves as `<name>.png`, empty input falls back to `mmterm-<timestamp>.png`, Esc cancels
- `--scope <name>` flag to isolate session storage per named workspace (`~/.config/mmterm/sessions/<name>.toml`); also accepts `--scope=<name>` and `-s <name>`
- `--list-scopes` flag to print all saved scope names and exit without launching the terminal
- visual bell: BEL (0x07) triggers a `●` indicator next to the mode badge in the status bar (150 ms, yellow); a 500 ms cooldown suppresses repeated bells (e.g. tab-completion spam); optional screen flash can be enabled with `visual_bell = true` in `[general]` (default `false`)
- test coverage for `app_state`, `command_palette`, and `renderer/overlays` modules: all dispatch_action arms, nudge_half, screenshot mode, rotate panes, search cycling, command palette entry construction, and overlay draw functions
- test coverage for `views`, `drain`, and `session` modules: pane view collection, tab title building, PTY byte draining, bell flash, clipboard handling, and session I/O round-trips
- reflow scrollback on resize: soft-wrapped lines re-wrap to the new column width when the terminal is resized; scroll position is preserved
- pane rotation: `Ctrl-W r` rotates panes forward (last → first slot) and `Ctrl-W R` rotates backward, like tmux
- screenshot mode (`Ctrl+W p`): interactive square region selector; arrow keys resize the square, `Shift+Arrow` moves it; `Enter` captures and saves a PNG to the configured directory (default `~/mmterm/shot`); configurable via `[general] screenshot_dir`
- passthrough mode (`Ctrl+B`): forwards all keystrokes directly to the PTY, bypassing mmterm shortcuts; status bar shows `INSERT PASS`; press `Ctrl+B` again to exit

## [0.4.1] - 2026-05-27

### Fixed
- panic (index out of bounds) when closing a full-screen app (e.g. vim `:q!`) after resizing the terminal while the app was open; `exit_alternate_screen` now refits the saved main-screen cell buffer and clamps cursor/scroll-region to current dimensions

## [0.4.0] - 2026-05-26

### Added
- `--version` / `-V` flag: prints `mmterm <version>` and exits; local builds include the git short hash (`0.4.0+abc1234`)
- `--help` / `-h` flag: prints usage and all supported options, then exits
- session persistence: tabs, splits, and pane CWDs are saved on quit and restored on next launch; quitting shows a prompt `[s] save and quit  [q] quit  [Esc] cancel`; configurable via `[general] restore_session`
- command palette (`Ctrl+Shift+P`): overlay to fuzzy-filter and execute any action by name; navigate with ↑/↓, confirm with Enter, dismiss with Esc
- DCS sequence dispatch: `hook()`, `put()`, and `unhook()` now route sixel graphics (`DCS...q...ST`) to a self-contained decoder; unknown DCS sequences are silently discarded
- sixel graphics: images decoded into RGBA pixel buffers and blitted as a post-pass overlay in the renderer, anchored to the cursor position at emission time; palette define (`#n;2;r;g;b`, `#n;1;h;l;s`), RLE (`!count<byte>`), carriage-return (`$`), and band-linefeed (`-`) are all supported
- RIS (`ESC c`): full terminal reset — clears screen, scrollback, SGR, scroll region, cursor, and all mode flags
- focus reporting (`?1004h/l`): send `\e[I` on focus-in and `\e[O` on focus-out; covers OS window focus, tab switches, and pane switches
- autowrap mode toggle (`?7h/l`, DECAWM): when disabled, characters at the right margin overwrite the last cell instead of wrapping to the next line
- DEC Special Graphics character set (`ESC ( 0` / `ESC ( B`): box-drawing characters for ncurses apps (`dialog`, `nmtui`, `mutt`, etc.)
- auto-split pane with `Ctrl+W a`: splits along the longest dimension (horizontal if wider, vertical if taller)
- resizable pane splits by dragging the separator line between panes; cursor changes to a resize icon on hover
- keyboard pane resizing with `Ctrl+Shift+Arrow` keys (Right/Left grow/shrink horizontally, Down/Up grow/shrink vertically); minimum pane size is 10% of the parent region
- `AppState` struct extracts all action-dispatch logic from `App` into a pure, winit-free type; `dispatch_action` returns `Vec<AppEffect>` allowing full unit-test coverage of every action without an event loop
- 6 VT integration scenario tests covering bash prompt sequences, alternate screen, SGR attributes, scrollback search, mouse reporting, and OSC 8 hyperlinks
- headless `App` constructor using `EventLoopBuilderExtX11::with_any_thread(true)` enabling `App`-level tests in CI without a display

### Fixed
- reap child shell processes with a dedicated `wait()` thread to prevent zombie processes accumulating on tab open and pane split
- clipboard `get_or_insert_with` panic in headless CI: replace `.expect()` with `.ok()` so Copy and VisualYankLine actions degrade gracefully when no display is available

### Performance
- replace `scroll_up`/`scroll_down` double-loop clones with `rotate_left`/`rotate_right`; reduces cost per scroll line ~3.3× (49 µs → 15 µs for 220×50); `seq 1 100000` drops from 4.4 s to 1.4 s
- drive a vsync-style render loop at ~60 fps while PTY data is flowing so output appears progressively instead of in large batches

### Changed
- status bar `right` config option is now a format string (e.g. `"%pwd  %date{%H:%M}"`) instead of an array; spaces and literal text between tokens are preserved as-is
- extract `field_value_display`, `draw_hex_color_swatch`, `badge_pixel` helpers from `renderer/overlays.rs`; use `self.request_redraw()` in `about_to_wait` — reduces `draw_config_field_row` complexity 12→7; add 8 unit tests covering both helpers
- extract `scrollback_char_at`, `row_col_range` from `terminal/grid.rs`; `cell_char_at` made `pub(crate)` — simplifies `selected_text` and `motion::char_at`
- extract `scrollback_cell` from `geometry.rs`; `cell_url_at_scroll` simplified to 2 lines
- extract `search_args` helper from `views.rs` — deduplicates search argument assembly for zoomed and normal pane views
- extract `sep_hit` helper from `ui/layout.rs` — eliminates repeated inline bounds + distance check in `find_sep_at_pixel`
- extract `write_pixel` from `renderer/draw_fns.rs` — removes 4 inline bounds-check `if` blocks from `draw_rect_border`
- extract `select_primary_font`, `try_resolve_from_fallbacks`, `make_glyph_outline` from `renderer/glyph.rs` — reduces `resolve_glyph` complexity 14→6
- extract `apply_bell_flash`, `draw_cell` from `renderer/text.rs` — reduces `draw` complexity 12→3, `render_row` 12→6
- extract `handle_resize`, `should_swallow_key` into dedicated `impl App` block in `main.rs`; simplify `about_to_wait` using `self.request_redraw()`
- simplify `move_tab_index` in `tabs.rs` — replace early-exit match with two `if` guards
- simplify `word_forward` and `word_end` in `motion.rs` — convert loop+break patterns to `while let`; combine conditions
- extract `visual_char_action` from `handle_visual` in `keybindings.rs` — reduces `handle_visual` cognitive complexity 16→8; add 4 tests
- extract free helper functions from `renderer/text.rs` into new `renderer/draw_fns.rs` module — reduces `text.rs` by ~360 LOC; no behavior change
- extract `collect_pane_views`, `build_tab_titles` from `main.rs` into `src/views.rs`; extract `build_saved_session`, `restore_session` into `src/restore.rs` — reduces `main.rs` by ~200 LOC
- extract `plot_one_sixel`, `plot_pixel` from `plot_sixel` in `sixel.rs` — reduces plot_sixel complexity 16→4; extract `cell_out_of_pane_bounds`, `should_draw_glyph` from `render_row` and `draw_search_info`, `draw_pane_title_centered`, `draw_right_status` from `draw_status_bar` in `renderer/text.rs`
- extract `do_visual_copy`, `do_visual_yank_line`, `do_scroll_up/down/top/bottom`, `do_go_to_tab`, `do_zoom_pane`, `do_reset_font_size` from `app_state.rs`; extract `best_dir_candidate` from `focus_dir` in `layout.rs` — reduces dispatch_action 16→1, dispatch_visual_action 15→5, focus_dir 15→3; add 3 tests for best_dir_candidate
- extract `osc_set_title`, `osc_set_cwd`, `osc_set_hyperlink`, `osc_clipboard`, `char_delete_n`, `char_insert_n` from `parser.rs`; extract `strip_trailing_punct`, `stamp_url_span`, `collect_row_text` from `grid.rs`; extract `apply_pwd_token`, `apply_date_token` from `statusbar.rs`; add 14 unit tests for new helpers
- extract `pick_seq`, `handle_ctrl_only`, `visual_up_action`, `visual_down_action` from `keybindings.rs`; rewrite `handle_global_shortcuts` as a flat if-cascade — reduces cognitive complexity of `handle_global_shortcuts` (20→7), `handle_visual` (17→3), `cursor_seq` (13→1); add 14 unit tests for new helpers
- extract `drain.rs` from `main.rs`: move `drain_all`, `poll_pane_bytes`, `process_pane_bytes`, `update_tab_after_pane_poll` to dedicated module; extract `handle_focus_changed`, `handle_redraw_requested`, `copy_selection_to_clipboard`, `next_bell_wakeup` — reduces `main.rs` by ~160 LOC and 5 complex functions; add 4 unit tests
- extract `renderer/blit.rs` from `renderer/text.rs`: move `blit_color_glyph`, `blit_gray_glyph` into dedicated module with `compose_color_pixel` helper; extract `blit_glyph_pixels` from `draw_str` — reduces `draw_str` complexity from 19 to 1, reduces `renderer/text.rs` LOC by ~120
- extract `ctrl_shift_action`, `ctrl_char_action`, `alt_action` from `handle_global_shortcuts()` and `encode_ctrl_key`, `encode_alt_key`, `cursor_seq` from `handle_insert()` in `keybindings.rs`; reduces cognitive complexity from 85 to ~30
- extract `active_entry`, `active_entry_mut`, `active_grid_rows`, `move_visual_cursor`, `copy_text_to_clipboard`, `adjust_visual_scroll_up/down`, `visual_start_pos` from `dispatch_action()` in `app_state.rs`; reduces cognitive complexity from 144 to 61; add 6 tests covering previously untested arms (`VisualWordBackward`, `VisualWordEnd`, `VisualBoundaryDown`, visual coordinate adjustment on scroll)
- extract `do_auto_split`, `do_resize_pane`, `do_toggle_log`, `do_paste` from `execute_action()` in `main.rs`; extract `dispatch_visual_action`, `visual_boundary_scroll_up/down` from `dispatch_action()` in `app_state.rs`
- split `window_event()` into `handle_keyboard_input`, `handle_cursor_moved`, `handle_mouse_input`, `handle_mouse_wheel`; split `redraw()` into `collect_pane_views`, `build_tab_titles` — reduces `main.rs` cognitive complexity from 171 to 58
- split `draw_pane()` in `renderer/text.rs` into `fill_pane_background`, `search_highlight`, `resolve_cell_colors`, `draw_cell_decorations`, `draw_cursor_overlay`, `draw_scrollbar`, `draw_images`, and `draw_glyph` — reduces max function size from 419 to 130 lines
- replace nested pixel-fill double-loops in `draw_tab_bar`, `draw_status_bar`, `draw()`, `draw_pane()`, and `draw_scrollbar()` with `fill_rect` calls — eliminates 10 nested loop pairs, reducing cognitive and indentation complexity
- extract `process_pane_bytes` free function from `drain_all()` in `main.rs` — eliminates 4 levels of nesting in the PTY data loop, reducing cognitive complexity
- move test module (735 lines) from `app_state.rs` to `app_state_test.rs` — reduces `app_state.rs` from 1547 to 810 lines, mirroring the `renderer/text_test.rs` split pattern
- move input/event handler methods (`handle_search_key`, `handle_command_palette_key`, `handle_rename_key`, `handle_config_key`, `handle_keyboard_input`, `handle_cursor_moved`, `handle_mouse_input`, `handle_mouse_wheel`, `apply_config`, `reseed_pane_palettes`, `copy_current_match`, `update_search_matches`, `scroll_to_match`) from `main.rs` to `src/app_event.rs` — reduces `main.rs` from 1707 to 1341 lines
- move overlay rendering (`draw_config_panel`, `draw_command_palette`, `draw_quit_confirm`, `draw_save_session_confirm`) plus `draw_badge_label` and `draw_confirm_dialog` helpers to `renderer/overlays.rs`; extract `dim_buffer`, `fill_rect`, `draw_rect_border` primitives; move tests to `renderer/text_test.rs` — reduces `renderer/text.rs` from 3042 to 1129 lines
- extract `handle_dec_private_modes`, `handle_erase_display`, `handle_erase_line`, `handle_sgr`, `handle_char_ops` from `csi_dispatch()` in `terminal/parser.rs`; extract `handle_global_shortcuts` from `handle_key_inner()` in `input/keybindings.rs`
- extract `render_row` from `draw_pane()` in `renderer/text.rs` — moves the inner cell loop into its own method, eliminating double-nesting and reducing `draw_pane` cognitive complexity
- extract `do_set_mode`, `do_clear_scrollback`, `do_rename_tab`, `do_search_next/prev`, `do_quit` from `dispatch_action()` in `app_state.rs` — reduces cognitive complexity from 41 to 16
- extract `cell_char_at` helper from `selected_text()` and `make_char_cell` from `write_char()` in `terminal/grid.rs` — flattens deeply nested scroll/cell lookup logic
- extract `try_dispatch_overlay_key`, `do_middle_click_paste` from `handle_keyboard_input/handle_mouse_input` in `app_event.rs`
- extract `is_cell_cursor`, `is_cell_selected`, `draw_clipped_hline`, `draw_clipped_vline`, `link_underline_color`, `blit_sixel_pixel` from `renderer/text.rs` — reduces max per-function complexity from 30 → 19; simplifies `draw_cursor_overlay`, `draw_cell_decorations`, and `draw_images`
- extract `poll_pane_bytes` from `drain_all()` in `main.rs` — eliminates triple-nested loop body
- extract `parse_color_from_params` from `handle_sgr()` in `terminal/parser.rs` — reduces handle_sgr complexity from 28 → 12
- extract `ctrl_dot_next_mode` from `handle_global_shortcuts()` in `input/keybindings.rs` — removes nested mode-cycle match
- extract `draw_config_field_row` from `draw_config_panel()` in `renderer/overlays.rs` — reduces complexity from 32 → 12; introduce `FieldRowLayout` context struct
- extract `blit_glyph_badge` from `draw_badge_label()` in `renderer/overlays.rs` — reduces complexity from 19 → 1
- extract `try_start_separator_drag`, `send_pty_mouse_click`, `handle_selection_click` from `handle_mouse_input()` in `app_event.rs` — reduces complexity from 30 → 13
- extract `move_separator_drag`, `report_pty_mouse_move` from `handle_cursor_moved()` in `app_event.rs` — reduces complexity from 30 → 14
- extract `send_pty_scroll`, `viewport_scroll` from `handle_mouse_wheel()` in `app_event.rs` — reduces complexity from 21 → 3
- extract `do_toggle_fullscreen`, `do_new_tab`, `do_send_to_pty` from `execute_action()` in `main.rs` — reduces complexity from 24 → 13
- extract `update_tab_after_pane_poll` from `drain_all()` in `main.rs` — reduces complexity from 22 → 11
- extract `ctrl_special_char_action`, `shift_scroll_action` from `handle_global_shortcuts()` in `input/keybindings.rs` — reduces complexity from 25 (extreme) → 20 (very complex)
- update `.kimun.toml` fail_below to "B" now that score reached 80.1

### Documentation
- add kimun code quality gates to workflow: `.kimun.toml` config and `doc/LLMs.md` section

## [0.3.0] - 2026-05-16

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
- reverse video (`\e[7m`) was invisible: `write_char` pre-swapped fg/bg and the renderer swapped again, cancelling the effect; now only the renderer swaps based on `cell.reverse`
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
- plain-text URL auto-detection and click-to-open; trailing `)`, `.`, `,`, and similar punctuation stripped from matched links

#### Search
- Scrollback search with live match highlighting (`/` in Normal mode)
- Regex search support
- Copy matched text to clipboard
- Clear scrollback action

#### UI / panes
- Split-pane support with binary-tree layout (horizontal and vertical)
- Vim-style pane focus navigation (`Ctrl+W h/j/k/l`)
- Pane zoom — toggle full-window focus (`Ctrl+W z`); `ZoomPane` action dispatched in both normal and `Ctrl+W` key paths
- `q` in Normal mode closes the active pane, not the application
- pane auto-closes when the shell process exits (`exit` or `Ctrl+D`)
- immediate redraw on tab switch and `Ctrl+W` actions
- Scrollbar and improved keyboard scroll bindings
- Configurable inactive pane dimming
- Tab activity dot indicator

#### Tabs
- Multi-tab support with tab bar (`Ctrl+T`, `Ctrl+PageUp/Down`)
- Tab renaming with inline edit in tab bar
- Tab reordering with `Ctrl+Shift+PageUp/Down`
- New tabs and split panes inherit the current working directory

#### Fonts and rendering
- Color emoji support via FreeType CBDT/CBLC; tab bar glyph advance corrected after emoji so spacing is preserved
- Wide-character and glyph fallback font support
- correct font metrics computation for accurate cell sizing and glyph placement
- Per-tab font size (`Ctrl++` / `Ctrl+-` / `Ctrl+0`)
- Cursor blink resets on keypress

#### Configuration
- System font loading with bundled JetBrains Mono fallback
- Full 16-color ANSI palette theming
- TUI config panel (`Ctrl+,`) — edit settings in-process
- Configurable cursor blink interval
- Session logging: `Ctrl+Shift+L` toggles PTY output capture per-pane; active pane shows a `● REC` badge in the status bar; configurable via `[logging]` section (`auto_log`, `log_dir`)

#### Input
- Modal input: Insert, Normal, Visual, Search modes (vim-style); `Escape` passes through to the PTY in Insert mode, `Ctrl+.` cycles between modes
- `Alt+Tab` modifier state fully consumed so bare tab keystrokes don't leak into the PTY; modifier state resets on focus loss
- `Home`/`End` send standard VT sequences for readline compatibility
- Mouse selection and copy/paste support
- `Ctrl+C` copy in Visual mode
- `Ctrl+Shift+C` / `Ctrl+Shift+V` clipboard bindings

#### Infrastructure
- GitHub Actions CI (fmt, clippy -D warnings, test)
- Release workflow: multi-platform binaries (Linux x86_64/aarch64, macOS x86_64/aarch64), tar.gz archives, SHA256 checksums
- Desktop install script
- App icon with MM branding
- event loop yields on idle to eliminate CPU busy-loop during quiescent periods

[Unreleased]: https://github.com/roramirez/mmterm/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/roramirez/mmterm/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/roramirez/mmterm/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/roramirez/mmterm/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/roramirez/mmterm/releases/tag/v0.1.0
