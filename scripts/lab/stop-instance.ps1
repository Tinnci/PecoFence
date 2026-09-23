# Stop a PecoFence lab instance: by recorded pid files first, then sweep any
# process whose executable lives under the instance directory.
# Usage: powershell -File scripts/lab/stop-instance.ps1 -Name normal-local-001
param(
  [Parameter(Mandatory = $true)][string]$Name,
  [string]$LabRoot = "C:\Users\Administrator\PecoFence-lab"
)
$ErrorActionPreference = "Stop"
$Inst = Join-Path $LabRoot "instances\$Name"
if (-not (Test-Path $Inst)) { throw "unknown instance: $Inst" }

foreach ($pidFile in @("spmd.pid", "pecofence.pid", "watchdog.pid")) {
  $p = Join-Path $Inst "logs\$pidFile"
  if (Test-Path $p) {
    $procId = Get-Content $p -ErrorAction SilentlyContinue
    if ($procId) { Stop-Process -Id ([int]$procId) -Force -ErrorAction SilentlyContinue }
    Remove-Item $p -Force -ErrorAction SilentlyContinue
  }
}
Get-Process pecofence, pecofence-watchdog, spmd -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -and $_.Path.StartsWith($Inst, [StringComparison]::OrdinalIgnoreCase) } |
  Stop-Process -Force -ErrorAction SilentlyContinue
Write-Host "stopped instance: $Name"