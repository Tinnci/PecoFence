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

  # Extra release-package review: exercise both sides of a mixed tab group.
  foreach ($content in $reloaded.layouts.contents) {
    if ($content.content.panel.provider -eq 'pecofence.todo') { $content.title='GridTodo' }
    if ($content.content.panel.provider -eq 'pecofence.note') { $content.title='GridNote' }
  }
  $file = @($reloaded.layouts.contents | Where-Object { $_.content.kind -eq 'fileCollection' })[0]
  if (!$file) { throw 'No isolated file fence for mixed-tab regression' }
  $file.title='GridFiles'
  $reloaded | ConvertTo-Json -Depth 100 | Set-Content $configPath -Encoding UTF8
  Set-Content $scriptPath -Encoding UTF8 -Value @(
    'sleep 400', 'merge GridTodo GridFiles', 'sleep 600',
    'activate GridTodo', 'sleep 500', 'dump widget-tab',
    'roll GridTodo', 'sleep 600', 'dump widget-rolled',
    'unroll GridTodo', 'sleep 700', 'dump widget-expanded',
    'activate GridFiles', 'sleep 500', 'dump file-tab',
    'detach GridTodo', 'sleep 600', 'dump detached-widget', 'exit'
  )
  $process=Start-Process $stagedBinary -ArgumentList @('--portable','--config-dir',$configDir,'--no-hide-icons','--test-script',$scriptPath,'--exit-after','10000') -PassThru -Wait
  if ($process.ExitCode -ne 0) { throw 'Mixed-tab run failed' }
  $tabLog=(Get-ChildItem $env:LOCALAPPDATA -Recurse -Filter '*.log' | ForEach-Object { Get-Content $_.FullName -Raw -Encoding UTF8 }) -join "`n"
  if ($tabLog -notmatch '\[widget-tab\].*tabs=2.*frameless=false widget_grid=true') { throw 'Widget tab has no grid' }
  if ($tabLog -notmatch '\[file-tab\].*tabs=2.*frameless=false widget_grid=false') { throw 'File tab retained stale widget rules' }
  if ($tabLog -notmatch '\[detached-widget\].*title="GridTodo".*tabs=1.*frameless=true widget_grid=true') { throw 'Detached widget has no frameless grid' }

  function Assert-RollGeometry([string]$log, [string]$open, [string]$closed, [string]$restored) {
    $rows = @{}
    foreach ($tag in @($open, $closed, $restored)) {
      $row = @($log -split "`n" | Where-Object { $_ -match ("\[" + $tag + "\].*tabs=2") })
      if ($row.Count -ne 1) { throw "Missing unique tab group geometry: $tag" }
      if ($row[0] -notmatch 'rect=\((-?\d+),(-?\d+),(-?\d+),(-?\d+)\)') { throw "Missing native rectangle: $tag" }
      $width = [int]$Matches[3] - [int]$Matches[1]
      $height = [int]$Matches[4] - [int]$Matches[2]
      if ($row[0] -notmatch 'title_height_px=(\d+) expanded_height_px=(\d+)') { throw "Missing height diagnostics: $tag" }
      $rows[$tag] = @{ width=$width; height=$height; title=[int]$Matches[1]; expanded=[int]$Matches[2] }
      $rolled = if ($tag -eq $closed) { 'true' } else { 'false' }
      if ($row[0] -notmatch ("rolled=" + $rolled + " rolling=false")) { throw "Roll did not settle: $tag" }
    }
    if ($rows[$closed].height -ne $rows[$closed].title) { throw "Collapsed panel exceeds title height: $closed" }
    if ($rows[$open].height -ne $rows[$restored].height) { throw "Expanded height not restored: $restored" }
    if ($rows[$open].expanded -ne $rows[$closed].expanded) { throw "Collapse overwrote saved expanded height: $closed" }
    if ($rows[$open].width -ne $rows[$closed].width) { throw "Collapse changed panel width: $closed" }
    Write-Output "PASS: $open $($rows[$open].height) px -> $closed $($rows[$closed].height) px -> $restored $($rows[$restored].height) px"
  }
  Assert-RollGeometry $tabLog 'widget-tab' 'widget-rolled' 'widget-expanded'

  # Calendar uses the same native min/max bridge but a different content minimum.
  # Change only this isolated note fixture after its persistence tests passed.
  $calendarConfig = Get-Content $configPath -Raw -Encoding UTF8 | ConvertFrom-Json
  $calendar = @($calendarConfig.layouts.contents | Where-Object { $_.title -eq 'GridNote' })[0]
  $calendar.title = 'GridCalendar'
  $calendar.content.panel.provider = 'pecofence.calendar'
  $calendar.content.panel.config = [pscustomobject]@{}
  $calendarConfig | ConvertTo-Json -Depth 100 | Set-Content $configPath -Encoding UTF8
  Set-Content $scriptPath -Encoding UTF8 -Value @(
    'sleep 400', 'merge GridCalendar GridFiles', 'sleep 600',
    'activate GridCalendar', 'sleep 500', 'dump calendar-open',
    'roll GridCalendar', 'sleep 600', 'dump calendar-rolled',
    'unroll GridCalendar', 'sleep 700', 'dump calendar-restored', 'exit'
  )
  $process = Start-Process $stagedBinary -ArgumentList @('--portable','--config-dir',$configDir,'--no-hide-icons','--test-script',$scriptPath,'--exit-after','10000') -PassThru -Wait
  if ($process.ExitCode -ne 0) { throw 'Calendar roll regression failed' }
  $calendarLog = (Get-ChildItem $env:LOCALAPPDATA -Recurse -Filter '*.log' | ForEach-Object { Get-Content $_.FullName -Raw -Encoding UTF8 }) -join "`n"
  Assert-RollGeometry $calendarLog 'calendar-open' 'calendar-rolled' 'calendar-restored'

  Write-Output "PASS: native TODO/calendar collapse geometry and expand restore; widget grids on reload, mixed-tab switch and detach; TODO operations plus multiline note edit, undo, native editor cancellation and restart restore ($instance)"
  $passed = $true
} finally {
  $env:PECOFENCE_INSTANCE = $previousInstance
  $env:LOCALAPPDATA = $previousLocalAppData
  $env:APPDATA = $previousAppData
  $env:RUST_LOG = $previousRustLog
  if ($passed -and (Test-Path $stage)) { Remove-Item -Recurse -Force $stage }
  elseif (!$passed) { Write-Warning "Test artifacts retained: $stage" }
}
