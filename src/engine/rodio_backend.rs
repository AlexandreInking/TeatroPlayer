//! Implementación de `AudioBackend` sobre rodio 0.22 (T-ENG-002, T-ENG-004,
//! T-ENG-005).
//!
//! Topología (ADR-002):
//!
//! ```text
//! DeviceSinkBuilder -> MixerDeviceSink -> [mixer del dispositivo]
//!                                             └── nuestro bus: MixerSource -> Limit
//!                                                      └── Player -> LiveGain -> Amplify -> Decoder
//! ```
//!
//! El bus propio (`rodio::mixer::mixer`) es lo que permite poner un limitador
//! de máster: `MixerDeviceSink` solo expone su mixer de salida, y ahí no hay
//! sitio donde insertar un efecto entre la mezcla y la tarjeta.

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::mixer::{mixer, Mixer};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use rodio::source::LimitSettings;

use super::backend::{AudioBackend, Health, OutputInfo, OutputSelection, TrackHandle, TrackState};
use super::live::{EnvelopeControl, LiveGain};
use super::model::{AudioSource, CueSpec, Curve, Entrance, LoopMode};

/// Cada cuánto el vigilante mira si hay que reabrir el stream (T-ENG-005).
pub const WATCHDOG_INTERVAL: Duration = Duration::from_millis(250);

/// Margen que se espera tras un fade out antes de cortar la pista.
const MARGEN_TRAS_FADE: Duration = Duration::from_millis(60);

/// Ajustes del limitador de máster (T-ENG-004).
///
/// **No** son los de `LimitSettings::default()` y conviene saber por qué. El
/// limitador de rodio es *feed-forward*, sin lookahead: el detector va siempre
/// un poco por detrás de la señal, así que durante el ataque se escapa un
/// sobrepico. Medido con una mezcla de -3 dBFS y +6 dBFS:
///
/// | Ajuste | Pico en el ataque | Pico ya asentado |
/// |---|---|---|
/// | default (-1 dB, 5 ms) | **1.91** recorta | 0.89 |
/// | -1 dB, 50 us | 1.04 recorta | 0.90 |
/// | **-3 dB, 50 us** | **0.85** seguro | 0.71 |
/// | -6 dB, 100 us | 0.77 seguro | 0.51 |
///
/// Con el umbral por defecto el sobrepico **pasa de 0 dBFS**, que es justo lo
/// que el limitador debe impedir. Bajando el umbral a -3 dBFS el sobrepico se
/// queda en 0.85 incluso en el peor caso, y a niveles normales de obra
/// (alrededor de -12 dBFS) el limitador ni actúa.
///
/// -6 dB sería más seguro todavía, pero regala 6 dB de volumen: en teatro se
/// suele necesitar nivel, y 3 dB de margen bastan.
pub fn master_limit_settings() -> LimitSettings {
    LimitSettings::default()
        .with_threshold(-3.0)
        .with_knee_width(4.0)
        .with_attack(Duration::from_micros(50))
        .with_release(Duration::from_millis(100))
}

/// `Mutex::lock` sin `unwrap`: si el mutex está envenenado seguimos con el dato.
/// El plan prohíbe `unwrap()` en errores recuperables (DoD del hito ENG).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `Read + Seek + Send` en un solo trait.
///
/// Hace falta porque Rust no permite `Box<dyn Read + Seek + Send>`: en un trait
/// object solo puede haber **un** trait que no sea auto (`Read`), más los auto
/// traits (`Send`). Con el supertrait, `Seek` deja de contar como adicional.
/// `Sync` también: rodio exige `R: Read + Seek + Send + Sync` para poder
/// mover el decoder entre el hilo de decodificación y el de audio.
pub trait AudioReader: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> AudioReader for T {}

/// Bus de salida: el sink del dispositivo y nuestro mixer, que es donde cuelgan
/// las pistas.
struct Bus {
    /// Hay que **mantenerlo vivo**: al soltarlo, rodio cierra el stream y para
    /// el audio. Por eso no vale con guardar solo el `Mixer`.
    sink: MixerDeviceSink,
    mixer: Mixer,
}

/// Una pista que está sonando.
#[derive(Clone)]
struct RodioTrack {
    player: Arc<Player>,
    control: EnvelopeControl,
    /// Duración del audio, para la cuenta atrás (T-UI-010).
    duracion: Option<Duration>,
    stopped: Arc<AtomicBool>,
    fading: Arc<AtomicBool>,
    failed: Arc<Mutex<Option<String>>>,
}

