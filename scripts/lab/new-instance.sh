#!/usr/bin/env bash
# Create a PecoFence lab instance directory from local WSL cross-builds.
# Usage: scripts/lab/new-instance.sh --name NAME [--fixture PATH] [--seed-config PATH]
#          [--spm-profile release] [--pf-profile release]
#
# --fixture points at the catalog file; its whole PARENT directory is copied
# (a fixture catalog references sibling files such as snapshots/*.json with
# relative paths, so the catalog alone is not runnable).
#
# An instance is a deployment SNAPSHOT: binaries are copied (never linked) from
# the repo target/ trees, and run-manifest.json records the exact provenance
# (commits, pinned spm rev, SHA256 of every deployed file).
# All copies are performed by WSL, so every file lands on NTFS as plaintext
# (DLP only encrypts Windows-process writes; see docs/ENVIRONMENT.md).
set -euo pipefail
cd "$(dirname "$0")/../.."

NAME=""
FIXTURE=""
SEED_CONFIG=""
SPM_PROFILE=release
PF_PROFILE=release
while [ "$#" -gt 0 ]; do
  case "$1" in
    --name)        NAME="$2"; shift 2 ;;
    --fixture)     FIXTURE="$2"; shift 2 ;;
    --seed-config) SEED_CONFIG="$2"; shift 2 ;;
    --spm-profile) SPM_PROFILE="$2"; shift 2 ;;
    --pf-profile)  PF_PROFILE="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done
[ -n "$NAME" ] || { echo "--name is required" >&2; exit 1; }
case "$NAME" in
  */*|*..*|*[\\:]*) echo "instance name must be a plain directory name: $NAME" >&2; exit 1 ;;
esac

SPM_REPO="${SPM_REPO:-$(cd ../spm 2>/dev/null && pwd)}"
[ -d "$SPM_REPO" ] || { echo "spm repo not found next to PecoFence (set SPM_REPO)" >&2; exit 1; }

TRIPLE=x86_64-pc-windows-msvc
LAB="${PFW_LAB_DIR:-/mnt/c/Users/Administrator/PecoFence-lab}"
INST="$LAB/instances/$NAME"
[ -e "$INST" ] && { echo "instance already exists: $INST" >&2; exit 1; }

PF_BIN="$PWD/target/$TRIPLE/$PF_PROFILE"
SPM_BIN="$SPM_REPO/target/$TRIPLE/$SPM_PROFILE"

required=(
  "$PF_BIN/pecofence.exe"
  "$PF_BIN/pecofence-watchdog.exe"
  "third_party/webview2/WebView2Loader.x64.dll"
  "$SPM_BIN/spmd.exe"
  "$SPM_BIN/spm.exe"
)
for f in "${required[@]}"; do
  [ -f "$f" ] || { echo "missing build artifact: $f (run scripts/build-windows.sh first)" >&2; exit 1; }
done

mkdir -p "$INST"/{config,data,logs,appdata/Roaming,appdata/Local} "$LAB/fixtures"

FIXTURE_REL=""
FIXTURE_SHA=""
if [ -n "$FIXTURE" ]; then
  [ -f "$FIXTURE" ] || { echo "fixture catalog not found: $FIXTURE" >&2; exit 1; }
  SRC_DIR="$(cd "$(dirname "$FIXTURE")" && pwd)"
  TARGET_NAME="$(basename "$FIXTURE" .json)"
  FIXTURE_DIR="$LAB/fixtures/$TARGET_NAME"
  [ -e "$FIXTURE_DIR" ] && { echo "fixture target already exists: $FIXTURE_DIR (remove it to re-import)" >&2; exit 1; }
  mkdir -p "$FIXTURE_DIR"
  cp -r "$SRC_DIR"/. "$FIXTURE_DIR"/
  FIXTURE_SHA="$(sha256sum "$FIXTURE_DIR/$(basename "$FIXTURE")" | cut -d" " -f1)"
  FIXTURE_REL="fixtures/$TARGET_NAME/$(basename "$FIXTURE")"
fi

cp "$PF_BIN/pecofence.exe" "$PF_BIN/pecofence-watchdog.exe" "$INST/"
cp third_party/webview2/WebView2Loader.x64.dll "$INST/WebView2Loader.dll"
cp "$SPM_BIN/spmd.exe" "$SPM_BIN/spm.exe" "$INST/"

SEED_NOTE="first-run defaults"
if [ -n "$SEED_CONFIG" ]; then
  [ -f "$SEED_CONFIG" ] || { echo "seed config not found: $SEED_CONFIG" >&2; exit 1; }
  cp "$SEED_CONFIG" "$INST/config/config.json"
  SEED_NOTE="seeded from $SEED_CONFIG"
fi

SPM_SHA="$(git -C "$SPM_REPO" rev-parse HEAD)"
PF_SHA="$(git rev-parse HEAD)"
SPM_REV_PINNED="$(grep -A3 '"spm-contracts"' Cargo.lock | grep -oE "[0-9a-f]{40}" | head -1)"

sha() { sha256sum "$1" | cut -d" " -f1; }
CARGO_LOCK_SHA="$(sha Cargo.lock)"
RUSTC_VERSION="$(rustc -V)"
PROTOCOL_SOURCE="$SPM_REPO/crates/spm-contracts/src"
protocol_version() {
  local key="$1" value
  value="$(grep -rhE "pub const PROTOCOL_${key}:" "$PROTOCOL_SOURCE" | sed -nE 's/.*= ([0-9]+);.*/\1/p' | head -1)"
  [[ "$value" =~ ^[0-9]+$ ]] || { echo "cannot read PROTOCOL_${key} from $PROTOCOL_SOURCE" >&2; exit 1; }
  printf '%s' "$value"
}
PROTOCOL_MAJOR="$(protocol_version MAJOR)"
PROTOCOL_MINOR="$(protocol_version MINOR)"

cat > "$INST/run-manifest.json" <<MANIFEST
{
  "instance": "$NAME",
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "source": "local",
  "spm_commit": "$SPM_SHA",
  "pecofence_commit": "$PF_SHA",
  "spm_rev_pinned_in_pecofence": "$SPM_REV_PINNED",
  "cargo_lock_sha256": "$CARGO_LOCK_SHA",
  "toolchain": { "rustc": "$RUSTC_VERSION", "windows_sdk": "10.0.26100.0", "msvc": "14.44.35207" },
  "wire_protocol": { "major": $PROTOCOL_MAJOR, "minor": $PROTOCOL_MINOR },
  "db_format": "v2",
  "binaries_sha256": {
    "pecofence.exe": "$(sha "$INST/pecofence.exe")",
    "pecofence-watchdog.exe": "$(sha "$INST/pecofence-watchdog.exe")",
    "spmd.exe": "$(sha "$INST/spmd.exe")",
    "spm.exe": "$(sha "$INST/spm.exe")",
    "WebView2Loader.dll": "$(sha "$INST/WebView2Loader.dll")"
  },
  "fixture": { "path": "$FIXTURE_REL", "sha256": "$FIXTURE_SHA" },
  "config": "$SEED_NOTE",
  "profiles": { "spm": "$SPM_PROFILE", "pecofence": "$PF_PROFILE" },
  "start_args": {
    "spmd": ["--ipc-version", "v2", "--fixture", "<lab-relative fixture path resolved by run-instance>"],
    "pecofence": ["--portable", "--no-hide-icons"]
  }
}
MANIFEST

echo "created instance: $INST"
cat "$INST/run-manifest.json"