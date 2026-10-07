# The desktop build contract shared by local verification, CI and tag releases.
# No publishing or machine-wide tool/credential configuration happens here.
[CmdletBinding()]
param(
  [string]$TargetDir = "target",
  [string]$Python = "python",
  [switch]$SourceOnly,
  [switch]$SkipSourceChecks,
  [switch]$RequireCleanSource
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Invoke-CheckedCommand([string]$Name, [string]$Program, [string[]]$Arguments) {
  Write-Host "`n[$Name] $Program $($Arguments -join ' ')"
  $timer = [Diagnostics.Stopwatch]::StartNew()
  try {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
      throw "$Name failed (exit code $LASTEXITCODE)"
    }
  } finally {
    $timer.Stop()
    Write-Host "[$Name] $([math]::Round($timer.Elapsed.TotalSeconds, 2))s"
  }
}

# Cheap source checks fail before the expensive native compilation.
if ($SourceOnly -and $SkipSourceChecks) { throw "SourceOnly cannot skip its own checks." }
if (-not $SkipSourceChecks) {
  Invoke-CheckedCommand "Public dependency policy tests" $Python @("scripts/test-public-dependencies.py")
  Invoke-CheckedCommand "Public dependency boundary" $Python @("scripts/check-public-dependencies.py")
  Invoke-CheckedCommand "Action policy tests" $Python @("scripts/test-actions-policy.py")
  Invoke-CheckedCommand "Action pin policy" $Python @("scripts/check-actions.py")
  Invoke-CheckedCommand "Native locale scanner tests" $Python @("scripts/test-locales.py")
  Invoke-CheckedCommand "Translations" $Python @("scripts/check-locales.py")
  Invoke-CheckedCommand "README translation tests" $Python @("scripts/test-readme-translations.py")
  Invoke-CheckedCommand "README translations" $Python @("scripts/check-readme-translations.py")
  Invoke-CheckedCommand "Desktop packaging tests" $Python @("scripts/test-package-desktop.py")
  Invoke-CheckedCommand "Windows App SDK runtime tests" $Python @("scripts/test-winappsdk-runtime.py")
}
if ($SourceOnly) { return }

Invoke-CheckedCommand "Format" "cargo" @("fmt", "--all", "--check")
Invoke-CheckedCommand "Resolved public dependency graph" $Python @("scripts/check-public-dependencies.py", "--resolved")
Invoke-CheckedCommand "Clippy" "cargo" @(
  "clippy", "--locked", "--workspace", "--all-targets", "--target-dir", $TargetDir,
  "--", "-D", "warnings"
)
Invoke-CheckedCommand "Generate bindings" "cargo" @(
  "run", "--locked", "-p", "tool_bindgen", "--target-dir", $TargetDir
)
Invoke-CheckedCommand "Bindings match source" "git" @(
  "diff", "--exit-code", "--", "crates/platform/src/bindings.rs",
  "crates/render/src/comp.rs", "crates/render/src/gpu_bindings.rs"
)

# Test executables carry the embedded WinUI activation manifest. Stage the
# self-contained runtime beside debug outputs before Cargo compiles and runs them.
Invoke-CheckedCommand "Stage debug Windows App SDK runtime" $Python @(
  "scripts/stage-winappsdk-runtime.py", "stage", "--target-dir", $TargetDir, "--profile", "debug"
)
Invoke-CheckedCommand "Workspace tests" "cargo" @(
  "test", "--locked", "--workspace", "--target-dir", $TargetDir
)
Invoke-CheckedCommand "Reactor dialog teardown regression tests" "cargo" @(
  "test", "--locked", "-p", "windows-reactor", "--lib", "content_dialog_reset_tests",
  "--target-dir", $TargetDir
)
Invoke-CheckedCommand "Workspace fixture validator tests" "cargo" @(
  "test", "--locked", "-p", "pecofence-core", "--example", "validate_workspace",
  "--target-dir", $TargetDir
)

# One release compilation; packaging must consume these exact binaries.
Write-Host "`n[Release build] Compile once with verified source/output receipt"
& (Join-Path $PSScriptRoot "build-desktop.ps1") -TargetDir $TargetDir -Python $Python
# The executable budget is separate from the self-contained runtime payload.
$binary = Join-Path $TargetDir "release/pecofence.exe"
$size = (Get-Item -LiteralPath $binary).Length
Write-Host "`n[Size gate] pecofence.exe = $size bytes (limit: 6815744)"
if ($size -gt 6815744) { throw "pecofence.exe exceeds the 6.5 MiB executable budget" }

Write-Host "`n[Package] Reusing the verified release binaries"
& (Join-Path $PSScriptRoot "make-portable.ps1") -SkipBuild -TargetDir $TargetDir -Python $Python -RequireCleanSource:$RequireCleanSource
