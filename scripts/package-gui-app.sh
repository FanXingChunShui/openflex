#!/usr/bin/env bash
# Builds a double-clickable OpenFlex.app (macOS) around the openflex-gui binary.
#
# Usage: scripts/package-gui-app.sh [--target <triple>] [--out <dir>] [--binary <path>]
#   --target  cargo target triple (default: the host triple)
#   --out     output directory for OpenFlex.app and the zip (default: dist/gui)
#   --binary  pre-built openflex-gui binary (skips the cargo build when given)
set -euo pipefail

target=""
out="dist/gui"
binary=""

while [ $# -gt 0 ]; do
    case "$1" in
        --target) target="${2:?--target needs a value}"; shift 2 ;;
        --out) out="${2:?--out needs a value}"; shift 2 ;;
        --binary) binary="${2:?--binary needs a value}"; shift 2 ;;
        -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [ -z "$target" ]; then
    target="$(rustc -vV | sed -n 's/^host: //p')"
fi

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
echo "packaging OpenFlex.app ${version} (${target})"

if [ -z "$binary" ]; then
    CARGO_INCREMENTAL=0 cargo build --release -p openflex-gui --target "$target" --locked
    binary="target/${target}/release/openflex-gui"
fi
if [ ! -x "$binary" ]; then
    echo "binary not found: $binary" >&2
    exit 1
fi

app="${out}/OpenFlex.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/openflex-gui"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>OpenFlex</string>
    <key>CFBundleDisplayName</key>
    <string>OpenFlex</string>
    <key>CFBundleIdentifier</key>
    <string>io.openflex.gui</string>
    <key>CFBundleVersion</key>
    <string>${version}</string>
    <key>CFBundleShortVersionString</key>
    <string>${version}</string>
    <key>CFBundleExecutable</key>
    <string>openflex-gui</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>OpenFlex model</string>
            <key>CFBundleTypeExtensions</key>
            <array><string>ofm</string></array>
            <key>CFBundleTypeRole</key>
            <string>Editor</string>
        </dict>
    </array>
</dict>
</plist>
PLIST

# Ad-hoc signature so Gatekeeper lets a locally built bundle start.
if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - "$app" >/dev/null 2>&1 || \
        echo "warning: ad-hoc codesign failed (the app still runs via 'open')" >&2
fi

zip_path="${out}/OpenFlex-${version}-${target}.app.zip"
rm -f "$zip_path"
( cd "$out" && zip -qry "$(basename "$zip_path")" "OpenFlex.app" )

echo "app:  $app"
echo "zip:  $zip_path"
echo "open: open '$app'"
