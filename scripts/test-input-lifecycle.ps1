param(
  [string]$Binary = (Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\debug\pecofence.exe'),
  [string]$ReportDirectory = (Join-Path $PSScriptRoot '..\.cache\input-lifecycle-audit'),
  [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot)
)
$ErrorActionPreference = 'Stop'
$instance = 'input-lifecycle-' + [guid]::NewGuid().ToString('N').Substring(0, 12)
$stage = Join-Path $env:TEMP $instance
$previous = @{}
foreach ($key in @('PECOFENCE_INSTANCE', 'PECOFENCE_UI_TEST_WINDOWS', 'LOCALAPPDATA', 'APPDATA', 'RUST_LOG')) {
  $previous[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
}
$process = $null
try {
  $binaryPath = (Resolve-Path -LiteralPath $Binary).ProviderPath
  $configDir = Join-Path $stage 'config'
  $files = Join-Path $stage 'files'
  $desktop = Join-Path $stage 'desktop'
  New-Item -ItemType Directory -Force -Path $stage, $configDir, $files, $desktop, $ReportDirectory | Out-Null
  [ordered]@{instance=$instance; passed=$false; status='running'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Encoding UTF8
  $env:PECOFENCE_INSTANCE = $instance
  $env:PECOFENCE_UI_TEST_WINDOWS = '1'
  $env:LOCALAPPDATA = Join-Path $stage 'local-appdata'
  $env:APPDATA = Join-Path $stage 'roaming-appdata'
  $env:RUST_LOG = 'info'
  foreach ($index in 0..19) { Set-Content -LiteralPath (Join-Path $files ('Fixture-{0:D2}.txt' -f $index)) -Value 'owned lifecycle fixture' }
  $fixture = Get-Content -LiteralPath (Join-Path $RepositoryRoot 'scripts\fixtures\input-lifecycle.json') -Raw -Encoding UTF8 | ConvertFrom-Json
  $fixture.settings.desktopPath = $desktop
  foreach ($content in $fixture.layouts[0].contents) { if ($content.content.kind -eq 'folderPortal') { $content.content.root = $files } }
  $utf8 = New-Object Text.UTF8Encoding($false)
  [IO.File]::WriteAllText((Join-Path $configDir 'workspace.v2.json'), ($fixture | ConvertTo-Json -Depth 40), $utf8)
  Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $stage 'pecofence.exe')
  $runtime = Get-Content -LiteralPath (Join-Path $RepositoryRoot 'third_party\winappsdk\runtime-manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
  foreach ($entry in $runtime.files) {
    $destination = Join-Path $stage $entry.path
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $binaryPath) $entry.path) -Destination $destination
  }
  $validation = & (Join-Path $stage 'pecofence.exe') --validate-workspace (Join-Path $configDir 'workspace.v2.json') | Out-String
  $validation | Set-Content -LiteralPath (Join-Path $ReportDirectory 'fixture-validation.txt') -Encoding UTF8
  if ($LASTEXITCODE -ne 0) { throw 'Lifecycle fixture validation failed before GUI startup' }
  $workflow = @(
    'sleep 700', 'bounds LifecycleA 300 200 700 400', 'merge LifecycleB LifecycleA', 'sleep 450', 'reorder LifecycleB 1', 'sleep 250',
    'input LifecycleA down 40 10', 'input LifecycleA move 600 10', 'dump held-tab',
    'quick-hide', 'dump cancelled-tab-hide',
    'input LifecycleA up 600 10', 'input LifecycleA down 600 10', 'dump blocked-hidden-tab',
    'sleep 350', 'quick-show', 'sleep 450',
    'input LifecycleA down 50 100', 'dump held-body-roll', 'roll LifecycleA', 'sleep 50', 'dump cancelled-body-roll',
    'input LifecycleA up 50 100', 'sleep 350', 'dump rolled', 'unroll LifecycleA', 'sleep 450',
    'input LifecycleA down 50 100', 'dump held-body-hide', 'quick-hide', 'dump cancelled-body-hide',
    'sleep 350', 'input LifecycleA up 50 100', 'dump hidden-up', 'quick-show', 'sleep 450', 'dump restored', 'exit'
  )
  $scriptPath = Join-Path $stage 'workflow.txt'
  [IO.File]::WriteAllLines($scriptPath, $workflow, $utf8)
  $stdout = Join-Path $stage 'stdout.txt'
  $stderr = Join-Path $stage 'stderr.txt'
  $arguments = @('--portable', '--no-hide-icons', '--config-dir', ('"' + $configDir + '"'), '--test-script', ('"' + $scriptPath + '"'), '--exit-after', '18000')
  $process = Start-Process -FilePath (Join-Path $stage 'pecofence.exe') -ArgumentList $arguments -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
  $null = $process.Handle # retain the native handle so ExitCode remains available
  if (!$process.WaitForExit(25000)) { throw 'Isolated lifecycle process timed out' }
  $logs = @((Get-Content -LiteralPath $stdout -Raw -Encoding UTF8), (Get-Content -LiteralPath $stderr -Raw -Encoding UTF8)) -join "`n"
  $logs | Set-Content -LiteralPath (Join-Path $ReportDirectory 'native.log') -Encoding UTF8
  if ($process.ExitCode -ne 0) { throw "Native process exited $($process.ExitCode)" }
  $lines = $logs -split "`r?`n"
  $heldWindows = @($lines | Where-Object { $_ -match '\[held-tab\] live .*tabs=2 ' })
  if ($heldWindows.Count -ne 1) { throw 'Expected one merged fixture window' }
  $windowId = [regex]::Match($heldWindows[0], 'hwnd=(0x[0-9a-f]+)').Groups[1].Value
  function Snapshot([string]$tag) {
    $found = @($lines | Where-Object { $_ -match ('\[' + [regex]::Escape($tag) + '\] live hwnd=' + $windowId + ' ') })
    if ($found.Count -ne 1) { throw "Expected one LifecycleA snapshot: $tag ($($found.Count))" }
    return $found[0]
  }
  foreach ($tag in @('held-tab', 'held-body-roll', 'held-body-hide')) {
    if ((Snapshot $tag) -notmatch 'capture=true') { throw "The fixture never captured: $tag" }
  }
  if ((Snapshot 'held-tab') -notmatch 'tab_drag=true') { throw 'The fixture never started a tab gesture' }
  foreach ($tag in @('cancelled-tab-hide', 'blocked-hidden-tab', 'cancelled-body-roll', 'rolled', 'cancelled-body-hide', 'hidden-up', 'restored')) {
    $line = Snapshot $tag
    foreach ($flag in @('capture', 'tab_drag', 'window_drag', 'detach_pending', 'pressed', 'marquee', 'scrollbar_press')) {
      if ($line -notmatch ($flag + '=false')) { throw "Gesture survived $tag ($flag)" }
    }
  }
  foreach ($tag in @('cancelled-tab-hide', 'blocked-hidden-tab', 'cancelled-body-hide', 'hidden-up')) {
    if ((Snapshot $tag) -notmatch 'input_window=false input_body=false') { throw "Inactive input remained available: $tag" }
  }
  if ((Snapshot 'rolled') -notmatch 'input_window=true input_body=false') { throw 'Collapsed title and body were not separated' }
  if ((Snapshot 'restored') -notmatch 'input_window=true input_body=true') { throw 'Expanded input was not restored' }
  if ($logs -cmatch '\bERROR\b|PANIC|panicked at') { throw 'Native log contains an error or panic' }
  $saved = Get-Content -LiteralPath (Join-Path $configDir 'workspace.v2.json') -Raw -Encoding UTF8 | ConvertFrom-Json
  $saved | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'saved-fixture.json') -Encoding UTF8
  $containers = @(foreach ($layout in $saved.layouts) { $layout.containers | Where-Object { $_.tabs.Count -eq 2 -and $_.tabs -contains '11111111-1111-4111-8111-000000000001' } })
  if ($containers.Count -ne 1) { throw 'Expected one persisted merged fixture container' }
  $container = $containers[0]
  if (($container.tabs -join ',') -cne '11111111-1111-4111-8111-000000000001,11111111-1111-4111-8111-000000000002') { throw 'Cancelled tab order was persisted' }
  [ordered]@{ instance = $instance; passed = $true; exitCode = $process.ExitCode; log = (Join-Path $ReportDirectory 'native.log'); scope = 'isolated native mouse messages, capture, hide/roll; no physical input, OLE or DPI matrix' } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Encoding UTF8
  Get-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Raw -Encoding UTF8
} finally {
  if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id -Force; $process.WaitForExit() }
  foreach ($key in $previous.Keys) { [Environment]::SetEnvironmentVariable($key, $previous[$key], 'Process') }
  if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
}
