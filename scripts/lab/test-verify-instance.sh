#!/usr/bin/env bash
# Local-only negative test. Requires scripts/build-windows.sh to have run on this
# machine and MSVC target artifacts to exist in the repository target directories.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

LAB="/tmp/pf-verify-test-$$"
NAME="verify-test"
INST="$LAB/instances/$NAME"
FIXTURE="/home/enterp/dev/spm/crates/spm-contracts/fixtures/catalog.json"
passes=0
failures=0
cleanup() { rm -rf "$LAB"; }
trap cleanup EXIT

pass() { printf 'PASS: %s\n' "$1"; passes=$((passes + 1)); }
fail() { printf 'FAIL: %s\n' "$1"; failures=$((failures + 1)); }
expect_verify() {
  local label="$1" expected="$2" reason="${3:-}" output status
  output="$(scripts/lab/verify-instance.sh --name "$NAME" --lab-dir "$LAB" 2>&1)"
  status=$?
  if [ "$expected" = ok ] && [ "$status" -eq 0 ] && [[ "$output" == *VERIFY-OK* ]]; then
    pass "$label"
  elif [ "$expected" = fail ] && [ "$status" -ne 0 ] && [[ "$output" == *"VERIFY-FAILED: $reason"* ]]; then
    pass "$label"
  else
    fail "$label (exit $status: $output)"
  fi
}

if ! PFW_LAB_DIR="$LAB" scripts/lab/new-instance.sh --name "$NAME" --fixture "$FIXTURE" > "$LAB-create.log" 2>&1; then
  fail "create instance ($(cat "$LAB-create.log"))"
  rm -f "$LAB-create.log"
  printf 'SUMMARY: %d PASS, %d FAIL\n' "$passes" "$failures"
  exit 1
fi
rm -f "$LAB-create.log"
expect_verify 'original instance' ok

cp "$INST/pecofence.exe" "$LAB/pecofence.exe.backup"
printf x >> "$INST/pecofence.exe"
expect_verify 'modified pecofence.exe rejected' fail 'binary sha256 mismatch: pecofence.exe'
cp "$LAB/pecofence.exe.backup" "$INST/pecofence.exe"

cp "$INST/run-manifest.json" "$LAB/manifest.backup"
python3 - "$INST/run-manifest.json" <<'PY'
import json
import pathlib
import sys
path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
data['spm_commit'] = '0' * 40
path.write_text(json.dumps(data, indent=2) + '\n')
PY
expect_verify 'invalid spm_commit rejected' fail 'commit does not exist: spm_commit'
cp "$LAB/manifest.backup" "$INST/run-manifest.json"

fixture_copy="$LAB/fixtures/catalog/catalog.json"
cp "$fixture_copy" "$LAB/fixture.backup"
printf x >> "$fixture_copy"
expect_verify 'modified fixture rejected' fail 'fixture sha256 mismatch:'
cp "$LAB/fixture.backup" "$fixture_copy"

printf 'SUMMARY: %d PASS, %d FAIL\n' "$passes" "$failures"
[ "$failures" -eq 0 ] && [ "$passes" -eq 4 ]
