//! Tests offline del motor de envolvente (T-SPIKE-003, T-SPIKE-004, T-SPIKE-007).
//!
//! No necesitan tarjeta de audio ni reloj de pared: trabajan solo con muestras,
//! así que son deterministas y corren en CI. Son la forma permanente de lo que
//! `tp-spike` comprobó una vez en la máquina de desarrollo.
//!
//! Los fixtures son generados por `scripts/gen_fixtures.py` y están en git:
//! tonos puros de 440/660/220 Hz, 6 s, 44100 Hz, 16-bit mono, amplitud 0.5.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rodio::{Decoder, Source};
use teatroplayer::engine::envelope::{Curve, GainRamp};

const AMP: f32 = 0.5;
const FREQ_A: f32 = 440.0;
const FREQ_B: f32 = 660.0;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn decode(name: &str) -> Decoder<BufReader<File>> {
    let path = fixture(name);
    Decoder::new(BufReader::new(File::open(&path).unwrap())).unwrap()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, v| m.max(v.abs()))
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

fn max_step(x: &[f32]) -> f32 {
    x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
}

/// Pendiente máxima natural de la suma de senos, con un 100 % de margen.
/// Cualquier salto por encima de esto es un clic, no sonido.
fn slope_limit(freqs: &[f32], amp: f32, sample_rate: u32) -> f32 {
    2.0 * freqs
        .iter()
        .map(|f| amp * 2.0 * std::f32::consts::PI * f / sample_rate as f32)
        .sum::<f32>()
}

// ---------------------------------------------------------------------------
// T-SPIKE-003 — fades de 0.1 s y de 60 s
// ---------------------------------------------------------------------------

#[test]
fn fade_in_de_100ms_arranca_en_silencio_y_sin_clics() {
    let src = decode("tone_a.wav");
    let sample_rate = src.sample_rate().get();
    assert_eq!(sample_rate, 44100);

    let ramp = GainRamp::new(src, Duration::from_millis(100), 0.0, 1.0, Curve::EqualPower);
    assert_eq!(ramp.ramp_frames(), 4410, "100 ms a 44100 Hz son 4410 frames");

    let samples: Vec<f32> = ramp.collect();

    // Arranca en silencio absoluto: si arrancara a medio volumen, eso es el clic.
    assert!(samples[0].abs() < 1e-3, "x[0] = {}", samples[0]);

    // Ningún salto por encima de la pendiente natural del tono.
    let limit = slope_limit(&[FREQ_A], AMP, sample_rate);
    let step = max_step(&samples);
    assert!(step < limit, "salto {step} >= limite {limit}");

    // Después de la rampa suena a amplitud plena.
    let tail_peak = peak(&samples[4410..]);
    assert!((tail_peak - AMP).abs() < AMP * 0.03, "cola = {tail_peak}");
}

#[test]
fn fade_in_de_60s_sigue_la_curva_equal_power() {
    // Un seno sintético continuo de 62 s: el fixture sólo dura 6 s y repetirlo
    // cortaría a mitad de ciclo, que sí sería un clic real.
    let sine = rodio::source::SineWave::new(FREQ_A)
        .amplify(AMP)
        .take_duration(Duration::from_secs(62));
    let sample_rate = sine.sample_rate().get();

    let ramp = GainRamp::new(sine, Duration::from_secs(60), 0.0, 1.0, Curve::EqualPower);
    let ramp_frames = ramp.ramp_frames() as usize;
    let samples: Vec<f32> = ramp.collect();

    assert!(samples[0].abs() < 1e-3, "x[0] = {}", samples[0]);

    let limit = slope_limit(&[FREQ_A], AMP, sample_rate);
    assert!(max_step(&samples) < limit);

    // La envolvente en 25 / 50 / 75 % de la rampa = sin(pi/2 * t) * AMP.
    let window = (2.0 * sample_rate as f32 / FREQ_A) as usize;
    for &p in &[0.25f32, 0.50, 0.75] {
        let start = (p * ramp_frames as f32) as usize;
        let frac = (start + window) as f32 / ramp_frames as f32;
        let esperado = AMP * (std::f32::consts::FRAC_PI_2 * frac).sin();
        let medido = peak(&samples[start..start + window]);
        let rel = (medido - esperado).abs() / esperado;
        assert!(rel < 0.05, "al {p:.0}%: {medido} vs {esperado} (dif {rel:.4})");
    }
}

