#!/usr/bin/env bash
# Pure verifier regression: owned synthetic artifacts, no builds, deployment,
# commits, application launch, or dependency on an authorized private checkout.
set -euo pipefail
cd "$(dirname "$0")/../.."
mkdir -p .cache
LAB="$(mktemp -d "$PWD/.cache/verify-instance.XXXXXX")"
trap 'rm -rf "$LAB"' EXIT
python3 - "$PWD/scripts/lab/verify-instance.sh" "$LAB" <<'PY'
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

if sys.version_info < (3, 11):
    raise SystemExit('Python >=3.11 required')
source_script = Path(sys.argv[1])
source_repo = source_script.parents[2]
root = Path(sys.argv[2])
pf, backend, lab = root / 'public', root / 'backend', root / 'lab'
script = pf / 'scripts/lab/verify-instance.sh'
script.parent.mkdir(parents=True)
shutil.copyfile(source_script, script)
(pf / 'scripts/winappsdk_runtime.py').write_text(
    "from pathlib import Path\n"
    "PACKAGE_VERSION = '2.5.1'\n"
    "def load_runtime_manifest(): return {'synthetic': True}\n"
    "def manifest_digest(_manifest): return '" + "c" * 64 + "'\n"
    "def validate_runtime_directory(root, _manifest):\n"
    "    return {'SyntheticSDK.dll': (Path(root) / 'SyntheticSDK.dll').read_bytes()}, {}\n",
    encoding='utf-8')
contracts = pf / 'crates/spm-contracts'
(contracts / 'src').mkdir(parents=True)
(contracts / 'Cargo.toml').write_text('[package]\nname="spm-contracts"\nversion="0.1.0"\n')
# Parser input only, not an implementation of the public protocol.
(contracts / 'src/lib.rs').write_text('pub const PROTOCOL_MAJOR: u32 = 2;\npub const PROTOCOL_MINOR: u32 = 0;\n')
(pf / 'Cargo.lock').write_text('# synthetic lockfile\n')
backend.mkdir()
def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args], text=True).strip()
# Reference existing objects read-only; do not create commits or change refs.
git_dir = git(source_repo, 'rev-parse', '--absolute-git-dir')
revision = git(source_repo, 'rev-parse', 'HEAD')
for repo in (pf, backend):
    (repo / '.git').write_text('gitdir: ' + git_dir + '\n')
def dependency(value):
    (backend / 'Cargo.toml').write_text('[workspace.dependencies]\nspm-contracts = ' + value + '\n')
dependency('{ version = "=0.1.0", path = "../public/crates/spm-contracts" }')
env = dict(os.environ, SPM_REPO=str(backend))
def invoke(*args, environment=env):
    return subprocess.run(['bash', str(script), *args], env=environment, text=True, capture_output=True)
result = invoke('--contract-provenance')
assert result.returncode == 0, result.stdout + result.stderr
provenance = json.loads(result.stdout)
inst = lab / 'instances/test'
inst.mkdir(parents=True)
fixture = lab / 'fixtures/synthetic/catalog.json'
fixture.parent.mkdir(parents=True)
fixture.write_text('{"synthetic": true}\n')
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
binaries = {}
for name in ('pecofence.exe', 'pecofence-watchdog.exe', 'spmd.exe', 'spm.exe'):
    (inst / name).write_text('not executable: synthetic verifier data\n')
    binaries[name] = digest(inst / name)
(inst / 'SyntheticSDK.dll').write_bytes(b'synthetic runtime')
original = {
    'schema_version': 4, 'instance': 'test', 'created_at': 'synthetic',
    'source': 'local', 'spm_commit': revision, 'pecofence_commit': revision,
    'contracts': provenance, 'cargo_lock_sha256': digest(pf / 'Cargo.lock'),
    'toolchain': {'rustc': 'synthetic', 'windows_sdk': 'synthetic', 'msvc': 'synthetic'},
    'wire_protocol': provenance['wire_protocol'], 'db_format': 'v2',
    'binaries_sha256': binaries,
    'windows_app_sdk': {
        'version': '2.5.1', 'manifest_sha256': 'c' * 64,
        'files_sha256': {'SyntheticSDK.dll': digest(inst / 'SyntheticSDK.dll')},
    },
    'fixture': {'path': 'fixtures/synthetic/catalog.json', 'sha256': digest(fixture)},
    'config': 'synthetic', 'profiles': {'spm': 'release', 'pecofence': 'release'},
    'start_args': {'spmd': ['synthetic'], 'pecofence': ['synthetic']},
}
manifest = inst / 'run-manifest.json'
passes = 0
def check(label, expected=None, data=None, environment=env):
    global passes
    manifest.write_text(json.dumps(original if data is None else data))
    result = invoke('--name', 'test', '--lab-dir', str(lab), environment=environment)
    output = result.stdout + result.stderr
    valid = (result.returncode == 0 and 'VERIFY-OK' in output) if expected is None else (
        result.returncode != 0 and 'VERIFY-FAILED: ' + expected in output)
    assert valid, (label, output)
    print('PASS:', label)
    passes += 1
