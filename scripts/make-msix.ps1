# Build the Microsoft Store MSIX package from the same binaries as the portable ZIP.
#
#   ./scripts/make-msix.ps1                 # unsigned .msix for Partner Center upload
#   ./scripts/make-msix.ps1 -TestSign       # also a self-signed copy for local install tests
#   ./scripts/make-msix.ps1 -SkipBuild      # reuse target/package/release from make-portable.ps1
#
# Identity values come from packaging/msix/identity.json (Partner Center > Product identity).
# Requires the Windows 10/11 SDK (makeappx, makepri, signtool) and Python with Pillow.
param(
  [string]$Version = "",
  [string]$TargetDir = "target/package",
  [string]$Python = "python",
  [string]$Identity = "packaging/msix/identity.json",
  [switch]$SkipBuild,
  [switch]$TestSign
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# Validate publishing ownership before locating SDK tools or compiling anything.
$id = Get-Content -LiteralPath $Identity -Raw | ConvertFrom-Json
foreach ($field in "identityName", "publisher", "publisherDisplayName") {
  if ([string]::IsNullOrWhiteSpace([string]$id.$field)) {
    throw "$Identity is not configured: set '$field' from your own Partner Center product identity. See docs/STORE.md."
  }
}

function Find-SdkTool([string]$name) {
  $cmd = Get-Command $name -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
  $versions = Get-ChildItem -Path $kits -Directory -Filter "10.*" -ErrorAction SilentlyContinue |
    Sort-Object { [version]$_.Name } -Descending
  foreach ($v in $versions) {
    $candidate = Join-Path $v.FullName "x64\$name"
    if (Test-Path -LiteralPath $candidate) { return $candidate }
  }
  throw "$name not found. Install the Windows SDK (winget install Microsoft.WindowsSDK.10.0.26100)."
}
$makeappx = Find-SdkTool "makeappx.exe"
$makepri = Find-SdkTool "makepri.exe"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  $env:PATH = (Join-Path $env:USERPROFILE ".cargo\bin") + ";" + $env:PATH
}
if (-not $SkipBuild) {
  & (Join-Path $PSScriptRoot "build-desktop.ps1") -TargetDir $TargetDir -Python $Python
}

if (-not $Version) {
  $Version = (Select-String -Path "Cargo.toml" -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
}
if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)$') { throw "Store MSIX requires a stable workspace version: $Version" }
foreach ($part in $Matches[1], $Matches[2], $Matches[3]) {
  if ([long]$part -gt 65535) { throw "MSIX version components cannot exceed 65535." }
}
# MSIX needs four parts; the Store requires the revision (last part) to be 0.
$packageVersion = "$($Matches[1]).$($Matches[2]).$($Matches[3]).0"

$distRoot = [IO.Path]::GetFullPath((Join-Path $root "dist"))
$stage = [IO.Path]::GetFullPath((Join-Path $distRoot (".msix-" + [guid]::NewGuid().ToString("N"))))
if (-not $stage.StartsWith($distRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsafe package destination" }
$priConfig = Join-Path $distRoot (".priconfig-" + [guid]::NewGuid().ToString("N") + ".xml")
try {
# Same verified PE-aware payload implementation as the portable ZIP.
& $Python scripts/package_desktop.py stage-msix --release (Join-Path $TargetDir "release") --version $Version --out $stage
if ($LASTEXITCODE -ne 0) { throw "Verified MSIX payload generation failed" }

# Tile and Store logos drawn directly by the standalone asset generator.
& $Python scripts/make-msix-assets.py (Join-Path $stage "Assets")
if ($LASTEXITCODE -ne 0) { throw "Asset generation failed" }

# Manifest with the Partner Center identity filled in.
$manifest = Get-Content -LiteralPath "packaging/msix/AppxManifest.xml" -Raw
$manifest = $manifest.Replace("__IDENTITY_NAME__", [Security.SecurityElement]::Escape([string]$id.identityName))
$manifest = $manifest.Replace("__IDENTITY_PUBLISHER__", [Security.SecurityElement]::Escape([string]$id.publisher))
$manifest = $manifest.Replace("__PUBLISHER_DISPLAY_NAME__", [Security.SecurityElement]::Escape([string]$id.publisherDisplayName))
$manifest = $manifest.Replace("__VERSION__", $packageVersion)
if ($manifest -match "__[A-Z_]+__") { throw "Unreplaced manifest token: $($Matches[0])" }
$manifestPath = Join-Path $stage "AppxManifest.xml"
[IO.File]::WriteAllText($manifestPath, $manifest, (New-Object Text.UTF8Encoding $false))

# Resource index so scale-qualified assets resolve.
& $makepri createconfig /cf $priConfig /dq en-US /pv 10.0.0 /o
if ($LASTEXITCODE -ne 0) { throw "makepri createconfig failed" }
& $makepri new /pr $stage /cf $priConfig /of (Join-Path $stage "resources.pri") /mn $manifestPath /o
if ($LASTEXITCODE -ne 0) { throw "makepri new failed" }
Remove-Item -LiteralPath $priConfig

$msix = Join-Path $distRoot "pecofence-$Version-x64.msix"
if (Test-Path -LiteralPath $msix) { Remove-Item -LiteralPath $msix }
& $makeappx pack /d $stage /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "makeappx pack failed" }
$hash = (Get-FileHash $msix -Algorithm SHA256).Hash
"$hash  $(Split-Path $msix -Leaf)" | Set-Content "$msix.sha256" -Encoding ascii
Write-Output "packed $msix (unsigned; the Store signs it after certification)"
Write-Output "SHA256 $hash"

if ($TestSign) {
  # Self-signed certificate whose subject equals the package Publisher, for local
  # Add-AppxPackage tests only. Install the .cer into Trusted People first.
  $signtool = Find-SdkTool "signtool.exe"
  $cert = $null
  try {
    $cert = New-SelfSignedCertificate -Type Custom -Subject ([string]$id.publisher) `
      -KeyUsage DigitalSignature -KeyExportPolicy NonExportable -FriendlyName "PecoFence MSIX test signing" `
      -CertStoreLocation "Cert:\CurrentUser\My" `
      -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
    # Export only the public certificate. Never write a private key/PFX into artifacts.
    $cer = Join-Path $distRoot "pecofence-test-signing.cer"
    Export-Certificate -Cert $cert -FilePath $cer | Out-Null
    $signed = $msix -replace '\.msix$', '-testsigned.msix'
    Copy-Item -LiteralPath $msix -Destination $signed -Force
    & $signtool sign /fd SHA256 /s My /sha1 $cert.Thumbprint $signed
    if ($LASTEXITCODE -ne 0) { throw "signtool failed" }
    $signedHash = (Get-FileHash $signed -Algorithm SHA256).Hash
    "$signedHash  $(Split-Path $signed -Leaf)" | Set-Content "$signed.sha256" -Encoding ascii
    Write-Output "test-signed $signed"
    Write-Output "The .cer is public and for local tests only; neither it nor the test-signed MSIX is a production release."
  } finally {
    if ($cert) { Remove-Item -LiteralPath ("Cert:\CurrentUser\My\" + $cert.Thumbprint) -DeleteKey -Force }
  }
}
} finally {
  if (Test-Path -LiteralPath $priConfig) { Remove-Item -LiteralPath $priConfig -Force }
  if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
}
