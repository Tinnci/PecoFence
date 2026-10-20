# Background startup entry point: retain the existing database and capture logs without a console.
param(
  [Parameter(Mandatory = $true)][string]$Executable,
  [Parameter(Mandatory = $true)][string]$Database,
  [Parameter(Mandatory = $true)][string]$LogDirectory,
  [Parameter(Mandatory = $true)][string]$Sha256
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) { throw 'SPM executable missing' }
if (-not (Test-Path -LiteralPath $Database -PathType Leaf)) { throw 'Existing SPM database missing; refusing to create a replacement' }
if ((Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash -ne $Sha256) { throw 'SPM executable checksum mismatch' }
New-Item -ItemType Directory -Force -Path $LogDirectory | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$daemon = Start-Process -FilePath $Executable -ArgumentList @('--db', ('"' + $Database + '"')) `
  -WorkingDirectory (Split-Path $Executable) -WindowStyle Hidden -Wait -PassThru `
  -RedirectStandardOutput (Join-Path $LogDirectory ('spmd-' + $stamp + '.out.log')) `
  -RedirectStandardError (Join-Path $LogDirectory ('spmd-' + $stamp + '.err.log'))
exit $daemon.ExitCode
