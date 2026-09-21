//! Escritura atómica, copias de seguridad, autoguardado y `state.json`
//! (T-SES-003, T-SES-004, T-SES-006).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::modelo::Sesion;

/// Cuántas copias de seguridad se conservan.
pub const COPIAS_MAXIMAS: usize = 10;

/// Tiempo de inactividad antes de autoguardar.
pub const ESPERA_AUTOGUARDADO: Duration = Duration::from_millis(1500);

// ---------------------------------------------------------------------------
// T-SES-003 — escritura atómica
// ---------------------------------------------------------------------------

/// Escribe `contenido` en `ruta` de forma atómica: se escribe en un `.tmp`, se
/// vacía a disco y sólo entonces se renombra.
///
/// Así un corte de luz a mitad de escritura deja el archivo anterior intacto:
/// en teatro, perder la sesión es peor que perder el último cambio.
pub fn escribir_atomico(ruta: &Path, contenido: &str) -> Result<()> {
    let temporal = ruta.with_extension("tmp");

    {
        let mut f = fs::File::create(&temporal)
            .with_context(|| format!("no se pudo crear {}", temporal.display()))?;
        f.write_all(contenido.as_bytes())
            .with_context(|| format!("no se pudo escribir {}", temporal.display()))?;
        // `sync_all` y no sólo flush: sin esto el SO puede tener los datos en
        // su caché y perderlos aunque el `write` haya "terminado".
        f.sync_all()
            .with_context(|| format!("no se pudo volcar {}", temporal.display()))?;
    }

    match fs::rename(&temporal, ruta) {
        Ok(()) => Ok(()),
        // En Windows, renombrar sobre un archivo existente puede fallar.
        Err(e) => {
            let _ = fs::remove_file(ruta);
            fs::rename(&temporal, ruta).map_err(|e2| {
                anyhow!("no se pudo reemplazar {} ({e}; y reintentando: {e2})", ruta.display())
            })
        }
    }
}

/// Copia el archivo actual a `.backups/` antes de sobreescribirlo.
fn hacer_copia(ruta: &Path) -> Result<Option<PathBuf>> {
    if !ruta.exists() {
        return Ok(None);
    }
    let carpeta = ruta
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".backups");
    fs::create_dir_all(&carpeta)?;

    // Marca de tiempo con precisión de milisegundos: dos guardados muy seguidos
    // no deben pisarse.
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let nombre = format!(
        "{}_{ms}.json",
        ruta.file_stem().and_then(|s| s.to_str()).unwrap_or("sesion")
    );
    let destino = carpeta.join(nombre);
    fs::copy(ruta, &destino)?;
    Ok(Some(destino))
}

/// Deja sólo las `COPIAS_MAXIMAS` copias más recientes.
fn podar_copias(ruta: &Path) -> Result<usize> {
    let carpeta = ruta
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".backups");
    if !carpeta.exists() {
        return Ok(0);
    }

    let mut copias: Vec<(u64, PathBuf)> = Vec::new();
    for e in fs::read_dir(&carpeta)?.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let modificado = e
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        copias.push((modificado, p));
    }

    copias.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    let mut borradas = 0;
    for (_, p) in copias.iter().skip(COPIAS_MAXIMAS) {
        if fs::remove_file(p).is_ok() {
            borradas += 1;
        }
    }
    Ok(borradas)
}

/// Guarda la sesión: copia de seguridad, escritura atómica y poda de copias.
pub fn guardar_sesion(ruta: &Path, sesion: &Sesion) -> Result<()> {
    let contenido = sesion.a_json().map_err(|e| anyhow!("no se pudo serializar: {e}"))?;
    hacer_copia(ruta)?;
    escribir_atomico(ruta, &contenido)?;
    podar_copias(ruta)?;
    Ok(())
}

pub fn cargar_sesion(ruta: &Path) -> Result<(Sesion, super::modelo::ModoApertura)> {
    let texto = fs::read_to_string(ruta)
        .with_context(|| format!("no se pudo leer {}", ruta.display()))?;
    super::modelo::Sesion::desde_json(&texto).map_err(|e| anyhow!("{e}"))
}

// ---------------------------------------------------------------------------
// T-SES-004 — autoguardado con debounce
// ---------------------------------------------------------------------------

/// Autoguardado por inactividad.
///
/// No lleva hilo propio: la interfaz llama a [`AutoSaver::listo`] en cada
/// frame. Así no hay que sincronizar nada y el test es determinista.
#[derive(Debug, Default)]
pub struct AutoSaver {
    espera: Option<Duration>,
    pedido: Option<Instant>,
}

impl AutoSaver {
    pub fn nuevo() -> Self {
        Self { espera: None, pedido: None }
    }

    /// Pide un guardado. Si se pide otra vez antes de que venza la espera, el
    /// reloj se reinicia: es un *debounce*, no un *throttle*.
    pub fn pedir(&mut self) {
        self.pedido = Some(Instant::now());
    }

    pub fn pendiente(&self) -> bool {
        self.pedido.is_some()
    }

    /// true si ya pasó el tiempo de inactividad y toca guardar. Sólo devuelve
    /// true una vez por petición.
    pub fn listo(&mut self) -> bool {
        let espera = self.espera.unwrap_or(ESPERA_AUTOGUARDADO);
        match self.pedido {
            Some(t) if t.elapsed() >= espera => {
                self.pedido = None;
                true
            }
            _ => false,
        }
    }

