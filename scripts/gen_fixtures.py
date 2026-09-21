#!/usr/bin/env python3
"""Genera los audios de prueba para el SPIKE y los tests.

Determinista: mismo comando = mismos bytes, en cualquier maquina.
No depende de nada fuera de la stdlib.

Uso:
    python scripts/gen_fixtures.py
"""
import math
import os
import struct
import wave

OUT_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "tests", "fixtures")
RATE = 44100
AMPL = 0.5           # amplitud pico (evita clipping al mezclar dos pistas)


def write_tone(path: str, freq: float, seconds: float, rate: int = RATE, ampl: float = AMPL) -> None:
    n = int(rate * seconds)
    frames = bytearray()
    for i in range(n):
        # 16-bit PCM, mono, little endian
        v = int(max(-1.0, min(1.0, math.sin(2 * math.pi * freq * i / rate) * ampl)) * 32767)
        frames += struct.pack("<h", v)
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(bytes(frames))
    print(f"{os.path.relpath(path)}  {seconds:.1f}s  {freq:.0f}Hz  {os.path.getsize(path)} bytes")


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    # Tres tonos puros distinguibles: sirven para oir y medir el crossfade A->B.
    write_tone(os.path.join(OUT_DIR, "tone_a.wav"), 440.0, 6.0)
    write_tone(os.path.join(OUT_DIR, "tone_b.wav"), 660.0, 6.0)
    write_tone(os.path.join(OUT_DIR, "tone_c.wav"), 220.0, 6.0)
    # Un tono corto para pruebas de fade de 0.1 s y de baja duracion.
    write_tone(os.path.join(OUT_DIR, "tone_short.wav"), 880.0, 1.0)
    print("fixtures listos en", OUT_DIR)


if __name__ == "__main__":
    main()