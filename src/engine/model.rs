//! Modelo de datos que consume el motor de audio.
//!
//! Espejo de `Docs/04-modelo-de-datos.md` (el JSON de `sesion.json`), pero solo
//! con lo que el motor necesita para reproducir una entrada.
//!
//! **Sin floats.** Todo es entero: un `f32` en el modelo haría que dos
//! guardados del mismo espectáculo produjeran archivos distintos (ver
//! `Docs/12-determinismo-y-sin-ia.md`). Los decibelios se guardan en
//! milidecibelios y los niveles en porcentaje.

use std::io::Cursor;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use crate::engine::envelope::Curve;

/// Extensiones de audio que sabemos decodificar (Symphonia vía rodio).
///
/// Vive aquí y no en la UI para que el empaquetador (`.tpshow`) y la interfaz
/// usen exactamente la misma lista.
pub const EXTENSIONES_AUDIO: [&str; 6] = ["wav", "mp3", "flac", "ogg", "m4a", "aac"];

/// Volumen en **milidecibelios**: `-3000` = `-3.0 dB`.
///
/// Entero, exacto y ordenable, así que se puede usar como clave de `BTreeMap`
/// y comparar sin tolerancias.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct MilliDb(pub i32);

impl MilliDb {
    /// 0 dB = sin cambio de volumen.
    pub const ZERO: Self = Self(0);
    /// Silencio práctico.
    pub const MUTE: Self = Self(-120_000);

    pub fn from_db(db: f32) -> Self {
        Self((db * 1000.0).round() as i32)
    }

    pub fn as_db(self) -> f32 {
        self.0 as f32 / 1000.0
    }

    /// Factor lineal de ganancia. `0 dB` -> `1.0`, `-6 dB` -> `0.501`.
    pub fn as_linear(self) -> f32 {
        10f32.powf(self.as_db() / 20.0)
    }
}

/// Cómo entra el audio cuando se dispara.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Entrance {
    /// De golpe, a volumen pleno.
    #[default]
    Hit,
    /// Rampa de `from_percent` a `to_percent`.
    ///
    /// Los dos extremos son configurables a propósito: un fade no tiene por
    /// qué ir siempre del silencio al 100 %. Un ambiente puede entrar de 0 a
    /// 60 %, y una escena que se va puede bajar de 80 a 20 en vez de apagarse
    /// del todo. Antes los extremos eran fijos (0 y 100) y cualquier otra cosa
    /// había que simularla bajando el volumen de la pista, que no es lo mismo:
    /// el volumen se aplica antes y no cambia la forma de la rampa.
    ///
    /// Ambos llevan `#[serde(default)]`: una sesión antigua, que sólo guardaba
    /// `durationMs` y `curve`, se abre como un fade 0 → 100 %, que es exactamente
    /// lo que hacía antes.
    FadeIn {
        #[serde(rename = "durationMs", with = "crate::serde_util::ms")]
        duration: Duration,
        #[serde(default)]
        curve: Curve,
        /// Volumen al empezar la rampa, 0–100 %.
        #[serde(default, rename = "fromPercent")]
        from_percent: u8,
        /// Volumen al terminar la rampa, 0–100 %.
        #[serde(default = "cien", rename = "toPercent")]
        to_percent: u8,
    },
}

/// 100 %, para los `#[serde(default = …)]` del modelo.
pub(crate) const fn cien() -> u8 {
    100
}

/// Qué le pasa a la pista que estaba sonando cuando entra esta.
///
/// Es **independiente** de `Entrance`: esa independencia es la que permite
/// todas las combinaciones que pide el usuario (A entra con fade y B entra de
/// golpe, A entra de golpe y B sale con fade, etc.).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OnPrevious {
    /// Se queda sonando y se encima.
    #[default]
    Keep,
    /// Sale con fade y, si se quiere, yo entro mas tarde.
    ///
    /// `gap` es la espera **desde que la anterior termina su fade** hasta que yo
    /// arranco. Con el sale tal cual la receta "B entra X segundos despues de que
    /// A haya terminado su fade out", sin que el operador calcule nada: el retardo
    /// total es `duration + gap`.
    FadeOut {
        #[serde(rename = "durationMs", with = "crate::serde_util::ms")]
        duration: Duration,
        curve: Curve,
        /// Espera despues de que la anterior termine su fade.
        #[serde(default, rename = "gapMs", with = "crate::serde_util::ms")]
        gap: Duration,
    },
    /// Corte seco inmediato.
    Stop,
    /// Baja a un nivel y se queda ahí (típico para una voz sobre un ambiente).
    Duck {
        #[serde(rename = "durationMs", with = "crate::serde_util::ms")]
        duration: Duration,
        curve: Curve,
        /// Nivel al que baja, 0–100 %.
        #[serde(rename = "duckLevelPercent")]
        level_percent: u8,
    },
}

