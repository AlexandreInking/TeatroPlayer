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
use teatroplayer::eventos::{Evento, PistaEvento, TipoEvento, BUCLE_INFINITO};
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

/// Paleta de 6 colores para los eventos, distinta de la de los audios: así se
/// distingue de un vistazo si lo que se mira es un archivo o una escena.
const EVENT_COLORS: [egui::Color32; 6] = [
    egui::Color32::from_rgb(0x5D, 0xBE, 0x6F),
    egui::Color32::from_rgb(0x38, 0xB5, 0xA6),
    egui::Color32::from_rgb(0x4F, 0x8F, 0xE0),
    egui::Color32::from_rgb(0x7B, 0x6F, 0xE0),
    egui::Color32::from_rgb(0xC5, 0x6F, 0xD0),
    egui::Color32::from_rgb(0xE8, 0x8C, 0x39),
];

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

// --- Medidas de la zona de reproductor --------------------------------------
//
// El panel de abajo crece con lo que hay sonando: es una lista de pistas, y
// una lista no puede tener un alto fijo sin dejar fuera justo la pista que al
// operador le interesa. De ahí que el alto se calcule y no se declare.
//
// Los números van **holgados a propósito**. Un panel `exact_size` al que le
// falta un píxel no avisa: recorta. Al que le sobra, sólo le queda un poco de
// aire abajo. Entre los dos fallos, el que no se nota es el segundo.

/// Alto de un botón de mandos. Es lo que hay que poder alcanzar de un
/// manotazo: no se toca.
const ALTO_BOTON_MANDOS: f32 = 38.0;
/// Ancho de un botón de mandos. Con icono en vez de texto el ancho ya no lo
/// dicta la palabra —"PARAR TODO" pedía 150— así que va ajustado.
const ANCHO_BOTON_MANDOS: f32 = 44.0;
/// Alto de la fila de mandos **y** de lo que cuelga de ella dentro del panel:
/// el separador y el aviso de "nada sonando".
///
/// Va **holgado a propósito**, y no es cuestión de estética: un panel
/// `exact_size` al que le falta un píxel no avisa. Recorta, y encima engaña
/// —cuando el contenido no cabe, egui reancla el panel a su borde de abajo, le
/// deja el hueco de más al panel central, y el panel central acaba pintándose
/// **encima** de la mitad de arriba de los botones—. Al que le sobra, sólo le
/// queda un poco de aire abajo.
const ALTO_MANDOS: f32 = 92.0;
/// Alto de cada fila de la lista: la fila (24) con su margen, más el hueco que
/// egui deja entre dos widgets seguidos.
const ALTO_FILA_ACTIVO: f32 = 36.0;
/// Alto que se lleva el registro cuando está abierto: su separador y el tope
/// de scroll de las líneas (90), con aire.
const ALTO_REGISTRO: f32 = 108.0;
/// Tope del panel. Por encima de esto la lista hace scroll: si no, una función
/// con ocho pistas sonando dejaría la lista de audios en un par de dedos.
const ALTO_TRANSPORTE_MAX: f32 = 320.0;

// ---------------------------------------------------------------------------
// Las dos listas del panel central
// ---------------------------------------------------------------------------
//
// En una van los **audios**: los archivos sueltos de la obra. En la otra, los
// **eventos**: escenas montadas con uno o dos de esos audios y una rampa de
// volumen entre dos porcentajes. Son dos cosas distintas y por eso van en dos
// listas separadas y no mezcladas: el audio es el material, el evento es lo
// que se lanza en la función.
//
// El panel de la derecha deja de tener pestañas: era lo que obligaba a poner
// iconos crípticos en un panel estrecho. Ahora es una columna con scroll donde
// todo se ve a la vez, que es lo que hace falta para ajustar una escena.

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum ListaTab {
    #[default]
    Audios,
    Eventos,
}

impl ListaTab {
    fn rotulo(self) -> &'static str {
        match self {
            ListaTab::Audios => "Audios",
            ListaTab::Eventos => "Eventos",
        }
    }

    fn icono(self) -> IconKind {
        match self {
            ListaTab::Audios => IconKind::AnadirAudio,
            ListaTab::Eventos => IconKind::Cruce,
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

    /// Congelada donde estaba. **Sigue contando como sonando**: la pista no se
    /// ha ido, está parada a media muestra.
    fn pausada(&self) -> bool {
        self.pista.as_ref().is_some_and(|p| p.is_paused())
    }

    /// Ganancia que el motor está aplicando ahora mismo (0.0–1.0), sin contar
    /// el volumen. Es lo que deja **ver** un fade en curso en la zona de
    /// reproductor: si sube poco a poco, la rampa va; si salta a 1, no hay.
    fn ganancia(&self) -> f32 {
        self.pista.as_ref().map(|p| p.gain()).unwrap_or(0.0)
    }

    /// true si suena en bucle sin fin.
    ///
    /// Es la condición que decide qué vuelve al pulsar PLAY después de un
    /// STOP: un bucle estaba pensado para durar toda la escena, un efecto de
    /// una sola pasada ya se oyó y repetirlo sería un susto.
    fn en_bucle(&self) -> bool {
        matches!(self.spec.loop_mode, LoopMode::Infinite)
    }

    /// Cómo se llama el botón que la saca, según lo que vaya a hacer de verdad.
    ///
    /// El botón no puede decir siempre lo mismo: con una salida configurada de
    /// "fade out" la pista tarda en irse, y eso el operador tiene que saberlo
    /// **antes** de pulsar, no después.
    fn rotulo_salida(&self) -> &'static str {
        match self.spec.exit {
            ExitMode::FadeOut { .. } => "SALIR",
            ExitMode::Hit => "CORTA",
            ExitMode::UntilEnd => "PARAR",
        }
    }

    /// Qué va a pasar al pulsar ese botón, para el tooltip.
    fn ayuda_salida(&self) -> String {
        match self.spec.exit {
            ExitMode::FadeOut { duration, .. } => {
                format!("Sale con un fade out de {}", formatear(duration))
            }
            ExitMode::Hit => "Corta de golpe".to_string(),
            ExitMode::UntilEnd => {
                "No tiene salida configurada, así que corta ya. Para que salga con fade, \
                 elige «Sale con fade out» en «Cómo sale»."
                    .to_string()
            }
        }
    }
}

/// Un evento en vivo: el evento guardado más las pistas que estén sonando.
///
/// Va aparte de [`Evento`] por el mismo motivo que `Entrada` no es `Cue`: el
/// modelo se serializa, las pistas en curso no.
struct EventoVivo {
    evento: Evento,
    /// Pista en curso de cada hueco de audio.
    ///
    /// `None` en un hueco significa "este hueco no suena por sí mismo": o está
    /// vacío, o ya estaba sonando como entrada y el evento se limitó a bajarla
    /// (el caso del crossfade sobre un ambiente que ya suena).
    pistas: Vec<Option<Box<dyn TrackHandle>>>,
    /// true si algún audio del evento no está en la lista de audios.
    falta: bool,
}

impl EventoVivo {
    fn nuevo(evento: Evento) -> Self {
        let huecos = evento.pistas.len();
        // `vec![None; n]` no sirve: `Box<dyn TrackHandle>` no es `Clone`, y no
        // tiene por qué serlo (una pista en curso no se debe duplicar).
        Self { evento, pistas: (0..huecos).map(|_| None).collect(), falta: false }
    }

    fn sonando(&self) -> bool {
        self.pistas
            .iter()
            .any(|p| p.as_ref().is_some_and(|t| !t.state().is_done()))
    }

    fn parar(&mut self) {
        for p in self.pistas.iter_mut().flatten() {
            p.stop();
        }
        for p in self.pistas.iter_mut() {
            *p = None;
        }
    }

    /// Suelta las pistas que ya terminaron, para no contarlas como sonando.
    fn limpiar_terminadas(&mut self) {
        for p in self.pistas.iter_mut() {
            if p.as_ref().is_some_and(|t| t.state().is_done()) {
                *p = None;
            }
        }
    }

    /// Congelada donde estaba. Igual que en `Entrada`, sigue contando como
    /// sonando.
    fn pausada(&self) -> bool {
        self.pistas.iter().flatten().any(|t| t.is_paused())
    }

    /// La ganancia más alta de sus pistas: si alguna está en rampa, se ve.
    fn ganancia(&self) -> f32 {
        self.pistas.iter().flatten().map(|t| t.gain()).fold(0.0, f32::max)
    }

    /// true si el evento suena en bucle sin fin (es lo que devuelve PLAY).
    fn en_bucle(&self) -> bool {
        self.evento.infinito()
    }
}

/// Foto del estado para deshacer: entradas **y** eventos.
#[derive(Clone, Default)]
struct Instantanea {
    filas: Vec<Fila>,
    eventos: Vec<Evento>,
}

/// A qué se le está editando la tecla y el pad: a un audio o a un evento.
///
/// Los dos tienen tecla y pad, así que el editor es el mismo y sólo cambia
/// dónde se guarda. El `sal` distingue los combo boxes de uno y de otro, que si
/// no compartirían id dentro de egui y se pisarían.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Objeto {
    Audio(usize),
    Evento(usize),
}

impl Objeto {
    fn sal(self) -> &'static str {
        match self {
            Objeto::Audio(_) => "audio",
            Objeto::Evento(_) => "evento",
        }
    }
}

/// Lo que se le puede pedir a una pista desde la zona de reproductor.
///
/// Son los tres gestos de cualquier reproductor, y los tres son distintos de
/// verdad, no tres nombres para lo mismo:
///
/// - `Pausar` congela **donde está**: la posición y la envolvente se quedan
///   intactas, aunque sea a media rampa y a mitad de un bucle.
/// - `Seguir` la descongela en esa misma muestra, con el fade por donde iba.
/// - `Parar` la saca. Es el único destructivo: después ya no hay a dónde
///   volver, y por eso PLAY no puede devolver un efecto de una sola pasada.
///   Sobre un **audio** no corta a lo bruto: obedece a la salida que ese audio
///   tenga configurada en el inspector (ver [`App::aplicar_accion`]), que es lo
///   que hace que la sección "Cómo sale" sirva de algo.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Accion {
    Seguir,
    Pausar,
    Parar,
}

impl Accion {
    fn aplicar(self, p: &dyn TrackHandle) {
        match self {
            Accion::Seguir => p.resume(),
            Accion::Pausar => p.pause(),
            Accion::Parar => p.stop(),
        }
    }

    fn rotulo(self) -> &'static str {
        match self {
            Accion::Seguir => "SEGUIR",
            Accion::Pausar => "PAUSA",
            Accion::Parar => "PARAR",
        }
    }
}

/// Una fila de la zona de reproductor: una pista que está sonando **ahora**.
///
/// Es una foto, no una referencia: se recoge antes de pintar para no tener
/// `self` prestado mientras se dibuja, y se pinta después.
struct Activo {
    /// A qué audio o evento pertenece, para saber a quién mandarle la orden.
    quien: Objeto,
    nombre: String,
    color: egui::Color32,
    pos: Option<Duration>,
    total: Option<Duration>,
    ganancia: f32,
    pausado: bool,
    saliendo: bool,
    bucle: bool,
    /// Cómo se llama el botón que la saca, y qué va a hacer.
    ///
    /// Se copian aquí y no se leen de `Entrada` al pintar porque dentro del
    /// bucle de pintado `self` está prestado.
    rotulo_salida: &'static str,
    ayuda_salida: String,
}

/// Asistente para crear un evento: primero el tipo, luego los audios.
struct Asistente {
    /// 0 = elegir el tipo, 1 = elegir los audios.
    paso: usize,
    tipo: TipoEvento,
    /// Índice del audio elegido para cada hueco, dentro de `self.entradas`.
    elegidos: Vec<Option<usize>>,
}

struct App {
    backend: Arc<RodioBackend>,
    salidas: Vec<OutputInfo>,
    elegida: usize,

    entradas: Vec<Entrada>,
    seleccionada: Option<usize>,
    /// Eventos predefinidos de la obra.
    eventos: Vec<EventoVivo>,
    evento_sel: Option<usize>,
    /// Qué lista se ve en el panel central.
    lista: ListaTab,
    /// Ventana paso a paso para crear un evento. `None` = cerrada.
    asistente: Option<Asistente>,
    /// Modo y bloqueo de edición (T-SHOW-001).
    bloqueo: Bloqueo,
    /// Dispara la entrada siguiente cuando toca (T-SHOW-002).
    programador: Programador,

    /// Lo que sonaba **en bucle** cuando se pulsó PARAR.
    ///
    /// Es la memoria de STOP. PLAY devuelve exactamente esto y nada más: un
    /// efecto de una sola pasada no entra aquí, porque ya se oyó entero y
    /// repetirlo en mitad de la función sería un susto, no una reanudación.
    /// Un audio que estaba sonando pero no en bucle tampoco: lo que se
    /// reanuda son los ambientes, no los remates.
    reanudables: Vec<Objeto>,

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
    historial: Historial<Instantanea>,
    /// Obra que hay que abrir en cuanto arranque la ventana: llega de la línea
    /// de comandos, que es lo que pasa Windows al hacer doble clic en un
    /// `.tpshow` una vez asociada la extensión (T-REL-004).
    pendiente_abrir: Option<PathBuf>,
    /// Cuántos frames lleva pintados.
    frames: u32,
    /// Frame en el que empezó la última pulsación del ratón.
    ///
    /// Sirve para descartar el clic que **ya venía en vuelo** cuando la ventana
    /// se abrió: si la pulsación empezó antes de que esta ventana existiera, el
    /// soltar no cuenta como un clic del operador. Sin esto, abrir el programa
    /// con el ratón pulsado en cualquier sitio cambiaba de lista solo.
    frame_pulsacion: Option<u32>,
}

