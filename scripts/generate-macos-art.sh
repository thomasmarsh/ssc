#!/bin/bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
swift -module-cache-path "$work/module-cache" "$root/scripts/generate-macos-art.swift"
mkdir "$work/AppIcon.iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$root/packaging/macos/AppIcon.png" \
        --out "$work/AppIcon.iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$root/packaging/macos/AppIcon.png" \
        --out "$work/AppIcon.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$work/AppIcon.iconset" -o "$root/packaging/macos/AppIcon.icns"
echo "Wrote packaging/macos/AppIcon.icns"
