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

/// Versión del formato de sesión que escribe esta build.
pub const VERSION: u32 = 1;

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
}

impl Default for Sesion {
    fn default() -> Self {
        Self { version: VERSION, nombre: "Nueva obra".to_string(), cues: Vec::new() }
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

/// Cadena de migraciones. Por ahora sólo hay v1, así que no hay ninguna
/// registrada; el mecanismo queda listo para cuando aparezca la v2.
///
/// Cada migración recibe el JSON crudo y devuelve el JSON migrado: así puede
/// trabajar con campos que ya no existen en el modelo actual.
pub fn migrar(valor: serde_json::Value) -> Result<serde_json::Value, String> {
    let version = valor.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

    let mut actual = valor;
    if version < VERSION {
        // Aquí se encadenarán las migraciones cuando exista la v2:
        //   if version == 1 { actual = migrar_v1_a_v2(actual)?; }
        return Err(format!("no hay migración de la versión {version} a la {VERSION}"));
    }

    if let Some(obj) = actual.as_object_mut() {
        obj.insert("version".to_string(), serde_json::Value::from(VERSION));
    }
    Ok(actual)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::model::MilliDb;
    use std::time::Duration;

    use crate::engine::model::{Curve, Entrance, LoopMode, OnPrevious};

    fn sesion_de_prueba() -> Sesion {
        let mut s = Sesion::nueva();
        s.nombre = "Mi obra".to_string();
        let mut cue = Cue::nuevo("Ambiente", "01_ambiente.wav");
        cue.spec.entrance = Entrance::FadeIn {
            duration: Duration::from_millis(4000),
            curve: Curve::EqualPower,
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
    fn las_duraciones_se_guardan_en_milisegundos() {
        let mut cue = Cue::nuevo("x", "x.wav");
        cue.spec.entrance =
            Entrance::FadeIn { duration: Duration::from_millis(1500), curve: Curve::Linear };
        let json = serde_json::to_string(&cue).unwrap();
        assert!(json.contains("1500"), "json: {json}");
        let cue2: Cue = serde_json::from_str(&json).unwrap();
        assert_eq!(
            cue2.spec.entrance,
            Entrance::FadeIn { duration: Duration::from_millis(1500), curve: Curve::Linear }
        );
    }
}