impl App {
    fn new() -> Self {
        Self {
            backend: RodioBackend::new(),
            salidas: Vec::new(),
            elegida: 0,
            entradas: Vec::new(),
            seleccionada: None,
            eventos: Vec::new(),
            evento_sel: None,
            lista: ListaTab::Audios,
            asistente: None,
            bloqueo: Bloqueo::nuevo(),
            programador: Programador::nuevo(),
            reanudables: Vec::new(),
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
            frames: 0,
            frame_pulsacion: None,
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
    fn instantanea(&self) -> Instantanea {
        Instantanea {
            filas: self
                .entradas
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
                .collect(),
            eventos: self.eventos.iter().map(|v| v.evento.clone()).collect(),
        }
    }

    /// Vuelve a una foto. Las posiciones que siguen existiendo **conservan su
    /// pista en curso**: deshacer no debe cortar lo que está sonando.
    fn aplicar_instantanea(&mut self, instanta: Instantanea) {
        let filas = instanta.filas;
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
        // La memoria de STOP son índices, y deshacer cambia lo que hay en
        // cada índice: se olvida en vez de devolver el audio equivocado.
        self.reanudables.clear();
        if self.seleccionada.is_some_and(|i| i >= self.entradas.len()) {
            self.seleccionada = None;
        }

        // Los eventos, igual: se cambia la configuración pero no se corta lo
        // que esté sonando de ese evento.
        let total_eventos = instanta.eventos.len();
        for (i, evento) in instanta.eventos.into_iter().enumerate() {
            match self.eventos.get_mut(i) {
                Some(vivo) => {
                    // Si cambia el número de huecos (por ejemplo, de fade in a
                    // crossfade), las pistas en curso dejan de cuadrar.
                    if vivo.pistas.len() != evento.pistas.len() {
                        vivo.parar();
                        vivo.pistas = (0..evento.pistas.len()).map(|_| None).collect();
                    }
                    vivo.evento = evento;
                }
                None => self.eventos.push(EventoVivo::nuevo(evento)),
            }
        }
        self.eventos.truncate(total_eventos);
        if self.evento_sel.is_some_and(|i| i >= self.eventos.len()) {
            self.evento_sel = None;
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
        for e in &mut self.entradas {
            if let Some(p) = e.pista.take() {
                p.stop();
            }
        }
        self.entradas.clear();
        self.seleccionada = None;
        self.eventos.clear();
        self.evento_sel = None;
        self.asistente = None;
        self.lista = ListaTab::Audios;
        self.ruta = None;
        self.sucio = false;
        self.solo_lectura = false;
        // La obra anterior se fue: sus bucles ya no existen.
        self.reanudables.clear();
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
        self.eventos.clear();
        // Otra obra: los bucles apuntados eran de la anterior.
        self.reanudables.clear();
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

        // Los eventos se cargan después de los audios: cada evento guarda el
        // `fileName` de sus audios y hay que poder comprobar que siguen ahí.
        self.eventos = sesion.eventos.iter().map(|e| self.cargar_evento(e)).collect();
        self.refrescar_falta_eventos();

        let cuantos = self.entradas.len();
        let faltantes = self.entradas.iter().filter(|e| e.falta).count();
        let eventos_sin_audio = self.eventos.iter().filter(|v| v.falta).count();
        self.seleccionada = if cuantos > 0 { Some(0) } else { None };
        self.evento_sel = if self.eventos.is_empty() { None } else { Some(0) };
        self.solo_lectura = modo == ModoApertura::SoloLectura;
        self.ruta = Some(ruta.clone());
        self.sucio = false;

        self.anotar(format!(
            "abierta {} ({} audios, {} eventos)",
            ruta.display(),
            cuantos,
            self.eventos.len()
        ));
        if faltantes > 0 {
            self.anotar(format!("AVISO: {faltantes} audios no están en el paquete"));
        }
        if eventos_sin_audio > 0 {
            self.anotar(format!(
                "AVISO: {eventos_sin_audio} eventos usan audios que no están en la obra"
            ));
        }
        if self.solo_lectura {
            self.anotar("AVISO: es de una versión más nueva; se abre sólo para mirar");
        }
    }

    /// Pone un evento en la lista, sin pistas en curso todavía.
    fn cargar_evento(&self, evento: &Evento) -> EventoVivo {
        EventoVivo::nuevo(evento.clone())
    }

    /// Marca los eventos cuyo audio ya no está en la lista de audios.
    ///
    /// Un evento no guarda el archivo, sino el `fileName` del audio con el que
    /// se montó: si ese audio deja de estar en la obra (se quitó, se renombró),
    /// el evento se ve como incompleto en vez de fallar al lanzarlo.
    fn refrescar_falta_eventos(&mut self) {
        for vivo in &mut self.eventos {
            vivo.falta = vivo.evento.pistas.iter().any(|p| {
                !p.vacio()
                    && !self
                        .entradas
                        .iter()
                        .any(|e| e.audio.file_name == p.audio.file_name)
            });
        }
    }

    /// ¿Se puede dar por bueno un clic del ratón ahora mismo?
    ///
    /// Es falso mientras la única pulsación vista sea la que **ya venía en
    /// vuelo** al abrirse la ventana: la que abrió el programa desde el acceso
    /// directo o la que manda el sistema al dar el foco. Esa pulsación no es
    /// del operador, y si el ratón estaba justo encima de un botón, el soltar
    /// se interpreta como un clic y acciona lo que hubiera debajo.
    ///
    /// Se usa en los sitios donde un clic fantasma hace daño: cambiar de lista
    /// o tirar la obra con "Nueva". En un botón de GO el daño sería sonar algo
    /// que nadie pidió, así que ahí también vale la pena.
    fn clic_fiable(&self) -> bool {
        self.frame_pulsacion.is_some_and(|f| f > 1)
    }

    /// Abre el asistente de creación de eventos.
    fn abrir_asistente(&mut self) {
        self.asistente = Some(Asistente {
            paso: 0,
            tipo: TipoEvento::FadeIn,
            elegidos: vec![None],
        });
        self.anotar("asistente: elige el tipo de evento");
    }

    /// Quita un evento. No corta lo que esté sonando de los demás.
    fn quitar_evento(&mut self, i: usize) {
        self.historial.registrar(&self.instantanea());
        if let Some(v) = self.eventos.get_mut(i) {
            v.parar();
        }
        self.eventos.remove(i);
        self.reanudables.clear();
        self.sucio = true;
        self.auto.pedir();
        if self.evento_sel == Some(i) {
            self.evento_sel = None;
        } else if let Some(s) = self.evento_sel {
            if s > i {
                self.evento_sel = Some(s - 1);
            }
        }
    }

    fn mover_evento(&mut self, i: usize, delta: isize) {
        self.historial.registrar(&self.instantanea());
        let destino = i as isize + delta;
        if destino < 0 || destino >= self.eventos.len() as isize {
            return;
        }
        self.eventos.swap(i, destino as usize);
        self.reanudables.clear();
        self.sucio = true;
        self.auto.pedir();
        if self.evento_sel == Some(i) {
            self.evento_sel = Some(destino as usize);
        }
    }

    /// Alarga o acorta la escena sin tocar nada más de su configuración.
    ///
    /// Es el ajuste que se hace sobre la marcha: los audios, la curva, el
    /// volumen y el bucle se quedan exactamente como estaban.
    fn ajustar_duracion_evento(&mut self, i: usize, delta_ms: i64) {
        let Some(v) = self.eventos.get(i) else { return };
        let antes = v.evento.duracion_ms;
        let nueva = (antes as i64 + delta_ms).clamp(100, 600_000) as u64;
        if nueva == antes {
            return;
        }
        let nombre = v.evento.nombre.clone();
        // No se registra en el historial: es un ajuste fino, y en Función
        // deshacer por accidente sería peor que no deshacer.
        self.eventos[i].evento.duracion_ms = nueva;
        self.anotar(format!(
            "'{nombre}': {} → {}",
            formatear(Duration::from_millis(antes)),
            formatear(Duration::from_millis(nueva))
        ));
        self.sucio = true;
        self.auto.pedir();
    }

    /// Índice en `entradas` del audio al que apunta un hueco de un evento.
    ///
    /// Se busca por `fileName` y no por posición: el evento guarda el nombre
    /// del archivo, así que sigue apuntando al mismo audio aunque el operador
    /// reordene la lista (`Docs/04` §3).
    fn indice_de_evento(&self, pista: &PistaEvento) -> Option<usize> {
        self.entradas
            .iter()
            .position(|e| !e.audio.file_name.is_empty() && e.audio.file_name == pista.audio.file_name)
    }

    /// El audio de la lista al que apunta un hueco de un evento.
    fn fuente_de_evento(&self, pista: &PistaEvento) -> Option<Fuente> {
        self.indice_de_evento(pista).map(|i| self.entradas[i].fuente.clone())
    }

    /// Dónde está sonando ya ese archivo, si es que está sonando en algún sitio.
    /// Devuelve de quién es y si está congelado.
    ///
    /// La identidad de un audio es **su archivo** (`AudioRef::file_name`): es lo
    /// mismo por lo que un evento encuentra su audio en la lista, y es lo que
    /// sobrevive a reordenar, a renombrar la fila y a reabrir la obra. Por eso
    /// la comprobación cruza las dos listas: un audio puede estar sonando como
    /// fila o dentro de un evento, y en los dos casos es el mismo audio sonando.
    ///
    /// `excepto` deja fuera a quien se está comprobando. Hace falta al relanzar
    /// un evento: su propia copia anterior todavía está montada en ese momento,
    /// y sin excluirla el evento se bloquearía a sí mismo.
    fn donde_suena(&self, file_name: &str, excepto: Option<Objeto>) -> Option<(String, bool)> {
        // Sin nombre de archivo no hay identidad que comparar (una fila sin
        // audio). Comparar vacíos haría que todas las filas rotas se estorbaran
        // entre sí.
        if file_name.is_empty() {
            return None;
        }
        for (i, e) in self.entradas.iter().enumerate() {
            if excepto == Some(Objeto::Audio(i)) {
                continue;
            }
            if e.sonando() && e.audio.file_name == file_name {
                return Some((e.nombre.clone(), e.pausada()));
            }
        }
        for (i, v) in self.eventos.iter().enumerate() {
            if excepto == Some(Objeto::Evento(i)) {
                continue;
            }
            if v.sonando() && v.evento.pistas.iter().any(|p| p.audio.file_name == file_name) {
                return Some((v.evento.nombre.clone(), v.pausada()));
            }
        }
        None
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
            eventos: self.eventos.iter().map(|v| v.evento.clone()).collect(),
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
        // Borrar una fila corre todos los índices de detrás: la memoria de
        // STOP dejaría de apuntar a lo que apuntaba.
        self.reanudables.clear();
        // Un evento que usara este audio se queda sin él: hay que decirlo.
        self.refrescar_falta_eventos();
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
        // Reordenar también mueve los índices.
        self.reanudables.clear();
        self.sucio = true;
        self.auto.pedir();
        if self.seleccionada == Some(i) {
            self.seleccionada = Some(destino as usize);
        }
    }

    // --- transporte -------------------------------------------------------

/// GO: aplica `on_previous` a lo que esté sonando y suena esta entrada.
///
/// Si la entrada tiene un `on_previous::FadeOut { gap }`, en vez de sonar
/// ahora se agenda: primero sale la anterior y, cuando termina su fade,
/// esperamos `gap` milisegundos y entonces suena esta. Es la receta "B
/// entra X segundos después de que A haya terminado su fade out" del
/// usuario, sin que el operador tenga que contar.
fn ir(&mut self, i: usize) {
        let Some(entrada) = self.entradas.get(i) else { return };
        if entrada.falta {
            self.anotar(format!("FALTA EL AUDIO de '{}'", entrada.nombre));
            return;
        }

        // Un audio **no se dispara sobre sí mismo**. Dos copias del mismo
        // archivo desfasadas unos segundos no son "más ambiente": son un eco
        // sucio. Si el original ya va por el segundo 20 y se le da otra vez a
        // GO, lo que se oye es el mismo audio en el 0 y en el 20 a la vez, y
        // repitiendo el gesto se apilan tantas copias que aquello es un caos
        // del que no se sale. Un GO sobre algo que ya suena no es un disparo:
        // es un accidente, y se avisa en vez de obedecer.
        //
        // Lo que sí se puede es apilar audios **distintos** (un ambiente, una
        // música y un efecto a la vez): lo prohibido es el mismo dos veces.
        //
        // Se mira dos veces a propósito. Primero esta fila, que da el mensaje
        // más directo ("ya está sonando"); y después si ese mismo archivo suena
        // **por otro sitio**, que es el caso que se cuela: un audio lanzado
        // dentro de un evento lleva su propia pista, así que la fila de la lista
        // no sabe nada de él y las dos copias convivirían sin enterarse.
        if self.entradas[i].sonando() {
            let como = if self.entradas[i].pausada() { "en pausa" } else { "sonando" };
            self.anotar(format!(
                "'{}' ya está {como}: no se apila una segunda copia encima",
                self.entradas[i].nombre
            ));
            self.seleccionada = Some(i);
            return;
        }
        let fichero = self.entradas[i].audio.file_name.clone();
        if let Some((quien, pausado)) = self.donde_suena(&fichero, Some(Objeto::Audio(i))) {
            let como = if pausado { "en pausa" } else { "sonando" };
            self.anotar(format!(
                "'{}' ya está {como} en '{quien}': el mismo audio no se apila",
                self.entradas[i].nombre
            ));
            self.seleccionada = Some(i);
            return;
        }

        let spec = entrada.spec.clone();
        let fuente = entrada.fuente.clone();
        let nombre = entrada.nombre.clone();
        let ahora = self.arranque.elapsed().as_millis() as u64;

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
                OnPrevious::FadeOut { duration, curve, .. } => p.stop_after(duration, curve),
                OnPrevious::Duck { duration, curve, level_percent } => {
                    p.fade_to(level_percent as f32 / 100.0, duration, curve);
                    e.duck = true;
                }
            }
        }

        // 2. ¿Hay retardo? Si lo hay, agendamos en vez de sonar ya.
        //    Sólo se retrasa cuando hay una pista sonando: si la sala está
        //    en silencio, no tiene sentido esperar al "fade out" de nadie.
        let retardo = match spec.on_previous {
            OnPrevious::FadeOut { duration, gap, .. } => {
                let sonando = self.entradas.iter().any(|e| e.sonando());
                if sonando {
                    duration + gap
                } else {
                    Duration::ZERO
                }
            }
            _ => Duration::ZERO,
        };

        if retardo > Duration::ZERO {
            let disparar_en = ahora + retardo.as_millis() as u64;
            self.programador.demorar_entrada(i, disparar_en);
            self.anotar(format!(
                "GO demorado: '{}' sonará en {} ms (al terminar el fade out)",
                nombre,
                retardo.as_millis()
            ));
            self.ultima_disparada = Some(i);
            self.seleccionada = Some(i);
            return;
        }

        // 3. Suena esta.
        match self.backend.play(&spec, fuente.como_audio_source()) {
            Ok(pista) => {
                self.anotar(format!("GO: {nombre}"));
                self.ultima_disparada = Some(i);

                // Auto-follow: si esta entrada pide que la siguiente arranque
                // sola, se programa aquí (T-SHOW-002).
                let follow = self.entradas[i].auto_follow;
                let hay_siguiente = i + 1 < self.entradas.len();
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

    /// Lanza un evento: monta la escena tal como está guardada.
    ///
    /// No hay que volver a configurar nada para acortar o alargar la escena: el
    /// evento se lee tal cual está en ese momento, así que cambiar su duración
    /// surte efecto en el siguiente lanzamiento.
    fn ir_evento(&mut self, i: usize) {
        let Some(vivo) = self.eventos.get(i) else { return };
        let evento = vivo.evento.clone();

        if !evento.completo() {
            self.anotar(format!(
                "'{}' está a medias: le falta(n) {} audio(s)",
                evento.nombre,
                evento.huecos_sin_audio()
            ));
            return;
        }
        if vivo.falta {
            self.anotar(format!("FALTA EL AUDIO de '{}'", evento.nombre));
            return;
        }

        // El audio que entra tampoco puede estar ya sonando, ni por su cuenta
        // ni dentro de otro evento: el evento montaría una segunda copia del
        // mismo archivo sobre la que ya va, que es justo el eco que hay que
        // evitar. Aquí se ve muy bien por qué: un crossfade que trae un
        // ambiente ya sonando no cruzaría nada, apilaría el ambiente sobre sí
        // mismo.
        //
        // Se excluye este mismo evento del registro: relanzarlo es reiniciarlo,
        // y eso sí se permite (abajo se para la copia anterior antes de montar
        // la nueva). Sin excluirlo, un evento se bloquearía a sí mismo.
        if let Some(pista) = evento.pistas.first() {
            if let Some((quien, pausado)) =
                self.donde_suena(&pista.audio.file_name, Some(Objeto::Evento(i)))
            {
                let como = if pausado { "en pausa" } else { "sonando" };
                self.anotar(format!(
                    "'{}': su audio ya está {como} en '{quien}'; el evento no monta una segunda copia",
                    evento.nombre
                ));
                return;
            }
        }

        // Relanzar un evento que ya está sonando lo reinicia: no se montan dos
        // copias del mismo audio encima.
        self.eventos[i].parar();

        let duracion = evento.duracion();
        let curva = evento.curva;

        // 1. Lo que sale. Sólo el crossfade y el fade out tocan lo que ya
        //    suena; un fade in o un disparo único se montan encima y no apagan
        //    nada. En los dos que sí actúan, el audio de base **no se elige**:
        //    es el que está sonando ahora mismo, así que se recorre lo que suena
        //    y se le aplica la rampa hacia su volumen objetivo. Si el objetivo
        //    es 0, la pista se apaga y se corta.
        let mut bajadas = 0;
        if evento.tipo.actua_sobre_lo_que_suena() {
            bajadas = self.bajar_lo_que_suena(evento.salida_pct, duracion, curva);
            if bajadas == 0 {
                self.anotar(format!(
                    "'{}': no hay nada sonando, no hay nada que bajar",
                    evento.nombre
                ));
            } else {
                self.anotar(format!("  · {bajadas} pista(s) → {} %", evento.salida_pct));
            }
        }

        // 2. Lo que entra. Sólo los tipos de entrada traen audio propio.
        let mut entra = false;
        if let Some(spec) = evento.spec_entrada() {
            if let Some(pista) = evento.pistas.first() {
                let nombre = pista.nombre.clone();
                match self.fuente_de_evento(pista) {
                    Some(fuente) => match self.backend.play(&spec, fuente.como_audio_source()) {
                        Ok(handle) => {
                            self.eventos[i].pistas[0] = Some(handle);
                            entra = true;
                            self.anotar(format!("  · {nombre}"));
                        }
                        Err(e) => {
                            self.anotar(format!("no se pudo reproducir '{nombre}': {e}"))
                        }
                    },
                    None => self.anotar(format!("FALTA EL AUDIO '{nombre}' en la lista")),
                }
            }
        }

        if entra || bajadas > 0 {
            let rotulo = match evento.tipo {
                TipoEvento::Golpe => "GOLPE",
                TipoEvento::FadeOut => "FADE OUT",
                _ => "EVENTO",
            };
            self.anotar(format!("{rotulo}: {} ({})", evento.nombre, evento.resumen()));
            self.evento_sel = Some(i);
        }
    }

    /// Aplica la rampa de salida a **lo que está sonando** y devuelve cuántas
    /// pistas ha movido.
    ///
    /// Es el lado que sale de un crossfade y el protagonista de un fade out:
    /// ninguno de los dos elige el audio, porque el que se va es el que ya
    /// está sonando. Su punto de partida es "donde esté sonando ahora", que no
    /// se sabe hasta que se lanza el evento; lo único configurable es el
    /// volumen al que llega.
    ///
    /// Si el objetivo es 0, la pista se apaga y **se corta** al terminar: sin
    /// el corte seguiría ocupando el mezclador en silencio hasta el final del
    /// archivo. El corte cae cuando la ganancia ya es 0, así que no se oye.
    fn bajar_lo_que_suena(&mut self, objetivo_pct: u8, duracion: Duration, curva: Curve) -> usize {
        let objetivo = objetivo_pct as f32 / 100.0;
        let mut cuantas = 0;

        // Audios de la lista principal
        for e in &mut self.entradas {
            let Some(p) = e.pista.as_ref() else { continue };
            if p.state().is_done() {
                continue;
            }
            if objetivo <= 0.0 {
                p.stop_after(duracion, curva);
            } else {
                p.fade_to(objetivo, duracion, curva);
            }
            cuantas += 1;
        }
        // Audios lanzados por eventos: un crossfade tiene que bajar lo que
        // esté sonando venga de donde venga —si sólo se mirara `entradas`,
        // un ambiente que entró por un FadeIn se quedaría sonando encima
        // del crossfade, que es justo el "no pasa" que reportó el usuario.
        for v in &mut self.eventos {
            for p in v.pistas.iter().flatten() {
                if p.state().is_done() {
                    continue;
                }
                if objetivo <= 0.0 {
                    p.stop_after(duracion, curva);
                } else {
                    p.fade_to(objetivo, duracion, curva);
                }
                cuantas += 1;
            }
        }
        cuantas
    }

    /// Parada (T-SHOW-003): fade corto y corte.
    ///
    /// El fade no es un adorno: cortar un PCM a mitad de ciclo se oye como un
    /// clic, y en una sala eso suena a fallo del equipo.
    ///
    /// Antes de cortar nada se apunta **qué bucles estaban sonando**, porque
    /// esa lista es lo único que PLAY devuelve después. Se apunta aquí y no en
    /// el botón para que cualquier parada —el botón, un atajo, lo que sea—
    /// deje la misma memoria.
    ///
    /// Un efecto de una sola pasada **no** se apunta: ya se oyó entero y
    /// repetirlo en mitad de la función sería un susto, no una reanudación.
    /// Es la diferencia que pidió el usuario entre parar y pausar: la pausa
    /// no pierde nada, la parada sólo guarda lo que estaba en bucle.
    fn parar_todo(&mut self) {
        self.reanudables = self
            .entradas
            .iter()
            .enumerate()
            .filter(|(_, e)| e.sonando() && e.en_bucle())
            .map(|(i, _)| Objeto::Audio(i))
            .chain(
                self.eventos
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.sonando() && v.en_bucle())
                    .map(|(i, _)| Objeto::Evento(i)),
            )
            .collect();
        let apuntados = self.reanudables.len();

        let fade = Duration::from_millis(show::FADE_EMERGENCIA_MS);
        self.backend.stop_all_con_fade(fade);
        self.programador.cancelar();
        for e in &mut self.entradas {
            if let Some(p) = e.pista.take() {
                p.stop();
            }
            e.duck = false;
        }
        for v in &mut self.eventos {
            v.parar();
        }
        self.backend.stop_all();
        self.anotar(format!("PARAR: todo cortado (fade {} ms)", show::FADE_EMERGENCIA_MS));
        match apuntados {
            0 => self.anotar("  · no había ningún bucle sonando: PLAY no tendrá qué devolver"),
            1 => self.anotar("  · 1 bucle apuntado: PLAY lo devuelve"),
            n => self.anotar(format!("  · {n} bucles apuntados: PLAY los devuelve")),
        }
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
        for v in &mut self.eventos {
            v.limpiar_terminadas();
        }
    }

    // --- transporte global ------------------------------------------------
    //
    // Los tres gestos de un reproductor, aplicados a todo lo que suena. La
    // diferencia entre ellos no es de matiz, es de qué se puede recuperar
    // después:
    //
    // | gesto  | posición | envolvente | se puede volver atrás |
    // |--------|----------|------------|-----------------------|
    // | PAUSA  | se queda | se queda   | sí, tal cual          |
    // | SEGUIR | sigue    | sigue      | —                     |
    // | PARAR  | se pierde| se pierde  | sólo los bucles       |
    //
    // De ahí la asimetría de PLAY: lo que se pausó vuelve donde estaba, y lo
    // que se paró sólo vuelve si era un bucle.

    /// true si hay algo congelado ahora mismo.
    fn hay_pausa(&self) -> bool {
        self.entradas.iter().any(|e| e.pausada()) || self.eventos.iter().any(|v| v.pausada())
    }

    /// Cuántas pistas están sonando ahora, contando las congeladas.
    fn cuantos_activos(&self) -> usize {
        self.entradas.iter().filter(|e| e.sonando()).count()
            + self.eventos.iter().filter(|v| v.sonando()).count()
    }

    /// Congela todo donde está.
    ///
    /// Es lo contrario de parar: aquí no se pierde nada. Un bucle se queda a
    /// mitad de vuelta, un efecto de una sola pasada se queda en su segundo
    /// doce, y un fade se queda a media rampa. Todo eso vuelve exactamente
    /// igual, porque una pista en pausa no consume muestras y el motor mide
    /// sus rampas en muestras, no en reloj.
    fn pausar_todo(&mut self) {
        if self.cuantos_activos() == 0 {
            self.anotar("PAUSA: no hay nada sonando");
            return;
        }
        let mut cuantas = 0;
        self.por_cada_pista(|p| {
            if !p.is_paused() {
                p.pause();
                cuantas += 1;
            }
        });
        match cuantas {
            0 => self.anotar("PAUSA: ya estaba todo congelado"),
            n => self.anotar(format!("PAUSA: {n} pista(s) congeladas donde estaban")),
        }
    }

    /// Pasa por todas las pistas vivas que la app tiene montadas.
    ///
    /// Se recorre lo que tiene la app y **no** el registro interno del motor,
    /// y eso es a propósito: lo que obedecen los mandos tiene que ser
    /// exactamente lo que enseña la zona de reproductor, ni una pista más ni
    /// una menos. Si los dos criterios se separaran, el operador vería una
    /// fila que no responde o, peor, se pararía algo que no está en la lista.
    fn por_cada_pista(&mut self, mut f: impl FnMut(&dyn TrackHandle)) {
        for e in &mut self.entradas {
            if let Some(p) = e.pista.as_deref() {
                if !p.state().is_done() {
                    f(p);
                }
            }
        }
        for v in &mut self.eventos {
            for p in v.pistas.iter().flatten() {
                if !p.state().is_done() {
                    f(p.as_ref());
                }
            }
        }
    }

    /// El botón PLAY, que hace dos cosas distintas según de dónde venga.
    ///
    /// Si hay algo en pausa, sigue donde estaba: la pausa no perdió nada, así
    /// que no hay nada que reconstruir. Si no hay nada en pausa, se viene de
    /// una parada, y entonces se devuelven los bucles que la parada apuntó.
    ///
    /// Los dos casos no se mezclan en la práctica —parar descongela todo—, así
    /// que no hay ambigüedad en el orden en que se comprueban.
    fn reproducir(&mut self) {
        if self.hay_pausa() {
            let mut cuantas = 0;
            self.por_cada_pista(|p| {
                if p.is_paused() {
                    p.resume();
                    cuantas += 1;
                }
            });
            self.anotar(format!("PLAY: {cuantas} pista(s) siguen donde se quedaron"));
            return;
        }
        self.reanudar_parada();
    }

    /// Devuelve los bucles que sonaban al parar.
    fn reanudar_parada(&mut self) {
        if self.reanudables.is_empty() {
            self.anotar("PLAY: no hay nada que reanudar (la última parada no apuntó ningún bucle)");
            return;
        }
        // La lista se vacía al usarla: si se quedara, un segundo PLAY montaría
        // otra vez los mismos bucles encima de sí mismos, que es justo lo que
        // no puede pasar.
        let lista = std::mem::take(&mut self.reanudables);
        let mut puestas = 0;
        for quien in lista {
            let ok = match quien {
                Objeto::Audio(i) => self.relanzar_en_sitio(i),
                Objeto::Evento(i) => self.relanzar_evento_en_sitio(i),
            };
            if ok {
                puestas += 1;
            }
        }
        self.anotar(format!("PLAY: {puestas} bucle(s) de vuelta"));
    }

    /// Vuelve a poner en marcha una entrada **tal cual estaba**.
    ///
    /// `ir()` monta la escena: aplica `on_previous` a lo que suena, agenda el
    /// retardo, programa el auto-follow. Al reanudar una parada no queremos
    /// nada de eso. Si se usara `ir()`, devolver tres ambientes a la vez sería
    /// un desastre: el segundo que arranca le aplicaría su `on_previous` al
    /// primero que se acaba de arrancar, y entre ellos se apagarían. Lo que
    /// hay que hacer es volver a ponerlos en marcha, no volver a montar la
    /// escena.
    fn relanzar_en_sitio(&mut self, i: usize) -> bool {
        let Some(e) = self.entradas.get(i) else { return false };
        if e.falta {
            let nombre = e.nombre.clone();
            self.anotar(format!("no se puede reanudar '{nombre}': falta el audio"));
            return false;
        }
        let spec = e.spec.clone();
        let fuente = e.fuente.clone();
        let nombre = e.nombre.clone();
        match self.backend.play(&spec, fuente.como_audio_source()) {
            Ok(pista) => {
                self.entradas[i].pista = Some(pista);
                self.entradas[i].duck = false;
                self.anotar(format!("  · {nombre}"));
                true
            }
            Err(err) => {
                self.anotar(format!("no se pudo reanudar '{nombre}': {err}"));
                false
            }
        }
    }

    /// Lo mismo que [`Self::relanzar_en_sitio`], para un evento.
    fn relanzar_evento_en_sitio(&mut self, i: usize) -> bool {
        let Some(v) = self.eventos.get(i) else { return false };
        if v.falta || !v.evento.completo() {
            return false;
        }
        let evento = v.evento.clone();
        let Some(spec) = evento.spec_entrada() else { return false };
        let Some(hueco) = evento.pistas.first().cloned() else { return false };
        let Some(fuente) = self.fuente_de_evento(&hueco) else {
            self.anotar(format!("no se puede reanudar '{}': falta el audio", evento.nombre));
            return false;
        };
        let nombre = hueco.nombre.clone();
        match self.backend.play(&spec, fuente.como_audio_source()) {
            Ok(pista) => {
                self.eventos[i].pistas[0] = Some(pista);
                self.anotar(format!("  · {nombre}"));
                true
            }
            Err(err) => {
                self.anotar(format!("no se pudo reanudar '{nombre}': {err}"));
                false
            }
        }
    }

    /// Foto de todo lo que está sonando en este instante.
    ///
    /// Incluye lo congelado: una pista en pausa sigue montada, con su posición
    /// y su envolvente, y el operador tiene que poder verla y seguirla desde
    /// aquí.
    fn activos(&self) -> Vec<Activo> {
        let mut v: Vec<Activo> = Vec::new();
        for (i, e) in self.entradas.iter().enumerate() {
            if !e.sonando() {
                continue;
            }
            v.push(Activo {
                quien: Objeto::Audio(i),
                nombre: e.nombre.clone(),
                color: CUE_COLORS[e.color],
                pos: e.pista.as_ref().map(|p| p.position()),
                total: e.pista.as_ref().and_then(|p| p.duration()),
                ganancia: e.ganancia(),
                pausado: e.pausada(),
                saliendo: matches!(e.estado(), Some(TrackState::FadingOut)),
                bucle: e.en_bucle(),
                rotulo_salida: e.rotulo_salida(),
                ayuda_salida: e.ayuda_salida(),
            });
        }
        for (i, ev) in self.eventos.iter().enumerate() {
            if !ev.sonando() {
                continue;
            }
            let p = ev.pistas.iter().flatten().next();
            let tipo = match ev.evento.tipo {
                TipoEvento::Golpe => "golpe",
                TipoEvento::FadeIn => "fade in",
                TipoEvento::FadeOut => "fade out",
                TipoEvento::Crossfade => "crossfade",
            };
            v.push(Activo {
                quien: Objeto::Evento(i),
                nombre: format!("{} ({tipo})", ev.evento.nombre),
                color: EVENT_COLORS[i % EVENT_COLORS.len()],
                pos: p.map(|t| t.position()),
                total: p.and_then(|t| t.duration()),
                ganancia: ev.ganancia(),
                pausado: ev.pausada(),
                saliendo: p.is_some_and(|t| matches!(t.state(), TrackState::FadingOut)),
                bucle: ev.en_bucle(),
                // Un evento no tiene salida configurable: se corta y ya.
                rotulo_salida: "PARAR",
                ayuda_salida: "Un evento no tiene salida configurable: se corta."
                    .to_string(),
            });
        }
        v
    }

    /// Nombre legible de lo que hay en una fila, para el registro.
    fn nombre_de(&self, quien: Objeto) -> String {
        match quien {
            Objeto::Audio(i) => self.entradas.get(i).map(|e| e.nombre.clone()),
            Objeto::Evento(i) => self.eventos.get(i).map(|v| v.evento.nombre.clone()),
        }
        .unwrap_or_default()
    }

    /// Manda una orden de transporte a **una** pista: la de su fila.
    ///
    /// Los mandos de arriba son para todo a la vez; esto es para cuando el
    /// operador quiere tocar una sola cosa sin mover el resto.
    ///
    /// `Parar` sobre un audio **no corta a lo bruto**: obedece a la salida que
    /// ese audio tenga configurada en el inspector (`FR-04`). Es lo que hace que
    /// la sección "Cómo sale" sirva de algo — hasta ahora se podía configurar
    /// entera y no la leía nadie, así que poner "Sale con fade out" y que el
    /// audio se cortara igual era lo normal. Un evento no tiene salida
    /// configurable, así que ahí se corta.
    fn aplicar_accion(&mut self, quien: Objeto, accion: Accion) {
        let mut cuantas = 0usize;
        let mut como = accion.rotulo();
        match quien {
            Objeto::Audio(i) => {
                let salida = self.entradas.get(i).map(|e| e.spec.exit);
                if let (Some(salida), Some(p)) =
                    (salida, self.entradas.get(i).and_then(|e| e.pista.as_deref()))
                {
                    match (accion, salida) {
                        (Accion::Parar, ExitMode::FadeOut { duration, curve }) => {
                            p.stop_after(duration, curve);
                            como = "SALIR";
                        }
                        _ => accion.aplicar(p),
                    }
                    cuantas = 1;
                }
            }
            Objeto::Evento(i) => {
                if let Some(v) = self.eventos.get(i) {
                    for p in v.pistas.iter().flatten() {
                        accion.aplicar(p.as_ref());
                        cuantas += 1;
                    }
                }
            }
        }
        if cuantas == 0 {
            return;
        }
        let nombre = self.nombre_de(quien);
        self.anotar(format!("{como}: {nombre}"));
    }

    /// Alto del panel de reproductor: una fila por pista activa, más los mandos.
    ///
    /// Se calcula **antes** de crear el panel porque egui necesita el alto de
    /// antemano.
    ///
    /// `alto_ventana` es lo que se lleva el reproductor como mucho: **un
    /// tercio**. En una pantalla baja, una función con ocho pistas sonando
    /// dejaría la lista de audios —que es donde el operador hace clic— en nada.
    /// A partir de ahí la lista de pistas hace scroll, y los mandos, que son lo
    /// que hay que alcanzar de un manotazo, siguen siempre a la vista.
    fn alto_transporte(&self, alto_ventana: f32) -> f32 {
        let mut alto = ALTO_MANDOS + ALTO_FILA_ACTIVO * self.cuantos_activos() as f32;
        if self.ver_registro {
            alto += ALTO_REGISTRO;
        }
        // El tope nunca puede quedar por debajo del mínimo, o `clamp` entraría
        // en pánico con el rango invertido. De ahí el `clamp` interior.
        let tope = (alto_ventana / 3.0).clamp(ALTO_MANDOS, ALTO_TRANSPORTE_MAX);
        alto.clamp(ALTO_MANDOS, tope)
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
    /// Rampa que sube: "entra con fade".
    Entrada,
    /// Dos rampas cruzadas: "sale la anterior mientras entra esta".
    Cruce,
    /// Rampa que baja: "sale con fade".
    Salida,
    /// Flecha con barra: "arranca la siguiente".
    Siguiente,
    /// Circuito cerrado: "en loop".
    Loop,
    /// Altavoz: volumen.
    Volumen,
    /// Reloj: espera.
    Espera,
    /// Rayo: disparo único, un efecto de golpe.
    Rayo,
    /// Más y menos: acortar o alargar una escena.
    Mas,
    Menos,
    /// Lápiz: el evento se puede editar sobre la marcha.
    Editar,
    /// Teclas: cómo se dispara a mano.
    Teclado,
    /// Triángulo: arranca, o sigue donde se quedó.
    Play,
    /// Dos barras: congela donde está, sin perder la posición.
    Pausa,
    /// Cuadrado: corta de verdad.
    Detener,
}

fn dibujar_icono(painter: &egui::Painter, rect: egui::Rect, kind: IconKind, color: egui::Color32) {
    let stroke = egui::Stroke { width: 1.5, color };
    let fill = color;
    let c = rect.center();
    let w = rect.width();
    let h = rect.height();
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
        // Una rampa que sube, con punta de flecha. Es la forma del fade in.
        IconKind::Entrada => {
            let o = egui::pos2(rect.left(), rect.bottom());
            let d = egui::pos2(rect.right(), rect.top());
            painter.line_segment([o, d], stroke);
            painter.add(egui::Shape::convex_polygon(
                vec![d, egui::pos2(d.x - w * 0.3, d.y + h * 0.12), egui::pos2(d.x - w * 0.12, d.y + h * 0.3)],
                color,
                egui::Stroke::NONE,
            ));
        }
        // Dos rampas cruzadas: la de A bajando y la de B subiendo.
        IconKind::Cruce => {
            painter.line_segment([egui::pos2(rect.left(), rect.top()), egui::pos2(rect.right(), rect.bottom())], stroke);
            painter.line_segment([egui::pos2(rect.left(), rect.bottom()), egui::pos2(rect.right(), rect.top())], stroke);
        }
        // Una rampa que baja, con punta de flecha. Es la forma del fade out.
        IconKind::Salida => {
            let o = egui::pos2(rect.left(), rect.top());
            let d = egui::pos2(rect.right(), rect.bottom());
            painter.line_segment([o, d], stroke);
            painter.add(egui::Shape::convex_polygon(
                vec![d, egui::pos2(d.x - w * 0.3, d.y - h * 0.12), egui::pos2(d.x - w * 0.12, d.y - h * 0.3)],
                color,
                egui::Stroke::NONE,
            ));
        }
        IconKind::Siguiente => {
            painter.line_segment([egui::pos2(rect.right(), rect.center().y), egui::pos2(rect.left() + w * 0.45, rect.center().y), ], stroke);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(rect.right(), rect.center().y),
                    egui::pos2(rect.right() - w * 0.28, rect.center().y - h * 0.22),
                    egui::pos2(rect.right() - w * 0.28, rect.center().y + h * 0.22),
                ],
                color,
                egui::Stroke::NONE,
            ));
            painter.line_segment([egui::pos2(rect.left(), rect.top() + h * 0.25), egui::pos2(rect.left(), rect.bottom() - h * 0.25)], stroke);
        }
        IconKind::Loop => {
            painter.circle_stroke(rect.center(), w * 0.34, stroke);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(rect.center().x + w * 0.34, rect.center().y - h * 0.05),
                    egui::pos2(rect.center().x + w * 0.22, rect.center().y - h * 0.28),
                    egui::pos2(rect.center().x + w * 0.46, rect.center().y - h * 0.22),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        IconKind::Volumen => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(rect.left(), rect.center().y - h * 0.18),
                    egui::pos2(rect.left() + w * 0.25, rect.center().y - h * 0.18),
                    egui::pos2(rect.left() + w * 0.52, rect.center().y - h * 0.34),
                    egui::pos2(rect.left() + w * 0.52, rect.center().y + h * 0.34),
                    egui::pos2(rect.left() + w * 0.25, rect.center().y + h * 0.18),
                    egui::pos2(rect.left(), rect.center().y + h * 0.18),
                ],
                color,
                egui::Stroke::NONE,
            ));
            painter.circle_stroke(egui::pos2(rect.left() + w * 0.62, rect.center().y), w * 0.1, stroke);
            painter.line_segment([egui::pos2(rect.right(), rect.center().y - h * 0.22), egui::pos2(rect.right(), rect.center().y + h * 0.22)], stroke);
        }
        IconKind::Espera => {
            painter.circle_stroke(rect.center(), w * 0.36, stroke);
            painter.line_segment([rect.center(), egui::pos2(rect.center().x, rect.center().y - h * 0.22), ], stroke);
            painter.line_segment([rect.center(), egui::pos2(rect.center().x + w * 0.2, rect.center().y), ], stroke);
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
        // El transporte: los tres gestos de siempre, dibujados como en
        // cualquier reproductor. Un triángulo, dos barras y un cuadrado.
        IconKind::Play => {
            let pad = w * 0.16;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(tl.x + pad, tl.y + pad),
                    egui::pos2(tl.x + pad, br.y - pad),
                    egui::pos2(br.x - pad * 0.5, c.y),
                ],
                fill,
                egui::Stroke::NONE,
            ));
        }
        IconKind::Pausa => {
            let ancho = w * 0.2;
            let alto = h * 0.7;
            for k in 0..2 {
                let x = tl.x + w * 0.27 + k as f32 * (ancho + w * 0.19);
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, c.y - alto / 2.0),
                        egui::vec2(ancho, alto),
                    ),
                    egui::CornerRadius::same(1),
                    fill,
                );
            }
        }
        IconKind::Detener => {
            let lado = w * 0.6;
            painter.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(lado, lado)),
                egui::CornerRadius::same(1),
                fill,
            );
        }
        // Rayo: un efecto que entra de golpe y se acaba.
        IconKind::Rayo => {
            let pts = [
                egui::pos2(c.x + w * 0.16, tl.y + 1.0),
                egui::pos2(c.x - w * 0.22, c.y),
                egui::pos2(c.x - w * 0.01, c.y),
                egui::pos2(c.x - w * 0.16, br.y - 1.0),
                egui::pos2(c.x + w * 0.26, c.y - h * 0.04),
                egui::pos2(c.x + w * 0.03, c.y - h * 0.04),
            ];
            painter.add(egui::Shape::convex_polygon(
                pts.to_vec(),
                fill,
                egui::Stroke::NONE,
            ));
        }
        // Más y menos: el mismo brazo, con o sin la barra horizontal.
        IconKind::Mas | IconKind::Menos => {
            let arm = w * 0.32;
            painter.line_segment(
                [egui::pos2(c.x - arm, c.y), egui::pos2(c.x + arm, c.y)],
                stroke,
            );
            if matches!(kind, IconKind::Mas) {
                painter.line_segment(
                    [egui::pos2(c.x, c.y - arm), egui::pos2(c.x, c.y + arm)],
                    stroke,
                );
            }
        }
        // Tres teclas en fila: cómo se dispara esto a mano.
        IconKind::Teclado => {
            let ancho_tecla = w * 0.26;
            let alto_tecla = h * 0.44;
            for i in 0..3 {
                let x = tl.x + w * 0.11 + i as f32 * (ancho_tecla + w * 0.06);
                let tecla = egui::Rect::from_min_size(
                    egui::pos2(x, c.y - alto_tecla / 2.0),
                    egui::vec2(ancho_tecla, alto_tecla),
                );
                painter.rect_stroke(
                    tecla,
                    egui::CornerRadius::same(2),
                    stroke,
                    egui::epaint::StrokeKind::Inside,
                );
            }
        }
        // Lápiz: este evento se puede retocar sin rehacerlo.
        IconKind::Editar => {
            let punta = egui::pos2(tl.x + w * 0.18, br.y - h * 0.18);
            let cabo = egui::pos2(br.x - w * 0.18, tl.y + h * 0.18);
            painter.line_segment([punta, cabo], stroke);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    punta,
                    egui::pos2(punta.x - w * 0.14, punta.y - h * 0.06),
                    egui::pos2(punta.x + w * 0.06, punta.y - h * 0.14),
                ],
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

