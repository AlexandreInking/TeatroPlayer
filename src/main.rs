//! TeatroPlayer — interfaz.
//!
//! Primera versión **comprobable** de la app: ventana, lista de entradas,
//! inspector y transporte. Cubre T-UI-001, 002, 003, 004, 005 y 006.
//!
//! Consola visible SOLO en desarrollo. En release el ejecutable es de tipo
//! "windows" y no abre ninguna ventana de CMD (ver `Docs/07`): por eso todo lo
//! que antes iba por `println!` se ve ahora en el panel "Registro".
//!
//! Paleta y rótulos: `Docs/06-ux-y-diseno-visual.md`.

// Sin ventana de consola en release (T-OPS-003). Con la feature `dev-console`
// se puede tener igualmente para depurar sobre un binario optimizado
// (T-OPS-004):  cargo build --release --features dev-console
#![cfg_attr(
    all(not(debug_assertions), not(feature = "dev-console")),
    windows_subsystem = "windows"
)]

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use eframe::egui;
use rodio::Source;
use teatroplayer::engine::backend::{
    AudioBackend, OutputInfo, OutputSelection, TrackHandle, TrackState,
};
use teatroplayer::engine::model::{
    AudioSource, CueSpec, Curve, Entrance, ExitMode, EXTENSIONES_AUDIO, LoopMode, MilliDb,
    OnPrevious,
};
use teatroplayer::sesion::links;
use teatroplayer::sesion::modelo::{AutoFollow, AudioRef};
use teatroplayer::sesion::{AutoSaver, Cue, Historial, ModoApertura, Sesion};
use teatroplayer::show::{self, Bloqueo, ModoShow, Programador};
use tracing::{error, info, warn};
use teatroplayer::engine::rodio_backend::RodioBackend;

// ---------------------------------------------------------------------------
// Paleta (`Docs/06` §2)
// ---------------------------------------------------------------------------

const BG_APP: egui::Color32 = egui::Color32::from_rgb(0x1F, 0x22, 0x28);
const BG_PANEL: egui::Color32 = egui::Color32::from_rgb(0x15, 0x18, 0x1D);
const BG_ROW: egui::Color32 = egui::Color32::from_rgb(0x25, 0x2A, 0x33);
const BG_ROW_ALT: egui::Color32 = egui::Color32::from_rgb(0x2A, 0x2F, 0x39);
const BG_ROW_ACTIVE: egui::Color32 = egui::Color32::from_rgb(0x32, 0x38, 0x47);

const FG_STRONG: egui::Color32 = egui::Color32::from_rgb(0xEC, 0xEE, 0xF2);
const FG_BASE: egui::Color32 = egui::Color32::from_rgb(0xC7, 0xCB, 0xD3);
const FG_MUTE: egui::Color32 = egui::Color32::from_rgb(0x8A, 0x8F, 0x99);

const GO_GREEN: egui::Color32 = egui::Color32::from_rgb(0x3D, 0xDC, 0x84);
const STOP_RED: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x4D, 0x4D);
const WARN_AMBER: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xB0, 0x20);
const DUCK_ORANGE: egui::Color32 = egui::Color32::from_rgb(0xEF, 0x9F, 0x27);

/// Paleta de 8 colores para las entradas, asignada en orden de creación.
const CUE_COLORS: [egui::Color32; 8] = [
    egui::Color32::from_rgb(0xE0, 0x48, 0x48),
    egui::Color32::from_rgb(0xF0, 0x8C, 0x2E),
    egui::Color32::from_rgb(0xE8, 0xB3, 0x39),
    egui::Color32::from_rgb(0x5D, 0xBE, 0x6F),
    egui::Color32::from_rgb(0x38, 0xB5, 0xA6),
    egui::Color32::from_rgb(0x4F, 0x8F, 0xE0),
    egui::Color32::from_rgb(0x7B, 0x6F, 0xE0),
    egui::Color32::from_rgb(0xC5, 0x6F, 0xD0),
];

// Las extensiones las define el modelo (`EXTENSIONES_AUDIO`), para que el
// empaquetador y la interfaz usen exactamente la misma lista.

// ---------------------------------------------------------------------------
// Modelo de la interfaz
// ---------------------------------------------------------------------------

// El modo y su bloqueo viven en `teatroplayer::show` para poder testearlos sin
// abrir una ventana (T-SHOW-001).

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Entrada,
    Anterior,
    Salida,
    Repeticion,
    Volumen,
}

impl Tab {
    const TODOS: [Tab; 5] = [Tab::Entrada, Tab::Anterior, Tab::Salida, Tab::Repeticion, Tab::Volumen];
    fn rotulo(self) -> &'static str {
        match self {
            Tab::Entrada => "Cómo entra",
            Tab::Anterior => "Lo anterior",
            Tab::Salida => "Cómo sale",
            Tab::Repeticion => "Repetición",
            Tab::Volumen => "Volumen",
        }
    }
}

/// Lo que se guarda de una entrada para poder deshacer (T-UI-008).
///
/// No incluye la pista en curso: deshacer un cambio no debe cortar el audio
/// que está sonando.
#[derive(Clone)]
struct Fila {
    nombre: String,
    spec: CueSpec,
    auto_follow: AutoFollow,
    tecla: Option<String>,
    pad: bool,
    audio: AudioRef,
    fuente: Fuente,
    falta: bool,
}

/// De dónde salen los bytes de una entrada.
#[derive(Clone, Debug)]
enum Fuente {
    /// Un archivo suelto en el disco.
    Archivo(PathBuf),
    /// Una entrada dentro de un `.tpshow`; no se extrae a disco.
    Paquete { tpshow: PathBuf, entrada: String },
}

impl Fuente {
    fn como_audio_source(&self) -> AudioSource {
        match self {
            Fuente::Archivo(p) => AudioSource::File(p.clone()),
            Fuente::Paquete { tpshow, entrada } => AudioSource::Paquete {
                tpshow: tpshow.clone(),
                entrada: entrada.clone(),
            },
        }
    }

    /// Texto corto para la columna de la lista.
    fn resumen(&self) -> String {
        match self {
            Fuente::Archivo(p) => p.display().to_string(),
            Fuente::Paquete { tpshow, entrada } => format!(
                "{} → {}",
                tpshow
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                entrada
            ),
        }
    }
}

/// Una fila de la lista.
struct Entrada {
    nombre: String,
    spec: CueSpec,
    color: usize,
    pista: Option<Box<dyn TrackHandle>>,
    /// true si otra entrada la bajó por `duck`. Se limpia al volver a sonar.
    duck: bool,
    /// De dónde se leen los bytes.
    fuente: Fuente,
    /// Cómo encontrar el audio al reabrir la sesión (`Docs/04` §3).
    audio: AudioRef,
    /// Si al terminar esta entrada arranca la siguiente sola (T-SHOW-002).
    auto_follow: AutoFollow,
    /// Tecla que la dispara en modo Función ("F1"…"F12"), T-UI-007.
    tecla: Option<String>,
    /// true = también aparece en la franja de pads (T-UI-009).
    pad: bool,
    /// true si el audio no se encontró: la fila se pinta en rojo y sin GO.
    falta: bool,
}

impl Entrada {
    fn sonando(&self) -> bool {
        self.pista.as_ref().is_some_and(|p| !p.state().is_done())
    }

    fn estado(&self) -> Option<TrackState> {
        self.pista.as_ref().map(|p| p.state())
    }
}

struct App {
    backend: Arc<RodioBackend>,
    salidas: Vec<OutputInfo>,
    elegida: usize,

    entradas: Vec<Entrada>,
    seleccionada: Option<usize>,
    /// Modo y bloqueo de edición (T-SHOW-001).
    bloqueo: Bloqueo,
    tab: Tab,
    /// Dispara la entrada siguiente cuando toca (T-SHOW-002).
    programador: Programador,

    /// Dónde está guardada la obra ahora mismo. `None` = nunca se guardó.
    ruta: Option<PathBuf>,
    /// Hay cambios sin guardar.
    sucio: bool,
    /// La sesión se abrió de una versión futura: se puede mirar, no guardar.
    solo_lectura: bool,
    auto: AutoSaver,

    registro: Vec<String>,
    iniciado: bool,
    arranque: Instant,
    ver_registro: bool,
    /// Ventana de diagnóstico (T-OPS-002), se abre con Ctrl+Shift+D.
    ver_diagnostico: bool,
    /// Últimas líneas leídas del archivo de log para esa ventana.
    log_leido: Vec<String>,
    /// Guardia del escritor de logs: vive mientras la app corre.
    guardia_log: Option<tracing_appender::non_blocking::WorkerGuard>,
    /// Última entrada disparada, para que Espacio lance la siguiente (T-UI-007).
    ultima_disparada: Option<usize>,
    /// Instantáneas para Ctrl+Z / Ctrl+Shift+Z (T-UI-008).
    historial: Historial<Vec<Fila>>,
    /// Obra que hay que abrir en cuanto arranque la ventana: llega de la línea
    /// de comandos, que es lo que pasa Windows al hacer doble clic en un
    /// `.tpshow` una vez asociada la extensión (T-REL-004).
    pendiente_abrir: Option<PathBuf>,
}

impl App {
    fn new() -> Self {
        Self {
            backend: RodioBackend::new(),
            salidas: Vec::new(),
            elegida: 0,
            entradas: Vec::new(),
            seleccionada: None,
            bloqueo: Bloqueo::nuevo(),
            tab: Tab::Entrada,
            programador: Programador::nuevo(),
            ruta: None,
            sucio: false,
            solo_lectura: false,
            auto: AutoSaver::nuevo(),
            registro: Vec::new(),
            iniciado: false,
            arranque: Instant::now(),
            ver_registro: false,
            ver_diagnostico: false,
            log_leido: Vec::new(),
            guardia_log: None,
            ultima_disparada: None,
            historial: Historial::nuevo(50),
            pendiente_abrir: None,
        }
    }

