param([Parameter(Mandatory = $true)][string]$Binary)
$ErrorActionPreference = 'Stop'
$binaryPath = (Resolve-Path $Binary).ProviderPath
$stage = Join-Path $env:TEMP ('pecofence-exit-test-' + [guid]::NewGuid().ToString('N'))
$keys = @('PECOFENCE_INSTANCE','PECOFENCE_UI_TEST_WINDOWS','PECOFENCE_TEST_STALL_REACTOR_EXIT','APPDATA','LOCALAPPDATA','RUST_LOG')
$previous = @{}
foreach ($key in $keys) { $previous[$key] = [Environment]::GetEnvironmentVariable($key) }
$passed = $false
try {
    New-Item -ItemType Directory $stage | Out-Null
    Copy-Item $binaryPath (Join-Path $stage 'pecofence.exe')
    $manifest = Get-Content (Join-Path $PSScriptRoot '..\third_party\winappsdk\runtime-manifest.json') -Raw | ConvertFrom-Json
    foreach ($entry in $manifest.files) {
        $destination = Join-Path $stage $entry.path
        New-Item -ItemType Directory -Force (Split-Path $destination) | Out-Null
        Copy-Item (Join-Path (Split-Path $binaryPath) $entry.path) $destination
    }
    $env:PECOFENCE_INSTANCE = 'exit-fault-' + [guid]::NewGuid().ToString('N')
    $env:PECOFENCE_UI_TEST_WINDOWS = '1'
    $env:PECOFENCE_TEST_STALL_REACTOR_EXIT = '1'
    $env:APPDATA = Join-Path $stage 'roaming'
    $env:LOCALAPPDATA = Join-Path $stage 'local'
    $env:RUST_LOG = 'info'
    $config = Join-Path $stage 'config'
    $process = Start-Process (Join-Path $stage 'pecofence.exe') -ArgumentList @('--portable','--no-hide-icons','--config-dir',$config,'--exit-after','400') -PassThru
    $null = $process.Handle
    if (-not $process.WaitForExit(10000)) { throw "Exit watchdog timed out; isolated process $($process.Id), evidence: $stage" }
    if ($process.ExitCode -ne 0) { throw 'The watchdog did not exit normally' }
    $log = (Get-ChildItem $env:LOCALAPPDATA -Recurse -Filter '*.log' | ForEach-Object { Get-Content $_.FullName -Raw }) -join "`n"
    foreach ($marker in @('exit.persistence_committed','exit.test_stalled_dispatch','exit.native_pump_fallback_after_committed_close')) {
        if (-not $log.Contains($marker)) { throw "Missing exit phase: $marker" }
    }
    $workspace = Join-Path $config 'workspace.v2.json'
    if (-not (Test-Path $workspace)) { throw 'Final workspace was not saved' }
    $validation = Start-Process (Join-Path $stage 'pecofence.exe') -ArgumentList @('--validate-workspace',$workspace) -PassThru -Wait
    if ($validation.ExitCode -ne 0) { throw 'The final workspace cannot be recovered' }
    Write-Output 'PASS: committed save, stalled dispatcher, native fallback and workspace recovery'
    $passed = $true
} finally {
    foreach ($key in $keys) { [Environment]::SetEnvironmentVariable($key,$previous[$key]) }
    if ($passed) { Remove-Item $stage -Recurse -Force }
    else { Write-Warning "Exit test evidence retained: $stage" }
}