/// Botón de la fila de mandos: rectangular, relleno de color y **sólo icono**.
///
/// El texto se cambió por iconos porque en esa fila el escaso es el ancho:
/// "SEGUIR", "PAUSA" y "PARAR TODO" se comían media barra y dejaban el nombre
/// de la salida en un susurro. Lo que un icono no puede decir —cuál de los dos
/// gestos de PLAY va a ocurrir, por ejemplo— lo dice el tooltip, que es donde
/// se busca cuando hace falta y no ocupa nada cuando no.
///
/// Se usa con `ui.add_enabled`, que ya se encarga de las dos cosas que debe
/// hacer un botón apagado: bajar la opacidad de lo que se pinte y no contar el
/// clic.
struct BotonMandos {
    icono: IconKind,
    tooltip: &'static str,
    relleno: egui::Color32,
    color: egui::Color32,
}

impl BotonMandos {
    fn nuevo(
        icono: IconKind,
        tooltip: &'static str,
        relleno: egui::Color32,
        color: egui::Color32,
    ) -> Self {
        Self {
            icono,
            tooltip,
            relleno,
            color,
        }
    }
}

impl egui::Widget for BotonMandos {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ANCHO_BOTON_MANDOS, ALTO_BOTON_MANDOS),
            egui::Sense::click(),
        );

        // Hundido mientras se mantiene pulsado, como los botones de texto.
        let relleno = if response.is_pointer_button_down_on() {
            self.relleno.gamma_multiply(0.85)
        } else {
            self.relleno
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), relleno);

        // El icono va en un cuadrado centrado: los tres gestos —triángulo, dos
        // barras, cuadrado— se dibujan sobre su rect, y con uno más ancho que
        // alto saldrían deformes.
        let icono = egui::Rect::from_center_size(rect.center(), egui::vec2(20.0, 20.0));
        dibujar_icono(ui.painter(), icono, self.icono, self.color);

        response.on_hover_text(self.tooltip)
    }
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

