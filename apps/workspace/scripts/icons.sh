#!/usr/bin/env bash
# Regenerates src-tauri/icons from icons/workspace.svg (48 px and up) and
# icons/workspace-small.svg (16, 24 and 32 px). Needs npx and ImageMagick.
set -euo pipefail
cd "$(dirname "$0")/.."
out=src-tauri/icons
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
npx tauri icon icons/workspace.svg -o "$out" >/dev/null 2>&1
npx tauri icon icons/workspace-small.svg -o "$tmp" >/dev/null 2>&1
rm -rf "$out/android" "$out/ios" "$out/icon.icns"
for size in 16 24 32; do
  magick "$tmp/icon.png" -filter Lanczos -resize "${size}x${size}" "$tmp/s$size.png"
done
for size in 48 64 256; do
  magick "$out/icon.png" -filter Lanczos -resize "${size}x${size}" "$tmp/l$size.png"
done
cp "$tmp/s32.png" "$out/32x32.png"
magick "$tmp/s16.png" "$tmp/s24.png" "$tmp/s32.png" "$tmp/l48.png" "$tmp/l64.png" "$tmp/l256.png" "$out/icon.ico"
