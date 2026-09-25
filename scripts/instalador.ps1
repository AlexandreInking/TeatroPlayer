# Compila el instalador NSIS (T-REL-002).
#
# NSIS no es una dependencia de Rust: es una herramienta externa. Este script
# la busca en las rutas habituales y, si no la encuentra, dice exactamente qué
# hay que instalar en vez de fallar con un error críptico.
#
# Uso:
#   powershell -ExecutionPolicy Bypass -File scripts\instalador.ps1

$ErrorActionPreference = "Stop"

$raiz = Split-Path -Parent $PSScriptRoot
Push-Location $raiz

$exe = "target\release\teatroplayer.exe"
if (-not (Test-Path $exe)) {
    Write-Host "FALLO: no existe $exe. Haz primero: cargo build --release"
    Pop-Location
    exit 1
}

if (-not (Test-Path "LICENSE")) {
    Write-Host "FALLO: el instalador incluye la licencia y no hay LICENSE en la raíz."
    Pop-Location
    exit 1
}

# --- buscar makensis -------------------------------------------------------
. "$PSScriptRoot\nsis.ps1"
$makensis = Buscar-Makensis

if (-not $makensis) {
    Write-Host "No se encuentra NSIS."
    Write-Host ""
    Write-Host "Para compilar el instalador hace falta NSIS 3. Dos formas:"
    Write-Host ""
    Write-Host "  Con administrador:"
    Write-Host "    winget install NSIS.NSIS"
    Write-Host ""
    Write-Host "  Sin administrador (portable):"
    Write-Host "    1. Descarga https://nsis.sourceforge.io/Download (nsis-3.x.zip)"
    Write-Host "    2. Descomprimelo donde quieras"
    Write-Host "    3. setx NSIS_DIR ""C:\ruta\a\nsis-3.x"""
    Write-Host ""
    Write-Host "El script del instalador está escrito y revisado en"
    Write-Host "installer\teatroplayer.nsi; sólo falta la herramienta."
    Pop-Location
    exit 2
}

Write-Host "makensis : $makensis"
# `dist\` tiene que existir antes de llamar a makensis: NSIS no crea la carpeta
# de `OutFile` y falla con "Can't open output file", que no dice por qué. En un
# clon recién hecho no está, porque `/dist/` va en `.gitignore`.
New-Item -ItemType Directory -Force -Path "dist" | Out-Null
# `/DRAIZ` con la ruta absoluta: NSIS une las rutas relativas con la carpeta
# del script, así que sin esto el `.nsi` depende de que la estructura de
# carpetas sea exactamente la del repositorio.
& $makensis "/DRAIZ=$raiz" "installer\teatroplayer.nsi"
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }

$salida = "dist\TeatroPlayer-Instalador.exe"
if (Test-Path $salida) {
    $tam = (Get-Item $salida).Length
    Write-Host ""
    Write-Host "instalador : $salida ($([math]::Round($tam/1MB,2)) MB)"
    Write-Host "OK"
} else {
    Write-Host "FALLO: makensis terminó pero no hay $salida"
    Pop-Location
    exit 1
}

Pop-Location
exit 0
