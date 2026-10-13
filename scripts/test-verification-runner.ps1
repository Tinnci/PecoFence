# Real process/report regressions without Cargo, downloads, GUI or desktop state.
[CmdletBinding()]
param([string]$Python = "python")
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
. (Join-Path $PSScriptRoot "verification-runner.ps1")
$shell = (Get-Process -Id $PID).Path
$temporary = Join-Path (Get-Location) "target/verification-tests-$([guid]::NewGuid().ToString('N'))"
[IO.Directory]::CreateDirectory($temporary) | Out-Null
$count = 0

function Assert-That([bool]$Condition, [string]$Message) {
  if (-not $Condition) { throw "Verification regression: $Message" }
}

function Read-Report([string]$Path) {
  Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Invoke-Fixture([object[]]$Plan, [int]$Timeout = 0, [string]$ReportPath = "") {
  if (-not $ReportPath) { $ReportPath = Join-Path $temporary "fixture.json" }
  $run = New-VerificationRun "Quick" $Timeout
  Invoke-VerificationPlan $run $Plan $ReportPath
  $code = Complete-VerificationRun $run $ReportPath
  [pscustomobject]@{ Run = $run; Code = $code; Path = $ReportPath }
}

function Assert-EntryError([string[]]$Arguments) {
  $report = Join-Path $temporary "entry-error.json"
  Write-Host "`n[TEST] Expect argument/environment error 2: $($Arguments -join ' ')"
  $fixture = Invoke-Fixture @(
    (New-VerificationCheck "entry" "CLI validation" $shell (
      @("-NoProfile", "-NonInteractive", "-File", "scripts/build-and-verify.ps1",
        "-ReportPath", $report) + $Arguments
    ))
  )
  Assert-That ($fixture.Run.Report.checks[0].exit_code -eq 2) "invalid CLI/environment must return 2"
  $result = Read-Report $report
  Assert-That ($result.exit_code -eq 2 -and $result.result -eq "error") "CLI error JSON"
  Assert-That (@($result.checks | Where-Object status -eq "pass").Count -eq 0) "error must not claim passed gates"
  Assert-That ($null -eq $result.source.commit) "invalid CLI must not execute Git provenance commands"
}

try {
  $source = @(Get-DesktopVerificationPlan "SourceOnly" $Python $shell "target" $false)
  Assert-That (@($source | Where-Object { $_.Enabled -and $_.Program -ne $Python }).Count -eq 0) `
    "SourceOnly must not invoke Rust, PowerShell tests or release commands"
  $quick = @(Get-DesktopVerificationPlan "Quick" $Python $shell "target" $false)
  $full = @(Get-DesktopVerificationPlan "Full" $Python $shell "target" $false)
  $fullOnly = @("bindings-generate", "bindings-diff", "release-build", "size", "package")
  foreach ($id in $fullOnly) {
    Assert-That (-not ($quick | Where-Object Id -eq $id).Enabled) "Quick must not run $id"
    Assert-That (($full | Where-Object Id -eq $id).Enabled) "Full must retain $id"
  }
  foreach ($id in @("format", "resolved-policy", "clippy", "runtime-debug", "workspace-tests", "reactor-tests", "validator-tests")) {
    Assert-That (($quick | Where-Object Id -eq $id).Enabled) "Quick must retain $id"
  }
  $count++

  $firstDirectory = Join-Path $temporary "first"
  $secondDirectory = Join-Path $temporary "second"
  [IO.Directory]::CreateDirectory($firstDirectory) | Out-Null
  [IO.Directory]::CreateDirectory($secondDirectory) | Out-Null
  $executableName = "verification-path-priority-$([guid]::NewGuid().ToString('N')).exe"
  # Discovery requires executable names, not runnable binaries. These fixtures
  # are never launched; all actual process regressions use the supplied Python.
  [IO.File]::WriteAllText((Join-Path $firstDirectory $executableName), "fixture")
  [IO.File]::WriteAllText((Join-Path $secondDirectory $executableName), "fixture")
  $savedPath = $env:PATH
  try {
    $env:PATH = "$firstDirectory$([IO.Path]::PathSeparator)$secondDirectory$([IO.Path]::PathSeparator)$savedPath"
    Assert-That (@(Get-Command $executableName -CommandType Application).Count -ge 2) "fixture must expose multiple PATH matches"
    $application = @(Resolve-VerificationApplication $executableName)
    Assert-That ($application.Count -eq 1) "resolve exactly one executable"
    Assert-That ($application[0].Source -eq (Join-Path $firstDirectory $executableName)) "honor PATH priority"
  } finally {
    $env:PATH = $savedPath
  }
  $count++

  $narrow = @(Get-DesktopVerificationPlan "Quick" $Python $shell "target" $false @("pecofence", "pecofence-core"))
  $narrowCheck = $narrow | Where-Object Id -eq "selected-package-tests"
  $narrowArguments = $narrowCheck.Arguments
  Assert-That (@($narrow | Where-Object Id -eq "workspace-tests").Count -eq 0) "selected tests cannot be labeled workspace"
  Assert-That ($narrowArguments -notcontains "--workspace") "narrow tests cannot claim workspace"
  Assert-That ($narrowArguments -contains "pecofence" -and $narrowArguments -contains "pecofence-core") "requested packages retained"
  Assert-That (($narrow | Where-Object Id -eq "reactor-tests").Enabled) "mandatory teardown regressions retained"
  Assert-That (($narrow | Where-Object Id -eq "validator-tests").Enabled) "mandatory validator retained"
  $clean = @(Get-DesktopVerificationPlan "Full" $Python $shell "target" $false @() $true)
  Assert-That (($clean | Where-Object Id -eq "package").Arguments -contains "-RequireCleanSource") "clean release provenance retained"
  $count++

  $literal = 'a space "quote" \path\ 中文 ; $notAShellCommand'
  $argumentFile = Join-Path $temporary "literal argument 中文.json"
  $fixture = Invoke-Fixture @(
    (New-VerificationCheck "literal" "Literal arguments" $Python @(
      "-c", "import pathlib,sys,json; pathlib.Path(sys.argv[1]).write_text(json.dumps(sys.argv[2]),encoding='utf-8')",
      $argumentFile, $literal
    )),
    (New-VerificationCheck "outside" "Release not requested" "does-not-exist" @() $false "Outside Quick scope.")
  )
  Assert-That ($fixture.Code -eq 0) "requested fixture must pass"
  Assert-That ((Get-Content $argumentFile -Raw | ConvertFrom-Json) -eq $literal) "no shell expansion or argument corruption"
  $report = Read-Report $fixture.Path
  Assert-That ($report.schema_version -eq 1 -and $report.mode -eq "Quick") "stable report schema"
  Assert-That (@($report.validated_scope).Count -eq 1 -and $report.validated_scope[0] -eq "literal") "NOT_RUN excluded from passed scope"
  Assert-That ($report.checks[1].status -eq "not_run" -and $null -eq $report.checks[1].exit_code) "not run is not success"
  $firstRunId = $report.run_id
  $count++

  $marker = Join-Path $temporary "must-not-run"
  $fixture = Invoke-Fixture @(
    (New-VerificationCheck "failure" "Failing tool" $Python @("-c", "raise SystemExit(23)")),
    (New-VerificationCheck "later" "Later check" $Python @("-c", "import pathlib,sys; pathlib.Path(sys.argv[1]).touch()", $marker))
  )
  $report = Read-Report $fixture.Path
  Assert-That ($fixture.Code -eq 1 -and $report.exit_code -eq 1 -and $report.result -eq "fail") "failure classification"
  Assert-That ($report.checks[0].exit_code -eq 23) "preserve underlying tool exit code"
  Assert-That ($report.checks[1].status -eq "not_run" -and -not (Test-Path $marker)) "fail fast without executing later checks"
  Assert-That ($report.run_id -ne $firstRunId -and @($report.validated_scope).Count -eq 0) "old passed report cannot leak into new run"
  $count++

  $fixture = Invoke-Fixture @(
    (New-VerificationCheck "missing" "Missing tool" "pecofence-missing-tool-$([guid]::NewGuid().ToString('N'))" @())
  )
  Assert-That ($fixture.Code -eq 2 -and $fixture.Run.Report.checks[0].status -eq "error") "missing executable is environment error"
  $count++

  $during = Join-Path $temporary "during.json"
  $fixture = Invoke-Fixture @(
    (New-VerificationCheck "during" "Read live report" $Python @(
      "-c",
      "import json,sys; r=json.load(open(sys.argv[1],encoding='utf-8')); assert r['result']=='running'; assert r['checks'][0]['status']=='running'; assert r['exit_code'] is None",
      $during
    ))
  ) -ReportPath $during
  Assert-That ($fixture.Code -eq 0) "partial report written before executing command"
  $count++

  $sourceFile = Join-Path $temporary "source.json"
  $sourceRun = Invoke-Fixture @(
    (New-VerificationCheck "source" "Bounded source provenance" $Python @(
      "scripts/verification-source.py", "--out", $sourceFile
    ) -FailureKind "error" -SourcePath $sourceFile)
  )
  Assert-That ($sourceRun.Code -eq 0) "source provenance runs inside the command runner"
  Assert-That ($sourceRun.Run.Report.source.commit -match "^[0-9a-f]{40}$") "source commit parsed into aggregate report"
  Assert-That ($sourceRun.Run.Report.source.dirty -is [bool]) "dirty state parsed"
  $count++

  $pidFile = Join-Path $temporary "child.pid"
  $fixture = Invoke-Fixture @(
    (New-VerificationCheck "timeout" "Process tree timeout" $Python @(
      "-c",
      "import pathlib,subprocess,sys,time; p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(p.pid)); time.sleep(30)",
      $pidFile
    )),
    (New-VerificationCheck "later" "After timeout" $Python @("-c", "raise SystemExit(0)"))
  ) -Timeout 2
  $report = Read-Report $fixture.Path
  Assert-That ($fixture.Code -eq 124 -and $report.result -eq "timeout") "timeout classification"
  Assert-That ($report.checks[1].status -eq "not_run") "stop after timeout"
  Assert-That (Test-Path $pidFile) "timeout fixture actually spawned a child"
  $childPid = [int](Get-Content $pidFile -Raw)
  $child = Get-Process -Id $childPid -ErrorAction SilentlyContinue
  if ($child) {
    Assert-That ($child.WaitForExit(3000)) "timeout must stop descendants, not only the parent"
    $child.Dispose()
  }
  $count++

  $expired = New-VerificationRun "Quick" 1
  $expired.Watch.Stop()
  # Inject an already exhausted clock, without waiting or launching a process.
  $expired.Watch = [pscustomobject]@{ Elapsed = [TimeSpan]::FromSeconds(2) }
  $expired.Watch | Add-Member ScriptMethod Stop {}
  $budgetPlan = @((New-VerificationCheck "budget" "Budget" "must-not-start-after-deadline" @()))
  Invoke-VerificationPlan $expired $budgetPlan
  Assert-That ((Complete-VerificationRun $expired) -eq 124) "budget timeout without report path"
  Assert-That ($null -eq $expired.Report.checks[0].exit_code) "expired budget cannot launch the command"
  $count++

  $noReport = New-VerificationRun "Quick"
  Invoke-VerificationPlan $noReport @((New-VerificationCheck "ok" "Console-only success" $Python @("-c", "raise SystemExit(0)")))
  Assert-That ((Complete-VerificationRun $noReport) -eq 0) "report path is optional"
  Assert-That ($noReport.Report.validated_scope -contains "ok") "console-only scope must not be empty"
  $count++

  $narrowScope = New-VerificationRun "Quick" -TestPackages @("pecofence-core")
  Invoke-VerificationPlan $narrowScope @(
    (New-VerificationCheck $narrowCheck.Id $narrowCheck.Name $Python @("-c", "raise SystemExit(0)"))
  )
  Assert-That ((Complete-VerificationRun $narrowScope) -eq 0) "selected package report"
  Assert-That ($narrowScope.Report.validated_scope -contains "selected-package-tests") "selected package scope explicit"
  Assert-That ($narrowScope.Report.validated_scope -notcontains "workspace-tests") "cannot claim whole workspace validated"
  $count++

  $interrupted = New-VerificationRun "Quick"
  $interrupted.Report.checks.Add([pscustomobject]@{
    id = "interrupted"; name = "Interrupted check"; status = "running"
    duration_seconds = 0; exit_code = $null; reason = $null; command = @()
  })
  Assert-That ((Complete-VerificationRun $interrupted) -eq 2) "incomplete run cannot pass"
  Assert-That ($interrupted.Report.checks[0].status -eq "error") "terminal report cannot contain a running check"
  $count++

  $badPath = Join-Path $temporary "is-a-directory"
  [IO.Directory]::CreateDirectory($badPath) | Out-Null
  $run = New-VerificationRun "Quick"
  Invoke-VerificationPlan $run @((New-VerificationCheck "ok" "Success" $Python @("-c", "raise SystemExit(0)")))
  Assert-That ((Complete-VerificationRun $run $badPath) -eq 2) "cannot report success when report writing failed"
  $count++

  foreach ($arguments in @(
    @("-Mode", "Unknown"),
    @("-UnknownParameter", "value"),
    @("-SourceOnly", "-Mode", "Quick"),
    @("-SourceOnly", "-SkipSourceChecks"),
    @("-Mode", "Full", "-TestPackage", "pecofence"),
    @("-Mode", "Quick", "-TestPackage", "--workspace"),
    @("-Mode", "Quick", "-RequireCleanSource"),
    @("-SourceOnly", "-TimeoutSeconds", "-1"),
    @("-SourceOnly", "-TimeoutSeconds", "not-a-number"),
    @("-SourceOnly", "-Python", "pecofence-missing-python")
  )) {
    Assert-EntryError $arguments
    $count++
  }
  Write-Host "`nVerification runner: $count regression groups passed."
} finally {
  # Owned test fixtures only, inside the ignored project target directory.
  Remove-Item -LiteralPath $temporary -Recurse -Force
}
