#!/usr/bin/env bash
# Compile the render crate's HLSL pixel shaders with the Windows SDK fxc, from WSL.
# Writes crates/render/shaders/<name>.cso and <name>.hlsl.fnv (the source hash a unit test
# compares against the source, so an edited shader cannot ship with a stale binary).
set -euo pipefail
cd "$(dirname "$0")/.."

SDK_ROOT="${PFW_SDK_ROOT:-/mnt/c/Program Files (x86)/Windows Kits/10}"
SDK_VER="${PFW_WINDOWS_SDK_VER:-10.0.26100.0}"
FXC="$SDK_ROOT/bin/$SDK_VER/x64/fxc.exe"
[ -x "$FXC" ] || { echo "missing fxc: $FXC" >&2; exit 1; }

# fxc runs on Windows: stage the sources in a Windows temp directory.
WIN_TMP="$(wslpath "$(cmd.exe /c 'echo %TEMP%' 2>/dev/null | tr -d '\r')")/pecofence-shaders"
mkdir -p "$WIN_TMP"

for src in crates/render/shaders/*.hlsl; do
  name="$(basename "$src" .hlsl)"
  cp "$src" "$WIN_TMP/$name.hlsl"
  "$FXC" /nologo /T ps_4_0 /E main /O3 /Qstrip_reflect /Qstrip_debug \
    /Fo "$(wslpath -w "$WIN_TMP/$name.cso")" "$(wslpath -w "$WIN_TMP/$name.hlsl")"
  cp "$WIN_TMP/$name.cso" "crates/render/shaders/$name.cso"
  python3 - "$src" > "crates/render/shaders/$name.hlsl.fnv" <<'PY'
import sys
data = open(sys.argv[1], "rb").read().replace(b"\r\n", b"\n")
h = 0xcbf29ce484222325
for b in data:
    h = ((h ^ b) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
print(f"{h:016x}")
PY
  echo "compiled $src -> crates/render/shaders/$name.cso ($(stat -c %s "crates/render/shaders/$name.cso") bytes)"
done
