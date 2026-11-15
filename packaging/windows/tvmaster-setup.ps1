# Setup helper used by the Windows installer (and by install.ps1 in the zip).
# Run from an elevated PowerShell:
#   .\tvmaster-setup.ps1 -Action Service  -InstallDir 'C:\Program Files\TVMaster' -DataDir 'C:\ProgramData\TVMaster'
#   .\tvmaster-setup.ps1 -Action Ffmpeg   -InstallDir 'C:\Program Files\TVMaster'
#   .\tvmaster-setup.ps1 -Action Firewall -InstallDir 'C:\Program Files\TVMaster'
#   .\tvmaster-setup.ps1 -Action Remove   -InstallDir 'C:\Program Files\TVMaster'
# Exit code 0 = done, 1 = failed (the message says why).
param(
  [Parameter(Mandatory)][ValidateSet('Service', 'Ffmpeg', 'Firewall', 'Remove')][string]$Action,
  [Parameter(Mandatory)][string]$InstallDir,
  [string]$DataDir = (Join-Path $env:ProgramData 'TVMaster'),
  [string]$Bind = '0.0.0.0:8787'
)
$ErrorActionPreference = 'Stop'
$TaskName = 'TVMaster'
$RuleName = 'TVMaster'
$Exe = Join-Path $InstallDir 'tvmaster.exe'

function Install-Service {
  New-Item -ItemType Directory -Force $DataDir | Out-Null
  # An access token, made once, readable by Administrators and SYSTEM only. The
  # server refuses to run without one when it listens on the network.
  $tokenFile = Join-Path $DataDir 'token'
  if (-not (Test-Path $tokenFile) -or (Get-Item $tokenFile).Length -eq 0) {
    $token = (& $Exe token | Select-Object -First 1).Trim()
    if (-not $token) { throw 'could not make an access token' }
    Set-Content -Path $tokenFile -Value $token -Encoding ascii -NoNewline
  }
  & icacls $tokenFile /inheritance:r /grant:r 'Administrators:F' 'SYSTEM:F' | Out-Null

  $serveArgs = "serve --bind $Bind --data-dir `"$DataDir`" --token-file `"$tokenFile`""
  $action = New-ScheduledTaskAction -Execute $Exe -Argument $serveArgs -WorkingDirectory $InstallDir
  $trigger = New-ScheduledTaskTrigger -AtStartup
  $principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
  # Start at boot, restart a minute after any failure, never time out.
  $settings = New-ScheduledTaskSettingsSet -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) `
    -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -MultipleInstances IgnoreNew `
    -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
  if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
    Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
  }
  Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Principal $principal `
    -Settings $settings -Description 'TVMaster retro TV station server' | Out-Null
  Start-ScheduledTask -TaskName $TaskName
  Write-Host "TVMaster is running as the scheduled task '$TaskName'. Access token: $tokenFile"
}

function Install-Ffmpeg {
  $bin = Join-Path $InstallDir 'ffmpeg\bin'
  if ((Test-Path (Join-Path $bin 'ffmpeg.exe')) -and (Test-Path (Join-Path $bin 'ffprobe.exe'))) {
    Write-Host 'FFmpeg is already installed beside TVMaster.'; return
  }
  if ((Get-Command ffmpeg -ErrorAction SilentlyContinue) -and (Get-Command ffprobe -ErrorAction SilentlyContinue)) {
    Write-Host 'FFmpeg is already on this PC.'; return
  }
  # A GPL build is fine here: TVMaster runs it as a separate program.
  $arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'winarm64' } else { 'win64' }
  $url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-$arch-gpl.zip"
  $zip = Join-Path $env:TEMP 'tvmaster-ffmpeg.zip'
  $tmp = Join-Path $env:TEMP 'tvmaster-ffmpeg'
  [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
  $ProgressPreference = 'SilentlyContinue'   # the progress bar makes Invoke-WebRequest very slow
  Write-Host "Downloading FFmpeg ($arch)..."
  Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
  if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
  Expand-Archive -Path $zip -DestinationPath $tmp -Force
  $inner = Get-ChildItem $tmp -Directory | Select-Object -First 1
  if (-not $inner -or -not (Test-Path (Join-Path $inner.FullName 'bin\ffmpeg.exe'))) { throw 'the download did not contain ffmpeg.exe' }
  $dest = Join-Path $InstallDir 'ffmpeg'
  if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
  Move-Item -Path $inner.FullName -Destination $dest
  Remove-Item -Force $zip; Remove-Item -Recurse -Force $tmp
  $v = & (Join-Path $dest 'bin\ffmpeg.exe') -version | Select-Object -First 1
  Write-Host "Installed: $v"
}

function Install-Firewall {
  # Home and work networks only, never public ones.
  Get-NetFirewallRule -DisplayName $RuleName -ErrorAction SilentlyContinue | Remove-NetFirewallRule
  New-NetFirewallRule -DisplayName $RuleName -Direction Inbound -Action Allow -Program $Exe `
    -Protocol TCP -Profile Private,Domain | Out-Null
  Write-Host 'Allowed TVMaster on private networks.'
}

function Remove-All {
  if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
    Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
  }
  Get-Process -Name tvmaster -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Get-NetFirewallRule -DisplayName $RuleName -ErrorAction SilentlyContinue | Remove-NetFirewallRule
}

try {
  switch ($Action) {
    'Service'  { Install-Service }
    'Ffmpeg'   { Install-Ffmpeg }
    'Firewall' { Install-Firewall }
    'Remove'   { Remove-All }
  }
  exit 0
} catch {
  Write-Error $_.Exception.Message
  exit 1
}
