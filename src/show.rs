//! Modo Función y *showtime* (hito SHOW).
//!
//! Todo lo de aquí es **lógica pura**: no toca la interfaz ni la tarjeta de
//! sonido. Así se puede testear lo que de verdad importa —que una edición no
//! se cuele en función, o que la siguiente entrada arranca cuando toca— sin
//! abrir una ventana.

use crate::sesion::modelo::AutoFollow;

/// Fade de la parada de emergencia (T-SHOW-003).
///
/// 50 ms es lo bastante corto para que parezca un corte y lo bastante largo
/// para que no suene un clic: cortar en seco un PCM a mitad de ciclo se oye.
pub const FADE_EMERGENCIA_MS: u64 = 50;

/// Ventana para que dos Esc seguidos cuenten como "salir de Función".
pub const VENTANA_DOBLE_ESC_MS: u64 = 600;

// ---------------------------------------------------------------------------
// T-SHOW-001 — modo y bloqueo de edición
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModoShow {
    /// Se puede editar.
    #[default]
    Diseno,
    /// Sólo se opera: nada de edición.
    Funcion,
}

/// Controla si se puede editar y gestiona el "doble Esc para salir".
///
/// El doble Esc evita que el operador salga del modo Función por un toque
/// accidental a mitad de función: hay que querer salir.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bloqueo {
    modo: ModoShow,
    /// Milisegundos (monótonos) del último Esc. `None` si no hubo.
    ultimo_esc_ms: Option<u64>,
}

impl Bloqueo {
    pub fn nuevo() -> Self {
        Self::default()
    }

    pub fn modo(&self) -> ModoShow {
        self.modo
    }

    /// ¿Se puede editar la sesión ahora mismo?
    pub fn permite_editar(&self) -> bool {
        self.modo == ModoShow::Diseno
    }

    pub fn en_funcion(&self) -> bool {
        self.modo == ModoShow::Funcion
    }

    pub fn entrar_en_funcion(&mut self) {
        self.modo = ModoShow::Funcion;
        self.ultimo_esc_ms = None;
    }

    pub fn volver_a_diseno(&mut self) {
        self.modo = ModoShow::Diseno;
        self.ultimo_esc_ms = None;
    }

    /// Registra una pulsación de Esc. Devuelve `true` si hay que salir de
    /// Función (es decir, si es la segunda dentro de la ventana).
    ///
    /// Fuera de Función un Esc no hace nada: no hay nada que desbloquear.
    pub fn esc(&mut self, ahora_ms: u64) -> bool {
        if self.modo != ModoShow::Funcion {
            self.ultimo_esc_ms = None;
            return false;
        }

        let es_segundo = self
            .ultimo_esc_ms
            .is_some_and(|t| ahora_ms.saturating_sub(t) <= VENTANA_DOBLE_ESC_MS);

        if es_segundo {
            self.volver_a_diseno();
            true
        } else {
            self.ultimo_esc_ms = Some(ahora_ms);
            false
        }
    }
}

// ---------------------------------------------------------------------------
// T-SHOW-002 — auto-follow
// ---------------------------------------------------------------------------

/// Lo que hay programado: disparar la entrada `indice`.
///
/// `en_ms` es el instante (monótono) en el que toca; si es `None`, toca cuando
/// la pista actual termine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cita {
    pub indice: usize,
    pub en_ms: Option<u64>,
}

/// Decide cuándo arranca la siguiente entrada.
#[derive(Clone, Copy, Debug, Default)]
pub struct Programador {
    cita: Option<Cita>,
}

impl Programador {
    pub fn nuevo() -> Self {
        Self::default()
    }

    /// Se llama justo al arrancar la entrada `i`.
    ///
    /// `hay_siguiente` es si existe una entrada `i + 1`: no tiene sentido
    /// programar nada si esta es la última.
    pub fn al_arrancar(
        &mut self,
        i: usize,
        follow: AutoFollow,
        ahora_ms: u64,
        hay_siguiente: bool,
    ) -> bool {
        if !hay_siguiente {
            self.cita = None;
            return false;
        }
        match follow {
            AutoFollow::None => {
                self.cita = None;
                false
            }
            AutoFollow::AfterMs(ms) => {
                self.cita = Some(Cita { indice: i + 1, en_ms: Some(ahora_ms + ms) });
                true
            }
            AutoFollow::WhenThisEnds => {
                self.cita = Some(Cita { indice: i + 1, en_ms: None });
                true
            }
        }
    }