    /// Escribe en el panel "Registro" y, si empieza por un nivel explícito,
    /// también al archivo de log (T-OPS-001).
    fn anotar(&mut self, linea: impl Into<String>) {
        let texto = linea.into();
        let t = self.arranque.elapsed().as_secs_f32();
        self.registro.push(format!("{t:6.2}s  {texto}"));

        // En teatro lo único que queda después es el log: nadie va a reproducir
        // el fallo en el momento.
        if let Some(resto) = texto.strip_prefix("AVISO: ") {
            warn!("{resto}");
        } else if texto.starts_with("no se pudo") || texto.contains("error") {
            error!("{texto}");
        } else {
            info!("{texto}");
        }
    }

    /// Se hace en el primer `logic`, no en `new()`: así un fallo de audio se ve
    /// en la ventana en vez de cerrar el programa sin más.
    fn iniciar(&mut self) {
        self.iniciado = true;

        match self.backend.outputs() {
            Ok(salidas) => {
                self.elegida = salidas.iter().position(|s| s.is_default).unwrap_or(0);
                for s in &salidas {
                    let marca = if s.is_default { " (predeterminada)" } else { "" };
                    self.anotar(format!("salida {}: {}{}", s.id, s.name, marca));
                }
                self.salidas = salidas;
            }
            Err(e) => self.anotar(format!("no se pudo enumerar salidas: {e}")),
        }

        match self.backend.open(OutputSelection::SystemDefault) {
            Ok(()) => {
                let config = self
                    .backend
                    .output_config()
                    .map(|(hz, canales)| format!(" a {hz} Hz / {canales} canales")
                        .to_string())
                    .unwrap_or_default();
                self.anotar(format!("salida abierta{config}"));
            }
            Err(e) => self.anotar(format!("no se pudo abrir la salida: {e}")),
        }
    }

    /// Foto del estado editable, para el historial.
    fn instantanea(&self) -> Vec<Fila> {
        self.entradas
            .iter()
            .map(|e| Fila {
                nombre: e.nombre.clone(),
                spec: e.spec.clone(),
                auto_follow: e.auto_follow,
                tecla: e.tecla.clone(),
                pad: e.pad,
                audio: e.audio.clone(),
                fuente: e.fuente.clone(),
                falta: e.falta,
            })
            .collect()
    }

    /// Vuelve a una foto. Las posiciones que siguen existiendo **conservan su
    /// pista en curso**: deshacer no debe cortar lo que está sonando.
    fn aplicar_instantanea(&mut self, filas: Vec<Fila>) {
        let total = filas.len();
        for (i, f) in filas.into_iter().enumerate() {
            if let Some(e) = self.entradas.get_mut(i) {
                e.nombre = f.nombre;
                e.spec = f.spec;
                e.auto_follow = f.auto_follow;
                e.tecla = f.tecla;
                e.pad = f.pad;
                e.audio = f.audio;
                e.fuente = f.fuente;
                e.falta = f.falta;
            } else {
                self.entradas.push(Entrada {
                    nombre: f.nombre,
                    spec: f.spec,
                    color: i % CUE_COLORS.len(),
                    pista: None,
                    duck: false,
                    fuente: f.fuente,
                    audio: f.audio,
                    auto_follow: f.auto_follow,
                    tecla: f.tecla,
                    pad: f.pad,
                    falta: f.falta,
                });
            }
        }
        self.entradas.truncate(total);
        if self.seleccionada.is_some_and(|i| i >= self.entradas.len()) {
            self.seleccionada = None;
        }
    }

    // --- lista ------------------------------------------------------------

    fn abrir_carpeta(&mut self) {
        let alguna = rfd::FileDialog::new().set_title("Elegir la carpeta de audios").pick_folder();
        let Some(carpeta) = alguna else { return };

        let mut encontrados: Vec<PathBuf> = Vec::new();
        if let Ok(dir) = std::fs::read_dir(&carpeta) {
            for e in dir.flatten() {
                let p = e.path();
                if es_audio(&p) {
                    encontrados.push(p);
                }
            }
        }
        encontrados.sort();

        if encontrados.is_empty() {
            self.anotar(format!(
                "no se encontraron audios en {}",
                carpeta.display()
            ));
            return;
        }

        let n = encontrados.len();
        for p in encontrados {
            self.empujar_entrada(p);
        }
        self.anotar(format!("{n} audios cargados desde {}", carpeta.display()));
        if self.seleccionada.is_none() && !self.entradas.is_empty() {
            self.seleccionada = Some(0);
        }
    }

    fn anadir_audio(&mut self) {
        let mut d = rfd::FileDialog::new().set_title("Elegir un audio");
        for ext in EXTENSIONES_AUDIO {
            d = d.add_filter("Audio", &[ext]);
        }
        if let Some(p) = d.pick_file() {
            self.anotar(format!("añadido {}", p.display()));
            self.empujar_entrada(p);
            self.seleccionada = Some(self.entradas.len() - 1);
        }
    }

