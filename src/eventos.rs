//! Eventos predefinidos (hito EVT).
//!
//! Un **audio** es un archivo suelto: "el trueno", "el ambiente del salón".
//! Un **evento** es una escena ya montada con ese audio: "el ambiente sube de
//! 0 a 80 % en 8 segundos y se queda", "la música baja mientras entra el
//! viento", "una explosión de golpe y ya está".
//!
//! Los eventos se guardan en la sesión y se reutilizan: se crean una vez y se
//! lanzan desde la lista, desde la franja de pads o con una tecla F. Y se
//! editan **sin rehacerlos**: cambiar la duración de una escena es mover un
//! número, no volver a elegir tipo y audios.
//!
//! Espejo de `Docs/04-modelo-de-datos.md` §5. Sin floats: los porcentajes son
//! `u8` y las duraciones, milisegundos (`Docs/12`).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::engine::model::{CueSpec, Curve, Entrance, LoopMode, MilliDb, OnPrevious};
use crate::sesion::modelo::AudioRef;

/// Valor de `bucle` que significa "para siempre".
///
/// Es `-1` y no un enum con datos por dos razones: es lo que el usuario pidió
/// explícitamente, y en el JSON se lee de un vistazo (`"bucle": -1`).
pub const BUCLE_INFINITO: i32 = -1;

/// Duración por defecto de un evento recién creado.
pub const DURACION_POR_DEFECTO_MS: u64 = 3000;

// ---------------------------------------------------------------------------
// Tipo de evento
// ---------------------------------------------------------------------------

/// Qué hace un evento. Se elige **primero**, porque decide cuántos audios
/// necesita y qué rampa le corresponde a cada uno.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TipoEvento {
    /// Un audio que sube: aparece poco a poco.
    #[default]
    FadeIn,
    /// Baja **lo que está sonando**. No se elige audio: el que se apaga es el
    /// que suena en ese momento.
    FadeOut,
    /// Intercambio: entra un audio nuevo mientras se va el que está sonando.
    /// Sólo se elige el que entra.
    Crossfade,
    /// Un audio de golpe, sin fade, una sola vez. Para un efecto puntual
    /// (una explosión, un rayo, un portazo): se monta **encima** de lo que
    /// ya esté sonando, no lo sustituye.
    Golpe,
}

impl TipoEvento {
    pub const TODOS: [TipoEvento; 4] = [
        TipoEvento::FadeIn,
        TipoEvento::FadeOut,
        TipoEvento::Crossfade,
        TipoEvento::Golpe,
    ];

    pub fn rotulo(self) -> &'static str {
        match self {
            TipoEvento::FadeIn => "Fade in",
            TipoEvento::FadeOut => "Fade out",
            TipoEvento::Crossfade => "Crossfade",
            TipoEvento::Golpe => "Disparo único",
        }
    }

    /// Una línea que dice qué hace, para el paso 1 del asistente.
    pub fn descripcion(self) -> &'static str {
        match self {
            TipoEvento::FadeIn => {
                "Eliges el audio que entra y aparece poco a poco, desde el volumen \
                 que quieras hasta el que quieras."
            }
            TipoEvento::FadeOut => {
                "No hay que elegir audio: baja el que esté sonando en ese momento. \
                 Es la forma de quitarse de encima una música sin buscar su ficha."
            }
            TipoEvento::Crossfade => {
                "Eliges sólo el audio que entra. El que está sonando se va a la vez, \
                 y no se elige: es el que suena. El cambio de escena de siempre."
            }
            TipoEvento::Golpe => {
                "Un audio suena de golpe, sin fade, una sola vez. Para una explosión, \
                 un rayo o un portazo: se monta sobre lo que ya esté sonando."
            }
        }
    }

    /// Cuántos audios hay que **elegir** para este evento.
    ///
    /// El fade out no pide ninguno: actúa sobre el audio que ya está sonando.
    /// Y el crossfade pide **uno**, el que entra: el que sale es, por
    /// definición, el que está sonando en ese momento.
    pub fn cuantos_audios(self) -> usize {
        match self {
            TipoEvento::FadeOut => 0,
            _ => 1,
        }
    }

    /// true si el evento actúa sobre lo que ya está sonando.
    ///
    /// En el crossfade el audio de base **no se elige**: es el que suena. Por
    /// eso no se puede tocar ni su identidad ni su volumen inicial — su punto
    /// de partida es "donde esté sonando ahora", y eso no lo sabe nadie hasta
    /// que se lanza el evento.
    pub fn actua_sobre_lo_que_suena(self) -> bool {
        matches!(self, TipoEvento::FadeOut | TipoEvento::Crossfade)
    }

    /// Si el evento hace una rampa de volumen. El disparo único no: entra a
    /// pleno volumen desde la primera muestra.
    pub fn usa_fade(self) -> bool {
        !matches!(self, TipoEvento::Golpe)
    }

    /// Extremos de la rampa del audio que **entra**.
    ///
    /// Son los valores con los que nace el evento; después se pueden cambiar a
    /// cualquier par (40 → 70, 0 → 60…).
    ///
    /// El fade out no tiene rampa propia que configurar aquí: la suya es la del
    /// audio que ya suena, y se ajusta con `Evento::salida_pct`.
    pub fn extremos_por_defecto(self) -> (u8, u8) {
        match self {
            // Entra desde el silencio.
            TipoEvento::FadeIn | TipoEvento::Crossfade => (0, 100),
            // Sin fade: entra y se queda a pleno volumen.
            TipoEvento::Golpe => (100, 100),
            TipoEvento::FadeOut => (100, 0),
        }
    }
}

