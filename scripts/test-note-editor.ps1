param(
  [string]$Binary = (Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-msvc\debug\pecofence.exe')
)

$ErrorActionPreference = 'Stop'
$binaryPath = (Resolve-Path $Binary).ProviderPath
$instance = 'note-editor-smoke-' + [guid]::NewGuid().ToString('N').Substring(0, 12)
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

Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class PecoFenceNoteEditorTest {
    private delegate bool EnumProc(IntPtr h, IntPtr param);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback, IntPtr param);
    [DllImport("user32.dll")] private static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr param);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr h, StringBuilder text, int capacity);
    [DllImport("user32.dll", EntryPoint="SendMessageW", CharSet=CharSet.Unicode)] public static extern IntPtr SetText(IntPtr h, uint msg, IntPtr w, string text);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    private static string Class(IntPtr h) { var s=new StringBuilder(128); GetClassName(h,s,128); return s.ToString(); }
    public static IntPtr Find(int pid, string cls) {
        IntPtr found=IntPtr.Zero;
        EnumWindows((h,p)=>{uint owner; GetWindowThreadProcessId(h,out owner);
            if(owner==(uint)pid && Class(h).Equals(cls,StringComparison.OrdinalIgnoreCase)){found=h;return false;}return true;},IntPtr.Zero);
        return found;
    }
    public static IntPtr Edit(IntPtr parent) {
        IntPtr found=IntPtr.Zero;
        EnumChildWindows(parent,(h,p)=>{if(Class(h).Equals("Edit",StringComparison.OrdinalIgnoreCase)){found=h;return false;}return true;},IntPtr.Zero);
        return found;
    }
}
'@
# Leave the automatically opened editor active. Quit must submit its current
# native text through the provider, including its second-pass PersistConfig.
Set-Content $scriptPath -Encoding UTF8 -Value @('sleep 300','new-note 400 250','sleep 12000','exit')
$passed=$false
try {
    $process=Start-Process $stagedBinary -ArgumentList @('--portable','--config-dir',$configDir,'--no-hide-icons','--test-script',$scriptPath,'--exit-after','16000') -PassThru
    $editor=[IntPtr]::Zero
    for($i=0;$i -lt 60;$i++) {
        Start-Sleep -Milliseconds 200
        $process.Refresh()
        if($process.HasExited){throw 'Fixture exited before the note editor opened'}
        $popup=[PecoFenceNoteEditorTest]::Find($process.Id,'PecoFence.Rename')
        if($popup -ne [IntPtr]::Zero){$editor=[PecoFenceNoteEditorTest]::Edit($popup)}
        if($editor -ne [IntPtr]::Zero){break}
    }
    if($editor -eq [IntPtr]::Zero){throw 'No owned native editor appeared'}
    $text="First line`r`n项目记录`r`n  keep this indentation"
    if([PecoFenceNoteEditorTest]::SetText($editor,0x000C,[IntPtr]::Zero,$text) -eq [IntPtr]::Zero){throw 'Native text input failed'}
    $control=[PecoFenceNoteEditorTest]::Find($process.Id,'PecoFence.Control')
    if(![PecoFenceNoteEditorTest]::PostMessage($control,0x800E,[IntPtr]::Zero,[IntPtr]::Zero)){throw 'Quit delivery failed'}
    if(!$process.WaitForExit(10000)){throw 'Quit did not finish after submitting native text'}
    if($process.ExitCode -ne 0){throw "Unexpected exit code: $($process.ExitCode)"}
    $workspace=Join-Path $configDir 'workspace.v2.json'
    $saved=Get-Content $workspace -Raw -Encoding UTF8 | ConvertFrom-Json
    $notes=@($saved.layouts.contents | Where-Object {$_.content.panel.provider -eq 'pecofence.note'})
    if($notes.Count -ne 1 -or $notes[0].content.panel.config.text -cne $text.Replace("`r`n","`n")){throw 'Quit lost text from the open native editor'}
    $validation=Start-Process $stagedBinary -ArgumentList @('--validate-workspace',$workspace) -PassThru -Wait
    if($validation.ExitCode -ne 0){throw 'Saved workspace failed native validation'}
    Write-Output 'PASS: Quit commits open native multiline text, Chinese characters and indentation before the final workspace save'
    $passed=$true
} finally {
    $env:PECOFENCE_INSTANCE=$previousInstance
    $env:LOCALAPPDATA=$previousLocalAppData
    $env:APPDATA=$previousAppData
    $env:RUST_LOG=$previousRustLog
    if($passed){Remove-Item $stage -Recurse -Force}
    else {Write-Warning "Test artifacts retained: $stage"}
}
