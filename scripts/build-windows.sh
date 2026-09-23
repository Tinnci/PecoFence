#!/usr/bin/env bash
# Cross-build Windows PE artifacts (x86_64-pc-windows-msvc) from WSL.
# Output stays in this repo ext4 target/: target/x86_64-pc-windows-msvc/<profile-dir>/
# Usage: scripts/build-windows.sh [profile] [package ...]   (default: release pecofence pecofence-watchdog)
#
# Toolchain: clang-cl (C deps) + lld-link (final link) against the Windows SDK and
# MSVC CRT libraries mounted at /mnt/c. The linker is selected through
# CARGO_TARGET_*_LINKER env only, so the Windows CI (link.exe) is unaffected.
# crt-static is applied by .cargo/config.toml (shared with the Windows CI).
set -euo pipefail
cd "$(dirname "$0")/.."

TRIPLE=x86_64-pc-windows-msvc
PROFILE="${1:-release}"
if [ "$#" -gt 0 ]; then shift; fi
PACKAGES=("$@")
if [ "${#PACKAGES[@]}" -eq 0 ]; then
  PACKAGES=(pecofence pecofence-watchdog)
fi

# Defense: an ambient CARGO_TARGET_DIR (e.g. the retired Windows user variable)
# must never redirect builds out of this repo target/ tree.
unset CARGO_TARGET_DIR

SDK_ROOT="${PFW_SDK_ROOT:-/mnt/c/Program Files (x86)/Windows Kits/10}"
SDK_VER="${PFW_WINDOWS_SDK_VER:-10.0.26100.0}"
MSVC_ROOT="${PFW_MSVC_ROOT:-/mnt/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207}"

for d in "$MSVC_ROOT/lib/x64" "$SDK_ROOT/Lib/$SDK_VER/ucrt/x64" "$SDK_ROOT/Lib/$SDK_VER/um/x64"; do
  [ -d "$d" ] || { echo "missing toolchain dir: $d" >&2; exit 1; }
done

export CC_x86_64_pc_windows_msvc=clang-cl-19
export AR_x86_64_pc_windows_msvc=llvm-lib
export CC_SHELL_ESCAPED_FLAGS=1
export CFLAGS_x86_64_pc_windows_msvc="-imsvc\"$MSVC_ROOT/include\" -imsvc\"$SDK_ROOT/Include/$SDK_VER/ucrt\" -imsvc\"$SDK_ROOT/Include/$SDK_VER/shared\" -imsvc\"$SDK_ROOT/Include/$SDK_VER/um\""
# lld-link resolves CRT/import libraries through LIB (Windows-style semicolon separator).
export LIB="$MSVC_ROOT/lib/x64;$SDK_ROOT/Lib/$SDK_VER/ucrt/x64;$SDK_ROOT/Lib/$SDK_VER/um/x64"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=lld-link

case "$PROFILE" in
  release) PROFILE_DIR=release ;;
  dev)     PROFILE_DIR=debug ;;
  *)       PROFILE_DIR="$PROFILE" ;;
esac

echo "repo     : $(pwd)"
echo "commit   : $(git rev-parse HEAD)"
echo "branch   : $(git rev-parse --abbrev-ref HEAD)"
echo "spm rev  : $(grep -A2 'name = \"spm-contracts\"' Cargo.lock | grep rev | head -1)"
echo "triple   : $TRIPLE  profile: $PROFILE"
echo "packages : ${PACKAGES[*]}"
echo "linker   : lld-link"
echo

pkg_flags=()
for p in "${PACKAGES[@]}"; do pkg_flags+=(-p "$p"); done
cargo build --locked --target "$TRIPLE" --profile "$PROFILE" "${pkg_flags[@]}"

echo
echo "artifacts under target/$TRIPLE/$PROFILE_DIR:"
for p in "${PACKAGES[@]}"; do
  ls -la "target/$TRIPLE/$PROFILE_DIR/$p.exe" 2>/dev/null || true
done
echo "runtime dll for deployment: third_party/webview2/WebView2Loader.x64.dll"