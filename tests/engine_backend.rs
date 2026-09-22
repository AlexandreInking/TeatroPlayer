//! Tests del motor (hito ENG).
//!
//! Los que necesitan tarjeta de sonido se omiten solos si no hay ninguna
//! disponible, así que esto también corre en una máquina sin audio (CI).

use std::path::Path;
use std::time::Duration;

use cpal::traits::HostTrait;
use rodio::mixer::mixer;
use rodio::{ChannelCount, SampleRate, Source};
use teatroplayer::engine::backend::{AudioBackend, OutputSelection, TrackState};
use teatroplayer::engine::model::{AudioSource, CueSpec, Entrance, MilliDb};
use teatroplayer::engine::rodio_backend::{master_limit_settings, RodioBackend};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn hay_salida() -> bool {
    cpal::default_host().default_output_device().is_some()
}

fn dbfs(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn pico(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, v| m.max(v.abs()))
}

// --------------------------------------------------------------------------
// T-ENG-004 — el limitador de máster
// --------------------------------------------------------------------------

/// Mezcla dos señales, una a -3 dBFS y otra a +6 dBFS, y devuelve el pico.
///
/// Se hace **offline** con el mismo `rodio::mixer` que usa el backend: no hace
/// falta tarjeta de sonido para comprobar el máster.
fn pico_de_la_mezcla(con_limitador: bool) -> f32 {
    let ch = ChannelCount::new(1).expect("1 canal");
    let sr = SampleRate::new(48_000).expect("48000 Hz");

    let (entrada, salida) = mixer(ch, sr);
    // Igual que en el backend: `Zero` mantiene vivo el bus.
    entrada.add(rodio::source::Zero::new(ch, sr));
    entrada.add(rodio::source::SineWave::new(220.0).amplify(dbfs(-3.0)));
    entrada.add(rodio::source::SineWave::new(440.0).amplify(dbfs(6.0)));

    // El `if` no puede devolver dos tipos distintos, así que se unifica en un
    // trait object (rodio implementa `Source` para `Box<dyn Source>`).
    let fuente: Box<dyn Source<Item = f32> + Send> = if con_limitador {
        Box::new(salida.limit(master_limit_settings()))
    } else {
        Box::new(salida)
    };

    let muestras: Vec<f32> = fuente.take(48_000 * 2).collect(); // 2 segundos
    pico(&muestras)
}

#[test]
fn sin_limitador_la_mezcla_recorta() {
    // Primero se comprueba que el test tiene dientes: si esto no recortara,
    // el test del limitador no estaría probando nada.
    let pico = pico_de_la_mezcla(false);
    assert!(
        pico > 1.0,
        "la mezcla sin limitar debería pasar de 0 dBFS y da {pico:.4}"
    );
}

#[test]
fn el_limitador_de_master_impide_el_recorte() {
    let pico = pico_de_la_mezcla(true);
    assert!(
        pico <= 1.0,
        "la salida limitada llegó a {pico:.4} (> 1.0 = 0 dBFS): recorta"
    );
}

#[test]
fn el_limitador_no_aplana_el_audio_normal() {
    // Con señal moderada el limitador debe ser transparente: si recortara de
    // más, estaría arruinando la dinámica de la obra.
    let ch = ChannelCount::new(1).expect("1 canal");
    let sr = SampleRate::new(48_000).expect("48000 Hz");

    let (entrada, salida) = mixer(ch, sr);
    entrada.add(rodio::source::Zero::new(ch, sr));
    entrada.add(rodio::source::SineWave::new(440.0).amplify(dbfs(-12.0)));

    let seco: Vec<f32> = salida.take(48_000).collect();

    let (entrada2, salida2) = mixer(ch, sr);
    entrada2.add(rodio::source::Zero::new(ch, sr));
    entrada2.add(rodio::source::SineWave::new(440.0).amplify(dbfs(-12.0)));
    let limitado: Vec<f32> = salida2
        .limit(master_limit_settings())
        .take(48_000)
        .collect();

    let pico_seco = pico(&seco);
    let pico_limitado = pico(&limitado);
    let diferencia = (pico_seco - pico_limitado).abs() / pico_seco;
    assert!(
        diferencia < 0.05,
        "a -12 dBFS el limitador cambió el nivel un {:.2} % ({pico_seco:.4} -> {pico_limitado:.4})",
        diferencia * 100.0
    );
}

