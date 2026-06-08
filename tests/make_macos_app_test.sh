#!/bin/sh
# Smoke test for scripts/make-macos-app.sh. macOS-only (uses sips/iconutil/codesign/
# hdiutil); skips cleanly elsewhere. Run: sh tests/make_macos_app_test.sh
set -eu

HERE="$(cd "$(dirname "$0")/.." && pwd)"

if [ "$(uname -s)" != "Darwin" ]; then
  printf 'skip - make-macos-app test (not macOS)\n'
  exit 0
fi

fails=0
check() {
  if [ "$2" = "$3" ]; then printf 'ok   - %s\n' "$1"
  else printf 'FAIL - %s\n      expected: %s\n      actual:   %s\n' "$1" "$3" "$2"; fails=$((fails + 1)); fi
}

work="$(mktemp -d)"
trap 'hdiutil detach "${work}/mnt" >/dev/null 2>&1 || true; rm -rf "${work}"' EXIT

# A minimal stand-in "binary" (the bundler does not execute it).
printf '#!/bin/sh\necho mmterm 9.9.9\n' > "${work}/mmterm"
chmod 755 "${work}/mmterm"

dmg="$(sh "${HERE}/scripts/make-macos-app.sh" "${work}/mmterm" 9.9.9 "${work}/out")"
check "dmg path printed" "$( [ -f "${dmg}" ] && echo yes )" "yes"
check "dmg name correct" "$(basename "${dmg}")" "mmterm-macos-aarch64.dmg"

# Mount and inspect the bundle.
mkdir -p "${work}/mnt"
hdiutil attach "${dmg}" -nobrowse -mountpoint "${work}/mnt" >/dev/null
app="${work}/mnt/mmterm.app"
check "app bundle present"    "$( [ -d "${app}" ] && echo yes )" "yes"
check "executable present"    "$( [ -x "${app}/Contents/MacOS/mmterm" ] && echo yes )" "yes"
check "icns present"          "$( [ -f "${app}/Contents/Resources/mmterm.icns" ] && echo yes )" "yes"
check "Applications symlink"  "$( [ -L "${work}/mnt/Applications" ] && echo yes )" "yes"
check "plist version" \
  "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "${app}/Contents/Info.plist")" "9.9.9"
check "bundle id" \
  "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "${app}/Contents/Info.plist")" "io.github.roramirez.mmterm"
if codesign --verify --strict "${app}" >/dev/null 2>&1; then
  check "ad-hoc signature valid" "ok" "ok"
else
  check "ad-hoc signature valid" "FAIL" "ok"
fi

[ "${fails}" -eq 0 ] || { printf '\n%s test(s) failed\n' "${fails}"; exit 1; }
printf '\nall tests passed\n'
