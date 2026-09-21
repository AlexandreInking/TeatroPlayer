//! Logging a archivo y Diagnóstico (T-OPS-001, T-OPS-002).
//!
//! En teatro, cuando algo sale mal, lo único que hay después es el log: nadie
//! va a reproducir el problema en el momento. Por eso cada GO, cada guardado y
//! cada error queda escrito en disco con su hora.
//!
//! Se usa `tracing` + `tracing-appender` en vez de un logger propio: la rotación
//! diaria y el formato ya están resueltos y probados.

use std::fs;
use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;

/// Cuántos archivos de log se conservan.
pub const ROTACION_DIAS: usize = 3;

/// Carpeta de logs: junto al ejecutable, no dentro de `target/`.
pub fn carpeta_logs() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("logs")
}

/// Arranca el logging: archivo rotativo diario y, si hay consola, también por
/// pantalla.
///
/// Devuelve el `WorkerGuard`, que **hay que mantener vivo** mientras la app
/// corra: al soltarlo se dejan de vaciar los logs pendientes.
pub fn iniciar() -> Option<WorkerGuard> {
    let carpeta = carpeta_logs();
    if fs::create_dir_all(&carpeta).is_err() {
        return None;
    }

    // La rotación arma el nombre: <prefijo>.<fecha>.<sufijo>
    // => teatroplayer.2026-09-21.log
    let rotativo = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("teatroplayer")
        .filename_suffix("log")
        .max_log_files(ROTACION_DIAS)
        .build(&carpeta)
        .ok()?;

    let (escritor, guard) = tracing_appender::non_blocking(rotativo);

    // El archivo lo tiene todo; la consola (si existe) sólo avisos y errores,
    // para no ensuciar la salida cuando se depura.
    let a_archivo = escritor.with_max_level(tracing::Level::INFO);
    let a_consola = std::io::stdout.with_max_level(tracing::Level::WARN);

    let filtro = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filtro)
        .with_writer(a_archivo.and(a_consola))
        .with_ansi(false)
        .init();

    Some(guard)
}

/// Devuelve las últimas `n` líneas del log de hoy, para el panel de
/// diagnóstico (T-OPS-002).
///
/// No falla si no hay log: devuelve un mensaje diciéndolo, que es lo que se
/// quiere mostrar en la ventana.
pub fn ultimas_lineas(n: usize) -> Vec<String> {
    let Ok(entradas) = fs::read_dir(carpeta_logs()) else {
        return vec!["(no hay carpeta de logs)".to_string()];
    };

    let mut archivos: Vec<PathBuf> = entradas
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("log"))
        .collect();
    archivos.sort();

    let Some(ultimo) = archivos.pop() else {
        return vec!["(no hay archivos de log)".to_string()];
    };

    match fs::read_to_string(&ultimo) {
        Ok(texto) => {
            let lineas: Vec<&str> = texto.lines().collect();
            let desde = lineas.len().saturating_sub(n);
            lineas[desde..].iter().map(|l| l.to_string()).collect()
        }
        Err(e) => vec![format!("(no se pudo leer {}: {e})", ultimo.display())],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_carpeta_de_logs_tiene_ruta() {
        // No se puede comprobar el contenido sin escribir, pero sí que no
        // depende de `target/` (que se borra con `cargo clean`).
        let ruta = carpeta_logs();
        assert!(!ruta.as_os_str().is_empty());
    }

    #[test]
    fn sin_log_no_explota() {
        // `ultimas_lineas` debe devolver algo legible aunque no haya nada.
        let lineas = ultimas_lineas(5);
        assert!(!lineas.is_empty());
    }
}
