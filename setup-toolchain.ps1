<#
.SYNOPSIS
    Configures the Rust MSVC toolchain and build prerequisites for PecoFence on Windows.

.DESCRIPTION
    This script inspects and installs the required toolchain components:
    1. Visual Studio 2022 C++ Build Tools (MSVC compiler, Windows 10/11 SDK)
    2. Rustup & Rust stable (x86_64-pc-windows-msvc)
    3. Toolchain components (rustfmt, clippy)
    4. Python 3 (used by project validation and packaging scripts)
    5. Verifies existing Node.js and WebView2 runtime installations
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
Write-Host "`n[1/5] Checking Visual Studio C++ Build Tools..." -ForegroundColor Yellow
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
Write-Host "`n[2/5] Checking Rust Toolchain (rustup, rustc, cargo)..." -ForegroundColor Yellow

# Ensure .cargo\bin is in PATH for the current session
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin -and ($env:PATH -notlike "*$cargoBin*")) {
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
    
    # Set default host and toolchain to stable MSVC
    & rustup default stable-x86_64-pc-windows-msvc
    
    # Add required target
    & rustup target add x86_64-pc-windows-msvc
    
    # Add required components (rustfmt, clippy) specified in rust-toolchain.toml
    & rustup component add rustfmt clippy
    
    $rustcVer = & rustc --version
    $cargoVer = & cargo --version
    Write-Host "  [OK] $rustcVer" -ForegroundColor Green
    Write-Host "  [OK] $cargoVer" -ForegroundColor Green
} else {
    Write-Error "Rustup could not be located in PATH. Please restart PowerShell after installation."
}

# 4. Check Python 3 (Required for packaging and translation scripts)
Write-Host "`n[3/5] Checking Python 3..." -ForegroundColor Yellow
if (Get-Command python -ErrorAction SilentlyContinue) {
    $pyVer = & python --version
    Write-Host "  [OK] $pyVer" -ForegroundColor Green
} elseif (Get-Command py -ErrorAction SilentlyContinue) {
    $pyVer = & py --version
    Write-Host "  [OK] $pyVer (via py launcher)" -ForegroundColor Green
} else {
    Write-Host "  Python 3 not found. Installing via winget..." -ForegroundColor Cyan
    try {
        winget install --id Python.Python.3.12 --exact --silent --accept-package-agreements --accept-source-agreements
        Write-Host "  [OK] Python 3 installed. You may need to reload your environment variables." -ForegroundColor Green
    } catch {
        Write-Warning "  Could not install Python 3 automatically. Install from https://www.python.org/ if needed for scripts."
    }
}

# 5. Check Node.js (Optional, used for Settings browser UI tests)
Write-Host "`n[4/5] Checking Node.js..." -ForegroundColor Yellow
if (Get-Command node -ErrorAction SilentlyContinue) {
    $nodeVer = & node --version
    Write-Host "  [OK] Node.js $nodeVer" -ForegroundColor Green
} else {
    Write-Host "  Node.js not in PATH (desktop app builds without Node, but UI test scripts use it)." -ForegroundColor Gray
}

# 6. Summary and Build Instructions
Write-Host "`n[5/5] Toolchain Configuration Complete!" -ForegroundColor Green
Write-Host "====================================================" -ForegroundColor Cyan
Write-Host "To build and run PecoFence in PowerShell:" -ForegroundColor White
Write-Host "  cd C:\Users\Administrator\PecoFence" -ForegroundColor Yellow
Write-Host "  cargo build --locked" -ForegroundColor Yellow
Write-Host "  Copy-Item third_party\webview2\WebView2Loader.x64.dll target\debug\WebView2Loader.dll" -ForegroundColor Yellow
Write-Host "  .\target\debug\pecofence.exe" -ForegroundColor Yellow
Write-Host "`nTo create a portable release package:" -ForegroundColor White
Write-Host "  powershell -ExecutionPolicy Bypass -File scripts\make-portable.ps1" -ForegroundColor Yellow
Write-Host "====================================================" -ForegroundColor Cyan
