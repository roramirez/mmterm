#!/usr/bin/env bash
# Generates assets/demo.gif by recording a live mmterm session.
# Requirements: xdotool, ffmpeg, gifsicle (all on PATH), DISPLAY set.

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
MMTERM="$REPO/target/release/mmterm"
OUT_DIR="$REPO/assets"
TMP_MP4="/tmp/mmterm_demo_$$.mp4"
OUT_GIF="$OUT_DIR/demo.gif"
CFG_DIR="/tmp/mmterm_demo_cfg_$$"

die() { echo "ERROR: $*" >&2; exit 1; }

[[ -x "$MMTERM" ]] || die "mmterm not built — run: cargo build --release"
command -v xdotool  >/dev/null || die "xdotool not found"
command -v ffmpeg   >/dev/null || die "ffmpeg not found"
command -v gifsicle >/dev/null || die "gifsicle not found"
[[ -n "${DISPLAY:-}" ]]        || die "DISPLAY not set"

mkdir -p "$OUT_DIR"

# ── Shell wrapper: starts zsh already in the project dir ─────────────────────
# (CommandBuilder::new takes a single executable, not a shell string with args,
#  so we use a wrapper script instead of "zsh --something ...")
cat > /tmp/mmterm_demo_shell.sh << SHEOF
#!/usr/bin/env zsh
[ -f "\$HOME/.zshrc" ] && source "\$HOME/.zshrc" 2>/dev/null
cd "$REPO"
# Clean prompt for the demo: no git branch, no dirty marker
export PROMPT='%F{cyan}%n%f :: %F{yellow}%~%f » '
export RPROMPT=''
exec zsh
SHEOF
chmod +x /tmp/mmterm_demo_shell.sh

# ── Temp config: 1280×720, shell wrapper ──────────────────────────────────────
mkdir -p "$CFG_DIR/mmterm"
cat > "$CFG_DIR/mmterm/config.toml" << 'TOML'
[font]
family = "monospace"
size   = 14.0

[window]
width           = 1280
height          = 720
title           = "mmterm"
cursor_blink_ms = 500

[shell]
program = "/tmp/mmterm_demo_shell.sh"
TOML

# ── Pre-written demo scripts ──────────────────────────────────────────────────
cat > /tmp/mmterm_colors.sh << 'SH'
#!/usr/bin/env zsh
for i in $(seq 0 255); do
    printf '\e[48;5;%dm  \e[0m' "$i"
    (( (i+1) % 16 == 0 )) && echo
done
echo
SH
chmod +x /tmp/mmterm_colors.sh

cat > /tmp/mmterm_attrs.sh << 'SH'
#!/usr/bin/env zsh
printf '  \e[1mbold\e[0m          \e[1;32m■\e[0m text rendered heavier\n'
printf '  \e[2mdim\e[0m           \e[2;32m■\e[0m reduced intensity\n'
printf '  \e[3mitalic\e[0m        \e[3;32m■\e[0m slanted text variant\n'
printf '  \e[4munderline\e[0m     \e[4;32m■\e[0m underscore decoration\n'
printf '  \e[53moverline\e[0m      \e[53;32m■\e[0m line above the text\n'
printf '  \e[9mstrikethrough\e[0m \e[9;32m■\e[0m crossed-out text\n'
printf '  \e[7mreverse video\e[0m \e[7;32m■\e[0m fg/bg colors swapped\n'
printf '  \e[1m\e[4mBold + underline combined\e[0m\n'
printf '  \e[31mred\e[0m \e[32mgreen\e[0m \e[33myellow\e[0m \e[34mblue\e[0m \e[35mmagenta\e[0m \e[36mcyan\e[0m\n'
SH
chmod +x /tmp/mmterm_attrs.sh

# ── Launch mmterm with custom config ─────────────────────────────────────────
echo "→ Launching mmterm (1280×720)…"
FFMPEG_PID=0
XDG_CONFIG_HOME="$CFG_DIR" "$MMTERM" &
MMTERM_PID=$!
trap 'kill $MMTERM_PID 2>/dev/null; [[ $FFMPEG_PID -ne 0 ]] && kill $FFMPEG_PID 2>/dev/null; rm -rf "$CFG_DIR"; wait 2>/dev/null' EXIT

# Wait for window (up to 6 s)
WID=""
for i in $(seq 1 30); do
    WID=$(xdotool search --pid "$MMTERM_PID" --onlyvisible 2>/dev/null | head -1 || true)
    [[ -n "$WID" ]] && break
    sleep 0.2
done
[[ -n "$WID" ]] || die "mmterm window never appeared"

xdotool windowraise "$WID"
xdotool windowfocus --sync "$WID"
sleep 1.5   # let shell fully initialise

