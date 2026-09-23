# Start a PecoFence lab instance: spmd (fixture mode) + pecofence (portable),
# each with per-instance isolation for logs, appdata and the WebView2 profile.
# Usage: powershell -File scripts/lab/run-instance.ps1 -Name normal-local-001
param(
  [Parameter(Mandatory = $true)][string]$Name,
  [string]$LabRoot = "C:\Users\Administrator\PecoFence-lab"
)
$ErrorActionPreference = "Stop"
$Inst = Join-Path $LabRoot "instances\$Name"
$manifestPath = Join-Path $Inst "run-manifest.json"
if (-not (Test-Path $manifestPath)) { throw "not a lab instance (no run-manifest.json): $Inst" }

$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
if (-not $manifest.fixture.path) { throw "manifest has no fixture; DB-mode start is not wired yet" }
$fixture = Join-Path $LabRoot $manifest.fixture.path
if (-not (Test-Path $fixture)) { throw "fixture missing: $fixture" }

foreach ($log in @("spmd.stderr.log", "spmd.stdout.log", "pecofence.stderr.log", "pecofence.stdout.log")) {
  $p = Join-Path $Inst "logs\$log"
  if (Test-Path $p) { Remove-Item $p -Force -ErrorAction SilentlyContinue }
}

# 1. spmd: v2 named pipe, fixture mode, instance-local logs.
$spmd = Start-Process -FilePath (Join-Path $Inst "spmd.exe") `
  -ArgumentList @("--ipc-version", "v2", "--fixture", $fixture) `
  -WorkingDirectory $Inst `
  -RedirectStandardOutput (Join-Path $Inst "logs\spmd.stdout.log") `
  -RedirectStandardError (Join-Path $Inst "logs\spmd.stderr.log") `
  -PassThru
Set-Content -Path (Join-Path $Inst "logs\spmd.pid") -Value $spmd.Id
Start-Sleep -Milliseconds 800

# 2. pecofence: portable config lives next to the exe; isolate APPDATA /
#    LOCALAPPDATA (logs, crash dumps, WebView2 profile) and the single-instance
#    mutex name so several lab instances can coexist.
$oldAppData = $env:APPDATA
$oldLocalAppData = $env:LOCALAPPDATA
$oldInstance = $env:PECOFENCE_INSTANCE
try {
  $env:APPDATA = Join-Path $Inst "appdata\Roaming"
  $env:LOCALAPPDATA = Join-Path $Inst "appdata\Local"
  $env:PECOFENCE_INSTANCE = $Name
  $pf = Start-Process -FilePath (Join-Path $Inst "pecofence.exe") `
    -ArgumentList @("--portable", "--no-hide-icons") `
    -WorkingDirectory $Inst `
    -RedirectStandardOutput (Join-Path $Inst "logs\pecofence.stdout.log") `
    -RedirectStandardError (Join-Path $Inst "logs\pecofence.stderr.log") `
    -PassThru
} finally {
  $env:APPDATA = $oldAppData
  $env:LOCALAPPDATA = $oldLocalAppData
  $env:PECOFENCE_INSTANCE = $oldInstance
}
Set-Content -Path (Join-Path $Inst "logs\pecofence.pid") -Value $pf.Id

Write-Host "instance  : $Name"
Write-Host "spmd pid  : $($spmd.Id)  fixture: $($manifest.fixture.path)"
Write-Host "pecofence : $($pf.Id)"
Write-Host "logs      : $Inst\logs"