// ---------------------------------------------------------------------------
// Un audio dentro del evento
// ---------------------------------------------------------------------------

/// Un audio dentro de un evento, con su propia rampa.
///
/// Cada audio lleva sus extremos **por separado**: en un crossfade el primero
/// puede bajar de 100 a 20 mientras el segundo sube de 0 a 80, que es justo lo
/// que no se podía hacer cuando los extremos eran fijos.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PistaEvento {
    /// De qué audio se trata. Guarda el `fileName` para reencontrarlo al
    /// reabrir la obra, igual que una entrada normal (`Docs/04` §3).
    #[serde(default)]
    pub audio: AudioRef,
    /// Nombre para mostrar. Es el del audio, copiado al elegirlo.
    #[serde(default)]
    pub nombre: String,
    /// Volumen propio de este audio, antes de la rampa.
    #[serde(default, rename = "volumeDb", with = "crate::serde_util::db")]
    pub volumen: MilliDb,
    /// Volumen al empezar el evento, 0–100 %.
    #[serde(default)]
    pub desde_pct: u8,
    /// Volumen al terminar el evento, 0–100 %.
    #[serde(default = "crate::engine::model::cien")]
    pub hasta_pct: u8,
}

impl PistaEvento {
    pub fn vacia(desde_pct: u8, hasta_pct: u8) -> Self {
        Self { desde_pct, hasta_pct, ..Default::default() }
    }

    pub fn con_audio(mut self, nombre: impl Into<String>, audio: AudioRef) -> Self {
        self.nombre = nombre.into();
        self.audio = audio;
        self
    }

    /// true si este hueco todavía no tiene audio elegido.
    pub fn vacio(&self) -> bool {
        self.audio.file_name.is_empty()
    }
}

// ---------------------------------------------------------------------------
// El evento
// ---------------------------------------------------------------------------

