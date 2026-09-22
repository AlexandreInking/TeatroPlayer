//! Tests de integración de los eventos predefinidos (hito EVT).
//!
//! Los unitarios del modelo están en `src/eventos.rs`. Aquí se prueba lo que
//! sólo se ve desde fuera: **que los eventos viajan dentro del `.tpshow`** y
//! que las rampas que generan son las que se espera oír.
//!
//! La regla que manda en todo el fichero: **sólo fade in, crossfade y disparo
//! único eligen audio**. El fade out y el lado que sale de un crossfade actúan
//! sobre lo que está sonando, así que no se eligen ni se guardan.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use teatroplayer::engine::model::{CueSpec, Curve, Entrance, LoopMode, MilliDb};
use teatroplayer::eventos::{Evento, PistaEvento, TipoEvento, BUCLE_INFINITO};
use teatroplayer::paquete;
use teatroplayer::sesion::modelo::{AudioRef, Cue, Sesion};

fn fixture(nombre: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(nombre)
}

fn audio(nombre: &str) -> AudioRef {
    AudioRef { file_name: nombre.to_string(), rel_path: format!("audio/{nombre}"), ..Default::default() }
}

/// Una obra con dos audios y tres eventos, guardada como `.tpshow`.
fn obra_de_prueba(etiqueta: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tp_eventos_it_{etiqueta}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("audio")).expect("crear audio/");
    for n in ["tone_a.wav", "tone_b.wav"] {
        fs::copy(fixture(n), dir.join("audio").join(n)).expect("copiar fixture");
    }

    let mut sesion = Sesion::nueva();
    sesion.nombre = "Obra con eventos".to_string();
    sesion.cues = vec![
        Cue::nuevo("Ambiente", "tone_a.wav"),
        Cue::nuevo("Viento", "tone_b.wav"),
    ];

    // Crossfade: sólo se elige el que entra. El que sale es el que suena.
    let mut crossfade = Evento::nuevo(TipoEvento::Crossfade);
    crossfade.nombre = "Cambio de escena".to_string();
    crossfade.duracion_ms = 8000;
    crossfade.bucle = BUCLE_INFINITO;
    crossfade.pistas[0] = PistaEvento::vacia(0, 100).con_audio("Viento", audio("tone_b.wav"));
    crossfade.salida_pct = 0;

    // Fade out: no elige audio, baja lo que suene.
    let mut fade_out = Evento::nuevo(TipoEvento::FadeOut);
    fade_out.nombre = "Quitar la música".to_string();
    fade_out.duracion_ms = 3000;
    fade_out.salida_pct = 0;

    let mut golpe = Evento::nuevo(TipoEvento::Golpe);
    golpe.nombre = "Trueno".to_string();
    golpe.pistas[0] = PistaEvento::vacia(100, 100).con_audio("Ambiente", audio("tone_a.wav"));
    golpe.tecla = Some("F3".to_string());
    golpe.pad = true;

    sesion.eventos = vec![crossfade, fade_out, golpe];

    fs::write(dir.join("sesion.json"), sesion.a_json().expect("serializar"))
        .expect("escribir sesion.json");
    let salida = dir.join("obra.tpshow");
    paquete::pack(&dir, &salida).expect("empaquetar");
    salida
}

fn abrir(etiqueta: &str) -> Sesion {
    let p = obra_de_prueba(etiqueta);
    let texto = paquete::leer_sesion(&p).expect("leer la sesión del paquete");
    let (sesion, _) = Sesion::desde_json(&texto).expect("la sesión debe abrir");
    sesion
}

#[test]
fn los_eventos_viajan_dentro_del_tpshow() {
    let sesion = abrir("viajan");
    assert_eq!(sesion.eventos.len(), 3, "los tres eventos deben volver");

    let crossfade = &sesion.eventos[0];
    assert_eq!(crossfade.nombre, "Cambio de escena");
    assert_eq!(crossfade.tipo, TipoEvento::Crossfade);
    assert_eq!(crossfade.duracion_ms, 8000);
    assert_eq!(crossfade.bucle, BUCLE_INFINITO);
    assert_eq!(crossfade.pistas.len(), 1, "sólo el que entra");
    assert_eq!(crossfade.pistas[0].audio.file_name, "tone_b.wav");
    assert_eq!(crossfade.salida_pct, 0);

    let fade_out = &sesion.eventos[1];
    assert_eq!(fade_out.tipo, TipoEvento::FadeOut);
    assert!(fade_out.pistas.is_empty(), "un fade out no guarda audio");

    let golpe = &sesion.eventos[2];
    assert_eq!(golpe.tipo, TipoEvento::Golpe);
    assert_eq!(golpe.tecla.as_deref(), Some("F3"));
    assert!(golpe.pad);
}

