param(
  [string]$Binary,
  [ValidateRange(1, 20)][int]$Rounds = 6
)

# Uses an isolated workspace and the real WinUI title-bar button. No user installation input.
$ErrorActionPreference = 'Stop'
if (-not $Binary) { $Binary = Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\debug\pecofence.exe' }
$source = (Resolve-Path $Binary).ProviderPath
$stage = Join-Path $env:TEMP ('pecofence-navigation-' + [guid]::NewGuid().ToString('N'))
$previous = @{}
foreach ($name in @('PECOFENCE_INSTANCE', 'LOCALAPPDATA', 'APPDATA', 'RUST_LOG')) {
  $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
$env:PECOFENCE_INSTANCE = Split-Path $stage -Leaf
$env:LOCALAPPDATA = Join-Path $stage 'local'
$env:APPDATA = Join-Path $stage 'roaming'
$env:RUST_LOG = 'debug'
$process = $null
$passed = $false
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NavigationRegression {
  delegate bool Callback(IntPtr hwnd, IntPtr value);
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
  [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb, IntPtr value);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder name, int size);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr w, IntPtr l);
  public static IntPtr Find(int process, bool settings) {
    IntPtr found = IntPtr.Zero;
    EnumWindows(delegate(IntPtr hwnd, IntPtr value) {
      uint pid; GetWindowThreadProcessId(hwnd, out pid);
      if (pid == (uint)process) {
        var name = new StringBuilder(256); GetClassName(hwnd, name, 256);
        if (settings ? IsWindowVisible(hwnd) && name.ToString().Contains("WinUI") : name.ToString() == "PecoFence.Control") {
          found = hwnd; return false;
        }
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
'@

function Elements {
  $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
}
function HeadingOffset {
  $rect = New-Object NavigationRegression+Rect
  [void][NavigationRegression]::GetWindowRect($window, [ref]$rect)
  $headings = @(Elements | Where-Object {
    $b = $_.Current.BoundingRectangle
    $_.Current.ClassName -eq 'TextBlock' -and -not $_.Current.IsOffscreen -and
      $b.Height / $scale -ge 28 -and ($b.Y - $rect.top) / $scale -gt 40 -and
      ($b.Y - $rect.top) / $scale -lt 105
  })
  if ($headings.Count -ne 1) { throw "Expected one visible page heading, found $($headings.Count)" }
  ($headings[0].Current.BoundingRectangle.X - $rect.left) / $scale
}
function AssertPane([bool]$Open, [string]$Phase) {
  $x = HeadingOffset
  if (($Open -and $x -lt 260) -or (-not $Open -and $x -gt 130)) {
    throw "$Phase : expected pane open=$Open; content heading offset=$x"
  }
}
function ResizeSettings([int]$Width) {
  if (-not [NavigationRegression]::SetWindowPos($window, [IntPtr]::Zero, 0, 0, [int]($Width * $scale), [int](780 * $scale), 0x16)) {
    throw 'Could not resize the isolated settings window'
  }
}
function SelectPage([int]$Index) {
  $items = @(Elements | Where-Object { $_.Current.ClassName -like '*NavigationViewItem*' })
  if ($items.Count -lt 5) {
    Write-Output ((Elements | ForEach-Object { $_.Current.ClassName } | Group-Object | Select-Object Name, Count) | ConvertTo-Json)
    throw 'Native navigation items were not found'
  }
  $selection = $items[$Index].GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
  $selection.Select()
  Start-Sleep -Milliseconds 350
}

try {
  New-Item -ItemType Directory -Force -Path $stage, $env:LOCALAPPDATA, $env:APPDATA | Out-Null
  $stagedBinary = Join-Path $stage 'pecofence.exe'
  Copy-Item -LiteralPath $source -Destination $stagedBinary
  $runtime = Get-Content -Raw -Encoding UTF8 (Join-Path $PSScriptRoot '..\third_party\winappsdk\runtime-manifest.json') | ConvertFrom-Json
  foreach ($entry in $runtime.files) {
    $destination = Join-Path $stage $entry.path
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $source) $entry.path) -Destination $destination
  }
  $script = Join-Path $stage 'wait.txt'
  [IO.File]::WriteAllText($script, "sleep 600000`nexit`n", [Text.UTF8Encoding]::new($false))
  $process = Start-Process $stagedBinary -PassThru -ArgumentList @('--portable', '--config-dir', (Join-Path $stage 'config'), '--no-hide-icons', '--open-settings', '--test-script', $script, '--exit-after', '120000')
  $null = $process.Handle
  $window = [IntPtr]::Zero
  for ($i = 0; $i -lt 100; $i++) {
    if ($process.HasExited) { throw 'Isolated application exited before settings readiness' }
    $window = [NavigationRegression]::Find($process.Id, $true)
    if ($window -ne [IntPtr]::Zero) { break }
    Start-Sleep -Milliseconds 100
  }
  if ($window -eq [IntPtr]::Zero) { throw 'Settings window did not appear' }
  $scale = [NavigationRegression]::GetDpiForWindow($window) / 96.0
  ResizeSettings 1080
  Start-Sleep -Seconds 2
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($window)
  $rect = New-Object NavigationRegression+Rect
  [void][NavigationRegression]::GetWindowRect($window, [ref]$rect)
  $hamburger = $null
  foreach ($element in (Elements)) {
    $b = $element.Current.BoundingRectangle
    $pattern = $null
    if ($b.Width -gt 0 -and ($b.X - $rect.left) / $scale -lt 60 -and ($b.Y - $rect.top) / $scale -lt 45 -and
      $element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
      $hamburger = $pattern; break
    }
  }
  if (-not $hamburger) { throw 'Native title-bar hamburger was not found' }
  $open = (HeadingOffset) -gt 240
  for ($round = 0; $round -lt $Rounds; $round++) {
    $burst = $round % 4 + 1
    for ($click = 0; $click -lt $burst; $click++) {
      $hamburger.Invoke(); $open = -not $open
      Start-Sleep -Milliseconds 20
    }
    Start-Sleep -Milliseconds 450
    AssertPane $open "toggle burst $round"
    SelectPage ($round % 5)
    AssertPane $open "page switch $round"
  }
  if ($open) { $hamburger.Invoke() }
  Start-Sleep -Milliseconds 450
  AssertPane $false 'manual close'
  for ($round = 0; $round -lt $Rounds; $round++) {
    foreach ($width in @(800, 580, 1080)) { ResizeSettings $width; Start-Sleep -Milliseconds 100 }
    Start-Sleep -Milliseconds 450
    AssertPane $false "adaptive resize $round"
  }
  $hamburger.Invoke()
  Start-Sleep -Milliseconds 450
  AssertPane $true 'explicit reopen after resize'
  $control = [NavigationRegression]::Find($process.Id, $false)
  [void][NavigationRegression]::PostMessage($control, 0x800e, [IntPtr]::Zero, [IntPtr]::Zero)
  if (-not $process.WaitForExit(15000) -or $process.ExitCode -ne 0) { throw 'Isolated application did not exit cleanly' }
  $passed = $true
  Write-Output "PASS: real title-bar toggle bursts, manual close, compact/minimal/expanded resizing and explicit reopen ($Rounds rounds)."
} finally {
  if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id }
  foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
  if ($passed) { Remove-Item -LiteralPath $stage -Recurse -Force }
  else { Write-Warning "Navigation test artifacts retained: $stage" }
}
