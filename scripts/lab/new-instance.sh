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
# (independent commits, public contract provenance, SHA256 of deployed files).
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
if [ -n "$SEED_CONFIG" ]; then
  [ -f "$SEED_CONFIG" ] || { echo "seed config not found: $SEED_CONFIG" >&2; exit 1; }
  # Validate the exact workspace format before creating directories/copying artifacts.
  cargo run --quiet --locked -p pecofence-core --example validate_workspace -- "$SEED_CONFIG"
fi

[ -n "${SPM_REPO:-}" ] || { echo "authorized backend integration requires explicit SPM_REPO" >&2; exit 1; }
export SPM_REPO
[ -d "$SPM_REPO" ] || { echo "spm repo not found: $SPM_REPO" >&2; exit 1; }
CONTRACTS="$(scripts/lab/verify-instance.sh --contract-provenance)"

TRIPLE=x86_64-pc-windows-msvc
LAB="${PFW_LAB_DIR:-/mnt/c/Users/Administrator/PecoFence-lab}"
INST="$LAB/instances/$NAME"
[ -e "$INST" ] && { echo "instance already exists: $INST" >&2; exit 1; }

PF_BIN="$PWD/target/$TRIPLE/$PF_PROFILE"
SPM_BIN="$SPM_REPO/target/$TRIPLE/$SPM_PROFILE"

required=(
  "$PF_BIN/pecofence.exe"
  "$PF_BIN/pecofence-watchdog.exe"
  "$SPM_BIN/spmd.exe"
  "$SPM_BIN/spm.exe"
)
for f in "${required[@]}"; do
  [ -f "$f" ] || { echo "missing build artifact: $f (run scripts/build-windows.sh first)" >&2; exit 1; }
done
python3 scripts/stage-winappsdk-runtime.py verify --directory "$PF_BIN"

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
python3 scripts/stage-winappsdk-runtime.py copy --source "$PF_BIN" --destination "$INST"
cp "$SPM_BIN/spmd.exe" "$SPM_BIN/spm.exe" "$INST/"

SEED_NOTE="first-run defaults"
if [ -n "$SEED_CONFIG" ]; then
  [ -f "$SEED_CONFIG" ] || { echo "seed config not found: $SEED_CONFIG" >&2; exit 1; }
  cp "$SEED_CONFIG" "$INST/config/workspace.v2.json"
  SEED_NOTE="seeded from $SEED_CONFIG"
fi

SPM_SHA="$(git -C "$SPM_REPO" rev-parse HEAD)"
PF_SHA="$(git rev-parse HEAD)"

sha() { sha256sum "$1" | cut -d" " -f1; }
CARGO_LOCK_SHA="$(sha Cargo.lock)"
RUSTC_VERSION="$(rustc -V)"
export INST NAME SPM_SHA PF_SHA CONTRACTS CARGO_LOCK_SHA RUSTC_VERSION
export FIXTURE_REL FIXTURE_SHA SEED_NOTE SPM_PROFILE PF_PROFILE
python3 - <<'PY'
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path.cwd() / 'scripts'))
import winappsdk_runtime as runtime
env = os.environ
inst = Path(env['INST'])
contracts = json.loads(env['CONTRACTS'])
runtime_manifest = runtime.load_runtime_manifest()
runtime_files, _ = runtime.validate_runtime_directory(inst, runtime_manifest)
manifest = {
    'schema_version': 4, 'instance': env['NAME'],
    'created_at': datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
    'source': 'local', 'spm_commit': env['SPM_SHA'], 'pecofence_commit': env['PF_SHA'],
    'contracts': contracts, 'cargo_lock_sha256': env['CARGO_LOCK_SHA'],
    'toolchain': {'rustc': env['RUSTC_VERSION'], 'windows_sdk': '10.0.26100.0', 'msvc': '14.44.35207'},
    'wire_protocol': contracts['wire_protocol'], 'db_format': 'v2',
    'binaries_sha256': {
        name: hashlib.sha256((inst / name).read_bytes()).hexdigest()
        for name in ('pecofence.exe', 'pecofence-watchdog.exe', 'spmd.exe', 'spm.exe')
    },
    'windows_app_sdk': {
        'version': runtime.PACKAGE_VERSION,
        'manifest_sha256': runtime.manifest_digest(runtime_manifest),
        'files_sha256': {
            name: hashlib.sha256(data).hexdigest()
            for name, data in sorted(runtime_files.items())
        },
    },
    'fixture': {'path': env['FIXTURE_REL'], 'sha256': env['FIXTURE_SHA']},
    'config': env['SEED_NOTE'],
    'profiles': {'spm': env['SPM_PROFILE'], 'pecofence': env['PF_PROFILE']},
    'start_args': {
        'spmd': ['--ipc-version', 'v2'] + (
            ['--fixture', '<lab-relative fixture path resolved by run-instance>'] if env['FIXTURE_REL'] else []),
        'pecofence': ['--portable', '--no-hide-icons'],
    },
}
(inst / 'run-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
PY

echo "created instance: $INST"
cat "$INST/run-manifest.json"