#[test]
fn solo_tres_tipos_necesitan_elegir_audio() {
    let sesion = abrir("cuantos");
    for e in &sesion.eventos {
        let esperados = e.tipo.cuantos_audios();
        assert_eq!(
            e.pistas.len(),
            esperados,
            "'{}' ({:?}) debe guardar {esperados} audio(s)",
            e.nombre,
            e.tipo
        );
    }
    // Y sólo esos tres lo piden.
    assert_eq!(TipoEvento::FadeIn.cuantos_audios(), 1);
    assert_eq!(TipoEvento::Crossfade.cuantos_audios(), 1);
    assert_eq!(TipoEvento::Golpe.cuantos_audios(), 1);
    assert_eq!(TipoEvento::FadeOut.cuantos_audios(), 0);
}

#[test]
fn el_crossfade_solo_lleva_el_audio_que_entra() {
    let sesion = abrir("rampas");
    let evento = &sesion.eventos[0];

    let entra = evento.spec_entrada().expect("el que entra");
    // El nuevo pasa de no sonar a sonar.
    assert_eq!(entra.extremos_de_entrada(), (0.0, 1.0));
    assert_eq!(entra.stop_after, None, "el que entra se queda sonando");
    assert_eq!(entra.loop_mode, LoopMode::Infinite);

    // El que sale no se elige: sólo se fija su volumen objetivo, y por defecto
    // se apaga del todo.
    assert_eq!(evento.salida_pct, 0);
    assert!(evento.apaga_al_salir());
    // La duración es una sola para los dos lados: es lo que lo hace un
    // crossfade y no dos fades seguidos.
    assert_eq!(evento.duracion(), Duration::from_millis(8000));
}

#[test]
fn un_fade_out_no_tiene_audio_de_entrada_y_actua_sobre_lo_que_suena() {
    let sesion = abrir("fadeout");
    let evento = &sesion.eventos[1];

    assert!(evento.tipo.actua_sobre_lo_que_suena());
    assert!(evento.spec_entrada().is_none(), "no hay nada que entrar");
    assert!(evento.completo(), "no le falta nada: el audio es el que suene");
    assert_eq!(evento.salida_pct, 0, "por defecto se apaga");
    assert!(evento.apaga_al_salir());
}

#[test]
fn el_disparo_unico_entra_de_golpe_y_solo_una_vez() {
    let sesion = abrir("golpe");
    let evento = &sesion.eventos[2];

    let spec = evento.spec_entrada().expect("debe existir");
    assert_eq!(spec.entrance, Entrance::Hit, "un efecto entra de golpe");
    assert_eq!(spec.loop_mode, LoopMode::None, "un efecto no se repite");
    assert_eq!(spec.stop_after, None);
    assert_eq!(spec.extremos_de_entrada(), (1.0, 1.0));
    // No toca lo que suena: se monta encima.
    assert!(!evento.tipo.actua_sobre_lo_que_suena());
}

#[test]
fn alargar_la_escena_no_toca_nada_mas_del_evento() {
    // Lo que se pide: acortar o ampliar una escena es mover un número, sin
    // rehacer la configuración. Se comprueba sobre la obra ya guardada y
    // vuelta a leer, que es el caso real.
    let mut sesion = abrir("alargar");
    let antes = sesion.eventos[0].clone();
    sesion.eventos[0].duracion_ms = 20000;

    let evento = &sesion.eventos[0];
    assert_eq!(evento.pistas, antes.pistas, "el audio que entra no se toca");
    assert_eq!(evento.tipo, antes.tipo);
    assert_eq!(evento.curva, antes.curva);
    assert_eq!(evento.bucle, antes.bucle);
    assert_eq!(evento.salida_pct, antes.salida_pct, "el volumen objetivo tampoco");
    assert_eq!(evento.duracion(), Duration::from_millis(20000));
}

#[test]
fn una_obra_sin_eventos_sigue_abriendo_igual() {
    // Una sesión sin la clave `eventos`: se abre y se migra.
    let mut s = Sesion::nueva();
    s.cues = vec![Cue::nuevo("Ambiente", "tone_a.wav")];
    let json = s.a_json().unwrap();

    let (leida, _) = Sesion::desde_json(&json).expect("debe abrir");
    assert!(leida.eventos.is_empty());
    assert_eq!(leida.cues.len(), 1);
}

#[test]
fn un_fade_entre_dos_porcentajes_cualquiera_llega_al_motor() {
    // El caso que antes no se podía pedir: entrar de 20 a 60 %, no de 0 a 100.
    let mut spec: CueSpec = CueSpec::con_rampa(20, 60, Duration::from_millis(4000), Curve::Linear);
    spec.volume = MilliDb::from_db(-6.0);

    let (desde, hasta) = spec.extremos_de_entrada();
    assert!((desde - 0.2).abs() < 1e-6);
    assert!((hasta - 0.6).abs() < 1e-6);

    // El volumen de la pista va antes de la envolvente: no cambia los extremos.
    assert!((spec.static_gain() - 0.501).abs() < 1e-2);
}
