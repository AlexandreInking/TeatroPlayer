//! Envolvente de ganancia **controlable mientras suena** (T-ENG-003).
//!
//! `GainRamp` (envelope.rs) decide la curva al construirse y no se puede
//! cambiar: sirve para el fade de entrada. Pero en teatro lo que más se usa es
//! lo contrario: la pista ya está sonando y el operador aprieta SALIR, o entra
//! la siguiente entrada y esta tiene que bajar. Ahí la rampa nace **después**.
//!
//! `LiveGain` resuelve eso sin locks en el hilo de audio:
//!
//! - El hilo de la UI publica una *generación*: escribe los parámetros y
//!   **después** incrementa `seq`.
//! - El hilo de audio compara `seq` con el último que vio. Si cambió, levanta
//!   los parámetros y arranca la rampa **desde la ganancia actual**, así que
//!   nunca hay un salto (y por tanto nunca hay clic).
//! - La ganancia se calcula por índice de frame, nunca con el reloj de pared:
//!   igual que `GainRamp`, sigue siendo determinista.
//!
//! La duración se guarda en **nanosegundos**, no en frames, porque quien pide
//! el fade no conoce el sample rate del audio; lo traduce el source, que sí.

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rodio::{ChannelCount, SampleRate, Source};

use super::envelope::{duration_to_frames, shape_of, Curve};
use super::model::Entrance;

/// Lo que el hilo de la UI escribe y el de audio lee.
struct Shared {
    /// Generación. Se incrementa **al final** de cada publicación.
    seq: AtomicU64,
    /// Ganancia objetivo, en bits de `f32`.
    to: AtomicU32,
    /// Duración de la rampa en nanosegundos. `0` = cambio inmediato.
    ramp_ns: AtomicU64,
    /// Curva, como `Curve::to_u8`.
    curve: AtomicU8,
    /// Última ganancia aplicada. Solo informativo, para la UI.
    last_gain: AtomicU32,
}

/// Rampa vigente, ya traducida a frames.
#[derive(Clone, Copy, Debug)]
struct Ramp {
    from: f32,
    to: f32,
    start_frame: u64,
    ramp_frames: u64,
    curve: Curve,
}

impl Ramp {
    /// Ganancia en silencio total y sin rampa: no hace nada.
    const fn unity() -> Self {
        Self { from: 1.0, to: 1.0, start_frame: 0, ramp_frames: 0, curve: Curve::Linear }
    }

    fn gain_at(&self, frame: u64) -> f32 {
        if self.ramp_frames == 0 {
            return self.to;
        }
        let elapsed = frame.saturating_sub(self.start_frame);
        let t = (elapsed as f32 / self.ramp_frames as f32).clamp(0.0, 1.0);
        let shape = shape_of(self.curve, t, self.to >= self.from);
        self.from + (self.to - self.from) * shape
    }
}

/// Manejador que ve la UI: `Clone`, `Send` y `Sync`, sin genéricos.
#[derive(Clone)]
pub struct EnvelopeControl {
    shared: Arc<Shared>,
}

impl EnvelopeControl {
    /// Cambia la ganancia de golpe. Úsese con cuidado: puede hacer clic.
    pub fn set_gain(&self, gain: f32) {
        self.fade_to(gain, Duration::ZERO, Curve::Linear);
    }

    /// Rampa hasta `gain` en `duration`. Si `duration` es 0, es inmediato.
    pub fn fade_to(&self, gain: f32, duration: Duration, curve: Curve) {
        let gain = gain.clamp(0.0, 1.0);
        self.shared.to.store(gain.to_bits(), Ordering::SeqCst);
        self.shared.ramp_ns.store(duration.as_nanos() as u64, Ordering::SeqCst);
        self.shared.curve.store(curve.to_u8(), Ordering::SeqCst);
        // El incremento va al final: el lector que vea el `seq` nuevo ya verá
        // los parámetros nuevos (todo SeqCst).
        self.shared.seq.fetch_add(1, Ordering::SeqCst);
    }

