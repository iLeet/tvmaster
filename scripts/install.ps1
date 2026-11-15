# Installs the TVMaster daemon on Windows, from an unpacked release.
#   .\install.ps1                       install to %LOCALAPPDATA%\TVMaster
#   .\install.ps1 -Service              also start it at boot (needs an elevated PowerShell)
#   .\install.ps1 -Service -Bind 0.0.0.0:8787 -DataDir D:\TVMaster
param(
  [string]$Prefix = "$env:LOCALAPPDATA\TVMaster",
  [string]$DataDir = "$env:ProgramData\TVMaster",
  [string]$Bind = "0.0.0.0:8787",
  [switch]$Service,
  [switch]$DryRun
)
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path $here "tvmaster.exe"
if (-not (Test-Path $src)) { throw "tvmaster.exe not found next to this script ($here)" }

function Run($what, [scriptblock]$do) { if ($DryRun) { Write-Host "+ $what" } else { & $do } }

if (-not ((Get-Command ffmpeg -ErrorAction SilentlyContinue) -and (Get-Command ffprobe -ErrorAction SilentlyContinue))) {
  Write-Warning "ffmpeg and ffprobe were not found on your PATH. TVMaster needs them to stream. Install with: winget install Gyan.FFmpeg"
}

$bin = Join-Path $Prefix "tvmaster.exe"
Run "create $Prefix and $DataDir" { New-Item -ItemType Directory -Force $Prefix, $DataDir | Out-Null }
Run "copy tvmaster.exe to $bin" { Copy-Item $src $bin -Force }
Write-Host "Installed $bin"

if ($Service) {
  $tokenFile = Join-Path $DataDir "token"
  if (-not (Test-Path $tokenFile)) {
    Run "write a new token to $tokenFile (readable by Administrators and SYSTEM only)" {
      & $bin token | Set-Content -Path $tokenFile -Encoding ascii
      icacls $tokenFile /inheritance:r /grant:r "Administrators:F" "SYSTEM:F" | Out-Null
    }
  }
  # The daemon prints the schtasks command that registers the startup task.
  $cmd = & $bin service windows --bin $bin --data-dir $DataDir --bind $Bind --token-file $tokenFile
  Run "register the startup task" { cmd /c ($cmd -join " ") }
  Run "start the task now" { schtasks /Run /TN "TVMaster" | Out-Null }
  Write-Host "Started. Open http://localhost:$($Bind.Split(':')[-1])/admin/"
  Write-Host "The access token is in $tokenFile"
} else {
  Write-Host "Run it with:  & '$bin' serve --data-dir '$DataDir'"
}
