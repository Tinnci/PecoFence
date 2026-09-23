# Restart only spmd while PecoFence stays running; verify subscription recovery.
param(
  [Parameter(Mandatory = $true)][string]$Name,
  [string]$LabRoot = "C:\Users\Administrator\PecoFence-lab"
)
$ErrorActionPreference = "Stop"
$Inst = Join-Path $LabRoot "instances\$Name"
$manifestPath = Join-Path $Inst "run-manifest.json"
$pidPath = Join-Path $Inst "logs\spmd.pid"
$panelLog = Join-Path $Inst "logs\pecofence.stderr.log"
if (-not (Test-Path $manifestPath)) { throw "not a lab instance: $Inst" }
if (-not (Test-Path $pidPath)) { throw "spmd is not recorded as running: $pidPath" }
$oldPid = [int](Get-Content $pidPath -Raw).Trim()
if (-not (Get-Process -Id $oldPid -ErrorAction SilentlyContinue)) { throw "spmd is not running: $oldPid" }
$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
if (-not $manifest.fixture.path) { throw "manifest has no fixture" }
$fixture = Join-Path $LabRoot $manifest.fixture.path
if (-not (Test-Path $fixture)) { throw "fixture missing: $fixture" }
$beforeLines = if (Test-Path $panelLog) { @(Get-Content $panelLog).Count } else { 0 }
Write-Host "before: spmd pid=$oldPid; pecofence log lines=$beforeLines"
Stop-Process -Id $oldPid -Force
Wait-Process -Id $oldPid -ErrorAction SilentlyContinue
$stdout = Join-Path $Inst "logs\spmd.stdout.log"
$stderr = Join-Path $Inst "logs\spmd.stderr.log"
# cmd redirection appends; Start-Process -RedirectStandard* truncates existing logs.
$command = '"{0}" --ipc-version v2 --fixture "{1}" --instance "{2}" 1>>"{3}" 2>>"{4}"' -f `
  (Join-Path $Inst "spmd.exe"), $fixture, $Name, $stdout, $stderr
$launcher = Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" `
  -ArgumentList @('/d', '/s', '/c', ('"' + $command + '"')) `
  -WorkingDirectory $Inst -PassThru
$spmd = $null
for ($attempt = 0; $attempt -lt 20 -and -not $spmd; $attempt++) {
  $spmd = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($launcher.Id)" |
    Where-Object { $_.Name -ieq 'spmd.exe' } | Select-Object -First 1
  if (-not $spmd) { Start-Sleep -Milliseconds 100 }
}
if (-not $spmd) { throw "spmd did not start; inspect $stderr" }
Set-Content -Path $pidPath -Value $spmd.ProcessId
$deadline = (Get-Date).AddSeconds(30)
do {
  $lines = if (Test-Path $panelLog) { @(Get-Content $panelLog) } else { @() }
  $newLines = @($lines | Select-Object -Skip $beforeLines)
  $subscribe = $newLines | Where-Object { $_ -match 'subscribe' } | Select-Object -First 1
  $received = $newLines | Where-Object { $_ -match 'snapshot\.received' } | Select-Object -First 1
  if ($subscribe -and $received) {
    Write-Host "PASS: spmd restarted pid=$($spmd.ProcessId); PecoFence reconnected"
    Write-Host $subscribe
    Write-Host $received
    exit 0
  }
  Start-Sleep -Milliseconds 500
} while ((Get-Date) -lt $deadline)
Write-Host "FAIL: no new subscribe and snapshot.received within 30s"
Get-Content $panelLog -Tail 30 -ErrorAction SilentlyContinue | ForEach-Object { Write-Host $_ }
exit 1
