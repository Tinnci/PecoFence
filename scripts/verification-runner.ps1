# Shared command runner and explicit verification plan. Dot-sourcing defines
# functions only; it does not execute checks or modify the machine.

function New-VerificationCheck {
  param(
    [string]$Id, [string]$Name, [string]$Program, [string[]]$Arguments,
    [bool]$Enabled = $true, [string]$SkipReason = "",
    [string]$FailureKind = "fail", [string]$SourcePath = ""
  )
  [pscustomobject]@{
    Id = $Id; Name = $Name; Program = $Program; Arguments = @($Arguments)
    Enabled = $Enabled; SkipReason = $SkipReason; FailureKind = $FailureKind
    SourcePath = $SourcePath
  }
}

function New-VerificationRun {
  param(
    [string]$Mode, [int]$TimeoutSeconds = 0,
    [string[]]$TestPackages = @(), [bool]$SkipSourceChecks = $false
  )
  [pscustomobject]@{
    Watch = [Diagnostics.Stopwatch]::StartNew()
    TimeoutSeconds = $TimeoutSeconds
    Report = [ordered]@{
      schema_version = 1
      run_id = [guid]::NewGuid().ToString()
      started_at = [DateTime]::UtcNow.ToString("o")
      mode = $Mode
      result = "running"
      exit_code = $null
      duration_seconds = 0
      source = [ordered]@{ commit = $null; dirty = $null }
      requested_test_packages = @($TestPackages)
      source_checks_skipped = $SkipSourceChecks
      validated_scope = @()
      checks = [Collections.Generic.List[object]]::new()
    }
  }
}

