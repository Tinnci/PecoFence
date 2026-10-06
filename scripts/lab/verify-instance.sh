#!/usr/bin/env bash
# Verify a lab instance against its creation manifest.
# Usage: scripts/lab/verify-instance.sh --name NAME [--lab-dir DIR]
set -euo pipefail

NAME=""
PROVENANCE_ONLY=0
LAB="/mnt/c/Users/Administrator/PecoFence-lab"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --contract-provenance) PROVENANCE_ONLY=1; shift ;;
    --name|--lab-dir)
      [ "$#" -ge 2 ] || { echo "VERIFY-FAILED: missing value for $1"; exit 1; }
      if [ "$1" = --name ]; then NAME="$2"; else LAB="$2"; fi
      shift 2 ;;
    *) echo "VERIFY-FAILED: unknown argument: $1"; exit 1 ;;
  esac
done
[ -n "$NAME" ] || [ "$PROVENANCE_ONLY" -eq 1 ] || { echo "VERIFY-FAILED: --name is required"; exit 1; }
case "$NAME" in
  */*|*..*|*[\\:]*) echo "VERIFY-FAILED: invalid instance name: $NAME"; exit 1 ;;
esac

PF_REPO="$(cd "$(dirname "$0")/../.." && pwd)"
[ -n "${SPM_REPO:-}" ] || { echo "VERIFY-FAILED: authorized backend integration requires explicit SPM_REPO"; exit 1; }
[ -e "$SPM_REPO/.git" ] || { echo "VERIFY-FAILED: spm repository does not exist: $SPM_REPO"; exit 1; }
export PF_REPO SPM_REPO
python3 - "$LAB" "$NAME" "$PROVENANCE_ONLY" <<'PY'
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
if sys.version_info < (3, 11):
    raise SystemExit('VERIFY-FAILED: Python >=3.11 is required')
import tomllib

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

def provenance():
    pf = Path(os.environ['PF_REPO']).resolve()
    backend = Path(os.environ['SPM_REPO']).resolve()
    contracts = pf / 'crates/spm-contracts'
    try:
        package = tomllib.loads((contracts / 'Cargo.toml').read_text())['package']
        if package.get('name') != 'spm-contracts':
            fail('unexpected public contract package identity')
        workspace = tomllib.loads((backend / 'Cargo.toml').read_text())
        dependency = workspace['workspace']['dependencies']['spm-contracts']
        files = [contracts / 'Cargo.toml', *sorted((contracts / 'src').rglob('*'))]
        hashes = {str(p.relative_to(contracts)).replace('\\', '/'): sha256(p)
                  for p in files if p.is_file()}
        if not isinstance(dependency, dict):
            fail('backend must declare an exact public git or same-checkout path contract dependency')
        if dependency.get('version') != '=' + package['version']:
            fail('backend public contract version must exactly match public package version')
        if 'branch' in dependency or 'tag' in dependency or ('path' in dependency and 'git' in dependency):
            fail('ambiguous backend contract dependency')
        if 'path' in dependency:
            if (backend / dependency['path']).resolve() != contracts:
                fail('backend contract path does not resolve to this public checkout')
            source = {'kind': 'path', 'path': dependency['path']}
        else:
            rev = dependency.get('rev', '')
            if not re.fullmatch('[0-9a-f]{40}', rev) or not dependency.get('git'):
                fail('backend public contract dependency requires an exact git revision')
            if dependency['git'].removesuffix('.git') != 'https://github.com/Tinnci/PecoFence':
                fail('backend contract git source is not the public PecoFence repository')
            source = {'kind': 'git', 'git': dependency['git'], 'rev': rev}
            for name, expected in hashes.items():
                result = subprocess.run(
                    ['git', '-C', str(pf), 'show', f'{rev}:crates/spm-contracts/{name}'],
                    capture_output=True)
                if result.returncode or hashlib.sha256(result.stdout).hexdigest() != expected:
                    fail(f'backend public contract revision byte mismatch: {name}')
            tree = subprocess.run(
                ['git', '-C', str(pf), 'ls-tree', '-r', '--name-only', rev,
                 'crates/spm-contracts/src', 'crates/spm-contracts/Cargo.toml'],
                capture_output=True, text=True, check=True)
            if set(tree.stdout.splitlines()) != {'crates/spm-contracts/' + p for p in hashes}:
                fail('backend public contract revision file set mismatch')
        text = '\n'.join(p.read_text() for p in sorted((contracts / 'src').rglob('*.rs')))
        protocol = {}
        for key in ('major', 'minor'):
            values = re.findall(r'pub const PROTOCOL_' + key.upper() + r'\s*:\s*\w+\s*=\s*(\d+)\s*;', text)
            if len(values) != 1:
                fail(f'cannot uniquely read public PROTOCOL_{key.upper()}')
            protocol[key] = int(values[0])
        return {'version': package['version'], 'source': 'crates/spm-contracts',
                'backend_dependency': source, 'files_sha256': hashes,
                'wire_protocol': protocol}
    except (OSError, KeyError, TypeError, AttributeError, ValueError, subprocess.CalledProcessError) as exc:
        fail(f'cannot read public contract provenance: {exc}')

current_contracts = provenance()
if sys.argv[3] == '1':
    print(json.dumps(current_contracts))
    raise SystemExit(0)

if not inst.is_dir():
    fail(f'instance does not exist: {inst}')
if not manifest_path.is_file():
    fail(f'missing run-manifest.json: {manifest_path}')
try:
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
except (OSError, UnicodeError, json.JSONDecodeError) as exc:
    fail(f'invalid run-manifest.json: {exc}')

for key in ('instance', 'created_at', 'source', 'spm_commit', 'pecofence_commit',
            'contracts', 'cargo_lock_sha256', 'db_format',
            'config', 'profiles', 'start_args'):
    required(manifest, key, '')
if manifest.get('schema_version') != 2:
    fail('unsupported manifest schema (recreate instance with schema_version 2)')
if 'spm_rev_pinned_in_pecofence' in manifest:
    fail('obsolete private backend pin assertion')
if manifest['instance'] != sys.argv[2]:
    fail('instance name mismatch')
if manifest['contracts'] != current_contracts:
    fail('public contract provenance mismatch')
if manifest.get('wire_protocol') != current_contracts['wire_protocol']:
    fail('public wire protocol mismatch')
if manifest['cargo_lock_sha256'] != sha256(Path(os.environ['PF_REPO']) / 'Cargo.lock'):
    fail('public Cargo.lock sha256 mismatch')
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
if set(binaries) != {'pecofence.exe', 'pecofence-watchdog.exe', 'spmd.exe', 'spm.exe', 'WebView2Loader.dll'}:
    fail('incomplete deployed binary hash inventory')
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
if not isinstance(fixture, dict) or 'path' not in fixture or 'sha256' not in fixture:
    fail('missing fixture fields')
fixture_path = fixture['path']
fixture_sha = fixture['sha256']
if not isinstance(fixture_path, str) or not isinstance(fixture_sha, str):
    fail('invalid fixture fields')
if fixture_path or fixture_sha:
    if not fixture_path or not re.fullmatch(r'[0-9a-fA-F]{64}', fixture_sha):
        fail('invalid fixture fields')
    path = (lab / fixture_path).resolve()
    if not path.is_relative_to(lab) or not path.is_file():
        fail(f'fixture does not exist: {fixture_path}')
    if sha256(path) != fixture_sha.lower():
        fail(f'fixture sha256 mismatch: {fixture_path}')

for key, repo in (('spm_commit', os.environ['SPM_REPO']),
                  ('pecofence_commit', os.environ['PF_REPO'])):
    commit = manifest[key]
    if not isinstance(commit, str) or not re.fullmatch(r'[0-9a-fA-F]{40}', commit):
        fail(f'invalid commit: {key}')
    result = subprocess.run(['git', '-C', repo, 'cat-file', '-e', f'{commit}^{{commit}}'],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if result.returncode:
        fail(f'commit does not exist: {key} ({commit})')
print('VERIFY-OK')
PY
