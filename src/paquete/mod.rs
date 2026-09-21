//! Formato de archivo único `.tpshow` (hito FMT).
//!
//! Especificación completa en `Docs/13-formato-de-archivo-unico.md`.
//!
//! Las dos propiedades que importan:
//! 1. **Determinismo**: el mismo contenido produce siempre los mismos bytes, así
//!    que dos personas pueden comparar paquetes con SHA-256.
//! 2. **Portabilidad**: el audio va dentro, así que un `.tpshow` se manda por
//!    WhatsApp y funciona en cualquier máquina sin audios sueltos.

pub mod lector;

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::engine::model::EXTENSIONES_AUDIO;

pub use lector::ZipEntryReader;

/// Versión del formato de paquete. Si un paquete trae una mayor, no se abre.
pub const VERSION_FORMATO: u32 = 1;

/// Timestamp fijo de todas las entradas: 1980-01-01 00:00:00 (DOS time).
///
/// Es el mínimo representable en ZIP y, sobre todo, **no depende de cuándo se
/// empaqueta**, que es lo que haría el archivo no determinista.
const TIMESTAMP_FIJO: DateTime = DateTime::DEFAULT;

/// Permisos UNIX de las entradas: 644 (rw-r--r--).
const PERMISOS: u32 = 0o644;

/// A partir de 4 GiB el ZIP clásico ya no puede expresar el tamaño y hay que
/// usar ZIP64 (`Docs/13` §2). Una obra larga en WAV sin comprimir pasa de ahí:
/// 90 minutos en estéreo 44.1k/16 son unos 900 MB, pero 5 horas se acercan.
pub const UMBRAL_ZIP64: u64 = 4 * 1024 * 1024 * 1024;

/// ¿Este tamaño obliga a escribir la entrada en ZIP64?
pub fn necesita_zip64(bytes: u64) -> bool {
    bytes >= UMBRAL_ZIP64
}

// ---------------------------------------------------------------------------
// manifest.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub generator: Generador,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Generador {
    pub name: String,
    pub version: String,
}

impl Manifest {
    pub fn nuevo() -> Self {
        Self {
            format: "teatroplayer-package".to_string(),
            version: VERSION_FORMATO,
            generator: Generador {
                name: "TeatroPlayer".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::nuevo()
    }
}

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn es_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONES_AUDIO.contains(&e.to_lowercase().as_str()))
}

/// Opciones de escritura: todo fijo para que el resultado sea determinista.
fn opciones() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(TIMESTAMP_FIJO)
        .unix_permissions(PERMISOS)
}

/// Orden lexicográfico por nombre UTF-8. Es lo que hace que dos carpetas con el
/// mismo contenido pero distinto orden de lectura den el mismo archivo.
fn ordenar(mut nombres: Vec<String>) -> Vec<String> {
    nombres.sort();
    nombres
}

// ---------------------------------------------------------------------------
// T-FMT-001 — pack
// ---------------------------------------------------------------------------