    fn empujar_entrada(&mut self, archivo: PathBuf) {
        let nombre = archivo
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| archivo.display().to_string());
        let file_name = archivo
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| nombre.clone());

        // Metadatos para poder reencontrar el audio aunque se mueva o renombre.
        let mut audio = AudioRef {
            file_name,
            rel_path: archivo
                .file_name()
                .map(|n| format!("audio/{}", n.to_string_lossy()))
                .unwrap_or_default(),
            abs_path: Some(archivo.to_string_lossy().to_string()),
            ..Default::default()
        };
        links::completar_metadatos(&mut audio, &archivo);

        let color = self.entradas.len() % CUE_COLORS.len();
        self.entradas.push(Entrada {
            nombre,
            spec: CueSpec::simple(),
            color,
            pista: None,
            duck: false,
            fuente: Fuente::Archivo(archivo),
            audio,
            auto_follow: AutoFollow::None,
            tecla: None,
            pad: false,
            falta: false,
        });
        self.historial.registrar(&self.instantanea());
        self.sucio = true;
        self.auto.pedir();
    }

    // --- sesión ------------------------------------------------------------

    fn obra_nueva(&mut self) {
        self.entradas.clear();
        self.seleccionada = None;
        self.ruta = None;
        self.sucio = false;
        self.solo_lectura = false;
        self.anotar("obra nueva");
    }

    /// Pide una obra con el diálogo nativo y la abre.
    fn abrir(&mut self) {
        let d = rfd::FileDialog::new()
            .set_title("Abrir una obra")
            .add_filter("Obra de TeatroPlayer", &["tpshow"]);
        let Some(ruta) = d.pick_file() else { return };
        self.abrir_archivo(ruta);
    }

    /// Abre un `.tpshow`. Los audios se leen de dentro del paquete, sin extraer.
    fn abrir_archivo(&mut self, ruta: PathBuf) {
        let texto = match teatroplayer::paquete::leer_sesion(&ruta) {
            Ok(t) => t,
            Err(e) => {
                self.anotar(format!("no se pudo leer: {e}"));
                return;
            }
        };
        let (sesion, modo) = match Sesion::desde_json(&texto) {
            Ok(x) => x,
            Err(e) => {
                self.anotar(format!("la sesión no sirve: {e}"));
                return;
            }
        };
        let dentro = teatroplayer::paquete::lista_audios(&ruta).unwrap_or_default();

        self.entradas.clear();
        for (i, cue) in sesion.cues.iter().enumerate() {
            let entrada = format!("audio/{}", cue.audio.file_name);
            // Sin nombre de archivo no hay nada que buscar: la fila sale
            // como FALTANTE y su GO queda deshabilitado.
            let falta =
                cue.audio.file_name.is_empty() || !dentro.contains(&cue.audio.file_name);
            let fuente = Fuente::Paquete { tpshow: ruta.clone(), entrada };
            self.entradas.push(Entrada {
                nombre: cue.nombre.clone(),
                spec: cue.spec.clone(),
                color: i % CUE_COLORS.len(),
                pista: None,
                duck: false,
                fuente,
                audio: cue.audio.clone(),
                auto_follow: cue.auto_follow,
                tecla: cue.tecla.clone(),
                pad: cue.pad,
                falta,
            });
        }

        let cuantos = self.entradas.len();
        let faltantes = self.entradas.iter().filter(|e| e.falta).count();
        self.seleccionada = if cuantos > 0 { Some(0) } else { None };
        self.solo_lectura = modo == ModoApertura::SoloLectura;
        self.ruta = Some(ruta.clone());
        self.sucio = false;

        self.anotar(format!("abierta {} ({} entradas)", ruta.display(), cuantos));
        if faltantes > 0 {
            self.anotar(format!("AVISO: {faltantes} audios no están en el paquete"));
        }
        if self.solo_lectura {
            self.anotar("AVISO: es de una versión más nueva; se abre sólo para mirar");
        }
    }

    /// Construye la `Sesion` a partir de lo que hay en pantalla.
    fn sesion_actual(&self) -> Sesion {
        let nombre = self
            .ruta
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Sin título".to_string());

        Sesion {
            version: teatroplayer::sesion::VERSION,
            nombre,
            cues: self
                .entradas
                .iter()
                .map(|e| Cue {
                    nombre: e.nombre.clone(),
                    audio: e.audio.clone(),
                    spec: e.spec.clone(),
                    tecla: e.tecla.clone(),
                    nota: String::new(),
                    auto_follow: e.auto_follow,
                    pad: e.pad,
                })
                .collect(),
        }
    }

    fn guardar_como(&mut self) {
        let d = rfd::FileDialog::new()
            .set_title("Guardar la obra como")
            .add_filter("Obra de TeatroPlayer", &["tpshow"])
            .set_file_name("MiObra.tpshow");
        let Some(destino) = d.save_file() else { return };
        self.ruta = Some(destino);
        self.guardar();
    }

    fn guardar(&mut self) {
        if self.solo_lectura {
            self.anotar("no se puede guardar: la obra es de una versión más nueva");
            return;
        }
        let Some(ruta) = self.ruta.clone() else {
            self.guardar_como();
            return;
        };

        let sesion = self.sesion_actual();
        let json = match sesion.a_json() {
            Ok(j) => j,
            Err(e) => {
                self.anotar(format!("no se pudo serializar: {e}"));
                return;
            }
        };

        // Todos los audios, vengan del disco o de un paquete. Los que vinieron
        // de un paquete se copian desde ese paquete en streaming, así que
        // guardar una obra abierta desde un .tpshow ya no la deja sin audios.
        let mut audios: Vec<(String, teatroplayer::paquete::OrigenAudio)> = Vec::new();
        let mut perdidos = 0;
        for e in &self.entradas {
            if e.audio.file_name.is_empty() {
                perdidos += 1;
                continue;
            }
            let origen = match &e.fuente {
                Fuente::Archivo(p) if p.is_file() => {
                    teatroplayer::paquete::OrigenAudio::Archivo(p.clone())
                }
                Fuente::Archivo(_) => {
                    perdidos += 1;
                    continue;
                }
                Fuente::Paquete { tpshow, entrada } => {
                    teatroplayer::paquete::OrigenAudio::Paquete {
                        tpshow: tpshow.clone(),
                        entrada: entrada.clone(),
                    }
                }
            };
            audios.push((e.audio.file_name.clone(), origen));
        }

        match teatroplayer::paquete::escribir(&ruta, &json, &audios) {
            Ok(()) => {
                self.sucio = false;
                self.anotar(format!(
                    "guardado {} ({} entradas, {} audios)",
                    ruta.display(),
                    sesion.cues.len(),
                    audios.len()
                ));
                if perdidos > 0 {
                    self.anotar(format!(
                        "AVISO: {perdidos} entradas se guardaron sin su audio"
                    ));
                }
            }
            Err(e) => self.anotar(format!("no se pudo guardar: {e}")),
        }
    }

    fn quitar(&mut self, i: usize) {
        self.historial.registrar(&self.instantanea());
        if let Some(e) = self.entradas.get(i) {
            if let Some(p) = &e.pista {
                p.stop();
            }
        }
        self.entradas.remove(i);
        self.sucio = true;
        self.auto.pedir();
        if self.seleccionada == Some(i) {
            self.seleccionada = None;
        } else if let Some(s) = self.seleccionada {
            if s > i {
                self.seleccionada = Some(s - 1);
            }
        }
    }

    fn mover(&mut self, i: usize, delta: isize) {
        self.historial.registrar(&self.instantanea());
        let destino = i as isize + delta;
        if destino < 0 || destino >= self.entradas.len() as isize {
            return;
        }
        self.entradas.swap(i, destino as usize);
        self.sucio = true;
        self.auto.pedir();
        if self.seleccionada == Some(i) {
            self.seleccionada = Some(destino as usize);
        }
    }

    // --- transporte -------------------------------------------------------

    /// GO: aplica `on_previous` a lo que esté sonando y suena esta entrada.
    fn ir(&mut self, i: usize) {
        let Some(entrada) = self.entradas.get(i) else { return };
        if entrada.falta {
            self.anotar(format!("FALTA EL AUDIO de '{}'", entrada.nombre));
            return;
        }
        let spec = entrada.spec.clone();
        let fuente = entrada.fuente.clone();

        // 1. Qué pasa con lo que ya sonaba. `entrance` y `on_previous` son
        //    independientes: eso es lo que permite todas las combinaciones.
        for (j, e) in self.entradas.iter_mut().enumerate() {
            if j == i {
                continue;
            }
            let Some(p) = e.pista.as_ref() else { continue };
            if p.state().is_done() {
                continue;
            }
            match spec.on_previous {
                OnPrevious::Keep => {}
                OnPrevious::Stop => p.stop(),
                OnPrevious::FadeOut { duration, curve } => p.stop_after(duration, curve),
                OnPrevious::Duck { duration, curve, level_percent } => {
                    p.fade_to(level_percent as f32 / 100.0, duration, curve);
                    e.duck = true;
                }
            }
        }

        // 2. Suena esta.
        match self.backend.play(&spec, fuente.como_audio_source()) {
            Ok(pista) => {
                let nombre = self.entradas[i].nombre.clone();
                self.anotar(format!("GO: {nombre}"));
                self.ultima_disparada = Some(i);

                // Auto-follow: si esta entrada pide que la siguiente arranque
                // sola, se programa aquí (T-SHOW-002).
                let follow = self.entradas[i].auto_follow;
                let hay_siguiente = i + 1 < self.entradas.len();
                let ahora = self.arranque.elapsed().as_millis() as u64;
                if self.programador.al_arrancar(i, follow, ahora, hay_siguiente) {
                    self.anotar(format!(
                        "auto: la entrada {} arrancará {}",
                        i + 2,
                        match follow {
                            AutoFollow::AfterMs(ms) => format!("en {ms} ms"),
                            AutoFollow::WhenThisEnds => "al terminar esta".to_string(),
                            AutoFollow::None => String::new(),
                        }
                    ));
                }
                self.entradas[i].pista = Some(pista);
                // Si estaba bajada por un duck de otra entrada, vuelve al 100 %.
                self.entradas[i].duck = false;
                self.seleccionada = Some(i);
            }
            Err(e) => self.anotar(format!("no se pudo reproducir: {e}")),
        }
    }

    /// Parada de emergencia (T-SHOW-003): fade corto y corte.
    ///
    /// El fade no es un adorno: cortar un PCM a mitad de ciclo se oye como un
    /// clic, y en una sala eso suena a fallo del equipo.
    fn parar_todo(&mut self) {
        let fade = Duration::from_millis(show::FADE_EMERGENCIA_MS);
        self.backend.stop_all_con_fade(fade);
        self.programador.cancelar();
        for e in &mut self.entradas {
            if let Some(p) = e.pista.take() {
                p.stop();
            }
            e.duck = false;
        }
        self.backend.stop_all();
        self.anotar(format!(
            "parada de emergencia (fade {} ms)",
            show::FADE_EMERGENCIA_MS
        ));
    }

    fn probar_salida(&mut self) {
        let mut spec = CueSpec::simple();
        spec.volume = MilliDb::from_db(-12.0);
        match tono_en_memoria() {
            Ok(cursor) => match self.backend.play(&spec, AudioSource::Memory(cursor)) {
                Ok(_) => self.anotar("prueba de salida: tono de 440 Hz, 1 s, -12 dBFS"),
                Err(e) => self.anotar(format!("no se pudo reproducir: {e}")),
            },
            Err(e) => self.anotar(format!("no se pudo generar el tono: {e}")),
        }
    }

    fn limpiar_terminadas(&mut self) {
        for e in &mut self.entradas {
            if e.pista.as_ref().is_some_and(|p| p.state().is_done()) {
                e.pista = None;
            }
        }
    }
}

fn es_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONES_AUDIO.contains(&e.to_lowercase().as_str()))
        && p.is_file()
}

// ---------------------------------------------------------------------------
// Iconos
// ---------------------------------------------------------------------------
//
// Se dibujan con `Painter` en vez de cargar assets: así la app es portable y
// el aspecto no depende de qué fuentes tenga el sistema para glifos Unicode.
// Cada uno es geometría pura: un `+`, una carpeta, una flecha...

#[derive(Clone, Copy)]
enum IconKind {
    Nueva, Abrir, Guardar, AnadirAudio, Registro,
    Subir, Bajar, Quitar, Probar,
}

