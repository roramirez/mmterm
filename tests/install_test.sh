#!/bin/sh
# Test harness for install.sh. Sources the script with MMTERM_TEST=1 so main() does
# not run, then exercises individual functions. Run: sh tests/install_test.sh
set -eu

HERE="$(cd "$(dirname "$0")/.." && pwd)"
fails=0
check() {
  # $1 = description, $2 = actual, $3 = expected
  if [ "$2" = "$3" ]; then
    printf 'ok   - %s\n' "$1"
  else
    printf 'FAIL - %s\n      expected: %s\n      actual:   %s\n' "$1" "$3" "$2"
    fails=$((fails + 1))
  fi
}
expect_fail() {
  # $1 = description; runs $2.. and expects non-zero exit
  desc="$1"; shift
  if "$@" >/dev/null 2>&1; then
    printf 'FAIL - %s (expected non-zero exit)\n' "$desc"
    fails=$((fails + 1))
  else
    printf 'ok   - %s\n' "$desc"
  fi
}

# shellcheck disable=SC2034  # read by install.sh when sourced
MMTERM_TEST=1
# shellcheck source=/dev/null
. "${HERE}/install.sh"

# Smoke test: helpers are defined after sourcing.
check "log is defined" "$(command -v log >/dev/null 2>&1 && echo yes)" "yes"

# detect_platform maps uname output to release artifact names.
# shellcheck disable=SC2317  # invoked indirectly via detect_platform and setup_desktop
uname() { case "$1" in -s) echo "${MOCK_OS}" ;; -m) echo "${MOCK_ARCH}" ;; esac; }

MOCK_OS=Linux  MOCK_ARCH=x86_64        ; check "linux x86_64"  "$(detect_platform)" "mmterm-linux-x86_64"
MOCK_OS=Linux  MOCK_ARCH=aarch64       ; check "linux aarch64" "$(detect_platform)" "mmterm-linux-aarch64"
MOCK_OS=Linux  MOCK_ARCH=arm64         ; check "linux arm64"   "$(detect_platform)" "mmterm-linux-aarch64"
MOCK_OS=Darwin MOCK_ARCH=arm64         ; check "macos arm64"   "$(detect_platform)" "mmterm-macos-aarch64"
# shellcheck disable=SC2016  # ${HERE} expands in the outer shell before sh -c receives the string
expect_fail "macos intel unsupported" sh -c '. "'"${HERE}"'/install.sh"; uname() { case "$1" in -s) echo Darwin;; -m) echo x86_64;; esac; }; detect_platform'

unset MMTERM_VERSION 2>/dev/null || true
check "latest url" "$(resolve_url mmterm-linux-x86_64.tar.gz)" \
  "https://github.com/roramirez/mmterm/releases/latest/download/mmterm-linux-x86_64.tar.gz"
# shellcheck disable=SC2034  # MMTERM_VERSION is read by resolve_url (sourced from install.sh)
check_pin="$(MMTERM_VERSION=v0.5.0; resolve_url checksums-sha256.txt)"
check "pinned url" "${check_pin}" \
  "https://github.com/roramirez/mmterm/releases/download/v0.5.0/checksums-sha256.txt"
unset MMTERM_VERSION 2>/dev/null || true

# sha256_of computes the hex digest of a file, matching the system tool.
tmpf="$(mktemp)"; printf 'mmterm' > "${tmpf}"
expected_hash="$( (sha256sum "${tmpf}" 2>/dev/null || shasum -a 256 "${tmpf}") | awk '{print $1}')"
check "sha256_of matches reference tool" "$(sha256_of "${tmpf}")" "${expected_hash}"
rm -f "${tmpf}"
check "fetch is defined" "$(command -v fetch >/dev/null 2>&1 && echo yes)" "yes"

# verify_checksum reads the artifacts/-prefixed checksums file and matches on basename.
vdir="$(mktemp -d)"
printf 'binary-bytes' > "${vdir}/mmterm-linux-x86_64.tar.gz"
good="$(sha256_of "${vdir}/mmterm-linux-x86_64.tar.gz")"
printf '%s  artifacts/mmterm-linux-x86_64.tar.gz\n' "${good}" > "${vdir}/checksums-sha256.txt"
printf 'deadbeef  artifacts/mmterm-linux-aarch64.tar.gz\n' >> "${vdir}/checksums-sha256.txt"