impl TrackHandle for RodioTrack {
    fn state(&self) -> TrackState {
        if let Some(msg) = lock(&self.failed).clone() {
            return TrackState::Failed(msg);
        }
        if self.stopped.load(Ordering::SeqCst) {
            return TrackState::Stopped;
        }
        if self.fading.load(Ordering::SeqCst) {
            return TrackState::FadingOut;
        }
        if self.player.empty() {
            return TrackState::Finished;
        }
        TrackState::Playing
    }

    fn position(&self) -> Duration {
        self.player.get_pos()
    }

    fn duration(&self) -> Option<Duration> {
        self.duracion
    }

    fn set_gain(&self, gain: f32) {
        self.control.set_gain(gain);
    }

    fn fade_to(&self, gain: f32, duration: Duration, curve: Curve) {
        if gain <= 0.0 {
            self.fading.store(true, Ordering::SeqCst);
        }
        self.control.fade_to(gain, duration, curve);
    }

    fn fade_out(&self, duration: Duration, curve: Curve) {
        self.fading.store(true, Ordering::SeqCst);
        self.control.fade_out(duration, curve);
    }

    fn stop_after(&self, fade: Duration, curve: Curve) {
        self.fading.store(true, Ordering::SeqCst);
        self.control.fade_out(fade, curve);

        // Un hilo por fade: en una función hay unos pocos por minuto, no miles.
        let player = Arc::clone(&self.player);
        let stopped = Arc::clone(&self.stopped);
        let fading = Arc::clone(&self.fading);
        thread::spawn(move || {
            thread::sleep(fade + MARGEN_TRAS_FADE);
            player.stop();
            stopped.store(true, Ordering::SeqCst);
            fading.store(false, Ordering::SeqCst);
        });
    }

    fn stop(&self) {
        self.player.stop();
        self.stopped.store(true, Ordering::SeqCst);
        self.fading.store(false, Ordering::SeqCst);
    }
}

/// Pista viva, con lo que hace falta para reanudarla si el stream se cae.
struct LiveTrack {
    spec: CueSpec,
    source: AudioSource,
    track: RodioTrack,
}

pub struct RodioBackend {
    bus: Mutex<Option<Bus>>,
    tracks: Mutex<Vec<LiveTrack>>,
    selection: Mutex<OutputSelection>,
    limit_enabled: AtomicBool,
    errors: Arc<AtomicU64>,
    reopens: AtomicU64,
    /// Es `Arc` porque el callback de cpal tiene que poder marcarlo sin
    /// capturar `&self`, que no vive lo suficiente.
    needs_reopen: Arc<AtomicBool>,
    /// Para que el vigilante no mantenga vivo el backend.
    me: OnceLock<Weak<Self>>,
}

impl RodioBackend {
    /// Crea el backend y arranca su vigilante de stream.
    ///
    /// Devuelve `Arc` porque el vigilante necesita una referencia débil a sí
    /// mismo para poder reabrir la salida (T-ENG-005).
    pub fn new() -> Arc<Self> {
        let me = Arc::new(Self {
            bus: Mutex::new(None),
            tracks: Mutex::new(Vec::new()),
            selection: Mutex::new(OutputSelection::SystemDefault),
            limit_enabled: AtomicBool::new(true),
            errors: Arc::new(AtomicU64::new(0)),
            reopens: AtomicU64::new(0),
            needs_reopen: Arc::new(AtomicBool::new(false)),
            me: OnceLock::new(),
        });
        let _ = me.me.set(Arc::downgrade(&me));

        let weak = Arc::downgrade(&me);
        thread::spawn(move || loop {
            thread::sleep(WATCHDOG_INTERVAL);
            match weak.upgrade() {
                Some(backend) => {
                    backend.reopen_if_needed();
                }
                // El backend se soltó: el vigilante se va.
                None => break,
            }
        });

        me
    }

