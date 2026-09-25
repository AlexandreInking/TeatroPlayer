; TeatroPlayer — instalador NSIS (T-REL-002, T-REL-004)
;
; Decisiones que importan:
;
;   * **Por usuario, sin administrador.** El que lo usa en un teatro no tiene
;     por qué tener permisos en el equipo. Todo va a HKCU y a $LOCALAPPDATA.
;   * **Registra la extensión .tpshow** para que al hacer doble clic en una
;     obra se abra el programa con esa obra (T-REL-004).
;   * **Desinstalador** que borra accesos directos, asociación, carpeta y
;     **todo lo que la app haya dejado dentro**: los logs y el `state.json`
;     viven junto al ejecutable, así que si no se borran, `RMDir "$INSTDIR"`
;     falla por carpeta no vacía y quedan restos (Docs/14 §6).
;   * **La licencia y el aviso de terceros se copian** a la carpeta de
;     instalación. La GPL-3.0 obliga a entregar el texto de la licencia con el
;     binario, y `THIRD-PARTY.html` es obligación de las licencias de las
;     dependencias (Docs/11 §5). Enseñarlos en una página del asistente no
;     basta: tienen que quedar en disco.
;
; Rutas: **NSIS une las rutas relativas con la carpeta del script**, no con el
; directorio desde donde se lanza `makensis`. Comprobado a base de golpes: un
; `LICENSE` a secas se buscaba en `installer\LICENSE`, y `${__FILEDIR__}\..`
; acababa en `installer\installer\..`.
;
; Por eso todas las rutas cuelgan de `${RAIZ}`, que por defecto es `..` — es
; decir, la raíz del repositorio una vez unida a `installer\`. Los scripts de
; `scripts\` pasan además la ruta absoluta con `/DRAIZ=…`, para que funcione
; aunque el `.nsi` se copie a otro sitio.
;
; Para compilarlo hace falta NSIS (https://nsis.sourceforge.io/):
;     makensis installer\teatroplayer.nsi
; o con el script de ayuda, que lo busca (incluida la versión portable, que no
; necesita permisos de administrador) y da instrucciones si no está:
;     powershell -ExecutionPolicy Bypass -File scripts\instalador.ps1

!include "MUI2.nsh"

!ifndef RAIZ
  !define RAIZ ".."
!endif

Name "TeatroPlayer"
; El instalador sale a `dist\` (en `.gitignore`), no a `target\`: `cargo clean`
; borra `target\` entero y se llevaría por delante el instalador ya compilado.
; **`dist\` tiene que existir antes de invocar a makensis**: NSIS no crea la
; carpeta de `OutFile` y falla con un "Can't open output file" que no explica
; nada. La crean `scripts\instalador.ps1` y `scripts\publicar.ps1`.
OutFile "${RAIZ}\dist\TeatroPlayer-Instalador.exe"
InstallDir "$LOCALAPPDATA\Programs\TeatroPlayer"
InstallDirRegKey HKCU "Software\TeatroPlayer" "InstallDir"
RequestExecutionLevel user          ; <- sin administrador, a propósito
SetCompressor /SOLID lzma

!define VERSION "0.3.0"
!define PUBLISHER "TeatroPlayer"

; --- páginas ---------------------------------------------------------------
!insertmacro MUI_PAGE_LICENSE "${RAIZ}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Spanish"

; --- instalación -----------------------------------------------------------
Section "TeatroPlayer" SEC_PRINCIPAL
  SetOutPath "$INSTDIR"

  ; El ejecutable y lo que tiene que ir a su lado.
  File "${RAIZ}\target\release\teatroplayer.exe"
  File /oname=LICENSE.txt "${RAIZ}\LICENSE"
  File /oname=TERCEROS.html "${RAIZ}\THIRD-PARTY.html"

  ; Accesos directos: escritorio y menú de inicio (de usuario, no de todos).
  CreateShortcut "$DESKTOP\TeatroPlayer.lnk" "$INSTDIR\teatroplayer.exe"
  CreateDirectory "$SMPROGRAMS\TeatroPlayer"
  CreateShortcut "$SMPROGRAMS\TeatroPlayer\TeatroPlayer.lnk" "$INSTDIR\teatroplayer.exe"
  CreateShortcut "$SMPROGRAMS\TeatroPlayer\Licencia y terceros.lnk" "$INSTDIR\TERCEROS.html"
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
  ; Lo que instalamos nosotros.
  Delete "$INSTDIR\teatroplayer.exe"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\TERCEROS.html"

  ; Lo que deja la app al usarse. El `state.json` guarda la última sesión
  ; abierta y los logs se escriben en `logs\`, los dos **junto al ejecutable**
  ; (`Estado::ruta()` y `diagnostico::carpeta_logs()`). Si no se borran,
  ; `RMDir "$INSTDIR"` falla por carpeta no vacía y el desinstalador deja
  ; restos, que es justo lo que `Docs/14` §6 pide que no pase.
  Delete "$INSTDIR\state.json"
  RMDir /r "$INSTDIR\logs"

  Delete "$INSTDIR\uninstall.exe"

  ; Accesos directos.
  Delete "$DESKTOP\TeatroPlayer.lnk"
  Delete "$SMPROGRAMS\TeatroPlayer\TeatroPlayer.lnk"
  Delete "$SMPROGRAMS\TeatroPlayer\Licencia y terceros.lnk"
  Delete "$SMPROGRAMS\TeatroPlayer\Desinstalar.lnk"
  RMDir "$SMPROGRAMS\TeatroPlayer"

  ; Quitar la asociación y su rama entera.
  DeleteRegKey HKCU "Software\Classes\TeatroPlayer.Show"
  DeleteRegKey HKCU "Software\Classes\.tpshow"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TeatroPlayer"
  DeleteRegKey HKCU "Software\TeatroPlayer"

  ; Sólo se borra la carpeta si quedó vacía: si el usuario guardó algo suyo
  ; ahí, se respeta.
  RMDir "$INSTDIR"

  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
SectionEnd
