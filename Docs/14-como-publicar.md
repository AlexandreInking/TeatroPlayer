# 14 — Cómo publicar una versión

Guion práctico. Casi todo está automatizado en `scripts/publicar.ps1`; lo que queda son decisiones y cosas que necesitan permisos.

---

## 1. Preparar (automático)

```
powershell -ExecutionPolicy Bypass -File scripts\publicar.ps1
```

Hace, en orden:

1. **Los tests.** Si alguno falla, se para: no se publica nada roto.
2. **El ejecutable de release** (`target/release/teatroplayer.exe`).
3. **El aviso de terceros** (`THIRD-PARTY.html`) con `cargo-about`. Obligación de la GPL-3.0.
4. **El ZIP portable** (`dist/TeatroPlayer-portable.zip`) con el exe, `portable.txt`, un `LEEME.txt` y la licencia.
5. **El instalador NSIS** (`dist/TeatroPlayer-Instalador.exe`), si `makensis` está disponible, y el **SHA-256** del instalador (que es el `InstallerSha256` que pide winget).

Si NSIS no está instalado, lo dice y sigue con el resto.

### Por qué los paquetes salen a `dist/` y no a `target/`

Los dos artefactos que se reparten (instalador y ZIP portable) se escriben en
`dist/`, que está en `.gitignore`. Antes salían a `target/release/`, y eso tenía
dos problemas:

- **`cargo clean` se los lleva.** `target/` es una carpeta de compilación y se
  borra sin miramientos; un instalador recién compilado no debería vivir ahí.
- **Se mezclan con 9 GB de objetos intermedios.** Encontrar «lo que hay que
  subir al release» entre miles de `.o` y `.rlib` es incómodo.

`dist/` está ignorado por git, así que **nada de lo que se genera entra al
repositorio**: el release se publica en GitHub, no en el control de versiones.

> **NSIS no crea la carpeta de `OutFile`.** Si `dist\` no existe, makensis falla
> con un `Can't open output file` que no explica nada. Por eso
> `scripts\instalador.ps1` y `scripts\publicar.ps1` hacen `New-Item -Force`
> antes de llamarlo. Al invocar `makensis` a mano, crear `dist\` primero.

### NSIS sin permisos de administrador (recomendado)

`winget install NSIS.NSIS` necesita elevación y en este equipo devuelve
`0x800704c7` (operación cancelada). No hace falta pelearse con eso: **NSIS se
puede usar sin instalar**.

```
1. Descarga el ZIP de https://nsis.sourceforge.io/Download   (nsis-3.10.zip, ~2 MB)
2. Descomprímelo donde quieras, por ejemplo C:\herramientas\nsis-3.10
3. setx NSIS_DIR "C:\herramientas\nsis-3.10"
4. Cierra y reabre la terminal
```

`scripts\nsis.ps1` busca `makensis` en `%NSIS_DIR%` **antes** que en el PATH y
que en las carpetas de la instalación normal, así que con eso basta. Probado
con NSIS 3.10 portable: el instalador sale igual que con la versión instalada.

También vale `winget install NSIS.NSIS` si tienes permisos; el script lo
encuentra igual.

### Rutas dentro del `.nsi`

NSIS une las rutas relativas con **la carpeta del script**, no con el directorio
desde donde lanzas `makensis`. Es una trampa que cuesta dos intentos: un
`LICENSE` a secas se busca en `installer\LICENSE`, y `${__FILEDIR__}\..` acaba
en `installer\installer\..`.

Por eso todas las rutas del `.nsi` cuelgan de `${RAIZ}`, que por defecto es `..`
(la raíz del repositorio, una vez unida a `installer\`). Los scripts de
`scripts\` pasan además la ruta absoluta con `/DRAIZ=…`, así que funciona
aunque el `.nsi` se copie a otra carpeta.

---

## 2. Revisar el estado del repo

```
git status
```

Si algo cambió:

```
git add -A
git commit -m "lo que sea"
```

---

## 3. Tag y push

El tag es lo que identifica el código fuente exacto del binario. La GPL-3.0 exige publicar ese código (ver `Docs/11` §5).

```
git tag -a v0.3.0 -m "TeatroPlayer 0.3.0"
git push origin main
git push origin v0.3.0
```

---

## 4. Crear el release

### Con `gh` (necesita sesión)

`gh` está instalado en este equipo pero **sin autenticar** (`gh auth status` lo
dice), así que hay que entrar una vez:

```
gh auth login
```

Y después, desde la raíz del repositorio:

```
gh release create v0.3.0 ^
  "dist\TeatroPlayer-Instalador.exe" ^
  "dist\TeatroPlayer-portable.zip" ^
  "THIRD-PARTY.html" ^
  --title "TeatroPlayer 0.3.0" ^
  --notes-file "installer\notas-release-0.3.0.md"
