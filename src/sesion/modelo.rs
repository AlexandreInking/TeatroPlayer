//! Modelo de sesión (T-SES-001).
//!
//! Espejo de `Docs/04-modelo-de-datos.md`. El JSON que se guarda usa
//! `camelCase` porque así está especificado; en Rust usamos `snake_case` y
//! `serde` hace la traducción.
//!
//! **Sin floats en el modelo** (ver `Docs/12`): los decibelios se guardan como
//! `volumeDb` (f64 en el JSON) pero viven como `MilliDb` (i32) en memoria.


use serde::{Deserialize, Serialize};

use crate::engine::model::CueSpec;
use crate::eventos::Evento;

/// Versión del formato de sesión que escribe esta build.
///
/// - La **2** añadió los eventos predefinidos. Se subió de número aunque la
///   migración no tuviera que convertir nada: es lo que hace que una build
///   vieja abra una sesión nueva **en sólo lectura**, en vez de cargarla, no
///   entender la lista de eventos y guardar por encima borrándola.
/// - La **3** cambia qué guarda un evento: el crossfade y el fade out ya no
///   eligen el audio que sale, porque ése es **el que está sonando**. Aquí la
///   migración **sí convierte**: una v2 guardaba el crossfade como
///   `[sale, entra]` y la v3 guarda sólo `entra`, con el volumen objetivo en
///   `salidaPct`. Sin convertir, el audio que salía se leería como el que
///   entra y el evento sonaría al revés.
pub const VERSION: u32 = 3;

/// Cómo se abrió una sesión: normal o sólo lectura (versión futura).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoApertura {
    Normal,
    SoloLectura,
}

// ---------------------------------------------------------------------------
// Ayudas de serialización
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Referencia al audio
// ---------------------------------------------------------------------------

/// Cómo encontrar el archivo de audio de una entrada.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AudioRef {
    /// Nombre dentro de `audio/` del paquete o de la carpeta.
    ///
    /// Todos los campos llevan `#[serde(default)]` a propósito: si un archivo
    /// viene incompleto (escrito a mano, o de una versión anterior), la sesión
    /// se abre igual y la entrada se marca FALTANTE, en vez de reventar
    /// (`Docs/04` §3: "la sesión nunca se abre rota").
    #[serde(default)]
    pub file_name: String,
    /// Ruta relativa a la carpeta de la sesión. Es la que se intenta primero.
    #[serde(default)]
    pub rel_path: String,
    /// Ruta absoluta de donde salió al importar. Se usa si `relPath` falla.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abs_path: Option<String>,
    /// Tamaño en bytes, para reconocer el archivo aunque se renombre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Hash blake3, para reconocerlo aunque se mueva y renombre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

/// Qué pasa cuando termina esta entrada: si arranca la siguiente sola.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutoFollow {
    /// No hace nada: el operador dispara la siguiente.
    #[default]
    None,
    /// Arranca la siguiente pasados N milisegundos.
    AfterMs(u64),
    /// Arranca la siguiente cuando esta termina de sonar.
    ///
    /// Solo tiene sentido si la entrada **no** lleva loop infinito: si no,
    /// nunca termina y nunca dispararía la siguiente.
    WhenThisEnds,
}

// Mismo motivo que `LoopMode`: en el JSON es `{"kind":"afterMs","afterMs":1000}`
// y una variante con datos no se puede etiquetar internamente.
impl serde::Serialize for AutoFollow {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(Some(2))?;
        match self {
            AutoFollow::None => m.serialize_entry("kind", "none")?,
            AutoFollow::WhenThisEnds => m.serialize_entry("kind", "whenThisEnds")?,
            AutoFollow::AfterMs(ms) => {
                m.serialize_entry("kind", "afterMs")?;
                m.serialize_entry("afterMs", ms)?;
            }
        }
        m.end()
    }
}

