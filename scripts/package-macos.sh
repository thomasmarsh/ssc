#!/bin/bash
# Build an SSC app bundle and drag-to-Applications disk image.
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: ./scripts/package-macos.sh [--target TRIPLE] [--app-only] [--no-finder]

Defaults to the Rust toolchain's host architecture.
  --target TRIPLE  aarch64-apple-darwin or x86_64-apple-darwin
  --app-only       Build SSC.app without creating a DMG
  --no-finder      Create a DMG without Finder layout (for headless builds)

Outputs: target/package/macos/TRIPLE/SSC.app and SSC-VERSION-ARCH.dmg
Requires Rust, Xcode Command Line Tools, and create-dmg (brew install create-dmg).
EOF
}

fail() { echo "Error: $*" >&2; exit 1; }
target=""
app_only=0
no_finder=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --target)
            [[ $# -ge 2 ]] || fail '--target needs a triple'
            target="$2"; shift 2 ;;
        --app-only) app_only=1; shift ;;
        --no-finder) no_finder=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; fail "Unknown argument: $1" ;;
    esac
done

[[ $(uname -s) == Darwin ]] || fail 'macOS is required to package SSC'
for tool in cargo rustc codesign plutil; do
    command -v "$tool" >/dev/null || fail "Missing required tool: $tool"
done
if [[ $app_only == 0 ]]; then
    command -v create-dmg >/dev/null || fail 'Install create-dmg: brew install create-dmg'
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
[[ -n "$target" ]] || target="$(rustc -vV | sed -n 's/^host: //p')"
case "$target" in
    aarch64-apple-darwin) arch=arm64 ;;
    x86_64-apple-darwin) arch=x86_64 ;;
    *) fail "Unsupported macOS target: $target" ;;
esac
pkgid="$(cargo pkgid --locked --package ssc)"
version="${pkgid##*#}"
version="${version##*@}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "Expected a release version, got $version"
art="$root/packaging/macos"
for asset in AppIcon.icns DMG-background.png Info.plist; do
    [[ -f "$art/$asset" ]] || fail "Missing packaging asset: $art/$asset"
done

# Explicit target and target directory keep Cargo configuration/environment overrides
# from causing us to package an unrelated or stale binary.
cargo build --release --locked --package ssc --bin ssc --features desktop \
    --target "$target" --target-dir "$root/target"
out="$root/target/package/macos/$target"
mkdir -p "$out"
stage="$(mktemp -d "$out/.stage.XXXXXX")"
dmg_work=""
cleanup() {
    rm -rf "$stage"
    if [[ -n "$dmg_work" ]]; then rm -rf "$dmg_work"; fi
}
trap cleanup EXIT
app="$stage/SSC.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$root/target/$target/release/ssc" "$app/Contents/MacOS/ssc"
chmod 755 "$app/Contents/MacOS/ssc"
cp "$art/AppIcon.icns" "$app/Contents/Resources/AppIcon.icns"
cp "$root/LICENSE" "$app/Contents/Resources/LICENSE"
cp "$art/Info.plist" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
# Local ad-hoc signing supports Apple Silicon launch; public distribution needs
# a Developer ID signature and notarization separately.
codesign --force --sign - "$app"
codesign --verify --strict "$app"

if [[ $app_only == 0 ]]; then
    dmg="SSC-$version-$arch.dmg"
    # Keep create-dmg's writable intermediate image out of the source folder,
    # and clean it up along with the final temporary image on failure.
    dmg_work="$(mktemp -d "$out/.dmg.XXXXXX")"
    dmg_temp="$dmg_work/$dmg"
    extra=()
    if [[ $no_finder == 1 ]]; then extra+=(--skip-jenkins); fi
    create-dmg \
        --volname 'SSC' \
        --volicon "$art/AppIcon.icns" \
        --background "$art/DMG-background.png" \
        --window-size 800 400 \
        --icon-size 128 \
        --text-size 14 \
        --icon 'SSC.app' 200 190 \
        --hide-extension 'SSC.app' \
        --app-drop-link 600 190 \
        ${extra[@]+"${extra[@]}"} \
        "$dmg_temp" "$stage"
    mv -f "$dmg_temp" "$out/$dmg"
    echo "DMG: $out/$dmg"
fi
rm -rf "$out/SSC.app"
mv "$app" "$out/SSC.app"
echo "App: $out/SSC.app"
