#!/usr/bin/env bash
# WSL cross-build against the same installed MSVC/SDK as PecoFence.
set -euo pipefail
cd "$(dirname "$0")/../../.."
workspace_sdk="${PFW_SDK_ROOT:-/mnt/c/Program Files (x86)/Windows Kits/10}"
workspace_sdk_version="${PFW_WINDOWS_SDK_VER:-10.0.26100.0}"
workspace_msvc="${PFW_MSVC_ROOT:-/mnt/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207}"
for directory in "$workspace_msvc/lib/x64" "$workspace_sdk/Lib/$workspace_sdk_version/ucrt/x64" "$workspace_sdk/Lib/$workspace_sdk_version/um/x64"; do
  test -d "$directory" || { echo 'Required MSVC or Windows SDK directory missing' >&2; exit 1; }
done
export LIB="$workspace_msvc/lib/x64;$workspace_sdk/Lib/$workspace_sdk_version/ucrt/x64;$workspace_sdk/Lib/$workspace_sdk_version/um/x64"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=lld-link
export RC=llvm-rc
npm ci --prefix apps/workspace --ignore-scripts
npm run build --prefix apps/workspace
cargo build --locked --release --manifest-path apps/workspace/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