    /// Fade out total.
    pub fn fade_out(&self, duration: Duration, curve: Curve) {
        self.fade_to(0.0, duration, curve);
    }

    /// Última ganancia aplicada. Aproximada: puede ir un frame por detrás.
    pub fn gain(&self) -> f32 {
        f32::from_bits(self.shared.last_gain.load(Ordering::Relaxed))
    }
}

/// Envuelve un `Source` y le aplica la ganancia que mande `EnvelopeControl`.
pub struct LiveGain<S> {
    inner: S,
    channels: u16,
    sample_rate: u32,
    /// Contador de muestras consumidas (los frames son `sample_idx / channels`).
    sample_idx: u64,
    shared: Arc<Shared>,
    ramp: Ramp,
    seq: u64,
}

impl<S> LiveGain<S>
where
    S: Source<Item = f32>,
{
    /// Envuelve el source arrancando en `start_gain`.
    pub fn new(inner: S, start_gain: f32) -> (Self, EnvelopeControl) {
        Self::with_entrance(inner, start_gain, None)
    }

    /// Arranca según la `Entrance` de la entrada del espectáculo y aplica
    /// `start_gain` como techo. Devuelve el source y el control para la UI.
    pub fn with_entrance(
        inner: S,
        start_gain: f32,
        entrance: Option<Entrance>,
    ) -> (Self, EnvelopeControl) {
        let sample_rate = inner.sample_rate().get();
        let channels = inner.channels().get();

        let ramp = match entrance {
            Some(Entrance::FadeIn { duration, curve }) => Ramp {
                from: 0.0,
                to: start_gain,
                start_frame: 0,
                ramp_frames: duration_to_frames(duration, sample_rate),
                curve,
            },
            _ => Ramp { from: start_gain, to: start_gain, ..Ramp::unity() },
        };

        let shared = Arc::new(Shared {
            seq: AtomicU64::new(0),
            to: AtomicU32::new(ramp.to.to_bits()),
            ramp_ns: AtomicU64::new(0),
            curve: AtomicU8::new(Curve::Linear.to_u8()),
            last_gain: AtomicU32::new(ramp.from.to_bits()),
        });

        let this = Self {
            inner,
            channels,
            sample_rate,
            sample_idx: 0,
            shared: Arc::clone(&shared),
            ramp,
            seq: 0,
        };
        (this, EnvelopeControl { shared })
    }

    /// Frame actual.
    pub fn frame(&self) -> u64 {
        self.sample_idx / self.channels.max(1) as u64
    }

    /// Sample rate del source envuelto.
    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate
    }

    /// Ganancia que corresponde al frame dado, actualizando la rampa si la UI
    /// publicó una nueva.
    fn gain_at(&mut self, frame: u64) -> f32 {
        let seq = self.shared.seq.load(Ordering::SeqCst);
        if seq != self.seq {
            self.seq = seq;
            let to = f32::from_bits(self.shared.to.load(Ordering::SeqCst));
            let ns = self.shared.ramp_ns.load(Ordering::SeqCst);
            let curve = Curve::from_u8(self.shared.curve.load(Ordering::SeqCst));
            let ramp_frames = duration_to_frames(Duration::from_nanos(ns), self.sample_rate);
            // Arrancar desde la ganancia actual es lo que evita el clic.
            let from = self.ramp.gain_at(frame);
            self.ramp = Ramp { from, to, start_frame: frame, ramp_frames, curve };
        }
        let gain = self.ramp.gain_at(frame);
        self.shared.last_gain.store(gain.to_bits(), Ordering::Relaxed);
        gain
    }
}

impl<S> Iterator for LiveGain<S>
where
    S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let frame = self.sample_idx / self.channels.max(1) as u64;
        self.sample_idx += 1;
        Some(sample * self.gain_at(frame))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S> Source for LiveGain<S>
