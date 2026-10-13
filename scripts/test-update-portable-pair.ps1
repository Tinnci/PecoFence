# Windows smoke test. Pass the freshly built Windows debug binaries and a v2 fixture directory.
param(
  [Parameter(Mandatory = $true)][string]$PecoFenceBinDir,
  [Parameter(Mandatory = $true)][string]$SpmBinDir,
  [Parameter(Mandatory = $true)][string]$FixtureDir,
  [switch]$FailNewFence,
  [switch]$Persistent,
  [string]$CandidatePecoFenceZip = "",
  [string]$CandidateSpmZip = ""
)
$ErrorActionPreference = "Stop"
if ([bool]$CandidatePecoFenceZip -ne [bool]$CandidateSpmZip) {
  throw "Both candidate ZIP paths are required together"
}
if ($FailNewFence -and $CandidatePecoFenceZip) { throw "FailNewFence uses the injected test ZIP" }
$root = Join-Path $env:TEMP ("pecofence-update-smoke-" + [guid]::NewGuid().ToString("N"))
$instance = "upgrade-" + [guid]::NewGuid().ToString("N")
$oldEnv = $env:PECOFENCE_INSTANCE
try {
  New-Item -ItemType Directory -Path $root -Force | Out-Null
  foreach ($file in @("pecofence.exe", "pecofence-watchdog.exe")) {
    Copy-Item (Join-Path $PecoFenceBinDir $file) (Join-Path $root $file)
  }
  $runtimeManifest = Get-Content -Raw -Encoding UTF8 (Join-Path $PSScriptRoot '..\third_party\winappsdk\runtime-manifest.json') | ConvertFrom-Json
  foreach ($entry in $runtimeManifest.files) {
    $destination = Join-Path $root $entry.path
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath (Join-Path $PecoFenceBinDir $entry.path) -Destination $destination
  }
  Copy-Item (Join-Path $SpmBinDir "spmd.exe") (Join-Path $root "spmd.exe")
  Copy-Item $FixtureDir (Join-Path $root "fixture") -Recurse
  $fixture = Join-Path $root "fixture\catalog.json"
  $database = Join-Path $root "state.sqlite"
  $pfStage = Join-Path $root "pf-package"
  $spmStage = Join-Path $root "spm-package"
  New-Item -ItemType Directory -Path $pfStage, $spmStage | Out-Null
  foreach ($file in @("pecofence.exe", "pecofence-watchdog.exe")) {
    Copy-Item (Join-Path $root $file) (Join-Path $pfStage $file)
  }
  foreach ($entry in $runtimeManifest.files) {
    $destination = Join-Path $pfStage $entry.path
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath (Join-Path $root $entry.path) -Destination $destination
  }
  if ($FailNewFence) {
    # Reach the rollback path after stopping the old daemon: workspace preflight
    # and protocol preflights succeed, then the candidate fails to launch. This is a
    # disposable test executable, never part of a product package.
    $compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    if (-not (Test-Path -LiteralPath $compiler)) { throw 'Rollback fixture requires the Windows .NET Framework C# compiler' }
    $failureSource = Join-Path $root 'failed-candidate.cs'
    @'
internal static class FailedCandidate {
    static int Main(string[] args) {
        bool validate = args.Length == 2 && args[0] == "--validate-workspace";
        bool probe = args.Length == 1 && args[0] == "--probe-spm";
        return validate || probe ? 0 : 42;
    }
}
'@ | Set-Content -LiteralPath $failureSource -Encoding UTF8
    & $compiler /nologo /target:exe ("/out:" + (Join-Path $pfStage 'pecofence.exe')) $failureSource
    if ($LASTEXITCODE -ne 0) { throw 'Cannot compile failed-candidate fixture' }
  }
  Copy-Item (Join-Path $root "spmd.exe") (Join-Path $spmStage "spmd.exe")
  $pfZip = Join-Path $root "pf.zip"
  $spmZip = Join-Path $root "spm.zip"
  Compress-Archive (Join-Path $pfStage '*') $pfZip
  Compress-Archive (Join-Path $spmStage '*') $spmZip
  foreach ($zip in @($pfZip, $spmZip)) {
    $hash = (Get-FileHash $zip -Algorithm SHA256).Hash
    "$hash  $(Split-Path $zip -Leaf)" | Set-Content "$zip.sha256"
  }
  if ($CandidatePecoFenceZip) {
    $pfZip = (Resolve-Path -LiteralPath $CandidatePecoFenceZip).ProviderPath
    $spmZip = (Resolve-Path -LiteralPath $CandidateSpmZip).ProviderPath
  }
  $daemonArguments = @("--instance", $instance)
  if ($Persistent) { $daemonArguments += @("--db", ('"' + $database + '"')) }
  else { $daemonArguments += @("--fixture", ('"' + $fixture + '"')) }
  $oldDaemon = Start-Process -FilePath (Join-Path $root "spmd.exe") `
    -ArgumentList $daemonArguments `
    -WorkingDirectory $root -PassThru
  $env:PECOFENCE_INSTANCE = $instance
  $oldFence = Start-Process -FilePath (Join-Path $root "pecofence.exe") `
    -ArgumentList @("--portable", "--no-hide-icons") -WorkingDirectory $root -PassThru
  $env:PECOFENCE_INSTANCE = $oldEnv
  Start-Sleep -Seconds 2
  $oldDaemon.Refresh()
  $oldFence.Refresh()
  if ($oldDaemon.HasExited -or $oldFence.HasExited) { throw "old pair failed to start" }
  if ($FailNewFence) {
    $failed = $false
    try {
      $updateArguments = @{
        InstallRoot = $root; PecoFenceZip = $pfZip; SpmZip = $spmZip
        Instance = $instance; NoHideIcons = $true
      }
      if ($Persistent) { $updateArguments.DbPath = $database }
      else { $updateArguments.Fixture = $fixture }
      & (Join-Path $PSScriptRoot "update-portable-pair.ps1") @updateArguments
    } catch { $failed = $true }
    if (-not $failed -or (Test-Path (Join-Path $root "active.json"))) {
      throw "failed candidate was activated"
    }
    if ($Persistent -and -not (Test-Path $database -PathType Leaf)) {
      throw "database missing after rollback"
    }
    $oldDaemon.Refresh()
    $oldFence.Refresh()
    if (-not $oldDaemon.HasExited -or -not $oldFence.HasExited) {
      throw 'Failure fixture did not reach both process replacements and rollback'
    }
    Start-Sleep -Seconds 1
    $restored = @(Get-Process spmd, pecofence -ErrorAction SilentlyContinue | Where-Object {
      $_.Path -and $_.Path.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase) -and
      [IO.Path]::GetDirectoryName($_.Path).Equals($root, [StringComparison]::OrdinalIgnoreCase)
    })
    if (@($restored | Where-Object ProcessName -eq "spmd").Count -ne 1 -or
        @($restored | Where-Object ProcessName -eq "pecofence").Count -ne 1) {
      throw "old pair was not restored after failed candidate"
    }
    Write-Output "PASS failed candidate rolled back to the old pair"
    return
  }
  $updateArguments = @{
    InstallRoot = $root; PecoFenceZip = $pfZip; SpmZip = $spmZip
    Instance = $instance; NoHideIcons = $true
  }
  if ($Persistent) { $updateArguments.DbPath = $database }
  else { $updateArguments.Fixture = $fixture }
  & (Join-Path $PSScriptRoot "update-portable-pair.ps1") @updateArguments
  $active = Get-Content (Join-Path $root "active.json") -Raw | ConvertFrom-Json
  if (-not (Test-Path $active.pecofence) -or -not (Test-Path $active.spmd)) {
    throw "active pointer is incomplete"
  }
  $oldDaemon.Refresh()
  $oldFence.Refresh()
  if (-not $oldDaemon.HasExited -or -not $oldFence.HasExited) { throw "old pair still running" }
  $first = $active
  & (Join-Path $PSScriptRoot "update-portable-pair.ps1") @updateArguments
  $active = Get-Content (Join-Path $root "active.json") -Raw | ConvertFrom-Json
  if ($active.pecofence -eq $first.pecofence -or $active.spmd -eq $first.spmd) {
    throw "second update did not switch both versioned binaries"
  }
  $retired = @(Get-Process spmd, pecofence -ErrorAction SilentlyContinue | Where-Object {
    $_.Path -and ($_.Path.Equals($first.pecofence, [StringComparison]::OrdinalIgnoreCase) -or
      $_.Path.Equals($first.spmd, [StringComparison]::OrdinalIgnoreCase))
  })
  if ($retired.Count -gt 0) { throw "first candidate remains running after second update" }
  Write-Output "PASS first and subsequent staged pair switches"
} finally {
  $env:PECOFENCE_INSTANCE = $oldEnv
  @(Get-Process spmd, pecofence, pecofence-watchdog -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase) }) |
    Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 500
  Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
}
