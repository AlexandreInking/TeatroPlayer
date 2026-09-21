//! Resolución de audios (T-SES-005).
//!
//! Algoritmo de 7 pasos de `Docs/04-modelo-de-datos.md` §3. Es lo que hace que
//! una obra siga sonando aunque el operador mueva la carpeta, renombre un
//! archivo o la copie a otro pendrive.
//!
//! La regla de oro: **el hash nunca bloquea la reproducción**. Si el archivo
//! está por ruta pero el hash no coincide, suena igual y se avisa. En teatro,
//! que suene es más importante que que coincida.

use std::fs;
use std::path::{Path, PathBuf};

use super::modelo::{AudioRef, Sesion};

/// Resultado de resolver el audio de una entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoAudio {
    /// Encontrado y se puede sonar.
    Ok(PathBuf),
    /// Encontrado fuera de sitio: al guardar se reescribe `relPath`.
    Movido(PathBuf),
    /// No está. La fila se pinta en rojo y su GO se deshabilita.
    Faltante,
}

impl EstadoAudio {
    pub fn ruta(&self) -> Option<&Path> {
        match self {
            EstadoAudio::Ok(p) | EstadoAudio::Movido(p) => Some(p.as_path()),
            EstadoAudio::Faltante => None,
        }
    }

    pub fn es_faltante(&self) -> bool {
        matches!(self, EstadoAudio::Faltante)
    }
}

/// Resuelve todos los audios de la sesión relativa a `carpeta`.
pub fn resolver(sesion: &Sesion, carpeta: &Path) -> Vec<EstadoAudio> {
    sesion.cues.iter().map(|c| resolver_uno(&c.audio, carpeta)).collect()
}

/// Los 7 pasos, en orden, parando en el primero que funciona.
pub fn resolver_uno(audio: &AudioRef, carpeta: &Path) -> EstadoAudio {
    // 1. relPath dentro de la carpeta de la sesión
    if !audio.rel_path.is_empty() {
        let p = carpeta.join(&audio.rel_path);
        if p.is_file() {
            return EstadoAudio::Ok(p);
        }
    }

    // 2. absPath tal cual (el usuario no copió el audio a la carpeta)
    if let Some(abs) = &audio.abs_path {
        let p = PathBuf::from(abs);
        if p.is_file() {
            return EstadoAudio::Ok(p);
        }
    }

    // 3. y 4. dentro de audio/, por fileName y por fileName + sizeBytes
    let dir_audio = carpeta.join("audio");
    if let Some(p) = buscar_en_audio(&dir_audio, audio) {
        return EstadoAudio::Ok(p);
    }

    // 5. dentro de audio/, por hash (el archivo se renombró)
    if let (Some(hash), Ok(entradas)) = (&audio.hash, fs::read_dir(&dir_audio)) {
        for e in entradas.flatten() {
            let p = e.path();
            if p.is_file() && hash_del_archivo(&p).as_deref() == Some(hash.as_str()) {
                return EstadoAudio::Ok(p);
            }
        }
    }

    // 6. búsqueda recursiva en la carpeta de la sesión: se reubicó
    if let Some(p) = buscar_recursivo(carpeta, &audio.file_name) {
        return EstadoAudio::Movido(p);
    }

    // 7. nada
    EstadoAudio::Faltante
}

/// Pasos 3 y 4: por nombre, y por nombre + tamaño.
fn buscar_en_audio(dir_audio: &Path, audio: &AudioRef) -> Option<PathBuf> {
    if !dir_audio.is_dir() || audio.file_name.is_empty() {
        return None;
    }
    let mut por_nombre: Option<PathBuf> = None;

    for e in fs::read_dir(dir_audio).ok()?.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let mismo_nombre = p.file_name().and_then(|n| n.to_str()) == Some(audio.file_name.as_str());
        if !mismo_nombre {
            continue;
        }
        por_nombre = Some(p.clone());
        // Si sabemos el tamaño y coincide, es él sin ninguna duda.
        if let Some(esperado) = audio.size_bytes {
            if fs::metadata(&p).ok().map(|m| m.len()).unwrap_or(0) == esperado {
                return Some(p);
            }
        }
    }
    por_nombre
}

