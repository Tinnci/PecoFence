param(
  [string]$Binary = (Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\debug\pecofence.exe'),
  [Parameter(Mandatory = $true)][string]$Out,
  [ValidateSet('dark', 'light')][string]$Theme = 'dark',
  [string]$Manifest = (Join-Path $PSScriptRoot '..\third_party\winappsdk\runtime-manifest.json')
)
# Hosting spike: a demo board fence re-parented into a WinUI workspace window.
# Drives real mouse and keyboard input, records a probe line and a screenshot per
# step in -Out (probes.jsonl, <theme>-<step>.png). Isolated portable instance;
# nothing contacts SPM and no system setting changes.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Spike {
  delegate bool Callback(IntPtr hwnd, IntPtr value);
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct Pt { public int x, y; }
  [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb, IntPtr value);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Pt p);
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, IntPtr e);
  [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  // The largest visible top-level window of the process: the workspace window.
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
  public static void Click(int x, int y) {
    SetCursorPos(x, y);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero);
    mouse_event(0x0004, 0, 0, 0, IntPtr.Zero);
  }
  public static void Key(byte vk) {
    keybd_event(vk, 0, 0, IntPtr.Zero);
    keybd_event(vk, 0, 2, IntPtr.Zero);
  }
}
'@
[void][Spike]::SetProcessDpiAwarenessContext([IntPtr]-4)

$source = (Resolve-Path $Binary).ProviderPath
$stage = Join-Path $env:TEMP ('pecofence-spike-' + [guid]::NewGuid().ToString('N'))
$saved = @{}
foreach ($key in 'PECOFENCE_INSTANCE', 'PECOFENCE_UI_TEST_WINDOWS', 'LOCALAPPDATA', 'APPDATA') {
  $saved[$key] = [Environment]::GetEnvironmentVariable($key)
}
$env:PECOFENCE_INSTANCE = Split-Path $stage -Leaf
$env:PECOFENCE_UI_TEST_WINDOWS = '1'
$env:LOCALAPPDATA = Join-Path $stage 'local'
$env:APPDATA = Join-Path $stage 'roaming'
$config = Join-Path $stage 'config'
New-Item -ItemType Directory -Force -Path $stage, $env:LOCALAPPDATA, $env:APPDATA, $config, $Out | Out-Null
$staged = Join-Path $stage 'pecofence.exe'
Copy-Item -LiteralPath $source -Destination $staged
$runtime = Get-Content -Raw -Encoding UTF8 $Manifest | ConvertFrom-Json
foreach ($entry in $runtime.files) {
  $destination = Join-Path $stage $entry.path
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
  Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $source) $entry.path) -Destination $destination
}
$probes = Join-Path $Out 'probes.jsonl'
Remove-Item -LiteralPath $probes -ErrorAction SilentlyContinue

