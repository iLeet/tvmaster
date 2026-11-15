; TVMaster installer for Windows (NSIS).
;   makensis -DVERSION=0.0.1 -DARCH=x64 -DEXE=path\to\tvmaster.exe -DOUT=TVMaster-0.0.1-x64-setup.exe tvmaster.nsi
; Silent install with every default:  TVMaster-x.y.z-x64-setup.exe /S
Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef ARCH
  !define ARCH "x64"
!endif
!ifndef OUT
  !define OUT "TVMaster-${VERSION}-${ARCH}-setup.exe"
!endif

Name "TVMaster"
OutFile "${OUT}"
InstallDir "$PROGRAMFILES64\TVMaster"
InstallDirRegKey HKLM "Software\TVMaster" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
BrandingText "TVMaster ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "TVMaster"
VIAddVersionKey "FileDescription" "TVMaster installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "iLeet"

; With SetShellVarContext all (set in .onInit) $APPDATA is C:\ProgramData, which is where the service keeps its data.
!define DATA_DIR "$APPDATA\TVMaster"
!define PORT "8787"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\TVMaster"

!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TEXT "This sets up TVMaster, your own retro TV station.$\r$\n$\r$\nIt installs the server, starts it in the background, and gets FFmpeg if you do not have it. When it is done you manage everything from your web browser."
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Open TVMaster in my browser"
!define MUI_FINISHPAGE_RUN_FUNCTION OpenAdmin
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Show my access token (Notepad)"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION ShowToken
!define MUI_FINISHPAGE_NOREBOOTSUPPORT

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; Everything is per machine: the 64-bit registry, the 64-bit System32 (so the
; 64-bit PowerShell runs, not the 32-bit one this installer itself is), and
; the all-users Start menu and ProgramData.
Function .onInit
  SetShellVarContext all
  ${If} ${RunningX64}
    SetRegView 64
    ${DisableX64FSRedirection}
  ${EndIf}
FunctionEnd

Function un.onInit
  SetShellVarContext all
  ${If} ${RunningX64}
    SetRegView 64
    ${DisableX64FSRedirection}
  ${EndIf}
FunctionEnd

Function OpenAdmin
  ExecShell "open" "http://localhost:${PORT}/admin/"
FunctionEnd

Function ShowToken
  Exec 'notepad.exe "${DATA_DIR}\token"'
FunctionEnd

; Runs one action of the setup script and shows what it said.
!macro Setup ACTION
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\tvmaster-setup.ps1" -Action ${ACTION} -InstallDir "$INSTDIR" -DataDir "${DATA_DIR}"'
  Pop $0
!macroend

Section "TVMaster server" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  ; Stop an older copy so its files can be replaced.
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -Command "Stop-ScheduledTask -TaskName TVMaster -ErrorAction SilentlyContinue; Get-Process tvmaster -ErrorAction SilentlyContinue | Stop-Process -Force"'
  File "${EXE}"
  File "tvmaster-setup.ps1"
  File "..\..\README.md"
  SetOutPath "$INSTDIR\docs"
  File /r "..\..\docs\*.md"
  SetOutPath "$INSTDIR"

  WriteRegStr HKLM "Software\TVMaster" "InstallDir" "$INSTDIR"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "TVMaster"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "Publisher" "iLeet"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\tvmaster.exe"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKLM "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${UNINST_KEY}" "EstimatedSize" "$0"

  ; Start menu: a shortcut that opens the admin page, and one for the token.
  CreateDirectory "$SMPROGRAMS\TVMaster"
  WriteINIStr "$SMPROGRAMS\TVMaster\TVMaster.url" "InternetShortcut" "URL" "http://localhost:${PORT}/admin/"
  CreateShortcut "$SMPROGRAMS\TVMaster\My access token.lnk" "notepad.exe" '"${DATA_DIR}\token"'
  CreateShortcut "$SMPROGRAMS\TVMaster\Uninstall TVMaster.lnk" "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Run TVMaster in the background and start it with Windows" SecService
  !insertmacro Setup "Service"
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "TVMaster was installed, but starting it as a background task failed. See the installer log, or run:$\r$\n  tvmaster.exe serve --data-dir $\"${DATA_DIR}$\""
  ${EndIf}
SectionEnd

Section "Let phones and TVs on my home network connect (Windows Firewall)" SecFirewall
  !insertmacro Setup "Firewall"
SectionEnd

Section "Download FFmpeg (TVMaster needs it to stream)" SecFfmpeg
  !insertmacro Setup "Ffmpeg"
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "TVMaster was installed, but FFmpeg could not be downloaded. Install it yourself (winget install Gyan.FFmpeg) and restart TVMaster."
  ${EndIf}
SectionEnd

LangString DESC_Main ${LANG_ENGLISH} "The TVMaster server and its documentation."
LangString DESC_Service ${LANG_ENGLISH} "Registers a Windows task that starts TVMaster when the PC starts and restarts it if it stops. Protected by an access token."
LangString DESC_Firewall ${LANG_ENGLISH} "Lets other devices on private (home) networks reach TVMaster. Public networks stay blocked."
LangString DESC_Ffmpeg ${LANG_ENGLISH} "Downloads FFmpeg (about 140 MB) next to TVMaster unless you already have it. Skip this if you installed FFmpeg yourself."
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} $(DESC_Main)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecService} $(DESC_Service)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecFirewall} $(DESC_Firewall)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecFfmpeg} $(DESC_Ffmpeg)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\tvmaster-setup.ps1" -Action Remove -InstallDir "$INSTDIR"'
  Pop $0
  RMDir /r "$INSTDIR\docs"
  RMDir /r "$INSTDIR\ffmpeg"
  Delete "$INSTDIR\tvmaster.exe"
  Delete "$INSTDIR\tvmaster-setup.ps1"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\TVMaster\*.*"
  RMDir "$SMPROGRAMS\TVMaster"
  DeleteRegKey HKLM "${UNINST_KEY}"
  DeleteRegKey HKLM "Software\TVMaster"
  ; Your channels, libraries and token are kept unless you say otherwise.
  ${IfNot} ${Silent}
    MessageBox MB_YESNO|MB_ICONQUESTION "Also delete your TVMaster data (channels, libraries, schedule, access token)?$\r$\n$\r$\nChoose No to keep it for a future install." IDNO keep
    RMDir /r "${DATA_DIR}"
    keep:
  ${EndIf}
SectionEnd