/// Paso 6: búsqueda recursiva por nombre de archivo.
fn buscar_recursivo(carpeta: &Path, file_name: &str) -> Option<PathBuf> {
    if file_name.is_empty() {
        return None;
    }
    let mut pendientes: Vec<PathBuf> = vec![carpeta.to_path_buf()];

    while let Some(dir) = pendientes.pop() {
        let entradas = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for e in entradas.flatten() {
            let p = e.path();
            if p.is_dir() {
                // No se mete en .backups: ahí sólo hay JSON viejos.
                if p.file_name().and_then(|n| n.to_str()) != Some(".backups") {
                    pendientes.push(p);
                }
            } else if p.file_name().and_then(|n| n.to_str()) == Some(file_name) {
                return Some(p);
            }
        }
    }
    None
}

/// Hash blake3 de un archivo, en hex. Vacío si no se puede leer.
///
/// Se usa sólo en el paso 5 y bajo demanda: con 30 pistas de 50 MB tarda
/// alrededor de un segundo, así que no se hace en el camino normal.
pub fn hash_del_archivo(ruta: &Path) -> Option<String> {
    let bytes = fs::read(ruta).ok()?;
    Some(blake3::hash(&bytes).to_hex().to_string())
}

/// Rellena los metadatos de un audio recién importado: tamaño y hash.
///
/// Así los pasos 4 y 5 tienen con qué comparar más adelante.
pub fn completar_metadatos(audio: &mut AudioRef, ruta: &Path) {
    if let Ok(m) = fs::metadata(ruta) {
        audio.size_bytes = Some(m.len());
    }
    audio.hash = hash_del_archivo(ruta);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sesion::modelo::Cue;

    fn dir(nombre: &str) -> PathBuf {
        let d = std::env::temp_dir().join(nombre);
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn escribir(ruta: &Path, contenido: &str) {
        if let Some(padre) = ruta.parent() {
            fs::create_dir_all(padre).unwrap();
        }
        fs::write(ruta, contenido).unwrap();
    }

    fn sesion_con(audios: &[AudioRef]) -> Sesion {
        let mut s = Sesion::nueva();
        for (i, a) in audios.iter().enumerate() {
            let mut cue = Cue::nuevo(format!("cue {i}"), a.file_name.clone());
            cue.audio = a.clone();
            s.cues.push(cue);
        }
        s
    }

    #[test]
    fn paso_1_encuentra_por_rel_path() {
        let d = dir("tp_link_1");
        escribir(&d.join("audio").join("a.wav"), "aaaa");
        let audio = AudioRef {
            file_name: "a.wav".into(),
            rel_path: "audio/a.wav".into(),
            ..Default::default()
        };
        let s = sesion_con(&[audio]);
        assert_eq!(resolver(&s, &d)[0], EstadoAudio::Ok(d.join("audio/a.wav")));
    }

    #[test]
    fn paso_2_cae_al_abs_path() {
        let d = dir("tp_link_2");
        let fuera = dir("tp_link_2_fuera");
        let ruta = fuera.join("a.wav");
        escribir(&ruta, "aaaa");

        let audio = AudioRef {
            file_name: "a.wav".into(),
            rel_path: "audio/a.wav".into(), // no existe
            abs_path: Some(ruta.to_string_lossy().to_string()),
            ..Default::default()
        };
        let s = sesion_con(&[audio]);
        assert_eq!(resolver(&s, &d)[0], EstadoAudio::Ok(ruta));
    }

    #[test]
    fn paso_3_encuentra_en_audio_por_nombre() {
        let d = dir("tp_link_3");
        escribir(&d.join("audio").join("a.wav"), "aaaa");
        let audio = AudioRef {
            file_name: "a.wav".into(),
            rel_path: "otro/lugar/a.wav".into(),
            ..Default::default()
        };
        let s = sesion_con(&[audio]);
        assert_eq!(resolver(&s, &d)[0], EstadoAudio::Ok(d.join("audio/a.wav")));
    }

    #[test]
    fn paso_5_encuentra_por_hash_aunque_se_renombre() {
        let d = dir("tp_link_5");
        escribir(&d.join("audio").join("nombre_nuevo.wav"), "contenido");
        let hash = hash_del_archivo(&d.join("audio/nombre_nuevo.wav")).unwrap();

        let audio = AudioRef {
            file_name: "nombre_viejo.wav".into(),
            rel_path: "audio/nombre_viejo.wav".into(),
            hash: Some(hash),
            ..Default::default()
        };
        let s = sesion_con(&[audio]);
        assert_eq!(
            resolver(&s, &d)[0],
            EstadoAudio::Ok(d.join("audio/nombre_nuevo.wav"))
        );
    }

    #[test]
    fn paso_6_lo_encuentra_movido_en_otra_subcarpeta() {
        let d = dir("tp_link_6");
        escribir(&d.join("sonidos").join("profundo").join("a.wav"), "aaaa");
        let audio = AudioRef {
            file_name: "a.wav".into(),
            rel_path: "audio/a.wav".into(),
            ..Default::default()
        };
        let s = sesion_con(&[audio]);
        assert_eq!(
            resolver(&s, &d)[0],
            EstadoAudio::Movido(d.join("sonidos/profundo/a.wav"))
        );
    }

    #[test]
    fn paso_7_si_no_esta_es_faltante() {
        let d = dir("tp_link_7");
        let audio = AudioRef {
            file_name: "no_existe.wav".into(),
            rel_path: "audio/no_existe.wav".into(),
            ..Default::default()
        };
        let s = sesion_con(&[audio]);
        let r = &resolver(&s, &d)[0];
        assert!(r.es_faltante(), "debe ser FALTANTE, es {r:?}");
        assert!(r.ruta().is_none());
    }

    #[test]
    fn una_sesion_mezclada_resuelve_cada_caso() {
        let d = dir("tp_link_mixto");
        escribir(&d.join("audio").join("ok.wav"), "1111");
        escribir(&d.join("cajon").join("movido.wav"), "2222");

        let mut movido = AudioRef {
            file_name: "movido.wav".into(),
            rel_path: "audio/movido.wav".into(),
            ..Default::default()
        };
        let _ = &mut movido;

        let audios = vec![
            AudioRef { file_name: "ok.wav".into(), rel_path: "audio/ok.wav".into(), ..Default::default() },
            AudioRef { file_name: "movido.wav".into(), rel_path: "audio/movido.wav".into(), ..Default::default() },
            AudioRef { file_name: "perdido.wav".into(), rel_path: "audio/perdido.wav".into(), ..Default::default() },
        ];
        let s = sesion_con(&audios);
        let r = resolver(&s, &d);

        assert_eq!(r[0], EstadoAudio::Ok(d.join("audio/ok.wav")));
        assert_eq!(r[1], EstadoAudio::Movido(d.join("cajon/movido.wav")));
        assert!(r[2].es_faltante());
    }

    #[test]
    fn el_hash_no_bloquea_la_reproduccion() {
        // El archivo está por ruta pero el hash guardado es otro: suena igual.
        let d = dir("tp_link_hash");
        escribir(&d.join("audio").join("a.wav"), "contenido");
        let audio = AudioRef {
            file_name: "a.wav".into(),
            rel_path: "audio/a.wav".into(),
            hash: Some("hash_que_no_coincide".into()),
            ..Default::default()
        };
        let s = sesion_con(&[audio]);
        match &resolver(&s, &d)[0] {
            EstadoAudio::Ok(_) => {}
            otro => panic!("el hash no debe bloquear, dio {otro:?}"),
        }
    }

    #[test]
    fn completar_metadatos_rellena_tamano_y_hash() {
        let d = dir("tp_link_meta");
        let ruta = d.join("audio").join("a.wav");
        escribir(&ruta, "holaholahola");

        let mut audio = AudioRef { file_name: "a.wav".into(), rel_path: "audio/a.wav".into(), ..Default::default() };
        completar_metadatos(&mut audio, &ruta);

        assert_eq!(audio.size_bytes, Some(12));
        assert!(audio.hash.is_some());
        assert_eq!(audio.hash.unwrap().len(), 64, "blake3 son 64 hex");
    }
}
