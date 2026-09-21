//! `ZipEntryReader`: lee una entrada del `.tpshow` **sin extraerla a disco**
//! (T-FMT-003).
//!
//! `rodio::Decoder::new` necesita `Read + Seek`, así que esto es lo que permite
//! decodificar un audio que vive dentro del paquete. Con STORE no hay que
//! descomprimir nada: los bytes están tal cual en el archivo, así que buscar es
//! traducir `pos` a `data_offset + pos` y leer.
//!
//! El `zip` crate ya trae `ZipFileSeek` para esto, pero **presta** el archivo
//! (`ZipFileSeek<'a, R>`), y el motor necesita un lector `'static` por pista:
//! cada pista abre su propio `File`. Por eso esto no es reinventar la rueda: la
//! rueda (parsear el ZIP, EOCD, ZIP64) la pone el crate; aquí solo hay un
//! `File` propio con un offset.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{anyhow, Result};
use zip::ZipArchive;

/// Dónde empiezan los datos de una entrada y cuánto miden.
pub fn localizar(ruta: &Path, entrada: &str) -> Result<(u64, u64)> {
    let mut archivo = ZipArchive::new(File::open(ruta)?)?;
    let zip_file = archivo
        .by_name(entrada)
        .map_err(|e| anyhow!("no se pudo abrir '{entrada}' dentro del paquete: {e}"))?;
    let offset = zip_file
        .data_start()
        .ok_or_else(|| anyhow!("el paquete no indica dónde empieza '{entrada}'"))?;
    let size = zip_file.size();
    Ok((offset, size))
}

/// Lector con `Read + Seek` sobre una entrada concreta de un `.tpshow`.
pub struct ZipEntryReader {
    file: File,
    entrada: String,
    data_offset: u64,
    size: u64,
    pos: u64,
}

impl ZipEntryReader {
    pub fn new(ruta: &Path, entrada: &str) -> Result<Self> {
        let (data_offset, size) = localizar(ruta, entrada)?;
        let mut file = File::open(ruta)?;
        file.seek(SeekFrom::Start(data_offset))?;
        Ok(Self { file, entrada: entrada.to_string(), data_offset, size, pos: 0 })
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn pos(&self) -> u64 {
        self.pos
    }

    pub fn entrada(&self) -> &str {
        &self.entrada
    }
}

impl Read for ZipEntryReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.size {
            return Ok(0);
        }
        // Nunca leer más allá del final de la entrada: detrás viene la
        // siguiente, y mezclarlas daría audio corrupto en vez de un error.
        let max = ((self.size - self.pos) as usize).min(buf.len());
        if self.file.stream_position()? != self.data_offset + self.pos {
            self.file.seek(SeekFrom::Start(self.data_offset + self.pos))?;
        }
        let n = self.file.read(&mut buf[..max])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for ZipEntryReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let destino: i128 = match pos {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(p) => self.size as i128 + p as i128,
            SeekFrom::Current(p) => self.pos as i128 + p as i128,
        };
        // Clamp al rango de la entrada: fuera de ahí no hay nada nuestro.
        let destino = destino.clamp(0, self.size as i128) as u64;
        self.pos = destino;
        self.file.seek(SeekFrom::Start(self.data_offset + destino))?;
        Ok(destino)
    }

    fn stream_position(&mut self) -> std::io::Result<u64> {
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_de_prueba(bytes: usize) -> Vec<u8> {
        // No hace falta que sea un WAV válido para probar el lector: solo
        // importa que los bytes sean distinguibles y reproducibles.
        (0..bytes).map(|i| (i % 251) as u8).collect()
    }

    fn hacer_paquete(dir: &Path, datos: &[u8]) -> std::path::PathBuf {
        std::fs::create_dir_all(dir.join("audio")).unwrap();
        std::fs::write(dir.join("audio").join("pista.wav"), datos).unwrap();
        let salida = dir.join("t.tpshow");
        crate::paquete::pack(dir, &salida).unwrap();
        salida
    }

    #[test]
    fn lee_secuencialmente_igual_que_el_original() {
        let dir = std::env::temp_dir().join("tp_lector_seq");
        let _ = std::fs::remove_dir_all(&dir);
        let datos = wav_de_prueba(70_000);
        let tpshow = hacer_paquete(&dir, &datos);

        let mut r = ZipEntryReader::new(&tpshow, "audio/pista.wav").unwrap();
        let mut leido = Vec::new();
        r.read_to_end(&mut leido).unwrap();
        assert_eq!(leido.len(), datos.len());
        assert!(leido == datos, "los bytes leidos no coinciden con los originales");
    }

    #[test]
    fn seek_al_50_por_ciento_y_de_vuelta_al_principio() {
        let dir = std::env::temp_dir().join("tp_lector_seek");
        let _ = std::fs::remove_dir_all(&dir);
        let datos = wav_de_prueba(70_000);
        let tpshow = hacer_paquete(&dir, &datos);

        let mut r = ZipEntryReader::new(&tpshow, "audio/pista.wav").unwrap();

        // Primeros 4 KiB
        let mut a = vec![0u8; 4096];
        r.read_exact(&mut a).unwrap();
        assert!(a == datos[..4096]);

        // Seek al 50 % y leer otros 4 KiB
        let medio = datos.len() / 2;
        r.seek(SeekFrom::Start(medio as u64)).unwrap();
        let mut b = vec![0u8; 4096];
        r.read_exact(&mut b).unwrap();
        assert!(b == datos[medio..medio + 4096], "la lectura al 50% no coincide");

        // Seek de vuelta a 0 y leer otra vez los primeros 4 KiB
        r.seek(SeekFrom::Start(0)).unwrap();
        let mut c = vec![0u8; 4096];
        r.read_exact(&mut c).unwrap();
        assert!(c == datos[..4096], "la relectura desde 0 no coincide");
        assert!(a == c, "leer dos veces el principio debe dar lo mismo");
    }

    #[test]
    fn no_se_sale_de_la_entrada() {
        let dir = std::env::temp_dir().join("tp_lector_limite");
        let _ = std::fs::remove_dir_all(&dir);
        let datos = wav_de_prueba(5_000);
        let tpshow = hacer_paquete(&dir, &datos);

        let mut r = ZipEntryReader::new(&tpshow, "audio/pista.wav").unwrap();
        assert_eq!(r.size(), 5_000);
        // Pedir más de lo que queda: devuelve lo que hay y luego 0.
        let mut buf = vec![0u8; 10_000];
        let n = r.read(&mut buf).unwrap();
        assert_eq!(n, 5_000);
        assert_eq!(r.read(&mut buf).unwrap(), 0);

        // Un seek más allá del final queda recortado al tamaño.
        assert_eq!(r.seek(SeekFrom::Start(999_999)).unwrap(), 5_000);
    }

    #[test]
    fn una_entrada_que_no_existe_da_error_no_panico() {
        let dir = std::env::temp_dir().join("tp_lector_falta");
        let _ = std::fs::remove_dir_all(&dir);
        let tpshow = hacer_paquete(&dir, &wav_de_prueba(100));
        assert!(ZipEntryReader::new(&tpshow, "audio/no_existe.wav").is_err());
    }
}