/// Un evento predefinido: tipo, audios y cómo se mueve el volumen.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Evento {
    #[serde(default)]
    pub nombre: String,
    #[serde(default)]
    pub tipo: TipoEvento,
    /// El audio que **entra**, si el tipo pide uno.
    ///
    /// Contiene 0 o 1 elementos: `FadeOut` no pide ninguno porque actúa sobre
    /// lo que ya suena, y `Crossfade` pide sólo el que entra, porque el que
    /// sale es el que está sonando. Los huecos sin rellenar existen y se ven en
    /// el inspector, para poder elegir el audio sin volver a empezar.
    #[serde(default)]
    pub pistas: Vec<PistaEvento>,
    /// Volumen al que va **lo que ya está sonando**: el audio de base de un
    /// crossfade, o el que se apaga en un fade out.
    ///
    /// Es el único valor del lado que sale que se puede tocar, porque su punto
    /// de partida es "donde esté sonando ahora" y eso no se sabe hasta que se
    /// lanza el evento. Si vale `0`, la pista se corta al terminar la rampa.
    #[serde(default)]
    pub salida_pct: u8,
    /// Cuánto dura el evento, en milisegundos. Es la rampa entera: **éste** es
    /// el número que se mueve para acortar o alargar una escena.
    #[serde(default = "duracion_por_defecto")]
    pub duracion_ms: u64,
    #[serde(default)]
    pub curva: Curve,
    /// `-1` = infinito; `0` o `1` = una vez; `N` = N veces.
    #[serde(default = "bucle_por_defecto")]
    pub bucle: i32,
    /// Nota libre para el operador.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub nota: String,
    /// Tecla que lo dispara en modo Función (F1, F2…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tecla: Option<String>,
    /// true = aparece también en la franja de pads.
    #[serde(default)]
    pub pad: bool,
}

fn duracion_por_defecto() -> u64 {
    DURACION_POR_DEFECTO_MS
}

fn bucle_por_defecto() -> i32 {
    BUCLE_INFINITO
}

impl Evento {
    /// Evento nuevo del tipo dado, con los huecos de audio vacíos y los
    /// extremos que le tocan por defecto.
    pub fn nuevo(tipo: TipoEvento) -> Self {
        let (desde, hasta) = tipo.extremos_por_defecto();
        let pistas = (0..tipo.cuantos_audios())
            .map(|_| PistaEvento::vacia(desde, hasta))
            .collect();
        Self {
            nombre: tipo.rotulo().to_string(),
            tipo,
            pistas,
            // Por defecto lo que suena se apaga: es lo que se espera de un
            // cambio de escena y de un fade out.
            salida_pct: 0,
            duracion_ms: DURACION_POR_DEFECTO_MS,
            // Lineal: es la que el usuario pidió como predeterminada.
            curva: Curve::Linear,
            bucle: BUCLE_INFINITO,
            nota: String::new(),
            tecla: None,
            pad: false,
        }
    }

    pub fn duracion(&self) -> Duration {
        Duration::from_millis(self.duracion_ms)
    }

    /// true si el evento suena para siempre.
    pub fn infinito(&self) -> bool {
        self.bucle < 0
    }

    /// Cómo se repite, traducido a lo que entiende el motor.
    ///
    /// `0` se trata como "una vez" y no como "cero veces": nadie crea un
    /// evento para que no suene, y un 0 escrito a mano en el JSON no debe
    /// dejar un evento mudo sin explicación.
    pub fn loop_mode(&self) -> LoopMode {
        match self.bucle {
            n if n < 0 => LoopMode::Infinite,
            0 | 1 => LoopMode::None,
            n => LoopMode::Count(n as u32),
        }
    }

    /// true si está listo para lanzarse: tiene los audios que su tipo pide.
    ///
    /// Un fade out no pide ninguno —actúa sobre lo que suene—, así que está
    /// completo desde que se crea. Un evento incompleto se ve en la lista, pero
    /// no se puede lanzar.
    pub fn completo(&self) -> bool {
        self.pistas.len() == self.tipo.cuantos_audios()
            && self.pistas.iter().all(|p| !p.vacio())
    }

    /// true si al terminar la rampa la pista que sale se apaga del todo.
    pub fn apaga_al_salir(&self) -> bool {
        self.salida_pct == 0
    }

    /// Cuántos huecos quedan por rellenar.
    pub fn huecos_sin_audio(&self) -> usize {
        self.pistas.iter().filter(|p| p.vacio()).count()
    }

