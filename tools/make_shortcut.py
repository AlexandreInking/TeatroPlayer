#!/usr/bin/env python3
r"""Crea un acceso directo de Windows (.lnk).

Utilidad de desarrollo / empaquetado. El plan usa esto en T-REL-002 para que el
instalador deje el icono en el escritorio.

Requiere pywin32:
    python -m pip install pywin32

Uso:
    python tools/make_shortcut.py --link TeatroPlayer.lnk ^
        --target C:\...\target\release\teatroplayer.exe ^
        --workdir C:\...\TeatroPlayer --desc "TeatroPlayer"
"""
import argparse
import os
import sys


def main() -> int:
    p = argparse.ArgumentParser(description="Crea un acceso directo .lnk de Windows")
    p.add_argument("--link", required=True, help="ruta del .lnk a crear")
    p.add_argument("--target", required=True, help="ruta del ejecutable destino")
    p.add_argument("--workdir", default=None, help="directorio de trabajo")
    p.add_argument("--desc", default="", help="descripcion del acceso directo")
    p.add_argument("--icon", default="shell32.dll,14", help="icono, formato 'archivo,indice'")
    args = p.parse_args()

    try:
        import win32com.client
    except ImportError:
        print("Falta pywin32. Instalalo con: python -m pip install pywin32", file=sys.stderr)
        return 1

    link = os.path.abspath(args.link)
    target = os.path.abspath(args.target)
    workdir = os.path.abspath(args.workdir or os.path.dirname(target))

    os.makedirs(os.path.dirname(link) or ".", exist_ok=True)

    ws = win32com.client.Dispatch("WScript.Shell")
    sc = ws.CreateShortcut(link)
    sc.TargetPath = target
    sc.WorkingDirectory = workdir
    sc.IconLocation = args.icon
    sc.Description = args.desc
    sc.WindowStyle = 1
    sc.Save()

    print(f"Acceso directo creado: {link}")
    print(f"  destino : {target}")
    print(f"  workdir : {workdir}")
    if not os.path.exists(target):
        print(f"  AVISO: el destino todavia no existe; el acceso directo funcionara "
              f"cuando se compile el binario.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())