/// La barra que deja **ver** la envolvente de una pista.
///
/// Va aparte del volumen a propósito. El volumen es lo que el operador
/// configuró: no cambia mientras suena, así que enseñarlo no diría nada. La
/// envolvente es lo que el motor aplica en esta muestra, y es lo único que se
/// mueve durante un fade. Enseñarla convierte el fade en algo visible, que es
/// justo lo que hacía falta para poder decir "el fade va" o "esto entra de
/// golpe" sin fiarse del oído en mitad de un ensayo.
///
/// En ámbar cuando la pista está congelada: la barra se queda quieta donde
/// estaba, y el color avisa de que no se ha acabado, de que está en pausa.
fn barra_envolvente(ui: &mut egui::Ui, ganancia: f32, pausado: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(88.0, 12.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, egui::CornerRadius::same(3), BG_ROW_ACTIVE);
    let g = ganancia.clamp(0.0, 1.0);
    if g > 0.001 {
        let lleno =
            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * g, rect.height()));
        p.rect_filled(
            lleno,
            egui::CornerRadius::same(3),
            if pausado { WARN_AMBER } else { GO_GREEN },
        );
    }
    p.rect_stroke(
        rect,
        egui::CornerRadius::same(3),
        egui::Stroke { width: 1.0, color: FG_MUTE },
        egui::epaint::StrokeKind::Inside,
    );
}

