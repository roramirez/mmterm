#!/bin/sh
# mmterm online installer — downloads and installs the prebuilt release binary.
# Usage: sh -c "$(curl -fsSL https://raw.githubusercontent.com/roramirez/mmterm/main/install.sh)"
# Env:   MMTERM_BIN_DIR  override install dir (default ~/.local/bin)
#        MMTERM_VERSION  pin a release tag (default: latest)
set -eu

REPO="roramirez/mmterm"
DEFAULT_BIN_DIR="${HOME}/.local/bin"
RAW_BASE="https://raw.githubusercontent.com/${REPO}/main"

log() { printf '%s\n' "$*" >&2; }
err() { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

detect_platform() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}:${arch}" in
    Linux:x86_64) printf 'mmterm-linux-x86_64' ;;
    Linux:aarch64 | Linux:arm64) printf 'mmterm-linux-aarch64' ;;
    Darwin:arm64) printf 'mmterm-macos-aarch64' ;;
    *)
      err "unsupported platform: ${os} ${arch}
Supported: Linux x86_64, Linux aarch64, macOS arm64.
Install from source instead:
  cargo install --git https://github.com/${REPO}" ;;
  esac
}

resolve_url() {
  # $1 = filename within the release (tarball or checksums file)
  if [ -n "${MMTERM_VERSION:-}" ]; then
    printf 'https://github.com/%s/releases/download/%s/%s' "${REPO}" "${MMTERM_VERSION}" "$1"
  else
    printf 'https://github.com/%s/releases/latest/download/%s' "${REPO}" "$1"
  fi
}

fetch() {
  # $1 = url, $2 = output path. HTTPS-only, no insecure fallback.
  if have curl; then
    curl --proto '=https' --tlsv1.2 -fsSL "$1" -o "$2"
  elif have wget; then
    # --https-only is GNU wget; busybox wget rejects it. URLs are already https://,
    # so the scheme enforces TLS even on the fallback.
    wget --https-only -q "$1" -O "$2" 2>/dev/null || wget -q "$1" -O "$2"
  else
    err "need curl or wget to download"
  fi
}

sha256_of() {
  # $1 = file -> prints lowercase hex digest. Fail-closed if no tool.
  if have sha256sum; then
    sha256sum "$1" | awk '{print $1}'
  elif have shasum; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    err "no sha256 tool (sha256sum/shasum) found; refusing to install unverified binary"
  fi
}

verify_checksum() {
  # $1 = artifact path, $2 = checksums file, $3 = artifact filename (with extension)
  # Match $3 against the basename of each checksums entry (paths are artifacts/-prefixed).
  expected="$(awk -v f="$3" '{ n=$2; sub(/.*\//, "", n); if (n == f) { print $1; exit } }' "$2")"
  [ -n "${expected}" ] || err "no checksum entry for $3"
  actual="$(sha256_of "$1")"
  [ "${expected}" = "${actual}" ] || err "checksum mismatch for $3
  expected ${expected}
  actual   ${actual}"
}

install_binary() {
  # $1 = source binary, $2 = target bin dir. Atomic: temp file + mv.
  mkdir -p "$2"
  tmp="$2/.mmterm.$$"
  cp "$1" "${tmp}"
  chmod 755 "${tmp}"
  mv -f "${tmp}" "$2/mmterm"
}

setup_desktop() {
  # $1 = installed binary path. Linux only; best-effort.
  [ "$(uname -s)" = "Linux" ] || return 0
  icon_dir="${HOME}/.local/share/icons/hicolor/256x256/apps"
  desktop_dir="${HOME}/.local/share/applications"
  mkdir -p "${icon_dir}" "${desktop_dir}"
  fetch "${RAW_BASE}/assets/icon.png" "${icon_dir}/mmterm.png" \
    || log "warning: could not download icon"
  cat > "${desktop_dir}/mmterm.desktop" <<EOF
[Desktop Entry]
Name=mmterm
Comment=Cross-platform CPU-rendered terminal emulator
Exec="$1"
Icon=mmterm
Type=Application
Categories=System;TerminalEmulator;
StartupNotify=false
EOF
  update-desktop-database "${desktop_dir}" 2>/dev/null || true
  update-icon-caches "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
}

shell_rc_file() {
  # Prints the rc file for the user's login shell, or empty if unknown.
  case "$(basename "${SHELL:-}")" in
    zsh) printf '%s' "${ZDOTDIR:-${HOME}}/.zshrc" ;;
    bash) printf '%s' "${HOME}/.bashrc" ;;
    *) printf '' ;;
  esac
}