    /// Programa la entrada `i` para que arranque a los `en_ms` (tiempo
    /// monótono). Se usa cuando el `on_previous` pide un retardo: la pista
    /// anterior está saliendo y la nueva espera su turno.
    ///
    /// Sobreescribe cualquier cita pendiente: si hay un auto-follow pendiente
    /// y el operador dispara una entrada con retardo, el auto-follow se
    /// reemplaza. Es lo predecible: lo último que pidió el operador gana.
    pub fn demorar_entrada(&mut self, i: usize, en_ms: u64) {
        self.cita = Some(Cita { indice: i, en_ms: Some(en_ms) });
    }

    /// Entrada que ya toca disparar por haber pasado su tiempo.
    pub fn vencida(&mut self, ahora_ms: u64) -> Option<usize> {
        match self.cita {
            Some(c) if c.en_ms.is_some_and(|ms| ahora_ms >= ms) => {
                self.cita = None;
                Some(c.indice)
            }
            _ => None,
        }
    }

    /// Entrada que toca disparar porque la pista terminó.
    pub fn al_terminar(&mut self) -> Option<usize> {
        match self.cita {
            Some(c) if c.en_ms.is_none() => {
                self.cita = None;
                Some(c.indice)
            }
            _ => None,
        }
    }

    /// Anula lo programado (la usa PARAR TODO y los cambios de selección).
    pub fn cancelar(&mut self) {
        self.cita = None;
    }

    pub fn pendiente(&self) -> Option<Cita> {
        self.cita
    }
}

// ---------------------------------------------------------------------------
// T-SHOW-005 — aviso de MP3 en loop
// ---------------------------------------------------------------------------

