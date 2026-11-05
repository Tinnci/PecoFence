#!/usr/bin/env bash
# Overwrites the active install with the current Windows build. While a window runs from the
# exe it is moved aside as `.old`, and the next Workspace start removes it. Two version folders
# stay; older ones go to the Recycle Bin.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=/mnt/c/Users/Administrator/AppData/Local/Tinnci/Workspace/versions
TARGET="${1:-$ROOT/$(ls -t "$ROOT" | head -1)}/tinnci-workspace.exe"
BUILT=src-tauri/target/x86_64-pc-windows-msvc/release/tinnci-workspace.exe
if ! cp "$BUILT" "$TARGET" 2>/dev/null; then
  rm -f "$TARGET.old"
  [ -e "$TARGET.old" ] && mv "$TARGET.old" "$TARGET.old$(date +%s)"
  mv "$TARGET" "$TARGET.old"
  cp "$BUILT" "$TARGET"
fi
cmp "$BUILT" "$TARGET"
rm -f "$TARGET".old* 2>/dev/null || true
PRUNE='C:\Users\Administrator\AppData\Local\Tinnci\SPM\prune-versions.ps1'
KEEP=$(basename "$(dirname "$TARGET")")
(cd /mnt/c/Windows && powershell.exe -NoProfile -NonInteractive -File "$PRUNE" \
  -Root "$(wslpath -w "$ROOT")" -Keep 2 -Protect "$KEEP")
echo "installed $TARGET"
