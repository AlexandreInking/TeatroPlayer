#!/usr/bin/env python3
"""Verifica que un ejecutable de release está listo para repartir (hito OPS).

Comprueba lo que se puede comprobar sin abrir la app:

  1. Que el ejecutable existe y pesa dentro del presupuesto (Docs/01: 6-12 MB).
  2. Que **no** tiene consola: el subsistema del PE debe ser 2 (GUI), no 3 (CUI).
     Un `.exe` de Windows con consola abre una ventana negra al lado, y eso en
     un teatro es inaceptable (T-OPS-003).
  3. Que la carpeta de logs se crea y el log se escribe (T-OPS-001).

Uso:
    python tools/verificar_release.py [ruta_al_exe]

Con `--dev-console` el subsistema esperado pasa a ser 3, porque esa es justo la
gracia de la feature `dev-console` (T-OPS-004).
"""

import os
import struct
import subprocess
import sys
import time

PRESUPUESTO_MB = (6, 12)

# Subsystem del encabezado opcional del PE (offset 68, igual en PE32 y PE32+).
SUBSISTEMA_GUI = 2
SUBSISTEMA_CONSOLA = 3


def subsistema_pe(ruta):
    """Devuelve el campo Subsystem del encabezado opcional del PE."""
    with open(ruta, "rb") as f:
        datos = f.read(4096)

    if datos[:2] != b"MZ":
        raise ValueError("no es un ejecutable (falta la firma MZ)")

    e_lfanew = struct.unpack_from("<I", datos, 0x3C)[0]
    if datos[e_lfanew:e_lfanew + 4] != b"PE\0\0":
        raise ValueError("no es un PE válido")

    # Encabezado COFF: 20 bytes. SizeOfOptionalHeader está en +16.
    inicio_opcional = e_lfanew + 4 + 20
    return struct.unpack_from("<H", datos, inicio_opcional + 68)[0]


def main():
    exe = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        "target", "release", "teatroplayer.exe")
    espera_consola = "--dev-console" in sys.argv

    if not os.path.isfile(exe):
        print(f"FALLO: no existe {exe}")
        return 1

    tamaño_mb = os.path.getsize(exe) / (1024 * 1024)
    print(f"ejecutable : {exe}")
    print(f"tamaño     : {tamaño_mb:.1f} MB "
          f"(presupuesto {PRESUPUESTO_MB[0]}-{PRESUPUESTO_MB[1]} MB)")

    subsistema = subsistema_pe(exe)
    nombre = {SUBSISTEMA_GUI: "GUI (sin consola)",
              SUBSISTEMA_CONSOLA: "CUI (con consola)"}.get(subsistema, str(subsistema))
    print(f"subsistema : {subsistema} = {nombre}")

    fallos = []

    if not (PRESUPUESTO_MB[0] <= tamaño_mb <= PRESUPUESTO_MB[1]):
        fallos.append(f"tamaño fuera de presupuesto: {tamaño_mb:.1f} MB")

    esperado = SUBSISTEMA_CONSOLA if espera_consola else SUBSISTEMA_GUI
    if subsistema != esperado:
        fallos.append(
            f"subsistema {subsistema}, se esperaba {esperado} "
            f"({'con' if espera_consola else 'sin'} consola)")

    # --- el log (T-OPS-001) -------------------------------------------------
    # Se arranca y se cierra enseguida: sólo queremos ver que crea la carpeta.
    print("\narrancando 3 s para comprobar el log…")
    try:
        p = subprocess.Popen([exe], cwd=os.path.dirname(os.path.abspath(exe)) or ".")
        time.sleep(3)
        p.terminate()
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()
    except OSError as e:
        print(f"  no se pudo arrancar: {e}")
        fallos.append("no se pudo arrancar el ejecutable")

    logs = os.path.join(os.path.dirname(os.path.abspath(exe)), "logs")
    if os.path.isdir(logs):
        archivos = [f for f in os.listdir(logs) if f.endswith(".log")]
        print(f"logs       : {len(archivos)} archivo(s) en {logs}")
        if not archivos:
            fallos.append("la carpeta de logs existe pero está vacía")
        else:
            with open(os.path.join(logs, sorted(archivos)[-1]),
                      encoding="utf-8", errors="replace") as f:
                contenido = f.read()
            if "arrancando" not in contenido:
                fallos.append("el log no tiene la línea de arranque")
            else:
                print("log        : contiene la línea de arranque ✓")
    else:
        fallos.append(f"no se creó la carpeta de logs: {logs}")

    print()
    if fallos:
        print("FALLO:")
        for f in fallos:
            print(f"  - {f}")
        return 1

    print("OK: listo para repartir")
    return 0


if __name__ == "__main__":
    sys.exit(main())
