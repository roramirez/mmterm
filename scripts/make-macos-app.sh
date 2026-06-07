#!/bin/sh
# Build an ad-hoc-signed mmterm.app and package it into mmterm-macos-aarch64.dmg.
# Usage: make-macos-app.sh <binary-path> <version> <out-dir>
# Requires macOS stock tools: sips, iconutil, codesign, hdiutil, plutil.
set -eu

BIN="${1:?usage: make-macos-app.sh <binary-path> <version> <out-dir>}"
VERSION="${2:?version required}"
OUT="${3:?output dir required}"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ICON_SRC="${REPO_ROOT}/assets/icon.png"
APP_NAME="mmterm"
DMG="mmterm-macos-aarch64.dmg"

[ -f "${BIN}" ] || { printf 'error: binary not found: %s\n' "${BIN}" >&2; exit 1; }
[ -f "${ICON_SRC}" ] || { printf 'error: icon not found: %s\n' "${ICON_SRC}" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

app="${work}/${APP_NAME}.app"
mkdir -p "${app}/Contents/MacOS" "${app}/Contents/Resources"

# 1. Binary
cp "${BIN}" "${app}/Contents/MacOS/${APP_NAME}"
chmod 755 "${app}/Contents/MacOS/${APP_NAME}"

# 2. Icon: build an iconset from the 256px source (no upscaling past source), then .icns
iconset="${work}/${APP_NAME}.iconset"
mkdir -p "${iconset}"
sips -z 16 16   "${ICON_SRC}" --out "${iconset}/icon_16x16.png"      >/dev/null
sips -z 32 32   "${ICON_SRC}" --out "${iconset}/icon_16x16@2x.png"   >/dev/null
sips -z 32 32   "${ICON_SRC}" --out "${iconset}/icon_32x32.png"      >/dev/null
sips -z 64 64   "${ICON_SRC}" --out "${iconset}/icon_32x32@2x.png"   >/dev/null
sips -z 128 128 "${ICON_SRC}" --out "${iconset}/icon_128x128.png"    >/dev/null
sips -z 256 256 "${ICON_SRC}" --out "${iconset}/icon_128x128@2x.png" >/dev/null
sips -z 256 256 "${ICON_SRC}" --out "${iconset}/icon_256x256.png"    >/dev/null
iconutil -c icns "${iconset}" -o "${app}/Contents/Resources/${APP_NAME}.icns"

# 3. Info.plist
cat > "${app}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>mmterm</string>
  <key>CFBundleDisplayName</key><string>mmterm</string>
  <key>CFBundleExecutable</key><string>mmterm</string>
  <key>CFBundleIdentifier</key><string>io.github.roramirez.mmterm</string>
  <key>CFBundleIconFile</key><string>mmterm</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict>
</plist>
PLIST
plutil -lint "${app}/Contents/Info.plist" >/dev/null

# 4. Ad-hoc sign (mandatory for Apple-Silicon bundles to launch at all).
# No --deep: the bundle has no nested code, and --deep is deprecated by Apple.
codesign --force --sign - "${app}"
codesign --verify --strict "${app}"

# 5. .dmg with an /Applications drop target
stage="${work}/dmg"
mkdir -p "${stage}"
cp -R "${app}" "${stage}/"
ln -s /Applications "${stage}/Applications"

mkdir -p "${OUT}"
OUT="$(cd "${OUT}" && pwd)"
rm -f "${OUT}/${DMG}"
hdiutil create -volname "${APP_NAME}" -srcfolder "${stage}" -ov -format UDZO "${OUT}/${DMG}" >/dev/null

printf '%s\n' "${OUT}/${DMG}"