    /// Abre (o reabre) el bus. `resume` indica si hay que reanudar las pistas
    /// que estuvieran sonando, que es lo que hace la recuperación automática.
    fn build_bus(&self, selection: &OutputSelection, resume: bool) -> Result<()> {
        let errors = Arc::clone(&self.errors);
        let needs_reopen = Arc::clone(&self.needs_reopen);
        let on_error = move |e: cpal::StreamError| {
            eprintln!("[audio] error de stream: {e}");
            errors.fetch_add(1, Ordering::SeqCst);
            // Aquí mismo se pide la reapertura: el vigilante la ejecuta en
            // menos de WATCHDOG_INTERVAL (T-ENG-005).
            needs_reopen.store(true, Ordering::SeqCst);
        };

        let mut sink = match selection {
            OutputSelection::SystemDefault => DeviceSinkBuilder::from_default_device()?
                .with_error_callback(on_error)
                .open_sink_or_fallback()?,
            OutputSelection::ById(id) => {
                let index: usize =
                    id.parse().map_err(|_| anyhow!("id de salida invalido: {id}"))?;
                let device = cpal::default_host()
                    .output_devices()
                    .map_err(|e| anyhow!("no se pudieron enumerar las salidas: {e}"))?
                    .nth(index)
                    .ok_or_else(|| anyhow!("ya no existe la salida {index}"))?;
                DeviceSinkBuilder::from_device(device)?
                    .with_error_callback(on_error)
                    .open_sink_or_fallback()?
            }
        };
        sink.log_on_drop(false);

        let channels = sink.config().channel_count();
        let sample_rate = sink.config().sample_rate();

        // Bus propio: es el único punto donde cabe un efecto de máster.
        let (mixer, mixer_source) = mixer(channels, sample_rate);
        // Un `MixerSource` sin fuentes devuelve `None` y el mixer del
        // dispositivo lo soltaría para siempre. `Zero` lo mantiene vivo.
        mixer.add(rodio::source::Zero::new(channels, sample_rate));

        if self.limit_enabled.load(Ordering::SeqCst) {
            sink.mixer().add(mixer_source.limit(master_limit_settings()));
        } else {
            sink.mixer().add(mixer_source);
        }

        *lock(&self.bus) = Some(Bus { sink, mixer });
        *lock(&self.selection) = selection.clone();

        if resume {
            self.resume_tracks();
        } else {
            lock(&self.tracks).clear();
        }
        Ok(())
    }

    /// Vuelve a sonar las pistas que no habían terminado, desde donde iban.
    ///
    /// En una recuperación no se repite el fade de entrada: la pista vuelve de
    /// golpe, que es lo menos distraído posible en medio de una función.
    fn resume_tracks(&self) {
        let pendientes: Vec<(CueSpec, AudioSource, Duration)> = {
            let mut tracks = lock(&self.tracks);
            tracks.retain(|t| !t.track.state().is_done());
            tracks
                .iter()
                .map(|t| (t.spec.clone(), t.source.clone(), t.track.position()))
                .collect()
        };
        lock(&self.tracks).clear();

        for (mut spec, source, pos) in pendientes {
            spec.entrance = Entrance::Hit;
            spec.start_at += pos;
            if let Err(e) = self.play(&spec, source) {
                eprintln!("[audio] no se pudo reanudar una pista: {e}");
            }
        }
    }

    /// Reabre el stream si el callback de cpal reportó un error.
    ///
    /// Es la pieza de T-ENG-005 y la llama el vigilante cada
    /// [`WATCHDOG_INTERVAL`], así que la recuperación tarda como mucho eso.
    pub(crate) fn reopen_if_needed(&self) -> bool {
        if !self.needs_reopen.swap(false, Ordering::SeqCst) {
            return false;
        }
        let selection = lock(&self.selection).clone();

        // T-SHOW-006: si la salida elegida ya no existe (se desenchufaron los
        // audífonos, se apagó la interfaz), se cae a la predeterminada del
        // sistema. En teatro lo importante es que vuelva a sonar, no que suene
        // por donde estaba sonando.
        match self.build_bus(&selection, true) {
            Ok(()) => {
                self.reopens.fetch_add(1, Ordering::SeqCst);
                println!(
                    "[audio] stream reabierto; reanudadas {} pistas",
                    self.active_tracks()
                );
                true
            }
            Err(e) => {
                eprintln!("[audio] no se pudo reabrir la salida elegida: {e}");
                match self.build_bus(&OutputSelection::SystemDefault, true) {
                    Ok(()) => {
                        self.reopens.fetch_add(1, Ordering::SeqCst);
                        println!("[audio] reabierto con la salida predeterminada del sistema");
                        true
                    }
                    Err(e2) => {
                        eprintln!("[audio] tampoco con la predeterminada: {e2}");
                        // Queda pedido para el siguiente ciclo del vigilante.
                        self.needs_reopen.store(true, Ordering::SeqCst);
                        false
                    }
                }
            }
        }
    }

