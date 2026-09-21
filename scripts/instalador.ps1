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
$makensis = Get-Command makensis -ErrorAction SilentlyContinue |
    Select-Object -ExpandProperty Source

if (-not $makensis) {
    foreach ($candidato in @(
        "C:\Program Files (x86)\NSIS\makensis.exe",
        "C:\Program Files\NSIS\makensis.exe"
    )) {
        if (Test-Path $candidato) { $makensis = $candidato; break }
    }
}

if (-not $makensis) {
    Write-Host "No se encuentra NSIS."
    Write-Host ""
    Write-Host "Para compilar el instalador hace falta NSIS 3:"
    Write-Host "  winget install NSIS.NSIS"
    Write-Host "  o  https://nsis.sourceforge.io/Download"
    Write-Host ""
    Write-Host "El script del instalador está escrito y revisado en"
    Write-Host "installer\teatroplayer.nsi; sólo falta la herramienta."
    Pop-Location
    exit 2
}

Write-Host "makensis : $makensis"
& $makensis "installer\teatroplayer.nsi"
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }

$salida = "target\release\TeatroPlayer-Instalador.exe"
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
