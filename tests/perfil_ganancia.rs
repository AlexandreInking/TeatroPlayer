//! Benchmark del camino caliente de la ganancia (T-PERF-001).
//!
//! `LiveGain::next()` se ejecuta **una vez por muestra** en el hilo de audio.
//! A 48 kHz y 8 pistas simultáneas son ~384 000 llamadas por segundo, así que
//! cada operación de más se multiplica por millones.
//!
//! Como el proyecto es `no_std`-free pero sin dependencias de benchmark
//! (`criterion` arrastraría decenas de crates), se mide con `Instant` a mano.
//! No hace falta precisión de laboratorio: hace falta saber si un cambio
//! **empeora o mejora de forma evidente**.
//!
//! Uso:
//!     cargo test --release --test perfil_ganancia -- --nocapture

use std::time::{Duration, Instant};

use teatroplayer::engine::envelope::Curve;
use teatroplayer::engine::live::LiveGain;
use rodio::{ChannelCount, SampleRate, Source};

/// Source constante, sin coste: así lo que se mide es la envolvente, no el
/// decodificador.
struct Const {
    value: f32,
    left: usize,
    rate: u32,
    channels: u16,
}

impl Iterator for Const {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        Some(self.value)
    }
}

impl Source for Const {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(self.channels).unwrap()
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.rate).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(self.left as f64 / self.rate as f64))
    }
}

/// 60 s de audio estéreo a 48 kHz = 5 760 000 muestras.
const MUESTRAS: usize = 48_000 * 60 * 2;

#[test]
fn perfil_del_camino_caliente() {
    // --- Caso A: rampa larga en curso (el caso normal de una función) -------
    let src = Const { value: 1.0, left: MUESTRAS, rate: 48_000, channels: 2 };
    let (mut g, control) = LiveGain::new(src, 1.0);
    // Un fade de 30 s: toda la medida cae dentro de la rampa, que es cuando
    // `gain_at` tiene trabajo de verdad (la curva, no una constante).
    control.fade_to(0.0, Duration::from_secs(30), Curve::EqualPower);

    let inicio = Instant::now();
    let mut suma = 0.0f32;
    for _ in 0..MUESTRAS {
        suma += g.next().unwrap();
    }
    let a = inicio.elapsed();

    // --- Caso B: sin rampa (ganancia fija) ---------------------------------
    let src2 = Const { value: 1.0, left: MUESTRAS, rate: 48_000, channels: 2 };
    let (mut g2, _c2) = LiveGain::new(src2, 1.0);
    let inicio2 = Instant::now();
    let mut suma2 = 0.0f32;
    for _ in 0..MUESTRAS {
        suma2 += g2.next().unwrap();
    }
    let b = inicio2.elapsed();

    println!("\n=== perfil de la envolvente en vivo ===");
    println!("muestras por caso : {MUESTRAS}");
    println!("A) rampa 30 s     : {a:?}  ({:.1} ns/muestra)", a.as_nanos() as f64 / MUESTRAS as f64);
    println!("B) sin rampa      : {b:?}  ({:.1} ns/muestra)", b.as_nanos() as f64 / MUESTRAS as f64);
    println!("sumas (no optimizar fuera): {suma} {suma2}");

    // El presupuesto real: 128 muestras por callback de cpal a 48 kHz dan
    // 2,67 ms. Con 8 pistas hay que gastar <300 µs en todas ellas, o sea
    // <40 µs por pista y callback. 40 µs / 128 muestras = 312 ns/muestra.
    let ns_muestra = a.as_nanos() as f64 / MUESTRAS as f64;
    println!("\npresupuesto por muestra: 312 ns (8 pistas, callback de 128)");
    println!("margen con rampa       : {:.1}x", 312.0 / ns_muestra);
    assert!(ns_muestra < 312.0, "la envolvente no cabe en el presupuesto de audio");
}
