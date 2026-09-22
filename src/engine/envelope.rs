//! Envolvente de ganancia (T-SPIKE-007).
//!
//! `GainRamp` envuelve cualquier `rodio::Source` y le aplica una rampa de ganancia
//! calculada **por muestra**, en_frames, nunca con el reloj de pared. Es la base de
//! los fade in, fade out y crossfades del programa.
//!
//! Determinismo: la ganancia depende únicamente del índice de frame y de la curva.
//! Dos ejecuciones con la misma entrada producen exactamente las mismas muestras.

use std::time::Duration;

use rodio::{ChannelCount, SampleRate, Source};

/// Forma de la curva del fade.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Curve {
    /// `g = from + (to - from) * t`. Simple, sirve para fades cortos y evidentes.
    ///
    /// Es la curva **predeterminada** del programa: el cambio de volumen a
    /// velocidad constante es lo que el usuario espera cuando no ha tocado
    /// nada, y lo que la mayoría de los programas de teatro exponen como
    /// única opción. Equal-power sigue disponible para quien la prefiera.
    #[default]
    Linear,
    /// Curva exponencial (k = 4): arranca muy lento y cierra rápido.
    /// Útil para que un audio "aparezca de la nada".
    Exponential,
    /// Equal-power (S): `sin` para subidas, `1 - cos` para bajadas.
    /// En el punto medio ambas valen 0.707, así que la suma de potencias se
    /// mantiene constante y el crossfade no tiene bache de volumen.
    EqualPower,
}

impl Curve {
    /// Código para pasar la curva por un `AtomicU8`.
    pub fn to_u8(self) -> u8 {
        match self {
            Curve::Linear => 0,
            Curve::Exponential => 1,
            Curve::EqualPower => 2,
        }
    }

    /// Inverso de [`Curve::to_u8`]. Cualquier valor desconocido da EqualPower.
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Curve::Linear,
            1 => Curve::Exponential,
            _ => Curve::EqualPower,
        }
    }
}

/// Constante de la curva exponencial.
const EXP_K: f32 = 4.0;

/// Rampa de ganancia sobre un `Source`.
pub struct GainRamp<S> {
    inner: S,
    channels: u16,
    sample_rate: u32,
    /// Contador de muestras consumidas (no de frames).
    sample_idx: u64,
    /// Duración de la rampa en frames. 0 = la rampa no existe.
    ramp_frames: u64,
    from: f32,
    to: f32,
    curve: Curve,
}

impl<S> GainRamp<S>
where
    S: Source,
{
    /// Crea una rampa de `from` a `to` en `duration`, aplicada desde el principio
    /// del source. Después de la rampa, la ganancia queda fija en `to`.
    pub fn new(inner: S, duration: Duration, from: f32, to: f32, curve: Curve) -> Self {
        let sample_rate = inner.sample_rate().get();
        let channels = inner.channels().get();
        let ramp_frames = duration_to_frames(duration, sample_rate);
        Self {
            inner,
            channels,
            sample_rate,
            sample_idx: 0,
            ramp_frames,
            from,
            to,
            curve,
        }
    }

    /// Ganancia en el frame indicado, sin efectos secundarios.
    pub fn gain_at(&self, frame: u64) -> f32 {
        if self.ramp_frames == 0 {
            return self.to;
        }
        let t = (frame as f32 / self.ramp_frames as f32).clamp(0.0, 1.0);
        let shape = shape_of(self.curve, t, self.to >= self.from);
        self.from + (self.to - self.from) * shape
    }

    /// Frame actual (para tests y diagnóstico).
    pub fn frame(&self) -> u64 {
        self.sample_idx / self.channels.max(1) as u64
    }

    /// Sample rate del source envuelto, en Hz.
    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate
    }

    /// Duración de la rampa en frames. `0` = sin rampa.
    pub fn ramp_frames(&self) -> u64 {
        self.ramp_frames
    }
}

/// Forma normalizada de la curva en `t` (0..=1), compartida por `GainRamp` y
/// por el envolvente en vivo (`live::LiveGain`): las dos deben sonar igual.
///
/// `rising` distingue subida de bajada, que en equal-power no son simétricas:
/// la subida usa `sin` y la bajada `1 - cos`, de modo que en el punto medio
/// ambas valen 0.707 y la suma de potencias se mantiene en 1.
pub(crate) fn shape_of(curve: Curve, t: f32, rising: bool) -> f32 {
    match curve {
        Curve::Linear => t,
        Curve::Exponential => exp_shape(t),
        Curve::EqualPower => {
            if rising {
                (std::f32::consts::FRAC_PI_2 * t).sin()
            } else {
                1.0 - (std::f32::consts::FRAC_PI_2 * t).cos()
            }
        }
    }
}

/// Forma de la curva, expuesta para que la interfaz pueda dibujar la rampa
/// exacta que se va a oír, en vez de una recta que se le parece.
///
/// Es la misma función que usa el audio: si se dibujara otra, el dibujo mentiría.
pub fn forma_de_curva(curve: Curve, t: f32, rising: bool) -> f32 {
    shape_of(curve, t, rising)
}

