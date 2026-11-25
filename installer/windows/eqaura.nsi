; Build: makensis -DVERSION=0.1.0 installer/windows/eqaura.nsi
; Expects dist/windows with eqaura.exe, eqaura-agent.exe, the license files and
; EqualizerAPO64.exe with its license and source pointer.
Unicode true
SetCompressor /SOLID lzma
!include "MUI2.nsh"
!include "x64.nsh"
!include "LogicLib.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!define DIST "..\..\dist\windows"
!define KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\EQaura"

Name "EQaura"
OutFile "..\..\dist\EQaura-${VERSION}-windows-x64-setup.exe"
InstallDir "$PROGRAMFILES64\EQaura"
RequestExecutionLevel admin
ManifestDPIAware true
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "EQaura"
VIAddVersionKey "FileDescription" "EQaura setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "MIT license"

!define MUI_ICON "..\..\apps\desktop\assets\icon.ico"
!define MUI_UNICON "..\..\apps\desktop\assets\icon.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TEXT "EQaura shapes the sound of everything you play.$\r$\n$\r$\nIt uses Equalizer APO, a free audio engine by Jonas Thedering (GPL-2.0). If you do not have it yet, setup installs it and opens its device list. Tick every device you listen on, then click OK or Close."
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${DIST}\LICENSE.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_FUNCTION StartApp
!define MUI_FINISHPAGE_TEXT "If you hear no change, restart Windows once. The System page in EQaura shows what is missing."
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "EQaura needs 64-bit Windows." /SD IDOK
    Abort
  ${EndIf}
  SetRegView 64
FunctionEnd

Function un.onInit
  SetRegView 64
FunctionEnd

; Explorer starts the app as the normal user, not as administrator.
Function StartApp
  Exec '"$WINDIR\explorer.exe" "$INSTDIR\eqaura.exe"'
FunctionEnd

Section "EQaura"
  SetShellVarContext all
  nsExec::Exec 'taskkill /IM eqaura-agent.exe /F'
  nsExec::Exec 'taskkill /IM eqaura.exe'
  Sleep 800
  SetOutPath "$INSTDIR"
  File "${DIST}\eqaura.exe"
  File "${DIST}\eqaura-agent.exe"
  File "${DIST}\LICENSE.txt"
  File "${DIST}\THIRD-PARTY-LICENSES.txt"
  File "${DIST}\EqualizerAPO-LICENSE.txt"

  ReadRegStr $0 HKLM "SOFTWARE\EqualizerAPO" "InstallPath"
  StrCpy $1 "kept"
  ${If} $0 == ""
    StrCpy $1 "new"
    DetailPrint "Installing Equalizer APO"
    SetOutPath "$PLUGINSDIR"
    File "${DIST}\EqualizerAPO64.exe"
    ExecWait '"$PLUGINSDIR\EqualizerAPO64.exe" /S' $2
    ReadRegStr $0 HKLM "SOFTWARE\EqualizerAPO" "InstallPath"
    ${If} $0 == ""
      MessageBox MB_ICONEXCLAMATION "Equalizer APO could not be installed (code $2). EQaura's System page explains how to add it later." /SD IDOK
    ${EndIf}
  ${EndIf}

  ${If} $0 != ""
    ReadRegStr $3 HKLM "SOFTWARE\EqualizerAPO" "ConfigPath"
    ${If} $3 == ""
      StrCpy $3 "$0\config"
    ${EndIf}
    ; Our own file, writable by normal users. Nothing else changes rights.
    ${IfNot} ${FileExists} "$3\EQaura.txt"
      FileOpen $4 "$3\EQaura.txt" w
      FileWrite $4 "# Written by EQaura. Changes made here are replaced.$\r$\n"
      FileClose $4
    ${EndIf}
    nsExec::Exec 'icacls "$3\EQaura.txt" /grant *S-1-5-32-545:M'
    ${If} $1 == "new"
      ; A fresh install ships an example EQ, which would colour the sound.
      ${IfNot} ${FileExists} "$3\config.before-eqaura.txt"
        CopyFiles /SILENT "$3\config.txt" "$3\config.before-eqaura.txt"
      ${EndIf}
      FileOpen $4 "$3\config.txt" w
      FileWrite $4 "Include: EQaura.txt$\r$\n"
      FileClose $4
    ${Else}
      nsExec::ExecToStack 'findstr /I /B /C:"Include: EQaura.txt" "$3\config.txt"'
      Pop $5
      Pop $6
      ${If} $5 != 0
        FileOpen $4 "$3\config.txt" a
        FileSeek $4 0 END
        FileWrite $4 "$\r$\nDevice: all$\r$\nChannel: all$\r$\nInclude: EQaura.txt$\r$\n"
        FileClose $4
      ${EndIf}
    ${EndIf}
  ${EndIf}

  SetOutPath "$INSTDIR"
  CreateShortcut "$SMPROGRAMS\EQaura.lnk" "$INSTDIR\eqaura.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "${KEY}" "DisplayName" "EQaura"
  WriteRegStr HKLM "${KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${KEY}" "Publisher" "AdityaTummuri"
  WriteRegStr HKLM "${KEY}" "URLInfoAbout" "https://github.com/AdityaTummuri/EQaura"
  WriteRegStr HKLM "${KEY}" "DisplayIcon" "$INSTDIR\eqaura.exe"
  WriteRegStr HKLM "${KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKLM "${KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext all
  nsExec::Exec 'taskkill /IM eqaura-agent.exe /F'
  nsExec::Exec 'taskkill /IM eqaura.exe /F'
  Sleep 500
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "EQaura"
  ReadRegStr $3 HKLM "SOFTWARE\EqualizerAPO" "ConfigPath"
  ${If} $3 != ""
    ; Remove our line and keep every other setting.
    nsExec::Exec 'cmd /C findstr /I /V /B /C:"Include: EQaura.txt" "$3\config.txt" > "$3\config.eqaura-new"'
    ${If} ${FileExists} "$3\config.eqaura-new"
      CopyFiles /SILENT "$3\config.eqaura-new" "$3\config.txt"
      Delete "$3\config.eqaura-new"
    ${EndIf}
    Delete "$3\EQaura.txt"
    Delete "$3\EQaura.txt.tmp"
  ${EndIf}
  Delete "$SMPROGRAMS\EQaura.lnk"
  Delete "$INSTDIR\eqaura.exe"
  Delete "$INSTDIR\eqaura-agent.exe"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\THIRD-PARTY-LICENSES.txt"
  Delete "$INSTDIR\EqualizerAPO-LICENSE.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKLM "${KEY}"
  ReadRegStr $0 HKLM "SOFTWARE\EqualizerAPO" "InstallPath"
  ${If} $0 != ""
  ${AndIf} ${FileExists} "$0\Uninstall.exe"
    MessageBox MB_YESNO|MB_ICONQUESTION "EQaura is removed. Do you want to remove Equalizer APO as well?" /SD IDNO IDNO done
    Exec '"$0\Uninstall.exe"'
    done:
  ${EndIf}
SectionEnd
