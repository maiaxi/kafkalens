#!/usr/bin/env bash
# Regenerates the app icons from scripts/make-icon.swift:
#   macos/Resources/AppIcon.icns (+ AppIcon.png)   macOS
#   desktop/assets/kafkalens.ico, kafkalens.png    Windows / Linux
# Only needed after changing the drawing; the outputs are committed.
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

# Windows and Linux icons fill more of their square than macOS ones, so drop
# the margin the macOS grid leaves around the body (824 of 1024 points).
ASSETS="$ROOT/desktop/assets"
mkdir -p "$ASSETS"
sips --cropToHeightWidth 860 860 "$WORK/icon-1024.png" --out "$WORK/body.png" >/dev/null
sips -z 512 512 "$WORK/body.png" --out "$ASSETS/kafkalens.png" >/dev/null
for size in 16 24 32 48 64 128 256; do
  sips -z $size $size "$WORK/body.png" --out "$WORK/ico-$size.png" >/dev/null
done
# An .ico is a small directory of images; since Windows Vista each entry
# can simply be a PNG.
python3 - "$WORK" "$ASSETS/kafkalens.ico" <<'PY'
import struct, sys
work, out = sys.argv[1], sys.argv[2]
sizes = [16, 24, 32, 48, 64, 128, 256]
images = [open(f"{work}/ico-{s}.png", "rb").read() for s in sizes]
offset = 6 + 16 * len(sizes)
header = struct.pack("<HHH", 0, 1, len(sizes))
entries = b""
for size, data in zip(sizes, images):
    dim = 0 if size == 256 else size  # 0 means 256
    entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
with open(out, "wb") as f:
    f.write(header + entries + b"".join(images))
PY
echo "icon -> desktop/assets/kafkalens.ico, kafkalens.png"
