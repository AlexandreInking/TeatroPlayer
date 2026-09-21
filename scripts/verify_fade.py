#!/usr/bin/env python3
r"""Verificador offline de fades y crossfades (T-SPIKE-003 / T-SPIKE-004).

Analiza los WAV que genera `tp-spike render-fade` / `render-xfade` y comprueba
las propiedades que importan en teatro:

  fade  - arranca en silencio real (no hay clic de arranque)
        - la envolvente sube de forma monótona
        - no hay saltos entre muestras por encima de la pendiente natural del
          audio (eso seria un clic)
        - al terminar la rampa se llega a la amplitud completa

  xfade - la energia (RMS) se mantiene constante durante todo el crossfade:
          con curva equal-power g1^2 + g2^2 = 1, asi que no aparece el bache
          de volumen que si aparece con una rampa lineal
        - al principio solo suena A, al final solo suena B

Solo usa la biblioteca estandar. `wave` no sirve: los WAV de rodio son
IEEE-float de 32 bits (formato 3) y el modulo `wave` no los lee, asi que
parseamos la cabecera RIFF a mano.

Uso:
    python scripts/verify_fade.py fade  target/spike/fade_100ms.wav --fade-ms 100
    python scripts/verify_fade.py fade  target/spike/fade_60s.wav   --fade-ms 60000
    python scripts/verify_fade.py xfade target/spike/xfade_5s.wav   --xfade-secs 5
"""

import argparse
import math
import struct
import sys

# Los fixtures son senos puros generados por scripts/gen_fixtures.py
AMP = 0.5      # amplitud de los tonos de prueba
FREQ_A = 440.0
FREQ_B = 660.0


# --------------------------------------------------------------------------
# Lectura de WAV IEEE-float
# --------------------------------------------------------------------------

def read_wav_f32(path):
    """Devuelve (sample_rate, channels, samples) de un WAV float de 32 bits."""
    with open(path, "rb") as fh:
        raw = fh.read()

    if raw[0:4] != b"RIFF" or raw[8:12] != b"WAVE":
        raise SystemExit(f"{path}: no es un WAV RIFF")

    pos = 12
    fmt = None
    fmt_body = b""
    data = None
    while pos + 8 <= len(raw):
        cid = raw[pos:pos + 4]
        size = struct.unpack_from("<I", raw, pos + 4)[0]
        body = raw[pos + 8:pos + 8 + size]
        if cid == b"fmt ":
            fmt = struct.unpack_from("<HHIIHH", body, 0)
            fmt_body = body
        elif cid == b"data":
            data = body
        pos += 8 + size + (size & 1)

    if fmt is None or data is None:
        raise SystemExit(f"{path}: faltan los chunks fmt o data")

    audio_format, channels, sample_rate, _, _, bits = fmt

    # hound escribe WAVE_FORMAT_EXTENSIBLE (0xFFFE) con el GUID del formato
    # real al final del chunk `fmt `.
    if audio_format == 0xFFFE:
        if len(fmt_body) < 40:
            raise SystemExit(f"{path}: chunk fmt extensible truncado")
        subformat = fmt_body[24:40]
        ieee_float = bytes([0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
                            0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71])
        if subformat != ieee_float:
            raise SystemExit(f"{path}: subformato extensible no es IEEE float")
        audio_format = 3

    if audio_format != 3 or bits != 32:
        raise SystemExit(f"{path}: se esperaba float de 32 bits, "
                         f"hay formato {audio_format} de {bits} bits")

    n = len(data) // 4
    samples = list(struct.unpack_from(f"<{n}f", data, 0))
    return sample_rate, channels, samples


def mono(samples, channels):
    """Si el WAV es multicanal, mezcla a mono promediando."""
    if channels == 1:
        return samples
    out = []
    for i in range(0, len(samples) - channels + 1, channels):
        out.append(sum(samples[i:i + channels]) / channels)
    return out


# --------------------------------------------------------------------------
# Mediciones
# --------------------------------------------------------------------------

def moving_peak(x, window):
    """Pico por ventana deslizante (envolvente)."""
    out = []
    for start in range(0, len(x) - window + 1, window):
        chunk = x[start:start + window]
        out.append(max(max(chunk), -min(chunk)))
    return out


def moving_rms(x, window):
    """RMS por ventana (energia)."""
    out = []
    for start in range(0, len(x) - window + 1, window):
        chunk = x[start:start + window]
        out.append(math.sqrt(sum(v * v for v in chunk) / len(chunk)))
    return out


def peak_of(x, start, window):
    """Pico de la señal en [start, start + window)."""
    chunk = x[start:start + window]
    if not chunk:
        return 0.0
    return max(max(chunk), -min(chunk))


