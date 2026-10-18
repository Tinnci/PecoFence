param(
  [string]$Binary = (Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\debug\pecofence.exe'),
  [string]$ReportDirectory = (Join-Path $PSScriptRoot '..\.cache\accessibility-audit'),
  [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot)
)
$ErrorActionPreference = 'Stop'
$instance = 'accessibility-' + [guid]::NewGuid().ToString('N').Substring(0, 12)
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
  $env:RUST_LOG = 'info,pecofence_platform::accessibility=debug'
  foreach ($index in 0..19) {
    $fileName = if ($index -eq 0) { 'Fixture-00-中文.txt' } else { 'Fixture-{0:D2}.txt' -f $index }
    Set-Content -LiteralPath (Join-Path $files $fileName) -Value 'owned accessibility fixture'
  }
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
  Add-Type -AssemblyName UIAutomationClient
  Add-Type -AssemblyName UIAutomationTypes
  Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class FixtureWindows {
  public delegate bool Callback(IntPtr window, IntPtr data);
  [DllImport("user32.dll")] static extern bool EnumWindows(Callback callback, IntPtr data);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
  public static IntPtr[] ForProcess(uint process) {
    var result = new List<IntPtr>();
    EnumWindows((window, data) => { uint owner; GetWindowThreadProcessId(window, out owner); if (owner == process) result.Add(window); return true; }, IntPtr.Zero);
    return result.ToArray();
  }
}
'@
  Add-Type -ReferencedAssemblies @([System.Windows.Automation.AutomationElement].Assembly.Location, [System.Windows.Automation.AutomationEventArgs].Assembly.Location) -TypeDefinition @'
using System;
using System.Collections.Concurrent;
using System.Windows.Automation;
public static class FixtureInvokeEvents {
  static readonly ConcurrentQueue<string> events = new ConcurrentQueue<string>();
  static System.Threading.Thread worker;
  static System.Threading.ManualResetEvent stop;
  static Exception failure;
  static readonly AutomationEventHandler handler = (sender, args) => {
    try { events.Enqueue(String.Join(",", ((AutomationElement)sender).GetRuntimeId())); }
    catch (Exception error) { events.Enqueue("ERROR:" + error.GetType().FullName); }
  };
  public static void Start(AutomationElement element) {
    stop = new System.Threading.ManualResetEvent(false);
    var ready = new System.Threading.ManualResetEvent(false);
    worker = new System.Threading.Thread(() => {
      try {
        Automation.AddAutomationEventHandler(InvokePattern.InvokedEvent, element, TreeScope.Subtree, handler);
        ready.Set();
        stop.WaitOne();
        Automation.RemoveAutomationEventHandler(InvokePattern.InvokedEvent, element, handler);
      } catch (Exception error) { failure = error; ready.Set(); }
    });
    worker.IsBackground = true;
    worker.SetApartmentState(System.Threading.ApartmentState.MTA);
    worker.Start();
    if (!ready.WaitOne(5000)) throw new TimeoutException("UIA event subscription timed out");
    if (failure != null) throw new Exception("UIA event subscription failed", failure);
  }
  public static string[] Drain() {
    var result = new System.Collections.Generic.List<string>();
    string value;
    while (events.TryDequeue(out value)) result.Add(value);
    return result.ToArray();
  }
  public static void Stop() {
    if (worker == null) return;
    stop.Set();
    if (!worker.Join(5000)) throw new TimeoutException("UIA event unsubscription timed out");
    worker = null;
    if (failure != null) throw new Exception("UIA event worker failed", failure);
  }
}
'@
  $advance = Join-Path $stage 'advance'
  $restore = Join-Path $stage 'restore'
  $finish = Join-Path $stage 'finish'
  $workflow = @('sleep 700', 'bounds LifecycleA 300 200 700 400', 'new-todo 1050 450', 'sleep 200', 'todo-add UIA Remove', 'sleep 200', 'todo-add UIA Keep', 'sleep 200', 'todo-toggle 0', 'sleep 200', 'new-note 1050 100', 'sleep 200', 'note-set "UIA 中文便笺"', 'sleep 800', 'dump uia-ready', ('wait-file ' + $advance), 'roll LifecycleA', 'todo-clear', 'sleep 450', 'dump uia-rolled', ('wait-file ' + $restore), 'unroll LifecycleA', 'todo-undo', 'sleep 450', 'dump uia-restored', ('wait-file ' + $finish), 'exit')
  $scriptPath = Join-Path $stage 'workflow.txt'
  [IO.File]::WriteAllLines($scriptPath, $workflow, $utf8)
  $stdout = Join-Path $stage 'stdout.txt'
  $stderr = Join-Path $stage 'stderr.txt'
  $arguments = @('--portable', '--no-hide-icons', '--config-dir', ('"' + $configDir + '"'), '--test-script', ('"' + $scriptPath + '"'), '--exit-after', '45000')
  $process = Start-Process -FilePath (Join-Path $stage 'pecofence.exe') -ArgumentList $arguments -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
  $null = $process.Handle
  function Wait-Marker([string]$tag) {
    $deadline = [DateTime]::UtcNow.AddSeconds(12)
    do {
      Start-Sleep -Milliseconds 100
      if ($process.HasExited) { throw "Fixture exited before $tag" }
      $ready = (@((Get-Content -LiteralPath $stdout -Raw -Encoding UTF8), (Get-Content -LiteralPath $stderr -Raw -Encoding UTF8)) -join "`n") -match ('\[' + [regex]::Escape($tag) + '\]')
    } while (!$ready -and [DateTime]::UtcNow -lt $deadline)
    if (!$ready) { throw "Fixture did not publish $tag" }
  }
  Wait-Marker 'uia-ready'
  function Find-Named($parent, [string]$name) {
    $filter = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $name)
    return $parent.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $filter)
  }
  $root = $null
  $todoRoot = $null
  $noteText = $null
  $noteRoot = $null
  $keptTask = $null
  $windows = @()
  foreach ($handle in [FixtureWindows]::ForProcess($process.Id)) {
    $candidate = [System.Windows.Automation.AutomationElement]::FromHandle($handle)
    if (!$candidate) { continue }
    $windows += [ordered]@{handle=$handle.ToInt64(); name=$candidate.Current.Name; framework=$candidate.Current.FrameworkId}
    if ($candidate.Current.Name -eq 'LifecycleA') { $root = $candidate }
    if ($candidate.Current.FrameworkId -eq 'PecoFence') {
      $kept = Find-Named $candidate 'UIA Keep'
      if ($kept) { $todoRoot = $candidate; $keptTask = $kept }
      $note = Find-Named $candidate 'UIA 中文便笺'
      if ($note) { $noteText = $note; $noteRoot = $candidate }
    }
  }
  $windows | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'windows.json') -Encoding UTF8
  if (!$root) { throw 'UIA could not find the owned LifecycleA root' }
  if (!$todoRoot -or !$keptTask -or !$noteText) { throw 'Plugin task/note semantics were not exported through the external UIA client' }
  $noteIdentity = @($noteText.GetRuntimeId())
  $noteParent = [System.Windows.Automation.TreeWalker]::RawViewWalker.GetParent($noteText)
  if ($noteParent.Current.ControlType -ne [System.Windows.Automation.ControlType]::Button -or [string]::IsNullOrWhiteSpace($noteParent.Current.Name)) { throw 'Note body text has no named edit-button parent' }
  $noteHint = $null
  foreach ($node in $noteRoot.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {
    if ($node.Current.Name -like 'Ctrl+Enter*') { $noteHint = $node }
  }
  if (!$noteHint) { throw 'Note footer hint was not exported' }
  $hintName = $noteHint.Current.Name
  $hintIdentity = @($noteHint.GetRuntimeId())
  $taskIdentity = @($keptTask.GetRuntimeId())
  $taskParent = [System.Windows.Automation.TreeWalker]::RawViewWalker.GetParent($keptTask)
  if ($taskParent.Current.ControlType -ne [System.Windows.Automation.ControlType]::Group) { throw 'Task title has no semantic row parent' }
  if ($noteText.Current.BoundingRectangle.Width -le 0 -or $keptTask.Current.BoundingRectangle.Width -le 0) { throw 'Plugin semantic geometry is empty' }
  [ordered]@{taskName=$keptTask.Current.Name; taskId=$taskIdentity; taskParentId=@($taskParent.GetRuntimeId()); noteName=$noteText.Current.Name} | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'plugins.json') -Encoding UTF8
  $descendants = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  $tree = @(foreach ($element in $descendants) {
    $current = $element.Current
    $rect = $current.BoundingRectangle
    [ordered]@{name=$current.Name; type=$current.ControlType.ProgrammaticName; framework=$current.FrameworkId; id=$current.AutomationId; runtimeId=@($element.GetRuntimeId()); x=$rect.X; y=$rect.Y; width=$rect.Width; height=$rect.Height; offscreen=$current.IsOffscreen}
  })
  $tree | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'tree.json') -Encoding UTF8
  $filesFound = @($tree | Where-Object {$_.type -eq 'ControlType.ListItem' -and $_.name -like 'Fixture-*'})
  if ($filesFound.Count -lt 1) { throw 'No fixture files were exported through the external UIA client' }
  if (!($filesFound | Where-Object {$_.name -eq 'Fixture-00-中文.txt'})) { throw 'Unicode filename did not survive the external UIA path' }
  $rootBounds = $root.Current.BoundingRectangle
  foreach ($file in $filesFound) {
    if ($file.width -le 0 -or $file.height -le 0 -or $file.offscreen) { throw 'Visible file has invalid UIA geometry' }
    if ($file.x -lt $rootBounds.X -or $file.y -lt $rootBounds.Y -or ($file.x + $file.width) -gt $rootBounds.Right -or ($file.y + $file.height) -gt $rootBounds.Bottom) { throw 'File geometry escaped the window viewport' }
    if ($file.framework -ne 'PecoFence' -or $file.runtimeId.Count -lt 2) { throw 'File identity did not come from the custom fragment provider' }
  }
  # Resolve the same element again through the external client to check stable identity.
  $name = $filesFound[0].name
  $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $name)
  $again = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
  if (($again.GetRuntimeId() -join ',') -cne ($filesFound[0].runtimeId -join ',')) { throw 'Runtime identity changed without a scene identity change' }
  $walker = [System.Windows.Automation.TreeWalker]::RawViewWalker
  $parent = $walker.GetParent($again)
  if ($parent.Current.ControlType -ne [System.Windows.Automation.ControlType]::List) { throw 'File is not parented by its content list' }
  $parentRoot = $walker.GetParent($parent)
  if (($parentRoot.GetRuntimeId() -join ',') -cne ($root.GetRuntimeId() -join ',')) { throw 'Fragment navigation changed root identity' }
  $undoButton = Find-Named $noteRoot '撤销上一步'
  if (!$undoButton) { $undoButton = Find-Named $noteRoot 'Undo last' }
  if (!$undoButton -or !$undoButton.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::IsInvokePatternAvailableProperty)) { throw 'Note Undo does not advertise Invoke' }
  $expectedInvokeSource = $undoButton.GetRuntimeId() -join ','
  [FixtureInvokeEvents]::Start($noteRoot)
  $undoPattern = $undoButton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
  $undoPattern.Invoke()
  $invokeDeadline = [DateTime]::UtcNow.AddSeconds(5)
  do {
    Start-Sleep -Milliseconds 50
    $updatedText = $noteText.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty)
  } while ($updatedText -ceq 'UIA 中文便笺' -and [DateTime]::UtcNow -lt $invokeDeadline)
  if ($updatedText -ceq 'UIA 中文便笺') { throw 'External UIA Invoke did not execute the note Undo action' }
  $staleInvokeRejected = $false
  try { $undoPattern.Invoke() } catch {
    $cause = $_.Exception
    while ($cause.InnerException) { $cause = $cause.InnerException }
    if ($cause -is [System.Windows.Automation.ElementNotAvailableException]) { $staleInvokeRejected = $true } else { throw }
  }
  if (!$staleInvokeRejected) { throw 'A removed Undo button still accepts invocation' }
  $invokedEvents = @()
  $eventDeadline = [DateTime]::UtcNow.AddSeconds(5)
  do {
    Start-Sleep -Milliseconds 50
    $invokedEvents += [FixtureInvokeEvents]::Drain()
  } while ($invokedEvents.Count -eq 0 -and [DateTime]::UtcNow -lt $eventDeadline)
  Start-Sleep -Milliseconds 150
  $invokedEvents += [FixtureInvokeEvents]::Drain()
  $invokedEvents | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'invoke-events.json') -Encoding UTF8
  if ($invokedEvents.Count -ne 1 -or $invokedEvents[0] -cne $expectedInvokeSource) { throw 'Expected exactly one Invoked event from the activated Undo button' }
  [FixtureInvokeEvents]::Stop()

  [IO.File]::WriteAllText($advance, 'continue', $utf8)
  Wait-Marker 'uia-rolled'
  if (($noteText.GetRuntimeId() -join ',') -cne ($noteIdentity -join ',') -or $noteText.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty) -ceq 'UIA 中文便笺') { throw 'Note body reference did not survive text undo correctly' }
  $hintAfterUndo = Find-Named $noteRoot $hintName
  if (!$hintAfterUndo -or ($hintAfterUndo.GetRuntimeId() -join ',') -cne ($hintIdentity -join ',') -or $noteHint.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty) -cne $hintName) { throw 'Note footer identity changed when Undo disappeared' }
  $afterClear = Find-Named $todoRoot 'UIA Keep'
  if (!$afterClear -or ($afterClear.GetRuntimeId() -join ',') -cne ($taskIdentity -join ',')) { throw 'Surviving task identity changed when an earlier task was cleared' }
  if (Find-Named $todoRoot 'UIA Remove') { throw 'Cleared task remains in the exported UIA tree' }
  if ($keptTask.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty) -cne 'UIA Keep') { throw 'Held task reference was reassigned to another task' }
  $rolledFile = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
  if ($rolledFile) { throw 'Collapsed body still exports a file node' }
  $expired = $false
  $expiryMode = 'unknown'
  try {
    $heldName = $again.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty, $true)
    $heldBounds = $again.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::BoundingRectangleProperty, $true)
    [ordered]@{name=[string]$heldName; bounds=[string]$heldBounds; nameUnsupported=($heldName -eq [System.Windows.Automation.AutomationElement]::NotSupported); boundsUnsupported=($heldBounds -eq [System.Windows.Automation.AutomationElement]::NotSupported)} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'held-after-collapse.json') -Encoding UTF8
    $expiryMode = 'unsupported properties'
    $expired = ($heldName -eq [System.Windows.Automation.AutomationElement]::NotSupported) -and ($heldBounds -eq [System.Windows.Automation.AutomationElement]::NotSupported)
  } catch {
    $errorObject = $_.Exception
    while ($errorObject.InnerException) { $errorObject = $errorObject.InnerException }
    if ($errorObject -is [System.Windows.Automation.ElementNotAvailableException]) { $expired = $true; $expiryMode = 'ElementNotAvailableException' } else { throw }
  }
  [ordered]@{expired=$expired; mode=$expiryMode} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'held-reference-result.json') -Encoding UTF8
  if (!$expired) { throw 'Held file reference remained available after collapse' }
  [IO.File]::WriteAllText($restore, 'continue', $utf8)
  Wait-Marker 'uia-restored'
  $afterUndo = Find-Named $todoRoot 'UIA Keep'
  if (!$afterUndo -or ($afterUndo.GetRuntimeId() -join ',') -cne ($taskIdentity -join ',') -or !(Find-Named $todoRoot 'UIA Remove')) { throw 'Undo did not restore task identities and content' }
  $restored = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
  if (!$restored -or ($restored.GetRuntimeId() -join ',') -cne ($filesFound[0].runtimeId -join ',')) { throw 'Expanded file did not restore its stable identity' }
  if ($restored.Current.BoundingRectangle.Width -le 0) { throw 'Restored file has empty geometry' }
  [IO.File]::WriteAllText($finish, 'continue', $utf8)
  if (!$process.WaitForExit(10000)) { throw 'UIA fixture timed out' }
  $logs = @((Get-Content -LiteralPath $stdout -Raw -Encoding UTF8), (Get-Content -LiteralPath $stderr -Raw -Encoding UTF8)) -join "`n"
  $logs | Set-Content -LiteralPath (Join-Path $ReportDirectory 'native.log') -Encoding UTF8
  if ($process.ExitCode -ne 0 -or $logs -cmatch '\bERROR\b|PANIC|panicked at') { throw 'UIA fixture failed or logged an error' }
  [ordered]@{instance=$instance; passed=$true; exitCode=$process.ExitCode; files=$filesFound.Count; heldReference=$expiryMode; externalInvoke=$true; invokedEvents=$invokedEvents.Count; staleInvokeRejected=$staleInvokeRejected; scope='external Windows UIA tree and plugin button Invoke, file geometry, parent/root identity, collapse invalidation, expansion recovery, plugin text, note undo/footer identity and task clear/undo identity; no focus/toggle/selection/property/structure events or DPI matrix'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Encoding UTF8
  Get-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Raw -Encoding UTF8
} catch {
  [ordered]@{instance=$instance; passed=$false; error=$_.Exception.Message; scope='external Windows UIA client'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.json') -Encoding UTF8
  throw
} finally {
  if ('FixtureInvokeEvents' -as [type]) { [FixtureInvokeEvents]::Stop() }
  foreach ($stream in @('stdout.txt', 'stderr.txt')) {
    $streamPath = Join-Path $stage $stream
    if (Test-Path -LiteralPath $streamPath) { Copy-Item -LiteralPath $streamPath -Destination (Join-Path $ReportDirectory $stream) -Force }
  }
  if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id -Force; $process.WaitForExit() }
  foreach ($key in $previous.Keys) { [Environment]::SetEnvironmentVariable($key, $previous[$key], 'Process') }
  if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
}
