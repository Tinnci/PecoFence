#!/usr/bin/env bash
# Regenerates src-tauri/icons: the large tiles from icons/lune.svg, then the pixel-fitted sizes
# and icon.ico from scripts/icons.py. Needs npx, Python and ImageMagick.
set -euo pipefail
cd "$(dirname "$0")/.."
out=src-tauri/icons
npx tauri icon icons/lune.svg -o "$out" >/dev/null 2>&1
rm -rf "$out/android" "$out/ios" "$out/icon.icns"
python3 scripts/icons.py
