# Build reproducible (T-REL-006).
#
# Objetivo: que dos personas (o la misma en dos días) puedan compilar el mismo
# commit y obtener un binario comparable. No se pretende que sea idéntico byte
# a byte —eso con MSVC no es realista— sino que NO haya diferencias por rutas
# de la máquina ni por la hora del reloj, que son las que hacen imposible
# comparar nada.
#
# Lo que se fija:
#   SOURCE_DATE_EPOCH = 0      -> ninguna marca de tiempo del build
#   --remap-path-prefix        -> las rutas absolutas del compilador se
#                                 reescriben a ".", así que C:\Users\pepito
#                                 no acaba dentro del binario.
#
# Uso:
#   powershell -ExecutionPolicy Bypass -File scripts\repro-build.ps1
#   powershell -File scripts\repro-build.ps1 -Limpiar   (fuerza recompilación)

param(
    [switch]$Limpiar
)

$ErrorActionPreference = "Stop"

$raiz = Split-Path -Parent $PSScriptRoot
Push-Location $raiz

$env:SOURCE_DATE_EPOCH = "0"
# Prefijo de remapeo: la ruta de la carpeta del proyecto -> "."
$remap = "$raiz=."
$env:RUSTFLAGS = "--remap-path-prefix=$remap"

Write-Host "SOURCE_DATE_EPOCH = $env:SOURCE_DATE_EPOCH"
Write-Host "RUSTFLAGS         = $env:RUSTFLAGS"

if ($Limpiar) {
    Write-Host "limpiando target/release/teatroplayer* ..."
    Remove-Item "target\release\teatroplayer.exe" -ErrorAction SilentlyContinue
    Remove-Item "target\release\teatroplayer.pdb" -ErrorAction SilentlyContinue
}

# `cargo` puede no estar en el PATH si el script se lanza desde un sitio que no
# es un shell con el entorno de Rust cargado (pasa al lanzarlo desde un editor o
# desde otra herramienta de automatización). Se busca explícitamente.
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) {
    $candidato = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    if (Test-Path $candidato) { $cargo = $candidato }
}
if (-not $cargo) {
    Write-Host "FALLO: no se encuentra cargo. Añade ~/.cargo/bin al PATH o instala Rust."
    Pop-Location
    exit 1
}

Write-Host "cargo        : $cargo"
& $cargo build --release
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }

$exe = "target\release\teatroplayer.exe"
$hash = (Get-FileHash $exe -Algorithm SHA256).Hash
$tam = (Get-Item $exe).Length

Write-Host ""
Write-Host "ejecutable : $exe"
Write-Host "tamaño     : $([math]::Round($tam/1MB,2)) MB"
Write-Host "sha256     : $hash"

# Se deja constancia en un archivo para poder comparar entre máquinas.
$salida = "target\release\repro-build.txt"
"sha256=$hash" | Set-Content $salida
"bytes=$tam"   | Add-Content $salida
"rustc=$(rustc --version)" | Add-Content $salida
Write-Host "guardado   : $salida"

# Aviso honesto sobre lo que NO se puede garantizar.
Write-Host ""
Write-Host "NOTA: con MSVC el enlazador incrusta una marca de tiempo y una"
Write-Host "      dirección de carga, así que el hash puede variar entre"
Write-Host "      máquinas aunque el código sea el mismo. Lo que este script"
Write-Host "      garantiza es que la variación NO viene de la ruta del"
Write-Host "      proyecto ni de la hora del reloj."

Pop-Location
exit 0
