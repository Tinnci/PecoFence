param(
  [string]$Binary = (Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\debug\pecofence.exe')
)

$ErrorActionPreference = 'Stop'
$binaryPath = (Resolve-Path $Binary).ProviderPath
$instance = 'todo-smoke-' + [guid]::NewGuid().ToString('N').Substring(0, 12)
$stage = Join-Path $env:TEMP $instance
$stagedBinary = Join-Path $stage 'pecofence.exe'
$configDir = Join-Path $stage 'config'
$scriptPath = Join-Path $stage 'todo.txt'
$previousInstance = $env:PECOFENCE_INSTANCE
$previousLocalAppData = $env:LOCALAPPDATA
$previousAppData = $env:APPDATA
$previousRustLog = $env:RUST_LOG
$env:PECOFENCE_INSTANCE = $instance
$env:LOCALAPPDATA = Join-Path $stage 'local-appdata'
$env:APPDATA = Join-Path $stage 'roaming-appdata'
$env:RUST_LOG = 'info'
New-Item -ItemType Directory -Force -Path $stage, $env:LOCALAPPDATA, $env:APPDATA | Out-Null
Copy-Item -Path $binaryPath -Destination $stagedBinary
$runtimeManifest = Get-Content -Raw -Encoding UTF8 (Join-Path $PSScriptRoot '..\third_party\winappsdk\runtime-manifest.json') | ConvertFrom-Json
foreach ($entry in $runtimeManifest.files) {
  $destination = Join-Path $stage $entry.path
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
  Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $binaryPath) $entry.path) -Destination $destination
}
Set-Content -Path $scriptPath -Encoding UTF8 -Value @(
  'sleep 400'
  'new-todo 1200 700'
  'sleep 400'
  'todo-add Buy milk'
  'sleep 400'
  'todo-add Write report'
  'sleep 400'
  'todo-toggle 0'
  'sleep 500'
  'exit'
)

$passed = $false
try {
  $args1 = @('--portable', '--config-dir', $configDir, '--no-hide-icons', '--test-script', $scriptPath, '--exit-after', '10000')
  $process = Start-Process -FilePath $stagedBinary -ArgumentList $args1 -PassThru -Wait
  if ($process.ExitCode -ne 0) { throw "First run failed: $($process.ExitCode)" }

  $configPath = Join-Path $configDir 'workspace.v2.json'
  $saved = Get-Content -Raw -Encoding UTF8 $configPath | ConvertFrom-Json
  $todo = @($saved.layouts.contents | Where-Object { $_.content.panel.provider -eq 'pecofence.todo' })
  if ($todo.Count -ne 1) { throw "Expected one TODO fence; found $($todo.Count)" }
  $tasks = @($todo[0].content.panel.config.tasks)
  if ($tasks.Count -ne 2 -or $tasks[0].title -ne 'Buy milk' -or !$tasks[0].done -or $tasks[1].title -ne 'Write report' -or $tasks[1].done) {
    throw "TODO contents were not saved: $($tasks | ConvertTo-Json -Compress)"
  }
  $before = $saved.layouts.containers | ConvertTo-Json -Compress -Depth 20

  $args2 = @('--portable', '--config-dir', $configDir, '--no-hide-icons', '--exit-after', '1200')
  $process = Start-Process -FilePath $stagedBinary -ArgumentList $args2 -PassThru -Wait
  if ($process.ExitCode -ne 0) { throw "Restart failed: $($process.ExitCode)" }
  $reloaded = Get-Content -Raw -Encoding UTF8 $configPath | ConvertFrom-Json
  $restored = @($reloaded.layouts.contents | Where-Object { $_.content.panel.provider -eq 'pecofence.todo' })
  if ($restored.Count -ne 1) { throw 'TODO fence was lost after restart' }
  $restoredTasks = @($restored[0].content.panel.config.tasks)
  if ($restoredTasks.Count -ne 2 -or !$restoredTasks[0].done -or $restoredTasks[1].done) {
    throw 'TODO tasks changed after restart'
  }
  $after = $reloaded.layouts.containers | ConvertTo-Json -Compress -Depth 20
  if ($before -ne $after) { throw 'TODO fence position changed after restart' }
  Write-Output "PASS: TODO add, toggle, config save and geometry restore ($instance)"
  $passed = $true
} finally {
  $env:PECOFENCE_INSTANCE = $previousInstance
  $env:LOCALAPPDATA = $previousLocalAppData
  $env:APPDATA = $previousAppData
  $env:RUST_LOG = $previousRustLog
  if ($passed -and (Test-Path $stage)) { Remove-Item -Recurse -Force $stage }
  elseif (!$passed) { Write-Warning "Test artifacts retained: $stage" }
}
