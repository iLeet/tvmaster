# Installs the TVMaster daemon on Windows from the unpacked zip (the setup .exe does
# the same with a wizard). Run from an elevated PowerShell:
#   .\install.ps1                 install to Program Files, start at boot, open the firewall, get FFmpeg
#   .\install.ps1 -Https           also serve HTTPS (self-signed certificate, port 8788)
#   .\install.ps1 -NoFfmpeg -NoFirewall -Bind 127.0.0.1:8787 -DataDir D:\TVMaster
param(
  [string]$Prefix = (Join-Path $env:ProgramFiles 'TVMaster'),
  [string]$DataDir = (Join-Path $env:ProgramData 'TVMaster'),
  [string]$Bind = '0.0.0.0:8787',
  [switch]$NoService,
  [switch]$NoFirewall,
  [switch]$NoFfmpeg,
  [switch]$Https
)
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole('Administrator')) {
  throw 'Run this from an elevated PowerShell (Run as administrator).'
}
foreach ($f in 'tvmaster.exe', 'tvmaster-setup.ps1') {
  if (-not (Test-Path (Join-Path $here $f))) { throw "$f not found next to this script ($here)" }
}

# Stop an older copy so its files can be replaced.
Stop-ScheduledTask -TaskName TVMaster -ErrorAction SilentlyContinue
Get-Process -Name tvmaster -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

New-Item -ItemType Directory -Force $Prefix, $DataDir | Out-Null
Copy-Item (Join-Path $here 'tvmaster.exe'), (Join-Path $here 'tvmaster-setup.ps1') $Prefix -Force
Write-Host "Installed to $Prefix"

$setup = Join-Path $Prefix 'tvmaster-setup.ps1'
if (-not $NoFfmpeg)   { & $setup -Action Ffmpeg   -InstallDir $Prefix -DataDir $DataDir -Bind $Bind }
if (-not $NoFirewall) { & $setup -Action Firewall -InstallDir $Prefix -DataDir $DataDir -Bind $Bind }
if (-not $NoService)  {
  & $setup -Action Service -InstallDir $Prefix -DataDir $DataDir -Bind $Bind -Https:$Https
  Write-Host "Open http://localhost:$($Bind.Split(':')[-1])/admin/  (the access token is in $DataDir\token)"
} else {
  Write-Host "Run it with:  & '$Prefix\tvmaster.exe' serve --data-dir '$DataDir'"
}
