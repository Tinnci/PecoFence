# Stage and switch one portable PecoFence + spmd installation.
# Both ZIPs must have the .zip.sha256 sidecar produced by the package scripts.
# First upgrades from binaries without --stop / WM_APP_UPDATE_QUIT require a manual exit.
param(
  [Parameter(Mandatory = $true)][string]$InstallRoot,
  [Parameter(Mandatory = $true)][string]$PecoFenceZip,
  [Parameter(Mandatory = $true)][string]$SpmZip,
  [string]$Instance = "",
  [string]$Fixture = "",
  [string]$LiveConfig = "",
  [string]$DbPath = "",
  [switch]$NoHideIcons
)
$ErrorActionPreference = "Stop"
if ($Instance -and $Instance -notmatch '^[A-Za-z0-9_-]{1,64}$') {
  throw "Instance must use 1-64 ASCII letters, digits, underscores or hyphens"
}
if ($Fixture -and ($LiveConfig -or $DbPath)) { throw "Fixture mode cannot use LiveConfig or DbPath" }
if (-not $Fixture -and -not $DbPath) { throw "Persistent/live mode requires an explicit absolute DbPath" }
if ($LiveConfig -and -not (Test-Path -LiteralPath $LiveConfig -PathType Leaf)) { throw "LiveConfig missing" }
if ($Fixture -and -not (Test-Path -LiteralPath $Fixture -PathType Leaf)) { throw "Fixture missing" }