# Use xwininfo for precise content-area coordinates (avoids WM decoration skew)
WIN_INFO=$(xwininfo -id "$WID" 2>/dev/null)
PX=$(echo "$WIN_INFO" | awk '/Absolute upper-left X:/ {print $NF}')
PY=$(echo "$WIN_INFO" | awk '/Absolute upper-left Y:/ {print $NF}')
W=$(echo  "$WIN_INFO" | awk '/Width:/  {print $NF}')
H=$(echo  "$WIN_INFO" | awk '/Height:/ {print $NF}')
echo "→ Window $WID  ${W}x${H} at ${PX},${PY}"

# ── Start recording ───────────────────────────────────────────────────────────
echo "→ Recording…"
ffmpeg -y \
    -f x11grab -r 20 -s "${W}x${H}" -i ":0.0+${PX},${PY}" \
    -c:v libx264 -preset ultrafast -crf 18 \
    "$TMP_MP4" 2>/dev/null &
FFMPEG_PID=$!
sleep 0.6   # ffmpeg warmup

# ── Helpers ───────────────────────────────────────────────────────────────────
K()     { xdotool key  --window "$WID" --clearmodifiers "$@"; }
TYP()   { xdotool type --window "$WID" --clearmodifiers --delay 55 "$@"; }
RET()   { K Return; }
PAUSE() { sleep "${1:-1}"; }
CW()    { xdotool keyup --window "$WID" shift; xdotool keydown --window "$WID" ctrl; xdotool key --window "$WID" w; xdotool keyup --window "$WID" ctrl; sleep 0.15; K "$1"; }  # Ctrl+W prefix + key

# ── Demo ──────────────────────────────────────────────────────────────────────
# Shell starts in $REPO; no personal paths are ever typed.
# Each scene opens with a shell comment that explains the keybinding shown.
PAUSE 0.8

# Scene 1 — SGR text attributes
TYP '# SGR text attributes — bold, dim, underline, strikethrough, reverse'; RET; PAUSE 0.4
TYP 'zsh /tmp/mmterm_attrs.sh'; RET; PAUSE 2.5

# Scene 2 — 256-color palette
TYP '# 256-color support'; RET; PAUSE 0.3
TYP 'zsh /tmp/mmterm_colors.sh'; RET; PAUSE 2

# Scene 3 — vertical split  Ctrl+W v
TYP '# split pane  Ctrl+W v'; RET; PAUSE 0.4
CW v; PAUSE 1.2
TYP 'ls --color=always src/renderer/'; RET; PAUSE 1.5

# Scene 4 — horizontal split  Ctrl+W s
TYP '# split pane  Ctrl+W s'; RET; PAUSE 0.4
CW s; PAUSE 1
TYP 'ls --color=always src/terminal/'; RET; PAUSE 1.2

# Scene 5 — navigate panes  Ctrl+W h/j/k/l
TYP '# navigate panes  Ctrl+W h/j/k/l'; RET; PAUSE 0.4
CW h; PAUSE 0.5
TYP 'ls --color=always src/ui/'; RET; PAUSE 1
CW l; PAUSE 0.4
CW j; PAUSE 0.4
TYP 'ls --color=always src/input/'; RET; PAUSE 1
CW k; PAUSE 0.5

# Scene 6 — zoom pane  Ctrl+W z
TYP '# zoom pane  Ctrl+W z'; RET; PAUSE 0.4
CW z; PAUSE 1.8
TYP 'ls --color=always src/'; RET; PAUSE 1.2
CW z; PAUSE 1       # unzoom

# Scene 7 — rename tab  Ctrl+Shift+R
TYP '# rename tab  Ctrl+Shift+R'; RET; PAUSE 0.4
K ctrl+shift+r; PAUSE 0.5
TYP 'mmterm'; PAUSE 0.8
K Return; PAUSE 1

# Scene 8 — new tab  Ctrl+T, rename, switch back  Ctrl+PageUp
TYP '# new tab  Ctrl+T'; RET; PAUSE 0.4
K ctrl+t; PAUSE 0.8
K ctrl+shift+r; PAUSE 0.4
TYP 'colors'; PAUSE 0.6
K Return; PAUSE 0.5
TYP 'zsh /tmp/mmterm_colors.sh'; RET; PAUSE 1.5
K ctrl+Prior; PAUSE 1   # back to tab 1

# Scene 9 — scrollback search  Ctrl+.  then /
TYP '# scrollback search  Ctrl+.  /pattern  n/N'; RET; PAUSE 0.4
K ctrl+period; PAUSE 0.6     # Insert → Normal
K slash; PAUSE 0.4
TYP 'renderer'; PAUSE 1.4
K n; PAUSE 0.6
K Escape; PAUSE 0.4          # Search → Normal
K Escape; PAUSE 0.6          # Normal → Insert

