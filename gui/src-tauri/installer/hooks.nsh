; Installer hooks for the Doppelpad setup (bundle.windows.nsis.installerHooks). Tauri
; includes this file near the top of its NSIS template, before the pages are declared.
;
; - Installs doppelpad.exe, the command-line tool, next to the app.
; - Installs the two drivers Doppelpad uses if they are missing: ViGEmBus (virtual
;   controllers) and HidHide (hides the physical controller from games). The official
;   setups are embedded, unpacked to $PLUGINSDIR and run silently. A checkbox on the
;   welcome page, on by default, turns this off.
; - Never removes the drivers on uninstall: other programs may use them.
; - Asks a running `doppelpad daemon` to stop before replacing or removing doppelpad.exe.
;
; Command line: /S (silent) and /P (passive) install missing drivers too, /NODRIVERS skips
; them. Updates (/UPDATE) never touch drivers.
;
; Updates: the app downloads the setup, stops its remapping service (which shows hidden
; controllers again) and exits, then the setup runs with /P /UPDATE /R: no pages, no
; drivers, and the app starts again when it is done (Tauri's template, .onInstSuccess).
;
; Build inputs, both ignored by git and prepared by `beforeBundleCommand`:
;   drivers\  driver setups and drivers.nsh (scripts/fetch-drivers.mjs)
;   bin\      doppelpad.exe (scripts/stage-cli.mjs)

!define DP_PAYLOAD "${__FILEDIR__}"

!if /FileExists "${DP_PAYLOAD}\drivers\drivers.nsh"
  !include "${DP_PAYLOAD}\drivers\drivers.nsh"
!else
  !error "Driver setups missing: run 'node scripts/fetch-drivers.mjs' in gui/"
!endif
!if /FileExists "${DP_PAYLOAD}\bin\doppelpad.exe"
!else
  !error "doppelpad.exe missing: run 'node scripts/stage-cli.mjs' in gui/"
!endif

; Texts. Language IDs: 1033 English, 1031 German (the languages in tauri.conf.json).
LangString DpWelcomeText 1033 "Setup will install $(^NameDA) on this computer.$\r$\n$\r$\n$(^NameDA) works with two free drivers: ViGEmBus creates virtual controllers, and HidHide hides the physical controller from games. Setup installs the ones that are missing. Windows needs a restart after HidHide is installed.$\r$\n$\r$\n$_CLICK"
LangString DpWelcomeText 1031 "Der Assistent installiert $(^NameDA) auf diesem Computer.$\r$\n$\r$\n$(^NameDA) arbeitet mit zwei freien Treibern: ViGEmBus erzeugt virtuelle Controller, HidHide verbirgt den echten Controller vor Spielen. Fehlende Treiber werden mitinstalliert. Nach der Installation von HidHide muss Windows neu gestartet werden.$\r$\n$\r$\n$_CLICK"
LangString DpInstallDrivers 1033 "Install drivers:"
LangString DpInstallDrivers 1031 "Treiber installieren:"
LangString DpDriversPresent 1033 "ViGEmBus and HidHide are already installed."
LangString DpDriversPresent 1031 "ViGEmBus und HidHide sind bereits installiert."
LangString DpDriversSkipped 1033 "Drivers: not installed (deselected)"
LangString DpDriversSkipped 1031 "Treiber: nicht installiert (abgewählt)"
LangString DpAlreadyInstalled 1033 "already installed"
LangString DpAlreadyInstalled 1031 "bereits installiert"
LangString DpInstalling 1033 "Installing driver"
LangString DpInstalling 1031 "Installiere Treiber"
LangString DpInstalled 1033 "installed"
LangString DpInstalled 1031 "installiert"
LangString DpFailed 1033 "setup failed with exit code"
LangString DpFailed 1031 "Setup fehlgeschlagen, Exit-Code"
LangString DpNotFound 1033 "setup finished, but the driver was not found"
LangString DpNotFound 1031 "Setup beendet, aber der Treiber wurde nicht gefunden"
LangString DpFailedHint 1033 "$(^NameDA) is installed, but this driver is missing. It can be installed later from:"
LangString DpFailedHint 1031 "$(^NameDA) ist installiert, aber dieser Treiber fehlt. Er lässt sich später hier installieren:"
LangString DpSetupLog 1033 "Setup log:"
LangString DpSetupLog 1031 "Setup-Protokoll:"
LangString DpRestartNeeded 1033 "Restart Windows to finish installing the drivers."
LangString DpRestartNeeded 1031 "Windows muss neu gestartet werden, um die Treiberinstallation abzuschließen."
LangString DpRebootText 1033 "Windows must be restarted to finish installing the drivers for $(^NameDA).$\r$\n$\r$\nDo you want to reboot now?"
LangString DpRebootText 1031 "Windows muss neu gestartet werden, um die Installation der Treiber für $(^NameDA) abzuschließen.$\r$\n$\r$\nJetzt neu starten?"

; The first MUI page in Tauri's template is the welcome page, so these apply to it.
!define MUI_WELCOMEPAGE_TEXT "$(DpWelcomeText)"
!define MUI_PAGE_CUSTOMFUNCTION_SHOW DpWelcomeShow
; Only this file sets the reboot flag, so the finish page can say why.
!define MUI_FINISHPAGE_TEXT_REBOOT "$(DpRebootText)"

Var DpSkipDrivers ; 1 = do not install drivers
Var DpCheckbox

; Pushes 1 if the kernel driver service is registered and its driver file exists, else 0.
;   Push "ViGEmBus"
;   Call DpDriverPresent
;   Pop $0
Function DpDriverPresent
  Exch $0
  Push $1
  Push $2
  ReadRegStr $1 HKLM "SYSTEM\CurrentControlSet\Services\$0" "ImagePath"
  StrCpy $0 0
  ${If} $1 != ""
    ExpandEnvStrings $1 $1
    ; \SystemRoot\System32\drivers\x.sys, System32\drivers\x.sys or \??\C:\...\x.sys
    StrCpy $2 $1 12
    ${If} $2 == "\SystemRoot\"
      StrCpy $1 $1 "" 12
      StrCpy $1 "$WINDIR\$1"
    ${Else}
      StrCpy $2 $1 4
      ${If} $2 == "\??\"
        StrCpy $1 $1 "" 4
      ${Else}
        StrCpy $2 $1 1 1
        ${If} $2 != ":"
          StrCpy $1 "$WINDIR\$1"
        ${EndIf}
      ${EndIf}
    ${EndIf}
    ; The installer is 32-bit: look at the real System32, not SysWOW64.
    ${DisableX64FSRedirection}
    ${If} ${FileExists} "$1"
      StrCpy $0 1
    ${EndIf}
    ${EnableX64FSRedirection}
  ${EndIf}
  Pop $2
  Pop $1
  Exch $0
FunctionEnd

; Welcome page: a checkbox below the text listing the missing drivers.
Function DpWelcomeShow
  Push $0
  Push $1
  StrCpy $1 ""
  Push "ViGEmBus"
  Call DpDriverPresent
  Pop $0
  ${If} $0 = 0
    StrCpy $1 "ViGEmBus"
  ${EndIf}
  Push "HidHide"
  Call DpDriverPresent
  Pop $0
  ${If} $0 = 0
    ${If} $1 != ""
      StrCpy $1 "$1, "
    ${EndIf}
    StrCpy $1 "$1HidHide"
  ${EndIf}

  ; The text label ends at 175u and the page is 193u high (MUI2 Welcome.nsh).
  ${If} $1 == ""
    ${NSD_CreateLabel} 120u 178u 195u 10u "$(DpDriversPresent)"
    Pop $0
  ${Else}
    ${NSD_CreateCheckbox} 120u 178u 195u 10u "$(DpInstallDrivers) $1"
    Pop $DpCheckbox
    ${If} $DpSkipDrivers != 1
      ${NSD_Check} $DpCheckbox
    ${EndIf}
    ${NSD_OnClick} $DpCheckbox DpCheckboxClick
    StrCpy $0 $DpCheckbox
  ${EndIf}
  ; MUI's default page background, which Tauri keeps.
  SetCtlColors $0 "" "FFFFFF"
  Pop $1
  Pop $0
FunctionEnd

Function DpCheckboxClick
  Exch $0 ; the checkbox
  ${NSD_GetState} $0 $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $DpSkipDrivers ""
  ${Else}
    StrCpy $DpSkipDrivers 1
  ${EndIf}
  Pop $0
FunctionEnd

; Installs one driver unless it is present. Uses $0-$2; $3 = 1 if setup is unattended.
; ALWAYS_REBOOT = 1: ask for a restart after a successful install even if the driver's
; setup does not.
!macro DP_INSTALL_DRIVER NAME VERSION SETUP URL ALWAYS_REBOOT
  Push "${NAME}"
  Call DpDriverPresent
  Pop $0
  ${If} $0 = 1
    DetailPrint "${NAME}: $(DpAlreadyInstalled)"
  ${Else}
    DetailPrint "$(DpInstalling) ${NAME} ${VERSION}"
    File "/oname=$PLUGINSDIR\${SETUP}" "${DP_PAYLOAD}\drivers\${SETUP}"
    StrCpy $2 "$TEMP\doppelpad-${NAME}-setup.log"
    ; Advanced Installer bootstrapper: /exenoui hides its own UI, the rest goes to msiexec.
    ExecWait '"$PLUGINSDIR\${SETUP}" /exenoui /qn /norestart /L*V "$2"' $1
    Delete "$PLUGINSDIR\${SETUP}"
    ${If} $1 = 0
    ${OrIf} $1 = 3010 ; ERROR_SUCCESS_REBOOT_REQUIRED
    ${OrIf} $1 = 1641 ; ERROR_SUCCESS_REBOOT_INITIATED
      Push "${NAME}"
      Call DpDriverPresent
      Pop $0
      ${If} $0 = 1
        DetailPrint "${NAME}: $(DpInstalled)"
        ${If} $1 <> 0
        ${OrIf} ${ALWAYS_REBOOT} = 1
          SetRebootFlag true
        ${EndIf}
      ${Else}
        StrCpy $1 "$(DpNotFound)"
      ${EndIf}
    ${Else}
      StrCpy $1 "$(DpFailed) $1"
    ${EndIf}
    ${If} $0 <> 1
      DetailPrint "${NAME}: $1 ($(DpSetupLog) $2)"
      ${If} $3 <> 1
        MessageBox MB_ICONEXCLAMATION|MB_OK "${NAME}: $1$\r$\n$\r$\n$(DpFailedHint)$\r$\n${URL}$\r$\n$\r$\n$(DpSetupLog) $2"
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

Function DpInstallDrivers
  Push $0
  Push $1
  Push $2
  Push $3

  ${GetOptions} $CMDLINE "/NODRIVERS" $0
  ${IfNot} ${Errors}
    StrCpy $DpSkipDrivers 1
  ${EndIf}
  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${IfNot} ${Errors}
    ; Updates never touch drivers.
  ${ElseIf} $DpSkipDrivers == 1
    DetailPrint "$(DpDriversSkipped)"
  ${Else}
    StrCpy $3 0
    ${GetOptions} $CMDLINE "/P" $0
    ${IfNot} ${Errors}
    ${OrIf} ${Silent}
      StrCpy $3 1
    ${EndIf}

    InitPluginsDir
    !insertmacro DP_INSTALL_DRIVER "ViGEmBus" "${DP_VIGEMBUS_VERSION}" "${DP_VIGEMBUS_SETUP}" \
      "https://github.com/nefarius/ViGEmBus/releases" 0
    ; HidHide attaches to the input device stacks only after a restart.
    !insertmacro DP_INSTALL_DRIVER "HidHide" "${DP_HIDHIDE_VERSION}" "${DP_HIDHIDE_SETUP}" \
      "https://github.com/nefarius/HidHide/releases" 1

    ${If} ${RebootFlag}
      DetailPrint "$(DpRestartNeeded)"
      ; With the reboot flag set, the finish page offers a restart instead of its
      ; checkboxes, so create the desktop shortcut that is on by default there.
      ${If} $3 <> 1
        Call CreateOrUpdateDesktopShortcut
      ${EndIf}
    ${EndIf}
  ${EndIf}

  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; Asks a running `doppelpad daemon` to stop, as `doppelpad shutdown` does, so it shows hidden
; controllers again and unplugs its virtual ones, and waits up to three seconds for it to
; exit. CheckIfAppIsRunning then closes whatever still runs. The app refuses the request (it
; quits from its tray); before an update it has stopped its service and exited already.
; A macro, not a function: functions in this file are compiled before Tauri's template adds
; the folder of its plugin (nsis_tauri_utils).
!macro DP_STOP_DAEMON
  Push $0
  Push $1
  ${If} ${FileExists} "$INSTDIR\doppelpad.exe"
    nsExec::Exec '"$INSTDIR\doppelpad.exe" shutdown'
    Pop $0
    ${If} $0 == 0
      StrCpy $1 0
      ${Do}
        nsis_tauri_utils::FindProcess "doppelpad.exe"
        Pop $0
        ${If} $0 <> 0 ; gone
        ${OrIf} $1 >= 30
          ${ExitDo}
        ${EndIf}
        Sleep 100
        IntOp $1 $1 + 1
      ${Loop}
    ${EndIf}
  ${EndIf}
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; The CLI may be running as `doppelpad daemon`.
  !insertmacro DP_STOP_DAEMON
  !insertmacro CheckIfAppIsRunning "$INSTDIR\doppelpad.exe" "doppelpad"
  File "/oname=$INSTDIR\doppelpad.exe" "${DP_PAYLOAD}\bin\doppelpad.exe"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  Call DpInstallDrivers
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro DP_STOP_DAEMON
  !insertmacro CheckIfAppIsRunning "$INSTDIR\doppelpad.exe" "doppelpad"
  Delete "$INSTDIR\doppelpad.exe"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; "Start with Windows" (app settings): the user's Run entry, plus Task Manager's on/off
  ; mark for it. Updates keep it; the path to the app stays the same.
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Doppelpad"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Doppelpad"
  ${EndIf}
  ; Tauri's "delete app data" removes $APPDATA\<identifier>; profiles and settings live in
  ; $APPDATA\doppelpad, logs in $LOCALAPPDATA\doppelpad\logs. The HidHide crash-recovery
  ; journal next to the logs stays, so the folder only goes when it is empty.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RMDir /r "$APPDATA\doppelpad"
    RMDir /r "$LOCALAPPDATA\doppelpad\logs"
    RMDir "$LOCALAPPDATA\doppelpad"
  ${EndIf}
!macroend