/// Pestaña con icono grande arriba y etiqueta debajo.
///
/// Es más compacta que `boton_icono` con texto al lado, así caben todas en una
/// franja estrecha. La pestaña activa se distingue por el fondo coloreado y un
/// borde verde (el mismo verde del GO).
///
/// `ancho` se puede dar más holgado cuando sólo hay dos pestañas y la etiqueta
/// es larga: las del panel central ("Audios" / "Eventos") lo necesitan.
fn pestana_icono(
    ui: &mut egui::Ui,
    icono: IconKind,
    rotulo: &str,
    activa: bool,
    ancho: f32,
) -> egui::Response {
    let w = ancho;
    let h = 64.0_f32;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());

    let bg = if activa {
        // La activa lleva el fondo del cue activo: se identifica sin tener
        // que leer la etiqueta.
        BG_ROW_ACTIVE
    } else if response.is_pointer_button_down_on() {
        BG_ROW_ACTIVE
    } else if response.hovered() {
        BG_ROW_ALT
    } else {
        BG_PANEL
    };
    ui.painter().rect_filled(rect, egui::CornerRadius::same(6), bg);
    if activa {
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(6),
            egui::Stroke { width: 1.5, color: GO_GREEN },
            egui::epaint::StrokeKind::Inside,
        );
    }

    let icon_color = if activa { GO_GREEN } else { FG_BASE };
    let icon_size = 24.0_f32;
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 8.0 + icon_size / 2.0),
        egui::vec2(icon_size, icon_size),
    );
    dibujar_icono(ui.painter(), icon_rect, icono, icon_color);

    let text_color = if activa { FG_STRONG } else { FG_BASE };
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 8.0),
        egui::Align2::CENTER_BOTTOM,
        rotulo,
        egui::FontId::proportional(13.0),
        text_color,
    );

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
        self.frames = self.frames.saturating_add(1);
        // Se anota en qué frame empezó la pulsación. En el frame 1 puede venir
        // una que se inició antes de que existiera esta ventana (la que abrió
        // el programa, o el foco que da el sistema): esa no es del operador y
        // no debe accionar nada.
        if ctx.input(|i| i.pointer.any_pressed()) {
            self.frame_pulsacion = Some(self.frames);
        }
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
                // Primero se mira si esa F está asignada a algo (un audio o un
                // evento); si no, la F dispara el audio de esa posición (F1 = el
                // primero), que es lo que hará cualquiera sin configurar nada.
                let asignada = self
                    .entradas
                    .iter()
                    .position(|e| e.tecla.as_deref() == Some(nombre.as_str()))
                    .map(Objeto::Audio)
                    .or_else(|| {
                        self.eventos
                            .iter()
                            .position(|v| v.evento.tecla.as_deref() == Some(nombre.as_str()))
                            .map(Objeto::Evento)
                    });

                let destino = asignada.or(Some(Objeto::Audio(n)));
                match destino {
                    Some(Objeto::Audio(i)) if i < self.entradas.len() => self.ir(i),
                    Some(Objeto::Evento(i)) => self.ir_evento(i),
                    _ => {}
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

        // Mientras haya algo montado se repinta seguido: la zona de
        // reproductor mueve posiciones, envolventes y contadores, y una pista
        // en pausa también cuenta (hay que poder ver dónde se quedó).
        let hay_audio = self.entradas.iter().any(|e| e.sonando())
            || self.eventos.iter().any(|v| v.sonando());
        if hay_audio {
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
            .exact_size(self.alto_transporte(ui.available_height()))
            .show(ui, |ui| self.transporte(ui));

        if self.bloqueo.permite_editar() {
            egui::Panel::right("inspector")
                .exact_size(420.0)
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
        // El asistente va al final: es una ventana encima de todo lo demás.
        if self.asistente.is_some() {
            self.ventana_asistente(ui);
        }
    }
}

impl App {
    fn barra_superior(&mut self, ui: &mut egui::Ui) {
        // Con la guarda de siempre: si el programa se acaba de abrir y la
        // pulsación venía en vuelo, no se acciona ningún botón de aquí. Daño
        // posible si no: "Nueva" tira la obra sin preguntar.
        let clic_del_operador = self.clic_fiable();
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

            if boton_icono(ui, IconKind::Nueva, "Nueva", None).clicked() && clic_del_operador {
                self.obra_nueva();
            }
            if boton_icono(ui, IconKind::Abrir, "Abrir…", None).clicked() && clic_del_operador {
                self.abrir();
            }
            if boton_icono(ui, IconKind::Guardar, "Guardar", None).clicked() && clic_del_operador {
                self.guardar();
            }
            if boton_icono(ui, IconKind::Guardar, "Guardar como…", None).clicked()
                && clic_del_operador
            {
                self.guardar_como();
            }
            ui.separator();
            if boton_icono(ui, IconKind::AnadirAudio, "Añadir audio…", None).clicked()
                && clic_del_operador
            {
                self.anadir_audio();
            }
            if boton_icono(ui, IconKind::Abrir, "Abrir carpeta…", None).clicked()
                && clic_del_operador
            {
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

    /// Zona de reproductor: los mandos globales y la lista de lo que suena.
    ///
    /// Es lo que faltaba para poder operar sin adivinar. Antes había un
    /// "PARAR TODO" y un texto con los nombres de lo que sonaba; con eso no se
    /// podía ni parar una sola cosa, ni saber si un fade estaba ocurriendo de
    /// verdad. Ahora hay los tres gestos para todo el conjunto y, en cada fila,
    /// seguir/pausar y **sacar esa pista** —que obedece a la salida que tenga
    /// configurada—, con la envolvente a la vista.
    fn transporte(&mut self, ui: &mut egui::Ui) {
        // PARAR TODO y Probar sonaron por un clic en vuelo es exactamente lo
        // que no puede pasar: el primero corta la función, el segundo mete un
        // tono por la salida. Con la guarda de siempre.
        let clic_del_operador = self.clic_fiable();
        let hay_algo = self.cuantos_activos() > 0;
        let en_pausa = self.hay_pausa();

        // --- mandos: los tres gestos, para todo a la vez --------------------
        //
        // La fila se lleva un alto **fijo**, no el que le deje el panel. El
        // resumen de la derecha va con `with_layout`, y `with_layout` se queda
        // con todo el alto que encuentre: sin el tope, la fila crecía hasta el
        // borde del panel, el separador y el aviso de abajo se salían por la
        // ventana, y el panel central —que se pinta después— acababa tapando
        // la mitad de arriba de los botones.
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ALTO_BOTON_MANDOS),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                // PLAY hace las dos cosas que se le pueden pedir —seguir si
                // estaba en pausa, devolver los bucles si se había parado—, así
                // que el icono es el mismo y lo que cambia es el tooltip. Con
                // texto eran dos etiquetas distintas, y el ancho de las tres
                // era justo lo que no cabía.
                let play = BotonMandos::nuevo(
                    IconKind::Play,
                    if en_pausa {
                        "Seguir donde se quedó"
                    } else {
                        "Reproducir"
                    },
                    GO_GREEN,
                    egui::Color32::BLACK,
                );
                if ui.add(play).clicked() && clic_del_operador {
                    self.reproducir();
                }

                let pausa = BotonMandos::nuevo(
                    IconKind::Pausa,
                    "Pausar todo",
                    BG_ROW_ACTIVE,
                    FG_STRONG,
                );
                if ui.add_enabled(hay_algo, pausa).clicked() && clic_del_operador {
                    self.pausar_todo();
                }

                let parar =
                    BotonMandos::nuevo(IconKind::Detener, "Parar todo", STOP_RED, FG_STRONG);
                if ui.add(parar).clicked() && clic_del_operador {
                    self.parar_todo();
                }

                ui.add_space(16.0);
                ui.label("Salida:");
                egui::ComboBox::from_id_salt("salida")
                    .width(260.0)
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
                if boton_icono(ui, IconKind::Probar, "Probar", None).clicked() && clic_del_operador {
                    self.probar_salida();
                }

                // Resumen a la derecha: cuántas pistas y en qué estado, sin
                // tener que contar filas.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = self.cuantos_activos();
                    let (texto, color) = if n == 0 {
                        ("en silencio".to_string(), FG_MUTE)
                    } else if en_pausa {
                        (format!("{n} en pausa"), WARN_AMBER)
                    } else {
                        (format!("{n} sonando"), GO_GREEN)
                    };
                    ui.label(egui::RichText::new(texto).color(color).strong());
                });
            },
        );

        ui.separator();
        self.lista_activos(ui);

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

    /// La lista de lo que suena ahora mismo: una fila por pista.
    ///
    /// La barra de envolvente es la respuesta a "¿el fade se está haciendo?".
    /// No enseña el volumen configurado —eso no cambia mientras suena y no
    /// diría nada—, sino la ganancia que el motor aplica **en esta muestra**.
    /// Si sube poco a poco, la rampa va; si salta de vacío a lleno en una
    /// sola fila, ese audio entra de golpe y no hay fade que valga, lo pida o
    /// no su configuración.
    fn lista_activos(&mut self, ui: &mut egui::Ui) {
        let activos = self.activos();
        if activos.is_empty() {
            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Nada sonando en este instante").color(FG_MUTE));
            });
            return;
        }

        let clic_del_operador = self.clic_fiable();
        // La orden se apunta y se aplica al salir del bucle: dentro no se
        // puede tocar `self`, que es de donde salen los nombres que se están
        // pintando.
        let mut orden: Option<(Objeto, Accion)> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for a in &activos {
                    let fila = egui::Frame::new()
                        .fill(if a.pausado { BG_ROW_ALT } else { BG_ROW })
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.set_min_height(ALTO_FILA_ACTIVO - 6.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("■").color(a.color).size(14.0));
                                ui.label(
                                    egui::RichText::new(&a.nombre)
                                        .color(if a.pausado { WARN_AMBER } else { GO_GREEN })
                                        .strong(),
                                );

                                barra_envolvente(ui, a.ganancia, a.pausado);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{:>3} %",
                                        (a.ganancia * 100.0).round() as i32
                                    ))
                                    .color(FG_BASE)
                                    .monospace()
                                    .size(11.0),
                                );

                                if let Some(p) = a.pos {
                                    let texto = match a.total {
                                        Some(t) => {
                                            format!("{} / {}", formato_mmss(p), formato_mmss(t))
                                        }
                                        None => formato_mmss(p),
                                    };
                                    ui.label(
                                        egui::RichText::new(texto)
                                            .color(FG_MUTE)
                                            .monospace()
                                            .size(11.0),
                                    );
                                }

                                if a.bucle {
                                    ui.label(egui::RichText::new("bucle").color(FG_MUTE).size(11.0));
                                }
                                if a.saliendo {
                                    ui.label(
                                        egui::RichText::new("saliendo").color(WARN_AMBER).size(11.0),
                                    );
                                }
                                if a.pausado {
                                    ui.label(
                                        egui::RichText::new("EN PAUSA")
                                            .color(WARN_AMBER)
                                            .size(11.0)
                                            .strong(),
                                    );
                                }

                                // Los mandos de esta pista. El de salir lleva
                                // su nombre de verdad —"SALIR", "CORTA" o
                                // "PARAR"— porque una pista con fade out tarda
                                // en irse y eso hay que saberlo antes de
                                // pulsar, no después.
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let salir =
                                            boton_icono(ui, IconKind::Detener, a.rotulo_salida, None);
                                        if salir.clicked() && clic_del_operador {
                                            orden = Some((a.quien, Accion::Parar));
                                        }
                                        salir.on_hover_text(&a.ayuda_salida);

                                        if a.pausado {
                                            if boton_icono_chico(
                                                ui,
                                                IconKind::Play,
                                                Some(GO_GREEN),
                                            )
                                            .clicked()
                                                && clic_del_operador
                                            {
                                                orden = Some((a.quien, Accion::Seguir));
                                            }
                                        } else if boton_icono_chico(ui, IconKind::Pausa, None)
                                            .clicked()
                                            && clic_del_operador
                                        {
                                            orden = Some((a.quien, Accion::Pausar));
                                        }
                                    },
                                );
                            });
                        });
                    let _ = fila;
                }
            });

        if let Some((quien, accion)) = orden {
            self.aplicar_accion(quien, accion);
        }
    }

    // --- lista de entradas -------------------------------------------------

    /// Panel central: la franja de pestañas y la lista que toque.
    fn lista(&mut self, ui: &mut egui::Ui) {
        let mut destino = self.lista;
        let clic_del_operador = self.clic_fiable();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
            for t in [ListaTab::Audios, ListaTab::Eventos] {
                let resp = pestana_icono(ui, t.icono(), t.rotulo(), self.lista == t, 96.0);
                if resp.clicked() && clic_del_operador {
                    destino = t;
                }
            }

            ui.add_space(12.0);
            if self.lista == ListaTab::Eventos {
                if self.bloqueo.permite_editar()
                    && boton_icono(ui, IconKind::Nueva, "Nuevo evento…", None).clicked()
                    && clic_del_operador
                {
                    self.abrir_asistente();
                }
                ui.label(
                    egui::RichText::new(format!("{} eventos", self.eventos.len()))
                        .color(FG_MUTE),
                );
            } else {
                ui.label(
                    egui::RichText::new(format!("{} audios", self.entradas.len())).color(FG_MUTE),
                );
            }
        });
        self.lista = destino;

        ui.separator();

        match self.lista {
            ListaTab::Audios => self.lista_audios(ui),
            ListaTab::Eventos => self.lista_eventos(ui),
        }
    }

    fn lista_audios(&mut self, ui: &mut egui::Ui) {
        // Un clic que venía en vuelo al abrirse la ventana no debe sonar ni
        // borrar nada: ver `clic_fiable`.
        let clic_del_operador = self.clic_fiable();
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
                                        if ui.add(go).clicked() && clic_del_operador {
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
                                        if boton_icono_chico(ui, IconKind::Quitar, Some(STOP_RED)).clicked()
                                            && clic_del_operador
                                        {
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

    /// Lista de eventos predefinidos.
    ///
    /// Los botones − y + de la duración funcionan **también en modo Función**:
    /// son la manera de acortar o alargar una escena sobre la marcha, que es
    /// justo lo que antes obligaba a rehacer la configuración. Mueven un
    /// número; los audios, la curva y el bucle se quedan como estaban.
    fn lista_eventos(&mut self, ui: &mut egui::Ui) {
        // Un clic que venía en vuelo al abrirse la ventana no debe sonar ni
        // borrar nada: ver `clic_fiable`.
        let clic_del_operador = self.clic_fiable();
        if self.eventos.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(70.0);
                ui.label(
                    egui::RichText::new("Todavía no hay eventos")
                        .color(FG_STRONG)
                        .size(20.0),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "Un evento es una escena montada: un audio que sube, que baja,\
                         dos que se cruzan, o un efecto de golpe.",
                    )
                    .color(FG_MUTE),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Se monta con los {} audios de la otra lista y se guarda con la obra.",
                        self.entradas.len()
                    ))
                    .color(FG_MUTE),
                );
                ui.add_space(16.0);
                if self.bloqueo.permite_editar()
                    && boton_icono(ui, IconKind::Nueva, "Crear un evento…", None).clicked()
                    && clic_del_operador
                {
                    self.abrir_asistente();
                }
            });
            return;
        }

        let mut seleccionar: Option<usize> = None;
        let mut lanzar: Option<usize> = None;
        let mut ajustar: Option<(usize, i64)> = None;
        let mut quitar: Option<usize> = None;
        let mut mover: Option<(usize, isize)> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in 0..self.eventos.len() {
                let v = &self.eventos[i];
                let nombre = v.evento.nombre.clone();
                let resumen = v.evento.resumen();
                let tipo = v.evento.tipo;
                let duracion = v.evento.duracion();
                let sonando = v.sonando();
                let falta = v.falta;
                let incompleto = !v.evento.completo();
                let tecla = v.evento.tecla.clone();
                let seleccionada = self.evento_sel == Some(i);
                let color = EVENT_COLORS[i % EVENT_COLORS.len()];
                let en_funcion = self.bloqueo.en_funcion();

                let fondo = if falta || incompleto {
                    egui::Color32::from_rgb(0x3A, 0x22, 0x26)
                } else if sonando || seleccionada {
                    BG_ROW_ACTIVE
                } else if i % 2 == 0 {
                    BG_ROW
                } else {
                    BG_ROW_ALT
                };

                let fila = egui::Frame::new()
                    .fill(fondo)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_min_height(if en_funcion { 72.0 } else { 40.0 });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("■").color(color).size(18.0));
                            ui.label(
                                egui::RichText::new(format!("{}", i + 1))
                                    .color(FG_MUTE)
                                    .monospace(),
                            );
                            if let Some(t) = &tecla {
                                ui.label(
                                    egui::RichText::new(t).color(FG_MUTE).monospace().size(11.0),
                                );
                            }

                            // Icono del tipo: es lo que distingue una fila de
                            // otra de un vistazo.
                            let icono_rect =
                                ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                            dibujar_icono(
                                ui.painter(),
                                icono_rect.0,
                                icono_de_tipo(tipo),
                                color,
                            );

                            let titulo = egui::RichText::new(&nombre)
                                .color(if falta || incompleto {
                                    STOP_RED
                                } else if sonando {
                                    GO_GREEN
                                } else {
                                    FG_STRONG
                                })
                                .size(if en_funcion { 18.0 } else { 14.0 });
                            if ui.selectable_label(seleccionada, titulo).clicked() {
                                seleccionar = Some(i);
                            }
                            ui.label(egui::RichText::new(&resumen).color(FG_BASE).size(11.0));

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Acortar / alargar la escena. Disponible
                                    // siempre: es el ajuste de una función.
                                    if boton_icono_chico(ui, IconKind::Mas, None).clicked()
                                        && clic_del_operador
                                    {
                                        ajustar = Some((i, paso_de_duracion(duracion) as i64));
                                    }
                                    ui.label(
                                        egui::RichText::new(formatear(duracion))
                                            .color(FG_STRONG)
                                            .monospace()
                                            .size(if en_funcion { 15.0 } else { 13.0 }),
                                    );
                                    if boton_icono_chico(ui, IconKind::Menos, None).clicked()
                                        && clic_del_operador
                                    {
                                        ajustar = Some((i, -(paso_de_duracion(duracion) as i64)));
                                    }

                                    if falta || incompleto {
                                        ui.add_enabled(
                                            false,
                                            egui::Button::new("FALTA").min_size(egui::vec2(70.0, 28.0)),
                                        );
                                    } else {
                                        let lanzar_btn = egui::Button::new(
                                            egui::RichText::new(if tipo.usa_fade() {
                                                "LANZAR"
                                            } else {
                                                "GOLPE"
                                            })
                                            .color(egui::Color32::BLACK)
                                            .strong(),
                                        )
                                        .fill(if tipo.usa_fade() { GO_GREEN } else { WARN_AMBER })
                                        .min_size(egui::vec2(
                                            78.0,
                                            if en_funcion { 46.0 } else { 28.0 },
                                        ));
                                        if ui.add(lanzar_btn).clicked() && clic_del_operador {
                                            lanzar = Some(i);
                                        }
                                    }

                                    if sonando {
                                        ui.label(
                                            egui::RichText::new("sonando")
                                                .color(GO_GREEN)
                                                .size(11.0),
                                        );
                                    }

                                    if self.bloqueo.permite_editar() {
                                        if boton_icono_chico(ui, IconKind::Subir, None).clicked() {
                                            mover = Some((i, -1));
                                        }
                                        if boton_icono_chico(ui, IconKind::Bajar, None).clicked() {
                                            mover = Some((i, 1));
                                        }
                                        if boton_icono_chico(ui, IconKind::Quitar, Some(STOP_RED))
                                            .clicked()
                                            && clic_del_operador
                                        {
                                            quitar = Some(i);
                                        }
                                    }
                                },
                            );
                        });
                    });
                let _ = fila;
            }
        });

        if let Some(i) = seleccionar {
            self.evento_sel = Some(i);
        }
        if let Some(i) = lanzar {
            self.ir_evento(i);
        }
        if let Some((i, delta)) = ajustar {
            self.ajustar_duracion_evento(i, delta);
        }
        if let Some(i) = quitar {
            self.quitar_evento(i);
        }
        if let Some((i, delta)) = mover {
            self.mover_evento(i, delta);
        }
    }

    // --- inspector ---------------------------------------------------------

    /// El panel de la derecha.
    ///
    /// Es **una sola columna con scroll**, sin pestañas. Antes había cinco
    /// pestañas con icono y había que adivinar en cuál vivía el control que se
    /// buscaba; ahora todo está a la vista en secciones con su rótulo, que es
    /// lo que hace falta cuando se está ajustando una escena.
    fn inspector(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            match self.lista {
                ListaTab::Audios => self.inspector_audio(ui),
                ListaTab::Eventos => self.inspector_evento(ui),
            }
        });
    }

    /// Ventana paso a paso para crear un evento: **primero el tipo, luego los
    /// audios**.
    ///
    /// Es el orden que tiene sentido: el tipo decide cuántos audios hacen falta
    /// y qué rampa le toca a cada uno, así que preguntarlo antes evita elegir
    /// dos audios para un evento que sólo necesita uno.
    fn ventana_asistente(&mut self, ui: &mut egui::Ui) {
        let Some(asistente) = self.asistente.as_mut() else { return };
        let mut cerrar = false;
        let mut crear = false;
        let mut atras = false;
        let mut adelante = false;

        let tipo = asistente.tipo;
        let paso = asistente.paso;

        egui::Window::new("Nuevo evento")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ui.ctx(), |ui| {
                ui.set_min_width(430.0);
                ui.label(
                    egui::RichText::new(format!("Paso {} de 2", paso + 1))
                        .color(FG_MUTE)
                        .size(11.0),
                );
                ui.add_space(6.0);

                if paso == 0 {
                    ui.label(
                        egui::RichText::new("1. ¿Qué tiene que pasar?")
                            .color(FG_STRONG)
                            .size(15.0),
                    );
                    ui.add_space(6.0);
                    for t in TipoEvento::TODOS {
                        ui.horizontal(|ui| {
                            let icono_rect = ui.allocate_exact_size(
                                egui::vec2(20.0, 20.0),
                                egui::Sense::hover(),
                            );
                            dibujar_icono(ui.painter(), icono_rect.0, icono_de_tipo(t), FG_BASE);
                            ui.radio_value(&mut asistente.tipo, t, t.rotulo());
                        });
                        ui.label(
                            egui::RichText::new(t.descripcion()).color(FG_MUTE).size(11.0),
                        );
                        ui.add_space(6.0);
                    }
                    if self.entradas.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "AVISO: todavía no hay audios en la obra. Se puede crear el \
                                 evento igual y elegirlos después.",
                            )
                            .color(WARN_AMBER)
                            .size(11.0),
                        );
                    }
                } else {
                    ui.label(
                        egui::RichText::new(format!("2. ¿Qué audio entra? ({})", tipo.rotulo()))
                            .color(FG_STRONG)
                            .size(15.0),
                    );
                    ui.add_space(6.0);

                    if tipo.cuantos_audios() == 0 {
                        // El fade out no pide audio: actúa sobre el que suene.
                        ui.label(
                            egui::RichText::new(
                                "Este evento no necesita ningún audio: baja el que esté \
                                 sonando en el momento en que lo lances.",
                            )
                            .color(FG_BASE),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                "La duración y hasta qué volumen baja se ajustan luego, en el \
                                 panel de la derecha.",
                            )
                            .color(FG_MUTE)
                            .size(11.0),
                        );
                    } else {
                        if tipo == TipoEvento::Crossfade {
                            ui.label(
                                egui::RichText::new(
                                    "El que se va no se elige: es el que esté sonando cuando \
                                     lances el evento.",
                                )
                                .color(FG_MUTE)
                                .size(11.0),
                            );
                            ui.add_space(6.0);
                        }

                        let nombres: Vec<String> =
                            self.entradas.iter().map(|e| e.nombre.clone()).collect();
                        let elegido = asistente.elegidos.first().copied().flatten();
                        let mut elegido_mut = elegido;
                        egui::ComboBox::from_id_salt("asistente_audio")
                            .width(280.0)
                            .selected_text(match elegido.and_then(|j| nombres.get(j)) {
                                Some(n) => n.clone(),
                                None => "— elige un audio —".to_string(),
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut elegido_mut, None, "— ninguno —");
                                for (j, nombre) in nombres.iter().enumerate() {
                                    ui.selectable_value(&mut elegido_mut, Some(j), nombre.clone());
                                }
                            });
                        if let Some(slot) = asistente.elegidos.first_mut() {
                            *slot = elegido_mut;
                        }

                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                "También se puede dejar para después: el evento se crea igual y \
                                 se elige el audio desde el panel de la derecha.",
                            )
                            .color(FG_MUTE)
                            .size(11.0),
                        );
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if paso == 0 {
                        if ui.button("Cancelar").clicked() {
                            cerrar = true;
                        }
                        if ui
                            .add(
                                egui::Button::new("Siguiente →")
                                    .fill(BG_ROW_ACTIVE)
                                    .min_size(egui::vec2(110.0, 28.0)),
                            )
                            .clicked()
                        {
                            adelante = true;
                        }
                    } else {
                        if ui.button("← Atrás").clicked() {
                            atras = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            cerrar = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Crear evento")
                                        .color(egui::Color32::BLACK)
                                        .strong(),
                                )
                                .fill(GO_GREEN)
                                .min_size(egui::vec2(120.0, 28.0)),
                            )
                            .clicked()
                        {
                            crear = true;
                        }
                    }
                });
            });

        if cerrar {
            self.asistente = None;
            self.anotar("asistente cancelado");
            return;
        }
        if atras {
            if let Some(a) = self.asistente.as_mut() {
                a.paso = 0;
            }
            return;
        }
        if adelante {
            if let Some(a) = self.asistente.as_mut() {
                // El tipo pudo cambiar en el paso 1: los huecos se ajustan al
                // pasar al paso 2, que es cuando se preguntan los audios.
                let cuantos = a.tipo.cuantos_audios();
                a.elegidos.resize(cuantos, None);
                a.paso = 1;
            }
            return;
        }
        if crear {
            self.crear_evento_del_asistente();
        }
    }

    /// Crea el evento con lo que se eligió en el asistente.
    fn crear_evento_del_asistente(&mut self) {
        let Some(a) = self.asistente.take() else { return };

        self.historial.registrar(&self.instantanea());

        let mut evento = Evento::nuevo(a.tipo);
        let cuantos = a.tipo.cuantos_audios();
        Self::reajustar_huecos(&mut evento);

        for slot in 0..cuantos {
            if let Some(j) = a.elegidos.get(slot).copied().flatten() {
                if let Some(entrada) = self.entradas.get(j) {
                    evento.pistas[slot].nombre = entrada.nombre.clone();
                    evento.pistas[slot].audio = entrada.audio.clone();
                }
            }
        }

        // Nombre con el tipo y un número: "Fade in 2". Se puede cambiar luego.
        let repetidos = self.eventos.iter().filter(|v| v.evento.tipo == a.tipo).count();
        evento.nombre = if repetidos == 0 {
            a.tipo.rotulo().to_string()
        } else {
            format!("{} {}", a.tipo.rotulo(), repetidos + 1)
        };

        let resumen = evento.resumen();
        self.eventos.push(EventoVivo::nuevo(evento.clone()));
        self.evento_sel = Some(self.eventos.len() - 1);
        self.lista = ListaTab::Eventos;
        self.refrescar_falta_eventos();
        self.sucio = true;
        self.auto.pedir();
        self.anotar(format!("evento creado: {} ({resumen})", evento.nombre));
    }

    /// Rótulo de sección: icono, título y una raya.
    ///
    /// El icono no es adorno: es lo que hace que se encuentre la sección de un
    /// vistazo al bajar por el panel, igual que en la lista de audios.
    fn seccion(ui: &mut egui::Ui, icono: IconKind, titulo: &str) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let rect = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            dibujar_icono(ui.painter(), rect.0, icono, FG_BASE);
            ui.add_space(4.0);
            ui.label(egui::RichText::new(titulo).color(FG_STRONG).size(14.0).strong());
        });
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(4.0);
    }

    /// Editor de un audio: nombre, transición, cruce, salida, repetición,
    /// volumen y los ajustes de la entrada (tecla y pad).
    fn inspector_audio(&mut self, ui: &mut egui::Ui) {
        let Some(i) = self.seleccionada else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    egui::RichText::new("Selecciona un audio\npara editarlo").color(FG_MUTE),
                );
            });
            return;
        };
        if self.entradas.get(i).is_none() {
            self.seleccionada = None;
            return;
        }

        // --- nombre ------------------------------------------------------
        let antes_nombre = self.entradas[i].nombre.clone();
        let mut nombre = antes_nombre.clone();
        ui.add(
            egui::TextEdit::singleline(&mut nombre)
                .font(egui::FontId::proportional(17.0))
                .desired_width(f32::INFINITY),
        );
        if nombre != antes_nombre {
            self.historial.registrar(&self.instantanea());
            self.entradas[i].nombre = nombre;
            self.sucio = true;
            self.auto.pedir();
        }
        ui.label(
            egui::RichText::new(self.entradas[i].fuente.resumen()).color(FG_MUTE).size(11.0),
        );

        // --- el sonido ---------------------------------------------------
        // Se saca una copia del spec, se edita y se vuelve a guardar: evita
        // pelearse con el borrow checker dentro de los closures de egui.
        let mut spec = self.entradas[i].spec.clone();
        let mut auto = self.entradas[i].auto_follow;
        let antes = spec.clone();
        let auto_antes = auto;

        App::seccion(ui, IconKind::Entrada, "Cómo entra este audio");
        ui_transicion(ui, &mut spec);

        App::seccion(ui, IconKind::Cruce, "Qué pasa con lo que esté sonando");
        ui_anterior(ui, &mut spec);

        App::seccion(ui, IconKind::Salida, "Cómo sale");
        ui_salida(ui, &mut spec, &mut auto);

        App::seccion(ui, IconKind::Loop, "Repetición");
        ui_repeticion(ui, &mut spec);

        App::seccion(ui, IconKind::Volumen, "Volumen");
        ui_volumen(ui, &mut spec);

        if spec != antes || auto != auto_antes {
            self.entradas[i].spec = spec;
            self.entradas[i].auto_follow = auto;
            self.sucio = true;
            self.auto.pedir();
        }

        App::seccion(ui, IconKind::Teclado, "Tecla y pads");
        self.ui_tecla_y_pad(ui, Objeto::Audio(i));
    }

    /// Editor de un evento: tipo, audios con sus dos extremos, duración,
    /// curva, bucle y ajustes del evento.
    fn inspector_evento(&mut self, ui: &mut egui::Ui) {
        let Some(i) = self.evento_sel else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    egui::RichText::new("Selecciona un evento\npara editarlo").color(FG_MUTE),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "Un evento es una escena montada con los audios de la otra lista.",
                    )
                    .color(FG_MUTE)
                    .size(11.0),
                );
            });
            return;
        };
        if self.eventos.get(i).is_none() {
            self.evento_sel = None;
            return;
        }

        let mut evento = self.eventos[i].evento.clone();
        let antes = evento.clone();

        // --- nombre ------------------------------------------------------
        ui.add(
            egui::TextEdit::singleline(&mut evento.nombre)
                .font(egui::FontId::proportional(17.0))
                .desired_width(f32::INFINITY),
        );

        if self.eventos[i].falta {
            ui.label(
                egui::RichText::new("Algún audio de este evento ya no está en la lista de audios")
                    .color(STOP_RED)
                    .size(11.0),
            );
        } else if !evento.completo() {
            ui.label(
                egui::RichText::new("Falta algún audio por elegir")
                    .color(WARN_AMBER)
                    .size(11.0),
            );
        }

        // --- tipo --------------------------------------------------------
        App::seccion(ui, IconKind::Editar, "Tipo de evento");
        // Se puede cambiar el tipo sin perder lo ya configurado: los audios
        // que caben se conservan y los huecos que sobren se van.
        let tipo_antes = evento.tipo;
        for t in TipoEvento::TODOS {
            ui.radio_value(&mut evento.tipo, t, t.rotulo());
        }
        if evento.tipo != tipo_antes {
            // Los huecos se reajustan al tipo nuevo, conservando los audios que
            // ya estuvieran elegidos.
            Self::reajustar_huecos(&mut evento);
        }
        ui.label(
            egui::RichText::new(evento.tipo.descripcion()).color(FG_MUTE).size(11.0),
        );

        // --- duración ----------------------------------------------------
        App::seccion(ui, IconKind::Espera, "Duración de la escena");
        if evento.tipo.usa_fade() {
            ui.label(
                egui::RichText::new(
                    "Éste es el número que se mueve para acortar o alargar la escena. \
                     Los audios y el resto de la configuración se quedan como están.",
                )
                .color(FG_MUTE)
                .size(11.0),
            );
            ui.add_space(4.0);
            let mut duracion = evento.duracion();
            ui.horizontal(|ui| {
                slider_duracion(ui, &mut duracion, "Duración");
            });
            evento.duracion_ms = duracion.as_millis() as u64;

            // Milisegundos exactos: para quien sabe que la escena son 7.500 ms.
            let mut ms = evento.duracion_ms as i64;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Exacto:").color(FG_MUTE));
                ui.add(
                    egui::DragValue::new(&mut ms)
                        .range(100..=600_000)
                        .speed(50.0)
                        .suffix(" ms"),
                );
            });
            evento.duracion_ms = ms.clamp(100, 600_000) as u64;

            ui.horizontal(|ui| {
                ui.label("Curva:");
                combo_curva(ui, &mut evento.curva);
            });
            dibujar_rampa(
                ui,
                evento.pistas.first().map(|p| p.desde_pct).unwrap_or(0),
                evento.pistas.first().map(|p| p.hasta_pct).unwrap_or(100),
                evento.curva,
            );
        } else {
            ui.label(
                egui::RichText::new(
                    "Un disparo único no tiene duración: suena de golpe y se acaba cuando \
                     termina el audio.",
                )
                .color(FG_MUTE)
                .size(11.0),
            );
        }

        // --- audios ------------------------------------------------------
        // --- lo que sale (sólo crossfade y fade out) ----------------------
        if evento.tipo.actua_sobre_lo_que_suena() {
            App::seccion(ui, IconKind::Salida, "Lo que está sonando");
            ui.label(
                egui::RichText::new(match evento.tipo {
                    TipoEvento::Crossfade => {
                        "El audio que se va no se elige: es el que está sonando cuando \
                         lances el evento, y sale desde donde esté en ese momento."
                    }
                    _ => "Este evento no lleva audio: baja el que esté sonando en ese momento.",
                })
                .color(FG_MUTE)
                .size(11.0),
            );
            ui.add_space(4.0);
            slider_porcentaje(ui, "Baja hasta", &mut evento.salida_pct);
            if evento.apaga_al_salir() {
                ui.label(
                    egui::RichText::new("Al llegar a 0 % la pista se apaga y se corta sola.")
                        .color(FG_MUTE)
                        .size(11.0),
                );
            } else {
                ui.label(
                    egui::RichText::new(format!(
                        "Se queda sonando al {} %, no se corta.",
                        evento.salida_pct
                    ))
                    .color(FG_MUTE)
                    .size(11.0),
                );
            }
            dibujar_rampa(ui, 100, evento.salida_pct, evento.curva);
        }

        // --- lo que entra (fade in, crossfade y disparo único) ------------
        if evento.tipo.cuantos_audios() > 0 {
            App::seccion(ui, IconKind::AnadirAudio, "Audio que entra");
            let nombres: Vec<String> = self.entradas.iter().map(|e| e.nombre.clone()).collect();

            let pista = &mut evento.pistas[0];
            let elegido = self.entradas.iter().position(|e| {
                !e.audio.file_name.is_empty() && e.audio.file_name == pista.audio.file_name
            });
            let mut elegido_mut = elegido;
            egui::ComboBox::from_id_salt("audio_evento_entra")
                .width(240.0)
                .selected_text(if pista.vacio() {
                    "— elige un audio —".to_string()
                } else {
                    pista.nombre.clone()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut elegido_mut, None, "— ninguno —");
                    for (j, nombre) in nombres.iter().enumerate() {
                        ui.selectable_value(&mut elegido_mut, Some(j), nombre.clone());
                    }
                });
            if elegido_mut != elegido {
                match elegido_mut {
                    Some(j) => {
                        let entrada = &self.entradas[j];
                        pista.nombre = entrada.nombre.clone();
                        pista.audio = entrada.audio.clone();
                    }
                    None => {
                        // Se quedan los porcentajes: quitar y volver a poner un
                        // audio no debe borrar la rampa que ya estaba ajustada.
                        pista.nombre.clear();
                        pista.audio = AudioRef::default();
                    }
                }
            }

            if evento.tipo.usa_fade() {
                slider_porcentaje(ui, "Desde", &mut pista.desde_pct);
                slider_porcentaje(ui, "Hasta", &mut pista.hasta_pct);
                if let Some(aviso) = aviso_de_rampa_plana(pista.desde_pct, pista.hasta_pct) {
                    ui.label(
                        egui::RichText::new(aviso).color(WARN_AMBER).size(11.0).strong(),
                    );
                }
            }
            ui_volumen_pista(ui, pista);
            ui.add_space(6.0);
        }

        // --- bucle -------------------------------------------------------
        App::seccion(ui, IconKind::Loop, "Repetición");
        ui_repeticion_evento(ui, &mut evento);

        if evento != antes {
            self.historial.registrar(&self.instantanea());
            self.eventos[i].evento = evento.clone();
            // Si el tipo cambió, las pistas en curso ya no cuadran.
            if self.eventos[i].pistas.len() != evento.pistas.len() {
                self.eventos[i].parar();
                self.eventos[i].pistas = (0..evento.pistas.len()).map(|_| None).collect();
            }
            self.refrescar_falta_eventos();
            self.sucio = true;
            self.auto.pedir();
        }

        App::seccion(ui, IconKind::Probar, "Lanzar");
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("LANZAR").color(egui::Color32::BLACK).strong(),
                    )
                    .fill(GO_GREEN)
                    .min_size(egui::vec2(110.0, 34.0)),
                )
                .clicked()
                && self.clic_fiable()
            {
                self.ir_evento(i);
            }
            ui.label(
                egui::RichText::new(self.eventos[i].evento.resumen()).color(FG_MUTE).size(11.0),
            );
        });

        App::seccion(ui, IconKind::Teclado, "Tecla y pads");
        self.ui_tecla_y_pad(ui, Objeto::Evento(i));
    }

    /// Tecla F y franja de pads, para un audio o para un evento.
    fn ui_tecla_y_pad(&mut self, ui: &mut egui::Ui, cual: Objeto) {
        let mut tecla = match cual {
            Objeto::Audio(i) => self.entradas[i].tecla.clone(),
            Objeto::Evento(i) => self.eventos[i].evento.tecla.clone(),
        };
        let mut pad = match cual {
            Objeto::Audio(i) => self.entradas[i].pad,
            Objeto::Evento(i) => self.eventos[i].evento.pad,
        };
        let antes_tecla = tecla.clone();
        let antes_pad = pad;

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Tecla:").color(FG_MUTE));
            egui::ComboBox::from_id_salt(("tecla", cual.sal()))
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
        ui.checkbox(&mut pad, "Aparece en la franja de pads");

        if tecla != antes_tecla || pad != antes_pad {
            self.historial.registrar(&self.instantanea());
            match cual {
                Objeto::Audio(i) => {
                    self.entradas[i].tecla = tecla;
                    self.entradas[i].pad = pad;
                }
                Objeto::Evento(i) => {
                    self.eventos[i].evento.tecla = tecla;
                    self.eventos[i].evento.pad = pad;
                }
            }
            self.sucio = true;
            self.auto.pedir();
        }
    }

    /// Ajusta el hueco de audio al tipo, conservando lo que ya estuviera
    /// elegido.
    ///
    /// Es lo que permite pasar de un fade in a un crossfade sin rehacer nada:
    /// los dos piden un audio —el que entra— y se queda donde estaba. Al pasar
    /// a un fade out el hueco desaparece, porque ese evento actúa sobre lo que
    /// ya suena.
    fn reajustar_huecos(evento: &mut Evento) {
        let cuantos = evento.tipo.cuantos_audios();
        while evento.pistas.len() > cuantos {
            evento.pistas.pop();
        }
        while evento.pistas.len() < cuantos {
            let (desde, hasta) = evento.tipo.extremos_por_defecto();
            evento.pistas.push(PistaEvento::vacia(desde, hasta));
        }
        // Los que siguen ahí conservan sus valores: sólo se recalculan los
        // extremos si el audio sigue sin elegir, que es cuando no hay nada que
        // perder.
        for pista in evento.pistas.iter_mut() {
            if pista.vacio() {
                let (desde, hasta) = evento.tipo.extremos_por_defecto();
                pista.desde_pct = desde;
                pista.hasta_pct = hasta;
            }
        }
    }

    /// Franja de pads (T-UI-009): efectos sueltos, disparables a mano.
    ///
    /// Un pad **no** apaga lo que está sonando por sí mismo: lo que pase con lo
    /// anterior lo decide el `onPrevious` de su entrada, igual que en la lista.
    fn franja_de_pads(&mut self, ui: &mut egui::Ui) {
        // Un clic que venía en vuelo al abrirse la ventana no debe sonar ni
        // borrar nada: ver `clic_fiable`.
        let clic_del_operador = self.clic_fiable();
        ui.label(egui::RichText::new("Pads").color(FG_MUTE).size(11.0));
        ui.add_space(2.0);

        let lado = if self.bloqueo.en_funcion() { 140.0 } else { 96.0 };

        // En la franja se mezclan audios y eventos: lo que el operador quiere
        // es tener a mano lo que lanza a mano, sin importarle de cuál de las
        // dos listas salga.
        let mut pads: Vec<Objeto> = self
            .entradas
            .iter()
            .enumerate()
            .filter(|(_, e)| e.pad)
            .map(|(i, _)| Objeto::Audio(i))
            .collect();
        pads.extend(
            self.eventos
                .iter()
                .enumerate()
                .filter(|(_, v)| v.evento.pad)
                .map(|(i, _)| Objeto::Evento(i)),
        );

        let mut disparar: Option<Objeto> = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for cual in pads {
                    let (color, sonando, falta, numero, nombre) = match cual {
                        Objeto::Audio(i) => {
                            let e = &self.entradas[i];
                            (CUE_COLORS[e.color], e.sonando(), e.falta, i + 1, e.nombre.clone())
                        }
                        Objeto::Evento(i) => {
                            let v = &self.eventos[i];
                            (
                                EVENT_COLORS[i % EVENT_COLORS.len()],
                                v.sonando(),
                                v.falta || !v.evento.completo(),
                                i + 1,
                                v.evento.nombre.clone(),
                            )
                        }
                    };
                    let relleno = if sonando {
                        color
                    } else {
                        egui::Color32::from_rgb(0x2C, 0x31, 0x3B)
                    };
                    let texto = format!("{numero}\n{nombre}");
                    let boton = egui::Button::new(
                        egui::RichText::new(texto)
                            .color(if sonando { egui::Color32::BLACK } else { FG_STRONG })
                            .size(if self.bloqueo.en_funcion() { 16.0 } else { 13.0 }),
                    )
                    .fill(relleno)
                    .min_size(egui::vec2(lado, lado));

                    if ui.add(boton).clicked() && !falta && clic_del_operador {
                        disparar = Some(cual);
                    }
                    ui.add_space(6.0);
                }
            });
        });

        match disparar {
            Some(Objeto::Audio(i)) => self.ir(i),
            Some(Objeto::Evento(i)) => self.ir_evento(i),
            None => {}
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
                    ui.horizontal(|ui| {
                        let icono_rect =
                            ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        dibujar_icono(
                            ui.painter(),
                            icono_rect.0,
                            IconKind::Siguiente,
                            FG_BASE,
                        );
                        ui.label(
                            egui::RichText::new(format!("después: {s}"))
                                .color(FG_MUTE)
                                .size(12.0),
                        );
                    });
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

/// Volumen propio de un audio dentro de un evento, en decibelios.
fn ui_volumen_pista(ui: &mut egui::Ui, pista: &mut PistaEvento) {
    let mut db = pista.volumen.as_db();
    ui.add(
        egui::Slider::new(&mut db, -60.0..=6.0)
            .text("Volumen")
            .suffix(" dB")
            .custom_formatter(|v, _| format!("{:+.1} dB", v)),
    );
    pista.volumen = MilliDb::from_db(db);
}

/// Repetición de un evento: para siempre, una vez, o N veces.
///
/// El valor por defecto es **infinito** (`-1`): un evento suele ser un
/// ambiente o una escena que se queda sonando, y es lo que el operador espera
/// si no ha tocado nada.
fn ui_repeticion_evento(ui: &mut egui::Ui, evento: &mut Evento) {
    if !evento.tipo.usa_fade() {
        ui.label(
            egui::RichText::new("Un disparo único suena una sola vez: es un efecto, no un loop.")
                .color(FG_MUTE)
                .size(11.0),
        );
        evento.bucle = 1;
        return;
    }
    if evento.tipo.cuantos_audios() == 0 {
        // El fade out no tiene audio propio que repetir: sólo mueve el que ya
        // suena, y eso no se repite.
        ui.label(
            egui::RichText::new(
                "Un fade out sólo mueve el audio que ya está sonando: no hay nada que repetir.",
            )
            .color(FG_MUTE)
            .size(11.0),
        );
        return;
    }

    let mut clase = if evento.infinito() { 0 } else if evento.bucle > 1 { 2 } else { 1 };
    ui.radio_value(&mut clase, 0, "Para siempre (loop)");
    ui.radio_value(&mut clase, 1, "Una sola vez");
    ui.radio_value(&mut clase, 2, "N veces");
    ui.add_space(6.0);

    match clase {
        0 => evento.bucle = BUCLE_INFINITO,
        1 => evento.bucle = 1,
        _ => {
            let mut n = if evento.bucle > 1 { evento.bucle } else { 2 } as f32;
            ui.add(egui::Slider::new(&mut n, 2.0..=20.0).text("Veces").suffix(" x"));
            evento.bucle = n.round().max(2.0) as i32;
        }
    }

    ui.label(
        egui::RichText::new(format!("(bucle = {})", evento.bucle))
            .color(FG_MUTE)
            .size(11.0)
            .monospace(),
    );
}

/// Icono de cada tipo de evento.
fn icono_de_tipo(tipo: TipoEvento) -> IconKind {
    match tipo {
        TipoEvento::FadeIn => IconKind::Entrada,
        TipoEvento::FadeOut => IconKind::Salida,
        TipoEvento::Crossfade => IconKind::Cruce,
        TipoEvento::Golpe => IconKind::Rayo,
    }
}

/// De cuánto es el paso de los botones − y + de la duración.
///
/// Un 10 % de la escena, redondeado a 250 ms y nunca menos de 250: así el
/// ajuste se nota igual si la escena dura 2 s que si dura 2 minutos, y pulsar
/// una vez siempre se oye.
fn paso_de_duracion(d: Duration) -> u64 {
    let bruto = (d.as_millis() as f64 * 0.10).round() as i64;
    ((bruto / 250).max(1) * 250) as u64
}

fn combo_curva(ui: &mut egui::Ui, curva: &mut Curve) {
    // Lineal va primero: es la opción por defecto y la única que la mayoría de
    // los usuarios necesitan.
    egui::ComboBox::from_id_salt("curva")
        .width(180.0)
        .selected_text(rotulo_curva(*curva))
        .show_ui(ui, |ui| {
            for c in [Curve::Linear, Curve::EqualPower, Curve::Exponential] {
                ui.selectable_value(curva, c, rotulo_curva(c));
            }
        });
}

fn rotulo_curva(c: Curve) -> &'static str {
    match c {
        Curve::Linear => "lineal (recomendada)",
        Curve::EqualPower => "equal-power",
        Curve::Exponential => "exponencial",
    }
}