# Scene 10 — per-tab font size  Ctrl++ / Ctrl+- / Ctrl+0
TYP '# per-tab font size  Ctrl++  Ctrl+-  Ctrl+0'; RET; PAUSE 0.4
K ctrl+plus;   PAUSE 0.35
K ctrl+plus;   PAUSE 0.35
K ctrl+plus;   PAUSE 0.6
K ctrl+minus;  PAUSE 0.35
K ctrl+minus;  PAUSE 0.35
K ctrl+minus;  PAUSE 0.35
K ctrl+0;      PAUSE 0.8

# Scene 11 — pane rotation  Ctrl+W r / Ctrl+W R
TYP '# rotate panes  Ctrl+W r / Ctrl+W R'; RET; PAUSE 0.4
CW r; PAUSE 0.9
CW r; PAUSE 0.9
CW 'shift+r'; PAUSE 1

# Scene 12 — keyboard pane resize  Ctrl+Shift+Arrow
TYP '# resize panes  Ctrl+Shift+Arrow'; RET; PAUSE 0.4
K ctrl+shift+Right; PAUSE 0.18
K ctrl+shift+Right; PAUSE 0.18
K ctrl+shift+Right; PAUSE 0.35
K ctrl+shift+Left;  PAUSE 0.18
K ctrl+shift+Left;  PAUSE 0.18
K ctrl+shift+Left;  PAUSE 0.8

# Scene 13 — auto-split  Ctrl+W a
TYP '# auto-split (longest dim)  Ctrl+W a'; RET; PAUSE 0.4
CW a; PAUSE 1.2
TYP 'ls --color=always src/pty/'; RET; PAUSE 1.2

# Scene 14 — visual mode selection  Ctrl+. Ctrl+. v … y
TYP '# visual selection  Ctrl+. twice → Visual  v anchor  y copy'; RET; PAUSE 0.4
TYP "printf 'line one\nline two\nline three\n'"; RET; PAUSE 0.8
K ctrl+period; PAUSE 0.4     # → Normal
K ctrl+period; PAUSE 0.4     # → Visual (unanchored)
K k; PAUSE 0.25
K k; PAUSE 0.25
K v; PAUSE 0.5               # set anchor
K j; PAUSE 0.3
K j; PAUSE 0.3
K y; PAUSE 0.6               # copy, exits visual
K Escape; PAUSE 0.3          # Visual/Normal → Normal
K Escape; PAUSE 0.5          # Normal → Insert

# Scene 15 — command palette  Ctrl+Shift+P
TYP '# command palette  Ctrl+Shift+P'; RET; PAUSE 0.4
K ctrl+shift+p; PAUSE 0.7
TYP 'split'; PAUSE 1.2
K Escape; PAUSE 0.8

# Scene 16 — screenshot mode  Ctrl+W p
TYP '# screenshot  Ctrl+W p → arrows → Enter'; RET; PAUSE 0.4
CW p; PAUSE 1
K Up; PAUSE 0.18
K Up; PAUSE 0.18
K shift+Right; PAUSE 0.18
K shift+Right; PAUSE 0.3
K Return; PAUSE 1.5

# Scene 17 — passthrough mode  Ctrl+B
TYP '# passthrough mode  Ctrl+B (bypasses all shortcuts)'; RET; PAUSE 0.4
K ctrl+b; PAUSE 1
K ctrl+b; PAUSE 0.8

# Scene 18 — Alt+1..9 direct tab jump
TYP '# jump to tab  Alt+1 / Alt+2'; RET; PAUSE 0.4
K alt+2; PAUSE 1
K alt+1; PAUSE 0.8

PAUSE 0.5

# ── Stop ─────────────────────────────────────────────────────────────────────
kill $FFMPEG_PID
wait $FFMPEG_PID 2>/dev/null || true
echo "→ Recording stopped."
kill $MMTERM_PID 2>/dev/null || true
trap - EXIT
rm -rf "$CFG_DIR"
wait 2>/dev/null || true

# ── MP4 → GIF ────────────────────────────────────────────────────────────────
echo "→ Converting to GIF…"
PALETTE="/tmp/mmterm_pal_$$.png"

# Trim the first 1 s (blank shell startup), scale to 1280 wide, crop bottom 2px
VFBASE="trim=start=1,setpts=PTS-STARTPTS,fps=18,scale=1280:-1:flags=lanczos"

ffmpeg -y -i "$TMP_MP4" \
    -vf "${VFBASE},palettegen=stats_mode=diff" \
    -update 1 "$PALETTE" 2>/dev/null

ffmpeg -y -i "$TMP_MP4" -i "$PALETTE" \
    -filter_complex "${VFBASE}[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=5" \
    "$OUT_GIF" 2>/dev/null

rm -f "$PALETTE" "$TMP_MP4"

echo "→ Optimising…"
gifsicle -O3 --lossy=60 --colors 256 "$OUT_GIF" -o "$OUT_GIF"

SIZE=$(du -sh "$OUT_GIF" | cut -f1)
echo "✓  $OUT_GIF  ($SIZE)"
