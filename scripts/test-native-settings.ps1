param(
    [Parameter(Mandatory = $true)][string]$TestBinary,
    [string]$EvidencePath = "target/native-settings-evidence.json"
)

# This test opens synthetic Settings windows only. It never constructs the
# desktop App, attaches its command queue, or changes Explorer/autostart/hotkeys.
$ErrorActionPreference = "Stop"
$exe = (Resolve-Path -LiteralPath $TestBinary).Path
$runtimeDir = Split-Path -Parent $exe
$manifest = Get-Content -Raw -LiteralPath "third_party/winappsdk/runtime-manifest.json" | ConvertFrom-Json
$runtimeNames = @{}
foreach ($record in $manifest.files) {
    if ($record.path -notmatch "/" -and $record.path.EndsWith(".dll")) {
        $runtimeNames[$record.path] = $true
    }
}

uv run --no-project --python ">=3.11" python scripts/stage-winappsdk-runtime.py verify --directory $runtimeDir
if ($LASTEXITCODE -ne 0) { throw "The self-contained runtime did not pass provenance verification" }
$evidence = [IO.Path]::GetFullPath($EvidencePath)
$directory = Split-Path -Parent $evidence
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$stdout = "$evidence.stdout.log"
$stderr = "$evidence.stderr.log"
$process = Start-Process -FilePath $exe -ArgumentList @(
    "live_self_contained_pages_close_and_reopen_without_desktop_side_effects",
    "--ignored", "--test-threads=1", "--nocapture"
) -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
$modules = @{}
$children = @{}
$clock = [Diagnostics.Stopwatch]::StartNew()
while (-not $process.HasExited) {
    if ($clock.Elapsed.TotalSeconds -gt 45) {
        $process.Kill()
        throw "Synthetic native Settings tour timed out"
    }
    try {
        $process.Refresh()
        foreach ($module in $process.Modules) {
            if ($runtimeNames.ContainsKey($module.ModuleName) -or $module.ModuleName -match "(?i)(webview|msedge)") {
                $modules[$module.ModuleName] = $module.FileName
            }
        }
    } catch {
        if (-not $process.HasExited) { throw }
    }
    foreach ($child in Get-CimInstance Win32_Process -Filter ("ParentProcessId = " + $process.Id)) {
        $children[[string]$child.ProcessId] = @{ name = $child.Name; path = $child.ExecutablePath }
    }
    Start-Sleep -Milliseconds 100
    $process.Refresh()
}
$process.WaitForExit()
Get-Content -LiteralPath $stdout
Get-Content -LiteralPath $stderr
if ($process.ExitCode -ne 0) { throw "Native test failed: $($process.ExitCode)" }
if (-not $modules.ContainsKey("Microsoft.UI.Xaml.dll")) {
    throw "No observed WinUI Xaml module; runtime provenance check is inconclusive"
}
foreach ($entry in $modules.GetEnumerator()) {
    if ($entry.Key -match "(?i)(webview|msedge)") { throw "Unexpected browser module: $($entry.Key)" }
    if ([IO.Path]::GetDirectoryName($entry.Value) -ne $runtimeDir) {
        throw "WinUI module resolved outside the staged runtime: $($entry.Value)"
    }
}
foreach ($child in $children.Values) {
    # Rust's test EXE is a console application. A system console host is not
    # a Settings renderer or browser process. CIM may omit its executable path
    # when a short-lived console host has already retired; retain that in evidence.
    $expectedConsole = [IO.Path]::GetFullPath((Join-Path $env:WINDIR "System32/conhost.exe"))
    $wrongConsolePath = $child.path -and [IO.Path]::GetFullPath($child.path) -ne $expectedConsole
    if ($child.name -ne "conhost.exe" -or $wrongConsolePath) {
        throw "Unexpected child process from the synthetic UI test: $($child.name)"
    }
}
@{
    testBinary = $exe
    runtimeVersion = $manifest.package.version
    observedModules = $modules
    observedChildProcesses = $children
    testExitCode = $process.ExitCode
    checks = @("five pages", "13 condition forms", "ten-locale page mounting",
        "adaptive navigation pane and titlebar hamburger",
        "native ContentDialog cancellation", "component and titlebar close/reopen",
        "application exit with an active modal", "local runtime modules")
    limitations = @("sampled process/module observation", "not keyboard/UIA/Narrator/DPI acceptance")
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $evidence -Encoding utf8
Write-Output "Native Settings passed; $($modules.Count) runtime modules loaded locally, no browser child processes observed."