/// Cómo sale el audio cuando el operador lo manda a salir.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExitMode {
    /// Corte seco.
    Hit,
    /// Sale con fade.
    FadeOut {
        #[serde(rename = "durationMs", with = "crate::serde_util::ms")]
        duration: Duration,
        curve: Curve,
    },
    /// Se queda hasta que el audio termine solo.
    #[default]
    UntilEnd,
}

/// Repetición del audio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoopMode {
    #[default]
    None,
    Infinite,
    Count(u32),
}

// `LoopMode` se serializa a mano porque en el JSON es `{"mode":"count","count":3}`
// y serde no soporta variantes nuevas etiquetadas internamente.
impl Serialize for LoopMode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(Some(2))?;
        match self {
            LoopMode::None => m.serialize_entry("mode", "none")?,
            LoopMode::Infinite => m.serialize_entry("mode", "infinite")?,
            LoopMode::Count(n) => {
                m.serialize_entry("mode", "count")?;
                m.serialize_entry("count", n)?;
            }
        }
        m.end()
    }
}

impl<'de> Deserialize<'de> for LoopMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Forma {
            #[serde(default)]
            mode: String,
            #[serde(default)]
            count: u32,
        }
        let f = Forma::deserialize(d)?;
        Ok(match f.mode.as_str() {
            "infinite" => LoopMode::Infinite,
            "count" => LoopMode::Count(f.count.max(2)),
            _ => LoopMode::None,
        })
    }
}

/// De dónde salen los bytes del audio.
///
/// `File` sirve mientras el espectáculo está suelto en el disco; `Memory` para
/// los tests y para audios pequeños; en el hito FMT se añadirá la variante que
/// lee de dentro del `.tpshow` sin extraer a disco.
#[derive(Clone, Debug)]
pub enum AudioSource {
    /// Un archivo suelto en el disco.
    File(PathBuf),
    /// Bytes en memoria (tests, y audios pequeños).
    Memory(Cursor<Vec<u8>>),
    /// Una entrada dentro de un `.tpshow`. No se extrae a disco: se lee con
    /// `ZipEntryReader` en modo STORE, así que el paquete sigue siendo un
    /// único archivo portable (hito FMT).
    Paquete { tpshow: PathBuf, entrada: String },
}

/// Una entrada del espectáculo: todo lo que el motor necesita para sonarla.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CueSpec {
    /// Volumen de la pista.
    ///
    /// Todos los campos son opcionales al leer: una sesión escrita a mano, o de
    /// una versión anterior, se abre igual y lo que falte queda en su valor por
    /// defecto. Es la regla de `Docs/04` §3: **la sesión nunca se abre rota**.
    #[serde(default, rename = "volumeDb", with = "crate::serde_util::db")]
    pub volume: MilliDb,
    /// Punto del audio por donde arrancar.
    #[serde(default, rename = "startAtMs", with = "crate::serde_util::ms")]
    pub start_at: Duration,
    #[serde(default)]
    pub entrance: Entrance,
    #[serde(default)]
    pub on_previous: OnPrevious,
    #[serde(default)]
    pub exit: ExitMode,
    #[serde(default)]
    pub loop_mode: LoopMode,
    /// Si se indica, la pista se acaba sola pasados esos milisegundos **desde
    /// que arranca**, sin que el operador tenga que darle a SALIR.
    ///
    /// Existe para los eventos con una duración cerrada (un fade out que
    /// termina en silencio, el lado que se apaga de un crossfade): sin esto la
    /// pista seguiría sonando en silencio hasta el final del archivo, gastando
    /// una pista del mezclador para nada.
    #[serde(default, rename = "stopAfterMs", with = "crate::serde_util::ms_opt")]
    pub stop_after: Option<Duration>,
}

impl CueSpec {
    /// Cuánto tarda esta entrada en empezar a sonar desde que se dispara.
    ///
    /// Solo hay retardo cuando la anterior sale con fade: primero baja ella y, si
    /// se pidio hueco, espero ese hueco antes de entrar yo.
    pub fn retardo_de_entrada(&self) -> Duration {
        match self.on_previous {
            OnPrevious::FadeOut { duration, gap, .. } => duration + gap,
            _ => Duration::ZERO,
        }
    }

    /// Entrada mínima: entra de golpe, se encima, y suena hasta el final.
    pub fn simple() -> Self {
        Self::default()
    }

