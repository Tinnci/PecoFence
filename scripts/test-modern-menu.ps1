param(
  [Parameter(Mandatory = $true)][string]$TestBinary,
  [string]$OutputDirectory
)

# Only interacts with the isolated test process, never the installed desktop app.
$ErrorActionPreference = 'Stop'
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $PSScriptRoot '..\target\modern-menu-tests' }
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class MenuTestInput {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
  [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
}
'@
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$outputPath = (Resolve-Path $OutputDirectory).ProviderPath
$sourceBinary = (Resolve-Path $TestBinary).ProviderPath
$stage = Join-Path $env:TEMP ('pecofence-menu-runtime-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
$stagedBinary = Join-Path $stage 'pecofence-menu-test.exe'
Copy-Item -LiteralPath $sourceBinary -Destination $stagedBinary
$runtimeManifest = Get-Content -Raw -Encoding UTF8 (Join-Path $PSScriptRoot '..\third_party\winappsdk\runtime-manifest.json') | ConvertFrom-Json
foreach ($entry in $runtimeManifest.files) {
  $destination = Join-Path $stage $entry.path
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
  Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $sourceBinary) $entry.path) -Destination $destination
}
$process = Start-Process -FilePath $stagedBinary -PassThru `
  -ArgumentList @('modern_menu::tests::live_menu_selection_submenu_and_cancellation', '--exact', '--ignored', '--nocapture', '--test-threads=1') `
  -RedirectStandardOutput (Join-Path $outputPath 'stdout.txt') `
  -RedirectStandardError (Join-Path $outputPath 'stderr.txt')
$null = $process.Handle # Keep a handle so Windows PowerShell retains ExitCode after exit.
$condition = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$process.Id)

function Find-MenuElement([string]$Name) {
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  do {
    if ($process.HasExited) { throw "Menu test exited early: $($process.ExitCode)" }
    try {
      $elements = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants, $condition)
      foreach ($element in $elements) {
        if ($element.Current.Name -eq $Name -and -not $element.Current.IsOffscreen) { return $element }
      }
    } catch [System.Management.Automation.MethodInvocationException] {
      if ($_.Exception.InnerException -isnot [System.Windows.Automation.ElementNotAvailableException]) { throw }
    }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "Menu element did not appear: $Name"
}

function Send-OwnedKey([byte]$Key) {
  [uint32]$foregroundProcess = 0
  [void][MenuTestInput]::GetWindowThreadProcessId([MenuTestInput]::GetForegroundWindow(), [ref]$foregroundProcess)
  if ($foregroundProcess -ne $process.Id) { throw 'Test menu does not own foreground input' }
  [MenuTestInput]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
  [MenuTestInput]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
}

function Save-MenuImage($Item, [string]$Name) {
  Start-Sleep -Milliseconds 250
  $menu = [System.Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($Item)
  $bounds = $menu.Current.BoundingRectangle
  if ($bounds.Width -lt 40 -or $bounds.Width -gt 1200 -or $bounds.Height -lt 40 -or $bounds.Height -gt 1200) {
    throw "Unexpected isolated menu bounds: $bounds"
  }
  $bitmap = New-Object System.Drawing.Bitmap([int]$bounds.Width, [int]$bounds.Height)
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  try {
    $graphics.CopyFromScreen([int]$bounds.X, [int]$bounds.Y, 0, 0, $bitmap.Size)
    $bitmap.Save((Join-Path $outputPath "$Name.png"))
  } finally { $graphics.Dispose(); $bitmap.Dispose() }
}

$passed = $false
try {
  $dark = Find-MenuElement 'Peco Dark toggle'
  $disabled = Find-MenuElement 'Peco disabled'
  if ($disabled.Current.IsEnabled) { throw 'Disabled command is enabled' }
  $toggle = $dark.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
  if ($toggle.Current.ToggleState -ne [System.Windows.Automation.ToggleState]::On) { throw 'Toggle lost its checked state' }
  Save-MenuImage $dark 'dark'
  $dark.SetFocus()
  Send-OwnedKey 13

  $light = Find-MenuElement 'Peco Light toggle'
  Save-MenuImage $light 'light'
  $submenu = Find-MenuElement 'Peco submenu'
  $expand = $submenu.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
  $expand.Expand()
  $nested = Find-MenuElement 'Peco nested command'
  $invoke = $nested.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
  $invoke.Invoke()

  $cancel = Find-MenuElement 'Peco Cancel toggle'
  $cancel.SetFocus()
  Send-OwnedKey 27
  if (-not $process.WaitForExit(15000)) { throw 'Menu test did not finish after cancellation' }
  if ($process.ExitCode -ne 0) { throw "Menu test failed: $($process.ExitCode)" }
  $passed = $true
  Write-Output "PASS: native WinUI menus, checked/disabled state, nested command, keyboard selection, Esc cancellation and repeated opening. Images: $outputPath"
} finally {
  if (-not $process.HasExited) { Stop-Process -Id $process.Id }
  if (-not $passed) { Get-Content (Join-Path $outputPath 'stderr.txt') -ErrorAction SilentlyContinue }
  Remove-Item -LiteralPath $stage -Recurse -Force
}