# App-side steps: each runs after the driver drops go<i>, then the app probes.
$steps = @(
  @{ name = 'attached'; app = @() },
  @{ name = 'click-board'; app = @() },
  @{ name = 'board-keys'; app = @() },
  @{ name = 'click-textbox'; app = @() },
  @{ name = 'type-textbox'; app = @() },
  @{ name = 'inactive-click-board'; app = @() },
  @{ name = 'resize-narrow'; app = @('spike-move 60 40 1000 700') },
  @{ name = 'resize-wide'; app = @('spike-move 20 20 1700 1000') },
  @{ name = 'dialog'; app = @('spike-dialog on') },
  @{ name = 'dialog-closed'; app = @('spike-dialog off') }
)
$lines = [Collections.Generic.List[string]]::new()
$lines.Add('sleep 2500')
$lines.Add('new-board 40 40 1200 800')
$lines.Add('sleep 800')
$lines.Add('workspace-spike Board')
$lines.Add('sleep 2500')
for ($i = 0; $i -lt $steps.Count; $i++) {
  $lines.Add('wait-file ' + (Join-Path $stage "go$i"))
  foreach ($line in $steps[$i].app) { $lines.Add($line); $lines.Add('sleep 700') }
  $lines.Add('sleep 300')
  $lines.Add("spike-probe $($steps[$i].name) $probes")
  $lines.Add('write-file ' + (Join-Path $stage "done$i"))
}
$lines.Add('exit')
$script = Join-Path $stage 'spike.txt'
[IO.File]::WriteAllText($script, ($lines -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
$themeArg = if ($Theme -eq 'dark') { '--dark' } else { '--light' }
$arguments = @('--portable', '--config-dir', ('"' + $config + '"'), '--no-hide-icons', $themeArg, '--test-script', ('"' + $script + '"'), '--exit-after', '180000')
$process = Start-Process $staged -PassThru -ArgumentList $arguments

function Host-Point([IntPtr]$h, [double]$dipX, [double]$dipY) {
  $s = [Spike]::GetDpiForWindow($h) / 96.0
  $p = New-Object Spike+Pt; $p.x = [int]($dipX * $s); $p.y = [int]($dipY * $s)
  [void][Spike]::ClientToScreen($h, [ref]$p)
  return $p
}
function Client-Dip([IntPtr]$h) {
  $r = New-Object Spike+Rect; [void][Spike]::GetClientRect($h, [ref]$r)
  $s = [Spike]::GetDpiForWindow($h) / 96.0
  return @{ w = $r.right / $s; h = $r.bottom / $s }
}
function Wait-For([string]$path) {
  for ($t = 0; $t -lt 200 -and -not (Test-Path $path); $t++) { Start-Sleep -Milliseconds 100 }
}
try {
  Start-Sleep -Milliseconds 9000
  for ($i = 0; $i -lt $steps.Count; $i++) {
    $h = [Spike]::Largest($process.Id)
    if ($h -eq [IntPtr]::Zero) { throw 'no workspace window' }
    $c = Client-Dip $h
    $boardX = 168 + ($c.w - 168 - 330) / 2
    switch ($steps[$i].name) {
      'click-board' { [void][Spike]::SetForegroundWindow($h); Start-Sleep -Milliseconds 300; $p = Host-Point $h $boardX 200; [Spike]::Click($p.x, $p.y) }
      'board-keys' { [Spike]::Key(0x28); Start-Sleep -Milliseconds 150; [Spike]::Key(0x28); Start-Sleep -Milliseconds 150; [Spike]::Key(0x0D) }
      'click-textbox' { $p = Host-Point $h ($c.w - 165) 124; [Spike]::Click($p.x, $p.y) }
      'type-textbox' { foreach ($vk in 0x41, 0x42, 0x43) { [Spike]::Key([byte]$vk); Start-Sleep -Milliseconds 80 } }
      'inactive-click-board' {
        [void][Spike]::SetForegroundWindow([Spike]::GetConsoleWindow()); Start-Sleep -Milliseconds 500
        $p = Host-Point $h $boardX 300; [Spike]::Click($p.x, $p.y)
      }
    }
    New-Item -ItemType File -Path (Join-Path $stage "go$i") | Out-Null
    Wait-For (Join-Path $stage "done$i")
    Start-Sleep -Milliseconds 400
    $h = [Spike]::Largest($process.Id)
    $r = New-Object Spike+Rect; [void][Spike]::GetWindowRect($h, [ref]$r)
    $bmp = New-Object System.Drawing.Bitmap ($r.right - $r.left), ($r.bottom - $r.top)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.left, $r.top, 0, 0, $bmp.Size)
    $file = Join-Path $Out "$Theme-$($steps[$i].name).png"
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output "step $($steps[$i].name): saved $file"
  }
  $process.WaitForExit(15000) | Out-Null
} finally {
  if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
  Start-Sleep -Milliseconds 500
  foreach ($key in $saved.Keys) { [Environment]::SetEnvironmentVariable($key, $saved[$key]) }
  Remove-Item -Recurse -Force -LiteralPath $stage -ErrorAction SilentlyContinue
}
