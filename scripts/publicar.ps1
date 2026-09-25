# Prepara todo lo necesario para publicar una versión (T-REL-002/003/005).
#
# Hace todo lo que se puede hacer sin permisos de administrador:
#   1. compila el ejecutable de release
#   2. genera el aviso de terceros (THIRD-PARTY.html)
#   3. crea el ZIP portable
#   4. compila el instalador NSIS si makensis está disponible
#   5. calcula el SHA-256 del instalador, que es lo que pide winget
#
# Lo que NO puede hacer por ti (necesita decisiones o permisos):
#   - instalar NSIS (si falta, lo dice)
#   - crear el tag y subirlo
#   - crear el release en GitHub
#   - abrir el PR del manifiesto winget
#
# Uso:
#   powershell -ExecutionPolicy Bypass -File scripts\publicar.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\publicar.ps1 -Version 0.3.0

param(
    [string]$Version = "0.3.0"
)

$ErrorActionPreference = "Stop"

$raiz = Split-Path -Parent $PSScriptRoot
Set-Location $raiz

Write-Host "=== TeatroPlayer $Version : preparando publicacion ==="
Write-Host ""

# --- 0. comprobaciones previas ----------------------------------------------
$fallos = @()

$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) {
    $candidato = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    if (Test-Path $candidato) { $cargo = $candidato }
}
if (-not $cargo) { $fallos += "no se encuentra cargo" }

if (-not (Test-Path "LICENSE")) { $fallos += "no hay LICENSE en la raiz (obligacion GPL-3.0)" }

if ($fallos.Count -gt 0) {
    Write-Host "FALLO:"
    $fallos | ForEach-Object { Write-Host "  - $_" }
    exit 1
}

# --- 1. tests antes de nada --------------------------------------------------
Write-Host "[1/5] tests..."
& $cargo test --workspace 2>&1 | Select-String -Pattern "^test result" | ForEach-Object { Write-Host "      $_" }
if ($LASTEXITCODE -ne 0) { Write-Host "FALLO: los tests no pasan. No se publica."; exit 1 }

# --- 2. ejecutable de release ------------------------------------------------
Write-Host "[2/5] compilando release..."
& $cargo build --release
if ($LASTEXITCODE -ne 0) { Write-Host "FALLO: no compila."; exit 1 }

$exe = "target\release\teatroplayer.exe"
$tam = [math]::Round((Get-Item $exe).Length / 1MB, 2)
Write-Host "      $exe ($tam MB)"

# --- 3. aviso de terceros ----------------------------------------------------
Write-Host "[3/5] aviso de terceros..."
$about = (Get-Command cargo-about -ErrorAction SilentlyContinue).Source
if ($about) {
    & $cargo about generate about.hbs -o THIRD-PARTY.html
    if ($LASTEXITCODE -eq 0) { Write-Host "      THIRD-PARTY.html" }
    else { Write-Host "      AVISO: cargo-about fallo" }
} else {
    Write-Host "      AVISO: cargo-about no instalado (cargo install cargo-about --features cli)"
}

# --- 4. ZIP portable e instalador -------------------------------------------
Write-Host "[4/5] ZIP portable..."
python tools\portable_zip.py
if ($LASTEXITCODE -ne 0) { Write-Host "      AVISO: no se genero el ZIP" }

. "$PSScriptRoot\nsis.ps1"
$makensis = Buscar-Makensis

if ($makensis) {
    Write-Host "[5/5] instalador NSIS..."
    # NSIS no crea la carpeta de `OutFile`: si no existe, falla con un
    # "Can't open output file" que no explica nada.
    New-Item -ItemType Directory -Force -Path "dist" | Out-Null
    # /DRAIZ con la ruta absoluta: NSIS une las rutas relativas con la carpeta del
    # script, asi que sin esto el .nsi depende de la estructura de carpetas.
    & $makensis "/DRAIZ=$raiz" "installer\teatroplayer.nsi"
    if ($LASTEXITCODE -eq 0) {
        $inst = "dist\TeatroPlayer-Instalador.exe"
        if (Test-Path $inst) {
            $hash = (Get-FileHash $inst -Algorithm SHA256).Hash
            Write-Host ""
            Write-Host "instalador : $inst"
            Write-Host "sha256     : $hash"
            Write-Host "  ^ este es el InstallerSha256 del manifiesto winget"
        }
    } else {
        Write-Host "      AVISO: makensis fallo"
    }
} else {
    Write-Host "[5/5] instalador NSIS: NO COMPILADO"
    Write-Host "      hace falta NSIS. Dos formas:"
    Write-Host "        con permisos : winget install NSIS.NSIS"
    Write-Host "        sin permisos : descargar el ZIP de nsis.sourceforge.io,"
    Write-Host "                       descomprimir y definir NSIS_DIR"
}

# --- resumen ----------------------------------------------------------------
Write-Host ""
Write-Host "=== listo para publicar ==="
Write-Host ""
Write-Host "Falta por hacer (esto si lo tienes que hacer tu):"
Write-Host "  1. revisar y commitear si algo cambio:"
Write-Host "       git add -A ; git commit -m '...'"
Write-Host "  2. crear el tag y subirlo:"
Write-Host "       git tag -a v$Version -m 'TeatroPlayer $Version'"
Write-Host "       git push origin v$Version"
Write-Host "  3. crear el release en GitHub adjuntando:"
Write-Host "       dist\TeatroPlayer-Instalador.exe"
Write-Host "       dist\TeatroPlayer-portable.zip"
Write-Host "       THIRD-PARTY.html"
Write-Host "     y enlazando al tag (con eso cumples la obligacion de codigo fuente de la GPL-3.0)"
Write-Host "  4. manifiesto winget en installer\winget\: rellenar InstallerUrl y InstallerSha256,"
Write-Host "     y abrir el PR en microsoft/winget-pkgs"
Write-Host ""
exit 0
