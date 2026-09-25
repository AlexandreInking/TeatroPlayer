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

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
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
    /// La rampa en curso ya llegó a su final.
    ///
    /// Lo escribe el hilo de audio y lo leen los temporizadores de "fade out y
    /// corta". Es la pieza que hace que una pausa **en medio de un fade** lo
    /// congele de verdad: el flag se mueve con los frames consumidos, no con el
    /// reloj de pared, así que si la pista está en pausa el fade no avanza y
    /// nadie la corta por debajo.
    ramp_done: AtomicBool,
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
    ///
    /// Los tres parámetros se escriben con `Relaxed` y el `seq` con `Release`:
    /// el `Release` garantiza que todo lo escrito antes es visible para quien
    /// lea el `seq` con `Acquire`. Encadenar `SeqCst` en los cuatro sería
    /// correcto pero metería tres barreras de más en cada GO, y esto lo llama
    /// la UI en cada fade.
    pub fn fade_to(&self, gain: f32, duration: Duration, curve: Curve) {
        let gain = gain.clamp(0.0, 1.0);
        self.shared.to.store(gain.to_bits(), Ordering::Relaxed);
        self.shared.ramp_ns.store(duration.as_nanos() as u64, Ordering::Relaxed);
        self.shared.curve.store(curve.to_u8(), Ordering::Relaxed);
        // El flag se publica **antes** del `seq`. Si no, quien preguntara
        // "¿terminó ya?" entre este momento y la primera muestra leería el
        // valor de la rampa anterior y cortaría la pista sin haber hecho el
        // fade: es el caso de pedir un fade out sobre una pista que ya estaba
        // a pleno volumen (rampa previa de duración 0 → terminada).
        self.shared.ramp_done.store(duration <= Duration::ZERO, Ordering::Relaxed);
        // El incremento va al final y con `Release`: el lector que vea el `seq`
        // nuevo ya verá los parámetros nuevos.
        self.shared.seq.fetch_add(1, Ordering::Release);
    }

    /// Fade out total.
    pub fn fade_out(&self, duration: Duration, curve: Curve) {
        self.fade_to(0.0, duration, curve);
    }

    /// Última ganancia aplicada. Aproximada: puede ir un frame por detrás.
    pub fn gain(&self) -> f32 {
        f32::from_bits(self.shared.last_gain.load(Ordering::Relaxed))
    }

    /// true si la rampa que se pidió ya se ha recorrido entera.
    ///
    /// Se mide en **frames consumidos**, no con el reloj de pared: una pista en
    /// pausa no consume frames, así que un fade out a medias sigue a medias
    /// aunque pasen minutos. Es lo que permite cortar la pista justo cuando la
    /// ganancia llega a cero sin depender de un temporizador que siga corriendo
    /// mientras la sala está en pausa.
    pub fn rampa_terminada(&self) -> bool {
        self.shared.ramp_done.load(Ordering::Acquire)
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
    /// Última ganancia calculada y para qué frame. Dentro de un mismo frame la
    /// ganancia no cambia (solo depende del índice de frame), así que en
    /// estéreo se calcula una vez y se reutiliza para el segundo canal.
    frame_cache: (u64, f32),
    /// Último valor publicado de `Shared::ramp_done`, para no escribir el
    /// atómico en cada muestra: sólo cuando cambia.
    done: bool,
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

        // `start_gain` es el techo (1.0 en la app): los porcentajes son
        // fracciones de ese techo, así que el volumen de la pista sigue yendo
        // antes de la envolvente y no altera la forma de la rampa.
        let ramp = match entrance {
            Some(Entrance::FadeIn { duration, curve, from_percent, to_percent }) => Ramp {
                from: start_gain * (from_percent as f32 / 100.0),
                to: start_gain * (to_percent as f32 / 100.0),
                start_frame: 0,
                ramp_frames: duration_to_frames(duration, sample_rate),
                curve,
            },
            _ => Ramp { from: start_gain, to: start_gain, ..Ramp::unity() },
        };

        // Sin rampa (o con una de duración 0) la envolvente ya está "terminada"
        // desde el principio: así un corte pedido sin fade corta ya.
        let done_inicial = ramp.ramp_frames == 0;

        let shared = Arc::new(Shared {
            seq: AtomicU64::new(0),
            to: AtomicU32::new(ramp.to.to_bits()),
            ramp_ns: AtomicU64::new(0),
            curve: AtomicU8::new(Curve::Linear.to_u8()),
            last_gain: AtomicU32::new(ramp.from.to_bits()),
            ramp_done: AtomicBool::new(done_inicial),
        });

        let this = Self {
            inner,
            channels,
            sample_rate,
            sample_idx: 0,
            shared: Arc::clone(&shared),
            ramp,
            seq: 0,
            frame_cache: (0, ramp.from),
            done: done_inicial,
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
    ///
    /// Esto corre **una vez por muestra** en el hilo de audio, así que todo lo
    /// que se pueda sacar de aquí, se saca:
    ///
    ///   * El `seq` se lee con `Acquire`/`Relaxed`, no `SeqCst`. Basta con que
    ///     el lector vea los parámetros *antes* que el `seq`: eso es
    ///     exactamente lo que da `Release` al escribir y `Acquire` al leer, y
    ///     evita la barrera completa (que en x86 es un `lock` real) en cada
    ///     muestra. La corrección no cambia.
    ///   * La ganancia **solo se recalcula cuando cambia de frame**. Un frame
    ///     son `channels` muestras (2 en estéreo), y dentro del mismo frame la
    ///     ganancia es idéntica por construcción (depende solo del índice de
    ///     frame). Se ahorra la mitad de los cálculos de curva en estéreo.
    fn gain_at(&mut self, frame: u64) -> f32 {
        let seq = self.shared.seq.load(Ordering::Acquire);
        if seq != self.seq {
            self.seq = seq;
            // Los tres parámetros se escriben antes del `seq` (Release al
            // publicar), así que aquí ya están visibles.
            let to = f32::from_bits(self.shared.to.load(Ordering::Relaxed));
            let ns = self.shared.ramp_ns.load(Ordering::Relaxed);
            let curve = Curve::from_u8(self.shared.curve.load(Ordering::Relaxed));
            let ramp_frames = duration_to_frames(Duration::from_nanos(ns), self.sample_rate);
            // Arrancar desde la ganancia actual es lo que evita el clic.
            let from = self.ramp.gain_at(frame);
            self.ramp = Ramp { from, to, start_frame: frame, ramp_frames, curve };
            self.frame_cache = (frame, self.ramp.gain_at(frame));
            // Acaba de nacer una rampa: la anterior ya no cuenta como terminada.
            // Si no tiene duración (cambio de golpe), lo está desde ya.
            let done = ramp_frames == 0;
            if done != self.done {
                self.done = done;
                self.shared.ramp_done.store(done, Ordering::Release);
            }
        }

        // Mismo frame que la última vez: la ganancia no ha cambiado. En
        // estéreo esto evita calcular la curva para el canal derecho.
        let gain = if self.frame_cache.0 == frame {
            self.frame_cache.1
        } else {
            let g = self.ramp.gain_at(frame);
            self.frame_cache = (frame, g);
            g
        };

        // ¿La rampa ya recorrió todo? Sólo se escribe cuando cambia: un
        // `store` relajado por muestra en el camino caliente se nota.
        let done = self.ramp.ramp_frames == 0
            || frame >= self.ramp.start_frame + self.ramp.ramp_frames;
        if done != self.done {
            self.done = done;
            self.shared.ramp_done.store(done, Ordering::Release);
        }

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

    /// La optimización de la caché de frame **no puede cambiar ni una muestra**.
    ///
    /// `gain_at` cachea la ganancia del frame en curso para no recalcular la
    /// curva en el segundo canal (estéreo). Ese atajo es correcto porque la
    /// ganancia depende solo del índice de frame, pero es justo el tipo de
    /// optimización que se rompe en silencio si alguien la toca sin entenderla:
    /// se oiría un canal desfasado del otro y nadie se daría cuenta hasta el
    /// estreno. Este test compara canal izquierdo y derecho muestra a muestra:
    /// con una entrada constante, tras multiplicar por la misma ganancia, los
    /// dos canales tienen que ser **exactamente iguales**, bit a bit.
    #[test]
    fn la_cache_de_frame_no_desfasa_los_canales() {
        // 2 s de estéreo con fade de 1 s: toda la rampa queda dentro.
        let src = Const { value: 0.75, left: 48_000 * 2 * 2, rate: 48_000, channels: 2 };
        let (mut g, _c) = LiveGain::with_entrance(
            src,
            1.0,
            Some(Entrance::FadeIn {
                duration: Duration::from_secs(1),
                curve: Curve::EqualPower,
                from_percent: 0,
                to_percent: 100,
            }),
        );

        let muestras: Vec<f32> = g.by_ref().take(48_000 * 2 * 2).collect();
        assert_eq!(muestras.len(), 48_000 * 2 * 2);

        for (i, par) in muestras.as_chunks::<2>().0.iter().enumerate() {
            assert_eq!(
                par[0].to_bits(),
                par[1].to_bits(),
                "frame {i}: canal L y R difieren ({} vs {})",
                par[0],
                par[1]
            );
        }

        // Y la rampa sigue estando: el primer frame está en silencio y a 1 s
        // ya llegó al techo. Sin esto el test pasaría aunque la ganancia
        // estuviera congelada en un valor constante.
        assert!(muestras[0].abs() < 1e-6);
        let a_un_segundo = muestras[48_000 * 2];
        assert!(
            (a_un_segundo - 0.75).abs() < 1e-4,
            "a 1 s debería estar en el techo 0.75, vale {a_un_segundo}"
        );
    }

    #[test]
    fn la_entrada_con_fade_in_arranca_en_cero() {
        let (mut g, _c) = LiveGain::with_entrance(
            mono(48_000),
            1.0,
            Some(Entrance::FadeIn {
                duration: Duration::from_secs(1),
                curve: Curve::EqualPower,
                from_percent: 0,
                to_percent: 100,
            }),
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

    /// `rampa_terminada` se mueve con las muestras, no con el reloj.
    ///
    /// Es la pieza de la que depende "fade out y corta": si avanzara con el
    /// reloj de pared, una pista en pausa se cortaría sola mientras el
    /// operador sigue en silencio. Aquí se comprueba justo lo contrario: sin
    /// consumir muestras, un fade de 1 s **no** termina nunca.
    #[test]
    fn la_rampa_no_termina_si_no_se_consumen_muestras() {
        let (mut g, c) = LiveGain::new(mono(48_000 * 3), 1.0);
        c.fade_out(Duration::from_secs(1), Curve::Linear);

        // Sin muestras consumidas, la rampa está empezada: no puede estar
        // terminada por mucho que pase el tiempo de verdad.
        assert!(!c.rampa_terminada(), "recién pedida, no puede estar terminada");

        // La rampa llega a cero justo en el frame 48.000, así que hacen falta
        // 48.001 muestras para que ese frame se haya consumido.
        let _ = g.by_ref().take(48_000 + 1).count();
        assert!(c.rampa_terminada(), "tras 1 s de muestras debe estar terminada");
    }

    #[test]
    fn un_cambio_de_golpe_esta_terminado_desde_el_principio() {
        let (_g, c) = LiveGain::new(mono(100), 1.0);
        assert!(c.rampa_terminada(), "sin rampa no hay nada que esperar");
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
