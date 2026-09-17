# Replace running PecoFence instance with newly built package
param(
    [string]$TargetDir = "C:\Users\Administrator\Downloads\pecofence-0.0.1-x64",
    [string]$SourceDir = "C:\Users\Administrator\PecoFence\dist\pecofence-0.0.3-x64"
)

$ErrorActionPreference = "Stop"

Write-Host "====================================================" -ForegroundColor Cyan
Write-Host "       PecoFence Hot Upgrade & Replace Helper       " -ForegroundColor Cyan
Write-Host "====================================================" -ForegroundColor Cyan

# 1. Terminate running instances
Write-Host "[1/4] Stopping running PecoFence processes..." -ForegroundColor Yellow
Stop-Process -Name "pecofence-watchdog" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "pecofence" -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1

# 2. Backup previous binaries
Write-Host "[2/4] Backing up previous binaries..." -ForegroundColor Yellow
$backupDir = Join-Path $TargetDir "backup-v0.0.1"
if (-not (Test-Path $backupDir)) {
    New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
    Copy-Item (Join-Path $TargetDir "pecofence.exe") (Join-Path $backupDir "pecofence.exe") -ErrorAction SilentlyContinue
    Copy-Item (Join-Path $TargetDir "pecofence-watchdog.exe") (Join-Path $backupDir "pecofence-watchdog.exe") -ErrorAction SilentlyContinue
    Write-Host "  Backed up to: $backupDir" -ForegroundColor Green
}

# 3. Copy new package binaries
Write-Host "[3/4] Copying new 0.0.3 release files..." -ForegroundColor Yellow
Get-ChildItem -Path $SourceDir | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $TargetDir -Recurse -Force
    Write-Host "  Updated: $($_.Name)" -ForegroundColor Green
}

# 4. Launch upgraded instance
Write-Host "[4/4] Launching upgraded PecoFence..." -ForegroundColor Yellow
$exePath = Join-Path $TargetDir "pecofence.exe"
Start-Process $exePath
Start-Sleep -Seconds 2

Write-Host "`n✅ Upgrade successful! Current running processes:" -ForegroundColor Green
Get-Process pecofence, pecofence-watchdog -ErrorAction SilentlyContinue | Select-Object Id, ProcessName, Path