    /// La configuración de sonido del audio que **entra**.
    ///
    /// Devuelve `None` cuando el tipo no tiene audio de entrada (fade out) o
    /// cuando todavía no se ha elegido.
    ///
    /// El evento **no** toca lo que ya esté sonando por su cuenta
    /// (`onPrevious` queda en `Keep`): de eso se encarga `salida_pct`, que es
    /// explícito y sólo afecta a la rampa de salida. Si además apagara cosas
    /// por su cuenta, lanzar un evento en medio de una función cortaría algo
    /// que nadie le mandó cortar.
    pub fn spec_entrada(&self) -> Option<CueSpec> {
        let pista = self.pistas.first()?;
        if pista.vacio() {
            return None;
        }

        let mut spec = CueSpec::simple();
        spec.volume = pista.volumen;
        spec.loop_mode = self.loop_mode();
        spec.on_previous = OnPrevious::Keep;

        if self.tipo.usa_fade() {
            spec.entrance = Entrance::FadeIn {
                duration: self.duracion(),
                curve: self.curva,
                from_percent: pista.desde_pct,
                to_percent: pista.hasta_pct,
            };
            // Si la rampa acaba en silencio, la pista se corta sola al llegar:
            // si no, seguiría sonando (en silencio) hasta el final del archivo,
            // ocupando una pista del mezclador para nada. El corte cae cuando la
            // ganancia ya es 0, así que no se oye.
            if pista.hasta_pct == 0 {
                spec.stop_after = Some(self.duracion());
            }
        } else {
            spec.entrance = Entrance::Hit;
            // Un disparo único suena una vez: un efecto en bucle deja de ser
            // un efecto.
            spec.loop_mode = LoopMode::None;
        }

        Some(spec)
    }

    /// Resumen de una línea para la lista.
    pub fn resumen(&self) -> String {
        let mut partes = vec![self.tipo.rotulo().to_string()];

        match self.tipo {
            TipoEvento::FadeOut => {
                // No hay audio que elegir: se resume la rampa que se le aplica
                // a lo que esté sonando.
                partes.push(format!("lo que suena → {}%", self.salida_pct));
                partes.push(formatear(self.duracion()));
            }
            TipoEvento::Crossfade => {
                // "entra" es el único audio que se elige; el otro es el que suena.
                let entra = self
                    .pistas
                    .first()
                    .map(|p| format!("{}→{}%", p.desde_pct, p.hasta_pct))
                    .unwrap_or_else(|| "sin audio".to_string());
                partes.push(format!("entra {entra}"));
                partes.push(format!("sale → {}%", self.salida_pct));
                partes.push(formatear(self.duracion()));
            }
            _ if self.tipo.usa_fade() => {
                let rampa = self
                    .pistas
                    .first()
                    .map(|p| format!("{}→{}%", p.desde_pct, p.hasta_pct))
                    .unwrap_or_else(|| "sin audio".to_string());
                partes.push(rampa);
                partes.push(formatear(self.duracion()));
            }
            _ => partes.push("sin fade".to_string()),
        }

        if self.tipo.usa_fade() {
            if self.infinito() {
                partes.push("loop ∞".to_string());
            } else if self.bucle > 1 {
                partes.push(format!("×{}", self.bucle));
            }
        }

        partes.join(" · ")
    }
}

