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
$fonts = Join-Path (Split-Path -Parent $binaryPath) 'fonts'
if (Test-Path $fonts) { Copy-Item -LiteralPath $fonts -Destination $stage -Recurse }
Set-Content -Path $scriptPath -Encoding UTF8 -Value @(
  'sleep 400'
  'new-todo 1200 700'
  'sleep 400'
  'todo-add Buy milk'
  'sleep 400'
  'todo-add Write report'
  'sleep 400'
  'todo-toggle 0'
  'sleep 200'
  'todo-undo'
  'sleep 200'
  'todo-toggle 0'
  'sleep 200'
  'todo-edit 0 Buy oat milk'
  'sleep 200'
  'todo-clear'
  'sleep 200'
  'todo-undo'
  'sleep 200'
  'todo-add Temporary task'
  'sleep 200'
  'todo-undo'
  'sleep 200'
  'todo-undo'
  'sleep 300'
  'new-note 800 700'
  'sleep 300'
  'dump note-created'
  'cancel-rename'
  'sleep 200'
  'dump note-cancelled'
  'note-set "First line\nSecond line"'
  'sleep 200'
  'note-set "Temporary edit"'
  'sleep 200'
  'note-undo'
  'sleep 200'
  'note-open'
  'sleep 300'
  'dump note-opened'
  'cancel-rename'
  'sleep 500'
  'dump note-closed'
  'exit'
)

$passed = $false
try {
  $args1 = @('--portable', '--config-dir', $configDir, '--no-hide-icons', '--test-script', $scriptPath, '--exit-after', '10000')
  $process = Start-Process -FilePath $stagedBinary -ArgumentList $args1 -PassThru -Wait
  if ($process.ExitCode -ne 0) { throw "First run failed: $($process.ExitCode)" }

  $nativeLog = (Get-ChildItem -LiteralPath $env:LOCALAPPDATA -Recurse -Filter '*.log' | ForEach-Object {
    Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8
  }) -join "`n"
  foreach ($tag in @('note-created', 'note-opened')) {
    if ($nativeLog -notmatch ("\[" + $tag + "\].*rename=true")) { throw "Native note editor did not open: $tag" }
  }
  foreach ($tag in @('note-cancelled', 'note-closed')) {
    if ($nativeLog -notmatch ("\[" + $tag + "\].*rename=false")) { throw "Native note editor did not close: $tag" }
  }
  if ($nativeLog -notmatch '\[note-closed\].*frameless=true widget_grid=true') {
    throw 'The newly created note has no frameless widget resize grid'
  }
  $configPath = Join-Path $configDir 'workspace.v2.json'
  $saved = Get-Content -Raw -Encoding UTF8 $configPath | ConvertFrom-Json
  $todo = @($saved.layouts.contents | Where-Object { $_.content.panel.provider -eq 'pecofence.todo' })
  if ($todo.Count -ne 1) { throw "Expected one TODO fence; found $($todo.Count)" }
  $tasks = @($todo[0].content.panel.config.tasks)
  if ($tasks.Count -ne 2 -or $tasks[0].title -ne 'Buy oat milk' -or !$tasks[0].done -or $tasks[1].title -ne 'Write report' -or $tasks[1].done) {
    throw "TODO contents were not saved: $($tasks | ConvertTo-Json -Compress)"
  }
  $notes = @($saved.layouts.contents | Where-Object { $_.content.panel.provider -eq 'pecofence.note' })
  if ($notes.Count -ne 1 -or $notes[0].content.panel.config.text -cne "First line`nSecond line") {
    throw 'Multiline note editing/undo/native cancellation was not saved correctly'
  }
  $before = $saved.layouts.containers | ConvertTo-Json -Compress -Depth 20

  Set-Content -Path $scriptPath -Encoding UTF8 -Value @('sleep 400', 'dump grid-reloaded', 'exit')
  $args2 = @('--portable', '--config-dir', $configDir, '--no-hide-icons', '--test-script', $scriptPath, '--exit-after', '3000')
  $process = Start-Process -FilePath $stagedBinary -ArgumentList $args2 -PassThru -Wait
  if ($process.ExitCode -ne 0) { throw "Restart failed: $($process.ExitCode)" }
  $restartLog = (Get-ChildItem -LiteralPath $env:LOCALAPPDATA -Recurse -Filter '*.log' | ForEach-Object {
    Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8
  }) -join "`n"
  $gridWindows = @([regex]::Matches($restartLog, '\[grid-reloaded\].*frameless=true widget_grid=true'))
  if ($gridWindows.Count -ne 2) {
    throw "Expected two frameless widget grids after restart; found $($gridWindows.Count)"
  }
  $reloaded = Get-Content -Raw -Encoding UTF8 $configPath | ConvertFrom-Json
  $restored = @($reloaded.layouts.contents | Where-Object { $_.content.panel.provider -eq 'pecofence.todo' })
  if ($restored.Count -ne 1) { throw 'TODO fence was lost after restart' }
  $restoredTasks = @($restored[0].content.panel.config.tasks)
  if ($restoredTasks.Count -ne 2 -or $restoredTasks[0].title -ne 'Buy oat milk' -or !$restoredTasks[0].done -or $restoredTasks[1].done) {
    throw 'TODO tasks changed after restart'
  }
  $restoredNotes = @($reloaded.layouts.contents | Where-Object { $_.content.panel.provider -eq 'pecofence.note' })
  if ($restoredNotes.Count -ne 1 -or $restoredNotes[0].content.panel.config.text -cne "First line`nSecond line") {
    throw 'Multiline note text changed after restart'
  }
  $after = $reloaded.layouts.containers | ConvertTo-Json -Compress -Depth 20
  if ($before -ne $after) { throw 'TODO fence position changed after restart' }
  Write-Output "PASS: TODO operations plus multiline note edit, undo, native editor cancellation and restart restore ($instance)"
  $passed = $true
} finally {
  $env:PECOFENCE_INSTANCE = $previousInstance
  $env:LOCALAPPDATA = $previousLocalAppData
  $env:APPDATA = $previousAppData
  $env:RUST_LOG = $previousRustLog
  if ($passed -and (Test-Path $stage)) { Remove-Item -Recurse -Force $stage }
  elseif (!$passed) { Write-Warning "Test artifacts retained: $stage" }
}