/// Las tres maneras de entrar que se le ofrecen al operador.
///
/// "Sube" y "Baja" sólo ponen los valores de partida: después los dos extremos
/// se pueden dejar en cualquier par (30 → 70, 90 → 40…), que es lo que hace
/// falta en teatro y lo que antes no se podía pedir.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TipoTransicion {
    Golpe,
    Sube,
    Baja,
}

/// Qué avisar cuando los dos extremos de la rampa son iguales.
///
/// Una rampa de 100 a 100 **no es una rampa**: es una entrada de golpe, con la
/// duración y la curva puestas de adorno. Y una de 0 a 0 no suena nada. En los
/// dos casos el panel sigue diciendo "con fade" mientras el audio hace otra
/// cosa, que es justo el síntoma con el que el operador no entiende qué pasa:
/// configuró un fade y el audio se coloca de golpe.
///
/// No se prohíbe —los dos extremos iguales y por encima de cero son una manera
/// legítima de decir "entra ya a este nivel"—, pero no puede pasar en silencio.
fn aviso_de_rampa_plana(desde: u8, hasta: u8) -> Option<&'static str> {
    if desde != hasta {
        return None;
    }
    Some(if hasta == 0 {
        "Los dos extremos están en 0 %: así no suena nada, no hay rampa que hacer."
    } else {
        "Los dos extremos son iguales, así que no hay rampa: esto entra de golpe. \
         Para que suba, baja el «Desde» por debajo del «Hasta»."
    })
}

fn ui_transicion(ui: &mut egui::Ui, spec: &mut CueSpec) {
    let (mut desde, mut hasta, mut duracion, mut curva, clase_inicial) = match spec.entrance {
        Entrance::Hit => (100u8, 100u8, Duration::from_secs(3), Curve::Linear, TipoTransicion::Golpe),
        Entrance::FadeIn { duration, curve, from_percent, to_percent } => {
            let clase = if to_percent >= from_percent {
                TipoTransicion::Sube
            } else {
                TipoTransicion::Baja
            };
            (from_percent, to_percent, duration, curve, clase)
        }
    };
    let mut clase = clase_inicial;

    ui.radio_value(&mut clase, TipoTransicion::Golpe, "De golpe (sin fade)");
    ui.radio_value(&mut clase, TipoTransicion::Sube, "Sube con fade");
    ui.radio_value(&mut clase, TipoTransicion::Baja, "Baja con fade");
    ui.add_space(6.0);

    // Al cambiar de tipo se ponen los extremos que le tocan: si no, pasar de
    // "sube" a "baja" dejaría una rampa plana que no haría nada.
    if clase != clase_inicial {
        (desde, hasta) = match clase {
            TipoTransicion::Sube => (0, 100),
            TipoTransicion::Baja => (100, 0),
            TipoTransicion::Golpe => (100, 100),
        };
    }

    if clase == TipoTransicion::Golpe {
        ui.label(
            egui::RichText::new(
                "Entra a pleno volumen desde la primera muestra: para un efecto puntual \
                 (una explosión, un rayo, un portazo).",
            )
            .color(FG_MUTE)
            .size(11.0),
        );
        ui.add_space(4.0);
        // Atajo: un efecto es "de golpe, una vez y se acaba". Las tres cosas a
        // la vez, que es lo que de verdad se quiere cuando se añade un trueno.
        if ui.button("Hacerlo disparo único (efecto)").clicked() {
            spec.entrance = Entrance::Hit;
            spec.loop_mode = LoopMode::None;
            spec.exit = ExitMode::UntilEnd;
            spec.on_previous = OnPrevious::Keep;
        }
        spec.entrance = Entrance::Hit;
        return;
    }

    slider_porcentaje(ui, "Desde", &mut desde);
    slider_porcentaje(ui, "Hasta", &mut hasta);

    if let Some(aviso) = aviso_de_rampa_plana(desde, hasta) {
        ui.label(egui::RichText::new(aviso).color(WARN_AMBER).size(11.0).strong());
    }

    ui.horizontal(|ui| {
        if ui.small_button("Invertir").clicked() {
            std::mem::swap(&mut desde, &mut hasta);
        }
        ui.label(
            egui::RichText::new(format!("{desde} % → {hasta} %"))
                .color(FG_STRONG)
                .monospace(),
        );
    });

    dibujar_rampa(ui, desde, hasta, curva);

    ui.horizontal(|ui| {
        slider_duracion(ui, &mut duracion, "Duración");
    });
    ui.horizontal(|ui| {
        ui.label("Curva:");
        combo_curva(ui, &mut curva);
    });

    spec.entrance = Entrance::FadeIn {
        duration: duracion,
        curve: curva,
        from_percent: desde,
        to_percent: hasta,
    };

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Los dos extremos son libres: un ambiente puede quedarse al 60 % en vez de \
             al 100 %, y una escena puede bajar hasta el 20 % sin apagarse del todo.",
        )
        .color(FG_MUTE)
        .size(11.0),
    );
}

