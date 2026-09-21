//! Tests del hito SHOW: parada de emergencia, auto-follow, ducking y loop.
//!
//! Los deterministas (bloqueo de edición, programador de auto-follow, aviso de
//! MP3) están como unitarios en `src/show.rs`. Aquí se prueba lo que necesita
//! el motor de audio de verdad.

use std::path::{Path, PathBuf};
use std::time::Duration;

use teatroplayer::engine::backend::{AudioBackend, OutputSelection};
use teatroplayer::engine::model::{AudioSource, CueSpec, Curve, LoopMode, MilliDb, OnPrevious};
use teatroplayer::engine::rodio_backend::RodioBackend;
use cpal::traits::HostTrait;

fn fixture(nombre: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(nombre)
}

fn hay_salida() -> bool {
    cpal::default_host().default_output_device().is_some()
}

// ---------------------------------------------------------------------------
// T-SHOW-003 — parada de emergencia con fade de 50 ms
// ---------------------------------------------------------------------------

#[test]
fn la_parada_de_emergencia_apaga_las_cuatro_pistas() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("abrir");

    let mut spec = CueSpec::simple();
    spec.volume = MilliDb::from_db(-40.0);
    spec.loop_mode = LoopMode::Infinite;

    for _ in 0..4 {
        backend
            .play(&spec, AudioSource::File(fixture("tone_short.wav")))
            .expect("debe sonar");
    }
    assert_eq!(backend.active_tracks(), 4);

    let inicio = std::time::Instant::now();
    backend.stop_all_con_fade(Duration::from_millis(
        teatroplayer::show::FADE_EMERGENCIA_MS,
    ));

    // El fade son 50 ms y luego se corta; se da un margen para que el hilo
    // de corte y el mixer hagan su trabajo.
    let mut paradas = false;
    while inicio.elapsed() < Duration::from_secs(3) {
        if backend.active_tracks() == 0 {
            paradas = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(paradas, "las pistas no se apagaron en 3 s");
    assert_eq!(backend.active_tracks(), 0);
}

#[test]
fn la_parada_de_emergencia_tarda_poco() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("abrir");

    let mut spec = CueSpec::simple();
    spec.volume = MilliDb::from_db(-45.0);
    spec.loop_mode = LoopMode::Infinite;
    backend
        .play(&spec, AudioSource::File(fixture("tone_short.wav")))
        .expect("debe sonar");

    let inicio = std::time::Instant::now();
    backend.stop_all_con_fade(Duration::from_millis(50));

    let mut tarde = None;
    while inicio.elapsed() < Duration::from_secs(2) {
        if backend.active_tracks() == 0 {
            tarde = Some(inicio.elapsed());
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let tarde = tarde.expect("no se apagó");
    // 50 ms de fade + un margen generoso para el hilo y el mixer.
    assert!(
        tarde < Duration::from_millis(500),
        "tardó {tarde:?} en apagarse; con un fade de 50 ms debería ser mucho menos"
    );
}

// ---------------------------------------------------------------------------
// T-SHOW-004 — ducking medido sobre la envolvente
// ---------------------------------------------------------------------------

/// El ducking se mide **offline** contra la envolvente: es lo único que se
/// puede comprobar de forma determinista, sin depender de la tarjeta.
#[test]
fn el_duck_deja_la_pista_al_30_por_ciento() {
    use rodio::SampleRate;
    use teatroplayer::engine::live::LiveGain;

    let sr = SampleRate::new(48_000).expect("48000 Hz");

    // Una pista sonando a pleno volumen (ganancia 1.0).
    let tono = rodio::source::SineWave::new(440.0);
    let (mut envuelta, control) = LiveGain::new(tono, 1.0);

    // Se dispara otra entrada con duck al 30 % en 200 ms.
    control.fade_to(0.30, Duration::from_millis(200), Curve::EqualPower);

    // A los 250 ms la primera debe estar ya en 0.3 (es decir, -10.46 dB).
    let frames_250ms = (sr.get() as f32 * 0.25) as usize;
    for _ in 0..frames_250ms {
        envuelta.next();
    }

    let ganancia = control.gain();
    // 0.30 lineal = -10.46 dB; la especificación pide -10.5 ± 0.5 dB.
    assert!(
        (ganancia - 0.30).abs() < 0.02,
        "la ganancia tras el duck es {ganancia}, se esperaba 0.30"
    );

    let db = 20.0 * ganancia.log10();
    assert!(
        (db - (-10.5)).abs() < 0.5,
        "el duck dejó la pista a {db:.2} dB, se esperaba -10.5 ± 0.5"
    );
}

#[test]
fn el_duck_no_produce_un_salto_brusco() {
    use rodio::SampleRate;
    use teatroplayer::engine::live::LiveGain;

    let sr = SampleRate::new(48_000).expect("48000 Hz");
    let (mut envuelta, control) = LiveGain::new(rodio::source::SineWave::new(440.0), 1.0);

    control.fade_to(0.30, Duration::from_millis(200), Curve::EqualPower);

    // Se mide la GANANCIA, no la muestra: el seno ya varía ~0.058 por muestra
    // a 440 Hz, así que mirar las muestras no diría nada del duck.
    //
    // Techo teorico: g(t) = 1 - 0.7*(1 - cos(pi/2 * t)), cuya pendiente maxima
    // es 0.7*(pi/2)/T por segundo, o sea 1.15e-4 por muestra a 48000 Hz.
    let mut anterior = control.gain();
    let mut peor = 0.0f32;
    for _ in 0..(sr.get() / 2) {
        envuelta.next();
        let g = control.gain();
        peor = peor.max((g - anterior).abs());
        anterior = g;
    }

    let limite = 3.0e-4; // unas 2.5 veces el techo, por redondeo
    assert!(
        peor < limite,
        "salto de ganancia {peor} >= {limite}: el duck produce un clic"
    );
}

// ---------------------------------------------------------------------------
// T-SHOW-005 — continuidad del loop
// ---------------------------------------------------------------------------

/// Un loop infinito no debe dejar huecos: se decodifica un buen rato y se
/// comprueba que no aparece un silencio largo (que sería el salto entre
/// vueltas).
#[test]
fn el_loop_no_deja_huecos_de_mas_de_5_ms() {
    use rodio::Source;

    let archivo = fixture("tone_short.wav");
    let decodificado = rodio::Decoder::new_looped(
        std::io::BufReader::new(std::fs::File::open(&archivo).expect("abrir wav")),
    )
    .expect("decodificar");

    let sample_rate = decodificado.sample_rate().get();
    let cinco_ms = (sample_rate as f32 * 0.005) as usize;

    // 3 segundos dan varias vueltas de un audio de 1 s.
    let total = sample_rate as usize * 3;
    let muestras: Vec<f32> = decodificado.take(total).collect();
    assert!(!muestras.is_empty());

    let mut silencio = 0usize;
    let mut peor_silencio = 0usize;
    for v in &muestras {
        if v.abs() < 1e-4 {
            silencio += 1;
            peor_silencio = peor_silencio.max(silencio);
        } else {
            silencio = 0;
        }
    }

    assert!(
        peor_silencio < cinco_ms,
        "hay un silencio de {peor_silencio} muestras (> {cinco_ms} = 5 ms): el loop salta"
    );
}

// ---------------------------------------------------------------------------
// T-UI-009 — pads: se disparan al margen de la secuencia
// ---------------------------------------------------------------------------

/// Un pad suena **además** de lo que ya estaba sonando. Lo que pase con lo
/// anterior lo decide el `onPrevious` de la entrada, igual que en la lista.
#[test]
fn un_pad_suena_sin_apagar_al_anterior() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("abrir");

    let mut spec = CueSpec::simple();
    spec.volume = MilliDb::from_db(-40.0);
    spec.on_previous = OnPrevious::Keep; // explícito: no apaga al anterior
    spec.loop_mode = LoopMode::Infinite;

    let a = backend
        .play(&spec, AudioSource::File(fixture("tone_a.wav")))
        .expect("pad A");
    let b = backend
        .play(&spec, AudioSource::File(fixture("tone_c.wav")))
        .expect("pad B");

    std::thread::sleep(std::time::Duration::from_millis(400));

    // Los dos siguen vivos: un pad no corta lo que suena.
    assert!(!a.state().is_done(), "A se apagó al sonar B");
    assert!(!b.state().is_done(), "B no llegó a sonar");
    assert_eq!(backend.active_tracks(), 2);

    backend.stop_all_con_fade(std::time::Duration::from_millis(50));
}

// ---------------------------------------------------------------------------
// T-UI-010 — la duración, base de la cuenta atrás
// ---------------------------------------------------------------------------

#[test]
fn una_pista_normal_sabe_cuanto_dura() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("abrir");

    let mut spec = CueSpec::simple();
    spec.volume = MilliDb::from_db(-45.0);
    // tone_a.wav son 6 segundos.
    let pista = backend
        .play(&spec, AudioSource::File(fixture("tone_a.wav")))
        .expect("debe sonar");

    let duracion = pista.duration();
    assert!(duracion.is_some(), "sin duración no hay cuenta atrás posible");
    let d = duracion.unwrap();
    assert!(
        (d.as_secs_f32() - 6.0).abs() < 0.5,
        "se esperaban ~6 s y llegó {d:?}"
    );
    pista.stop();
}