/// Empaqueta una carpeta de sesión en un `.tpshow`.
///
/// La carpeta puede tener los audios en `audio/` o sueltos en la raíz; en
/// cualquier caso acaban todos en `audio/` dentro del paquete.
pub fn pack(carpeta: &Path, salida: &Path) -> Result<()> {
    if !carpeta.is_dir() {
        return Err(anyhow!("{} no es una carpeta", carpeta.display()));
    }

    let mut audios: Vec<PathBuf> = Vec::new();
    for sub in ["audio", "."] {
        let dir = carpeta.join(sub);
        if let Ok(entradas) = fs::read_dir(&dir) {
            for e in entradas.flatten() {
                let p = e.path();
                if p.is_file() && es_audio(&p) {
                    audios.push(p);
                }
            }
        }
    }
    audios.sort();

    // manifest.json y sesion.json van serializados con el mismo orden de claves
    // (BTreeMap no hace falta: la struct siempre se serializa igual).
    let manifest = serde_json::to_vec_pretty(&Manifest::nuevo())?;
    let sesion = match fs::read(carpeta.join("sesion.json")) {
        Ok(bytes) => bytes,
        // Sin sesion.json el paquete sigue siendo válido: se abre con cero cues.
        Err(_) => br#"{"version":1,"cues":[]}"#.to_vec(),
    };

    let file = File::create(salida)?;
    let mut zip = ZipWriter::new(file);

    zip.start_file("manifest.json", opciones())?;
    io::Write::write_all(&mut zip, &manifest)?;

    zip.start_file("sesion.json", opciones())?;
    io::Write::write_all(&mut zip, &sesion)?;

    // Los nombres se ordenan; el contenido se copia tal cual (STORE = 1:1).
    let nombres: Vec<String> = audios
        .iter()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        })
        .collect();
    for nombre in ordenar(nombres) {
        let origen = audios
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy() == nombre))
            .ok_or_else(|| anyhow!("se perdió la pista de {nombre}"))?;
        // Igual que en `escribir`: el tamaño decide si la cabecera va en ZIP64.
        // Sin esto, el crate `zip` **revienta con una aserción** al pasar de
        // 4 GiB (lo encontró el test de 5 GB).
        let tamano = fs::metadata(origen)?.len();
        let opciones = if necesita_zip64(tamano) {
            opciones().large_file(true)
        } else {
            opciones()
        };
        zip.start_file(format!("audio/{nombre}"), opciones)?;
        let mut f = File::open(origen)?;
        io::copy(&mut f, &mut zip)?;
    }

    zip.finish()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// T-FMT-002 — unpack
// ---------------------------------------------------------------------------

/// Extrae un `.tpshow` a una carpeta. Es para depuración y para el "Exportar
/// como carpeta" que se ofrecerá en la UI.
pub fn unpack(paquete: &Path, destino: &Path) -> Result<usize> {
    let mut archivo = ZipArchive::new(File::open(paquete)?)?;
    fs::create_dir_all(destino)?;

    let mut nombres: Vec<String> = archivo.file_names().map(|s| s.to_string()).collect();
    nombres.sort();

    for nombre in nombres {
        let salida = destino.join(&nombre);
        if nombre.ends_with('/') {
            fs::create_dir_all(&salida)?;
            continue;
        }
        if let Some(padre) = salida.parent() {
            fs::create_dir_all(padre)?;
        }
        let mut entrada = archivo.by_name(&nombre)?;
        let mut f = File::create(&salida)?;
        io::copy(&mut entrada, &mut f)?;
    }

    Ok(archivo.len())
}

// ---------------------------------------------------------------------------
// T-FMT-005 — verify
// ---------------------------------------------------------------------------

/// Referencias de audio que declara `sesion.json`.
#[derive(Debug, Deserialize)]
struct SesionMinima {
    #[serde(default)]
    cues: Vec<CueMinimo>,
}

#[derive(Debug, Deserialize)]
struct CueMinimo {
    #[serde(default)]
    audio: AudioMinimo,
    #[serde(default)]
    nombre: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct AudioMinimo {
    #[serde(default)]
    #[serde(rename = "fileName")]
    file_name: Option<String>,
}

/// Resultado de verificar un paquete.
#[derive(Debug, Default)]
pub struct Informe {
    pub entradas: usize,
    pub problemas: Vec<String>,
}

impl Informe {
    pub fn ok(&self) -> bool {
        self.problemas.is_empty()
    }

