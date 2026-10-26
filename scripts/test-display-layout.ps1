param(
  [string]$Binary = (Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\debug\pecofence.exe'),
  [string]$ReportDirectory = (Join-Path $PSScriptRoot '..\.cache\display-layout-audit')
)
$ErrorActionPreference = 'Stop'
$instance = 'display-layout-' + [guid]::NewGuid().ToString('N').Substring(0, 12)
$stage = Join-Path $env:TEMP $instance
$previous = @{}
foreach ($key in @('PECOFENCE_INSTANCE', 'PECOFENCE_UI_TEST_WINDOWS', 'LOCALAPPDATA', 'APPDATA', 'RUST_LOG')) {
  $previous[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
}
$process = $null
try {
  $binaryPath = (Resolve-Path -LiteralPath $Binary).ProviderPath
  $configDir = Join-Path $stage 'config'
  $desktop = Join-Path $stage 'desktop'
  New-Item -ItemType Directory -Force -Path $stage, $configDir, $desktop, $ReportDirectory | Out-Null
  [ordered]@{passed=$false; status='running'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Encoding UTF8
  $env:PECOFENCE_INSTANCE = $instance
  $env:PECOFENCE_UI_TEST_WINDOWS = '1'
  $env:LOCALAPPDATA = Join-Path $stage 'local-appdata'
  $env:APPDATA = Join-Path $stage 'roaming-appdata'
  $env:RUST_LOG = 'info'
  $fixture = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'fixtures\input-lifecycle.json') -Raw -Encoding UTF8 | ConvertFrom-Json
  $fixture.settings.desktopPath = $desktop
  $fixture.settings.language = 'en'
  $fixture.settings.peek.dim = $false
  $layout = $fixture.layouts[0]
  $layout.contents[0].title = 'DisplayClock'
  $layout.contents[0].content = [ordered]@{kind='panel'; panel=@{provider='pecofence.clock'; instanceId=[guid]::NewGuid().ToString(); configVersion=1; config=@{}}}
  $layout.contents[1].title = 'DisplayNote'
  $layout.contents[1].content = [ordered]@{kind='panel'; panel=@{provider='pecofence.note'; instanceId=[guid]::NewGuid().ToString(); configVersion=1; config=@{text='layout regression fixture'}}}
  $layout.containers[1].excludeFromQuickHide = $true
  $layout.contents[2].title = 'DisplayShared'
  $other = $layout | ConvertTo-Json -Depth 40 | ConvertFrom-Json
  $other.fingerprint = @(@{devicePath='display-layout-inactive'; workDip=@(1920.0, 1040.0); dpi=96})
  # A monitor-specific layout replaces the two widget hosts and retains the collection HWND.
  $other.containers[0].id = [guid]::NewGuid().ToString()
  $other.containers[1].id = [guid]::NewGuid().ToString()
  $other.containers[2].excludeFromQuickHide = $true
  $fixture.layouts = @($layout, $other)
  $utf8 = New-Object Text.UTF8Encoding($false)
  [IO.File]::WriteAllText((Join-Path $configDir 'workspace.v2.json'), ($fixture | ConvertTo-Json -Depth 40), $utf8)
  $stagedBinary = Join-Path $stage 'pecofence.exe'
  Copy-Item -LiteralPath $binaryPath -Destination $stagedBinary
  $runtime = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\third_party\winappsdk\runtime-manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
  foreach ($entry in $runtime.files) {
    $destination = Join-Path $stage $entry.path
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $binaryPath) $entry.path) -Destination $destination
  }
  $validation = & $stagedBinary --validate-workspace (Join-Path $configDir 'workspace.v2.json') | Out-String
  if ($LASTEXITCODE -ne 0) { throw "Display fixture validation failed: $validation" }
  $workflow = @(
    'sleep 700', 'dump initial',
    'hide-window DisplayShared', 'dump externally-hidden',
    'display-change', 'sleep 500', 'dump recovered',
    'display-change-layout 1', 'sleep 500', 'dump switched',
    'display-change-layout 0', 'sleep 500', 'dump returned',
    'quick-hide', 'sleep 350', 'dump hidden',
    'display-change-layout 1', 'sleep 500', 'dump switched-hidden',
    'display-change-layout 0', 'sleep 500', 'dump returned-hidden',
    'peek', 'sleep 500', 'dump peeking',
    'quick-hide-exclude DisplayShared true',
    'new-collection 100 100 300 200', 'sleep 500', 'dump peek-created',
    'end-peek', 'sleep 500', 'dump peek-ended',
    'delete New', 'sleep 500',
    'quick-hide-exclude DisplayShared false', 'sleep 350', 'dump exclusion-removed',
    'quick-show', 'sleep 500', 'dump restored', 'exit'
  )
  $scriptPath = Join-Path $stage 'workflow.txt'
  [IO.File]::WriteAllLines($scriptPath, $workflow, $utf8)
  $stdout = Join-Path $stage 'stdout.txt'
  $stderr = Join-Path $stage 'stderr.txt'
  $arguments = @('--portable', '--no-hide-icons', '--config-dir', ('"' + $configDir + '"'), '--test-script', ('"' + $scriptPath + '"'), '--exit-after', '15000')
  $process = Start-Process -FilePath $stagedBinary -ArgumentList $arguments -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
  $null = $process.Handle
  if (!$process.WaitForExit(20000)) { throw 'Display layout fixture timed out' }
  $logs = @((Get-Content -LiteralPath $stdout -Raw -Encoding UTF8), (Get-Content -LiteralPath $stderr -Raw -Encoding UTF8)) -join "`n"
  $logs | Set-Content -LiteralPath (Join-Path $ReportDirectory 'native.log') -Encoding UTF8
  if ($process.ExitCode -ne 0) { throw "Native process exited $($process.ExitCode)" }
  if ($logs -cmatch '\bERROR\b|PANIC|panicked at') { throw 'Native log contains an error or panic' }
  $lines = $logs -split "`r?`n"
  function Snapshot([string]$tag, [string]$title) {
    $found = @($lines | Where-Object { $_ -match ('\[' + [regex]::Escape($tag) + '\] live .*title="' + $title + '"') })
    if ($found.Count -ne 1) { throw "Expected one $title window at $tag, found $($found.Count)" }
    return $found[0]
  }
  foreach ($tag in @('initial', 'recovered', 'switched', 'returned', 'restored', 'hidden', 'switched-hidden', 'returned-hidden', 'peeking', 'exclusion-removed')) {
    $live = @($lines | Where-Object { $_ -match ('\[' + $tag + '\] live ') })
    if ($live.Count -ne 3) { throw "Expected three live windows at $tag" }
    foreach ($title in @('DisplayClock', 'DisplayNote', 'DisplayShared')) {
      $line = Snapshot $tag $title
      $visible = $tag -notin @('hidden', 'switched-hidden', 'returned-hidden', 'exclusion-removed') -or $title -eq 'DisplayNote' -or ($tag -eq 'switched-hidden' -and $title -eq 'DisplayShared')
      if ($line -notmatch ('visible=' + $visible.ToString().ToLowerInvariant() + ' ')) { throw "Incorrect visibility: $tag $title" }
    }
  }
  if ((Snapshot 'externally-hidden' 'DisplayShared') -notmatch 'visible=false') { throw 'External hide fault was not applied' }
  foreach ($tag in @('peek-created', 'peek-ended')) {
    $live = @($lines | Where-Object { $_ -match ('\[' + $tag + '\] live ') })
    if ($live.Count -ne 4) { throw "Expected four live windows at $tag" }
    foreach ($title in @('DisplayClock', 'DisplayNote', 'DisplayShared', 'New fence')) {
      $visible = $tag -eq 'peek-created' -or $title -in @('DisplayNote', 'DisplayShared')
      if ((Snapshot $tag $title) -notmatch ('visible=' + $visible.ToString().ToLowerInvariant() + ' ')) { throw "Incorrect Peek visibility: $tag $title" }
    }
  }
  foreach ($tag in @('peeking', 'peek-created')) {
    if ($logs -notmatch ('\[' + $tag + '\] fences=\d+ dying=\d+ peek=true')) { throw "Peek did not stay active at $tag" }
  }
  foreach ($title in @('DisplayClock', 'DisplayNote')) {
    $initial = [regex]::Match((Snapshot 'initial' $title), 'hwnd=\S+').Value
    $switched = [regex]::Match((Snapshot 'switched' $title), 'hwnd=\S+').Value
    if ($initial -eq $switched) { throw "Layout switch did not recreate $title" }
  }
  $shared = [regex]::Match((Snapshot 'initial' 'DisplayShared'), 'hwnd=\S+').Value
  foreach ($tag in @('recovered', 'switched', 'returned', 'switched-hidden', 'returned-hidden', 'restored')) {
    if ([regex]::Match((Snapshot $tag 'DisplayShared'), 'hwnd=\S+').Value -ne $shared) { throw 'Shared container HWND was replaced' }
  }
  [ordered]@{passed=$true; scope='isolated native layout switch, retained-window recovery, per-layout exclusions, Peek creation and current-policy restoration'; log=(Join-Path $ReportDirectory 'native.log')} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Encoding UTF8
  Get-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Raw -Encoding UTF8
} catch {
  [ordered]@{passed=$false; status='failed'; error=$_.Exception.Message} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Encoding UTF8
  throw
} finally {
  if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id -Force; $process.WaitForExit() }
  foreach ($key in $previous.Keys) { [Environment]::SetEnvironmentVariable($key, $previous[$key], 'Process') }
  if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
}
