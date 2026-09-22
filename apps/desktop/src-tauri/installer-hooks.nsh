; HushType NSIS installer hooks (included by Tauri's installer template).

!macro NSIS_HOOK_PREUNINSTALL
  ; Stop a running instance so its files can be removed.
  nsExec::Exec 'taskkill /IM hushtype.exe /F'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Remove the "start with Windows" entry.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "HushType"
  ; "Delete application data" checkbox: settings, dictionary, history, models, logs.
  ${If} $DeleteAppDataCheckboxState = 1
    RMDir /r "$APPDATA\HushType"
    RMDir /r "$LOCALAPPDATA\HushType"
  ${EndIf}
!macroend