def max_step(x):
    """Mayor salto entre dos muestras consecutivas."""
    return max(abs(x[i + 1] - x[i]) for i in range(len(x) - 1))


def slope_limit(freqs, amp, sample_rate):
    """Pendiente maxima natural de la suma de senos, con margen del 100%.

    d/dt[A*sin(2*pi*f*t)] maximo es A*2*pi*f ; por muestra, /sample_rate.
    Cualquier salto por encima de esto es un clic, no audio.
    """
    natural = sum(amp * 2 * math.pi * f / sample_rate for f in freqs)
    return 2.0 * natural


def plot(values, title, height=12, width=100):
    """Perfil ASCII para mirar la forma de la envolvente."""
    # Submuestreo a lo ancho de la terminal: con ventanas de 200 muestras un
    # audio de 6 s daria mas de 1300 columnas.
    if len(values) > width:
        step = len(values) / width
        values = [max(values[int(i * step):int((i + 1) * step)] or [0.0])
                  for i in range(width)]

    lo, hi = 0.0, max(values) or 1.0
    print(f"  {title}")
    for row in range(height, 0, -1):
        level = lo + (hi - lo) * row / height
        bar = "".join("#" if v >= level else " " for v in values)
        print(f"  {level:6.3f} |{bar}")
    print(f"         +{'-' * len(values)}")
    print(f"          0{' ' * (len(values) - 8)}{len(values)} ventanas")


# --------------------------------------------------------------------------
# Comandos
# --------------------------------------------------------------------------

def check(ok, label, detail):
    print(f"  [{'OK ' if ok else 'FALLO'}] {label}: {detail}")
    return ok