if verify_checksum "${vdir}/mmterm-linux-x86_64.tar.gz" "${vdir}/checksums-sha256.txt" mmterm-linux-x86_64; then
  check "verify_checksum accepts good hash" "ok" "ok"
else
  check "verify_checksum accepts good hash" "FAIL" "ok"
fi
expect_fail "verify_checksum rejects bad hash" \
  sh -c '. "'"${HERE}"'/install.sh"; verify_checksum "'"${vdir}"'/mmterm-linux-x86_64.tar.gz" "'"${vdir}"'/checksums-sha256.txt" mmterm-linux-aarch64'
expect_fail "verify_checksum rejects missing entry" \
  sh -c '. "'"${HERE}"'/install.sh"; verify_checksum "'"${vdir}"'/mmterm-linux-x86_64.tar.gz" "'"${vdir}"'/checksums-sha256.txt" mmterm-macos-aarch64'
rm -rf "${vdir}"

# install_binary copies atomically into the target dir with mode 755.
idir="$(mktemp -d)"; src="$(mktemp)"; printf '#!/bin/sh\necho hi\n' > "${src}"
install_binary "${src}" "${idir}/bin"
check "binary installed"   "$( [ -f "${idir}/bin/mmterm" ] && echo yes )" "yes"
check "binary executable"  "$( [ -x "${idir}/bin/mmterm" ] && echo yes )" "yes"
check "no temp leftover"   "$( find "${idir}/bin" -name '.mmterm.*' 2>/dev/null | wc -l | tr -d ' ' )" "0"
rm -rf "${idir}" "${src}"

# verify_provenance is a no-op (success) when gh is absent.
if ! command -v gh >/dev/null 2>&1; then
  if verify_provenance /nonexistent; then
    check "provenance skipped without gh" "ok" "ok"
  else
    check "provenance skipped without gh" "FAIL" "ok"
  fi
fi

# ensure_on_path: already-on-PATH does nothing and writes nothing.
ep_home="$(mktemp -d)"
PATH_BAK="${PATH}"; HOME_BAK="${HOME}"; SHELL_BAK="${SHELL:-}"
PATH="/usr/bin:/bin"; HOME="${ep_home}"; SHELL="/bin/zsh"
out="$(ensure_on_path /usr/bin 2>&1 || true)"
check "ensure_on_path silent when on PATH" "$(printf '%s' "${out}" | wc -c | tr -d ' ')" "0"
check "ensure_on_path writes nothing when on PATH" "$( [ -e "${ep_home}/.zshrc" ] && echo exists || echo none )" "none"

# ensure_on_path (zsh): off PATH -> append export to ~/.zshrc, idempotent.
out="$(ensure_on_path /opt/mm/bin 2>&1 || true)"
check "zsh rc gets export" "$(grep -c '/opt/mm/bin' "${ep_home}/.zshrc" 2>/dev/null || echo 0)" "1"
check "zsh message mentions added" "$(printf '%s' "${out}" | grep -c 'added /opt/mm/bin')" "1"
out2="$(ensure_on_path /opt/mm/bin 2>&1 || true)"
check "zsh rc not duplicated" "$(grep -c '/opt/mm/bin' "${ep_home}/.zshrc" 2>/dev/null || echo 0)" "1"
check "zsh idempotent note" "$(printf '%s' "${out2}" | grep -c 'already referenced')" "1"

# ensure_on_path (bash): off PATH -> append export to ~/.bashrc.
SHELL="/bin/bash"
out="$(ensure_on_path /opt/mm/bin 2>&1 || true)"
check "bash rc gets export" "$(grep -c '/opt/mm/bin' "${ep_home}/.bashrc" 2>/dev/null || echo 0)" "1"

# ensure_on_path (unknown shell): off PATH -> printed fallback, writes nothing new.
SHELL="/usr/bin/fish"
out="$(ensure_on_path /opt/zz/bin 2>&1 || true)"
check "unknown shell fallback printed" "$(printf '%s' "${out}" | grep -c 'not on your PATH')" "1"
check "unknown shell writes nothing" "$( grep -rl '/opt/zz/bin' "${ep_home}" 2>/dev/null | wc -l | tr -d ' ' )" "0"

PATH="${PATH_BAK}"; HOME="${HOME_BAK}"; SHELL="${SHELL_BAK}"
rm -rf "${ep_home}"