    /// Pide una reapertura. En producción lo hace el callback de cpal, que
    /// tiene su propia copia del flag; este método es para los tests.
    #[cfg(test)]
    pub(crate) fn request_reopen(&self) {
        self.needs_reopen.store(true, Ordering::SeqCst);
    }

    /// Simula un error de stream: cuenta el error y pide la reapertura, igual
    /// que haría el callback de cpal. Existe para poder probar T-ENG-005 sin
    /// tener que romper la tarjeta de sonido de verdad.
    #[cfg(test)]
    pub(crate) fn note_stream_error(&self) {
        self.errors.fetch_add(1, Ordering::SeqCst);
        self.request_reopen();
    }

    /// Sample rate y número de canales de la salida abierta.
    ///
    /// La UI lo necesita para dibujar el medidor y para avisar si el audio del
    /// espectáculo no coincide con la tarjeta.
    pub fn output_config(&self) -> Option<(u32, u16)> {
        lock(&self.bus)
            .as_ref()
            .map(|b| (b.sink.config().sample_rate().get(), b.sink.config().channel_count().get()))
    }

    /// Parada de emergencia: baja todo con un fade corto y luego corta
    /// (T-SHOW-003).
    ///
    /// El fade no es adorno: cortar un PCM a mitad de ciclo se oye como un
    /// clic, y en una sala eso suena a fallo. 50 ms basta para que no se oiga.
    pub fn stop_all_con_fade(&self, fade: Duration) {
        for t in lock(&self.tracks).iter() {
            t.track.stop_after(fade, Curve::EqualPower);
        }
    }

    /// Construye la cadena de audio de una entrada, y devuelve el control de
    /// envolvente **del mismo source**: si fueran de sources distintos, los
    /// fades no tendrían efecto.
    fn build_source(
        &self,
        spec: &CueSpec,
        source: AudioSource,
    ) -> Result<(Box<dyn Source<Item = f32> + Send>, EnvelopeControl)> {
        let reader: Box<dyn AudioReader> = match source {
            AudioSource::File(path) => Box::new(BufReader::new(
                File::open(&path)
                    .map_err(|e| anyhow!("no se pudo abrir {}: {e}", path.display()))?,
            )),
            AudioSource::Memory(cursor) => Box::new(cursor),
            AudioSource::Paquete { tpshow, entrada } => Box::new(
                crate::paquete::ZipEntryReader::new(&tpshow, &entrada).map_err(|e| {
                    anyhow!("no se pudo leer '{entrada}' de {}: {e}", tpshow.display())
                })?,
            ),
        };

        let decoded: Box<dyn Source<Item = f32> + Send> = match spec.loop_mode {
            LoopMode::None => Box::new(
                Decoder::new(reader).map_err(|e| anyhow!("no se pudo decodificar: {e}"))?,
            ),
            LoopMode::Infinite => Box::new(
                Decoder::new_looped(reader).map_err(|e| anyhow!("no se pudo decodificar: {e}"))?,
            ),
            LoopMode::Count(n) => {
                let d = Decoder::new(reader).map_err(|e| anyhow!("no se pudo decodificar: {e}"))?;
                match d.total_duration() {
                    Some(total) if n > 0 => Box::new(d.repeat_infinite().take_duration(total * n)),
                    // Sin duración conocida no se puede cortar a las N vueltas:
                    // antes que fallar en función, se queda en loop.
                    _ => Box::new(d.repeat_infinite()),
                }
            }
        };

        // El salto va antes del volumen y de la envolvente: el fade de entrada
        // tiene que empezar donde empieza el audio útil.
        let saltado: Box<dyn Source<Item = f32> + Send> = if spec.start_at > Duration::ZERO {
            Box::new(decoded.skip_duration(spec.start_at))
        } else {
            decoded
        };

        // Volumen estático antes de la envolvente: así el fade trabaja siempre
        // sobre 0..1 y el volumen de la pista no altera la forma de la curva.
        let amplificado = saltado.amplify(spec.static_gain());

        // Corte automático: la pista se acaba sola al llegar a `stop_after`.
        // Va aquí, en la cadena de audio, y no en un temporizador: el corte
        // cae exactamente en una muestra, es determinista y no depende de que
        // la UI esté repintando.
        let cortado: Box<dyn Source<Item = f32> + Send> = match spec.stop_after {
            Some(d) if d > Duration::ZERO => Box::new(amplificado.take_duration(d)),
            _ => Box::new(amplificado),
        };

        let (live, control) = LiveGain::with_entrance(cortado, 1.0, Some(spec.entrance));
        Ok((Box::new(live), control))
    }
}

