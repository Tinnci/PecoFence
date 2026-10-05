# The desktop build contract shared by local verification, CI and tag releases.
# No publishing or machine-wide tool/credential configuration happens here.
[CmdletBinding()]
param(
  [string]$TargetDir = "target",
  [string]$Python = "python"
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
Invoke-CheckedCommand "Format" "cargo" @("fmt", "--all", "--check")
Invoke-CheckedCommand "Translations" $Python @("scripts/check-locales.py")
Invoke-CheckedCommand "README translations" $Python @("scripts/check-readme-translations.py")
Invoke-CheckedCommand "Website" $Python @("scripts/build-site.py", "--strict", "--out", ".cache/site-check")

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

# Make runtime dependencies explicit rather than depending on the runner's DLL search path.
Invoke-CheckedCommand "Compile tests" "cargo" @(
  "test", "--locked", "--workspace", "--no-run", "--target-dir", $TargetDir
)
Copy-Item -LiteralPath "third_party/webview2/WebView2Loader.x64.dll" `
  -Destination (Join-Path $TargetDir "debug/deps/WebView2Loader.dll") -Force
Invoke-CheckedCommand "Workspace tests" "cargo" @(
  "test", "--locked", "--workspace", "--target-dir", $TargetDir
)

# One release compilation; packaging must consume these exact binaries.
Invoke-CheckedCommand "Release build" "cargo" @(
  "build", "--locked", "--release", "-p", "pecofence", "-p", "pecofence-watchdog",
  "--target-dir", $TargetDir
)
# Existing 4.5 MiB budget includes the SPM host integration. Keep the same gate
# for pull requests and releases; do not trade diagnostics for a smaller number.
$binary = Join-Path $TargetDir "release/pecofence.exe"
$size = (Get-Item -LiteralPath $binary).Length
Write-Host "`n[Size gate] pecofence.exe = $size bytes (limit: 4718592)"
if ($size -gt 4718592) { throw "pecofence.exe exceeds the 4.5 MiB size budget" }

Write-Host "`n[Package] Reusing the verified release binaries"
& (Join-Path $PSScriptRoot "make-portable.ps1") -SkipBuild -TargetDir $TargetDir -Python $Python