def cmd_fade(path, fade_ms):
    sr, ch, raw = read_wav_f32(path)
    x = mono(raw, ch)
    print(f"T-SPIKE-003 — {path}")
    print(f"  {sr} Hz / {ch} canal(es) / {len(x)} muestras "
          f"({len(x) / sr:.2f} s)")
    print(f"  fade solicitado: {fade_ms} ms")

    fade_frames = int(round(fade_ms / 1000.0 * sr))
    window = max(8, int(round(sr / FREQ_A)) * 2)   # 2 periodos del tono
    env = moving_peak(x, window)

    # Arranca en silencio: la primerisima muestra vale ~0. Si arrancara en
    # silencio "a medias" apareceria el clic tipico de un fade mal hecho.
    ok_start = abs(x[0]) < 1e-3

    # La envolvente sigue la curva equal-power: g(t) = sin(pi/2 * t).
    # Se mide al 25%, 50% y 75% de la rampa y se compara con la teoria.
    curve_rows = []
    ok_curve = True
    if fade_frames + window <= len(x):
        for p in (0.25, 0.50, 0.75):
            start = int(p * fade_frames)
            # El pico de la ventana corresponde a la ganancia al final de ella.
            frac = (start + window) / fade_frames
            expected = AMP * math.sin(math.pi / 2 * frac)
            measured = peak_of(x, start, window)
            rel = abs(measured - expected) / expected
            ok_curve = ok_curve and rel < 0.10
            curve_rows.append((p, measured, expected, rel))
    else:
        ok_curve = None   # la rampa no cabe en el archivo: no se puede medir

    # Monotonia dentro de la rampa (tolerancia por el redondeo de la ventana).
    ramp_win = max(1, min(fade_frames // window, len(env)))
    ramp = env[:max(1, ramp_win)]
    drops = [b - a for a, b in zip(ramp, ramp[1:]) if b - a < -0.004]
    ok_mono = not drops

    # Anti-clic: ningun salto por encima de la pendiente natural.
    limit = slope_limit([FREQ_A], AMP, sr)
    step = max_step(x)
    ok_step = step < limit

    # Al final de la rampa se llega a la amplitud completa.
    if fade_frames + window <= len(x):
        tail = peak_of(x, fade_frames, window)
        ok_end = abs(tail - AMP) < AMP * 0.03
        end_detail = f"{tail:.5f} (~{AMP}) tras {fade_ms} ms"
    else:
        ok_end = None
        end_detail = "la rampa no cabe en el archivo (se valida en los tests Rust)"

    # Nunca se supera la amplitud original (no hay recorte).
    peak = max(max(x), -min(x))
    ok_peak = peak <= AMP * 1.02

    # El perfil solo cubre la rampa (mas un poco despues): si se dibujara el
    # archivo entero, un fade de 100 ms sobre 6 s ocuparia una sola columna.
    plot(env[:min(len(env), fade_frames // window + 3)], "envolvente (zona de la rampa)")
    print()
    results = [
        check(ok_start, "arranca en silencio", f"x[0] = {x[0]:.6f}"),
        check(ok_step, "sin clics",
              f"salto maximo {step:.5f} (< {limit:.5f})"),
        check(ok_mono, "envolvente monótona",
              f"{len(drops)} bajadas dentro de la rampa"),
        check(ok_peak, "sin recorte", f"pico {peak:.5f} (<= {AMP * 1.02:.5f})"),
    ]
    if curve_rows:
        print("  curva equal-power (medido vs teorico):")
        for p, measured, expected, rel in curve_rows:
            print(f"    {p * 100:>3.0f}% de la rampa: {measured:.5f} vs "
                  f"{expected:.5f}  (dif {rel * 100:.2f}%)")
        print()
        results.append(check(ok_curve, "sigue la curva equal-power",
                             "los 3 puntos dentro del 10%"))
    if ok_end is not None:
        results.append(check(ok_end, "llega a amplitud completa", end_detail))
    print()
    if all(results):
        print(f"PASA: fade de {fade_ms} ms correcto")
        return 0
    print(f"FALLA: fade de {fade_ms} ms")
    return 1


def cmd_xfade(path, xfade_secs):
    sr, ch, raw = read_wav_f32(path)
    x = mono(raw, ch)
    print(f"T-SPIKE-004 — {path}")
    print(f"  {sr} Hz / {ch} canal(es) / {len(x)} muestras "
          f"({len(x) / sr:.2f} s)")
    print(f"  crossfade solicitado: {xfade_secs} s")

    # Ventana larga: 0.1 s, ~44 periodos de 440 Hz y ~66 de 660 Hz, para que
    # el RMS no dependa de donde corte la ventana.
    window = int(round(sr / 10))
    rms = moving_rms(x, window)

    # Equal-power: g1^2 + g2^2 = 1 en todo momento -> RMS constante.
    # (Con rampa lineal el RMS caeria un 29% en el centro.)
    ref = sum(rms) / len(rms) if rms else 0.0
    worst = max(abs(v - ref) for v in rms) if rms else 1.0
    variation = worst / ref if ref else 1.0
    ok_flat = variation < 0.02

    # Al principio solo A (440 Hz), al final solo B (660 Hz).
    head = dominant_freq(x[:window], sr)
    tail = dominant_freq(x[-window:], sr)
    ok_a = abs(head - FREQ_A) < 5.0
    ok_b = abs(tail - FREQ_B) < 5.0

    # Anti-clic tambien aqui.
    limit = slope_limit([FREQ_A, FREQ_B], AMP, sr)
    step = max_step(x)
    ok_step = step < limit

    plot(rms, "RMS por ventana (debe ser plano)")
    print()
    results = [
        check(ok_flat, "energia constante (equal-power)",
              f"variacion {variation * 100:.2f}% (< 2.00%)"),
        check(ok_a, "empieza sonando A",
              f"{head:.1f} Hz (~{FREQ_A})"),
        check(ok_b, "termina sonando B",
              f"{tail:.1f} Hz (~{FREQ_B})"),
        check(ok_step, "sin clics",
              f"salto maximo {step:.5f} (< {limit:.5f})"),
    ]
    print()
    if all(results):
        print(f"PASA: crossfade de {xfade_secs} s correcto")
        return 0
    print(f"FALLA: crossfade de {xfade_secs} s")
    return 1


def dominant_freq(chunk, sample_rate):
    """Frecuencia dominante por DFT directa en el rango 100-1000 Hz.

    Es fuerza bruta y solo corre sobre 0.1 s de audio; no merece una FFT.
    """
    n = len(chunk)
    best_f, best_mag = 0.0, -1.0
    for f in range(100, 1001):
        wr = 2 * math.pi * f / sample_rate
        re = sum(chunk[i] * math.cos(wr * i) for i in range(n))
        im = sum(chunk[i] * math.sin(wr * i) for i in range(n))
        mag = math.hypot(re, im)
        if mag > best_mag:
            best_mag, best_f = mag, float(f)
    return best_f


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_fade = sub.add_parser("fade", help="verifica un fade in")
    p_fade.add_argument("wav")
    p_fade.add_argument("--fade-ms", type=float, required=True)

    p_x = sub.add_parser("xfade", help="verifica un crossfade A -> B")
    p_x.add_argument("wav")
    p_x.add_argument("--xfade-secs", type=float, required=True)

    args = ap.parse_args()
    if args.cmd == "fade":
        return cmd_fade(args.wav, args.fade_ms)
    return cmd_xfade(args.wav, args.xfade_secs)


if __name__ == "__main__":
    sys.exit(main())
