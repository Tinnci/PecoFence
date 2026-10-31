# Per-user install of Tinnci Workspace with Start menu and desktop shortcuts.
# Copies the executable into versions\<version>-<sha256 prefix>; earlier versions stay.
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$Executable,
  [Parameter(Mandatory)][string]$Version,
  [string]$InstallRoot = "$env:LOCALAPPDATA\Tinnci\Workspace",
  [switch]$NoDesktop,
  [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Invalid version: $Version" }
$Executable = (Resolve-Path -LiteralPath $Executable).ProviderPath
$hash = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash.ToLower()
$directory = Join-Path $InstallRoot "versions\$Version-$($hash.Substring(0, 12))"
$target = Join-Path $directory 'tinnci-workspace.exe'
$programs = Join-Path ([Environment]::GetFolderPath('Programs')) 'Tinnci'
$shortcuts = @(
  @{ Path = Join-Path $programs '工作台.lnk'; Arguments = '' },
  @{ Path = Join-Path $programs '工作台 · 设置清单.lnk'; Arguments = '--open setup' },
  @{ Path = Join-Path $programs '工作台 · 运行状况.lnk'; Arguments = '--open health' }
)
if (-not $NoDesktop) {
  $shortcuts += @{ Path = Join-Path ([Environment]::GetFolderPath('Desktop')) '工作台.lnk'; Arguments = '' }
}
if ($DryRun) {
  [ordered]@{ target = $target; sha256 = $hash; shortcuts = $shortcuts.Path } | ConvertTo-Json
  return
}
if (Test-Path -LiteralPath $target) {
  if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLower() -ne $hash) {
    throw "Existing $target differs from $Executable"
  }
} else {
  New-Item -ItemType Directory -Force -Path $directory | Out-Null
  Copy-Item -LiteralPath $Executable -Destination "$target.partial"
  if ((Get-FileHash -LiteralPath "$target.partial" -Algorithm SHA256).Hash.ToLower() -ne $hash) {
    Remove-Item -LiteralPath "$target.partial"
    throw 'Copied executable checksum mismatch'
  }
  Move-Item -LiteralPath "$target.partial" -Destination $target
}
New-Item -ItemType Directory -Force -Path $programs | Out-Null
$shell = New-Object -ComObject WScript.Shell
foreach ($entry in $shortcuts) {
  $link = $shell.CreateShortcut($entry.Path)
  $link.TargetPath = $target
  $link.Arguments = $entry.Arguments
  $link.WorkingDirectory = $directory
  $link.IconLocation = "$target,0"
  $link.Description = 'Tinnci Workspace · 本机工具与 SPM 项目'
  $link.Save()
}
[ordered]@{ target = $target; sha256 = $hash; shortcuts = $shortcuts.Path } | ConvertTo-Json
