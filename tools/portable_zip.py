#!/usr/bin/env python3
"""Genera el ZIP portable (T-REL-003).

La idea: una carpeta que se copia a un pendrive o a otra máquina y funciona
sin instalar nada. Para que eso sea cierto, el ZIP tiene que llevar **todo lo
que el ejecutable necesita y nada que dependa de esta máquina**:

  teatroplayer.exe   el binario
  portable.txt       marca para el humano: "esta copia es la portable"
  LEEME.txt          cómo se usa
  LICENSE            la GPL-3.0, que hay que repartir junto al binario

No incluye `logs/` ni `state.json`: los crea la app al arrancar.

El ZIP sale a `dist\`, no a `target\`: `cargo clean` borra `target\` entero y se
llevaría por delante el paquete listo para repartir. `dist\` está en
`.gitignore`, así que el ZIP tampoco entra al repositorio.

`portable.txt` **no cambia el comportamiento del programa**, y conviene no
prometer lo contrario: la app es portable por diseño y escribe siempre junto al
ejecutable (`Estado::ruta()` y `diagnostico::carpeta_logs()`), nunca en
`%APPDATA%` ni en el registro. Por eso el ZIP funciona sin instalar nada. El
archivo queda como señal para quien abre la carpeta, no como interruptor.

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
Esta es la versión portable de TeatroPlayer.

TeatroPlayer no se instala y no escribe nada fuera de su propia carpeta: los
logs y el recuerdo de la última obra se guardan aquí al lado. Puedes copiar
esta carpeta a un pendrive, llevarla a otro equipo y borrarla entera para no
dejar rastro.

Este archivo es sólo una señal de que estás ante la copia portable. Borrarlo no
cambia nada del comportamiento del programa.

Si quieres que los archivos .tpshow se abran con doble clic, usa el instalador
en vez de esta versión: es lo único que registra la asociación.
"""


def main():
    salida = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        "dist", "TeatroPlayer-portable.zip")

    exe = os.path.join(RAIZ, EXE)
    if not os.path.isfile(exe):
        print(f"FALLO: no existe {exe}. Haz `cargo build --release` primero.")
        return 1

    # Los dos avisos legales van SIEMPRE, y si falta alguno el ZIP no sale.
    # Repartir el binario sin ellos incumple la GPL-3.0 (el texto de la
    # licencia tiene que viajar con el programa) y las licencias de las
    # dependencias, que es lo que recoge `THIRD-PARTY.html` (`Docs/11` §5).
    # Un ZIP que "casi" cumple es un ZIP que incumple, así que aquí se para.
    legales = [("LICENSE", "LICENSE"), ("THIRD-PARTY.html", "TERCEROS.html")]
    for origen, destino in legales:
        if not os.path.isfile(os.path.join(RAIZ, origen)):
            print(f"FALLO: falta {origen} en la raíz del repositorio.")
            if origen == "THIRD-PARTY.html":
                print("       Generalo con: cargo about generate about.hbs -o THIRD-PARTY.html")
            return 1

    print(f"generando {salida}")
    # `dist\` puede no existir en un clon recién hecho: se crea, en vez de
    # fallar con un error de ruta que no dice nada.
    os.makedirs(os.path.dirname(os.path.abspath(salida)), exist_ok=True)
    with zipfile.ZipFile(salida, "w", zipfile.ZIP_DEFLATED) as z:
        z.write(exe, "TeatroPlayer.exe")
        z.writestr("portable.txt", PORTABLE)
        z.writestr("LEEME.txt", LEEME)
        for origen, destino in legales:
            z.write(os.path.join(RAIZ, origen), destino)

    tamaño = os.path.getsize(salida) / (1024 * 1024)
    print(f"  {tamaño:.1f} MB")
    with zipfile.ZipFile(salida) as z:
        for n in z.namelist():
            print(f"  - {n}")

    print("\nOK: ZIP portable listo")
    return 0


if __name__ == "__main__":
    sys.exit(main())