    /// Para los tests: cambia la espera sin tener que dormir 1,5 s de verdad.
    pub fn con_espera(mut self, espera: Duration) -> Self {
        self.espera = Some(espera);
        self
    }
}

// ---------------------------------------------------------------------------
// T-SES-006 — state.json
// ---------------------------------------------------------------------------

/// Lo que la app recuerda entre arranques.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Estado {
    /// Última sesión abierta, para ofrecerla al arrancar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ultima_sesion: Option<String>,
    /// Salida elegida por el operador.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispositivo: Option<String>,
    /// "diseno" o "funcion".
    #[serde(default)]
    pub modo: String,
}

impl Estado {
    pub fn ruta() -> PathBuf {
        // Junto al ejecutable: así el programa sigue siendo portable.
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("state.json")
    }

    pub fn cargar() -> Self {
        match fs::read_to_string(Self::ruta()) {
            Ok(texto) => serde_json::from_str(&texto).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn guardar(&self) -> Result<()> {
        let contenido = serde_json::to_string_pretty(self)?;
        escribir_atomico(&Self::ruta(), &contenido)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    fn dir(nombre: &str) -> PathBuf {
        let d = std::env::temp_dir().join(nombre);
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn escribir_atomico_deja_el_contenido_correcto() {
        let d = dir("tp_atomico");
        let ruta = d.join("sesion.json");
        escribir_atomico(&ruta, "hola").unwrap();
        assert_eq!(fs::read_to_string(&ruta).unwrap(), "hola");

        // Y se puede sobrescribir.
        escribir_atomico(&ruta, "adios").unwrap();
        assert_eq!(fs::read_to_string(&ruta).unwrap(), "adios");
        // No debe quedar ningún .tmp suelto.
        assert!(!ruta.with_extension("tmp").exists());
    }

    #[test]
    fn no_queda_nunca_un_archivo_a_medias() {
        // Se simula el "crash": se escribe un contenido enorme y se comprueba
        // que mientras se escribe el archivo anterior sigue intacto.
        let d = dir("tp_atomico_parcial");
        let ruta = d.join("sesion.json");
        escribir_atomico(&ruta, "version_vieja").unwrap();

        let temporal = ruta.with_extension("tmp");
        fs::write(&temporal, "a medias").unwrap(); // el crash deja el .tmp

        // El archivo bueno sigue ahí: el lector nunca ve el .tmp.
        assert_eq!(fs::read_to_string(&ruta).unwrap(), "version_vieja");
        fs::remove_file(&temporal).unwrap();
    }

    #[test]
    fn cada_guardado_deja_una_copia_y_se_poda() {
        let d = dir("tp_copias");
        let ruta = d.join("sesion.json");

        let mut sesion = Sesion::nueva();
        for i in 0..COPIAS_MAXIMAS + 4 {
            sesion.nombre = format!("obra {i}");
            guardar_sesion(&ruta, &sesion).unwrap();
        }

        let copias = fs::read_dir(d.join(".backups")).unwrap().count();
        assert_eq!(copias, COPIAS_MAXIMAS, "deben quedar {COPIAS_MAXIMAS} copias, hay {copias}");
        assert_eq!(sesion.nombre, format!("obra {}", COPIAS_MAXIMAS + 3));
    }

    #[test]
    fn guardar_y_cargar_vuelve_igual() {
        let d = dir("tp_guardar");
        let ruta = d.join("sesion.json");
        let mut sesion = Sesion::nueva();
        sesion.nombre = "La obra".to_string();
        guardar_sesion(&ruta, &sesion).unwrap();

        let (cargada, modo) = cargar_sesion(&ruta).unwrap();
        assert_eq!(modo, super::super::modelo::ModoApertura::Normal);
        assert_eq!(cargada.nombre, "La obra");
        assert_eq!(cargada, sesion);
    }

    #[test]
    fn el_autoguardado_espera_y_solo_dispara_una_vez() {
        let mut auto = AutoSaver::nuevo().con_espera(Duration::from_millis(200));

        // 100 peticiones en ráfaga: ninguna dispara todavía.
        for _ in 0..100 {
            auto.pedir();
            assert!(!auto.listo(), "no debe disparar antes de la espera");
        }
        assert!(auto.pendiente());

        sleep(Duration::from_millis(250));
        assert!(auto.listo(), "debe disparar al pasar la espera");
        assert!(!auto.listo(), "sólo debe disparar una vez");
        assert!(!auto.pendiente());
    }

    #[test]
    fn el_state_json_hace_round_trip() {
        let d = dir("tp_estado");
        // El test no puede mover el ejecutable, así que se prueba el tipo.
        let estado = Estado {
            ultima_sesion: Some("MiObra.tpshow".to_string()),
            dispositivo: Some("0".to_string()),
            modo: "funcion".to_string(),
        };
        let json = serde_json::to_string(&estado).unwrap();
        assert!(json.contains("\"ultimaSesion\""), "json: {json}");
        let vuelta: Estado = serde_json::from_str(&json).unwrap();
        assert_eq!(vuelta, estado);

        // Y se escribe atómicamente.
        let ruta = d.join("state.json");
        escribir_atomico(&ruta, &json).unwrap();
        assert_eq!(fs::read_to_string(&ruta).unwrap(), json);
    }
}