/// Slider de porcentaje entero, 0–100.
fn slider_porcentaje(ui: &mut egui::Ui, etiqueta: &str, valor: &mut u8) {
    let mut v = *valor as f32;
    ui.add(
        egui::Slider::new(&mut v, 0.0..=100.0)
            .text(etiqueta)
            .step_by(1.0)
            .custom_formatter(|v, _| format!("{} %", v.round() as i32)),
    );
    *valor = v.round().clamp(0.0, 100.0) as u8;
}

/// Dibuja la rampa que se va a oír, con la curva de verdad.
///
/// Se dibuja con la misma función que usa el audio: si se dibujara una recta
/// cuando la curva es exponencial, el panel estaría mintiendo.
fn dibujar_rampa(ui: &mut egui::Ui, desde: u8, hasta: u8, curva: Curve) {
    let alto = 44.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width().max(60.0), alto), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, egui::CornerRadius::same(4), BG_PANEL);

    // Rejilla: las líneas de 0, 50 y 100 %.
    for fraccion in [0.0f32, 0.5, 1.0] {
        let y = rect.bottom() - fraccion * rect.height();
        p.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke { width: 1.0, color: BG_ROW },
        );
    }

    let sube = hasta >= desde;
    let pasos = 40;
    let mut puntos = Vec::with_capacity(pasos + 1);
    for i in 0..=pasos {
        let t = i as f32 / pasos as f32;
        let forma = teatroplayer::engine::envelope::forma_de_curva(curva, t, sube);
        let nivel = desde as f32 + (hasta as f32 - desde as f32) * forma;
        puntos.push(egui::pos2(
            rect.left() + t * rect.width(),
            rect.bottom() - (nivel / 100.0) * rect.height(),
        ));
    }
    p.add(egui::Shape::line(
        puntos,
        egui::Stroke { width: 2.0, color: GO_GREEN },
    ));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TipoAnterior {
    Keep,
    FadeOut,
    Stop,
    Duck,
}