fn dibujar_icono(painter: &egui::Painter, rect: egui::Rect, kind: IconKind, color: egui::Color32) {
    let stroke = egui::Stroke { width: 1.5, color };
    let fill = color;
    let c = rect.center();
    let w = rect.width();
    let tl = rect.left_top();
    let br = rect.right_bottom();
    match kind {
        IconKind::Nueva => {
            let r = w * 0.5;
            painter.circle_stroke(c, r, stroke);
            let d = r * 0.7;
            painter.line_segment([egui::pos2(c.x, c.y - d), egui::pos2(c.x, c.y + d)], stroke);
            painter.line_segment([egui::pos2(c.x - d, c.y), egui::pos2(c.x + d, c.y)], stroke);
        }
        IconKind::Abrir => {
            let hh = rect.height() * 0.22;
            // la pestaña que sobresale por arriba
            painter.line_segment(
                [egui::pos2(tl.x, tl.y + hh), egui::pos2(tl.x + w * 0.55, tl.y + hh)],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(tl.x + w * 0.55, tl.y + hh),
                    egui::pos2(tl.x + w * 0.55 + 3.0, tl.y + hh + 3.0),
                ],
                stroke,
            );
            // el cuerpo
            // cuerpo de la carpeta: cuatro lineas
            let body = egui::Rect::from_min_max(
                egui::pos2(tl.x, tl.y + hh + 3.0),
                egui::pos2(br.x, br.y),
            );
            painter.line_segment([body.left_top(), body.right_top()], stroke);
            painter.line_segment([body.left_bottom(), body.right_bottom()], stroke);
            painter.line_segment([body.left_top(), body.left_bottom()], stroke);
            painter.line_segment([body.right_top(), body.right_bottom()], stroke);
        }
        IconKind::Guardar => {
            // bandeja
            let by = br.y - 1.0;
            painter.line_segment([egui::pos2(tl.x, by), egui::pos2(br.x, by)], stroke);
            // flecha hacia abajo
            let cx = c.x;
            let arm = w * 0.3;
            painter.line_segment(
                [egui::pos2(cx, tl.y + 1.0), egui::pos2(cx, by - 1.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(cx - arm, by - 1.0 - arm * 0.6), egui::pos2(cx, by - 1.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(cx + arm, by - 1.0 - arm * 0.6), egui::pos2(cx, by - 1.0)],
                stroke,
            );
        }
        IconKind::AnadirAudio => {
            // nota musical: cabeza rellena + asta + banderín
            let r = w * 0.22;
            let head = egui::Pos2::new(tl.x + r + 2.0, br.y - r - 2.0);
            painter.circle_filled(head, r, fill);
            let stem_top = egui::Pos2::new(head.x + r * 0.9, tl.y + 2.0);
            painter.line_segment([egui::Pos2::new(head.x, head.y), stem_top], stroke);
            painter.line_segment(
                [stem_top, egui::Pos2::new(stem_top.x, stem_top.y + w * 0.25)],
                stroke,
            );
            painter.line_segment(
                [
                    egui::Pos2::new(stem_top.x, stem_top.y + w * 0.25),
                    egui::Pos2::new(stem_top.x + w * 0.18, stem_top.y + w * 0.31),
                ],
                stroke,
            );
        }
        IconKind::Registro => {
            // tres líneas (lista / log)
            let pad = w * 0.18;
            let n = 3;
            let step = (rect.height() - pad * 2.0) / (n - 1) as f32;
            for i in 0..n {
                let y = tl.y + pad + step * i as f32;
                painter.line_segment(
                    [egui::pos2(tl.x + pad, y), egui::pos2(br.x - pad, y)],
                    stroke,
                );
            }
        }
        IconKind::Subir | IconKind::Bajar => {
            let cx = c.x;
            let arm = w * 0.34;
            painter.line_segment(
                [egui::pos2(cx, tl.y + 1.0), egui::pos2(cx, br.y - 1.0)],
                stroke,
            );
            let (head_y, va_hacia_arriba) = match kind {
                IconKind::Subir => (tl.y + 1.0 + arm, true),
                _ => (br.y - 1.0 - arm, false),
            };
            let head_apex_y = if va_hacia_arriba { tl.y + 1.0 } else { br.y - 1.0 };
            painter.line_segment(
                [egui::pos2(cx - arm, head_y), egui::pos2(cx, head_apex_y)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(cx + arm, head_y), egui::pos2(cx, head_apex_y)],
                stroke,
            );
        }
        IconKind::Quitar => {
            let pad = w * 0.2;
            painter.line_segment(
                [
                    egui::pos2(tl.x + pad, tl.y + pad),
                    egui::pos2(br.x - pad, br.y - pad),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(br.x - pad, tl.y + pad),
                    egui::pos2(tl.x + pad, br.y - pad),
                ],
                stroke,
            );
        }
        IconKind::Probar => {
            // triángulo de play, relleno
            let pad = w * 0.2;
            let pts = [
                egui::pos2(tl.x + pad, tl.y + pad),
                egui::pos2(tl.x + pad, br.y - pad),
                egui::pos2(br.x - pad * 0.6, c.y),
            ];
            painter.add(egui::Shape::convex_polygon(
                pts.to_vec(),
                fill,
                egui::Stroke::NONE,
            ));
        }
    }
}

/// Botón de la barra superior: icono + texto. Tamaño ajustado al contenido.
///
/// `bg_activo` permite "encender" el botón (lo usa el toggle de Registro).
fn boton_icono(
    ui: &mut egui::Ui,
    icono: IconKind,
    texto: &str,
    bg_activo: Option<egui::Color32>,
) -> egui::Response {
    let icon_size = 14.0_f32;
    let pad_x = 8.0_f32;
    let gap = 5.0_f32;
    let h = 26.0_f32;
    let font = egui::FontId::proportional(13.0);
    let galley = ui
        .painter()
        .layout_no_wrap(texto.to_string(), font.clone(), FG_BASE);
    let text_w = galley.size().x;
    let w = pad_x + icon_size + gap + text_w + pad_x;

    let (rect, response) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());

    let bg = if let Some(a) = bg_activo {
        a
    } else if response.is_pointer_button_down_on() {
        BG_ROW_ACTIVE
    } else if response.hovered() {
        BG_ROW_ALT
    } else {
        egui::Color32::TRANSPARENT
    };
    if bg != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(4), bg);
    }

    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + pad_x, rect.min.y + (h - icon_size) / 2.0),
        egui::vec2(icon_size, icon_size),
    );
    let icon_color = if bg_activo.is_some() || response.hovered() { FG_STRONG } else { FG_BASE };
    dibujar_icono(ui.painter(), icon_rect, icono, icon_color);

    let text_color = if bg_activo.is_some() || response.hovered() { FG_STRONG } else { FG_BASE };
    ui.painter().text(
        egui::pos2(icon_rect.max.x + gap, rect.center().y),
        egui::Align2::LEFT_CENTER,
        texto,
        font,
        text_color,
    );


    response
}

/// Cuadrado pequeño con solo icono: las flechas y la X de la fila.
fn boton_icono_chico(
    ui: &mut egui::Ui,
    icono: IconKind,
    color: Option<egui::Color32>,
) -> egui::Response {
    let size = 24.0_f32;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let default_color = color.unwrap_or(FG_BASE);
    let paint_color = if response.hovered() { FG_STRONG } else { default_color };

    let bg = if response.is_pointer_button_down_on() {
        BG_ROW_ACTIVE
    } else if response.hovered() {
        BG_ROW_ALT
    } else {
        egui::Color32::TRANSPARENT
    };
    if bg != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(3), bg);
    }

    let icon_rect = rect.shrink(4.0);
    dibujar_icono(ui.painter(), icon_rect, icono, paint_color);


    response
}


/// Genera un WAV de 440 Hz en memoria con `rodio::wav_to_writer`.
///
/// Así la prueba de salida no depende de que exista `tests/fixtures`, que no se
/// distribuye con el ejecutable. Y de paso ejercita la ruta
/// `AudioSource::Memory`, que es la que usará el `.tpshow` (hito FMT).
fn tono_en_memoria() -> Result<Cursor<Vec<u8>>> {
    let mut buf = Cursor::new(Vec::new());
    let tono = rodio::source::SineWave::new(440.0)
        .take_duration(Duration::from_secs(1))
        .amplify(0.5);
    rodio::wav_to_writer(tono, &mut buf).map_err(|e| anyhow!("wav: {e}"))?;
    buf.set_position(0);
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Arranque
// ---------------------------------------------------------------------------

/// Comandos de paquete (`Docs/13` §6). Se atienden antes de abrir la ventana
/// porque no necesitan interfaz: son para depurar y para mandar una obra.
fn cli() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return None;
    }

    match args[1].as_str() {
        "pack" if args.len() >= 4 => {
            let (carpeta, salida) = (Path::new(&args[2]), Path::new(&args[3]));
            match teatroplayer::paquete::pack(carpeta, salida) {
                Ok(()) => {
                    println!("empaquetado {}", salida.display());
                    Some(0)
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    Some(1)
                }
            }
        }
        "unpack" if args.len() >= 4 => {
            let (paquete, destino) = (Path::new(&args[2]), Path::new(&args[3]));
            match teatroplayer::paquete::unpack(paquete, destino) {
                Ok(n) => {
                    println!("extraidas {n} entradas en {}", destino.display());
                    Some(0)
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    Some(1)
                }
            }
        }
        "verify" if args.len() >= 3 => {
            match teatroplayer::paquete::verify(Path::new(&args[2])) {
                Ok(informe) if informe.ok() => {
                    println!("OK: {} entradas, sin problemas", informe.entradas);
                    Some(0)
                }
                Ok(informe) => {
                    println!("FALLA: {} problemas", informe.problemas.len());
                    for p in &informe.problemas {
                        println!("  - {p}");
                    }
                    // exit 2 = paquete invalido, como pide la especificacion.
                    Some(2)
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    Some(1)
                }
            }
        }
        "diff" if args.len() >= 4 => {
            match teatroplayer::paquete::diff(Path::new(&args[2]), Path::new(&args[3])) {
                Ok(d) => {
                    println!("iguales   : {}", d.iguales);
                    println!("cambiaron : {:?}", d.cambiaron);
                    println!("solo en A : {:?}", d.solo_en_a);
                    println!("solo en B : {:?}", d.solo_en_b);
                    Some(0)
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    Some(1)
                }
            }
        }
        // Doble clic en un `.tpshow`: Windows arranca el programa pasándole la
        // ruta. No se trata aquí porque hace falta la ventana para mostrarlo.
        _ if args[1].to_lowercase().ends_with(".tpshow") => None,

        "help" | "--help" | "-h" => {
            println!("TeatroPlayer {}", env!("CARGO_PKG_VERSION"));
            println!();
            println!("  teatroplayer                                abre la interfaz");
            println!("  teatroplayer pack   <carpeta> <salida.tpshow>");
            println!("  teatroplayer unpack <entrada.tpshow> <carpeta>");
            println!("  teatroplayer verify <archivo.tpshow>");
            println!("  teatroplayer diff   <a.tpshow> <b.tpshow>");
            Some(0)
        }
        _ => None,
    }
}

fn main() -> eframe::Result<()> {
    if let Some(codigo) = cli() {
        std::process::exit(codigo);
    }

    // El guard debe vivir mientras la app corra: al soltarlo se dejarían de
    // vaciar los logs pendientes. Se mantiene en el App.
    let guardia_log = teatroplayer::diagnostico::iniciar();
    info!(
        "TeatroPlayer {} arrancando",
        env!("CARGO_PKG_VERSION")
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([900.0, 520.0]),
        ..Default::default()
    };

    eframe::run_native(
        "TeatroPlayer",
        options,
        Box::new(|cc| {
            aplicar_paleta(&cc.egui_ctx);
            let mut app = App::new();
            app.guardia_log = guardia_log;
            let primero = std::env::args().nth(1).unwrap_or_default();
            if primero.to_lowercase().ends_with(".tpshow") {
                app.pendiente_abrir = Some(PathBuf::from(primero));
            }
            Ok(Box::new(app))
        }),
    )
}