impl AudioBackend for RodioBackend {
    fn outputs(&self) -> Result<Vec<OutputInfo>> {
        let host = cpal::default_host();
        let default_name = host
            .default_output_device()
            .and_then(|d| d.description().ok())
            .map(|d| d.to_string());

        let devices = host
            .output_devices()
            .map_err(|e| anyhow!("no se pudieron enumerar las salidas de audio: {e}"))?;

        Ok(devices
            .enumerate()
            .map(|(i, d)| {
                let name = d
                    .description()
                    .map(|x| x.to_string())
                    .unwrap_or_else(|_| "<sin nombre>".to_string());
                let is_default = default_name.as_deref() == Some(name.as_str());
                // El id es el índice: los endpoints de Windows se renumeran al
                // enchufar audífonos, así que no hay id estable que guardar
                // (riesgo R4).
                OutputInfo { id: i.to_string(), name, is_default }
            })
            .collect())
    }

    fn open(&self, selection: OutputSelection) -> Result<()> {
        self.build_bus(&selection, false)
    }

    fn is_open(&self) -> bool {
        lock(&self.bus).is_some()
    }

    fn close(&self) -> Result<()> {
        lock(&self.tracks).clear();
        *lock(&self.bus) = None;
        Ok(())
    }

    fn play(&self, spec: &CueSpec, source: AudioSource) -> Result<Box<dyn TrackHandle>> {
        let guard = lock(&self.bus);
        let bus = guard
            .as_ref()
            .ok_or_else(|| anyhow!("no hay ninguna salida de audio abierta"))?;

        let (chain, control) = self.build_source(spec, source.clone())?;
        // Se lee antes de entregar la cadena al Player: con loop infinito no hay
        // duración, y eso está bien (una cuenta atrás ahí no tendría sentido).
        let duracion = chain.total_duration();

        let player = Arc::new(Player::connect_new(&bus.mixer));
        player.append(chain);

        let track = RodioTrack {
            player,
            control,
            duracion,
            stopped: Arc::new(AtomicBool::new(false)),
            fading: Arc::new(AtomicBool::new(false)),
            failed: Arc::new(Mutex::new(None)),
        };

        let mut tracks = lock(&self.tracks);
        tracks.retain(|t| !t.track.state().is_done());
        tracks.push(LiveTrack { spec: spec.clone(), source, track: track.clone() });

        Ok(Box::new(track))
    }

    fn stop_all(&self) {
        for t in lock(&self.tracks).iter() {
            t.track.stop();
        }
        lock(&self.tracks).clear();
    }

    fn set_master_limit(&self, enabled: bool) {
        self.limit_enabled.store(enabled, Ordering::SeqCst);
        // El limitador va en el bus, así que cambiarlo obliga a reconstruirlo.
        // No es una operación de función: se hace al configurar, no en escena.
        if self.is_open() {
            let selection = lock(&self.selection).clone();
            if let Err(e) = self.build_bus(&selection, false) {
                eprintln!("[audio] no se pudo reconstruir el bus: {e}");
            }
        }
    }

    fn active_tracks(&self) -> usize {
        let mut tracks = lock(&self.tracks);
        tracks.retain(|t| !t.track.state().is_done());
        tracks.len()
    }