# setup_desktop is a no-op on non-Linux (returns success).
# shellcheck disable=SC2317  # invoked indirectly via setup_desktop
uname() { echo Darwin; }
if setup_desktop /tmp/mmterm; then
  check "setup_desktop noop on macos" "ok" "ok"
else
  check "setup_desktop noop on macos" "FAIL" "ok"
fi
unset -f uname 2>/dev/null || true

# End-to-end: fake local release, mock fetch/uname/provenance, run real main.
e2e="$(mktemp -d)"
mkdir -p "${e2e}/release" "${e2e}/home"
printf '#!/bin/sh\necho "mmterm 9.9.9 (test)"\n' > "${e2e}/mmterm"
chmod 755 "${e2e}/mmterm"
( cd "${e2e}" && tar -czf release/mmterm-linux-x86_64.tar.gz mmterm )
e2e_hash="$(sha256_of "${e2e}/release/mmterm-linux-x86_64.tar.gz")"
printf '%s  artifacts/mmterm-linux-x86_64.tar.gz\n' "${e2e_hash}" > "${e2e}/release/checksums-sha256.txt"

# Mocks: serve downloads from local dir; force Linux x86_64; skip real gh provenance.
# shellcheck disable=SC2317  # invoked indirectly inside the main subshell below
fetch() { cp "${e2e}/release/$(basename "$1")" "$2" 2>/dev/null || return 1; }
# shellcheck disable=SC2317  # invoked indirectly inside the main subshell below
uname() { case "$1" in -s) echo Linux ;; -m) echo x86_64 ;; esac; }
# shellcheck disable=SC2317  # invoked indirectly inside the main subshell below
verify_provenance() { return 0; }

# Run main in a subshell so HOME/SHELL/trap/env changes stay isolated.
# shellcheck disable=SC2034  # MMTERM_BIN_DIR is read by main() sourced from install.sh
if ( HOME="${e2e}/home"; SHELL="/bin/zsh"; MMTERM_BIN_DIR="${e2e}/bin"; main ) >/dev/null 2>&1; then
  check "e2e main succeeds" "ok" "ok"
else
  check "e2e main succeeds" "FAIL" "ok"
fi
check "e2e installs binary" "$( [ -x "${e2e}/bin/mmterm" ] && echo yes )" "yes"
check "e2e binary runs"     "$( "${e2e}/bin/mmterm" )" "mmterm 9.9.9 (test)"
rm -rf "${e2e}"
unset -f fetch uname verify_provenance 2>/dev/null || true

# main aborts (does not install) when the tarball lacks the 'mmterm' binary.
e2e="$(mktemp -d)"
mkdir -p "${e2e}/release" "${e2e}/home"
printf 'not-the-binary' > "${e2e}/wrongname"
( cd "${e2e}" && tar -czf release/mmterm-linux-x86_64.tar.gz wrongname )
e2e_hash="$(sha256_of "${e2e}/release/mmterm-linux-x86_64.tar.gz")"
printf '%s  artifacts/mmterm-linux-x86_64.tar.gz\n' "${e2e_hash}" > "${e2e}/release/checksums-sha256.txt"
# shellcheck disable=SC2317  # invoked indirectly inside the main subshell below
fetch() { cp "${e2e}/release/$(basename "$1")" "$2" 2>/dev/null || return 1; }
# shellcheck disable=SC2317  # invoked indirectly inside the main subshell below
uname() { case "$1" in -s) echo Linux ;; -m) echo x86_64 ;; esac; }
# shellcheck disable=SC2317  # invoked indirectly inside the main subshell below
verify_provenance() { return 0; }
# shellcheck disable=SC2034  # MMTERM_BIN_DIR is read by main() sourced from install.sh
if ( HOME="${e2e}/home"; SHELL="/bin/zsh"; MMTERM_BIN_DIR="${e2e}/bin"; main ) >/dev/null 2>&1; then
  check "e2e aborts on missing binary" "installed" "aborted"
else
  check "e2e aborts on missing binary" "aborted" "aborted"
fi
check "e2e missing-binary installs nothing" "$( [ -e "${e2e}/bin/mmterm" ] && echo exists || echo none )" "none"
rm -rf "${e2e}"
unset -f fetch uname verify_provenance 2>/dev/null || true

[ "${fails}" -eq 0 ] || { printf '\n%s test(s) failed\n' "${fails}"; exit 1; }
printf '\nall tests passed\n'
