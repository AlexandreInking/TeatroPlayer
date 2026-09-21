#!/usr/bin/env python3
"""Genera el ZIP portable (T-REL-003).

La idea: una carpeta que se copia a un pendrive o a otra máquina y funciona
sin instalar nada. Para que eso sea cierto, el ZIP tiene que llevar **todo lo
que el ejecutable necesita y nada que dependa de esta máquina**:

  teatroplayer.exe   el binario
  portable.txt       marca: si existe, la app no escribe fuera de su carpeta
  LEEME.txt          cómo se usa
  LICENSE            la GPL-3.0, que hay que repartir junto al binario

No incluye `logs/` ni `state.json`: los crea la app al arrancar.

Uso:
    python tools/portable_zip.py [salida.zip]
"""

import os
import sys
import zipfile

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = os.path.join("target", "release", "teatroplayer.exe")

LEEME = """\
TeatroPlayer — versión portable
===============================

1. Descomprime esta carpeta donde quieras (vale un pendrive).
2. Haz doble clic en TeatroPlayer.exe.
3. Para abrir una obra: botón "Abrir…" y elige el archivo .tpshow.

No hace falta instalar nada ni ser administrador. Todo lo que el programa
escribe (logs, y el recuerdo de la última obra) queda DENTRO de esta carpeta,
así que puedes borrarla entera para dejar el equipo limpio.

Si quieres asociar los .tpshow para abrirlos con doble clic, usa el
instalador (installer/teatroplayer.nsi) en vez de esta versión.

TeatroPlayer es software libre bajo GNU GPL v3.0; el texto completo está en
LICENSE.
"""

PORTABLE = """\
Este archivo le dice a TeatroPlayer que está corriendo en modo portable:
no escribirá nada fuera de esta carpeta.

Borrar este archivo NO rompe el programa, pero sí hace que empiece a guardar
su estado junto al ejecutable en la carpeta de instalación en vez de aquí.
"""


def main():
    salida = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        "target", "release", "TeatroPlayer-portable.zip")

    exe = os.path.join(RAIZ, EXE)
    if not os.path.isfile(exe):
        print(f"FALLO: no existe {exe}. Haz `cargo build --release` primero.")
        return 1

    licencia = os.path.join(RAIZ, "LICENSE")
    if not os.path.isfile(licencia):
        print("AVISO: no hay LICENSE en la raíz; el ZIP saldrá sin ella.")
        licencia = None

    print(f"generando {salida}")
    with zipfile.ZipFile(salida, "w", zipfile.ZIP_DEFLATED) as z:
        z.write(exe, "TeatroPlayer.exe")
        z.writestr("portable.txt", PORTABLE)
        z.writestr("LEEME.txt", LEEME)
        if licencia:
            z.write(licencia, "LICENSE")

    tamaño = os.path.getsize(salida) / (1024 * 1024)
    print(f"  {tamaño:.1f} MB")
    with zipfile.ZipFile(salida) as z:
        for n in z.namelist():
            print(f"  - {n}")

    print("\nOK: ZIP portable listo")
    return 0


if __name__ == "__main__":
    sys.exit(main())
