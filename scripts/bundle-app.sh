#!/usr/bin/env bash
# Builds the core and the SwiftUI app in release mode and packages
# dist/Kafkalens.app.
#
# Usage: scripts/bundle-app.sh [--universal] [--install]
#   --universal   arm64 + x86_64 (what releases ship); default is this Mac's arch
#   --install     also copy the app to /Applications, replacing any older copy
#
# Signing: ad-hoc by default (fine for local use). Set KAFKALENS_SIGN_IDENTITY
# to a "Developer ID Application: …" identity to sign for distribution with
# the hardened runtime, as notarization requires.
set -euo pipefail

INSTALL=false
UNIVERSAL=false
for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=true ;;
    --universal) UNIVERSAL=true ;;
    *) echo "unknown option: $arg" >&2; exit 1 ;;
  esac
done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/core/Cargo.toml" | head -1)"
APP="$ROOT/dist/Kafkalens.app"

SWIFT_ARCHS=()
if $UNIVERSAL; then
  "$ROOT/scripts/build-core.sh" release --universal
  SWIFT_ARCHS=(--arch arm64 --arch x86_64)
else
  "$ROOT/scripts/build-core.sh" release
fi

cd "$ROOT/macos"
swift build -c release ${SWIFT_ARCHS[@]+"${SWIFT_ARCHS[@]}"}
BIN="$(swift build -c release ${SWIFT_ARCHS[@]+"${SWIFT_ARCHS[@]}"} --show-bin-path)/Kafkalens"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/Kafkalens"
cp "$ROOT/macos/Resources/AppIcon.icns" "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>Kafkalens</string>
  <key>CFBundleDisplayName</key>
  <string>Kafkalens</string>
  <key>CFBundleIdentifier</key>
  <string>dev.kafkalens.app</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleExecutable</key>
  <string>Kafkalens</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

if [[ -n "${KAFKALENS_SIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --timestamp \
    --sign "$KAFKALENS_SIGN_IDENTITY" "$APP"
else
  codesign --force --sign - "$APP" >/dev/null
fi
echo "app -> $APP ($(lipo -archs "$APP/Contents/MacOS/Kafkalens"))"

if $INSTALL; then
  # Quit a running copy so the bundle is not replaced underneath it.
  osascript -e 'quit app "Kafkalens"' 2>/dev/null || true
  rm -rf /Applications/Kafkalens.app
  cp -R "$APP" /Applications/
  echo "installed -> /Applications/Kafkalens.app"
fi
