; TeatroPlayer — instalador NSIS (T-REL-002, T-REL-004)
;
; Decisiones que importan:
;
;   * **Por usuario, sin administrador.** El que lo usa en un teatro no tiene
;     por qué tener permisos en el equipo. Todo va a HKCU y a $LOCALAPPDATA.
;   * **Registra la extensión .tpshow** para que al hacer doble clic en una
;     obra se abra el programa con esa obra (T-REL-004).
;   * **Desinstalador** que borra accesos directos, asociación y carpeta, y
;     deja el registro limpio.
;
; Para compilarlo hace falta NSIS (https://nsis.sourceforge.io/):
;     makensis installer\teatroplayer.nsi
; o con el script de ayuda, que lo busca y da instrucciones si no está:
;     powershell -ExecutionPolicy Bypass -File scripts\instalador.ps1

!include "MUI2.nsh"

Name "TeatroPlayer"
OutFile "target\release\TeatroPlayer-Instalador.exe"
InstallDir "$LOCALAPPDATA\Programs\TeatroPlayer"
InstallDirRegKey HKCU "Software\TeatroPlayer" "InstallDir"
RequestExecutionLevel user          ; <- sin administrador, a propósito
SetCompressor /SOLID lzma

!define VERSION "0.1.0"
!define PUBLISHER "TeatroPlayer"

; --- páginas ---------------------------------------------------------------
!insertmacro MUI_PAGE_LICENSE "LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Spanish"

; --- instalación -----------------------------------------------------------
Section "TeatroPlayer" SEC_PRINCIPAL
  SetOutPath "$INSTDIR"

  ; El ejecutable y lo que necesita al lado.
  File "target\release\teatroplayer.exe"

  ; Accesos directos: escritorio y menú de inicio (de usuario, no de todos).
  CreateShortcut "$DESKTOP\TeatroPlayer.lnk" "$INSTDIR\teatroplayer.exe"
  CreateDirectory "$SMPROGRAMS\TeatroPlayer"
  CreateShortcut "$SMPROGRAMS\TeatroPlayer\TeatroPlayer.lnk" "$INSTDIR\teatroplayer.exe"
  CreateShortcut "$SMPROGRAMS\TeatroPlayer\Desinstalar.lnk" "$INSTDIR\uninstall.exe"

  ; Desinstalador y su entrada en "Programas y características".
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TeatroPlayer" \
    "DisplayName" "TeatroPlayer ${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TeatroPlayer" \
    "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TeatroPlayer" \
    "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TeatroPlayer" \
    "Publisher" "${PUBLISHER}"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TeatroPlayer" \
    "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TeatroPlayer" \
    "NoRepair" 1

  ; --- Asociación de .tpshow (T-REL-004) -----------------------------------
  ; "%1" es la ruta de la obra: el programa la recibe como primer argumento y
  ; la abre al arrancar.
  WriteRegStr HKCU "Software\Classes\.tpshow" "" "TeatroPlayer.Show"
  WriteRegStr HKCU "Software\Classes\TeatroPlayer.Show" "" "Obra de TeatroPlayer"
  WriteRegStr HKCU "Software\Classes\TeatroPlayer.Show\DefaultIcon" "" "$INSTDIR\teatroplayer.exe,0"
  WriteRegStr HKCU "Software\Classes\TeatroPlayer.Show\shell\open\command" "" \
    '"$INSTDIR\teatroplayer.exe" "%1"'

  ; Avisar al Explorer de que la asociación cambió.
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
SectionEnd

; --- desinstalación --------------------------------------------------------
Section "Uninstall"
  Delete "$INSTDIR\teatroplayer.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$DESKTOP\TeatroPlayer.lnk"
  Delete "$SMPROGRAMS\TeatroPlayer\TeatroPlayer.lnk"
  Delete "$SMPROGRAMS\TeatroPlayer\Desinstalar.lnk"
  RMDir "$SMPROGRAMS\TeatroPlayer"

  ; Quitar la asociación y su rama entera.
  DeleteRegKey HKCU "Software\Classes\TeatroPlayer.Show"
  DeleteRegKey HKCU "Software\Classes\.tpshow"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TeatroPlayer"
  DeleteRegKey HKCU "Software\TeatroPlayer"

  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
SectionEnd