ensure_on_path() {
  # $1 = bin dir. If already on PATH, do nothing. Otherwise auto-append an export
  # line to the user's shell rc (bash/zsh), idempotently; fall back to instructions.
  case ":${PATH}:" in
    *":$1:"*) return 0 ;;
  esac

  line="export PATH=\"$1:\$PATH\""
  rc="$(shell_rc_file)"

  if [ -n "${rc}" ]; then
    if [ -f "${rc}" ] && grep -qF "$1" "${rc}" 2>/dev/null; then
      log ""
      log "note: $1 already referenced in ${rc}"
      log "restart your shell or run: source ${rc}"
      return 0
    fi
    if {
      printf '\n# Added by mmterm installer\n'
      printf '%s\n' "${line}"
    } >>"${rc}" 2>/dev/null; then
      log ""
      log "added $1 to your PATH in ${rc}"
      log "restart your shell or run: source ${rc}"
      return 0
    fi
    log "warning: could not write to ${rc}"
  fi

  log ""
  log "warning: $1 is not on your PATH."
  log "Add this line to your shell profile (~/.zshrc, ~/.bashrc, etc.):"
  log "  ${line}"
}

macos_install() {
  # $1 = artifact basename (e.g. mmterm-macos-aarch64). Downloads the .dmg, verifies
  # it, parks it in ~/Downloads, and opens it for drag-to-/Applications.
  dmg="$1.dmg"
  dmg_url="$(resolve_url "${dmg}")"
  checksums_url="$(resolve_url "checksums-sha256.txt")"

  workdir="$(mktemp -d)"
  trap 'rm -rf "${workdir}"' EXIT

  log "downloading ${dmg}..."
  fetch "${dmg_url}" "${workdir}/${dmg}"
  fetch "${checksums_url}" "${workdir}/checksums-sha256.txt"

  log "verifying checksum..."
  verify_checksum "${workdir}/${dmg}" "${workdir}/checksums-sha256.txt" "${dmg}"

  dest_dir="${HOME}/Downloads"
  mkdir -p "${dest_dir}"
  mv -f "${workdir}/${dmg}" "${dest_dir}/${dmg}"

  log "opening ${dest_dir}/${dmg}..."
  open "${dest_dir}/${dmg}" || {
    log "warning: could not open ${dest_dir}/${dmg} automatically."
    log "open it manually from ~/Downloads to install."
  }

  log ""
  log "mmterm downloaded. In the disk-image window that just opened:"
  log "  drag mmterm.app onto the Applications folder."
  log "First launch: right-click mmterm.app -> Open (ad-hoc signed, not notarized)."
}

main() {
  artifact="$(detect_platform)"

  if [ "$(uname -s)" = "Darwin" ]; then
    macos_install "${artifact}"
    return
  fi

  bin_dir="${MMTERM_BIN_DIR:-${DEFAULT_BIN_DIR}}"
  tarball_url="$(resolve_url "${artifact}.tar.gz")"
  checksums_url="$(resolve_url "checksums-sha256.txt")"

  workdir="$(mktemp -d)"
  trap 'rm -rf "${workdir}"' EXIT

  log "downloading ${artifact}..."
  fetch "${tarball_url}" "${workdir}/${artifact}.tar.gz"
  fetch "${checksums_url}" "${workdir}/checksums-sha256.txt"

  log "verifying checksum..."
  verify_checksum "${workdir}/${artifact}.tar.gz" "${workdir}/checksums-sha256.txt" "${artifact}.tar.gz"

  log "installing to ${bin_dir}..."
  tar -xzf "${workdir}/${artifact}.tar.gz" -C "${workdir}"
  [ -f "${workdir}/mmterm" ] || err "tarball did not contain expected binary 'mmterm'"
  install_binary "${workdir}/mmterm" "${bin_dir}"
  setup_desktop "${bin_dir}/mmterm"

  log "installed: $("${bin_dir}/mmterm" --version 2>/dev/null || echo mmterm)"
  ensure_on_path "${bin_dir}"
  log "done. run: mmterm"
}

# Guarded so the test harness can source functions without running main.
# main MUST stay the last line: a truncated download never reaches it.
[ -n "${MMTERM_TEST:-}" ] || main "$@"
