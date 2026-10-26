param(
  # A debug build: the demo board is created through the test script.
  [Parameter(Mandatory=$true)][string]$RepoRoot,
  [string]$Binary = '',
  [ValidateSet('dark', 'light')][string]$Theme = 'dark',
  # Text scale in percent for this instance only (debug builds); the system setting is not changed.
  [int]$TextScale = 0,
  # With -Out, run a scripted tour and save screenshots there, then exit.
  [string]$Out = '',
  [ValidateSet('tour')][string]$Tour = 'tour'
)
# Evidence board demo in an isolated, portable PecoFence instance. The board uses the
# deterministic demo source (fictional Orion data); it does not contact SPM.
# Interactive:  pwsh scripts/demo-board.ps1 [-Theme light]
# Screenshots:  pwsh scripts/demo-board.ps1 -Out C:\temp\board-shots [-Tour tour|scaled|live]
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class BoardShot {
  delegate bool Callback(IntPtr hwnd, IntPtr value);
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
  [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb, IntPtr value);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
  // The largest visible top-level window of the process: the board fence.
  public static IntPtr Largest(int process) {
    IntPtr found = IntPtr.Zero; long best = 0;
    EnumWindows(delegate(IntPtr hwnd, IntPtr value) {
      uint pid; GetWindowThreadProcessId(hwnd, out pid);
      Rect r;
      if (pid == (uint)process && IsWindowVisible(hwnd) && GetWindowRect(hwnd, out r)) {
        long area = (long)(r.right - r.left) * (r.bottom - r.top);
        if (area > best) { best = area; found = hwnd; }
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
'@
[void][BoardShot]::SetProcessDpiAwarenessContext([IntPtr]-4)

if (-not $Binary) { $Binary=Join-Path $RepoRoot 'target\x86_64-pc-windows-msvc\debug\pecofence.exe' }
$source = (Resolve-Path $Binary).ProviderPath
$stage = Join-Path $env:TEMP ('pecofence-ci-watch-' + [guid]::NewGuid().ToString('N'))
$saved = @{}
foreach ($key in 'PECOFENCE_INSTANCE', 'PECOFENCE_UI_TEST_WINDOWS', 'PECOFENCE_TEST_TEXT_SCALE', 'LOCALAPPDATA', 'APPDATA') {
  $saved[$key] = [Environment]::GetEnvironmentVariable($key)
}
$env:PECOFENCE_INSTANCE = Split-Path $stage -Leaf
$env:PECOFENCE_UI_TEST_WINDOWS = '1'
$env:LOCALAPPDATA = Join-Path $stage 'local'
$env:APPDATA = Join-Path $stage 'roaming'
if ($TextScale -gt 0) { $env:PECOFENCE_TEST_TEXT_SCALE = "$TextScale" }
$config = Join-Path $stage 'config'
New-Item -ItemType Directory -Force -Path $stage, $env:LOCALAPPDATA, $env:APPDATA, $config | Out-Null
$staged = Join-Path $stage 'pecofence.exe'
Copy-Item -LiteralPath $source -Destination $staged
$runtime = Get-Content -Raw -Encoding UTF8 (Join-Path $RepoRoot 'third_party\winappsdk\runtime-manifest.json') | ConvertFrom-Json
foreach ($entry in $runtime.files) {
  $destination = Join-Path $stage $entry.path
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
  Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $source) $entry.path) -Destination $destination
}

function Key([int]$vk) { "input Board vk $vk 0" }
$F5 = Key 116; $Down = Key 40; $Right = Key 39; $Enter = Key 13; $Esc = Key 27
# Each step: script lines to run, then a screenshot named after the step.
$steps = @(
  @{ name = 'ci-builds'; lines = @() },
  @{ name = 'ci-watchlist'; lines = @('input CI vk 39 0') }
)
$create = 'new-ci-watch 40 40 480 780'
$lines = [Collections.Generic.List[string]]::new()
$lines.Add('sleep 2500')
$lines.Add('set-setting widgetPaperStyle "' + $Theme + '"')
$lines.Add($create)
$lines.Add('sleep 400')
$lines.Add('pin-test-windows')
if ($Out) {
  New-Item -ItemType Directory -Force -Path $Out | Out-Null
  for ($i = 0; $i -lt $steps.Count; $i++) {
    foreach ($line in $steps[$i].lines) { $lines.Add($line); $lines.Add('sleep 150') }
    $lines.Add('wait-file ' + (Join-Path $stage "go$i"))
  }
  $lines.Add('exit')
} else {
  $lines.Add('sleep 86400000')
}
$script = Join-Path $stage 'ci-watch.txt'
[IO.File]::WriteAllText($script, ($lines -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
$themeArg = if ($Theme -eq 'dark') { '--dark' } else { '--light' }
$arguments = @('--portable', '--config-dir', ('"' + $config + '"'), '--no-hide-icons', $themeArg, '--test-script', ('"' + $script + '"'))
if ($Out) { $arguments += @('--exit-after', '180000') }
$process = Start-Process $staged -PassThru -ArgumentList $arguments
try {
  if (-not $Out) {
    Write-Output "Demo board running (pid $($process.Id)). Close it from the tray menu; this window waits."
    $process.WaitForExit()
    return
  }
  Start-Sleep -Milliseconds 5500
  for ($i = 0; $i -lt $steps.Count; $i++) {
    $wait = if ($steps[$i].wait) { $steps[$i].wait } elseif ($i -eq 0) { 1500 } else { 2600 }
    Start-Sleep -Milliseconds $wait
    $window = [BoardShot]::Largest($process.Id)
    if ($window -eq [IntPtr]::Zero) { throw 'no board window' }
    $r = New-Object BoardShot+Rect; [void][BoardShot]::GetWindowRect($window, [ref]$r)
    $bmp = New-Object System.Drawing.Bitmap ($r.right - $r.left), ($r.bottom - $r.top)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.left, $r.top, 0, 0, $bmp.Size)
    $file = Join-Path $Out "$Theme-$($steps[$i].name).png"
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output "saved $file ($($r.right - $r.left) x $($r.bottom - $r.top))"
    New-Item -ItemType File -Path (Join-Path $stage "go$i") | Out-Null
  }
  $process.WaitForExit(15000) | Out-Null
} finally {
  if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
  Start-Sleep -Milliseconds 500
  foreach ($key in $saved.Keys) { [Environment]::SetEnvironmentVariable($key, $saved[$key]) }
  Remove-Item -Recurse -Force -LiteralPath $stage -ErrorAction SilentlyContinue
}
