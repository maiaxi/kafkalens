#!/usr/bin/env bash
# Builds the core and the SwiftUI app in release mode and packages
# dist/Kafkalens.app.
#
# Usage: scripts/bundle-app.sh [--universal] [--install]
#   --universal   arm64 + x86_64 (what releases ship); default is this Mac's arch
#   --install     also copy the app to /Applications, replacing any older copy
#
# Signing: ad-hoc by default (fine for local use). Set KAFKALENS_SIGN_IDENTITY
# to sign with a certificate from the keychain, with the hardened runtime.
# Releases use a self-signed one: Gatekeeper still treats the app as from an
# unidentified developer, but the signature stays the same across versions,
# so Keychain "Always Allow" survives updates. A "Developer ID Application"
# identity also gets a secure timestamp, which notarization requires.
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
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
APP="$ROOT/dist/Kafkalens.app"

if $UNIVERSAL; then
  "$ROOT/scripts/build-core.sh" release --universal
  ARCHS=(arm64 x86_64)
else
  "$ROOT/scripts/build-core.sh" release
  ARCHS=("$(uname -m)")
fi

# One build per architecture, merged with lipo. `swift build --arch a --arch b`
# switches to the Xcode build system, which fails to find the bindings
# module on some Xcode versions.
cd "$ROOT/macos"
BINS=()
for arch in "${ARCHS[@]}"; do
  triple="$arch-apple-macosx14.0"
  swift build -c release --triple "$triple"
  # Copied right away: some toolchains use one output path for every triple.
  cp "$(swift build -c release --triple "$triple" --show-bin-path)/Kafkalens" "$ROOT/macos/.build/Kafkalens-$arch"
  BINS+=("$ROOT/macos/.build/Kafkalens-$arch")
done
BIN="$ROOT/macos/.build/Kafkalens-$(IFS=-; echo "${ARCHS[*]}")"
lipo -create "${BINS[@]}" -output "$BIN"

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
  TIMESTAMP=()
  [[ "$KAFKALENS_SIGN_IDENTITY" == "Developer ID"* ]] && TIMESTAMP=(--timestamp)
  codesign --force --options runtime ${TIMESTAMP[@]+"${TIMESTAMP[@]}"} \
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
