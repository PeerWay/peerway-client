; Auto-start is opt-in: it is only written when the user turns it on in
; Settings, and removed again on uninstall.
!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "PeerWay"
!macroend

; Tauri only removes its identifier directories ($APPDATA\io.peerway.desktop).
; The node stores config, identity, token and logs in $APPDATA\PeerWay, so remove
; that directory too when the user checks "Delete app data" on the uninstaller.
; Downloads are intentionally kept: they may contain files received from other peers.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RMDir /r "$APPDATA\PeerWay"
    RMDir /r "$LOCALAPPDATA\PeerWay"
  ${EndIf}
!macroend
