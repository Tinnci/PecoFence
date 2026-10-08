# The desktop build contract shared by local verification, CI and tag releases.
# No publishing or machine-wide tool/credential configuration happens here.
[CmdletBinding()]
param(
  [string]$TargetDir = "target",
  [string]$Python = "python",
  [string]$Mode = "Full",
  [string]$ReportPath = "",
  [string]$TimeoutSeconds = "0",
  [string[]]$TestPackage = @(),
  [switch]$SourceOnly,
  [switch]$SkipSourceChecks,
  [switch]$RequireCleanSource,
  [Parameter(ValueFromRemainingArguments=$true)][string[]]$RemainingArguments
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
. (Join-Path $PSScriptRoot "verification-runner.ps1")
$effectiveMode = if ($SourceOnly) { "SourceOnly" } else { $Mode }
$packages = @($TestPackage | ForEach-Object { $_ -split "," } | ForEach-Object { $_.Trim() })
$run = New-VerificationRun $effectiveMode -TestPackages $packages -SkipSourceChecks $SkipSourceChecks.IsPresent
$sourcePath = Join-Path $root "target/verification-source-$($run.Report.run_id).json"
try {
  if ($PSVersionTable.PSVersion.Major -lt 7) { throw "PowerShell 7 or newer is required." }
  if ($RemainingArguments.Count -gt 0) { throw "Unexpected arguments: $($RemainingArguments -join ' ')" }
  if ($SourceOnly -and $PSBoundParameters.ContainsKey("Mode") -and $Mode -ne "SourceOnly") {
    throw "Use either -SourceOnly or -Mode, not conflicting modes."
  }
  if ($effectiveMode -notin @("SourceOnly", "Quick", "Full")) {
    throw "Mode must be SourceOnly, Quick, or Full."
  }
  $effectiveMode = @("SourceOnly", "Quick", "Full") |
    Where-Object { $_ -eq $effectiveMode }
  $run.Report.mode = $effectiveMode
  $budget = 0
  if (-not [int]::TryParse($TimeoutSeconds, [ref]$budget) -or $budget -lt 0) {
    throw "TimeoutSeconds must be a non-negative integer; 0 means no time limit."
  }
  $run.TimeoutSeconds = $budget
  if ($effectiveMode -eq "SourceOnly" -and $SkipSourceChecks) { throw "SourceOnly cannot skip its own checks." }
  if ($packages.Count -gt 0 -and $effectiveMode -ne "Quick") { throw "TestPackage is only valid in Quick mode." }
  foreach ($package in $packages) {
    if ($package -notmatch "^[A-Za-z0-9][A-Za-z0-9_-]*$") { throw "Invalid test package: $package" }
  }
  if ($RequireCleanSource -and $effectiveMode -ne "Full") {
    throw "RequireCleanSource is only valid in Full mode."
  }
  $powerShell = (Get-Process -Id $PID).Path
  $plan = @(Get-DesktopVerificationPlan $effectiveMode $Python $powerShell $TargetDir `
    $SkipSourceChecks.IsPresent $packages $RequireCleanSource.IsPresent $sourcePath)
  Invoke-VerificationPlan $run $plan $ReportPath
} catch {
  Add-VerificationError $run "configuration" $_.Exception.Message
} finally {
  try {
    if ([IO.File]::Exists($sourcePath)) { [IO.File]::Delete($sourcePath) }
  } catch {
    Add-VerificationError $run "source-cleanup" $_.Exception.Message
  }
}
$code = Complete-VerificationRun $run $ReportPath
exit $code
