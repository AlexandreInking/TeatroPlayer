# Localiza makensis (el compilador de NSIS). Devuelve la ruta, o $null.
#
# Lo usan `instalador.ps1` y `publicar.ps1`: antes cada uno tenía su propia
# búsqueda y las dos se olvidaban de la versión portable.
#
# Se busca por este orden:
#
#   1. **`$env:NSIS_DIR`** — la carpeta que contiene `makensis.exe`.
#   2. El **PATH**.
#   3. Las dos carpetas de la instalación normal (la que pide administrador).
#
# El paso 1 es el importante: NSIS se puede usar **sin instalar**. Se descarga
# el ZIP de https://nsis.sourceforge.io/Download, se descomprime donde sea y se
# apunta `NSIS_DIR` ahí. En un equipo de teatro —donde nadie tiene permisos de
# administrador— esa es la diferencia entre poder compilar el instalador o no:
#
#     setx NSIS_DIR "C:\herramientas\nsis-3.10"
#
# (NSIS no se puede instalar desde aquí con `winget install NSIS.NSIS`: pide
# elevación y devuelve 0x800704c7.)
function Buscar-Makensis {
    if ($env:NSIS_DIR) {
        $candidato = Join-Path $env:NSIS_DIR "makensis.exe"
        if (Test-Path $candidato) { return $candidato }
    }

    $enPath = Get-Command makensis -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty Source
    if ($enPath) { return $enPath }

    foreach ($candidato in @(
        "C:\Program Files (x86)\NSIS\makensis.exe",
        "C:\Program Files\NSIS\makensis.exe"
    )) {
        if (Test-Path $candidato) { return $candidato }
    }

    return $null
}