fn aplicar_paleta(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG_APP;
    visuals.window_fill = BG_APP;
    visuals.extreme_bg_color = BG_PANEL;
    visuals.widgets.noninteractive.bg_fill = BG_ROW;
    visuals.widgets.inactive.bg_fill = BG_ROW_ALT;
    visuals.selection.bg_fill = BG_ROW_ACTIVE;
    visuals.selection.stroke.color = GO_GREEN;
    visuals.override_text_color = Some(FG_BASE);
    ctx.set_visuals(visuals);
}

// ---------------------------------------------------------------------------
// Interfaz
// ---------------------------------------------------------------------------

impl eframe::App for App {
    /// Sin UI: sólo estado. Va aquí y no en `ui()` porque egui 0.36 llama a
    /// `logic` también con la ventana oculta, y porque desde aquí se puede
    /// pedir un repintado sin dibujar nada.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.iniciado {
            self.iniciar();
        }
        self.limpiar_terminadas();

        // T-REL-004: si arrancamos con una obra (doble clic en un .tpshow),
        // se abre aquí, cuando la ventana ya existe y se puede anotar el log.
        if let Some(ruta) = self.pendiente_abrir.take() {
            self.abrir_archivo(ruta);
        }

        // T-UI-008: Ctrl+Z / Ctrl+Shift+Z, pero sólo en Diseño. En Función,
        // deshacer por accidente en medio de una obra sería un desastre.
        if self.bloqueo.permite_editar() {
            let (z, y) = ctx.input(|i| {
                (
                    i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                    i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::Z),
                )
            });
            if z {
                let actual = self.instantanea();
                if let Some(anterior) = self.historial.deshacer(&actual) {
                    self.aplicar_instantanea(anterior);
                    self.sucio = true;
                    self.auto.pedir();
                    self.anotar("deshecho");
                }
            }
            if y {
                let actual = self.instantanea();
                if let Some(siguiente) = self.historial.rehacer(&actual) {
                    self.aplicar_instantanea(siguiente);
                    self.sucio = true;
                    self.auto.pedir();
                    self.anotar("rehecho");
                }
            }
        }

        // T-OPS-002: Ctrl+Shift+D abre el diagnóstico.
        ctx.input(|i| {
            if i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::D) {
                self.ver_diagnostico = !self.ver_diagnostico;
                if self.ver_diagnostico {
                    self.log_leido = teatroplayer::diagnostico::ultimas_lineas(50);
                    self.anotar("diagnóstico abierto");
                }
            }
        });

        // T-UI-007: las teclas F1..F12 disparan su entrada. Sólo en Función:
        // en Diseño una F no debería sonar nada por un despiste.
        if self.bloqueo.en_funcion() {
            let teclas = [
                egui::Key::F1, egui::Key::F2, egui::Key::F3, egui::Key::F4,
                egui::Key::F5, egui::Key::F6, egui::Key::F7, egui::Key::F8,
                egui::Key::F9, egui::Key::F10, egui::Key::F11, egui::Key::F12,
            ];
            let mut pulsada: Option<usize> = None;
            ctx.input(|i| {
                for (n, k) in teclas.iter().enumerate() {
                    if i.key_pressed(*k) {
                        pulsada = Some(n);
                        break;
                    }
                }
            });
            if let Some(n) = pulsada {
                let nombre = format!("F{}", n + 1);
                let asignada = self
                    .entradas
                    .iter()
                    .position(|e| e.tecla.as_deref() == Some(nombre.as_str()));
                // Si esa F está asignada a una entrada, se usa; si no, la F
                // dispara la entrada de esa posición (F1 = la primera), que es lo
                // que pide el plan y lo que hará cualquiera sin configurar nada.
                let destino = asignada.or(Some(n)).filter(|i| *i < self.entradas.len());
                if let Some(i) = destino {
                    self.ir(i);
                }
            }

            // Espacio = siguiente GO. Es la tecla que se aprieta en una función
            // real: no hay que mirar la pantalla para saber dónde está.
            if ctx.input(|i| i.key_pressed(egui::Key::Space)) && !self.entradas.is_empty() {
                let siguiente = match self.ultima_disparada {
                    Some(u) if u + 1 < self.entradas.len() => u + 1,
                    _ => 0,
                };
                self.ir(siguiente);
            }
        }

        // T-SHOW-001: doble Esc para salir del modo Función.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            let ahora = self.arranque.elapsed().as_millis() as u64;
            if self.bloqueo.esc(ahora) {
                self.anotar("doble Esc: vuelta a Diseño");
            }
        }

        // T-SHOW-002: auto-follow, por tiempo o por fin de la pista anterior.
        let ahora = self.arranque.elapsed().as_millis() as u64;
        let por_tiempo = self.programador.vencida(ahora);
        let por_fin = self
            .programador
            .pendiente()
            .filter(|c| c.en_ms.is_none())
            .and_then(|c| {
                let anterior = c.indice.saturating_sub(1);
                let termino = self
                    .entradas
                    .get(anterior)
                    .and_then(|e| e.pista.as_ref())
                    .is_none_or(|p| p.state().is_done());
                termino.then_some(c.indice)
            });

        if let Some(i) = por_tiempo.or(por_fin) {
            self.programador.cancelar();
            self.ir(i);
        }

        // Autoguardado: se pide al editar y se ejecuta tras 1,5 s sin cambios.
        if self.auto.listo() && self.sucio && self.ruta.is_some() && !self.solo_lectura {
            self.guardar();
        }

        if self.entradas.iter().any(|e| e.sonando()) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // egui 0.36 unificó TopBottomPanel/SidePanel en `Panel`; el tamaño se
        // da con `exact_size` (ancho en paneles laterales, alto en los otros).
        egui::Panel::top("barra")
            .exact_size(46.0)
            .show(ui, |ui| self.barra_superior(ui));

        egui::Panel::bottom("transporte")
            .exact_size(58.0)
            .show(ui, |ui| self.transporte(ui));

        if self.bloqueo.permite_editar() {
            egui::Panel::right("inspector")
                .exact_size(360.0)
                .show(ui, |ui| self.inspector(ui));
        }

        // T-UI-010: en Función, un contador grande arriba. Se mira de reojo
        // desde lejos, así que va arriba y bien grande.
        if self.bloqueo.en_funcion() {
            // Sin animación: el panel aparece y desaparece con el audio, que es
            // más fácil de seguir que una transición.
            if self.entradas.iter().any(|e| e.sonando()) {
                egui::Panel::top("contador").exact_size(96.0).show(ui, |ui| {
                    self.contador_grande(ui);
                });
            }
        }

        // T-UI-009: franja de pads abajo, sólo si hay al menos un pad.
        if self.entradas.iter().any(|e| e.pad) {
            let alto = if self.bloqueo.en_funcion() { 168.0 } else { 124.0 };
            egui::Panel::bottom("cart").exact_size(alto).show(ui, |ui| {
                self.franja_de_pads(ui);
            });
        }

        egui::CentralPanel::default().show(ui, |ui| self.lista(ui));

        if self.ver_diagnostico {
            self.ventana_diagnostico(ui);
        }
    }
}

