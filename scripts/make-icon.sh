#!/usr/bin/env bash
# Regenerates macos/Resources/AppIcon.icns from scripts/make-icon.swift.
# Only needed after changing the drawing; the .icns is committed.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

swift "$ROOT/scripts/make-icon.swift" "$WORK/icon-1024.png"

SET="$WORK/AppIcon.iconset"
mkdir -p "$SET"
for size in 16 32 128 256 512; do
  sips -z $size $size "$WORK/icon-1024.png" --out "$SET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z $double $double "$WORK/icon-1024.png" --out "$SET/icon_${size}x${size}@2x.png" >/dev/null
done

mkdir -p "$ROOT/macos/Resources"
iconutil -c icns "$SET" -o "$ROOT/macos/Resources/AppIcon.icns"
cp "$WORK/icon-1024.png" "$ROOT/macos/Resources/AppIcon.png"
echo "icon -> macos/Resources/AppIcon.icns"