function Write-VerificationReport {
  param($Run, [string]$ReportPath)
  $Run.Report.duration_seconds = [math]::Round($Run.Watch.Elapsed.TotalSeconds, 2)
  $Run.Report.validated_scope = @(
    $Run.Report.checks | Where-Object status -eq "pass" | ForEach-Object id
  )
  if (-not $ReportPath) { return }
  $path = [IO.Path]::GetFullPath($ReportPath)
  $parent = [IO.Path]::GetDirectoryName($path)
  [IO.Directory]::CreateDirectory($parent) | Out-Null
  $temporary = "$path.tmp-$([guid]::NewGuid().ToString('N'))"
  try {
    $json = $Run.Report | ConvertTo-Json -Depth 12
    [IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($temporary, $path, $true)
  } finally {
    if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
  }
}

function Add-VerificationError {
  param($Run, [string]$Id, [string]$Reason)
  $Run.Report.checks.Add([pscustomobject]@{
    id = $Id; name = $Id; status = "error"; duration_seconds = 0
    exit_code = $null; reason = $Reason; command = @()
  })
}

function Resolve-VerificationApplication {
  param([string]$Program)
  # Get-Command -CommandType Application can return several PATH matches.
  # Match ordinary shell resolution: launch only the highest-priority one.
  $applications = @(Get-Command $Program -CommandType Application -ErrorAction Stop)
  $applications[0]
}

function Invoke-VerificationPlan {
  param($Run, [object[]]$Plan, [string]$ReportPath = "")
  foreach ($check in $Plan) {
    $Run.Report.checks.Add([pscustomobject]@{
      id = $check.Id; name = $check.Name; status = "not_run"; duration_seconds = 0
      exit_code = $null; reason = "An earlier check did not complete."
      command = @($check.Program) + @($check.Arguments)
    })
  }
  Write-VerificationReport $Run $ReportPath
  for ($index = 0; $index -lt $Plan.Count; $index++) {
    $check = $Plan[$index]
    $entry = $Run.Report.checks[$index]
    if (-not $check.Enabled) {
      $entry.reason = $check.SkipReason
      Write-Host "[NOT_RUN] $($check.Name) — $($entry.reason)"
      continue
    }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $process = $null
    $started = $false
    try {
      $entry.status = "running"
      $entry.reason = $null
      Write-VerificationReport $Run $ReportPath
      $remaining = if ($Run.TimeoutSeconds -gt 0) {
        $Run.TimeoutSeconds - $Run.Watch.Elapsed.TotalSeconds
      } else { [double]::PositiveInfinity }
      if ($remaining -le 0) {
        $entry.status = "timeout"
        $entry.reason = "The verification time budget was exhausted before this check."
      } else {
        $application = Resolve-VerificationApplication $check.Program
        $start = [Diagnostics.ProcessStartInfo]::new()
        $start.FileName = $application.Source
        $start.UseShellExecute = $false
        $start.WorkingDirectory = (Get-Location).ProviderPath
        foreach ($argument in $check.Arguments) {
          $start.ArgumentList.Add([string]$argument)
        }
        # Inherit stdout/stderr: raw tool diagnostics remain live and untruncated.
        # No shell evaluates the arguments, and no environment values enter JSON.
        $process = [Diagnostics.Process]::new()
        $process.StartInfo = $start
        Write-Host "`n[RUN] $($check.Name): $($check.Program) $($check.Arguments -join ' ')"
        if (-not $process.Start()) { throw "Could not start $($check.Program)." }
        $started = $true
        if ($Run.TimeoutSeconds -eq 0) {
          $process.WaitForExit()
          $completed = $true
        } else {
          $completed = $false
          do {
            $remaining = [math]::Max(0, $Run.TimeoutSeconds - $Run.Watch.Elapsed.TotalSeconds)
            $milliseconds = [int][math]::Min(1000, [math]::Ceiling($remaining * 1000))
            $completed = $process.WaitForExit($milliseconds)
          } while (-not $completed -and $remaining -gt 0)
        }
        if (-not $completed) {
          $process.Kill($true)
          $process.WaitForExit()
          $entry.exit_code = $process.ExitCode
          $entry.status = "timeout"
          $entry.reason = "The verification time budget expired; the command process tree was stopped."
        } else {
          $entry.exit_code = $process.ExitCode
          $entry.status = if ($process.ExitCode -eq 0) { "pass" } else { $check.FailureKind }
          if ($process.ExitCode -ne 0) {
            $entry.reason = "Command returned exit code $($process.ExitCode). See its diagnostics above."
          } elseif ($check.SourcePath) {
            $Run.Report.source = Get-Content -LiteralPath $check.SourcePath -Raw | ConvertFrom-Json
          }
        }
      }
    } catch {
      $entry.status = "error"
      $entry.reason = $_.Exception.Message
    } finally {
      if ($process) {
        try {
          if ($started -and -not $process.HasExited) {
            $process.Kill($true)
            $process.WaitForExit()
          }
        } catch {
          $entry.status = "error"
          $entry.reason = "Could not stop command process tree: $($_.Exception.Message)"
        } finally {
          $process.Dispose()
        }
      }
      $timer.Stop()
      $entry.duration_seconds = [math]::Round($timer.Elapsed.TotalSeconds, 2)
    }
    Write-Host "[$($entry.status.ToUpperInvariant())] $($check.Name) — $($entry.duration_seconds)s"
    if ($entry.reason) { Write-Host "  $($entry.reason)" }
    Write-VerificationReport $Run $ReportPath
    if ($entry.status -ne "pass") { break }
  }
}

function Complete-VerificationRun {
  param($Run, [string]$ReportPath = "")
  $Run.Watch.Stop()
  foreach ($entry in $Run.Report.checks | Where-Object status -eq "running") {
    $entry.status = "error"
    $entry.reason = "Command execution was interrupted before a result was recorded."
  }
  $failed = $Run.Report.checks |
    Where-Object { $_.status -in @("fail", "error", "timeout", "running") } |
    Select-Object -First 1
  if ($failed) {
    $Run.Report.result = if ($failed.status -eq "running") { "error" } else { $failed.status }
    $Run.Report.exit_code = switch ($Run.Report.result) {
      "fail" { 1 }; "timeout" { 124 }; default { 2 }
    }
  } else {
    $Run.Report.result = "passed"
    $Run.Report.exit_code = 0
  }
  $reportWritten = $false
  try {
    Write-VerificationReport $Run $ReportPath
    $reportWritten = [bool]$ReportPath
  } catch {
    Add-VerificationError $Run "report" "Could not write report: $($_.Exception.Message)"
    $Run.Report.result = "error"
    $Run.Report.exit_code = 2
    Write-Host "[ERROR] Report — $($_.Exception.Message)"
  }
  $failed = $Run.Report.checks |
    Where-Object { $_.status -in @("fail", "error", "timeout", "running") } |
    Select-Object -First 1
  Write-Host "`nRESULT: $($Run.Report.result.ToUpperInvariant())"
  Write-Host "MODE: $($Run.Report.mode)"
  Write-Host "EXIT_CODE: $($Run.Report.exit_code)"
  Write-Host "DURATION: $([math]::Round($Run.Watch.Elapsed.TotalSeconds, 2))s"
  if ($failed) {
    Write-Host "FAILED_CHECK: $($failed.name)"
    if ($failed.reason) { Write-Host "DIAGNOSTIC: $($failed.reason)" }
    Write-Host "NEXT: Resolve the diagnostic above and rerun the same mode."
  } elseif ($Run.Report.exit_code -eq 0) {
    Write-Host "SCOPE: $($Run.Report.validated_scope -join ', ')"
    if ($Run.Report.mode -ne "Full" -or $Run.Report.source_checks_skipped) {
      Write-Host "NEXT: Only the requested scope passed. Consult NOT_RUN checks before claiming full verification."
    } else {
      Write-Host "NEXT: Automated Full verification passed; real-machine release acceptance remains separate."
    }
  }
  $notRun = @($Run.Report.checks | Where-Object status -eq "not_run" | ForEach-Object id)
  if ($notRun.Count -gt 0) { Write-Host "NOT_RUN: $($notRun -join ', ')" }
  if ($reportWritten) { Write-Host "REPORT: $([IO.Path]::GetFullPath($ReportPath))" }
  [int]$Run.Report.exit_code
}

function Get-DesktopVerificationPlan {
  param(
    [string]$Mode, [string]$Python, [string]$PowerShell,
    [string]$TargetDir, [bool]$SkipSourceChecks,
    [string[]]$TestPackages = @(), [bool]$RequireCleanSource = $false,
    [string]$SourcePath = ""
  )
  $plan = [Collections.Generic.List[object]]::new()
  $source = -not $SkipSourceChecks
  $native = $Mode -ne "SourceOnly"
  $full = $Mode -eq "Full"
  $sourceReason = "Source checks were explicitly skipped; consult the separate preflight report."
  $nativeReason = "Outside SourceOnly scope; requires the native toolchain."
  $fullReason = "Outside $Mode scope; run Full to verify generated bindings and release artifacts."
  $plan.Add((New-VerificationCheck "python" "Python >=3.11" $Python @(
    "-c", "import sys; print('Python ' + sys.version.split()[0]); sys.exit(0 if sys.version_info >= (3, 11) else 2)"
  ) -FailureKind "error"))
  if (-not $SourcePath) {
    $SourcePath = Join-Path $TargetDir "verification-source-$([guid]::NewGuid().ToString('N')).json"
  }
  $plan.Add((New-VerificationCheck "source" "Source provenance" $Python @(
    "scripts/verification-source.py", "--out", $SourcePath
  ) -FailureKind "error" -SourcePath $SourcePath))
  foreach ($item in @(
    @("public-policy-tests", "Public dependency policy tests", "test-public-dependencies.py"),
    @("public-policy", "Public dependency boundary", "check-public-dependencies.py"),
    @("git-cache-tests", "Public Git cache safety tests", "test-public-git-cache.py"),
    @("action-tests", "Action policy tests", "test-actions-policy.py"),
    @("actions", "Action pin policy", "check-actions.py"),
    @("locale-tests", "Native locale scanner tests", "test-locales.py"),
    @("locales", "Translations", "check-locales.py"),
    @("readme-tests", "README translation tests", "test-readme-translations.py"),
    @("readmes", "README translations", "check-readme-translations.py"),
    @("package-tests", "Desktop packaging tests", "test-package-desktop.py"),
    @("runtime-tests", "Windows App SDK runtime tests", "test-winappsdk-runtime.py")
  )) {
    $plan.Add((New-VerificationCheck $item[0] $item[1] $Python @("scripts/$($item[2])") $source $sourceReason))
  }
  $plan.Add((New-VerificationCheck "runner-tests" "Verification runner tests" $PowerShell @(
    "-NoProfile", "-NonInteractive", "-File", "scripts/test-verification-runner.ps1", "-Python", $Python
  ) $native $nativeReason))
  $plan.Add((New-VerificationCheck "cargo" "Rust toolchain" "cargo" @("--version") $native $nativeReason "error"))
  $plan.Add((New-VerificationCheck "git" "Git toolchain" "git" @("--version") $native $nativeReason "error"))
  $plan.Add((New-VerificationCheck "format" "Format" "cargo" @("fmt", "--all", "--check") $native $nativeReason))
  $plan.Add((New-VerificationCheck "resolved-policy" "Resolved public dependency graph" $Python @(
    "scripts/check-public-dependencies.py", "--resolved"
  ) $native $nativeReason))
  $plan.Add((New-VerificationCheck "clippy" "Clippy" "cargo" @(
    "clippy", "--locked", "--workspace", "--all-targets", "--target-dir", $TargetDir, "--", "-D", "warnings"
  ) $native $nativeReason))
  $plan.Add((New-VerificationCheck "bindings-generate" "Generate bindings" "cargo" @(
    "run", "--locked", "-p", "tool_bindgen", "--target-dir", $TargetDir
  ) $full $fullReason))
  $plan.Add((New-VerificationCheck "bindings-diff" "Bindings match source" "git" @(
    "diff", "--exit-code", "--", "crates/platform/src/bindings.rs",
    "crates/render/src/comp.rs", "crates/render/src/gpu_bindings.rs"
  ) $full $fullReason))
  $plan.Add((New-VerificationCheck "runtime-debug" "Stage debug Windows App SDK runtime" $Python @(
    "scripts/stage-winappsdk-runtime.py", "stage", "--target-dir", $TargetDir, "--profile", "debug"
  ) $native $nativeReason))
  $tests = @("test", "--locked")
  $testId = "workspace-tests"
  $testName = "Workspace tests"
  if ($TestPackages.Count -eq 0) { $tests += "--workspace" }
  else {
    $testId = "selected-package-tests"
    $testName = "Selected package tests: $($TestPackages -join ', ')"
    foreach ($package in $TestPackages) { $tests += @("-p", $package) }
  }
  $tests += @("--target-dir", $TargetDir)
  $plan.Add((New-VerificationCheck $testId $testName "cargo" $tests $native $nativeReason))
  $plan.Add((New-VerificationCheck "reactor-tests" "Reactor dialog teardown regression tests" "cargo" @(
    "test", "--locked", "-p", "windows-reactor", "--lib", "content_dialog_reset_tests", "--target-dir", $TargetDir
  ) $native $nativeReason))
  $plan.Add((New-VerificationCheck "validator-tests" "Workspace fixture validator tests" "cargo" @(
    "test", "--locked", "-p", "pecofence-core", "--example", "validate_workspace", "--target-dir", $TargetDir
  ) $native $nativeReason))
  $plan.Add((New-VerificationCheck "release-build" "Release build" $PowerShell @(
    "-NoProfile", "-NonInteractive", "-File", "scripts/build-desktop.ps1", "-TargetDir", $TargetDir, "-Python", $Python
  ) $full $fullReason))
  $plan.Add((New-VerificationCheck "size" "6.5 MiB executable budget" $Python @(
    "-c",
    "import pathlib,sys; size=pathlib.Path(sys.argv[1]).stat().st_size; print(f'pecofence.exe = {size} bytes (limit: 6815744)'); sys.exit(0 if size <= 6815744 else 1)",
    (Join-Path $TargetDir "release/pecofence.exe")
  ) $full $fullReason))
  $packageArguments = @(
    "-NoProfile", "-NonInteractive", "-File", "scripts/make-portable.ps1", "-SkipBuild", "-TargetDir", $TargetDir, "-Python", $Python
  )
  if ($RequireCleanSource) { $packageArguments += "-RequireCleanSource" }
  $plan.Add((New-VerificationCheck "package" "Verify portable package" $PowerShell $packageArguments $full $fullReason))
  $plan.ToArray()
}