// --------------------------------------------------------------------------
// T-ENG-002 / T-ENG-003 — el backend sobre la tarjeta real
// ------------------------------------------------------------------------//

#[test]
fn una_entrada_suena_y_reporta_su_estado() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("debe abrir");

    let mut spec = CueSpec::simple();
    spec.volume = MilliDb::from_db(-40.0);
    let track = backend
        .play(&spec, AudioSource::File(fixture("tone_short.wav")))
        .expect("debe sonar");

    assert_eq!(backend.active_tracks(), 1);
    assert!(!track.state().is_done());

    std::thread::sleep(Duration::from_millis(300));
    // Debe haber avanzado: la posición la lleva rodio.
    assert!(
        track.position() > Duration::from_millis(50),
        "posicion = {:?}",
        track.position()
    );

    track.stop();
    std::thread::sleep(Duration::from_millis(200));
    assert!(track.state().is_done());
    assert_eq!(track.state(), TrackState::Stopped);
    assert_eq!(backend.active_tracks(), 0);
}

#[test]
fn cuatro_pistas_a_la_vez_con_fades_a_distintos_tiempos() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("debe abrir");

    let nombres = [
        "tone_a.wav",
        "tone_b.wav",
        "tone_c.wav",
        "tone_short.wav",
    ];
    let mut handles = Vec::new();
    for nombre in nombres {
        let mut spec = CueSpec::simple();
        spec.volume = MilliDb::from_db(-40.0);
        spec.entrance = Entrance::FadeIn {
            duration: Duration::from_millis(200),
            curve: Default::default(),
            from_percent: 0,
            to_percent: 100,
        };
        handles.push(
            backend
                .play(&spec, AudioSource::File(fixture(nombre)))
                .expect("debe sonar"),
        );
        std::thread::sleep(Duration::from_millis(120));
    }

    assert_eq!(backend.active_tracks(), 4, "deben estar las 4 sonando");

    // Se van bajando escalonadamente, como haría el operador.
    for (i, h) in handles.iter().enumerate() {
        h.fade_out(Duration::from_millis(300), Default::default());
        std::thread::sleep(Duration::from_millis(150 + 80 * i as u64));
    }

    // La posición debe ser monótona mientras suena: nunca retrocede.
    let mut ultima = Duration::ZERO;
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(50));
        let ahora = handles[1].position();
        assert!(ahora >= ultima, "la posición retrocedió: {ahora:?} < {ultima:?}");
        ultima = ahora;
    }

    for h in &handles {
        h.stop();
    }
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(backend.active_tracks(), 0);
}

#[test]
fn stop_after_hace_el_fade_y_luego_corta() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("debe abrir");

    let mut spec = CueSpec::simple();
    spec.volume = MilliDb::from_db(-40.0);
    let track = backend
        .play(&spec, AudioSource::File(fixture("tone_c.wav")))
        .expect("debe sonar");

    track.stop_after(Duration::from_millis(400), Default::default());

    // Durante el fade la pista sigue viva y en estado FadingOut.
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(track.state(), TrackState::FadingOut, "estado = {:?}", track.state());

    // Al terminar el fade (+ margen), se corta sola.
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(track.state(), TrackState::Stopped, "estado = {:?}", track.state());
    assert_eq!(backend.active_tracks(), 0);
}

#[test]
fn stop_all_para_todas_las_pistas() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("debe abrir");

    let mut spec = CueSpec::simple();
    spec.volume = MilliDb::from_db(-45.0);
    for _ in 0..3 {
        let _ = backend.play(&spec, AudioSource::File(fixture("tone_a.wav")));
    }
    assert_eq!(backend.active_tracks(), 3);

    backend.stop_all();
    assert_eq!(backend.active_tracks(), 0);
}
