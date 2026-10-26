param(
  [string]$Binary = (Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\debug\pecofence.exe'),
  [string]$ReportDirectory = (Join-Path $PSScriptRoot '..\.cache\caption-controls-audit')
)
# Synthetic, isolated native input against the demo board. No private backend,
# physical pointer movement, desktop icon changes or existing configuration.
$ErrorActionPreference = 'Stop'
$instance = 'caption-' + [guid]::NewGuid().ToString('N').Substring(0,12)
$stage = Join-Path $env:TEMP $instance
$previous = @{}
$process = $null
foreach ($key in @('PECOFENCE_INSTANCE','PECOFENCE_UI_TEST_WINDOWS','PECOFENCE_HOLIDAY_OFFLINE','LOCALAPPDATA','APPDATA','RUST_LOG')) {
  $previous[$key] = [Environment]::GetEnvironmentVariable($key,'Process')
}
try {
  $source = (Resolve-Path -LiteralPath $Binary).ProviderPath
  $config = Join-Path $stage 'config'
  $desktop = Join-Path $stage 'desktop'
  New-Item -ItemType Directory -Force $stage,$config,$desktop,$ReportDirectory | Out-Null
  $env:PECOFENCE_INSTANCE = $instance
  $env:PECOFENCE_UI_TEST_WINDOWS = '1'
  $env:PECOFENCE_HOLIDAY_OFFLINE = '1'
  $env:LOCALAPPDATA = Join-Path $stage 'local'
  $env:APPDATA = Join-Path $stage 'roaming'
  $env:RUST_LOG = 'info'
  $fixture = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'fixtures\input-lifecycle.json') -Raw -Encoding UTF8 | ConvertFrom-Json
  $fixture.settings.desktopPath = $desktop
  foreach ($content in $fixture.layouts[0].contents) {
    if ($content.content.kind -eq 'folderPortal') { $content.content.root = $desktop }
  }
  $utf8 = [Text.UTF8Encoding]::new($false)
  [IO.File]::WriteAllText((Join-Path $config 'workspace.v2.json'),($fixture | ConvertTo-Json -Depth 40),$utf8)
  $binaryPath = Join-Path $stage 'pecofence.exe'
  Copy-Item -LiteralPath $source -Destination $binaryPath
  $runtime = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\third_party\winappsdk\runtime-manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
  foreach ($entry in $runtime.files) {
    $destination = Join-Path $stage $entry.path
    New-Item -ItemType Directory -Force (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $source) $entry.path) -Destination $destination
  }
  $workflow = @('sleep 700','new-board 40 40 900 700','sleep 700','bounds Board 40 40 900 700','sleep 400','dump baseline',
    'input Board caption-edges 0 0',
    'input Board caption-max-down 0 0','input Board caption-max-outside-up 0 0','sleep 250','dump cancelled-outside',
    'input Board caption-max-down 0 0','input Board capture-lost 0 0','input Board caption-max-up 0 0','sleep 250','dump cancelled-capture')
  foreach ($cycle in 1..3) {
    $workflow += @('input Board caption-max-down 0 0','input Board caption-max-up 0 0','sleep 400',"dump maximized-$cycle",'input Board caption-edges 0 0',
      'input Board caption-max-down 0 0','input Board caption-max-up 0 0','sleep 400',"dump restored-$cycle")
  }
  $workflow += @('input Board caption-close-down 0 0','input Board caption-close-up 0 0','sleep 600','dump closed','exit')
  $scriptPath = Join-Path $stage 'workflow.txt'
  [IO.File]::WriteAllLines($scriptPath,$workflow,$utf8)
  $stdout = Join-Path $stage 'stdout.txt'; $stderr = Join-Path $stage 'stderr.txt'
  $process = Start-Process -FilePath $binaryPath -ArgumentList @('--portable','--no-hide-icons','--config-dir',('"'+$config+'"'),'--test-script',('"'+$scriptPath+'"'),'--exit-after','25000') -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
  $null = $process.Handle
  if (!$process.WaitForExit(35000)) { throw 'Isolated caption test did not exit within its deadline' }
  $logs = (Get-Content $stdout -Raw -Encoding UTF8) + (Get-Content $stderr -Raw -Encoding UTF8)
  $logs | Set-Content -LiteralPath (Join-Path $ReportDirectory 'native.log') -Encoding UTF8
  if ($process.ExitCode -ne 0) { throw "Native caption test exited $($process.ExitCode)" }
  function Snapshot([string]$tag) {
    $pattern = '\[' + [regex]::Escape($tag) + '\] live .*title="Board"'
    $matches = @($logs -split "`r?`n" | Where-Object { $_ -match $pattern })
    if ($matches.Count -ne 1) { throw "Expected one Board snapshot at $tag" }
    $rect = [regex]::Match($matches[0],'rect=\(([^)]+)\)').Groups[1].Value
    if ($matches[0] -notmatch 'capture=false .*pressed=false') { throw "Input state was left held at $tag" }
    return $rect
  }
  $baseline = Snapshot 'baseline'
  foreach ($tag in @('cancelled-outside','cancelled-capture','restored-1','restored-2','restored-3')) {
    if ((Snapshot $tag) -ne $baseline) { throw "Unexpected geometry after $tag" }
  }
  $maximized = Snapshot 'maximized-1'
  if ($maximized -eq $baseline) { throw 'Caption Maximize did not change geometry' }
  foreach ($cycle in 2..3) {
    if ((Snapshot "maximized-$cycle") -ne $maximized) { throw 'Repeated caption clicks changed maximized geometry' }
  }
  if ($logs -match '\[closed\] live .*title="Board"') { throw 'Caption Close did not retire the Board window' }
  $saved = Get-Content (Join-Path $config 'workspace.v2.json') -Raw -Encoding UTF8 | ConvertFrom-Json
  $closed = @(foreach ($layout in $saved.layouts) { $layout.containers | Where-Object { $_.closed } })
  if ($closed.Count -ne 1) { throw 'Caption Close was not persisted' }
  [ordered]@{passed=$true;cycles=3;baseline=$baseline;maximized=$maximized;scope='native synthetic messages, edge hit tests, release outside, capture cancellation, maximize/restore and persisted close'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Encoding UTF8
  Get-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Raw -Encoding UTF8
} finally {
  if ($process -and !$process.HasExited) { $null = $process.WaitForExit(35000) }
  foreach ($key in $previous.Keys) { [Environment]::SetEnvironmentVariable($key,$previous[$key],'Process') }
  if (!$process -or $process.HasExited) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue }
}