impl<'de> serde::Deserialize<'de> for AutoFollow {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Forma {
            #[serde(default)]
            kind: String,
            #[serde(default)]
            #[serde(rename = "afterMs")]
            after_ms: u64,
        }
        let f = Forma::deserialize(d)?;
        Ok(match f.kind.as_str() {
            "afterMs" => AutoFollow::AfterMs(f.after_ms),
            "whenThisEnds" => AutoFollow::WhenThisEnds,
            _ => AutoFollow::None,
        })
    }
}

impl AutoFollow {
    /// Un `whenThisEnds` sobre un loop infinito nunca se dispararía: es un
    /// error de configuración que conviene detectar al diseñar, no en función.
    pub fn es_imposible(&self, loop_infinito: bool) -> bool {
        matches!(self, AutoFollow::WhenThisEnds) && loop_infinito
    }
}

// ---------------------------------------------------------------------------
// Entrada (cue)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    #[serde(default)]
    pub nombre: String,
    #[serde(default)]
    pub audio: AudioRef,
    /// Configuración de sonido: cómo entra, qué pasa con lo anterior, etc.
    #[serde(flatten)]
    pub spec: CueSpec,
    /// Tecla que la dispara en modo Función (F1, F2…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tecla: Option<String>,
    /// Nota libre para el operador.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub nota: String,
    /// Si al terminar esta entrada arranca la siguiente sola.
    #[serde(default)]
    pub auto_follow: AutoFollow,
    /// true = aparece también en la franja de pads, disparable a mano al
    /// margen de la secuencia (`Docs/04` §2, B3). No la saca de la lista.
    #[serde(default)]
    pub pad: bool,
}

