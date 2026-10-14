#!/usr/bin/env bash
# Builds the Rust core as a static library and generates the Swift bindings
# the macOS app links against:
#
#   macos/RustLib/KafkalensCoreFFI.xcframework   static lib + C header/modulemap
#   macos/RustLib/KafkalensCore/kafkalens_core.swift
#
# Usage: scripts/build-core.sh [debug|release] [--universal]
#   --universal   build arm64 + x86_64 (for releases); default is this Mac's arch
set -euo pipefail

PROFILE=release
UNIVERSAL=false
for arg in "$@"; do
  case "$arg" in
    debug|release) PROFILE="$arg" ;;
    --universal) UNIVERSAL=true ;;
    *) echo "unknown option: $arg" >&2; exit 1 ;;
  esac
done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/macos/RustLib"
GEN="$ROOT/target/uniffi-swift"
cd "$ROOT"

if $UNIVERSAL; then
  ARCHS=(arm64 x86_64)
else
  ARCHS=("$(uname -m)")
fi

CARGO_PROFILE=()
[[ "$PROFILE" == "release" ]] && CARGO_PROFILE=(--release)

LIBS=()
for arch in "${ARCHS[@]}"; do
  case "$arch" in
    arm64) triple=aarch64-apple-darwin ;;
    x86_64) triple=x86_64-apple-darwin ;;
  esac
  rustup target add "$triple" >/dev/null 2>&1
  # librdkafka is built by its own configure script, which ignores cargo's
  # target; the -arch flags make it compile for the right CPU.
  CFLAGS="-arch $arch" LDFLAGS="-arch $arch" \
    cargo build ${CARGO_PROFILE[@]+"${CARGO_PROFILE[@]}"} -p kafkalens-core --lib --target "$triple"
  LIBS+=("$ROOT/target/$triple/$PROFILE/libkafkalens_core.a")
done

# Bindings come from the library metadata, which is identical on every arch.
rm -rf "$GEN"
cargo run --quiet -p kafkalens-core --features bindgen --bin uniffi-bindgen -- \
  generate --library "${LIBS[0]%.a}.dylib" \
  --language swift --out-dir "$GEN"

# The XCFramework is assembled by hand rather than with
# `xcodebuild -create-xcframework`, so only the Command Line Tools are needed.
SLICE="macos-$(IFS=_; echo "${ARCHS[*]}")"
XCF="$OUT/KafkalensCoreFFI.xcframework"

rm -rf "$OUT"
mkdir -p "$XCF/$SLICE/Headers" "$OUT/KafkalensCore"
lipo -create "${LIBS[@]}" -output "$XCF/$SLICE/libkafkalens_core.a"
cp "$GEN/kafkalens_coreFFI.h" "$XCF/$SLICE/Headers/"
cp "$GEN/kafkalens_coreFFI.modulemap" "$XCF/$SLICE/Headers/module.modulemap"
cp "$GEN/kafkalens_core.swift" "$OUT/KafkalensCore/"

ARCH_ENTRIES=""
for arch in "${ARCHS[@]}"; do
  ARCH_ENTRIES+="        <string>$arch</string>"$'\n'
done

cat > "$XCF/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>AvailableLibraries</key>
  <array>
    <dict>
      <key>HeadersPath</key>
      <string>Headers</string>
      <key>LibraryIdentifier</key>
      <string>$SLICE</string>
      <key>LibraryPath</key>
      <string>libkafkalens_core.a</string>
      <key>SupportedArchitectures</key>
      <array>
${ARCH_ENTRIES}      </array>
      <key>SupportedPlatform</key>
      <string>macos</string>
    </dict>
  </array>
  <key>CFBundlePackageType</key>
  <string>XFWK</string>
  <key>XCFrameworkFormatVersion</key>
  <string>1.0</string>
</dict>
</plist>
PLIST

echo "core ($PROFILE, ${ARCHS[*]}) -> $OUT"
