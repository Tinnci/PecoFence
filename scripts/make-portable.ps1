# Build separately from a running development app, or consume a verified build receipt.
[CmdletBinding()]
param(
  [string]$Version = "",
  [string]$TargetDir = "target/package",
  [string]$Python = "python",
  [switch]$SkipBuild,
  [switch]$RequireCleanSource
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
if (-not $SkipBuild) {
  & "$PSScriptRoot/build-desktop.ps1" -TargetDir $TargetDir -Python $Python
}
$arguments = @("scripts/package_desktop.py", "portable", "--release", (Join-Path $TargetDir "release"), "--out", "dist")
if ($Version) { $arguments += @("--version", $Version) }
if ($RequireCleanSource) { $arguments += "--require-clean" }
& $Python @arguments
if ($LASTEXITCODE -ne 0) { throw "Verified portable packaging failed." }