impl Cue {
    pub fn nuevo(nombre: impl Into<String>, file_name: impl Into<String>) -> Self {
        let file_name = file_name.into();
        Self {
            nombre: nombre.into(),
            audio: AudioRef {
                rel_path: format!("audio/{file_name}"),
                file_name,
                ..Default::default()
            },
            spec: CueSpec::simple(),
            tecla: None,
            nota: String::new(),
            auto_follow: AutoFollow::None,
            pad: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Sesión
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Sesion {
    pub version: u32,
    #[serde(default)]
    pub nombre: String,
    #[serde(default)]
    pub cues: Vec<Cue>,
    /// Eventos predefinidos de la obra: escenas montadas con los audios de
    /// `cues`, guardadas aparte para poder lanzarlas y reutilizarlas.
    ///
    /// `#[serde(default)]` por el motivo de siempre: una sesión escrita a mano
    /// o anterior a los eventos se abre igual, sin eventos.
    #[serde(default)]
    pub eventos: Vec<Evento>,
}

impl Default for Sesion {
    fn default() -> Self {
        Self {
            version: VERSION,
            nombre: "Nueva obra".to_string(),
            cues: Vec::new(),
            eventos: Vec::new(),
        }
    }
}

impl Sesion {
    pub fn nueva() -> Self {
        Self::default()
    }

    /// Resultado de intentar abrir un JSON de sesión.
    pub fn desde_json(texto: &str) -> Result<(Self, ModoApertura), String> {
        // Primero se mira la versión a secas: si es del futuro no se debe
        // tocar el archivo, y si falta, el error tiene que decirlo claro.
        let cabecera: serde_json::Value =
            serde_json::from_str(texto).map_err(|e| format!("el archivo no es JSON válido: {e}"))?;

        let version = cabecera
            .get("version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "el archivo no indica la versión de la sesión (falta \"version\")".to_string())?
            as u32;

        if version > VERSION {
            // Más nueva que esta build: se abre para mirar, no para guardar.
            let sesion: Self = serde_json::from_str(texto).map_err(|e| {
                format!("la sesión es de la versión {version} y no se pudo leer: {e}")
            })?;
            return Ok((sesion, ModoApertura::SoloLectura));
        }

        let migrada = migrar(cabecera)?;
        let sesion: Self = serde_json::from_value(migrada)
            .map_err(|e| format!("la sesión no tiene la forma esperada: {e}"))?;
        Ok((sesion, ModoApertura::Normal))
    }

    pub fn a_json(&self) -> Result<String, String> {
        // `to_string_pretty` para que se pueda leer y comparar a mano; el orden
        // de las claves es el de la struct, así que es estable.
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// T-SES-002 — migraciones
// ---------------------------------------------------------------------------

/// Cadena de migraciones. Cada una recibe el JSON crudo y devuelve el JSON
/// migrado: así puede trabajar con campos que ya no existen en el modelo
/// actual.
pub fn migrar(valor: serde_json::Value) -> Result<serde_json::Value, String> {
    let version = valor.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

    let mut actual = valor;
    if version < VERSION {
        if version == 1 {
            actual = migrar_v1_a_v2(actual)?;
        }
        if version <= 2 {
            actual = migrar_v2_a_v3(actual)?;
        }
        if version < 1 {
            return Err(format!("no hay migración de la versión {version} a la {VERSION}"));
        }
    }

    if let Some(obj) = actual.as_object_mut() {
        obj.insert("version".to_string(), serde_json::Value::from(VERSION));
    }
    Ok(actual)
}

/// v1 → v2: no hay nada que convertir.
///
/// La v2 sólo **añade** la lista de eventos, y los campos nuevos del modelo
/// llevan `#[serde(default)]`, así que el JSON de la v1 ya se interpreta bien
/// tal cual. La migración existe para que la cadena esté completa y el salto
/// quede documentado en el sitio donde se busca.
pub fn migrar_v1_a_v2(valor: serde_json::Value) -> Result<serde_json::Value, String> {
    Ok(valor)
}

/// v2 → v3: los eventos dejan de elegir el audio que sale.
///
/// En la v2, un crossfade guardaba **dos** audios —`[el que sale, el que
/// entra]`— y un fade out guardaba uno. En la v3 el que sale **no se elige**:
/// es el que está sonando en ese momento, y del lado que sale sólo queda el
/// volumen al que va, `salidaPct`.
///
/// La conversión, por evento:
///
/// | Tipo | v2 | v3 |
/// |---|---|---|
/// | `crossfade` | `pistas = [sale, entra]` | `pistas = [entra]`, `salidaPct = sale.hastaPct` |
/// | `fadeOut` | `pistas = [sale]` | `pistas = []`, `salidaPct = sale.hastaPct` |
/// | `fadeIn` / `golpe` | `pistas = [audio]` | igual |
///
/// Se lee del JSON crudo y no del modelo actual a propósito: la v2 tenía campos
/// que ya no existen con ese significado.
pub fn migrar_v2_a_v3(valor: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut raiz = valor;
    let Some(eventos) = raiz.get_mut("eventos").and_then(|e| e.as_array_mut()) else {
        return Ok(raiz);
    };

    for evento in eventos.iter_mut() {
        let tipo = evento
            .get("tipo")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string();

        // 1. El volumen objetivo sale del audio que se iba.
        let salida = evento
            .get("pistas")
            .and_then(|p| p.as_array())
            .and_then(|p| match tipo.as_str() {
                // En la v2 el que salía iba primero.
                "crossfade" if p.len() >= 2 => p[0].get("hastaPct"),
                "fadeOut" => p.first().and_then(|x| x.get("hastaPct")),
                _ => None,
            })
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // 2. Se reescriben las pistas.
        if let Some(pistas) = evento.get_mut("pistas").and_then(|p| p.as_array_mut()) {
            match tipo.as_str() {
                "crossfade" if pistas.len() >= 2 => {
                    // Se queda el que entra, que era el segundo.
                    let entra = pistas[1].clone();
                    pistas.clear();
                    pistas.push(entra);
                }
                "fadeOut" => pistas.clear(),
                _ => {}
            }
        }

        // 3. El volumen objetivo pasa al evento.
        if let Some(obj) = evento.as_object_mut() {
            obj.insert("salidaPct".to_string(), serde_json::Value::from(salida));
        }
    }

    Ok(raiz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::model::MilliDb;
    use std::time::Duration;

    use crate::engine::model::{Curve, Entrance, LoopMode, OnPrevious};
    use crate::eventos::{Evento, TipoEvento};

    fn sesion_de_prueba() -> Sesion {
        let mut s = Sesion::nueva();
        s.nombre = "Mi obra".to_string();
        let mut cue = Cue::nuevo("Ambiente", "01_ambiente.wav");
        cue.spec.entrance = Entrance::FadeIn {
            duration: Duration::from_millis(4000),
            curve: Curve::EqualPower,
            from_percent: 10,
            to_percent: 80,
        };
        cue.spec.on_previous = OnPrevious::Duck {
            duration: Duration::from_millis(500),
            curve: Curve::EqualPower,
            level_percent: 30,
        };
        cue.spec.loop_mode = LoopMode::Infinite;
        cue.spec.volume = MilliDb::from_db(-3.0);
        cue.tecla = Some("F1".to_string());
        cue.nota = "sube despacio".to_string();
        s.cues.push(cue);
        s
    }

    #[test]
    fn round_trip_completo() {
        let s = sesion_de_prueba();
        let json = s.a_json().expect("serializar");
        let (s2, modo) = Sesion::desde_json(&json).expect("deserializar");
        assert_eq!(modo, ModoApertura::Normal);
        assert_eq!(s, s2, "el round-trip no es exacto");
    }

    #[test]
    fn el_json_usa_camel_case_como_dice_la_especificacion() {
        let json = sesion_de_prueba().a_json().unwrap();
        for campo in ["\"version\"", "\"nombre\"", "\"cues\"", "\"fileName\"", "\"relPath\""] {
            assert!(json.contains(campo), "falta {campo} en el JSON:\n{json}");
        }
        // Los campos del spec van aplanados dentro del cue.
        assert!(json.contains("\"entrance\""));
        assert!(json.contains("\"durationMs\""), "las duraciones van en ms:\n{json}");
        assert!(json.contains("\"duckLevelPercent\""));
        assert!(json.contains("\"volumeDb\""));
        assert!(json.contains("\"equalPower\""));
    }

    #[test]
    fn una_sesion_sin_version_da_un_error_claro() {
        let json = r#"{"nombre":"x","cues":[]}"#;
        let err = Sesion::desde_json(json).unwrap_err();
        assert!(err.contains("version"), "el error debe mencionar la versión: {err}");
    }

    #[test]
    fn una_sesion_con_campos_a_medias_se_abre_igual() {
        // Un `sesion.json` escrito a mano, sin relPath ni absPath: la sesión se
        // abre y la entrada queda sin audio, no revienta (Docs/04 §3).
        let json = r#"{"version":1,"nombre":"A mano","cues":[
            {"nombre":"Ambiente","audio":{"fileName":"ambiente.wav"}}
        ]}"#;
        let (s, _) = Sesion::desde_json(json).expect("debe abrir igual");
        assert_eq!(s.cues.len(), 1);
        assert_eq!(s.cues[0].audio.file_name, "ambiente.wav");
        assert_eq!(s.cues[0].audio.rel_path, "", "no inventamos rutas");
        assert!(s.cues[0].audio.size_bytes.is_none());
    }

    #[test]
    fn una_sesion_del_futuro_se_abre_en_solo_lectura() {
        let json = format!(
            r#"{{"version":{},"nombre":"Del futuro","cues":[]}}"#,
            VERSION + 1
        );
        let (s, modo) = Sesion::desde_json(&json).expect("debe abrir igual");
        assert_eq!(modo, ModoApertura::SoloLectura);
        assert_eq!(s.nombre, "Del futuro");
    }

    #[test]
    fn un_json_roto_da_error_no_panico() {
        assert!(Sesion::desde_json("no es json").is_err());
        assert!(Sesion::desde_json("{}").is_err());
    }

    #[test]
    fn el_volumen_se_guarda_en_db_y_vuelve_igual() {
        let mut cue = Cue::nuevo("x", "x.wav");
        cue.spec.volume = MilliDb::from_db(-12.5);
        let json = serde_json::to_string(&cue).unwrap();
        assert!(json.contains("-12.5"), "json: {json}");
        let cue2: Cue = serde_json::from_str(&json).unwrap();
        assert_eq!(cue2.spec.volume, MilliDb::from_db(-12.5));
    }

    #[test]
    fn el_auto_follow_se_guarda_como_after_ms() {
        let mut cue = Cue::nuevo("x", "x.wav");
        cue.auto_follow = AutoFollow::AfterMs(1500);
        let json = serde_json::to_string(&cue).unwrap();
        assert!(json.contains("\"afterMs\""), "json: {json}");
        let cue2: Cue = serde_json::from_str(&json).unwrap();
        assert_eq!(cue2.auto_follow, AutoFollow::AfterMs(1500));
    }

    #[test]
    fn un_when_this_ends_con_loop_infinito_es_imposible() {
        use crate::engine::model::LoopMode;
        assert!(AutoFollow::WhenThisEnds.es_imposible(true));
        assert!(!AutoFollow::WhenThisEnds.es_imposible(false));
        // Con afterMs sí tiene sentido aunque haya loop.
        assert!(!AutoFollow::AfterMs(1000).es_imposible(true));
        let _ = LoopMode::Infinite;
    }

    #[test]
    fn los_eventos_viajan_con_la_sesion() {
        let mut s = sesion_de_prueba();
        let mut e = Evento::nuevo(TipoEvento::Crossfade);
        e.nombre = "Cambio de escena".to_string();
        e.duracion_ms = 6000;
        e.bucle = -1;
        // Sólo el audio que entra: el que sale es el que está sonando.
        e.pistas[0].audio.file_name = "02_viento.wav".to_string();
        e.salida_pct = 0;
        s.eventos.push(e);

        let json = s.a_json().expect("serializar");
        assert!(json.contains("\"eventos\""), "los eventos se guardan:\n{json}");

        let (s2, modo) = Sesion::desde_json(&json).expect("deserializar");
        assert_eq!(modo, ModoApertura::Normal);
        assert_eq!(s2.eventos.len(), 1, "el evento debe volver");
        assert_eq!(s2.eventos[0].nombre, "Cambio de escena");
        assert_eq!(s2.eventos[0].duracion_ms, 6000);
        assert_eq!(s2.eventos[0].bucle, -1);
        assert_eq!(s2, s, "el round-trip no es exacto");
    }

    #[test]
    fn una_sesion_de_la_v1_se_abre_y_se_migra() {
        // Una obra guardada antes de que existieran los eventos: se abre, se
        // puede editar y guardar, y simplemente no tiene eventos.
        let json = r#"{"version":1,"nombre":"Obra vieja","cues":[]}"#;
        let (s, modo) = Sesion::desde_json(json).expect("debe abrir igual");
        assert_eq!(modo, ModoApertura::Normal, "la v1 se migra, no se abre sólo para mirar");
        assert_eq!(s.version, VERSION);
        assert!(s.eventos.is_empty());
        assert_eq!(s.nombre, "Obra vieja");
    }

    #[test]
    fn la_v2_migra_los_eventos_al_modelo_sin_audio_de_salida() {
        // Una sesión tal como la escribía la v2: el crossfade con DOS audios
        // (el que sale primero) y el fade out con uno.
        let json = r#"{
          "version": 2,
          "nombre": "Obra v2",
          "cues": [],
          "eventos": [
            {
              "nombre": "Cambio de escena",
              "tipo": "crossfade",
              "duracionMs": 8000,
              "bucle": -1,
              "pistas": [
                { "nombre": "Música", "desdePct": 100, "hastaPct": 0,
                  "audio": { "fileName": "musica.wav" } },
                { "nombre": "Viento", "desdePct": 0, "hastaPct": 100,
                  "audio": { "fileName": "viento.wav" } }
              ]
            },
            {
              "nombre": "Quitar la música",
              "tipo": "fadeOut",
              "duracionMs": 3000,
              "bucle": -1,
              "pistas": [
                { "nombre": "Música", "desdePct": 100, "hastaPct": 0,
                  "audio": { "fileName": "musica.wav" } }
              ]
            },
            {
              "nombre": "Trueno",
              "tipo": "golpe",
              "pistas": [
                { "nombre": "Trueno", "desdePct": 100, "hastaPct": 100,
                  "audio": { "fileName": "trueno.wav" } }
              ]
            }
          ]
        }"#;

        let (s, modo) = Sesion::desde_json(json).expect("debe migrar y abrir");
        assert_eq!(modo, ModoApertura::Normal);
        assert_eq!(s.version, VERSION);
        assert_eq!(s.eventos.len(), 3);

        // El crossfade se queda sólo con el que entra, y el volumen objetivo
        // del que sale pasa al evento.
        let crossfade = &s.eventos[0];
        assert_eq!(crossfade.pistas.len(), 1, "sólo el que entra");
        assert_eq!(crossfade.pistas[0].audio.file_name, "viento.wav");
        assert_eq!(crossfade.pistas[0].desde_pct, 0);
        assert_eq!(crossfade.pistas[0].hasta_pct, 100);
        assert_eq!(crossfade.salida_pct, 0, "lo que salía acababa en 0 %");

        // El fade out se queda sin audios: actúa sobre lo que suene.
        let fade_out = &s.eventos[1];
        assert!(fade_out.pistas.is_empty(), "un fade out no elige audio");
        assert_eq!(fade_out.salida_pct, 0);
        assert!(fade_out.completo());

        // El disparo único no cambia.
        assert_eq!(s.eventos[2].pistas.len(), 1);
        assert_eq!(s.eventos[2].pistas[0].audio.file_name, "trueno.wav");
    }

    #[test]
    fn la_v2_conserva_el_volumen_objetivo_del_que_sale() {
        // Un crossfade de la v2 que dejaba la música de fondo al 30 %.
        let json = r#"{"version":2,"nombre":"x","cues":[],"eventos":[
          {"tipo":"crossfade","pistas":[
            {"desdePct":100,"hastaPct":30,"audio":{"fileName":"musica.wav"}},
            {"desdePct":0,"hastaPct":100,"audio":{"fileName":"viento.wav"}}
          ]}
        ]}"#;
        let (s, _) = Sesion::desde_json(json).expect("debe migrar");
        assert_eq!(s.eventos[0].salida_pct, 30);
        assert!(!s.eventos[0].apaga_al_salir(), "acaba sonando al 30 %");
    }

    #[test]
    fn una_sesion_sin_eventos_no_lleva_la_clave_vacia() {
        // Sin eventos no se escribe `"eventos": []`: menos ruido en el archivo
        // y una sesión de v2 sigue siendo legible para quien la abra a mano.
        let json = Sesion::nueva().a_json().unwrap();
        assert!(
            json.contains(&format!("\"version\": {VERSION}")),
            "la versión escrita debe ser la de esta build:\n{json}"
        );
    }

    #[test]
    fn las_duraciones_se_guardan_en_milisegundos() {
        let mut cue = Cue::nuevo("x", "x.wav");
        cue.spec.entrance = Entrance::FadeIn {
            duration: Duration::from_millis(1500),
            curve: Curve::Linear,
            from_percent: 0,
            to_percent: 100,
        };
        let json = serde_json::to_string(&cue).unwrap();
        assert!(json.contains("1500"), "json: {json}");
        let cue2: Cue = serde_json::from_str(&json).unwrap();
        assert_eq!(
            cue2.spec.entrance,
            Entrance::FadeIn {
                duration: Duration::from_millis(1500),
                curve: Curve::Linear,
                from_percent: 0,
                to_percent: 100,
            }
        );
    }
}