/// Segundos con dos decimales, o milisegundos si es corto.
fn formatear(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.2} s", d.as_secs_f32())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(nombre: &str) -> AudioRef {
        AudioRef { file_name: nombre.to_string(), ..Default::default() }
    }

    fn evento_crossfade() -> Evento {
        let mut e = Evento::nuevo(TipoEvento::Crossfade);
        e.nombre = "Cambio de escena".to_string();
        // Sólo se elige el que entra. El que sale es el que está sonando.
        e.pistas[0] = PistaEvento::vacia(0, 100).con_audio("Viento", audio("viento.wav"));
        e.duracion_ms = 8000;
        e.bucle = 3;
        e
    }

    #[test]
    fn solo_tres_tipos_piden_audio() {
        // Entrada o intercambio. El fade out actúa sobre lo que suena.
        assert_eq!(TipoEvento::FadeIn.cuantos_audios(), 1, "fade in: el que entra");
        assert_eq!(TipoEvento::Crossfade.cuantos_audios(), 1, "crossfade: el que entra");
        assert_eq!(TipoEvento::Golpe.cuantos_audios(), 1, "disparo único: el que suena");
        assert_eq!(TipoEvento::FadeOut.cuantos_audios(), 0, "fade out: el que ya suena");

        assert!(!TipoEvento::FadeIn.actua_sobre_lo_que_suena());
        assert!(!TipoEvento::Golpe.actua_sobre_lo_que_suena());
        assert!(TipoEvento::FadeOut.actua_sobre_lo_que_suena());
        assert!(TipoEvento::Crossfade.actua_sobre_lo_que_suena());
    }

    #[test]
    fn los_extremos_por_defecto_son_los_de_sentido_comun() {
        // El que entra, desde el silencio.
        assert_eq!(TipoEvento::FadeIn.extremos_por_defecto(), (0, 100));
        assert_eq!(TipoEvento::Crossfade.extremos_por_defecto(), (0, 100));
        // El disparo único no tiene rampa.
        assert_eq!(TipoEvento::Golpe.extremos_por_defecto(), (100, 100));
    }

    #[test]
    fn un_fade_out_no_pide_audio_y_esta_completo_desde_que_nace() {
        let e = Evento::nuevo(TipoEvento::FadeOut);
        assert!(e.pistas.is_empty(), "no hay audio que elegir");
        assert!(e.completo(), "actúa sobre lo que suene: no le falta nada");
        assert!(e.spec_entrada().is_none(), "no tiene audio de entrada");
        // Y se apaga lo que suene por defecto.
        assert_eq!(e.salida_pct, 0);
        assert!(e.apaga_al_salir());
    }

    #[test]
    fn solo_el_disparo_unico_no_lleva_fade() {
        assert!(TipoEvento::FadeIn.usa_fade());
        assert!(TipoEvento::FadeOut.usa_fade());
        assert!(TipoEvento::Crossfade.usa_fade());
        assert!(!TipoEvento::Golpe.usa_fade());
    }

    #[test]
    fn un_evento_nuevo_viene_en_infinito_y_con_el_hueco_vacio() {
        let e = Evento::nuevo(TipoEvento::Crossfade);
        assert_eq!(e.bucle, BUCLE_INFINITO);
        assert!(e.infinito());
        assert_eq!(e.duracion_ms, DURACION_POR_DEFECTO_MS);
        assert_eq!(e.pistas.len(), 1, "sólo el que entra");
        assert_eq!(e.huecos_sin_audio(), 1);
        assert!(!e.completo(), "sin el audio que entra no se puede lanzar");
    }

    #[test]
    fn el_evento_round_trip_por_json() {
        let e = evento_crossfade();
        let json = serde_json::to_string(&e).unwrap();
        for campo in ["\"crossfade\"", "\"duracionMs\"", "\"desdePct\"", "\"hastaPct\"", "\"bucle\""] {
            assert!(json.contains(campo), "falta {campo} en el JSON:\n{json}");
        }
        let vuelta: Evento = serde_json::from_str(&json).unwrap();
        assert_eq!(vuelta, e, "el round-trip no es exacto");
    }

    #[test]
    fn un_evento_escrito_a_mano_se_abre_igual() {
        // Escrito a mano, con lo mínimo: el resto queda en su defecto. Es la
        // regla de `Docs/04` §3, "la sesión nunca se abre rota".
        let json = r#"{"nombre":"Trueno","tipo":"golpe"}"#;
        let e: Evento = serde_json::from_str(json).expect("debe abrir igual");
        assert_eq!(e.tipo, TipoEvento::Golpe);
        assert_eq!(e.duracion_ms, DURACION_POR_DEFECTO_MS);
        assert_eq!(e.bucle, BUCLE_INFINITO);
    }

    #[test]
    fn el_bucle_se_traduce_a_lo_que_entiende_el_motor() {
        let mut e = Evento::nuevo(TipoEvento::FadeIn);
        e.bucle = BUCLE_INFINITO;
        assert_eq!(e.loop_mode(), LoopMode::Infinite);
        e.bucle = 0;
        assert_eq!(e.loop_mode(), LoopMode::None, "un 0 no debe dejar un evento mudo");
        e.bucle = 1;
        assert_eq!(e.loop_mode(), LoopMode::None);
        e.bucle = 4;
        assert_eq!(e.loop_mode(), LoopMode::Count(4));
    }

    #[test]
    fn el_crossfade_solo_elige_el_que_entra() {
        let e = evento_crossfade();

        // Sólo hay un audio: el que entra.
        assert_eq!(e.pistas.len(), 1);
        assert!(e.completo());

        let entra = e.spec_entrada().expect("el que entra");
        // El nuevo pasa de no sonar a sonar.
        assert_eq!(entra.extremos_de_entrada(), (0.0, 1.0));
        assert_eq!(entra.stop_after, None, "el que entra se queda sonando");

        // El que sale no se elige: se le fija sólo el volumen objetivo, y por
        // defecto se apaga del todo.
        assert_eq!(e.salida_pct, 0);
        assert!(e.apaga_al_salir());
        assert_eq!(e.loop_mode(), LoopMode::Count(3));
    }

    #[test]
    fn el_volumen_objetivo_de_lo_que_sale_es_configurable() {
        // Baja pero no del todo: se queda de fondo al 25 %.
        let mut e = evento_crossfade();
        e.salida_pct = 25;
        assert!(!e.apaga_al_salir(), "acaba sonando, no se debe cortar");
        assert!(e.resumen().contains("sale → 25%"), "resumen: {}", e.resumen());
    }

    #[test]
    fn un_disparo_unico_suena_de_golpe_y_una_sola_vez() {
        let mut e = Evento::nuevo(TipoEvento::Golpe);
        e.pistas[0] = PistaEvento::vacia(100, 100).con_audio("Explosión", audio("boom.wav"));
        // Aunque el evento esté en infinito, un efecto no se repite.
        e.bucle = BUCLE_INFINITO;

        let spec = e.spec_entrada().expect("debe existir");
        assert_eq!(spec.entrance, Entrance::Hit, "sin fade: entra de golpe");
        assert_eq!(spec.loop_mode, LoopMode::None, "un efecto no se repite");
        assert_eq!(spec.stop_after, None);
        // Y se monta sobre lo que suene: no apaga nada.
        assert_eq!(spec.on_previous, OnPrevious::Keep);
    }

    #[test]
    fn un_fade_in_que_no_acaba_en_silencio_no_se_corta() {
        let mut e = Evento::nuevo(TipoEvento::FadeIn);
        // Sube de 0 a 60 % y se queda ahí sonando: no se debe cortar.
        e.pistas[0] = PistaEvento::vacia(0, 60).con_audio("Ambiente", audio("amb.wav"));
        let spec = e.spec_entrada().unwrap();
        assert_eq!(spec.extremos_de_entrada(), (0.0, 0.6));
        assert_eq!(spec.stop_after, None, "acaba sonando al 60 %, no se corta");
    }

    #[test]
    fn un_hueco_sin_audio_no_da_especificacion() {
        let e = Evento::nuevo(TipoEvento::FadeIn);
        assert!(e.spec_entrada().is_none(), "sin audio elegido no hay nada que sonar");
    }

    #[test]
    fn cambiar_la_duracion_no_toca_el_resto_de_la_configuracion() {
        // Es justo lo que se pide: alargar una escena es mover un número.
        let mut e = evento_crossfade();
        let antes = e.clone();
        e.duracion_ms = 15000;

        assert_eq!(e.pistas, antes.pistas, "los audios se quedan donde estaban");
        assert_eq!(e.tipo, antes.tipo);
        assert_eq!(e.curva, antes.curva);
        assert_eq!(e.bucle, antes.bucle);
        assert_eq!(e.salida_pct, antes.salida_pct);
        // La duración es una sola para los dos lados: es lo que la hace un
        // crossfade y no dos fades seguidos.
        assert_eq!(e.duracion(), Duration::from_millis(15000));
    }

    #[test]
    fn el_resumen_dice_el_tipo_las_rampas_y_el_bucle() {
        let e = evento_crossfade();
        let r = e.resumen();
        assert!(r.contains("Crossfade"), "resumen: {r}");
        assert!(r.contains("entra 0→100%"), "resumen: {r}");
        assert!(r.contains("sale → 0%"), "resumen: {r}");
        assert!(r.contains("×3"), "resumen: {r}");
    }

    #[test]
    fn el_resumen_del_fade_out_habla_de_lo_que_suena() {
        let e = Evento::nuevo(TipoEvento::FadeOut);
        let r = e.resumen();
        assert!(r.contains("lo que suena"), "resumen: {r}");
        assert!(!r.contains("sin audio"), "un fade out no está 'sin audio': {r}");
    }
}