    /// Entrada que hace una rampa de `desde`% a `hasta`% en `duration`.
    ///
    /// Es la que construyen los eventos: un evento es, en el fondo, una o dos
    /// de estas con una duración común.
    pub fn con_rampa(desde: u8, hasta: u8, duration: Duration, curve: Curve) -> Self {
        Self {
            entrance: Entrance::FadeIn {
                duration,
                curve,
                from_percent: desde,
                to_percent: hasta,
            },
            ..Self::default()
        }
    }

    /// Los dos extremos de la rampa de entrada, en tanto por uno (0.0–1.0).
    ///
    /// Con `Entrance::Hit` no hay rampa: el audio entra ya a pleno volumen, así
    /// que los dos extremos son 1.0.
    pub fn extremos_de_entrada(&self) -> (f32, f32) {
        match self.entrance {
            Entrance::FadeIn { from_percent, to_percent, .. } => {
                (from_percent as f32 / 100.0, to_percent as f32 / 100.0)
            }
            Entrance::Hit => (1.0, 1.0),
        }
    }

    /// Ganancia estática que hay que aplicar antes de la envolvente.
    ///
    /// El volumen va **antes** del envolvente y no dentro de él: así el fade
    /// trabaja siempre sobre 0..1 y el volumen de la pista no altera la forma
    /// de la curva.
    pub fn static_gain(&self) -> f32 {
        self.volume.as_linear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millidbconvierte_bien() {
        assert_eq!(MilliDb::from_db(-3.0), MilliDb(-3000));
        assert!((MilliDb::ZERO.as_linear() - 1.0).abs() < 1e-6);
        assert!((MilliDb::from_db(-6.0).as_linear() - 0.501187).abs() < 1e-4);
        assert!(MilliDb::MUTE.as_linear() < 1e-5);
    }

    #[test]
    fn el_volumen_no_es_un_float_en_el_modelo() {
        // Garantiza serialización determinista: el espec es Eq y Ord.
        let a = CueSpec { volume: MilliDb(-3000), ..CueSpec::default() };
        let b = CueSpec { volume: MilliDb(-3000), ..CueSpec::default() };
        assert_eq!(a, b);
    }

    #[test]
    fn los_valores_por_defecto_son_los_mas_simples() {
        let c = CueSpec::simple();
        assert_eq!(c.entrance, Entrance::Hit);
        assert_eq!(c.on_previous, OnPrevious::Keep);
        assert_eq!(c.exit, ExitMode::UntilEnd);
        assert_eq!(c.loop_mode, LoopMode::None);
        assert_eq!(c.stop_after, None);
    }

    #[test]
    fn un_fade_puede_ir_de_cualquier_porcentaje_a_cualquiera() {
        let c = CueSpec::con_rampa(20, 60, Duration::from_millis(2500), Curve::Linear);
        assert_eq!(c.extremos_de_entrada(), (0.2, 0.6));

        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"fromPercent\":20"), "json: {json}");
        assert!(json.contains("\"toPercent\":60"), "json: {json}");

        let vuelta: CueSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(vuelta, c, "el round-trip no es exacto");
    }

    #[test]
    fn un_fade_antiguo_sin_porcentajes_se_abre_como_cero_a_cien() {
        // Sesiones guardadas antes de que existieran `fromPercent`/`toPercent`:
        // deben abrir como un fade completo, que es lo que hacían entonces.
        let json = r#"{"kind":"fadeIn","durationMs":4000,"curve":"linear"}"#;
        let e: Entrance = serde_json::from_str(json).expect("debe abrir igual");
        assert_eq!(
            e,
            Entrance::FadeIn {
                duration: Duration::from_millis(4000),
                curve: Curve::Linear,
                from_percent: 0,
                to_percent: 100,
            }
        );
    }

    #[test]
    fn el_corte_automatico_se_guarda_en_milisegundos_y_puede_no_estar() {
        let mut c = CueSpec::simple();
        c.stop_after = Some(Duration::from_millis(3200));
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"stopAfterMs\":3200"), "json: {json}");

        let vuelta: CueSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(vuelta.stop_after, Some(Duration::from_millis(3200)));

        // Y una sesión sin el campo abre con `None`, sin reventar.
        let vieja: CueSpec = serde_json::from_str("{}").unwrap();
        assert_eq!(vieja.stop_after, None);
    }

    #[test]
    fn un_golpe_no_tiene_rampa() {
        // `Hit` entra a pleno volumen: los dos extremos valen 1.0.
        assert_eq!(CueSpec::simple().extremos_de_entrada(), (1.0, 1.0));
    }
}