/// `t` -> forma normalizada exponencial, 0..=1.
fn exp_shape(t: f32) -> f32 {
    let denom = EXP_K.exp() - 1.0;
    if denom.abs() < f32::EPSILON {
        return t;
    }
    ((EXP_K * t).exp() - 1.0) / denom
}

/// Convierte una duración a frames. Redondea al frame más cercano: es la unidad
/// de verdad del programa (ver `Docs/12`).
pub(crate) fn duration_to_frames(d: Duration, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }
    let secs = d.as_secs_f64();
    if secs <= 0.0 {
        return 0;
    }
    (secs * sample_rate as f64).round() as u64
}

impl<S> Iterator for GainRamp<S>
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

impl<S> Source for GainRamp<S>
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

    /// Source de prueba: un valor constante, `n` frames de 1 canal.
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

    fn ramp(duration: Duration, from: f32, to: f32, curve: Curve, frames: usize) -> GainRamp<Const> {
        GainRamp::new(
            Const { value: 1.0, left: frames, rate: 48_000, channels: 1 },
            duration,
            from,
            to,
            curve,
        )
    }

    #[test]
    fn linear_sube_de_0_a_1() {
        let r = ramp(Duration::from_secs(1), 0.0, 1.0, Curve::Linear, 48_000);
        assert_eq!(r.gain_at(0), 0.0);
        assert!((r.gain_at(24_000) - 0.5).abs() < 1e-6);
        assert!((r.gain_at(48_000) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn duracion_cero_es_no_op() {
        let r = ramp(Duration::from_millis(0), 0.0, 1.0, Curve::EqualPower, 100);
        assert_eq!(r.gain_at(0), 1.0, "duracion 0 debe dejar la ganancia en el valor final");
    }

    #[test]
    fn equal_power_es_simetrica_en_el_medio() {
        let subida = ramp(Duration::from_secs(2), 0.0, 1.0, Curve::EqualPower, 96_000);
        let bajada = ramp(Duration::from_secs(2), 1.0, 0.0, Curve::EqualPower, 96_000);
        let medio = 48_000;
        let g_in = subida.gain_at(medio);
        let g_out = bajada.gain_at(medio);
        assert!((g_in - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5, "g_in={g_in}");
        assert!((g_out - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5, "g_out={g_out}");
        // Suma de potencias constante: 0.707^2 + 0.707^2 = 1
        assert!((g_in * g_in + g_out * g_out - 1.0).abs() < 1e-5);
    }

    #[test]
    fn exponencial_arranca_lento() {
        let r = ramp(Duration::from_secs(1), 0.0, 1.0, Curve::Exponential, 48_000);
        let lineal = ramp(Duration::from_secs(1), 0.0, 1.0, Curve::Linear, 48_000);
        assert!(r.gain_at(12_000) < lineal.gain_at(12_000));
        assert!((r.gain_at(0) - 0.0).abs() < 1e-6);
        assert!((r.gain_at(48_000) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn la_ganancia_se_satura_despues_de_la_rampa() {
        let r = ramp(Duration::from_millis(100), 0.0, 1.0, Curve::Linear, 48_000);
        assert!((r.gain_at(4800) - 1.0).abs() < 1e-6);
        assert!((r.gain_at(100_000) - 1.0).abs() < 1e-6, "no debe seguir subiendo");
    }

    #[test]
    fn fade_de_60_segundos_cubre_toda_la_rampa() {
        let r = ramp(Duration::from_secs(60), 0.0, 1.0, Curve::EqualPower, 48_000 * 90);
        assert_eq!(r.gain_at(0), 0.0);
        assert!(r.gain_at(48_000 * 30) > 0.6 && r.gain_at(48_000 * 30) < 0.8);
        assert!((r.gain_at(48_000 * 60) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn aplica_la_ganancia_por_muestra() {
        let mut r = ramp(Duration::from_secs(1), 0.0, 1.0, Curve::Linear, 4);
        // rate 48000, 1 canal -> la rampa dura 48000 frames; con 4 muestras
        // la ganancia es practicamente 0.
        let samples: Vec<f32> = r.by_ref().take(4).collect();
        assert_eq!(samples.len(), 4);
        assert!(samples[0] < 1e-4);
        assert!(samples[3] < 1e-3);
        assert_eq!(r.frame(), 4);
    }

    #[test]
    fn estereo_cuenta_frames_no_muestras() {
        let mut r = GainRamp::new(
            Const { value: 1.0, left: 8, rate: 48_000, channels: 2 },
            Duration::from_secs(1),
            0.0,
            1.0,
            Curve::Linear,
        );
        // 8 muestras en 2 canales = 4 frames.
        let _ = r.by_ref().take(8).count();
        assert_eq!(r.frame(), 4, "con 2 canales 8 muestras son 4 frames");
    }
}