check('same-checkout authorized integration')
data = copy.deepcopy(original)
data['fixture'] = {'path': '', 'sha256': ''}
check('optional fixture absent', data=data)
data['fixture'] = {'path': '', 'sha256': '0' * 64}
check('partial optional fixture rejected', 'invalid fixture fields', data)
check('explicit backend required', 'authorized backend integration requires explicit SPM_REPO',
      environment={k: v for k, v in env.items() if k != 'SPM_REPO'})
for key, value, reason in (
    ('spm_commit', '0' * 40, 'commit does not exist: spm_commit'),
    ('pecofence_commit', '0' * 40, 'commit does not exist: pecofence_commit'),
    ('schema_version', 2, 'unsupported manifest schema'),
    ('cargo_lock_sha256', '0' * 64, 'public Cargo.lock sha256 mismatch'),
    ('wire_protocol', {'major': 9, 'minor': 0}, 'public wire protocol mismatch'),
    ('spm_rev_pinned_in_pecofence', '0' * 40, 'obsolete private backend pin assertion'),
):
    data = copy.deepcopy(original)
    data[key] = value
    check(key + ' mismatch', reason, data)
data = copy.deepcopy(original)
data['contracts']['version'] = '9.0.0'
check('contract provenance mismatch', 'public contract provenance mismatch', data)
data = copy.deepcopy(original)
del data['binaries_sha256']['spmd.exe']
check('incomplete binary hashes', 'incomplete deployed binary hash inventory', data)
data = copy.deepcopy(original)
data['windows_app_sdk']['files_sha256']['SyntheticSDK.dll'] = '0' * 64
check('runtime hash mismatch', 'Windows App SDK runtime provenance/hash mismatch', data)
data = copy.deepcopy(original)
data['binaries_sha256']['WebView2Loader.dll'] = '0' * 64
check('obsolete WebView inventory rejected', 'incomplete deployed binary hash inventory', data)
webview_payload = inst / 'WebView2Loader.dll'
webview_payload.write_text('obsolete synthetic WebView payload\n')
check('unmanifested WebView payload rejected', 'WebView payload is not supported in lab instances')
webview_payload.unlink()
for path, reason in ((inst / 'pecofence.exe', 'binary sha256 mismatch: pecofence.exe'),
                     (inst / 'SyntheticSDK.dll', 'Windows App SDK runtime provenance/hash mismatch'),
                     (fixture, 'fixture sha256 mismatch:')):
    saved = path.read_bytes()
    path.write_bytes(saved + b'x')
    check('tampered ' + path.name, reason)
    path.write_bytes(saved)
saved = (contracts / 'src/lib.rs').read_bytes()
(contracts / 'src/lib.rs').write_bytes(saved + b'// modified public source\n')
check('public source mismatch', 'public contract provenance mismatch')
(contracts / 'src/lib.rs').write_bytes(saved)
dependency('{ version = "=0.1.0", path = "../duplicate-contracts" }')
check('duplicate contract directory', 'backend contract path does not resolve')
dependency('{ version = "=9.0.0", path = "../public/crates/spm-contracts" }')
check('backend contract version mismatch', 'backend public contract version must exactly match')
dependency('{ version = "=0.1.0", git = "https://github.com/Tinnci/PecoFence.git", rev = "' + revision + '" }')
# The owned synthetic manifest deliberately differs from every real public pin.
check('git dependency byte mismatch', 'backend public contract revision byte mismatch')
dependency('{ version = "=0.1.0", git = "https://github.com/Tinnci/spm.git", rev = "' + revision + '" }')
check('private git source rejected', 'backend contract git source is not the public PecoFence repository')
dependency('{ version = "=0.1.0", git = "https://github.com/Tinnci/PecoFence.git", rev = "main" }')
check('nonexact git revision rejected', 'backend public contract dependency requires an exact git revision')
print(f'SUMMARY: {passes} PASS, 0 FAIL')
PY