fn ui_anterior(ui: &mut egui::Ui, spec: &mut CueSpec) {
    let (mut tipo, mut duracion, mut curva, mut nivel, mut espera) = match spec.on_previous {
        OnPrevious::Keep => (TipoAnterior::Keep, Duration::from_secs(3), Curve::Linear, 30u8, Duration::ZERO),
        OnPrevious::FadeOut { duration, curve, gap } => {
            (TipoAnterior::FadeOut, duration, curve, 30, gap)
        }
        OnPrevious::Stop => (TipoAnterior::Stop, Duration::from_secs(3), Curve::Linear, 30, Duration::ZERO),
        OnPrevious::Duck { duration, curve, level_percent } => {
            (TipoAnterior::Duck, duration, curve, level_percent, Duration::ZERO)
        }
    };

    ui.radio_value(&mut tipo, TipoAnterior::Keep, "Se queda sonando (se encima)");
    ui.radio_value(&mut tipo, TipoAnterior::FadeOut, "Sale con fade out");
    ui.radio_value(&mut tipo, TipoAnterior::Stop, "Corta de golpe");
    ui.radio_value(&mut tipo, TipoAnterior::Duck, "Baja de volumen (duck)");
    ui.add_space(8.0);

    // Atajo para lo más pedido: que esta entre subiendo mientras la otra sale
    // bajando, con la misma duración y la misma curva. A mano son cuatro
    // controles en dos sitios distintos; aquí es un botón.
    if ui.button("Crossfade: yo subo y la anterior baja").clicked() {
        let (d, c) = match spec.entrance {
            Entrance::FadeIn { duration, curve, .. } => (duration, curve),
            Entrance::Hit => (Duration::from_secs(3), Curve::Linear),
        };
        // Si esta entrada entraba de golpe, se le pone su subida: un
        // crossfade con un lado plano no es un crossfade.
        if matches!(spec.entrance, Entrance::Hit) {
            spec.entrance = Entrance::FadeIn {
                duration: d,
                curve: c,
                from_percent: 0,
                to_percent: 100,
            };
        }
        duracion = d;
        curva = c;
        espera = Duration::ZERO;
        tipo = TipoAnterior::FadeOut;
    }
    ui.add_space(6.0);

    if tipo == TipoAnterior::FadeOut || tipo == TipoAnterior::Duck {
        ui.horizontal(|ui| {
            slider_duracion(ui, &mut duracion, "Duración");
        });
        ui.horizontal(|ui| {
            ui.label("Curva:");
            combo_curva(ui, &mut curva);
        });
    }
    if tipo == TipoAnterior::FadeOut {
        // El "gap" es lo que el usuario pidió: el tiempo entre que A termina
        // su fade y B empieza a sonar. Sin gap, B entra justo cuando A calla;
        // con gap, hay un silencio en medio.
        ui.horizontal(|ui| {
            // El icono de reloj (Espera) señala visualmente que esto es un
            // retardo, no una rampa de volumen.
            let icono_rect = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
            dibujar_icono(
                ui.painter(),
                icono_rect.0,
                IconKind::Espera,
                WARN_AMBER,
            );
            slider_duracion(ui, &mut espera, "Espera tras el fade");
        });
        ui.label(
            egui::RichText::new(
                "Esta entrada se dispara sola, después de que la anterior termine su fade out.",
            )
            .color(FG_MUTE)
            .size(11.0),
        );
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
        TipoAnterior::FadeOut => OnPrevious::FadeOut {
            duration: duracion,
            curve: curva,
            gap: espera,
        },
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
        egui::RichText::new("Cuando aprietas SALIR").color(FG_MUTE).size(11.0),
    );
    ui.add_space(4.0);

    let (mut tipo, mut duracion, mut curva) = match spec.exit {
        ExitMode::UntilEnd => (TipoSalida::UntilEnd, Duration::from_secs(2), Curve::Linear),
        ExitMode::FadeOut { duration, curve } => (TipoSalida::FadeOut, duration, curve),
        ExitMode::Hit => (TipoSalida::Hit, Duration::from_secs(2), Curve::Linear),
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
        // Con los dos extremos a la vista: un fade que va de 20 a 60 no se
        // puede resumir diciendo sólo cuánto dura.
        Entrance::FadeIn { duration, from_percent, to_percent, .. } => format!(
            "{}→{} % en {}",
            from_percent,
            to_percent,
            formatear(duration)
        ),
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

// ---------------------------------------------------------------------------
// Pruebas de la interfaz
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// La guarda del clic fantasma, que es fácil de romper sin darse cuenta.
    ///
    /// El síntoma que evita: abrir el programa con el ratón pulsado en otra
    /// ventana y que la pulsación, al soltarse encima, cambie de lista o
    /// dispare algo. No se puede probar con una ventana de verdad, pero sí la
    /// decisión, que es donde estaba el fallo.
    #[test]
    fn el_clic_que_venia_en_vuelo_no_es_fiable() {
        let mut app = App::new();

        // Sin haber visto ninguna pulsación todavía no se da por bueno nada.
        assert_eq!(app.frame_pulsacion, None);
        assert!(!app.clic_fiable());

        // La pulsación que llegó en el primer frame es la que abrió la ventana
        // (o el foco del sistema): no es del operador.
        app.frame_pulsacion = Some(1);
        assert!(!app.clic_fiable(), "la pulsación de apertura no debe accionar nada");

        // Con la ventana ya en marcha, cualquier pulsación posterior es real.
        app.frame_pulsacion = Some(2);
        assert!(app.clic_fiable());
        app.frame_pulsacion = Some(500);
        assert!(app.clic_fiable());
    }

    /// El paso de los botones ± de la duración de un evento.
    #[test]
    fn el_paso_de_duracion_se_adapta_a_la_escena() {
        // Escenas cortas: el mínimo de 250 ms, para que un toque se note.
        assert_eq!(paso_de_duracion(Duration::from_millis(200)), 250);
        assert_eq!(paso_de_duracion(Duration::from_millis(1000)), 250);
        // Un 10 %, redondeado a 250 ms.
        assert_eq!(paso_de_duracion(Duration::from_secs(3)), 250);
        assert_eq!(paso_de_duracion(Duration::from_secs(5)), 500);
        assert_eq!(paso_de_duracion(Duration::from_secs(8)), 750);
        assert_eq!(paso_de_duracion(Duration::from_secs(60)), 6000);
    }

    /// El icono de cada tipo de evento, para que no se cruce ninguno.
    #[test]
    fn cada_tipo_de_evento_tiene_su_icono() {
        let iconos: Vec<IconKind> =
            TipoEvento::TODOS.iter().map(|t| icono_de_tipo(*t)).collect();
        // Los cuatro tipos usan cuatro iconos distintos: si dos coincidieran,
        // la lista de eventos dejaría de distinguirse de un vistazo.
        for (i, a) in iconos.iter().enumerate() {
            for (j, b) in iconos.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        std::mem::discriminant(a),
                        std::mem::discriminant(b),
                        "los tipos {i} y {j} comparten icono"
                    );
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // La zona de reproductor
    // -----------------------------------------------------------------
    //
    // Aquí se prueba la **política** del transporte: quién se pausa, quién se
    // para, y qué se apunta para después. Que el fade dure lo que dice y que
    // la pausa congele la envolvente a medias se prueba aparte, contra la
    // cadena de audio de verdad, en `tests/fade_en_vivo.rs`: eso es motor, y
    // el motor no se puede comprobar con pistas de mentira.

    /// Pista de mentira: no suena, pero obedece y se acuerda de lo que le
    /// hicieron. Basta para comprobar a quién le llega cada orden.
    struct PistaFalsa {
        pausada: AtomicBool,
        parada: AtomicBool,
        /// Le pidieron bajar y cortarse al terminar, en vez de un corte seco.
        ///
        /// Se distinguen a propósito: es la diferencia entre que la salida
        /// configurada en el inspector sirva de algo o no.
        con_fade: AtomicBool,
    }

    impl PistaFalsa {
        fn nueva() -> Self {
            Self {
                pausada: AtomicBool::new(false),
                parada: AtomicBool::new(false),
                con_fade: AtomicBool::new(false),
            }
        }
    }

    impl TrackHandle for PistaFalsa {
        fn state(&self) -> TrackState {
            if self.parada.load(Ordering::SeqCst) {
                TrackState::Stopped
            } else if self.con_fade.load(Ordering::SeqCst) {
                TrackState::FadingOut
            } else if self.pausada.load(Ordering::SeqCst) {
                TrackState::Paused
            } else {
                TrackState::Playing
            }
        }
        fn position(&self) -> Duration {
            Duration::from_secs(7)
        }
        fn duration(&self) -> Option<Duration> {
            None
        }
        fn set_gain(&self, _gain: f32) {}
        fn fade_to(&self, _gain: f32, _d: Duration, _c: Curve) {}
        fn fade_out(&self, _d: Duration, _c: Curve) {}
        fn stop_after(&self, _d: Duration, _c: Curve) {
            self.con_fade.store(true, Ordering::SeqCst);
        }
        fn pause(&self) {
            self.pausada.store(true, Ordering::SeqCst);
        }
        fn resume(&self) {
            self.pausada.store(false, Ordering::SeqCst);
        }
        fn is_paused(&self) -> bool {
            self.pausada.load(Ordering::SeqCst)
        }
        fn gain(&self) -> f32 {
            0.5
        }
        fn stop(&self) {
            self.parada.store(true, Ordering::SeqCst);
        }
    }

    /// Una entrada con una pista ya montada, o sin pista si no suena.
    fn entrada_de_prueba(nombre: &str, bucle: LoopMode, suena: bool) -> Entrada {
        Entrada {
            nombre: nombre.to_string(),
            spec: CueSpec { loop_mode: bucle, ..CueSpec::simple() },
            color: 0,
            pista: suena.then(|| Box::new(PistaFalsa::nueva()) as Box<dyn TrackHandle>),
            duck: false,
            fuente: Fuente::Archivo(PathBuf::from("no-existe.wav")),
            audio: AudioRef::default(),
            auto_follow: AutoFollow::None,
            tecla: None,
            pad: false,
            falta: false,
        }
    }

    /// Un evento con su único hueco ya montado, o sin pista si no suena.
    fn evento_de_prueba(nombre: &str, bucle: i32, suena: bool) -> EventoVivo {
        let mut evento = Evento::nuevo(TipoEvento::FadeIn);
        evento.nombre = nombre.to_string();
        evento.bucle = bucle;
        evento.pistas[0] = PistaEvento::vacia(0, 100).con_audio(
            "ambiente",
            AudioRef { file_name: "ambiente.wav".to_string(), ..Default::default() },
        );
        let mut vivo = EventoVivo::nuevo(evento);
        if suena {
            vivo.pistas[0] = Some(Box::new(PistaFalsa::nueva()));
        }
        vivo
    }

    /// Lo que se anotó en el registro a partir de un punto.
    fn dicho_desde(app: &App, desde: usize) -> String {
        app.registro[desde.min(app.registro.len())..].join(" | ")
    }

    /// GO sobre un audio que ya suena no lo apila: lo avisa y no hace nada.
    ///
    /// Es el "eco irregular" que describió el usuario. Si el audio va por el
    /// segundo 20 y se le vuelve a dar a GO, lo que se oía era el mismo audio
    /// en el segundo 0 y en el 20 a la vez, y repitiendo el gesto se apilaban
    /// tantas copias que aquello dejaba de ser una función.
    #[test]
    fn un_audio_no_se_dispara_sobre_si_mismo() {
        let mut app = App::new();
        app.entradas.push(entrada_de_prueba("ambiente", LoopMode::Infinite, true));
        let antes = app.registro.len();

        app.ir(0);

        let dicho = dicho_desde(&app, antes);
        assert!(dicho.contains("ya está sonando"), "debería avisar: {dicho}");
        // Y lo que importa: no se ha intentado reproducir nada. Si hubiera
        // seguido adelante, el motor habría fallado al abrir "no-existe.wav".
        assert!(
            !dicho.contains("no se pudo reproducir"),
            "no debe llegar a intentarlo: {dicho}"
        );
        assert!(app.entradas[0].sonando(), "la pista que sonaba sigue siendo la misma");
    }

    /// Un audio en pausa tampoco se apila: el aviso lo dice tal cual.
    #[test]
    fn un_audio_en_pausa_tampoco_se_apila() {
        let mut app = App::new();
        app.entradas.push(entrada_de_prueba("ambiente", LoopMode::Infinite, true));
        app.pausar_todo();
        let antes = app.registro.len();

        app.ir(0);

        let dicho = dicho_desde(&app, antes);
        assert!(dicho.contains("ya está en pausa"), "{dicho}");
        assert!(!dicho.contains("no se pudo reproducir"), "{dicho}");
    }

    /// El mismo archivo no suena dos veces ni cruzando las dos listas.
    ///
    /// Un audio puede estar sonando como fila de la lista o dentro de un evento.
    /// El caso que se colaba es este: un evento lleva su propia pista, así que
    /// la fila de la lista no sabe nada de él y las dos copias del mismo archivo
    /// convivirían sin enterarse. La identidad es el archivo, así que la
    /// comprobación tiene que mirar las dos listas.
    #[test]
    fn el_mismo_archivo_no_suena_dos_veces_ni_cruzando_listas() {
        let mut app = App::new();
        // La fila apunta al mismo archivo que el hueco del evento.
        let mut fila = entrada_de_prueba("ambiente", LoopMode::Infinite, false);
        fila.audio.file_name = "ambiente.wav".to_string();
        app.entradas.push(fila);
        // `evento_de_prueba` usa "ambiente.wav", y está sonando.
        app.eventos.push(evento_de_prueba("escena", BUCLE_INFINITO, true));
        let antes = app.registro.len();

        app.ir(0);

        let dicho = dicho_desde(&app, antes);
        assert!(dicho.contains("escena"), "debería decir quién lo tiene: {dicho}");
        assert!(!dicho.contains("no se pudo reproducir"), "{dicho}");
        assert!(!app.entradas[0].sonando(), "no se ha montado una segunda copia");
    }

    /// Y al revés: el evento no se lanza si su audio ya suena como fila.
    #[test]
    fn el_evento_no_se_lanza_si_su_audio_ya_suena() {
        let mut app = App::new();
        let mut fila = entrada_de_prueba("ambiente", LoopMode::Infinite, true);
        fila.audio.file_name = "ambiente.wav".to_string();
        app.entradas.push(fila);
        app.eventos.push(evento_de_prueba("escena", BUCLE_INFINITO, false));
        let antes = app.registro.len();

        app.ir_evento(0);

        let dicho = dicho_desde(&app, antes);
        assert!(dicho.contains("no monta una segunda copia"), "{dicho}");
        assert!(!app.eventos[0].sonando(), "el evento no se ha montado");
    }

    /// Relanzar un evento que ya suena sigue permitido: se reinicia.
    ///
    /// La exclusión del propio evento dentro de `donde_suena` es justo lo que
    /// hace que esto funcione. Sin ella el evento se encontraría a sí mismo, se
    /// bloquearía, y no habría forma de repetir una escena.
    #[test]
    fn un_evento_se_puede_relanzar_a_si_mismo() {
        let mut app = App::new();
        app.eventos.push(evento_de_prueba("escena", BUCLE_INFINITO, true));
        let antes = app.registro.len();

        app.ir_evento(0);

        let dicho = dicho_desde(&app, antes);
        assert!(!dicho.contains("segunda copia"), "no debe bloquearse a sí mismo: {dicho}");
        // Ha llegado hasta el final: monta la escena y sólo se atasca en que el
        // audio no está en la lista, que en este test es lo esperado.
        assert!(dicho.contains("FALTA EL AUDIO"), "{dicho}");
    }

    /// Al parar se apuntan los bucles, y **sólo** los bucles.
    ///
    /// Es la regla que pidió el usuario: si se paró en mitad de un efecto de
    /// una sola pasada, PLAY devuelve los ambientes que sonaban en bucle, no
    /// el efecto. Repetir un trueno al reanudar sería un susto, no una
    /// reanudación.
    #[test]
    fn al_parar_solo_se_apuntan_los_bucles() {
        let mut app = App::new();
        app.entradas.push(entrada_de_prueba("ambiente", LoopMode::Infinite, true));
        app.entradas.push(entrada_de_prueba("trueno", LoopMode::None, true));
        // Un bucle que no está sonando no se apunta: no hay qué devolver.
        app.entradas.push(entrada_de_prueba("otro ambiente", LoopMode::Infinite, false));
        app.eventos.push(evento_de_prueba("escena", BUCLE_INFINITO, true));
        app.eventos.push(evento_de_prueba("remate", 1, true));

        app.parar_todo();

        assert_eq!(app.reanudables.len(), 2, "los dos bucles que estaban sonando");
        assert!(matches!(app.reanudables[0], Objeto::Audio(0)));
        assert!(matches!(app.reanudables[1], Objeto::Evento(0)));
        assert_eq!(app.cuantos_activos(), 0, "al parar no queda nada montado");
    }

    /// PAUSA no pierde nada: todo sigue montado y vuelve donde estaba.
    ///
    /// La diferencia con PARAR es que aquí no hay que apuntar nada para
    /// después, porque no hace falta: la pista sigue ahí, congelada.
    #[test]
    fn la_pausa_no_pierde_lo_que_la_parada_pierde() {
        let mut app = App::new();
        app.entradas.push(entrada_de_prueba("ambiente", LoopMode::Infinite, true));
        app.entradas.push(entrada_de_prueba("trueno", LoopMode::None, true));

        app.pausar_todo();

        assert!(app.hay_pausa(), "las dos quedan congeladas");
        assert!(app.entradas[0].pausada() && app.entradas[1].pausada());
        // El efecto de una sola pasada sigue montado: la pausa no lo tira.
        assert_eq!(app.cuantos_activos(), 2);
        assert!(app.reanudables.is_empty(), "la pausa no necesita apuntar nada");

        app.reproducir();

        assert!(!app.hay_pausa(), "PLAY las descongela");
        assert!(app.entradas[0].sonando() && app.entradas[1].sonando());
    }

    /// PLAY tras una parada sin bucles no se inventa nada.
    #[test]
    fn play_sin_bucles_apuntados_no_reanuda_nada() {
        let mut app = App::new();
        app.entradas.push(entrada_de_prueba("trueno", LoopMode::None, true));
        app.parar_todo();
        let antes = app.registro.len();

        app.reproducir();

        assert!(dicho_desde(&app, antes).contains("no hay nada que reanudar"));
        assert_eq!(app.cuantos_activos(), 0);
    }

    /// La memoria de STOP se gasta al usarla: un segundo PLAY no vuelve a
    /// montar los mismos bucles encima de sí mismos.
    #[test]
    fn la_memoria_de_la_parada_se_gasta() {
        let mut app = App::new();
        app.entradas.push(entrada_de_prueba("ambiente", LoopMode::Infinite, true));
        app.parar_todo();
        assert_eq!(app.reanudables.len(), 1);

        // Reanuda de verdad: el archivo no existe, así que el motor fallará,
        // pero la memoria se gasta igual. Es justo lo que hay que comprobar.
        app.reproducir();
        assert!(app.reanudables.is_empty(), "ya se ha usado");

        let antes = app.registro.len();
        app.reproducir();
        assert!(dicho_desde(&app, antes).contains("no hay nada que reanudar"));
    }

    /// Los mandos de una fila tocan sólo esa fila.
    #[test]
    fn el_mando_de_una_fila_no_toca_las_demas() {
        let mut app = App::new();
        app.entradas.push(entrada_de_prueba("ambiente", LoopMode::Infinite, true));
        app.entradas.push(entrada_de_prueba("musica", LoopMode::Infinite, true));

        app.aplicar_accion(Objeto::Audio(1), Accion::Pausar);

        assert!(!app.entradas[0].pausada(), "el ambiente no se toca");
        assert!(app.entradas[1].pausada(), "la música sí");

        app.aplicar_accion(Objeto::Audio(0), Accion::Parar);

        assert!(!app.entradas[0].sonando(), "parada de verdad");
        assert!(app.entradas[1].sonando(), "y la otra sigue en pausa, no parada");
        assert!(app.entradas[1].pausada());
    }

    /// La zona de reproductor enseña lo que suena, incluido lo congelado.
    #[test]
    fn la_lista_de_activos_incluye_lo_congelado() {
        let mut app = App::new();
        app.entradas.push(entrada_de_prueba("ambiente", LoopMode::Infinite, true));
        app.entradas.push(entrada_de_prueba("trueno", LoopMode::None, false));
        app.eventos.push(evento_de_prueba("escena", BUCLE_INFINITO, true));

        app.pausar_todo();
        let activos = app.activos();

        assert_eq!(activos.len(), 2, "el que no suena no sale");
        assert!(activos.iter().all(|a| a.pausado), "y se ve que están en pausa");
        assert!(activos.iter().any(|a| a.bucle), "el ambiente va en bucle");
        assert!(matches!(activos[0].quien, Objeto::Audio(0)));
        assert!(matches!(activos[1].quien, Objeto::Evento(0)));
    }

    /// El panel de reproductor crece con lo que suena y nunca se come la lista.
    #[test]
    fn el_alto_del_reproductor_crece_y_tiene_tope() {
        let mut app = App::new();
        // Una ventana de las normales: manda el tope del propio panel.
        const VENTANA: f32 = 1080.0;
        assert_eq!(app.alto_transporte(VENTANA), ALTO_MANDOS, "en silencio, lo mínimo");

        app.entradas.push(entrada_de_prueba("uno", LoopMode::Infinite, true));
        let con_uno = app.alto_transporte(VENTANA);
        app.entradas.push(entrada_de_prueba("dos", LoopMode::Infinite, true));
        assert!(app.alto_transporte(VENTANA) > con_uno, "cada pista añade su fila");

        // Muchas pistas a la vez: el panel se planta y la lista hace scroll,
        // porque si no se comería la lista central.
        for i in 0..20 {
            app.entradas.push(entrada_de_prueba(&format!("más {i}"), LoopMode::Infinite, true));
        }
        assert_eq!(app.alto_transporte(VENTANA), ALTO_TRANSPORTE_MAX);

        // En una pantalla baja manda la ventana: un tercio, y ni un píxel más.
        assert_eq!(app.alto_transporte(600.0), 200.0);
        assert!(app.alto_transporte(600.0) < ALTO_TRANSPORTE_MAX);
        // Y por muy pequeña que sea, los mandos caben: el panel nunca baja del
        // mínimo, que es lo que evita que `clamp` reciba un rango invertido.
        assert_eq!(app.alto_transporte(120.0), ALTO_MANDOS);
    }

    /// La zona de reproductor se pinta entera sin reventar.
    ///
    /// Es un test de humo: no juzga el aspecto —eso hay que verlo—, pero sí
    /// recorre el camino de pintado con datos de verdad (un bucle sonando, un
    /// efecto, un evento, y luego todo congelado) y comprueba que no se sale de
    /// ningún índice ni peta al dibujar. Es lo único que toca ese código: un
    /// panel que revienta al pintarse deja la ventana en negro, y eso no lo ve
    /// ningún otro test.
    #[test]
    fn la_zona_de_reproductor_se_pinta_entera() {
        let mut app = App::new();
        app.entradas.push(entrada_de_prueba("ambiente", LoopMode::Infinite, true));
        app.entradas.push(entrada_de_prueba("trueno", LoopMode::None, false));
        app.eventos.push(evento_de_prueba("escena", BUCLE_INFINITO, true));

        egui::__run_test_ui(|ui| app.transporte(ui));

        // Y la otra rama de cada fila: con la pista congelada, que pinta la
        // barra en ámbar y cambia el botón de pausa por el de seguir.
        app.pausar_todo();
        egui::__run_test_ui(|ui| app.transporte(ui));
    }

    /// Los mandos caben en el panel: ni se recortan ni se cuelan por encima.
    ///
    /// Es el fallo que se vio en pantalla. Con las tres etiquetas de texto, la
    /// fila de mandos pedía más alto del que el panel tenía, y un panel
    /// `exact_size` al que le falta un píxel no avisa: recorta. Y encima
    /// engaña, porque egui reancla el panel a su borde de abajo cuando el
    /// contenido no cabe, le deja el hueco de más al panel central, y el panel
    /// central —que se pinta después— tapa la mitad de arriba de los botones.
    ///
    /// El test mide las dos mitades del síntoma sobre un egui de verdad, no a
    /// ojo: que el contenido quepa en el panel, y que el borde de arriba del
    /// panel sea el que toca (si se reancla, baja).
    #[test]
    fn los_mandos_caben_en_el_panel() {
        const VENTANA: f32 = 720.0;
        const BARRA: f32 = 46.0;

        // Las dos formas que tiene el panel de llenarse: el aviso de "nada
        // sonando" cuando está vacío, y la lista cuando hay pistas.
        for pistas in [0, 1, 3] {
            let mut app = App::new();
            for i in 0..pistas {
                app.entradas.push(entrada_de_prueba(
                    &format!("pista {i}"),
                    LoopMode::Infinite,
                    true,
                ));
            }

            let ctx = egui::Context::default();
            aplicar_paleta(&ctx);
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(1280.0, VENTANA),
                )),
                ..Default::default()
            };

            let mut contenido = 0.0_f32;
            let mut hueco = 0.0_f32;
            let mut borde = 0.0_f32;
            let mut alto = 0.0_f32;
            // Dos pasadas: la primera deja el estado de los paneles, y con él
            // se mide igual que en la ventana de verdad.
            for _ in 0..2 {
                let mut salida = ctx.run_ui(raw.clone(), |ui| {
                    egui::Panel::top("barra").exact_size(BARRA).show(ui, |_ui| {});
                    alto = app.alto_transporte(ui.available_height());
                    let r = egui::Panel::bottom("transporte")
                        .exact_size(alto)
                        .show(ui, |ui| {
                            let arriba = ui.max_rect().top();
                            hueco = ui.max_rect().height();
                            app.transporte(ui);
                            contenido = ui.min_rect().bottom() - arriba;
                        });
                    borde = r.response.rect.top();
                });
                salida.textures_delta.clear();
            }

            assert!(
                contenido <= hueco,
                "con {pistas} pistas, el contenido ({contenido:.1}) no cabe en el hueco del panel ({hueco:.1}): se recorta"
            );
            assert_eq!(
                borde,
                VENTANA - alto,
                "con {pistas} pistas el panel se ha reanclado hacia abajo: el central le pintará encima"
            );
        }
    }

    /// Una rampa plana se avisa; una de verdad, no.
    ///
    /// Es la trampa que produce el síntoma exacto que reportó el usuario: con
    /// los dos extremos iguales el panel sigue diciendo "con fade" mientras el
    /// audio se coloca de golpe. El aviso es lo único que lo delata.
    #[test]
    fn la_rampa_plana_se_avisa() {
        // Rampas de verdad: ni palabra.
        assert!(aviso_de_rampa_plana(0, 100).is_none());
        assert!(aviso_de_rampa_plana(100, 0).is_none());
        assert!(aviso_de_rampa_plana(20, 60).is_none());
        assert!(aviso_de_rampa_plana(60, 20).is_none());

        // Planas: aviso, y distinguiendo el silencio de la entrada de golpe.
        let golpe = aviso_de_rampa_plana(100, 100).expect("100→100 no es una rampa");
        assert!(golpe.contains("de golpe"), "{golpe}");
        assert!(aviso_de_rampa_plana(60, 60).is_some(), "60→60 tampoco es rampa");
        let mudo = aviso_de_rampa_plana(0, 0).expect("0→0 no suena");
        assert!(mudo.contains("no suena"), "{mudo}");
    }

    /// El botón de salir obedece a la salida configurada en el inspector.
    ///
    /// Es `FR-04`, que estaba sin implementar: la sección "Cómo sale" se podía
    /// configurar entera y **no la leía nadie**, así que poner "Sale con fade
    /// out" y que el audio se cortara igual era lo normal.
    #[test]
    fn la_salida_obedece_lo_que_diga_el_inspector() {
        let mut app = App::new();
        // Sin tocar nada, la salida por defecto es cortar ya.
        app.entradas.push(entrada_de_prueba("ambiente", LoopMode::Infinite, true));
        app.aplicar_accion(Objeto::Audio(0), Accion::Parar);
        assert_eq!(
            app.entradas[0].estado(),
            Some(TrackState::Stopped),
            "por defecto corta"
        );

        // Con "Sale con fade out", se le pide el fade y el corte al terminar,
        // en vez del corte seco.
        let mut con_fade = entrada_de_prueba("musica", LoopMode::Infinite, true);
        con_fade.spec.exit =
            ExitMode::FadeOut { duration: Duration::from_secs(2), curve: Curve::Linear };
        app.entradas.push(con_fade);

        app.aplicar_accion(Objeto::Audio(1), Accion::Parar);

        assert_eq!(
            app.entradas[1].estado(),
            Some(TrackState::FadingOut),
            "debe estar bajando, no cortada"
        );
        assert!(app.entradas[1].sonando(), "y sigue sonando mientras baja");
    }

    /// El botón dice lo que va a hacer, no siempre lo mismo.
    ///
    /// Una pista con fade out tarda en irse: eso hay que saberlo **antes** de
    /// pulsar, no después.
    #[test]
    fn el_boton_de_salir_se_llama_segun_lo_que_hace() {
        let mut e = entrada_de_prueba("x", LoopMode::Infinite, true);
        assert_eq!(e.rotulo_salida(), "PARAR", "sin salida configurada, corta");
        assert!(e.ayuda_salida().contains("corta ya"));

        e.spec.exit = ExitMode::Hit;
        assert_eq!(e.rotulo_salida(), "CORTA");

        e.spec.exit =
            ExitMode::FadeOut { duration: Duration::from_secs(2), curve: Curve::Linear };
        assert_eq!(e.rotulo_salida(), "SALIR");
        assert!(
            e.ayuda_salida().contains("2.00 s"),
            "el tooltip dice cuánto tarda: {}",
            e.ayuda_salida()
        );
    }

    /// Un crossfade baja lo que esté sonando, **venga de donde venga**.
    ///
    /// El bug: `bajar_lo_que_suena` sólo miraba `self.entradas` y se saltaba
    /// `self.eventos`. Así, si el audio que sonaba fue lanzado por un evento
    /// (un FadeIn, por ejemplo), el crossfade no lo bajaba: el ambiente seguía
    /// sonando encima del cambio de escena, y no se detenía al llegar a 0 %.
    /// Es justo el "no pasa" que reportó el usuario.
    #[test]
    fn el_crossfade_baja_las_pistas_de_los_eventos() {
        let mut app = App::new();

        // Un ambiente lanzado por un FadeIn, sonando.
        app.eventos.push(evento_de_prueba("Ambiente", BUCLE_INFINITO, true));

        // Un crossfade que entra con otro audio distinto y apaga lo que suena.
        let mut cross = Evento::nuevo(TipoEvento::Crossfade);
        cross.nombre = "Cambio".to_string();
        cross.pistas[0] = PistaEvento::vacia(0, 100)
            .con_audio("Viento", AudioRef { file_name: "viento.wav".to_string(), ..Default::default() });
        cross.salida_pct = 0; // apaga lo que suena
        app.eventos.push(EventoVivo::nuevo(cross));

        let antes = app.registro.len();
        app.ir_evento(1); // lanza el crossfade

        let dicho = dicho_desde(&app, antes);

        // La pista del FadeIn debe haber recibido stop_after: su estado es
        // FadingOut, no Playing. Antes del fix seguía en Playing.
        let pista = app.eventos[0].pistas[0]
            .as_ref()
            .expect("la pista del FadeIn sigue montada");
        assert_eq!(
            pista.state(),
            TrackState::FadingOut,
            "el ambiente del evento debe estar saliendo con fade, no seguir sonando"
        );

        // Y el registro lo cuenta.
        assert!(
            dicho.contains("1 pista"),
            "debe contar 1 pista bajada: {dicho}"
        );
    }

    /// Y también las de la lista principal: un crossfade baja las pistas que
    /// están sonando como filas, no sólo las de los eventos.
    #[test]
    fn el_crossfade_tambien_baja_las_pistas_de_la_lista() {
        let mut app = App::new();

        // Un audio sonando desde la lista principal.
        app.entradas.push(entrada_de_prueba("ambiente", LoopMode::Infinite, true));

        // Un crossfade que entra con otro audio distinto y apaga lo que suena.
        let mut cross = Evento::nuevo(TipoEvento::Crossfade);
        cross.nombre = "Cambio".to_string();
        cross.pistas[0] = PistaEvento::vacia(0, 100)
            .con_audio("Viento", AudioRef { file_name: "viento.wav".to_string(), ..Default::default() });
        cross.salida_pct = 0;
        app.eventos.push(EventoVivo::nuevo(cross));

        let antes = app.registro.len();
        app.ir_evento(0);

        let dicho = dicho_desde(&app, antes);

        assert_eq!(
            app.entradas[0].estado(),
            Some(TrackState::FadingOut),
            "la pista de la lista debe estar saliendo con fade"
        );
        assert!(
            dicho.contains("1 pista"),
            "debe contar 1 pista bajada: {dicho}"
        );
    }

    /// Un fade out de evento también tiene que bajar las pistas de eventos,
    /// no sólo las de la lista. Es el mismo código (`bajar_lo_que_suena`).
    #[test]
    fn el_fade_out_de_evento_baja_las_pistas_de_los_eventos() {
        let mut app = App::new();

        // Dos eventos sonando: un bucle y un efecto.
        app.eventos.push(evento_de_prueba("Ambiente", BUCLE_INFINITO, true));
        app.eventos.push(evento_de_prueba("Trueno", 1, true));

        // Un fade out que apaga lo que suena.
        let mut fout = Evento::nuevo(TipoEvento::FadeOut);
        fout.nombre = "Telón".to_string();
        fout.salida_pct = 0;
        app.eventos.push(EventoVivo::nuevo(fout));

        let antes = app.registro.len();
        app.ir_evento(2); // lanza el fade out

        let dicho = dicho_desde(&app, antes);

        // Las dos pistas de eventos deben estar saliendo con fade.
        for (i, nombre) in ["Ambiente", "Trueno"].iter().enumerate() {
            let pista = app.eventos[i].pistas[0]
                .as_ref()
                .unwrap_or_else(|| panic!("la pista de {nombre} sigue montada"));
            assert_eq!(
                pista.state(),
                TrackState::FadingOut,
                "la pista de {nombre} debe estar saliendo con fade"
            );
        }
        assert!(
            dicho.contains("2 pista"),
            "debe contar 2 pistas bajadas: {dicho}"
        );
    }
}