impl App {
    fn barra_superior(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(
                egui::RichText::new(format!("TeatroPlayer {}", env!("CARGO_PKG_VERSION")))
                    .color(FG_STRONG)
                    .strong(),
            );
            ui.add_space(18.0);

            let mut modo = self.bloqueo.modo();
            ui.selectable_value(&mut modo, ModoShow::Diseno, "Diseño");
            ui.selectable_value(&mut modo, ModoShow::Funcion, "Función");
            if modo != self.bloqueo.modo() {
                match modo {
                    ModoShow::Diseno => self.bloqueo.volver_a_diseno(),
                    ModoShow::Funcion => {
                        self.bloqueo.entrar_en_funcion();
                        self.anotar("modo Función: doble Esc para volver a Diseño");
                    }
                }
            }
            ui.add_space(18.0);

            if boton_icono(ui, IconKind::Nueva, "Nueva", None).clicked() {
                self.obra_nueva();
            }
            if boton_icono(ui, IconKind::Abrir, "Abrir…", None).clicked() {
                self.abrir();
            }
            if boton_icono(ui, IconKind::Guardar, "Guardar", None).clicked() {
                self.guardar();
            }
            if boton_icono(ui, IconKind::Guardar, "Guardar como…", None).clicked() {
                self.guardar_como();
            }
            ui.separator();
            if boton_icono(ui, IconKind::AnadirAudio, "Añadir audio…", None).clicked() {
                self.anadir_audio();
            }
            if boton_icono(ui, IconKind::Abrir, "Abrir carpeta…", None).clicked() {
                self.abrir_carpeta();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if boton_icono(
                    ui,
                    IconKind::Registro,
                    "Registro",
                    self.ver_registro.then_some(BG_ROW_ACTIVE),
                )
                .clicked()
                {
                    self.ver_registro = !self.ver_registro;
                }
                ui.add_space(10.0);
                if self.sucio {
                    ui.label(egui::RichText::new("sin guardar").color(WARN_AMBER));
                    ui.add_space(8.0);
                }
                if self.solo_lectura {
                    ui.label(egui::RichText::new("sólo lectura").color(STOP_RED));
                    ui.add_space(8.0);
                }
                ui.label(
                    egui::RichText::new(format!("{} entradas", self.entradas.len()))
                        .color(FG_MUTE),
                );
            });
        });
    }

    fn transporte(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            let parar = egui::Button::new(
                egui::RichText::new("PARAR TODO").color(FG_STRONG).strong(),
            )
            .fill(STOP_RED)
            .min_size(egui::vec2(150.0, 38.0));
            if ui.add(parar).clicked() {
                self.parar_todo();
            }

            ui.add_space(16.0);
            ui.label("Salida:");
            egui::ComboBox::from_id_salt("salida")
                .width(300.0)
                .selected_text(
                    self.salidas
                        .get(self.elegida)
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| "—".to_string()),
                )
                .show_ui(ui, |ui| {
                    for (i, s) in self.salidas.clone().iter().enumerate() {
                        ui.selectable_value(&mut self.elegida, i, &s.name);
                    }
                });
            if boton_icono(ui, IconKind::Probar, "Probar", None).clicked() {
                self.probar_salida();
            }

            // Estado de lo que está sonando ahora mismo.
            let sonando: Vec<String> = self
                .entradas
                .iter()
                .filter(|e| e.sonando())
                .map(|e| {
                    let pos = e
                        .pista
                        .as_ref()
                        .map(|p| p.position().as_secs_f32())
                        .unwrap_or(0.0);
                    format!("{} {:.1}s", e.nombre, pos)
                })
                .collect();

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if sonando.is_empty() {
                    ui.label(egui::RichText::new("en silencio").color(FG_MUTE));
                } else {
                    ui.label(
                        egui::RichText::new(format!("SONANDO: {}", sonando.join(" · ")))
                            .color(GO_GREEN),
                    );
                }
            });
        });

        if self.ver_registro {
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(90.0)
                .show(ui, |ui| {
                    for linea in self.registro.iter().rev().take(20) {
                        ui.monospace(egui::RichText::new(linea).color(FG_MUTE).size(11.0));
                    }
                });
        }
    }

    // --- lista de entradas -------------------------------------------------

    fn lista(&mut self, ui: &mut egui::Ui) {
        if self.entradas.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(
                    egui::RichText::new("Todavía no hay audios")
                        .color(FG_STRONG)
                        .size(20.0),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "Pulsa «Abrir carpeta…» y elige la carpeta con los audios de la obra.",
                    )
                    .color(FG_MUTE),
                );
            });
            return;
        }

        let mut ir_a: Option<usize> = None;
        let mut seleccionar: Option<usize> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in 0..self.entradas.len() {
                let alto = if self.bloqueo.en_funcion() { 72.0 } else { 40.0 };
                let e = &self.entradas[i];
                let nombre = e.nombre.clone();
                let archivo = e.fuente.resumen();
                let falta = e.falta;
                let color = CUE_COLORS[e.color];
                let sonando = e.sonando();
                let estado = e.estado();
                let saliendo = matches!(estado, Some(TrackState::FadingOut));
                let duck = e.duck;
                let posicion = e.pista.as_ref().map(|p| p.position().as_secs_f32()).unwrap_or(0.0);
                let seleccionada = self.seleccionada == Some(i);
                let fondo = if falta {
                    // Las filas con el audio perdido se ven rojas antes de la
                    // función, no durante: el operador tiene que saberlo antes.
                    egui::Color32::from_rgb(0x3A, 0x22, 0x26)
                } else if sonando || seleccionada {
                    BG_ROW_ACTIVE
                } else if i % 2 == 0 {
                    BG_ROW
                } else {
                    BG_ROW_ALT
                };
                let resumen = resumir(&e.spec);
                let tecla = e.tecla.clone();
                let restante = e.pista.as_ref().and_then(|p| match (p.duration(), Some(p.position())) {
                    (Some(total), Some(pos)) if total > pos => Some(total - pos),
                    _ => None,
                });
                let aviso_mp3 = show::aviso_loop_mp3(
                    &e.audio.file_name,
                    matches!(e.spec.loop_mode, LoopMode::Infinite),
                );

                let fila = egui::Frame::new()
                    .fill(fondo)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_min_height(alto);
                        ui.horizontal(|ui| {
                            // Chip de color + número
                            ui.label(egui::RichText::new("■").color(color).size(18.0));
                            ui.label(
                                egui::RichText::new(format!("{}", i + 1))
                                    .color(FG_MUTE)
                                    .monospace(),
                            );

                            // En Función la tecla va delante: es lo que el operador
                            // tiene que recordar (T-UI-007).
                            if let Some(t) = &tecla {
                                ui.label(
                                    egui::RichText::new(t).color(FG_MUTE).monospace().size(11.0),
                                );
                            }

                            let titulo = egui::RichText::new(&nombre)
                                .color(if falta {
                                    STOP_RED
                                } else if sonando {
                                    GO_GREEN
                                } else {
                                    FG_STRONG
                                })
                                .size(if self.bloqueo.en_funcion() { 18.0 } else { 14.0 });
                            if ui.selectable_label(seleccionada, titulo).clicked() {
                                seleccionar = Some(i);
                            }

                            if self.bloqueo.permite_editar() {
                                ui.label(egui::RichText::new(&archivo).color(FG_MUTE).size(11.0));
                                ui.label(egui::RichText::new(&resumen).color(FG_BASE).size(11.0));
                                // T-SHOW-005: el aviso se ve al disenar, no en funcion.
                                if let Some(aviso) = aviso_mp3 {
                                    ui.label(
                                        egui::RichText::new("MP3")
                                            .color(WARN_AMBER)
                                            .size(10.0)
                                            .strong(),
                                    )
                                    .on_hover_text(aviso);
                                }
                            }

                            // Estado + botones, a la derecha
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if falta {
                                        // Sin audio no hay GO: mejor un botón
                                        // que dice por qué no funciona.
                                        ui.add_enabled(
                                            false,
                                            egui::Button::new("FALTA").min_size(egui::vec2(64.0, 28.0)),
                                        );
                                    } else {
                                        let go = egui::Button::new(
                                            egui::RichText::new("GO")
                                                .color(egui::Color32::BLACK)
                                                .strong(),
                                        )
                                        .fill(GO_GREEN)
                                        .min_size(egui::vec2(
                                            64.0,
                                            if self.bloqueo.en_funcion() { 46.0 } else { 28.0 },
                                        ));
                                        if ui.add(go).clicked() {
                                            ir_a = Some(i);
                                        }
                                    }

                                    if duck {
                                        ui.label(
                                            egui::RichText::new("DUCK")
                                                .color(DUCK_ORANGE)
                                                .size(11.0),
                                        );
                                    }
                                    if saliendo {
                                        ui.label(
                                            egui::RichText::new("saliendo")
                                                .color(WARN_AMBER)
                                                .size(11.0),
                                        );
                                    }
                                    if sonando {
                                        // T-UI-010: en Función se ve lo que QUEDA,
                                        // no lo que ha pasado. Es lo que necesita
                                        // quien está esperando para entrar.
                                        if self.bloqueo.en_funcion() {
                                            if let Some(r) = restante {
                                                ui.label(
                                                    egui::RichText::new(formato_mmss(r))
                                                        .color(GO_GREEN)
                                                        .monospace()
                                                        .size(15.0),
                                                );
                                            } else {
                                                ui.label(
                                                    egui::RichText::new("∞")
                                                        .color(GO_GREEN)
                                                        .monospace()
                                                        .size(15.0),
                                                );
                                            }
                                        } else {
                                            ui.label(
                                                egui::RichText::new(format!("{posicion:.1} s"))
                                                    .color(GO_GREEN)
                                                    .monospace(),
                                            );
                                        }
                                    } else if let Some(TrackState::Failed(m)) = &estado {
                                        ui.label(egui::RichText::new(m).color(STOP_RED));
                                    }

                                    if self.bloqueo.permite_editar() {
                                        if boton_icono_chico(ui, IconKind::Subir, None).clicked() {
                                            self.mover(i, -1);
                                        }
                                        if boton_icono_chico(ui, IconKind::Bajar, None).clicked() {
                                            self.mover(i, 1);
                                        }
                                        if boton_icono_chico(ui, IconKind::Quitar, Some(STOP_RED)).clicked() {
                                            self.quitar(i);
                                        }
                                    }
                                },
                            );
                        });
                    });

                // Se vuelve a pintar mientras suena para mover el contador.
                let _ = fila;
            }
        });

        if let Some(i) = seleccionar {
            self.seleccionada = Some(i);
        }
        if let Some(i) = ir_a {
            self.ir(i);
        }
    }

    // --- inspector ---------------------------------------------------------

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let Some(i) = self.seleccionada else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    egui::RichText::new("Selecciona una entrada\npara editarla")
                        .color(FG_MUTE),
                );
            });
            return;
        };
        if self.entradas.get(i).is_none() {
            self.seleccionada = None;
            return;
        }

        let nombre = self.entradas[i].nombre.clone();
        ui.label(egui::RichText::new(&nombre).color(FG_STRONG).size(17.0).strong());

        // Pad y tecla no son del sonido sino de la entrada: van aquí arriba,
        // siempre visibles, no dentro de una pestaña (T-UI-007, T-UI-009).
        {
            let mut pad = self.entradas[i].pad;
            if ui.checkbox(&mut pad, "Aparece en la franja de pads").changed() {
                self.historial.registrar(&self.instantanea());
                self.entradas[i].pad = pad;
                self.sucio = true;
                self.auto.pedir();
            }
        }

        {
            let mut tecla = self.entradas[i].tecla.clone();
            let antes = tecla.clone();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Tecla:").color(FG_MUTE));
                egui::ComboBox::from_id_salt("tecla")
                    .width(120.0)
                    .selected_text(tecla.clone().unwrap_or_else(|| "ninguna".to_string()))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut tecla, None, "ninguna");
                        for n in 1..=12 {
                            let t = format!("F{n}");
                            ui.selectable_value(&mut tecla, Some(t.clone()), t);
                        }
                    });
            });
            if tecla != antes {
                self.entradas[i].tecla = tecla;
                self.sucio = true;
                self.auto.pedir();
            }
        }
        ui.label(
            egui::RichText::new(self.entradas[i].fuente.resumen())
                .color(FG_MUTE)
                .size(11.0),
        );
        ui.separator();

        ui.horizontal(|ui| {
            for t in Tab::TODOS {
                ui.selectable_value(&mut self.tab, t, t.rotulo());
            }
        });
        ui.separator();

        // Se saca una copia del spec, se edita y se vuelve a guardar: evita
        // pelearse con el borrow checker dentro de los closures de egui.
        let mut spec = self.entradas[i].spec.clone();
        let mut auto = self.entradas[i].auto_follow;
        let antes = spec.clone();
        let auto_antes = auto;
        match self.tab {
            Tab::Entrada => ui_entrada(ui, &mut spec),
            Tab::Anterior => ui_anterior(ui, &mut spec),
            Tab::Salida => ui_salida(ui, &mut spec, &mut auto),
            Tab::Repeticion => ui_repeticion(ui, &mut spec),
            Tab::Volumen => ui_volumen(ui, &mut spec),
        }
        if spec != antes || auto != auto_antes {
            self.entradas[i].spec = spec;
            self.entradas[i].auto_follow = auto;
            self.sucio = true;
            self.auto.pedir();
        }
    }

    /// Franja de pads (T-UI-009): efectos sueltos, disparables a mano.
    ///
    /// Un pad **no** apaga lo que está sonando por sí mismo: lo que pase con lo
    /// anterior lo decide el `onPrevious` de su entrada, igual que en la lista.
    fn franja_de_pads(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Pads").color(FG_MUTE).size(11.0));
        ui.add_space(2.0);

        let lado = if self.bloqueo.en_funcion() { 140.0 } else { 96.0 };
        let indices: Vec<usize> = self
            .entradas
            .iter()
            .enumerate()
            .filter(|(_, e)| e.pad)
            .map(|(i, _)| i)
            .collect();

        let mut disparar: Option<usize> = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for i in indices {
                    let e = &self.entradas[i];
                    let color = CUE_COLORS[e.color];
                    let sonando = e.sonando();
                    let relleno = if sonando {
                        color
                    } else {
                        egui::Color32::from_rgb(0x2C, 0x31, 0x3B)
                    };
                    let texto = format!("{}\n{}", i + 1, e.nombre.clone());
                    let boton = egui::Button::new(
                        egui::RichText::new(texto)
                            .color(if sonando { egui::Color32::BLACK } else { FG_STRONG })
                            .size(if self.bloqueo.en_funcion() { 16.0 } else { 13.0 }),
                    )
                    .fill(relleno)
                    .min_size(egui::vec2(lado, lado));

                    if ui.add(boton).clicked() && !e.falta {
                        disparar = Some(i);
                    }
                    ui.add_space(6.0);
                }
            });
        });

        if let Some(i) = disparar {
            self.ir(i);
        }
    }

    /// Contador grande de la pista activa (T-UI-010).
    fn contador_grande(&mut self, ui: &mut egui::Ui) {
        let activa = self
            .entradas
            .iter()
            .filter(|e| e.sonando())
            .map(|e| {
                let restante =
                    e.pista.as_ref().and_then(|p| match (p.duration(), Some(p.position())) {
                        (Some(total), Some(pos)) if total > pos => Some(total - pos),
                        _ => None,
                    });
                (e.nombre.clone(), restante)
            })
            .next();

        let Some((nombre, restante)) = activa else { return };

        ui.horizontal(|ui| {
            let texto = match restante {
                Some(r) => formato_mmss(r),
                // Con loop no se miente diciendo "queda 0:00".
                None => INFINITO.to_string(),
            };
            ui.label(egui::RichText::new(texto).color(GO_GREEN).size(44.0).strong());
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("queda").color(FG_MUTE).size(13.0));
                ui.label(egui::RichText::new(&nombre).color(FG_STRONG).size(16.0));
                let siguiente = self
                    .ultima_disparada
                    .and_then(|u| self.entradas.get(u + 1))
                    .map(|e| e.nombre.clone());
                if let Some(s) = siguiente {
                    ui.label(
                        egui::RichText::new(format!("después: {s}"))
                            .color(FG_MUTE)
                            .size(12.0),
                    );
                }
            });
        });
    }

    /// Ventana de diagnóstico (T-OPS-002): estado de las pistas + log.
    ///
    /// Se abre con Ctrl+Shift+D. Es para cuando algo va mal en un teatro: lo
    /// único que se puede mirar después es esto.
    fn ventana_diagnostico(&mut self, ui: &mut egui::Ui) {
        let mut abierta = self.ver_diagnostico;
        egui::Window::new("Diagnóstico")
            .open(&mut abierta)
            .default_size([720.0, 440.0])
            .show(ui.ctx(), |ui| {
                ui.label(egui::RichText::new("Pistas activas").color(FG_STRONG).size(15.0));
                if self.backend.active_tracks() == 0 {
                    ui.label(egui::RichText::new("ninguna").color(FG_MUTE));
                } else {
                    for e in &self.entradas {
                        let Some(p) = e.pista.as_ref() else { continue };
                        if p.state().is_done() {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&e.nombre).color(FG_BASE));
                            ui.label(
                                egui::RichText::new(format!("{:?}", p.state()))
                                    .color(GO_GREEN)
                                    .monospace()
                                    .size(11.0),
                            );
                            ui.label(
                                egui::RichText::new(format!("{:.2} s", p.position().as_secs_f32()))
                                    .monospace()
                                    .size(11.0),
                            );
                        });
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Log (últimas 50 líneas)")
                            .color(FG_STRONG)
                            .size(15.0),
                    );
                    if ui.small_button("Actualizar").clicked() {
                        self.log_leido = teatroplayer::diagnostico::ultimas_lineas(50);
                    }
                });
                ui.label(
                    egui::RichText::new(
                        teatroplayer::diagnostico::carpeta_logs().display().to_string(),
                    )
                    .color(FG_MUTE)
                    .size(11.0),
                );

                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for linea in &self.log_leido {
                            ui.monospace(egui::RichText::new(linea).size(11.0).color(FG_BASE));
                        }
                    });
            });
        self.ver_diagnostico = abierta;
    }
}