where
    S: Source<Item = f32>,
{
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Source de prueba: valor constante, `n` muestras.
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

    fn mono(frames: usize) -> Const {
        Const { value: 1.0, left: frames, rate: 48_000, channels: 1 }
    }

    #[test]
    fn sin_ordenes_la_ganancia_se_mantiene() {
        let (mut g, _c) = LiveGain::new(mono(4_800), 1.0);
        let s: Vec<f32> = g.by_ref().take(4_800).collect();
        assert!(s.iter().all(|v| (*v - 1.0).abs() < 1e-6));
    }

    #[test]
    fn un_fade_out_en_vivo_baja_hasta_cero() {
        let (mut g, c) = LiveGain::new(mono(48_000 * 3), 1.0);
        let _: Vec<f32> = g.by_ref().take(48_000).collect(); // 1 s sonando

        c.fade_out(Duration::from_secs(1), Curve::EqualPower);

        let after: Vec<f32> = g.by_ref().take(48_000).collect(); // el segundo 1 s
        let first = after[0];
        let last = after[after.len() - 1];
        assert!((first - 1.0).abs() < 0.02, "arranca cerca de 1, vale {first}");
        assert!(last < 1e-4, "termina en silencio, vale {last}");
    }

    #[test]
    fn cambiar_la_ganancia_no_produce_un_salto() {
        // Un clic sería un salto grande entre dos muestras consecutivas.
        let (mut g, c) = LiveGain::new(mono(48_000 * 2), 1.0);
        let mut prev = 1.0f32;
        let mut worst_step = 0.0f32;

        for i in 0..96_000 {
            if i == 40_000 {
                c.fade_out(Duration::from_millis(500), Curve::EqualPower);
            }
            let v = g.next().unwrap();
            worst_step = worst_step.max((v - prev).abs());
            prev = v;
            if i == 20_000 {
                // también un cambio de objetivo en plena rampa
                c.fade_to(0.4, Duration::from_millis(300), Curve::Linear);
            }
        }

        // 500 ms de rampa sobre 48000 Hz: la ganancia baja a lo sumo
        // 1/24000 por muestra. Cualquier salto mayor sería un clic.
        let limite = 1.0 / 12_000.0;
        assert!(
            worst_step < limite,
            "salto maximo {worst_step} >= limite {limite}: hay clic"
        );
    }

    #[test]
    fn la_entrada_con_fade_in_arranca_en_cero() {
        let (mut g, _c) = LiveGain::with_entrance(
            mono(48_000),
            1.0,
            Some(Entrance::FadeIn { duration: Duration::from_secs(1), curve: Curve::EqualPower }),
        );
        let s: Vec<f32> = g.by_ref().take(10).collect();
        assert!(s[0].abs() < 1e-6, "x[0] = {}", s[0]);
        assert!(s[9] < 0.02);
    }

    #[test]
    fn la_entrada_de_golpe_empieza_al_techo() {
        let (mut g, _c) = LiveGain::with_entrance(mono(100), 0.5, Some(Entrance::Hit));
        let s: Vec<f32> = g.by_ref().take(4).collect();
        assert!((s[0] - 0.5).abs() < 1e-6, "x[0] = {}", s[0]);
    }

    #[test]
    fn estereo_cuenta_frames_no_muestras() {
        let src = Const { value: 1.0, left: 8, rate: 48_000, channels: 2 };
        let (mut g, _c) = LiveGain::new(src, 1.0);
        let _ = g.by_ref().take(8).count();
        assert_eq!(g.frame(), 4, "con 2 canales, 8 muestras son 4 frames");
    }

    #[test]
    fn el_control_expone_la_ganancia() {
        let (mut g, c) = LiveGain::new(mono(48_000), 1.0);
        let _ = g.by_ref().take(100).count();
        assert!((c.gain() - 1.0).abs() < 1e-6);

        c.fade_out(Duration::from_millis(100), Curve::Linear);
        let _ = g.by_ref().take(4_800).count(); // 100 ms
        assert!(c.gain() < 1e-3, "gain = {}", c.gain());
    }
}
