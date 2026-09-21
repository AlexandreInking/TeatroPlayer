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
4. **El ZIP portable** (`TeatroPlayer-portable.zip`) con el exe, `portable.txt`, un `LEEME.txt` y la licencia.
5. **El instalador NSIS**, si `makensis` está disponible, y el **SHA-256** del instalador (que es el `InstallerSha256` que pide winget).

Si NSIS no está instalado, lo dice y sigue con el resto. Para instalarlo:

```
winget install NSIS.NSIS
```

> NSIS **no** se pudo instalar desde aquí: `winget` devolvió `0x800704c7` (operación cancelada) porque necesita elevar permisos. Lo tienes que lanzar tú.

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
git tag -a v0.1.0 -m "TeatroPlayer 0.1.0"
git push origin main
git push origin v0.1.0
```

---

## 4. Crear el release

En GitHub, sobre el tag `v0.1.0`, adjuntando:

| Archivo | Para qué |
|---|---|
| `target/release/TeatroPlayer-Instalador.exe` | instalación normal, por usuario y sin administrador |
| `target/release/TeatroPlayer-portable.zip` | llevar en un pendrive, sin instalar |
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
manifests/t/TeatroPlayer/TeatroPlayer/0.1.0/
```

con los tres archivos, y abre el PR. La validación automática de winget comprueba el hash.

Cuando lo acepten:

```
winget install --id TeatroPlayer.TeatroPlayer -e
```

---

## 6. Comprobación final

Antes de cerrar:

- [ ] El instalador funciona en una cuenta **sin** administrador.
- [ ] Al desinstalar no queda nada (accesos directos, asociación `.tpshow`, carpeta).
- [ ] Doble clic en un `.tpshow` abre el programa con esa obra.
- [ ] El ZIP portable funciona en otra máquina (o con `%APPDATA%` renombrado).
- [ ] `cargo deny check` sigue en verde después de cualquier cambio de dependencias.

---

## Atajos

| Qué | Cómo |
|---|---|
| Todo lo automatizable | `scripts\publicar.ps1` (con PowerShell) |
| Solo el instalador | `powershell -File scripts\instalador.ps1` |
| Solo el ZIP portable | `python tools\portable_zip.py` |
| Verificar el ejecutable | `python tools\verificar_release.py` |
| Build reproducible | `scripts\repro-build.ps1` |
