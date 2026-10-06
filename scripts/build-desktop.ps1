# Compile once and bind the exact outputs to source/toolchain fingerprints.
[CmdletBinding()]
param([string]$TargetDir = "target/package", [string]$Python = "python")
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
$captured = Join-Path $TargetDir "build-inputs.json"
& $Python scripts/package_desktop.py capture --out $captured
if ($LASTEXITCODE -ne 0) { throw "Cannot capture build inputs." }
cargo build --locked --release -p pecofence -p pecofence-watchdog --target-dir $TargetDir
if ($LASTEXITCODE -ne 0) { throw "Cargo desktop build failed." }
& $Python scripts/package_desktop.py record-build --release (Join-Path $TargetDir "release") --captured $captured
if ($LASTEXITCODE -ne 0) { throw "Cannot verify/stamp desktop build outputs." }
