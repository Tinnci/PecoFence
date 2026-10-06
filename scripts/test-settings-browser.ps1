# Headless production Settings UI against the local mock; never starts PecoFence.
[CmdletBinding()]
param([string]$Browser = "")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
if (!$Browser) {
  foreach ($candidate in @(
    (Join-Path $env:ProgramFiles "Google/Chrome/Application/chrome.exe"),
    (Join-Path ${env:ProgramFiles(x86)} "Microsoft/Edge/Application/msedge.exe"),
    (Join-Path $env:ProgramFiles "Microsoft/Edge/Application/msedge.exe")
  )) {
    if (Test-Path -LiteralPath $candidate) { $Browser = $candidate; break }
  }
}
if (!$Browser) { throw "Install Chrome/Edge or provide -Browser for Settings browser verification." }

$output = Join-Path $root (".cache/settings-browser/" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $output -Force | Out-Null
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()
$previousPort = $env:PECOFENCE_UI_TEST_PORT
$node = $null
$browserProcess = $null
$passed = $false
try {
  $env:PECOFENCE_UI_TEST_PORT = "$port"
  $node = Start-Process node -ArgumentList "scripts/test-settings-ui.mjs" -WorkingDirectory $root `
    -RedirectStandardOutput (Join-Path $output "server.txt") `
    -RedirectStandardError (Join-Path $output "server-errors.txt") -WindowStyle Hidden -PassThru
  $ready = $false
  for ($attempt = 0; $attempt -lt 50; $attempt++) {
    if ($node.HasExited) { throw (Get-Content -Raw (Join-Path $output "server-errors.txt")) }
    try {
      Invoke-WebRequest "http://127.0.0.1:$port" -UseBasicParsing -TimeoutSec 1 | Out-Null
      $ready = $true; break
    } catch { Start-Sleep -Milliseconds 100 }
  }
  if (!$ready) { throw "Settings mock server did not start." }
  $arguments = @(
    "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
    "--disable-background-networking", "--disable-sync", "--disable-extensions", "--disable-component-update",
    "--user-data-dir=`"$(Join-Path $output 'profile')`"", "--virtual-time-budget=60000", "--dump-dom",
    "http://127.0.0.1:$port"
  )
  $browserProcess = Start-Process $Browser -ArgumentList $arguments `
    -RedirectStandardOutput (Join-Path $output "dom.html") `
    -RedirectStandardError (Join-Path $output "browser-errors.txt") -WindowStyle Hidden -PassThru
  if (!$browserProcess.WaitForExit(120000)) { throw "Settings browser verification timed out." }
  $html = Get-Content -Raw (Join-Path $output "dom.html")
  [regex]::Matches($html, '<p id="summary">[^<]*</p>|<li class="fail">.*?</li>') |
    ForEach-Object { Write-Host $_.Value }
  if ($browserProcess.ExitCode -ne 0 -or $html -notmatch '<p id="summary">\d+ passed; 0 failed</p>') {
    throw "Settings browser verification failed. Evidence: $output"
  }
  $passed = $true
} finally {
  $env:PECOFENCE_UI_TEST_PORT = $previousPort
  if ($browserProcess -and !$browserProcess.HasExited) { $browserProcess.Kill() }
  if ($node -and !$node.HasExited) { Stop-Process -Id $node.Id -Force }
  if ($passed) {
    # Only remove this test's newly-created directory inside the worktree.
    Remove-Item -LiteralPath $output -Recurse -Force -ErrorAction SilentlyContinue
  } else {
    Write-Host "Settings browser evidence retained at $output"
  }
}
