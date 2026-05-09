# mmterm

<img src="assets/icon.svg" alt="mmterm icon" width="128"/>

A cross-platform, GPU-free terminal emulator written in Rust with vim-style modal input, split panes, and multi-tab sessions.

Renders entirely via a CPU pixel buffer — no GPU, no OpenGL, no Vulkan.

## Features

- **Modal input** — Insert, Normal, Visual, and Search modes (vim-style)
- **Split panes** — binary-tree layout, horizontal and vertical splits
- **Multi-tab** — independent pane trees and font metrics per tab
- **Scrollback search** — live match highlighting across 10 000-line buffer
- **OSC 8 hyperlinks** — clickable URLs rendered in the terminal
- **Pane zoom** — full-window focus for the active pane
- **Session logging** — capture PTY output per-pane to `~/.mmterm/` with `Ctrl+Shift+L`
- **Color emoji** — rendered via FreeType CBDT/CBLC
- **TUI config editor** — edit settings in-process with `Ctrl+,`
- **Zero-config startup** — bundled JetBrains Mono fallback font

## Requirements

- Rust 1.85+ (edition 2024)
- Linux (X11 or Wayland) or macOS
- On Linux: a C toolchain and FreeType headers (`libfreetype-dev`)

## Build

```sh
cargo build --release
```

The binary is at `target/release/mmterm`.

## Install

```sh
cargo install --path .
```

## Running

```sh
mmterm
```

Enable logging:

```sh
RUST_LOG=info mmterm
```

## Configuration

On first run, a config file is created at:

- **Linux/macOS**: `$XDG_CONFIG_HOME/mmterm/config.toml` (defaults to `~/.config/mmterm/config.toml`)

```toml
[font]
family = "Noto Sans Mono"
size   = 16.0

[window]
width           = 800
height          = 600
title           = "mmterm"
cursor_blink_ms = 500

[shell]
# program = "/bin/zsh"   # defaults to $SHELL

[logging]
auto_log = false         # start logging automatically for every new pane
log_dir  = ""            # destination directory (empty = ~/.mmterm)

[colors]
background = "#121212"
foreground = "#a0a0a0"
cursor     = "#bbbbbb"
selection  = "#3d3d3d"
palette    = [ ... ]     # 16-color ANSI palette
```

You can also edit settings live with `Ctrl+,`.

## Key Bindings

### Global

| Binding | Action |
|---|---|
| `Ctrl+Q` | Quit |
| `Ctrl+,` | Open config panel |
| `Ctrl+T` | New tab |
| `Ctrl+PageUp` / `Ctrl+PageDown` | Previous / next tab |
| `Ctrl+Shift+W` | Close tab |
| `Ctrl++` / `Ctrl+=` | Increase font size (current tab) |
| `Ctrl+-` | Decrease font size (current tab) |
| `Ctrl+0` | Reset font size |

### Modes

| Binding | Action |
|---|---|
| `Ctrl+.` | Cycle Insert → Normal → Visual → Insert |
| `Ctrl+\` | Enter Normal mode |

### Panes (`Ctrl+W` prefix)

| Binding | Action |
|---|---|
| `Ctrl+W v` | Split horizontally |
| `Ctrl+W s` | Split vertically |
| `Ctrl+W h/j/k/l` | Focus left / down / up / right |
| `Ctrl+W w` | Cycle focus |
| `Ctrl+W q` | Close pane |
| `Ctrl+W z` | Toggle pane zoom |
| `Ctrl+Shift+L` | Toggle session logging for active pane |

### Scrollback

| Binding | Action |
|---|---|
| `Shift+PageUp` / `Shift+PageDown` | Scroll half screen |
| `Ctrl+Shift+↑` / `Ctrl+Shift+↓` | Scroll one line |
| `Ctrl+Shift+Home` / `Ctrl+Shift+End` | Jump to top / bottom |

### Search (enter from Normal mode with `/`)

| Binding | Action |
|---|---|
| `/` | Open search |
| `Enter` | Next match |
| `n` / `N` | Next / previous match (Normal mode) |
| `Escape` | Exit search |

### Clipboard

| Binding | Action |
|---|---|
| `Ctrl+Shift+C` | Copy selection |
| `Ctrl+Shift+V` | Paste |

## Architecture

```
main.rs (App, event loop)
├── input/      — key → Action mapping, modal state
├── pty/        — PTY fork, shell spawn, read/write
├── terminal/   — VT/ANSI parser, cell grid, scrollback
├── ui/         — binary-tree split layout, pane struct
├── renderer/   — CPU pixel rendering, glyph cache
├── config.rs   — TOML load/save
└── tui_config/ — in-process config editor
```

See [`doc/SPEC.md`](doc/SPEC.md) for the full architecture and feature specification.

## License

GPL-2.0 — see [LICENSE](LICENSE).