// ---------------------------------------------------------------------------
// T-SPIKE-004 — crossfade A -> B
// ---------------------------------------------------------------------------

fn rms_por_ventanas(curve: Curve, window: usize) -> Vec<f32> {
    let dur = Duration::from_secs(5);
    let a = GainRamp::new(decode("tone_a.wav").take_duration(dur), dur, 1.0, 0.0, curve);
    let b = GainRamp::new(decode("tone_b.wav").take_duration(dur), dur, 0.0, 1.0, curve);
    let mixed: Vec<f32> = a.mix(b).collect();
    // `take_duration` corta por spans y puede pasarse unos pocos samples
    // (5 s -> 220507 en vez de 220500), así que la última ventana queda
    // incompleta y su RMS no es comparable: se descarta.
    mixed.chunks(window).filter(|c| c.len() == window).map(rms).collect()
}

#[test]
fn crossfade_equal_power_mantiene_la_energia_constante() {
    // 4410 muestras = 0.1 s: caben ~44 periodos de 440 Hz y ~66 de 660 Hz, así
    // que el RMS no depende de dónde corte la ventana.
    let ventanas = rms_por_ventanas(Curve::EqualPower, 4410);
    assert!(ventanas.len() >= 40, "solo {} ventanas", ventanas.len());

    let max = ventanas.iter().cloned().fold(0.0, f32::max);
    let min = ventanas.iter().cloned().fold(f32::MAX, f32::min);
    let variacion = (max - min) / max;

    assert!(
        variacion < 0.02,
        "la energia varió un {:.2} % durante el cruce (max {max}, min {min})",
        variacion * 100.0
    );
}

#[test]
fn crossfade_lineal_tiene_un_bache_de_volumen() {
    // Este test existe para documentar por qué NO usamos la curva lineal que
    // trae rodio (`fade_in` / `take_crossfade_with`). No es un fallo: es la
    // justificación medida de ADR-002.
    let ventanas = rms_por_ventanas(Curve::Linear, 4410);

    let max = ventanas.iter().cloned().fold(0.0, f32::max);
    let min = ventanas.iter().cloned().fold(f32::MAX, f32::min);
    let variacion = (max - min) / max;

    // Teoría: con rampa lineal la suma de potencias vale (1-t)^2 + t^2, que en
    // el centro cae a 0.5 -> la amplitud cae a 0.707 -> bache de -29.3 %.
    assert!(
        variacion > 0.15,
        "se esperaba un bache claro con la curva lineal y solo varió un {:.2} %",
        variacion * 100.0
    );
}

#[test]
fn el_crossfade_pasa_de_un_tono_al_otro_sin_clics() {
    let dur = Duration::from_secs(5);
    let a = GainRamp::new(decode("tone_a.wav").take_duration(dur), dur, 1.0, 0.0, Curve::EqualPower);
    let b = GainRamp::new(decode("tone_b.wav").take_duration(dur), dur, 0.0, 1.0, Curve::EqualPower);
    let samples: Vec<f32> = a.mix(b).collect();

    let limit = slope_limit(&[FREQ_A, FREQ_B], AMP, 44100);
    let step = max_step(&samples);
    assert!(step < limit, "salto {step} >= limite {limit}");

    // No se recorta: dos tonos de amplitud AMP pueden coincidir en fase, así
    // que el techo del pico es AMP * (g1 + g2), que con equal-power llega a
    // AMP * sqrt(2) en el centro del cruce. Sigue lejos del recorte (1.0).
    let techo = AMP * std::f32::consts::SQRT_2 * 1.02;
    assert!(peak(&samples) <= techo, "pico {} > techo {}", peak(&samples), techo);
}

