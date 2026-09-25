//! Contrato del motor de audio (T-ENG-001).
//!
//! El trait existe por una sola razón: el día que haga falta multicanal real,
//! plugins o MIDI, el backend se reimplementa sobre otro motor **sin tocar la
//! UI ni el modelo de sesión**. No es una abstracción defensiva contra rodio:
//! ADR-002 (ver `Docs/02-stack-tecnico.md` §7) ya decidió que el backend es
//! rodio y que no escribimos un mezclador propio.
//!
//! Todos los métodos toman `&self`: el backend vive detrás de un `Arc` y lo
//! usan a la vez el hilo de la UI y el de audio, así que la mutabilidad va por
//! dentro.

use std::time::Duration;

use anyhow::Result;

use super::model::{AudioSource, CueSpec, Curve};

/// Una salida de audio que el sistema expone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputInfo {
    /// Identificador estable dentro de la sesión actual.
    ///
    /// Ojo: en Windows los endpoints se renumeran al enchufar audífonos, así
    /// que este id **no** se debe guardar en la sesión (riesgo R4).
    pub id: String,
    /// Nombre legible para el selector.
    pub name: String,
    /// true si es la salida predeterminada del sistema ahora mismo.
    pub is_default: bool,
}

/// Qué salida abrir.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum OutputSelection {
    /// La que el sistema tenga marcada como predeterminada.
    ///
    /// Es la opción recomendada y la única fiable para seguir a la ficha de
    /// audífonos: en Windows no existe un dispositivo enumerable llamado
    /// "Auriculares" (verificado en T-SPIKE-001, riesgo R4).
    #[default]
    SystemDefault,
    /// Un endpoint concreto, por el `id` de [`OutputInfo`].
    ById(String),
}

/// Estado de una pista.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackState {
    /// Sonando (o en el fade de entrada).
    Playing,
    /// Congelada donde estaba. **No** es un estado final: al reanudar sigue
    /// por la misma muestra, con el fade por la mitad si estaba a medias.
    Paused,
    /// Bajando porque alguien pidió un fade out.
    FadingOut,
    /// Cortada antes de terminar.
    Stopped,
    /// Llegó al final del audio.
    Finished,
    /// Falló. El mensaje es para mostrárselo al operador con el nombre del
    /// archivo: los errores de decodificación no se silencian (FR-11).
    Failed(String),
}

impl TrackState {
    /// true si ya no va a sonar más.
    pub fn is_done(&self) -> bool {
        matches!(self, TrackState::Stopped | TrackState::Finished | TrackState::Failed(_))
    }
}

/// Manejador de una pista que está sonando.
///
/// `Send + Sync` es obligatorio: la UI lo usa desde su hilo mientras el audio
/// corre en el de rodio.
pub trait TrackHandle: Send + Sync {
    fn state(&self) -> TrackState;

    /// Posición dentro del audio. Monótona mientras suena.
    fn position(&self) -> Duration;

    /// Duración total, si el audio la conoce. Un loop infinito no la tiene.
    fn duration(&self) -> Option<Duration>;

    /// Ganancia de golpe. Puede hacer clic: para bajar, usar `fade_out`.
    fn set_gain(&self, gain: f32);

    /// Rampa hasta `gain` en `duration`.
    fn fade_to(&self, gain: f32, duration: Duration, curve: Curve);

    /// Baja hasta el silencio en `duration`. Para "duck", usar `fade_to`.
    fn fade_out(&self, duration: Duration, curve: Curve);

    /// Hace fade out y corta la pista al terminar.
    ///
    /// Es lo que hace el botón SALIR del modo Función.
    ///
    /// El corte cae **cuando la rampa se ha recorrido**, medido en muestras: si
    /// la pista se pone en pausa a media bajada, se queda a media bajada y no
    /// se corta sola por mucho que pase el reloj.
    fn stop_after(&self, fade: Duration, curve: Curve);

    /// Congela la pista donde está.
    ///
    ///Es distinto de `stop`: la pista sigue montada, con su posición y su
    /// envolvente intactas, y vuelve con `resume`. Vale igual para un loop que
    /// para un efecto de una sola pasada.
    fn pause(&self);

    /// Sigue donde se quedó.
    fn resume(&self);

    fn is_paused(&self) -> bool;

    /// Ganancia que se está aplicando ahora mismo (0.0–1.0), sin contar el
    /// volumen de la pista. Sirve para que el operador **vea** el fade: si la
    /// barra se mueve, el fade va; si salta de 0 a 1, no hay fade.
    fn gain(&self) -> f32;

    /// Corte inmediato.
    fn stop(&self);
}

/// Salud del stream, para T-ENG-005 y para el diagnóstico en la UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub open: bool,
    /// Errores reportados por el callback del stream desde que se abrió.
    pub stream_errors: u64,
    /// Veces que el backend reabrió el stream él solo.
    pub reopens: u64,
}

/// Motor de audio.
pub trait AudioBackend: Send + Sync {
    /// Salidas disponibles ahora mismo.
    fn outputs(&self) -> Result<Vec<OutputInfo>>;

    /// Abre una salida. Si ya había una abierta, la cierra primero.
    fn open(&self, selection: OutputSelection) -> Result<()>;

    fn is_open(&self) -> bool;

    /// Cierra la salida y para todo.
    fn close(&self) -> Result<()>;

    /// Suena una entrada del espectáculo. Devuelve el manejador de la pista.
    fn play(&self, spec: &CueSpec, source: AudioSource) -> Result<Box<dyn TrackHandle>>;

    /// Para todas las pistas.
    fn stop_all(&self);

    /// Congela todas las pistas vivas. Las que ya sonaban siguen montadas.
    fn pause_all(&self);

    /// Reanuda todas las pistas congeladas.
    fn resume_all(&self);

    /// Limita el máster para que la mezcla nunca pase de 0 dBFS (T-ENG-004).
    fn set_master_limit(&self, enabled: bool);

    /// Pistas vivas ahora mismo.
    fn active_tracks(&self) -> usize;

    /// Salud del stream.
    fn health(&self) -> Health;
}