$rootPath = [IO.Path]::GetFullPath($InstallRoot)
if ($rootPath.Length -gt 3) { $rootPath = $rootPath.TrimEnd('\') }
$pfZipPath = (Resolve-Path -LiteralPath $PecoFenceZip).ProviderPath
$spmZipPath = (Resolve-Path -LiteralPath $SpmZip).ProviderPath
if (-not (Test-Path -LiteralPath $rootPath -PathType Container)) { throw "InstallRoot missing: $rootPath" }
$fixturePath = if ($Fixture) { (Resolve-Path -LiteralPath $Fixture).ProviderPath } else { "" }
$liveConfigPath = if ($LiveConfig) { (Resolve-Path -LiteralPath $LiveConfig).ProviderPath } else { "" }
$databasePath = if ($DbPath) { [IO.Path]::GetFullPath($DbPath) } else { "" }
if ($databasePath -and -not [IO.Path]::IsPathRooted($DbPath)) { throw "DbPath must be absolute" }
$configPath = Join-Path $rootPath "config"
$logPath = Join-Path $rootPath "update-logs"
$versionRoot = Join-Path $rootPath "versions"
New-Item -ItemType Directory -Path $logPath, $versionRoot -Force | Out-Null
$lockFile = [IO.File]::Open((Join-Path $rootPath "update.lock"), [IO.FileMode]::OpenOrCreate,
  [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)

if (-not ("PecoFenceUpdateWindow" -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class PecoFenceUpdateWindow {
    private delegate bool EnumCallback(IntPtr hwnd, IntPtr param);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumCallback callback, IntPtr param);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetClassName(IntPtr hwnd, StringBuilder text, int size);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
    public static IntPtr Find(int processId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows(delegate(IntPtr hwnd, IntPtr param) {
            uint owner;
            GetWindowThreadProcessId(hwnd, out owner);
            if (owner == (uint)processId) {
                StringBuilder name = new StringBuilder(128);
                GetClassName(hwnd, name, name.Capacity);
                if (name.ToString() == "PecoFence.Control") { found = hwnd; return false; }
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static bool RequestQuit(int processId) {
        IntPtr hwnd = Find(processId);
        return hwnd != IntPtr.Zero && PostMessage(hwnd, 0x800E, IntPtr.Zero, IntPtr.Zero);
    }
}
'@
}

function Assert-Package([string]$archivePath) {
  $sidecar = "$archivePath.sha256"
  if (-not (Test-Path -LiteralPath $sidecar -PathType Leaf)) { throw "checksum missing: $sidecar" }
  $expected = (Get-Content -LiteralPath $sidecar -Raw).Trim().Split(' ')[0]
  if ($expected -notmatch '^[0-9a-fA-F]{64}$') { throw "invalid checksum: $sidecar" }
  $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
  if ($actual -ne $expected) { throw "SHA256 mismatch: $archivePath" }
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $archive = [IO.Compression.ZipFile]::OpenRead($archivePath)
  try {
    foreach ($entry in $archive.Entries) {
      $name = $entry.FullName.Replace('\', '/')
      if ($name.StartsWith('/') -or $name.Contains(':') -or
          @($name.Split('/') | Where-Object { $_ -eq '..' }).Count -gt 0) {
        throw "unsafe ZIP entry: $name"
      }
    }
  } finally { $archive.Dispose() }
}

function Start-Daemon([string]$binary, [string]$label) {
  $arguments = @("--ipc-version", "v2")
  if ($Instance) { $arguments += @("--instance", $Instance) }
  if ($fixturePath) { $arguments += @("--fixture", ('"' + $fixturePath + '"')) }
  else {
    $arguments += @("--db", ('"' + $databasePath + '"'))
    if ($liveConfigPath) { $arguments += @("--config", ('"' + $liveConfigPath + '"')) }
  }
  Start-Process -FilePath $binary -ArgumentList $arguments -WorkingDirectory $rootPath `
    -RedirectStandardOutput (Join-Path $logPath "$label.spmd.out.log") `
    -RedirectStandardError (Join-Path $logPath "$label.spmd.err.log") -PassThru
}

function Start-Fence([string]$binary, [string]$label, [bool]$sharedConfig) {
  $arguments = @("--portable")
  if ($sharedConfig) { $arguments += @("--config-dir", ('"' + $configPath + '"')) }
  if ($NoHideIcons) { $arguments += "--no-hide-icons" }
  $previous = $env:PECOFENCE_INSTANCE
  try {
    if ($Instance) { $env:PECOFENCE_INSTANCE = $Instance }
    else { Remove-Item Env:PECOFENCE_INSTANCE -ErrorAction SilentlyContinue }
    Start-Process -FilePath $binary -ArgumentList $arguments -WorkingDirectory $rootPath `
      -RedirectStandardOutput (Join-Path $logPath "$label.pecofence.out.log") `
      -RedirectStandardError (Join-Path $logPath "$label.pecofence.err.log") -PassThru
  } finally { $env:PECOFENCE_INSTANCE = $previous }
}

function Wait-DaemonReady([System.Diagnostics.Process]$process, [string]$binary) {
  $probeArgs = @("--probe")
  if ($Instance) { $probeArgs += @("--instance", $Instance) }
  for ($attempt = 0; $attempt -lt 20; $attempt++) {
    $process.Refresh()
    if ($process.HasExited) { throw "spmd exited before readiness: $binary" }
    $reply = & $binary @probeArgs 2>&1
    $process.Refresh()
    if ($LASTEXITCODE -eq 0 -and $reply -and -not $process.HasExited) { return }
    Start-Sleep -Milliseconds 500
  }
  throw "spmd did not complete a v2 Hello handshake: $binary"
}

function Wait-FenceReady([System.Diagnostics.Process]$process) {
  for ($attempt = 0; $attempt -lt 40; $attempt++) {
    $process.Refresh()
    if ($process.HasExited) { throw "PecoFence exited before its control window was ready" }
    if ([PecoFenceUpdateWindow]::Find($process.Id) -ne [IntPtr]::Zero) {
      Start-Sleep -Milliseconds 500
      $process.Refresh()
      if (-not $process.HasExited) { return }
    }
    Start-Sleep -Milliseconds 250
  }
  throw "PecoFence control window did not become ready"
}

function Assert-PairCompatible([string]$binary) {
  $previous = $env:PECOFENCE_INSTANCE
  try {
    if ($Instance) { $env:PECOFENCE_INSTANCE = $Instance }
    else { Remove-Item Env:PECOFENCE_INSTANCE -ErrorAction SilentlyContinue }
    $probe = Start-Process -FilePath $binary -ArgumentList @("--probe-spm") `
      -WorkingDirectory $rootPath -RedirectStandardError (Join-Path $logPath "candidate.pair-probe.err.log") `
      -Wait -PassThru
    if ($probe.ExitCode -ne 0) {
      throw "candidate PecoFence cannot complete an SPM v2 handshake; see candidate.pair-probe.err.log"
    }
  } finally { $env:PECOFENCE_INSTANCE = $previous }
}

function Get-ExactProcess([string]$binary) {
  $name = [IO.Path]::GetFileNameWithoutExtension($binary)
  $found = @(Get-Process -Name $name -ErrorAction SilentlyContinue | Where-Object {
    $_.Path -and $_.Path.Equals($binary, [StringComparison]::OrdinalIgnoreCase)
  })
  if ($found.Count -gt 1) { throw "multiple processes run from $binary" }
  if ($found.Count -eq 1) { return $found[0] }
  return $null
}

$candidate = $null
$newSpmd = $null
$newFence = $null
$oldSpmdStopped = $false
$oldFenceStopped = $false
$startupChanged = $false
$backup = @()
$databaseBackedUp = $false
try {
  $activePath = Join-Path $rootPath "active.json"
  if (Test-Path -LiteralPath $activePath) {
    $active = Get-Content -LiteralPath $activePath -Raw | ConvertFrom-Json
    $oldSpmdExe = [IO.Path]::GetFullPath($active.spmd)
    $oldFenceExe = [IO.Path]::GetFullPath($active.pecofence)
    if ($active.instance -ne $Instance) { throw "active instance identity differs from -Instance" }
  } else {
    $oldSpmdExe = Join-Path $rootPath "spmd.exe"
    $oldFenceExe = Join-Path $rootPath "pecofence.exe"
  }
  foreach ($binary in @($oldSpmdExe, $oldFenceExe)) {
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "current binary missing: $binary" }
    if (-not $binary.StartsWith($rootPath + '\', [StringComparison]::OrdinalIgnoreCase)) {
      throw "current binary is outside InstallRoot: $binary"
    }
  }
  $oldSpmd = Get-ExactProcess $oldSpmdExe
  $oldFence = Get-ExactProcess $oldFenceExe
  if ($oldSpmd) {
    $oldDaemonCommand = (Get-CimInstance Win32_Process -Filter "ProcessId = $($oldSpmd.Id)").CommandLine
    $sourcePath = if ($fixturePath) { $fixturePath } else { $databasePath }
    if (-not $oldDaemonCommand -or
        $oldDaemonCommand.IndexOf($sourcePath, [StringComparison]::OrdinalIgnoreCase) -lt 0 -or
        ($Instance -and $oldDaemonCommand.IndexOf($Instance, [StringComparison]::OrdinalIgnoreCase) -lt 0)) {
      throw "running spmd arguments do not match the requested instance and data source"
    }
  }
  if ($oldFence) {
    $oldFenceCommand = (Get-CimInstance Win32_Process -Filter "ProcessId = $($oldFence.Id)").CommandLine
    if (-not $oldFenceCommand -or $oldFenceCommand -notmatch '(?i)(?:^|\s)--portable(?:\s|$)') {
      throw "running PecoFence is not portable; this updater cannot preserve its configuration"
    }
  }
  $other = @(Get-Process spmd, pecofence -ErrorAction SilentlyContinue | Where-Object {
    $_.Path -and $_.Path.StartsWith($rootPath + '\', [StringComparison]::OrdinalIgnoreCase) -and
    -not $_.Path.Equals($oldSpmdExe, [StringComparison]::OrdinalIgnoreCase) -and
    -not $_.Path.Equals($oldFenceExe, [StringComparison]::OrdinalIgnoreCase)
  })
  if ($other.Count -gt 0) { throw "unexpected process under InstallRoot: $($other.Id -join ', ')" }

  Assert-Package $pfZipPath
  Assert-Package $spmZipPath
  $candidate = Join-Path $versionRoot ("candidate-" + [guid]::NewGuid().ToString("N"))
  $pfDir = Join-Path $candidate "pecofence"
  $spmDir = Join-Path $candidate "spm"
  New-Item -ItemType Directory -Path $pfDir, $spmDir -Force | Out-Null
  Expand-Archive -LiteralPath $pfZipPath -DestinationPath $pfDir
  Expand-Archive -LiteralPath $spmZipPath -DestinationPath $spmDir
  $newFenceExe = Join-Path $pfDir "pecofence.exe"
  $newSpmdExe = Join-Path $spmDir "spmd.exe"
  foreach ($binary in @($newFenceExe, (Join-Path $pfDir "pecofence-watchdog.exe"),
      (Join-Path $pfDir "Microsoft.UI.Xaml.dll"), $newSpmdExe)) {
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "package missing: $binary" }
  }
  $workspace = Join-Path $configPath 'workspace.v2.json'
  if (-not (Test-Path -LiteralPath $workspace) -and (Test-Path -LiteralPath (Join-Path $configPath 'config.json'))) {
    throw 'Legacy configuration needs an explicit migration decision; current installation was not stopped'
  }
  if (Test-Path -LiteralPath $workspace) {
    $validation = Start-Process -FilePath $newFenceExe -ArgumentList @('--validate-workspace', ('"' + $workspace + '"')) -Wait -PassThru
    if ($validation.ExitCode -ne 0) { throw 'Candidate rejected existing workspace; current installation was not stopped' }
  }
  & $newSpmdExe --help | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "staged spmd failed its executable preflight" }

  if ($oldSpmd) {
    $stopArgs = @("--stop")
    if ($Instance) { $stopArgs += @("--instance", $Instance) }
    & $oldSpmdExe @stopArgs | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not $oldSpmd.WaitForExit(10000)) {
      throw "old spmd has no graceful stop support or did not exit; stop it manually and retry"
    }
    $oldSpmdStopped = $true
  }
  if ($databasePath) {
    foreach ($suffix in @("", "-wal", "-shm")) {
      $file = "$databasePath$suffix"
      if (Test-Path -LiteralPath $file -PathType Leaf) {
        $copy = Join-Path $candidate ("db-backup" + $suffix)
        Copy-Item -LiteralPath $file -Destination $copy
        $backup += @{ source = $file; copy = $copy }
      }
    }
    $databaseBackedUp = $true
  }
  $newSpmd = Start-Daemon $newSpmdExe "candidate"
  Wait-DaemonReady $newSpmd $newSpmdExe
  Assert-PairCompatible $newFenceExe

  if ($oldFence) {
    if (-not [PecoFenceUpdateWindow]::RequestQuit($oldFence.Id) -or
        -not $oldFence.WaitForExit(15000)) {
      throw "old PecoFence has no graceful update exit or did not exit; exit it manually and retry"
    }
    $oldFenceStopped = $true
  }
  $newFence = Start-Fence $newFenceExe "candidate" $true
  Wait-FenceReady $newFence

  $runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
  $oldStartup = (Get-ItemProperty -Path $runKey -Name PecoFence -ErrorAction SilentlyContinue).PecoFence
  if (-not $Instance -and $oldStartup) {
    $quotedOld = '"' + $oldFenceExe + '"'
    if ($oldStartup -eq $quotedOld -or $oldStartup.StartsWith($quotedOld + ' ', [StringComparison]::OrdinalIgnoreCase)) {
      $newStartup = '"' + $newFenceExe + '" --portable --config-dir "' + $configPath + '"'
      Set-ItemProperty -Path $runKey -Name PecoFence -Value $newStartup
      $startupChanged = $true
    }
  }
  $activeRecord = @{
    pecofence = $newFenceExe; spmd = $newSpmdExe; instance = $Instance
    updated_utc = [DateTime]::UtcNow.ToString("o")
    pecofence_sha256 = (Get-FileHash $newFenceExe -Algorithm SHA256).Hash
    spmd_sha256 = (Get-FileHash $newSpmdExe -Algorithm SHA256).Hash
  }
  $tempActive = "$activePath.tmp"
  $activeRecord | ConvertTo-Json | Set-Content -LiteralPath $tempActive -Encoding UTF8
  Move-Item -LiteralPath $tempActive -Destination $activePath -Force
  Write-Output "updated: PecoFence PID $($newFence.Id), spmd PID $($newSpmd.Id)"
  Write-Output "active: $activePath"
} catch {
  $failure = $_
  if ($newFence) {
    if (-not [PecoFenceUpdateWindow]::RequestQuit($newFence.Id) -or
        -not $newFence.WaitForExit(5000)) {
      $candidateFence = Get-Process -Id $newFence.Id -ErrorAction SilentlyContinue
      if ($candidateFence -and $candidateFence.Path -and
          $candidateFence.Path.Equals($newFenceExe, [StringComparison]::OrdinalIgnoreCase)) {
        $candidateFence | Stop-Process -Force -ErrorAction SilentlyContinue
        $newFence.WaitForExit(5000) | Out-Null
      }
    }
  }
  if ($newSpmd) {
    $stopArgs = @("--stop")
    if ($Instance) { $stopArgs += @("--instance", $Instance) }
    try { & $newSpmdExe @stopArgs 2>$null | Out-Null } catch {
      Write-Warning "candidate spmd did not accept a graceful stop request"
    }
    if (-not $newSpmd.WaitForExit(5000)) {
      $candidateDaemon = Get-Process -Id $newSpmd.Id -ErrorAction SilentlyContinue
      if ($candidateDaemon -and $candidateDaemon.Path -and
          $candidateDaemon.Path.Equals($newSpmdExe, [StringComparison]::OrdinalIgnoreCase)) {
        $candidateDaemon | Stop-Process -Force -ErrorAction SilentlyContinue
        $newSpmd.WaitForExit(5000) | Out-Null
      }
    }
  }
  if ($startupChanged) { Set-ItemProperty -Path $runKey -Name PecoFence -Value $oldStartup }
  if ($databaseBackedUp) {
    if ($databasePath) {
      foreach ($suffix in @("", "-wal", "-shm")) {
        Remove-Item -LiteralPath "$databasePath$suffix" -Force -ErrorAction SilentlyContinue
      }
      foreach ($file in $backup) { Copy-Item -LiteralPath $file.copy -Destination $file.source -Force }
    }
  }
  if ($oldSpmdStopped) {
    $restoredSpmd = Start-Daemon $oldSpmdExe "rollback"
    Start-Sleep -Milliseconds 800
    $restoredSpmd.Refresh()
    if ($restoredSpmd.HasExited) { Write-Warning "old spmd failed to restart" }
  }
  if ($oldFenceStopped) {
    $oldWasVersioned = $oldFenceExe.StartsWith($versionRoot + '\', [StringComparison]::OrdinalIgnoreCase)
    $restoredFence = Start-Fence $oldFenceExe "rollback" $oldWasVersioned
    try { Wait-FenceReady $restoredFence } catch { Write-Warning "old PecoFence failed to restart" }
  }
  throw $failure
} finally {
  $lockFile.Dispose()
}