    fn health(&self) -> Health {
        Health {
            open: self.is_open(),
            stream_errors: self.errors.load(Ordering::SeqCst),
            reopens: self.reopens.load(Ordering::SeqCst),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Si no hay salida de audio, los tests que la necesitan se omiten: así
    /// pueden correr en una máquina sin tarjeta (CI).
    fn salida_disponible() -> bool {
        cpal::default_host().default_output_device().is_some()
    }

    #[test]
    fn sin_abrir_no_se_puede_reproducir() {
        let backend = RodioBackend::new();
        assert!(!backend.is_open());
        let err = backend.play(&CueSpec::simple(), AudioSource::Memory(Cursor::new(Vec::new())));
        assert!(err.is_err(), "debe fallar si no se abrió ninguna salida");
    }

    #[test]
    fn el_estado_inicial_es_cerrado_y_sano() {
        let backend = RodioBackend::new();
        let h = backend.health();
        assert!(!h.open);
        assert_eq!(h.stream_errors, 0);
        assert_eq!(h.reopens, 0);
    }

    #[test]
    fn la_recuperacion_no_hace_nada_si_no_hubo_error() {
        let backend = RodioBackend::new();
        assert!(!backend.reopen_if_needed());
    }

    #[test]
    fn un_error_de_stream_deja_constancia_y_pide_reapertura() {
        let backend = RodioBackend::new();
        backend.note_stream_error();
        backend.note_stream_error();
        assert_eq!(backend.health().stream_errors, 2);
        // Sin salida abierta no se puede reabrir, pero sí queda pedido.
        assert!(backend.needs_reopen.load(Ordering::SeqCst));
    }

    #[test]
    fn abrir_la_salida_predeterminada_y_listar() {
        if !salida_disponible() {
            eprintln!("sin salida de audio en esta máquina: se omite");
            return;
        }
        let backend = RodioBackend::new();
        let salidas = backend.outputs().expect("debe poder enumerar salidas");
        assert!(!salidas.is_empty());

        backend.open(OutputSelection::SystemDefault).expect("debe abrir");
        assert!(backend.is_open());
        assert!(backend.health().open);
        assert_eq!(backend.active_tracks(), 0);

        backend.close().expect("debe cerrar");
        assert!(!backend.is_open());
    }

    #[test]
    fn una_pista_suena_y_se_puede_cortar() {
        if !salida_disponible() {
            eprintln!("sin salida de audio en esta máquina: se omite");
            return;
        }
        let backend = RodioBackend::new();
        backend.open(OutputSelection::SystemDefault).expect("debe abrir");

        let mut spec = CueSpec::simple();
        spec.volume = MilliDb::from_db(-40.0); // casi nada: es un test
        let ruta = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/tone_short.wav");
        let track = backend.play(&spec, AudioSource::File(ruta)).expect("debe sonar");

        assert_eq!(backend.active_tracks(), 1);
        assert!(!track.state().is_done());

        track.stop();
        // El corte no es instantáneo: la cola se vacía en el próximo ciclo.
        std::thread::sleep(Duration::from_millis(200));
        assert!(track.state().is_done(), "estado = {:?}", track.state());
    }

    use std::io::Cursor;
    use std::path::Path;

    use super::super::model::MilliDb;

    /// T-ENG-005: el backend se recupera solo de un error de stream.
    ///
    /// No podemos romper la tarjeta de sonido de verdad, así que se simula el
    /// mismo camino que seguiría el callback de cpal: contar el error y pedir
    /// la reapertura.
    #[test]
    fn un_error_de_stream_se_recupera_solo() {
        if !salida_disponible() {
            eprintln!("sin salida de audio en esta máquina: se omite");
            return;
        }
        let backend = RodioBackend::new();
        backend.open(OutputSelection::SystemDefault).expect("debe abrir");

        let mut spec = CueSpec::simple();
        spec.volume = MilliDb::from_db(-40.0);
        let ruta = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/tone_a.wav");
        let _track = backend.play(&spec, AudioSource::File(ruta)).expect("debe sonar");
        assert_eq!(backend.active_tracks(), 1);

        backend.note_stream_error();
        assert_eq!(backend.health().stream_errors, 1);

        // El vigilante mira cada WATCHDOG_INTERVAL (250 ms). El plan pide
        // recuperar en <= 500 ms; damos 1 s para que el test no sea frágil.
        let mut reabierto = false;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(25));
            if backend.health().reopens >= 1 {
                reabierto = true;
                break;
            }
        }

        assert!(reabierto, "el vigilante no reabrió el stream");
        assert!(backend.is_open(), "debe quedar abierto tras recuperar");
        assert!(
            backend.active_tracks() >= 1,
            "la pista que sonaba debe reanudarse, hay {}",
            backend.active_tracks()
        );

        backend.stop_all();
    }
}