/// Aviso si una entrada con loop usa MP3.
///
/// El MP3 guarda padding del codificador al principio y al final, así que al
/// encadenar vueltas aparece un salto de unos milisegundos. En un ambiente
/// (loop infinito) se nota; en un efecto de una sola vez no.
pub fn aviso_loop_mp3(nombre_archivo: &str, en_loop: bool) -> Option<&'static str> {
    if !en_loop {
        return None;
    }
    let ext = nombre_archivo
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "mp3" => Some("MP3 en loop: puede dar un salto al repetir. Mejor WAV o FLAC."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- T-SHOW-001 --------------------------------------------------------

    #[test]
    fn en_diseno_se_puede_editar_y_en_funcion_no() {
        let mut b = Bloqueo::nuevo();
        assert!(b.permite_editar(), "en Diseño se edita");
        b.entrar_en_funcion();
        assert!(!b.permite_editar(), "en Función NO se edita");
        assert!(b.en_funcion());
    }

    #[test]
    fn un_solo_esc_no_sale_de_funcion() {
        let mut b = Bloqueo::nuevo();
        b.entrar_en_funcion();
        assert!(!b.esc(1000), "el primer Esc no debe sacar de Función");
        assert!(b.en_funcion(), "seguimos en Función");
    }

    #[test]
    fn doble_esc_sale_de_funcion() {
        let mut b = Bloqueo::nuevo();
        b.entrar_en_funcion();
        assert!(!b.esc(1000));
        assert!(b.esc(1200), "el segundo Esc dentro de la ventana debe salir");
        assert!(!b.en_funcion());
        assert!(b.permite_editar());
    }

    #[test]
    fn dos_esc_separados_por_mucho_tiempo_no_cuentan() {
        let mut b = Bloqueo::nuevo();
        b.entrar_en_funcion();
        assert!(!b.esc(1000));
        assert!(!b.esc(1000 + VENTANA_DOBLE_ESC_MS + 1), "fuera de ventana no cuenta");
        assert!(b.en_funcion());
    }

    #[test]
    fn fuera_de_funcion_el_esc_no_hace_nada() {
        let mut b = Bloqueo::nuevo();
        assert!(!b.esc(100));
        assert!(!b.esc(200));
        assert!(b.permite_editar());
    }

    // --- T-SHOW-002 --------------------------------------------------------

    #[test]
    fn after_ms_dispara_cuando_vence() {
        let mut p = Programador::nuevo();
        assert!(p.al_arrancar(0, AutoFollow::AfterMs(1000), 500, true));
        assert_eq!(p.pendiente(), Some(Cita { indice: 1, en_ms: Some(1500) }));

        assert_eq!(p.vencida(1499), None, "aún no toca");
        assert_eq!(p.vencida(1500), Some(1), "justo en el instante, toca");
        assert_eq!(p.vencida(1600), None, "ya se consumió");
    }

    #[test]
    fn when_this_ends_dispara_al_terminar() {
        let mut p = Programador::nuevo();
        assert!(p.al_arrancar(2, AutoFollow::WhenThisEnds, 0, true));
        assert_eq!(p.pendiente(), Some(Cita { indice: 3, en_ms: None }));

        // El tiempo no importa: no vence por reloj.
        assert_eq!(p.vencida(999_999), None);
        assert_eq!(p.al_terminar(), Some(3));
        assert_eq!(p.al_terminar(), None, "sólo una vez");
    }

    #[test]
    fn sin_auto_follow_no_se_programa_nada() {
        let mut p = Programador::nuevo();
        assert!(!p.al_arrancar(0, AutoFollow::None, 0, true));
        assert_eq!(p.pendiente(), None);
        assert_eq!(p.vencida(100_000), None);
    }

    #[test]
    fn la_ultima_entrada_no_programa_nada() {
        let mut p = Programador::nuevo();
        assert!(!p.al_arrancar(4, AutoFollow::AfterMs(100), 0, false));
        assert_eq!(p.pendiente(), None, "no hay entrada 5");
    }

    #[test]
    fn cancelar_anula_lo_programado() {
        let mut p = Programador::nuevo();
        p.al_arrancar(0, AutoFollow::AfterMs(100), 0, true);
        p.cancelar();
        assert_eq!(p.vencida(10_000), None);
    }

    #[test]
    fn demorar_entrada_agenda_la_misma_entrada_no_la_siguiente() {
        let mut p = Programador::nuevo();
        // La receta "B entra X segundos después de que A haya terminado su
        // fade out": agendamos B, no la siguiente. Antes y después de la
        // demora B es la cita, así que el tick puede dispararla sin confusión
        // con un auto-follow.
        p.demorar_entrada(3, 5_000);
        assert_eq!(p.pendiente(), Some(Cita { indice: 3, en_ms: Some(5_000) }));

        assert_eq!(p.vencida(4_999), None, "aún no toca");
        assert_eq!(p.vencida(5_000), Some(3), "justo en el instante, dispara B");
        assert_eq!(p.vencida(6_000), None, "ya se consumió");
    }

    #[test]
    fn demorar_entrada_pisa_un_auto_follow_anterior() {
        // El operador primero confió en el auto-follow y luego corrigió a mano
        // con un GO demorado: lo último que pidió gana.
        let mut p = Programador::nuevo();
        p.al_arrancar(0, AutoFollow::WhenThisEnds, 100, true);
        p.demorar_entrada(1, 1_500);
        assert_eq!(p.pendiente(), Some(Cita { indice: 1, en_ms: Some(1_500) }));
    }

    #[test]
    fn al_arrancar_otra_entrada_se_reprograma() {
        let mut p = Programador::nuevo();
        p.al_arrancar(0, AutoFollow::AfterMs(1000), 0, true);
        p.al_arrancar(1, AutoFollow::WhenThisEnds, 10, true);
        assert_eq!(p.pendiente(), Some(Cita { indice: 2, en_ms: None }));
    }

    // --- T-SHOW-005 --------------------------------------------------------

    #[test]
    fn avisa_solo_si_mp3_y_en_loop() {
        assert!(aviso_loop_mp3("ambiente.mp3", true).is_some());
        assert!(aviso_loop_mp3("ambiente.MP3", true).is_some(), "sin distinguir mayúsculas");
        assert!(aviso_loop_mp3("ambiente.wav", true).is_none());
        assert!(aviso_loop_mp3("ambiente.flac", true).is_none());
        assert!(aviso_loop_mp3("trueno.mp3", false).is_none(), "sin loop no avisa");
    }

    #[test]
    fn el_aviso_explica_el_por_que() {
        let aviso = aviso_loop_mp3("x.mp3", true).expect("debe avisar");
        assert!(aviso.contains("WAV") || aviso.contains("FLAC"), "debe sugerir alternativa: {aviso}");
    }
}
