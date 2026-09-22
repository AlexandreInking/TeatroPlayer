#!/usr/bin/env python3
r"""Muestra a dónde apunta un acceso directo de Windows (.lnk).

Compañero de `make_shortcut.py`: sirve para responder a la pregunta "el icono
del escritorio me abre la versión anterior, ¿a qué ejecutable está apuntando?".

Ojo: el acceso directo lanza el **binario de release**, así que después de
tocar el código hay que `cargo build --release` o el escritorio seguirá
abriendo la build vieja aunque `Cargo.toml` ya diga otra versión.

Requiere pywin32:
    python -m pip install pywin32

Uso:
    python tools/lnk_info.py TeatroPlayer.lnk
"""
import argparse
import os
import sys


def main() -> int:
    p = argparse.ArgumentParser(description="Muestra los campos de un .lnk")
    p.add_argument("lnk", help="ruta del acceso directo a inspeccionar")
    args = p.parse_args()

    try:
        import win32com.client
    except ImportError:
        print("Falta pywin32. Instalalo con: python -m pip install pywin32", file=sys.stderr)
        return 1

    ruta = os.path.abspath(args.lnk)
    if not os.path.exists(ruta):
        print(f"No existe: {ruta}", file=sys.stderr)
        return 1

    ws = win32com.client.Dispatch("WScript.Shell")
    sc = ws.CreateShortcut(ruta)

    print("Acceso directo   :", ruta)
    print("TargetPath       :", sc.TargetPath)
    print("WorkingDirectory :", sc.WorkingDirectory)
    print("Arguments        :", repr(sc.Arguments))
    print("IconLocation     :", sc.IconLocation)
    print("Description      :", sc.Description)
    print("WindowStyle      :", sc.WindowStyle)

    existe = os.path.exists(sc.TargetPath)
    print("destino existe   :", existe)
    if existe:
        import time

        sello = os.path.getmtime(sc.TargetPath)
        print("destino modificado:", time.strftime("%Y-%m-%d %H:%M", time.localtime(sello)))
        print("tamano destino   :", os.path.getsize(sc.TargetPath), "bytes")
    else:
        print("AVISO: el destino no existe; compila el binario de release.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
