#!/usr/bin/env bash
# Verify a lab instance against its creation manifest.
# Usage: scripts/lab/verify-instance.sh --name NAME [--lab-dir DIR]
set -euo pipefail

NAME=""
LAB="/mnt/c/Users/Administrator/PecoFence-lab"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --name|--lab-dir)
      [ "$#" -ge 2 ] || { echo "VERIFY-FAILED: missing value for $1"; exit 1; }
      if [ "$1" = --name ]; then NAME="$2"; else LAB="$2"; fi
      shift 2 ;;
    *) echo "VERIFY-FAILED: unknown argument: $1"; exit 1 ;;
  esac
done
[ -n "$NAME" ] || { echo "VERIFY-FAILED: --name is required"; exit 1; }
case "$NAME" in
  */*|*..*|*[\\:]*) echo "VERIFY-FAILED: invalid instance name: $NAME"; exit 1 ;;
esac

PF_REPO="$(cd "$(dirname "$0")/../.." && pwd)"
SPM_REPO="${SPM_REPO:-$PF_REPO/../spm}"
[ -d "$SPM_REPO/.git" ] || { echo "VERIFY-FAILED: spm repository does not exist: $SPM_REPO"; exit 1; }
export PF_REPO SPM_REPO
python3 - "$LAB" "$NAME" <<'PY'
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

lab = Path(sys.argv[1]).resolve()
inst = lab / 'instances' / sys.argv[2]
manifest_path = inst / 'run-manifest.json'

def fail(reason):
    print(f'VERIFY-FAILED: {reason}')
    raise SystemExit(1)

def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()

def required(obj, key, location):
    if not isinstance(obj, dict) or key not in obj or obj[key] is None or obj[key] == '':
        fail(f'missing field: {location}{key}')
    return obj[key]

if not inst.is_dir():
    fail(f'instance does not exist: {inst}')
if not manifest_path.is_file():
    fail(f'missing run-manifest.json: {manifest_path}')
try:
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
except (OSError, UnicodeError, json.JSONDecodeError) as exc:
    fail(f'invalid run-manifest.json: {exc}')

for key in ('instance', 'created_at', 'source', 'spm_commit', 'pecofence_commit',
            'spm_rev_pinned_in_pecofence', 'cargo_lock_sha256', 'db_format',
            'config', 'profiles', 'start_args'):
    required(manifest, key, '')
for key in ('rustc', 'windows_sdk', 'msvc'):
    required(required(manifest, 'toolchain', ''), key, 'toolchain.')
for key in ('major', 'minor'):
    value = required(required(manifest, 'wire_protocol', ''), key, 'wire_protocol.')
    if type(value) is not int:
        fail(f'invalid field: wire_protocol.{key}')
for key in ('spm', 'pecofence'):
    required(manifest['profiles'], key, 'profiles.')
for key in ('spmd', 'pecofence'):
    required(manifest['start_args'], key, 'start_args.')

binaries = required(manifest, 'binaries_sha256', '')
if not isinstance(binaries, dict) or not binaries:
    fail('invalid binaries_sha256')
for filename, expected in binaries.items():
    if not isinstance(filename, str) or Path(filename).name != filename or filename in ('.', '..'):
        fail(f'invalid binary path: {filename}')
    if not isinstance(expected, str) or not re.fullmatch(r'[0-9a-fA-F]{64}', expected):
        fail(f'invalid binary sha256: {filename}')
    path = inst / filename
    if not path.is_file():
        fail(f'missing binary: {filename}')
    if sha256(path) != expected.lower():
        fail(f'binary sha256 mismatch: {filename}')

fixture = required(manifest, 'fixture', '')
fixture_path = required(fixture, 'path', 'fixture.')
fixture_sha = required(fixture, 'sha256', 'fixture.')
if not isinstance(fixture_path, str) or not isinstance(fixture_sha, str):
    fail('invalid fixture fields')
path = (lab / fixture_path).resolve()
if not path.is_relative_to(lab) or not path.is_file():
    fail(f'fixture does not exist: {fixture_path}')
if sha256(path) != fixture_sha.lower():
    fail(f'fixture sha256 mismatch: {fixture_path}')

for key, repo in (('spm_commit', os.environ['SPM_REPO']),
                  ('pecofence_commit', os.environ['PF_REPO']),
                  ('spm_rev_pinned_in_pecofence', os.environ['SPM_REPO'])):
    commit = manifest[key]
    if not isinstance(commit, str) or not re.fullmatch(r'[0-9a-fA-F]{40}', commit):
        fail(f'invalid commit: {key}')
    result = subprocess.run(['git', '-C', repo, 'cat-file', '-e', f'{commit}^{{commit}}'],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if result.returncode:
        fail(f'commit does not exist: {key} ({commit})')
print('VERIFY-OK')
PY
