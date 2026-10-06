# Offline per-user install. No download, app launch, registry write or automatic cleanup.
[CmdletBinding(SupportsShouldProcess)]
param(
  [Parameter(Mandatory = $true)][string]$Archive,
  [string]$ChecksumFile = "",
  [string]$Destination = "",
  [string]$Python = "python",
  [switch]$CreateShortcut
)
$ErrorActionPreference = "Stop"
$Archive = (Resolve-Path -LiteralPath $Archive).Path
if (!$ChecksumFile) { $ChecksumFile = "$Archive.sha256" }
$checksum = Get-Content -LiteralPath $ChecksumFile -Raw
if ($checksum -notmatch '\A([0-9A-Fa-f]{64})\s+([^\r\n]+)\s*\z') {
  throw "Expected a SHA-256 sidecar containing exactly one archive hash and filename."
}
$hash = $Matches[1]
if ($Matches[2].Trim() -cne (Split-Path $Archive -Leaf)) { throw "Checksum filename does not match archive." }
if (!$Destination) { $Destination = Join-Path $env:LOCALAPPDATA "Tinnci/PecoFence/versions" }
$Destination = [IO.Path]::GetFullPath($Destination)
$helper = Join-Path $PSScriptRoot "package_desktop.py"
# Validation precedes filesystem writes, including WhatIf.
$planned = & $Python $helper install --archive $Archive --sha256 $hash --destination $Destination --dry-run
if ($LASTEXITCODE -ne 0) { throw "Package verification failed. Nothing was installed." }
if (!$PSCmdlet.ShouldProcess($planned, "Install verified immutable PecoFence payload")) { return }
$installed = & $Python $helper install --archive $Archive --sha256 $hash --destination $Destination
if ($LASTEXITCODE -ne 0) { throw "Installation failed." }
Write-Host "Installed: $installed"
Write-Host "Run: $(Join-Path $installed 'pecofence.exe')"
Write-Host "SHA-256 proves integrity, not publisher identity. Install only from a trusted source."
if ($CreateShortcut) {
  $shortcutPath = Join-Path ([Environment]::GetFolderPath("Programs")) "PecoFence (Tinnci).lnk"
  if ($PSCmdlet.ShouldProcess($shortcutPath, "Create/update this installation's Start Menu shortcut")) {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutPath)
    $marker = "Tinnci/PecoFence per-user installation"
    if ((Test-Path -LiteralPath $shortcutPath) -and
        ($shortcut.Description -cne $marker -or !$shortcut.TargetPath.StartsWith($Destination + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase))) {
      throw "Refusing to overwrite a shortcut owned by another installation."
    }
    $shortcut.TargetPath = Join-Path $installed "pecofence.exe"
    $shortcut.WorkingDirectory = $installed
    $shortcut.Description = $marker
    $shortcut.Save()
  }
}