```

Las notas ya están escritas en `installer\notas-release-0.3.0.md`: se pueden
editar antes de lanzarlo. **No hace falta reescribirlas.**

> El `InstallerUrl` del manifiesto de winget apunta justo a
> `releases/download/v0.3.0/TeatroPlayer-Instalador.exe`, así que **hasta que
> exista el release esa URL no resuelve** y winget no puede validar el paquete.

### A mano, en la web

En GitHub, sobre el tag `v0.3.0`, adjuntando:

| Archivo | Para qué |
|---|---|
| `dist/TeatroPlayer-Instalador.exe` | instalación normal, por usuario y sin administrador |
| `dist/TeatroPlayer-portable.zip` | llevar en un pendrive, sin instalar |
| `THIRD-PARTY.html` | aviso de licencias de terceros |

Y en el texto del release, un enlace al tag. **Con eso se cumple la obligación de código fuente de la GPL-3.0**: el binario y su fuente quedan publicados juntos.

Antes de subirlo, conviene mirar `installer/teatroplayer.nsi` y comprobar que la instalación crea el acceso directo y asocia `.tpshow`.

---

## 5. winget (opcional pero recomendable)

Los tres YAML están en `installer/winget/`. Hay que rellenar dos cosas en `TeatroPlayer.TeatroPlayer.installer.yaml`:

- `InstallerUrl` → la URL del `.exe` del release
- `InstallerSha256` → el SHA-256 que imprimió `scripts/publicar.ps1`

Después, bifurca `microsoft/winget-pkgs` y crea:

```
manifests/t/TeatroPlayer/TeatroPlayer/0.3.0/
```

con los tres archivos, y abre el PR. La validación automática de winget comprueba el hash.

Cuando lo acepten:

```
winget install --id TeatroPlayer.TeatroPlayer -e
```

---

## 6. Comprobación final

Antes de cerrar:

- [x] El instalador funciona en una cuenta **sin** administrador.
- [x] Al desinstalar no queda nada (accesos directos, asociación `.tpshow`, carpeta).
- [x] Doble clic en un `.tpshow` abre el programa con esa obra.
- [ ] El ZIP portable funciona en otra máquina (o con `%APPDATA%` renombrado).
- [x] `cargo deny check` sigue en verde después de cualquier cambio de dependencias.

### Comprobado sin publicar nada (2026-09-22)

| Comprobación | Cómo se hizo | Resultado |
|---|---|---|
| El instalador **compila** | `makensis` con NSIS 3.10 portable | 2,83 MB |
| El ejecutable **no abre consola** | `tools/verificar_release.py` | subsistema 2 = GUI |
| Tamaño del ejecutable | idem | 7,7 MB (presupuesto 6-12 MB) |
| Los logs se escriben | idem, arrancando 3 s | OK |
| `cargo deny check` | `cargo deny check` | advisories, bans, licencias y fuentes OK |
| El ZIP portable se genera | `tools/portable_zip.py` | 3,4 MB, con licencia y terceros |
| Aviso de terceros al día | `cargo about generate about.hbs -o THIRD-PARTY.html` | OK |
| **Instala sin administrador** | `TeatroPlayer-Instalador.exe /S` | en `%LOCALAPPDATA%\Programs\TeatroPlayer`, sin UAC |
| **Deja lo que debe** | inspección de la carpeta y del registro | los 4 archivos, entrada en Programas y características con `DisplayVersion 0.3.0`, `.tpshow` → `TeatroPlayer.Show`, accesos en escritorio y menú de inicio |
| **La asociación funciona** | abrir una obra con el ejecutable instalado | arranca con la obra y escribe `logs\` a su lado |
| **Desinstala limpio** | `uninstall.exe /S` y volver a mirar | carpeta, 4 claves de registro y los dos accesos directos: todo fuera |

La prueba de desinstalación es la que valida el arreglo de T-PUB-004: antes de
desinstalar había un `logs\` dentro de la carpeta, y la carpeta se borró entera.

### Lo que queda por comprobar

| Comprobación | Qué hace falta |
|---|---|
| ZIP portable en otra máquina | Otro equipo, o renombrar `%APPDATA%` |
| `InstallerUrl` / `InstallerSha256` de winget | Un **repositorio en GitHub** con el release subido (§7) |

---

## 7. Dónde se publica

El remoto ya está configurado:

```
origin  https://github.com/AlexandreInking/TeatroPlayer.git   (rama main)
```

Los cuatro enlaces de los manifiestos apuntan ahí, y el `InstallerSha256` está
relleno con el hash del instalador de la 0.3.0:

| Manifiesto | Qué lleva |
|---|---|
| `TeatroPlayer.TeatroPlayer.installer.yaml` | `InstallerUrl` (tag `v0.3.0`) y `InstallerSha256` |
| `TeatroPlayer.TeatroPlayer.locale.es-ES.yaml` | `PackageUrl`, `LicenseUrl`, `DocumentUrl` |
| `TeatroPlayer.TeatroPlayer.yaml` | sólo la versión |

**Si se recompila el instalador, el `InstallerSha256` deja de valer.** Hay que
volver a calcularlo y actualizarlo antes de abrir el PR de winget:

```
(Get-FileHash dist\TeatroPlayer-Instalador.exe -Algorithm SHA256).Hash
```

o copiarlo de lo que imprime `scripts\publicar.ps1`. Un hash que no cuadra hace
que winget rechace el paquete en la validación automática.

> **Ojo con los finales de línea.** Estos YAML y el `.nsi` están en CRLF. Si se
> editan con una herramienta que busque por `\n`, la sustitución **falla en
> silencio** y el archivo queda como estaba. Pasó al cablear la URL: el
> reemplazo del comentario no entró y el del hash sí (era de una sola línea).
> Si algo no se sustituye, mirar los bytes antes de dar por hecho que el texto
> no estaba.

---

## Atajos

| Qué | Cómo |
|---|---|
| Todo lo automatizable | `scripts\publicar.ps1` (con PowerShell) |
| Solo el instalador | `powershell -File scripts\instalador.ps1` → `dist\` |
| Solo el ZIP portable | `python tools\portable_zip.py` → `dist\` |
| Verificar el ejecutable | `python tools\verificar_release.py` |
| Build reproducible | `scripts\repro-build.ps1` |
| Comparar el coste del fade | `cargo test --release --test perfil_ganancia -- --nocapture` |