// ---------------------------------------------------------------------------
// Controles del inspector
// ---------------------------------------------------------------------------

/// Slider de duración en escala logarítmica: 0.1 s a 60 s.
///
/// Lineal no sirve: entre 0 y 60 s, el tramo que más se usa (0,1–5 s) ocuparía
/// un 8 % del recorrido y sería imposible de ajustar fino.
fn slider_duracion(ui: &mut egui::Ui, valor: &mut Duration, etiqueta: &str) {
    let mut ms = (valor.as_millis() as f32).clamp(100.0, 60_000.0);
    let mut log_ms = ms.ln();
    let r = ui.add(
        egui::Slider::new(&mut log_ms, 100.0f32.ln()..=60_000.0f32.ln())
            .text(etiqueta)
            .show_value(false),
    );
    if r.changed() {
        ms = log_ms.exp().clamp(100.0, 60_000.0);
        *valor = Duration::from_millis(ms.round() as u64);
    }
    ui.label(egui::RichText::new(formatear(*valor)).color(FG_STRONG).monospace());
}

/// Cuenta atrás legible: `1:05`. En teatro se mira de reojo, así que minutos y
/// segundos separados y sin decimales (T-UI-010).
/// Símbolo para cuando la pista no termina nunca (loop).
const INFINITO: &str = "\u{221e}";

fn formato_mmss(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn formatear(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.2} s", d.as_secs_f32())
    }
}

fn combo_curva(ui: &mut egui::Ui, curva: &mut Curve) {
    egui::ComboBox::from_id_salt("curva")
        .width(150.0)
        .selected_text(rotulo_curva(*curva))
        .show_ui(ui, |ui| {
            for c in [Curve::EqualPower, Curve::Linear, Curve::Exponential] {
                ui.selectable_value(curva, c, rotulo_curva(c));
            }
        });
}

fn rotulo_curva(c: Curve) -> &'static str {
    match c {
        Curve::EqualPower => "equal-power (recomendada)",
        Curve::Linear => "lineal",
        Curve::Exponential => "exponencial",
    }
}

fn duracion_de(spec: &CueSpec, por_defecto: Duration) -> Duration {
    match spec.entrance {
        Entrance::FadeIn { duration, .. } => duration,
        Entrance::Hit => por_defecto,
    }
}

fn curva_de_entrada(spec: &CueSpec) -> Curve {
    match spec.entrance {
        Entrance::FadeIn { curve, .. } => curve,
        Entrance::Hit => Curve::EqualPower,
    }
}

