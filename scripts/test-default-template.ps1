param(
  [Parameter(Mandatory=$true)][string]$Binary,
  [Parameter(Mandatory=$true)][string]$RepositoryRoot,
  [Parameter(Mandatory=$true)][string]$ReportDirectory
)
# Owns an isolated instance only. Invokes actual WinUI buttons and reads saved fixture data.
$ErrorActionPreference='Stop'
$source=(Resolve-Path -LiteralPath $Binary).ProviderPath
$stage=Join-Path $env:TEMP ('pecofence-defaults-'+[guid]::NewGuid().ToString('N'))
$configDir=Join-Path $stage 'config'
$previous=@{}
foreach($name in @('PECOFENCE_INSTANCE','PECOFENCE_UI_TEST_WINDOWS','LOCALAPPDATA','APPDATA','RUST_LOG')) {$previous[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
$process=$null
$runKey='HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$startup=(Get-ItemProperty $runKey -Name PecoFence -ErrorAction SilentlyContinue).PecoFence
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class DefaultTemplateWindow {
  delegate bool Callback(IntPtr hwnd,IntPtr value);
  [DllImport("user32.dll")] static extern bool EnumWindows(Callback callback,IntPtr value);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd,StringBuilder name,int size);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd,uint message,IntPtr w,IntPtr l);
  public static IntPtr Find(int pid,bool settings) {
    IntPtr found=IntPtr.Zero;
    EnumWindows((hwnd,value)=>{uint owner;GetWindowThreadProcessId(hwnd,out owner);var name=new StringBuilder(128);GetClassName(hwnd,name,128);
      if(owner==pid && (settings ? IsWindowVisible(hwnd)&&name.ToString().Contains("WinUI") : name.ToString()=="PecoFence.Control")){found=hwnd;return false;}return true;},IntPtr.Zero);
    return found;
  }
}
'@
function Elements {$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)}
function Invoke-Button([string]$Name) {
  $element=Elements | Where-Object {$_.Current.Name -eq $Name -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button} | Select-Object -First 1
  if(!$element){throw "Missing native button: $Name"}
  $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
function Read-Workspace {Get-Content (Join-Path $configDir 'workspace.v2.json') -Raw -Encoding UTF8 | ConvertFrom-Json}
function Active-Layout($workspace) { @($workspace.layouts | Where-Object {$_.fingerprint.Count -gt 0})[-1] }
function Wait-Snapshots([int]$Count) {
  for($i=0;$i -lt 60;$i++) {
    Start-Sleep -Milliseconds 100
    $saved=Read-Workspace
    if(@($saved.snapshots).Count -eq $Count){return $saved}
  }
  throw "Saved snapshot count did not reach $Count"
}
function Confirm([string]$Label,[bool]$Accept) {
  Invoke-Button $Label
  Start-Sleep -Milliseconds 350
  if($Accept){Invoke-Button 'Confirm and continue'}else{Invoke-Button 'Cancel'}
  Start-Sleep -Milliseconds 400
}
try {
  New-Item -ItemType Directory -Force -Path $stage,$configDir,$ReportDirectory | Out-Null
  $env:PECOFENCE_INSTANCE=Split-Path $stage -Leaf
  $env:PECOFENCE_UI_TEST_WINDOWS='1'
  $env:LOCALAPPDATA=Join-Path $stage 'local'
  $env:APPDATA=Join-Path $stage 'roaming'
  $env:RUST_LOG='info'
  $utf8=New-Object Text.UTF8Encoding($false)
  $fixture=Get-Content (Join-Path $RepositoryRoot 'scripts\fixtures\input-lifecycle.json') -Raw -Encoding UTF8 | ConvertFrom-Json
  $fixture.settings.language='en'
  $fixture.settings.theme='dark'
  $fixture.settings | Add-Member -NotePropertyName widgetPaperStyle -NotePropertyValue 'dark' -Force
  $fixture.settings | Add-Member -NotePropertyName widgetPaperBrightness -NotePropertyValue 0.8 -Force
  $view=$fixture.layouts[0].contents[0].view
  $prototype=$fixture.layouts[0].containers[0]
  $contents=@();$containers=@();$noteId=$null
  foreach($title in @('Programs','Folders','Files and documents','Desktop','Clock','Note')) {
    $id=[guid]::NewGuid().ToString()
    if($title -in @('Clock','Note')) {
      $provider=if($title -eq 'Clock'){'pecofence.clock'}else{'pecofence.note'}
      $data=if($title -eq 'Note'){@{text='Preserve my note'}}else{@{}}
      $content=@{kind='panel';panel=@{provider=$provider;instanceId=[guid]::NewGuid().ToString();configVersion=1;config=$data}}
      if($title -eq 'Note'){$noteId=$id}
    }else{$content=@{kind='fileCollection';inbox=($title -eq 'Desktop');items=@()}}
    $contents+=@{id=$id;title=$title;view=$view;content=$content}
    $window=$prototype | ConvertTo-Json -Depth 40 | ConvertFrom-Json
    $window.id=[guid]::NewGuid().ToString();$window.tabs=@($id);$window.activeTab=$id
    $window.geometry.x=if($title -in @('Clock','Note')){48}else{1600}
    $window.geometry.y=if($title -eq 'Clock'){32}elseif($title -eq 'Note'){270}else{16+190*$containers.Count}
    $window.geometry.w=if($title -eq 'Clock'){320}elseif($title -eq 'Note'){400}else{240}
    $window.geometry.h=if($title -eq 'Clock'){192}elseif($title -eq 'Note'){384}else{174}
    $window.expandedH=$window.geometry.h
    $window.appearance=@{opacity=0.65;tintRgb=@(13,71,222)}
    $containers+=$window
  }
  $fixture.layouts=@(@{fingerprint=@();containers=$containers;contents=$contents})
  $fixture.snapshots=@()
  [IO.File]::WriteAllText((Join-Path $configDir 'workspace.v2.json'),($fixture|ConvertTo-Json -Depth 50),$utf8)
  Copy-Item $source (Join-Path $stage 'pecofence.exe')
  $runtime=Get-Content (Join-Path $RepositoryRoot 'third_party\winappsdk\runtime-manifest.json') -Raw | ConvertFrom-Json
  foreach($entry in $runtime.files) {
    $destination=Join-Path $stage $entry.path
    New-Item -ItemType Directory -Force -Path (Split-Path $destination) | Out-Null
    Copy-Item -LiteralPath (Join-Path (Split-Path $source) $entry.path) -Destination $destination
  }
  $fonts=Join-Path (Split-Path $source) 'fonts'
  if(Test-Path $fonts){Copy-Item $fonts $stage -Recurse}
  $process=Start-Process (Join-Path $stage 'pecofence.exe') -PassThru -ArgumentList @('--portable','--no-hide-icons','--config-dir',('"'+$configDir+'"'),'--open-settings','--exit-after','90000')
  $null=$process.Handle
  $settings=[IntPtr]::Zero
  for($i=0;$i -lt 80;$i++){Start-Sleep -Milliseconds 100;$settings=[DefaultTemplateWindow]::Find($process.Id,$true);if($settings -ne [IntPtr]::Zero){break}}
  if($settings -eq [IntPtr]::Zero){throw 'Isolated settings did not appear'}
  Start-Sleep -Seconds 2
  $root=[System.Windows.Automation.AutomationElement]::FromHandle($settings)
  $navigation=@(Elements | Where-Object {$_.Current.ClassName -like '*NavigationViewItem*'})
  $navigation[4].GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
  Start-Sleep -Milliseconds 500
  $before=Read-Workspace
  $noteBefore=(Active-Layout $before).contents | Where-Object {$_.id -eq $noteId}
  Confirm 'Restore default arrangement' $false
  if(@((Read-Workspace).snapshots).Count -ne 0){throw 'Cancelled operation changed history'}
  Confirm 'Add missing default widgets' $true
  $added=Wait-Snapshots 1
  if(@((Active-Layout $added).contents).Count -ne 8){throw 'Missing widgets were not added'}
  Confirm 'Add missing default widgets' $true
  Start-Sleep -Milliseconds 500
  if(@((Read-Workspace).snapshots).Count -ne 1){throw 'Repeated supplement created a duplicate change'}
  Confirm 'Restore default arrangement' $true
  $arranged=Wait-Snapshots 2
  $noteAfter=(Active-Layout $arranged).contents | Where-Object {$_.id -eq $noteId}
  if(($noteBefore.content | ConvertTo-Json -Depth 30 -Compress) -cne ($noteAfter.content | ConvertTo-Json -Depth 30 -Compress)){throw 'Note identity or business data changed'}
  Confirm 'Restore default appearance' $true
  $appearance=Wait-Snapshots 3
  if($appearance.settings.widgetPaperStyle -ne 'adaptive' -or $appearance.settings.widgetPaperBrightness -ne 1.0){throw 'Default paper appearance was not restored'}
  if($appearance.settings.autostart -ne $false){throw 'Appearance changed startup preference'}
  if(@((Active-Layout $appearance).containers | Where-Object {$_.appearance}).Count -ne 0){throw 'Window appearance overrides remained'}
  Invoke-Button 'Restore: Before restoring the default appearance'
  Start-Sleep -Milliseconds 350
  Invoke-Button 'Restore'
  $undone=Wait-Snapshots 4
  if($undone.settings.widgetPaperStyle -ne 'dark' -or $undone.settings.widgetPaperBrightness -ne 0.8){throw 'Appearance undo did not restore the previous palette'}
  $undoNote=(Active-Layout $undone).contents | Where-Object {$_.id -eq $noteId}
  if(($noteBefore.content | ConvertTo-Json -Depth 30 -Compress) -cne ($undoNote.content | ConvertTo-Json -Depth 30 -Compress)){throw 'Snapshot undo changed widget data'}
  $control=[DefaultTemplateWindow]::Find($process.Id,$false)
  [void][DefaultTemplateWindow]::PostMessage($settings,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
  [void][DefaultTemplateWindow]::PostMessage($control,0x800E,[IntPtr]::Zero,[IntPtr]::Zero)
  if(!$process.WaitForExit(15000) -or $process.ExitCode -ne 0){throw 'Isolated application did not close cleanly'}
  if((Get-ItemProperty $runKey -Name PecoFence -ErrorAction SilentlyContinue).PecoFence -ne $startup){throw 'User startup entry changed'}
  [ordered]@{passed=$true;addedWidgets=2;roles=8;snapshots=4;appearanceUndoPassed=$true;duplicateSupplementRejected=$true;noteIdentityAndDataPreserved=$true;cancelPreservedState=$true;appearanceRestored=$true;startupEntryUnchanged=$true} | ConvertTo-Json | Set-Content (Join-Path $ReportDirectory 'report.json') -Encoding UTF8
  Get-Content (Join-Path $ReportDirectory 'report.json') -Raw
} finally {
  if($process -and !$process.HasExited){Stop-Process -Id $process.Id -Force}
  $log=Join-Path $stage 'local\PecoFence\pecofence.log'
  if(Test-Path $log){Copy-Item $log (Join-Path $ReportDirectory 'native.log') -Force}
  foreach($name in $previous.Keys){[Environment]::SetEnvironmentVariable($name,$previous[$name],'Process')}
  if(Test-Path $stage){Remove-Item $stage -Recurse -Force}
}
