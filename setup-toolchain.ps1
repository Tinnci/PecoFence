<#
.SYNOPSIS
    Configures the Rust MSVC toolchain and build prerequisites for PecoFence on Windows.

.DESCRIPTION
    This script inspects and installs the required toolchain components:
    1. Visual Studio 2022 C++ Build Tools (MSVC compiler, Windows 10/11 SDK)
    2. Rustup & Rust stable (x86_64-pc-windows-msvc)
    3. Toolchain components (rustfmt, clippy)
    4. Python >=3.11 via uv (used by project validation and packaging scripts)
#>

[CmdletBinding()]
param(
    [switch]$SkipBuildTools = $false,
    [switch]$NonInteractive = $false
)

$ErrorActionPreference = "Stop"
Write-Host "====================================================" -ForegroundColor Cyan
Write-Host "       PecoFence Windows Toolchain Setup            " -ForegroundColor Cyan
Write-Host "====================================================" -ForegroundColor Cyan

# 1. Check Administrator Privileges
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Warning "This script is not running as Administrator. Installing VS Build Tools via winget may require Administrator privileges."
}

# 2. Check for Visual Studio C++ Build Tools (MSVC & Windows SDK)
Write-Host "`n[1/4] Checking Visual Studio C++ Build Tools..." -ForegroundColor Yellow
$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$hasVCTools = $false

if (Test-Path $vsWhere) {
    $vsInstall = & $vsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($vsInstall) {
        Write-Host "  [OK] Found Visual Studio C++ Build Tools at: $vsInstall" -ForegroundColor Green
        $hasVCTools = $true
    }
}

if (-not $hasVCTools -and -not $SkipBuildTools) {
    Write-Host "  Visual Studio C++ Build Tools not detected." -ForegroundColor Yellow
    Write-Host "  Installing via winget (Microsoft.VisualStudio.2022.BuildTools)..." -ForegroundColor Cyan
    try {
        winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --silent --override "--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
        Write-Host "  [OK] Visual Studio Build Tools installation initiated/completed." -ForegroundColor Green
    } catch {
        Write-Warning "  Automatic winget installation encountered an issue. You can install it manually from:"
        Write-Host "  https://aka.ms/vs/17/release/vs_BuildTools.exe" -ForegroundColor White
        Write-Host "  Make sure to check 'Desktop development with C++'." -ForegroundColor White
    }
} elseif ($SkipBuildTools) {
    Write-Host "  Skipping Visual Studio Build Tools check/installation as requested." -ForegroundColor Gray
}

# 3. Check and Configure Rust (MSVC Toolchain)
Write-Host "`n[2/4] Checking Rust Toolchain (rustup, rustc, cargo)..." -ForegroundColor Yellow

# Ensure .cargo\bin is in PATH for the current session
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if ((Test-Path $cargoBin) -and ($env:PATH -notlike "*$cargoBin*")) {
    $env:PATH = "$cargoBin;$env:PATH"
}

if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
    Write-Host "  rustup not found in PATH. Checking winget..." -ForegroundColor Yellow
    try {
        Write-Host "  Installing Rustup via winget..." -ForegroundColor Cyan
        winget install --id Rustlang.Rustup --exact --silent --accept-package-agreements --accept-source-agreements
        if (Test-Path $cargoBin) {
            $env:PATH = "$cargoBin;$env:PATH"
        }
    } catch {
        Write-Host "  Downloading rustup-init.exe directly..." -ForegroundColor Cyan
        $rustupInit = Join-Path $env:TEMP "rustup-init.exe"
        Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $rustupInit
        Write-Host "  Running rustup-init for x86_64-pc-windows-msvc..." -ForegroundColor Cyan
        & $rustupInit -y --default-host x86_64-pc-windows-msvc --default-toolchain stable --profile default
        if (Test-Path $cargoBin) {
            $env:PATH = "$cargoBin;$env:PATH"
        }
    }
}

if (Get-Command rustup -ErrorAction SilentlyContinue) {
    Write-Host "  Configuring rustup for PecoFence..." -ForegroundColor Cyan
    
    # Selecting an already-installed stable toolchain does not update it.
    & rustup update stable-x86_64-pc-windows-msvc --no-self-update
    if ($LASTEXITCODE -ne 0) { throw "Rust stable update failed." }

    # Set default host and toolchain to stable MSVC
    & rustup default stable-x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw "Rust stable selection failed." }
    
    # Add required target
    & rustup target add x86_64-pc-windows-msvc --toolchain stable-x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw "Rust MSVC target installation failed." }
    
    # Add required components (rustfmt, clippy) specified in rust-toolchain.toml
    & rustup component add rustfmt clippy --toolchain stable-x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw "Rust component installation failed." }
    
    $rustcVer = & rustc --version
    $cargoVer = & cargo --version
    Write-Host "  [OK] $rustcVer" -ForegroundColor Green
    Write-Host "  [OK] $cargoVer" -ForegroundColor Green
} else {
    Write-Error "Rustup could not be located in PATH. Please restart PowerShell after installation."
}

# 4. Resolve a supported Python without replacing the system interpreter.
# Source packaging uses tomllib (added in Python 3.11); newer versions are welcome.
Write-Host "`n[3/4] Checking Python >=3.11 via uv..." -ForegroundColor Yellow
$uvCommand = Get-Command uv -ErrorAction SilentlyContinue
$uvPrefix = @()
if (-not $uvCommand) {
    $bootstrapPython = Get-Command python -ErrorAction SilentlyContinue
    if (-not $bootstrapPython) {
        $bootstrapPython = Get-Command py -ErrorAction SilentlyContinue
    }
    if (-not $bootstrapPython) {
        throw "Install uv first (https://docs.astral.sh/uv/getting-started/installation/), then rerun this script."
    }
    Write-Host "  Installing uv from PyPI..." -ForegroundColor Cyan
    & $bootstrapPython.Source -m pip install --user --upgrade uv
    if ($LASTEXITCODE -ne 0) { throw "uv installation from PyPI failed." }
    # Module invocation works even when pip's Scripts directory is not on PATH.
    $uvCommand = $bootstrapPython
    $uvPrefix = @("-m", "uv")
}
& $uvCommand.Source @uvPrefix run --no-project --python ">=3.11" python --version
if ($LASTEXITCODE -ne 0) { throw "Python >=3.11 setup failed." }
Write-Host "  [OK] Use uv run --no-project --python '>=3.11' python scripts/<script>.py" -ForegroundColor Green

# 5. Summary and Build Instructions
Write-Host "`n[4/4] Toolchain Configuration Complete!" -ForegroundColor Green
Write-Host "====================================================" -ForegroundColor Cyan
Write-Host "To build and run PecoFence in PowerShell:" -ForegroundColor White
Write-Host "  cd C:\Users\Administrator\PecoFence" -ForegroundColor Yellow
Write-Host "  cargo build --locked" -ForegroundColor Yellow
Write-Host "  uv run --no-project --python '>=3.11' python scripts\stage-winappsdk-runtime.py stage --target-dir target --profile debug" -ForegroundColor Yellow
Write-Host "  .\target\debug\pecofence.exe" -ForegroundColor Yellow
Write-Host "`nTo create a portable release package:" -ForegroundColor White
Write-Host "  powershell -ExecutionPolicy Bypass -File scripts\make-portable.ps1" -ForegroundColor Yellow
Write-Host "====================================================" -ForegroundColor Cyan