#[test]
fn un_loop_infinito_no_tiene_duracion() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("abrir");

    let mut spec = CueSpec::simple();
    spec.volume = MilliDb::from_db(-45.0);
    spec.loop_mode = LoopMode::Infinite;
    let pista = backend
        .play(&spec, AudioSource::File(fixture("tone_short.wav")))
        .expect("debe sonar");

    // Un loop no termina: mostrar "queda 0:00" sería mentir, así que la UI
    // pinta un ∞ en vez de una cuenta atrás.
    assert!(pista.duration().is_none(), "un loop infinito no debe dar duración");
    pista.stop();
}

// ---------------------------------------------------------------------------
// T-SHOW-001 — en modo Función la edición no se aplica
// ---------------------------------------------------------------------------

#[test]
fn en_funcion_un_intento_de_edicion_no_cambia_el_modelo() {
    use teatroplayer::sesion::modelo::{Cue, Sesion};
    use teatroplayer::show::Bloqueo;

    let mut sesion = Sesion::nueva();
    let mut cue = Cue::nuevo("Ambiente", "a.wav");
    cue.spec.volume = MilliDb::from_db(0.0);
    sesion.cues.push(cue);

    let mut bloqueo = Bloqueo::nuevo();
    bloqueo.entrar_en_funcion();

    // El "dispatch" de la UI pasa por este filtro: si no permite editar, el
    // cambio no llega al modelo.
    let antes = sesion.clone();
    if bloqueo.permite_editar() {
        sesion.cues[0].spec.volume = MilliDb::from_db(-6.0);
    }

    assert_eq!(sesion, antes, "en Función la sesión no debe cambiar");
}