fn ui_entrada(ui: &mut egui::Ui, spec: &mut CueSpec) {
    ui.label(egui::RichText::new("Cómo entra este audio").color(FG_STRONG).size(15.0));
    ui.add_space(6.0);

    let mut con_fade = matches!(spec.entrance, Entrance::FadeIn { .. });
    ui.radio_value(&mut con_fade, false, "De golpe");
    ui.radio_value(&mut con_fade, true, "Con fade in");
    ui.add_space(8.0);

    if con_fade {
        let mut duracion = duracion_de(spec, Duration::from_secs(3));
        let mut curva = curva_de_entrada(spec);
        ui.horizontal(|ui| {
            slider_duracion(ui, &mut duracion, "Duración");
        });
        ui.horizontal(|ui| {
            ui.label("Curva:");
            combo_curva(ui, &mut curva);
        });
        spec.entrance = Entrance::FadeIn { duration: duracion, curve: curva };
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "La curva equal-power mantiene la energía constante durante el cruce: \
                 con una lineal se oye un bache de volumen en el centro.",
            )
            .color(FG_MUTE)
            .size(11.0),
        );
    } else {
        spec.entrance = Entrance::Hit;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TipoAnterior {
    Keep,
    FadeOut,
    Stop,
    Duck,
}

fn ui_anterior(ui: &mut egui::Ui, spec: &mut CueSpec) {
    ui.label(
        egui::RichText::new("Qué pasa con lo que esté sonando")
            .color(FG_STRONG)
            .size(15.0),
    );
    ui.add_space(6.0);

    let (mut tipo, mut duracion, mut curva, mut nivel) = match spec.on_previous {
        OnPrevious::Keep => (TipoAnterior::Keep, Duration::from_secs(3), Curve::EqualPower, 30u8),
        OnPrevious::FadeOut { duration, curve } => (TipoAnterior::FadeOut, duration, curve, 30),
        OnPrevious::Stop => (TipoAnterior::Stop, Duration::from_secs(3), Curve::EqualPower, 30),
        OnPrevious::Duck { duration, curve, level_percent } => {
            (TipoAnterior::Duck, duration, curve, level_percent)
        }
    };

    ui.radio_value(&mut tipo, TipoAnterior::Keep, "Se queda sonando (se encima)");
    ui.radio_value(&mut tipo, TipoAnterior::FadeOut, "Sale con fade out");
    ui.radio_value(&mut tipo, TipoAnterior::Stop, "Corta de golpe");
    ui.radio_value(&mut tipo, TipoAnterior::Duck, "Baja de volumen (duck)");
    ui.add_space(8.0);

    if tipo == TipoAnterior::FadeOut || tipo == TipoAnterior::Duck {
        ui.horizontal(|ui| {
            slider_duracion(ui, &mut duracion, "Duración");
        });
        ui.horizontal(|ui| {
            ui.label("Curva:");
            combo_curva(ui, &mut curva);
        });
    }
    if tipo == TipoAnterior::Duck {
        let mut pct = nivel as f32;
        ui.add(egui::Slider::new(&mut pct, 0.0..=90.0).text("Se queda al %").suffix(" %"));
        nivel = pct.round() as u8;
        ui.label(
            egui::RichText::new("Útil para una voz sobre un ambiente: el ambiente baja y vuelve.")
                .color(FG_MUTE)
                .size(11.0),
        );
    }

    spec.on_previous = match tipo {
        TipoAnterior::Keep => OnPrevious::Keep,
        TipoAnterior::FadeOut => OnPrevious::FadeOut { duration: duracion, curve: curva },
        TipoAnterior::Stop => OnPrevious::Stop,
        TipoAnterior::Duck => {
            OnPrevious::Duck { duration: duracion, curve: curva, level_percent: nivel }
        }
    };
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TipoSalida {
    UntilEnd,
    FadeOut,
    Hit,
}

fn ui_salida(ui: &mut egui::Ui, spec: &mut CueSpec, auto: &mut AutoFollow) {
    ui.label(
        egui::RichText::new("Cómo sale cuando aprietas SALIR")
            .color(FG_STRONG)
            .size(15.0),
    );
    ui.add_space(6.0);

    let (mut tipo, mut duracion, mut curva) = match spec.exit {
        ExitMode::UntilEnd => (TipoSalida::UntilEnd, Duration::from_secs(2), Curve::EqualPower),
        ExitMode::FadeOut { duration, curve } => (TipoSalida::FadeOut, duration, curve),
        ExitMode::Hit => (TipoSalida::Hit, Duration::from_secs(2), Curve::EqualPower),
    };

    ui.radio_value(&mut tipo, TipoSalida::UntilEnd, "Se queda hasta el final");
    ui.radio_value(&mut tipo, TipoSalida::FadeOut, "Sale con fade out");
    ui.radio_value(&mut tipo, TipoSalida::Hit, "Corta de golpe");
    ui.add_space(8.0);

    if tipo == TipoSalida::FadeOut {
        ui.horizontal(|ui| {
            slider_duracion(ui, &mut duracion, "Duración");
        });
        ui.horizontal(|ui| {
            ui.label("Curva:");
            combo_curva(ui, &mut curva);
        });
    }

    spec.exit = match tipo {
        TipoSalida::UntilEnd => ExitMode::UntilEnd,
        TipoSalida::FadeOut => ExitMode::FadeOut { duration: duracion, curve: curva },
        TipoSalida::Hit => ExitMode::Hit,
    };

    // --- Auto-follow (T-SHOW-002) -----------------------------------------
    ui.add_space(10.0);
    ui.separator();
    ui.label(
        egui::RichText::new("Al terminar, la siguiente entrada")
            .color(FG_STRONG)
            .size(14.0),
    );
    ui.add_space(4.0);

    let mut clase = match *auto {
        AutoFollow::None => 0,
        AutoFollow::AfterMs(_) => 1,
        AutoFollow::WhenThisEnds => 2,
    };
    ui.radio_value(&mut clase, 0, "No hace nada (la disparo yo)");
    ui.radio_value(&mut clase, 1, "Arranca pasados unos segundos");
    let con_loop = matches!(spec.loop_mode, LoopMode::Infinite);
    if con_loop {
        ui.add_enabled(
            false,
            egui::RadioButton::new(false, "Cuando termine esta (no vale con loop)"),
        );
    } else {
        ui.radio_value(&mut clase, 2, "Arranca cuando termine esta");
    }

    if clase == 1 {
        let mut dur = match *auto {
            AutoFollow::AfterMs(m) => Duration::from_millis(m),
            _ => Duration::from_millis(1000),
        };
        ui.horizontal(|ui| {
            slider_duracion(ui, &mut dur, "Espera");
        });
        *auto = AutoFollow::AfterMs(dur.as_millis().max(100) as u64);
    } else if clase == 2 {
        *auto = AutoFollow::WhenThisEnds;
    } else {
        *auto = AutoFollow::None;
    }

    if auto.es_imposible(con_loop) {
        ui.label(
            egui::RichText::new("Con loop infinito esto nunca se dispararía.")
                .color(WARN_AMBER)
                .size(11.0),
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TipoLoop {
    None,
    Infinite,
    Count,
}

fn ui_repeticion(ui: &mut egui::Ui, spec: &mut CueSpec) {
    ui.label(egui::RichText::new("Repetición").color(FG_STRONG).size(15.0));
    ui.add_space(6.0);

    let (mut tipo, mut cuenta) = match spec.loop_mode {
        LoopMode::None => (TipoLoop::None, 2u32),
        LoopMode::Infinite => (TipoLoop::Infinite, 2),
        LoopMode::Count(n) => (TipoLoop::Count, n.max(2)),
    };

    ui.radio_value(&mut tipo, TipoLoop::None, "No repetir");
    ui.radio_value(&mut tipo, TipoLoop::Infinite, "Loop infinito (ambiente)");
    ui.radio_value(&mut tipo, TipoLoop::Count, "Repetir N veces");
    ui.add_space(8.0);

    if tipo == TipoLoop::Count {
        let mut n = cuenta as f32;
        ui.add(egui::Slider::new(&mut n, 2.0..=20.0).text("Veces").suffix(" x"));
        cuenta = n.round() as u32;
    }
    if tipo == TipoLoop::Infinite {
        ui.label(
            egui::RichText::new(
                "Con loop infinito el audio se lee del disco en cada vuelta, no se carga en memoria."
            )
            .color(FG_MUTE)
            .size(11.0),
        );
    }

    spec.loop_mode = match tipo {
        TipoLoop::None => LoopMode::None,
        TipoLoop::Infinite => LoopMode::Infinite,
        TipoLoop::Count => LoopMode::Count(cuenta),
    };
}

fn ui_volumen(ui: &mut egui::Ui, spec: &mut CueSpec) {
    ui.label(egui::RichText::new("Volumen de esta pista").color(FG_STRONG).size(15.0));
    ui.add_space(6.0);

    let mut db = spec.volume.as_db();
    ui.add(
        egui::Slider::new(&mut db, -60.0..=6.0)
            .text("Volumen")
            .suffix(" dB")
            .custom_formatter(|v, _| format!("{:+.1} dB", v)),
    );
    spec.volume = MilliDb::from_db(db);

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!(
            "Ganancia lineal: {:.3}  ({} milidecibelios)",
            spec.volume.as_linear(),
            spec.volume.0
        ))
        .color(FG_MUTE)
        .size(11.0),
    );
    ui.label(
        egui::RichText::new(
            "El volumen se aplica antes del fade, así que no cambia la forma de la curva.",
        )
        .color(FG_MUTE)
        .size(11.0),
    );
}

/// Resumen corto de la configuración, para la columna de la lista.
fn resumir(spec: &CueSpec) -> String {
    let mut partes: Vec<String> = Vec::new();

    partes.push(match spec.entrance {
        Entrance::Hit => "entra de golpe".to_string(),
        Entrance::FadeIn { duration, .. } => format!("entra en {}", formatear(duration)),
    });

    match spec.on_previous {
        OnPrevious::Keep => {}
        OnPrevious::FadeOut { duration, .. } => {
            partes.push(format!("anterior sale en {}", formatear(duration)))
        }
        OnPrevious::Stop => partes.push("anterior corta".to_string()),
        OnPrevious::Duck { level_percent, .. } => {
            partes.push(format!("anterior baja a {level_percent}%"))
        }
    }

    match spec.loop_mode {
        LoopMode::None => {}
        LoopMode::Infinite => partes.push("loop ∞".to_string()),
        LoopMode::Count(n) => partes.push(format!("loop ×{n}")),
    }

    if spec.volume != MilliDb::ZERO {
        partes.push(format!("{:+.0} dB", spec.volume.as_db()));
    }

    partes.join(" · ")
}