    pub fn audios(&self) -> usize {
        self.entradas
    }
}

/// Verifica el paquete: métodos STORE, CRCs, tamaños y que todo lo que
/// referencia `sesion.json` exista dentro.
///
/// El CRC lo comprueba el propio `zip` crate al leer la entrada completa: si
/// algo no cuadra devuelve error. No escribimos nuestro propio CRC-32.
pub fn verify(paquete: &Path) -> Result<Informe> {
    let mut informe = Informe::default();
    let mut archivo = ZipArchive::new(File::open(paquete)?)?;

    let mut nombres: Vec<String> = archivo.file_names().map(|s| s.to_string()).collect();
    nombres.sort();
    informe.entradas = nombres.len();

    for nombre in &nombres {
        if nombre.ends_with('/') {
            continue;
        }
        let mut entrada = match archivo.by_name(nombre) {
            Ok(e) => e,
            Err(e) => {
                informe.problemas.push(format!("{nombre}: no se pudo abrir ({e})"));
                continue;
            }
        };

        if entrada.size() == 0 {
            informe.problemas.push(format!("{nombre}: la entrada está vacía"));
        }
        if entrada.compression() != CompressionMethod::Stored {
            informe.problemas.push(format!(
                "{nombre}: método {:?}, se esperaba Stored",
                entrada.compression()
            ));
        }

        // Leer entero: aquí salta el CRC si está corrupto.
        let mut buf = Vec::new();
        match entrada.read_to_end(&mut buf) {
            Ok(_) => {}
            Err(e) => informe.problemas.push(format!("{nombre}: {e}")),
        }
    }

    // ¿Están todos los audios que pide la sesión?
    let mut sesion_bytes = Vec::new();
    if archivo.by_name("sesion.json").is_ok() {
        let mut s = archivo.by_name("sesion.json")?;
        s.read_to_end(&mut sesion_bytes)?;
    } else {
        informe.problemas.push("falta sesion.json en el paquete".to_string());
    }

    if !sesion_bytes.is_empty() {
        match serde_json::from_slice::<SesionMinima>(&sesion_bytes) {
            Ok(sesion) => {
                for (i, cue) in sesion.cues.iter().enumerate() {
                    let Some(f) = &cue.audio.file_name else { continue };
                    let dentro = format!("audio/{f}");
                    if !nombres.iter().any(|n| n == &dentro) {
                        let quien = cue.nombre.clone().unwrap_or_else(|| format!("cue {i}"));
                        informe
                            .problemas
                            .push(format!("{quien}: falta el audio '{dentro}'"));
                    }
                }
            }
            Err(e) => informe.problemas.push(format!("sesion.json no es válido: {e}")),
        }
    }

    Ok(informe)
}

// ---------------------------------------------------------------------------
// Escritura directa desde la sesion (para "Guardar como .tpshow")
// ---------------------------------------------------------------------------

/// De dónde salen los bytes de un audio al guardar.
///
/// Hace falta porque una obra puede haberse abierto **desde un paquete**: en ese
/// caso el audio no está suelto en el disco, está dentro del `.tpshow` de origen.
/// Sin esta variante, guardar una obra así la dejaría sin audios.
#[derive(Clone, Debug)]
pub enum OrigenAudio {
    /// Un archivo suelto en el disco.
    Archivo(PathBuf),
    /// Una entrada dentro de otro `.tpshow`.
    Paquete { tpshow: PathBuf, entrada: String },
}

/// Escribe un `.tpshow` a partir del JSON de la sesión y los audios ya
/// resueltos. No hace falta crear una carpeta intermedia.
///
/// `audios` son pares `(nombre_dentro_del_paquete, origen)`.
///
/// Se escribe en un `.tmp` y se renombra al final. No es decorativo: al guardar
/// una obra abierta desde un paquete **se lee del mismo archivo que se está
/// escribiendo**, así que escribir directo la destruiría. Y de paso un fallo a
/// mitad de guardado deja la obra anterior intacta.
pub fn escribir(salida: &Path, sesion_json: &str, audios: &[(String, OrigenAudio)]) -> Result<()> {
    let temporal = salida.with_extension("tmp");
    let resultado = escribir_en(&temporal, sesion_json, audios);

    match resultado {
        Ok(()) => {
            fs::rename(&temporal, salida).or_else(|e| {
                // En Windows, renombrar sobre un archivo existente puede fallar.
                let _ = fs::remove_file(salida);
                fs::rename(&temporal, salida).map_err(|e2| {
                    anyhow!(
                        "no se pudo reemplazar {} ({e}; y reintentando: {e2})",
                        salida.display()
                    )
                })
            })?;
            Ok(())
        }
        Err(e) => {
            // No se deja basura: si el guardado falla, fuera el temporal.
            let _ = fs::remove_file(&temporal);
            Err(e)
        }
    }
}

fn escribir_en(
    salida: &Path,
    sesion_json: &str,
    audios: &[(String, OrigenAudio)],
) -> Result<()> {
    let manifest = serde_json::to_vec_pretty(&Manifest::nuevo())?;
    let file = File::create(salida)?;
    let mut zip = ZipWriter::new(file);

    zip.start_file("manifest.json", opciones())?;
    io::Write::write_all(&mut zip, &manifest)?;

    zip.start_file("sesion.json", opciones())?;
    io::Write::write_all(&mut zip, sesion_json.as_bytes())?;

    let mut ordenados: Vec<&(String, OrigenAudio)> = audios.iter().collect();
    ordenados.sort_by(|a, b| a.0.cmp(&b.0));

    for (nombre, origen) in ordenados {
        // El tamaño hay que saberlo ANTES de abrir la entrada: es lo que decide
        // si la cabecera se escribe en ZIP64 o no.
        let tamano = match origen {
            OrigenAudio::Archivo(ruta) => fs::metadata(ruta)
                .with_context(|| format!("no se pudo medir {}", ruta.display()))?
                .len(),
            OrigenAudio::Paquete { tpshow, entrada } => {
                let mut ar = ZipArchive::new(File::open(tpshow)?)?;
                // Se guarda en una variable: el préstamo de `ar` tiene que
                // terminar antes de que `ar` se suelte.
                let size = ar.by_name(entrada)?.size();
                size
            }
        };

        let opciones = if necesita_zip64(tamano) {
            opciones().large_file(true)
        } else {
            opciones()
        };
        zip.start_file(format!("audio/{nombre}"), opciones)?;

        match origen {
            OrigenAudio::Archivo(ruta) => {
                let mut f = File::open(ruta).with_context(|| {
                    format!("no se pudo leer el audio {}", ruta.display())
                })?;
                io::copy(&mut f, &mut zip)?;
            }
            OrigenAudio::Paquete { tpshow, entrada } => {
                // Se copia en streaming, sin cargar el audio en memoria: una obra
                // puede tener pistas de 50 MB.
                let mut ar = ZipArchive::new(File::open(tpshow).with_context(|| {
                    format!("no se pudo abrir el paquete de origen {}", tpshow.display())
                })?)
                .with_context(|| format!("{} no es un paquete válido", tpshow.display()))?;
                let mut fuente = ar.by_name(entrada).with_context(|| {
                    format!("no se encuentra '{entrada}' en {}", tpshow.display())
                })?;
                io::copy(&mut fuente, &mut zip)?;
            }
        }
    }

    zip.finish()?;
    Ok(())
}

/// Lee el `sesion.json` que va dentro del paquete.
pub fn leer_sesion(paquete: &Path) -> Result<String> {
    let mut ar = ZipArchive::new(File::open(paquete)?)?;
    let mut entrada = ar
        .by_name("sesion.json")
        .map_err(|_| anyhow!("el paquete no contiene sesion.json"))?;
    let mut texto = String::new();
    entrada.read_to_string(&mut texto)?;
    Ok(texto)
}

/// Nombres de los audios que hay dentro del paquete (sin el prefijo `audio/`).
pub fn lista_audios(paquete: &Path) -> Result<Vec<String>> {
    let ar = ZipArchive::new(File::open(paquete)?)?;
    let mut nombres: Vec<String> = ar
        .file_names()
        .filter(|n| n.starts_with("audio/") && !n.ends_with('/'))
        .map(|n| n.trim_start_matches("audio/").to_string())
        .collect();
    nombres.sort();
    Ok(nombres)
}

// ---------------------------------------------------------------------------
// diff
// ---------------------------------------------------------------------------

/// Compara dos paquetes por nombre de entrada y tamaño.
#[derive(Debug, Default)]
pub struct Diferencias {
    pub solo_en_a: Vec<String>,
    pub solo_en_b: Vec<String>,
    pub cambiaron: Vec<String>,
    pub iguales: usize,
}

pub fn diff(a: &Path, b: &Path) -> Result<Diferencias> {
    let mapa = |p: &Path| -> Result<Vec<(String, u64)>> {
        let mut ar = ZipArchive::new(File::open(p)?)?;
        // Los nombres se recogen primero: `file_names` presta el archivo y no
        // se puede pedir `by_name` mientras ese préstamo vive.
        let nombres: Vec<String> = ar.file_names().map(|s| s.to_string()).collect();
        let mut v = Vec::new();
        for nombre in nombres {
            v.push((nombre.clone(), ar.by_name(&nombre)?.size()));
        }
        v.sort();
        Ok(v)
    };

    let ma = mapa(a)?;
    let mb = mapa(b)?;
    let mut d = Diferencias::default();

    for (nombre, size) in &ma {
        match mb.iter().find(|(n, _)| n == nombre) {
            Some((_, s)) if s == size => d.iguales += 1,
            Some(_) => d.cambiaron.push(nombre.clone()),
            None => d.solo_en_a.push(nombre.clone()),
        }
    }
    for (nombre, _) in &mb {
        if !ma.iter().any(|(n, _)| n == nombre) {
            d.solo_en_b.push(nombre.clone());
        }
    }
    Ok(d)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests_escribir {
    use super::*;
    use std::io::Write;

    #[test]
    fn escribir_y_leer_vuelve_igual() {
        let d = std::env::temp_dir().join("tp_paq_escribir");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let wav = d.join("a.wav");
        let mut f = File::create(&wav).unwrap();
        f.write_all(b"RIFFloque sea").unwrap();
        drop(f);

        let salida = d.join("obra.tpshow");
        escribir(
            &salida,
            r#"{"version":1,"nombre":"O","cues":[]}"#,
            &[("a.wav".to_string(), OrigenAudio::Archivo(wav))],
        )
        .unwrap();

        assert_eq!(leer_sesion(&salida).unwrap(), r#"{"version":1,"nombre":"O","cues":[]}"#);
        assert_eq!(lista_audios(&salida).unwrap(), vec!["a.wav".to_string()]);

        let informe = verify(&salida).unwrap();
        assert!(informe.ok(), "problemas: {:?}", informe.problemas);
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sha256(p: &Path) -> String {
        let bytes = fs::read(p).unwrap();
        // sha256 sin dependencias extra no es razonable; usamos el tamaño más
        // un hash casero estable sólo para comparar entre sí en el test.
        let mut h: u64 = 0xcbf29ce484222325;
        for b in &bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        format!("{h:016x}:{}", bytes.len())
    }

    fn carpeta_de_prueba(nombre: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(nombre);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("audio")).unwrap();
        for (i, n) in ["01_ambiente.wav", "02_trueno.wav", "03_viento.wav"].iter().enumerate() {
            let mut f = File::create(dir.join("audio").join(n)).unwrap();
            f.write_all(&vec![(i as u8).wrapping_add(1); 2048]).unwrap();
        }
        fs::write(
            dir.join("sesion.json"),
            r#"{"version":1,"cues":[{"nombre":"Ambiente","audio":{"fileName":"01_ambiente.wav"}},{"nombre":"Trueno","audio":{"fileName":"02_trueno.wav"}}]}"#,
        )
        .unwrap();
        dir
    }

    #[test]
    fn dos_packs_de_lo_mismo_dan_el_mismo_archivo() {
        let dir = carpeta_de_prueba("tp_det_a");
        let a = dir.join("a.tpshow");
        let b = dir.join("b.tpshow");
        pack(&dir, &a).unwrap();
        pack(&dir, &b).unwrap();
        assert_eq!(sha256(&a), sha256(&b), "dos packs de la misma fuente difieren");
        assert!(fs::read(&a).unwrap() == fs::read(&b).unwrap());
    }

    #[test]
    fn pack_unpack_pack_es_un_roundtrip_exacto() {
        let dir = carpeta_de_prueba("tp_det_b");
        let primero = dir.join("1.tpshow");
        pack(&dir, &primero).unwrap();

        let copia = std::env::temp_dir().join("tp_det_b_copia");
        let _ = fs::remove_dir_all(&copia);
        unpack(&primero, &copia).unwrap();

        let segundo = dir.join("2.tpshow");
        pack(&copia, &segundo).unwrap();

        assert_eq!(sha256(&primero), sha256(&segundo), "el round-trip no es exacto");
    }

    #[test]
    fn el_pack_lleva_manifest_sesion_y_los_audios() {
        let dir = carpeta_de_prueba("tp_det_c");
        let p = dir.join("t.tpshow");
        pack(&dir, &p).unwrap();

        let ar = ZipArchive::new(File::open(&p).unwrap()).unwrap();
        let mut nombres: Vec<String> = ar.file_names().map(|s| s.to_string()).collect();
        nombres.sort();
        assert_eq!(
            nombres,
            vec![
                "audio/01_ambiente.wav",
                "audio/02_trueno.wav",
                "audio/03_viento.wav",
                "manifest.json",
                "sesion.json",
            ]
        );
    }

    #[test]
    fn todo_se_guarda_sin_comprimir_y_con_timestamp_fijo() {
        let dir = carpeta_de_prueba("tp_det_d");
        let p = dir.join("t.tpshow");
        pack(&dir, &p).unwrap();

        let mut ar = ZipArchive::new(File::open(&p).unwrap()).unwrap();
        for i in 0..ar.len() {
            let nombre = ar.name_for_index(i).unwrap().to_string();
            let e = ar.by_name(&nombre).unwrap();
            assert_eq!(
                e.compression(),
                CompressionMethod::Stored,
                "{nombre} no está en STORE"
            );
            assert_eq!(
                e.last_modified().unwrap_or_default().year(),
                1980,
                "{nombre} no lleva el timestamp fijo de 1980"
            );
        }
    }

    #[test]
    fn verify_pasa_en_un_paquete_sano() {
        let dir = carpeta_de_prueba("tp_det_e");
        let p = dir.join("t.tpshow");
        pack(&dir, &p).unwrap();
        let informe = verify(&p).unwrap();
        assert!(informe.ok(), "problemas: {:?}", informe.problemas);
    }

    #[test]
    fn verify_detecta_un_byte_corrupto() {
        let dir = carpeta_de_prueba("tp_det_f");
        let p = dir.join("t.tpshow");
        pack(&dir, &p).unwrap();

        // Voltear un bit dentro de los datos de un audio (no en el ZIP).
        let (offset, _size) = lector::localizar(&p, "audio/02_trueno.wav").unwrap();
        let mut bytes = fs::read(&p).unwrap();
        bytes[offset as usize + 10] ^= 0b0000_0001;
        fs::write(&p, &bytes).unwrap();

        let informe = verify(&p).unwrap();
        assert!(!informe.ok(), "un byte corrupto debería detectarse");
        assert!(
            informe.problemas.iter().any(|x| x.contains("02_trueno.wav")),
            "el problema debe nombrar la entrada corrupta: {:?}",
            informe.problemas
        );
    }

    #[test]
    fn verify_detecta_un_audio_que_falta() {
        let dir = carpeta_de_prueba("tp_det_g");
        let p = dir.join("t.tpshow");
        pack(&dir, &p).unwrap();
        fs::remove_file(dir.join("audio").join("01_ambiente.wav")).unwrap();

        let p2 = dir.join("t2.tpshow");
        pack(&dir, &p2).unwrap();

        let informe = verify(&p2).unwrap();
        assert!(!informe.ok());
        assert!(
            informe.problemas.iter().any(|x| x.contains("01_ambiente.wav")),
            "debe avisar del audio que falta: {:?}",
            informe.problemas
        );
    }

    #[test]
    fn diff_encuentra_lo_que_cambio() {
        let dir = carpeta_de_prueba("tp_det_h");
        let a = dir.join("a.tpshow");
        pack(&dir, &a).unwrap();

        // Cambiar un audio y quitar otro
        fs::write(dir.join("audio").join("03_viento.wav"), vec![9u8; 900]).unwrap();
        fs::remove_file(dir.join("audio").join("02_trueno.wav")).unwrap();
        let b = dir.join("b.tpshow");
        pack(&dir, &b).unwrap();

        let d = diff(&a, &b).unwrap();
        assert!(d.cambiaron.contains(&"audio/03_viento.wav".to_string()));
        assert!(d.solo_en_a.contains(&"audio/02_trueno.wav".to_string()));
        assert!(d.solo_en_b.is_empty());
    }
}
