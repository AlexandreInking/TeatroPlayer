//! Tests de integración del `.tpshow` con el motor de audio (hito FMT).
//!
//! Los deterministas (pack, unpack, verify, diff, ZipEntryReader) están como
//! unitarios en `src/paquete/`. Aquí se prueba lo que necesita el motor de
//! verdad: **sonar un audio que vive dentro del paquete** sin extraerlo.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cpal::traits::HostTrait;
use teatroplayer::engine::backend::{AudioBackend, OutputSelection};
use teatroplayer::engine::model::{AudioSource, CueSpec, MilliDb};
use teatroplayer::engine::rodio_backend::RodioBackend;
use teatroplayer::paquete;

fn fixture(nombre: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(nombre)
}

fn hay_salida() -> bool {
    cpal::default_host().default_output_device().is_some()
}

/// Construye un `.tpshow` con audios reales (los mismos fixtures de 6 s).
///
/// Cada test pasa su propia `etiqueta`: si compartieran carpeta, uno haría
/// `remove_dir_all` mientras otro todavía está copiando y fallaría al azar.
fn paquete_de_prueba(etiqueta: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tp_paquete_it_{etiqueta}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("audio")).expect("crear audio/");
    for n in ["tone_a.wav", "tone_short.wav"] {
        fs::copy(fixture(n), dir.join("audio").join(n)).expect("copiar fixture");
    }
    fs::write(
        dir.join("sesion.json"),
        r#"{"version":1,"name":"Prueba","cues":[{"nombre":"Ambiente","audio":{"fileName":"tone_a.wav"}}]}"#,
    )
    .expect("escribir sesion.json");

    let salida = dir.join("obra.tpshow");
    paquete::pack(&dir, &salida).expect("empaquetar");
    salida
}

#[test]
fn el_paquete_lleva_los_audios_y_la_sesion() {
    let p = paquete_de_prueba("contenido");
    let informe = paquete::verify(&p).expect("verificar");
    assert!(informe.ok(), "problemas: {:?}", informe.problemas);
    assert_eq!(informe.entradas, 4); // manifest + sesion + 2 audios
}

#[test]
fn suena_un_audio_que_vive_dentro_del_tpshow() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let p = paquete_de_prueba("suena");
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("abrir");

    let mut spec = CueSpec::simple();
    spec.volume = MilliDb::from_db(-40.0);
    let fuente = AudioSource::Paquete {
        tpshow: p.clone(),
        entrada: "audio/tone_a.wav".to_string(),
    };

    let pista = backend.play(&spec, fuente).expect("debe sonar desde el paquete");
    assert_eq!(backend.active_tracks(), 1);

    std::thread::sleep(Duration::from_millis(400));
    assert!(
        pista.position() > Duration::from_millis(50),
        "no avanzó la posición: {:?}",
        pista.position()
    );
    assert!(!pista.state().is_done(), "estado = {:?}", pista.state());

    pista.stop();
    std::thread::sleep(Duration::from_millis(200));
    assert!(pista.state().is_done());
}

#[test]
fn el_loop_desde_el_paquete_avanza_sin_errores() {
    if !hay_salida() {
        eprintln!("sin salida de audio: se omite");
        return;
    }
    let p = paquete_de_prueba("loop");
    let backend = RodioBackend::new();
    backend.open(OutputSelection::SystemDefault).expect("abrir");

    let mut spec = CueSpec::simple();
    spec.volume = MilliDb::from_db(-45.0);
    // tone_short.wav dura 1 s: con 4 s de espera da varias vueltas, y cada
    // vuelta obliga al ZipEntryReader a volver al principio de la entrada.
    spec.loop_mode = teatroplayer::engine::model::LoopMode::Infinite;

    let pista = backend
        .play(
            &spec,
            AudioSource::Paquete {
                tpshow: p.clone(),
                entrada: "audio/tone_short.wav".to_string(),
            },
        )
        .expect("debe sonar en loop");

    std::thread::sleep(Duration::from_millis(2500));
    let sano = backend.health();
    assert_eq!(sano.stream_errors, 0, "errores de stream: {}", sano.stream_errors);
    assert!(!pista.state().is_done(), "el loop se cortó: {:?}", pista.state());

    pista.stop();
}

/// El hueco que faltaba: una obra abierta **desde un paquete** se podía guardar
/// y quedarse sin audios. Ahora los audios se copian del paquete de origen.
#[test]
fn guardar_una_obra_abierta_desde_un_paquete_conserva_los_audios() {
    // 1. Un paquete de origen con dos audios reales.
    let dir = std::env::temp_dir().join("tp_paquete_reguardar");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("audio")).unwrap();
    for n in ["tone_a.wav", "tone_c.wav"] {
        fs::copy(fixture(n), dir.join("audio").join(n)).unwrap();
    }
    let origen = dir.join("origen.tpshow");
    paquete::pack(&dir, &origen).unwrap();

    // 2. Simular que la app la abrió: las entradas apuntan al paquete, no al disco.
    let audios: Vec<(String, paquete::OrigenAudio)> = ["tone_a.wav", "tone_c.wav"]
        .iter()
        .map(|n| {
            (
                n.to_string(),
                paquete::OrigenAudio::Paquete {
                    tpshow: origen.clone(),
                    entrada: format!("audio/{n}"),
                },
            )
        })
        .collect();

    // 3. Guardar en un paquete NUEVO.
    let destino = dir.join("destino.tpshow");
    let json = r#"{"version":1,"nombre":"Reguardada","cues":[]}"#;
    paquete::escribir(&destino, json, &audios).unwrap();

    let dentro = paquete::lista_audios(&destino).unwrap();
    assert_eq!(dentro, vec!["tone_a.wav".to_string(), "tone_c.wav".to_string()]);

    // Y los bytes son los mismos que los originales, no un hueco.
    let (offset, size) = paquete::lector::localizar(&destino, "audio/tone_a.wav").unwrap();
    let original = fs::read(fixture("tone_a.wav")).unwrap();
    assert_eq!(size as usize, original.len());
    let bytes = &fs::read(&destino).unwrap()[offset as usize..(offset + size) as usize];
    assert!(bytes == original.as_slice(), "el audio copiado no coincide");

    let informe = paquete::verify(&destino).unwrap();
    assert!(informe.ok(), "problemas: {:?}", informe.problemas);
}

/// Guardar sobre el MISMO archivo del que se está leyendo era lo peligroso:
/// se escribe en un .tmp y se renombra, así que no se destruye la obra.
#[test]
fn guardar_sobre_el_mismo_paquete_no_lo_destruye() {
    let dir = std::env::temp_dir().join("tp_paquete_mismo");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("audio")).unwrap();
    fs::copy(fixture("tone_a.wav"), dir.join("audio").join("tone_a.wav")).unwrap();
    let ruta = dir.join("obra.tpshow");
    paquete::pack(&dir, &ruta).unwrap();

    let audios = vec![(
        "tone_a.wav".to_string(),
        paquete::OrigenAudio::Paquete {
            tpshow: ruta.clone(),
            entrada: "audio/tone_a.wav".to_string(),
        },
    )];
    let json = r#"{"version":1,"nombre":"Sobre sí misma","cues":[]}"#;
    paquete::escribir(&ruta, json, &audios).unwrap();

    assert_eq!(paquete::leer_sesion(&ruta).unwrap(), json);
    assert_eq!(
        paquete::lista_audios(&ruta).unwrap(),
        vec!["tone_a.wav".to_string()]
    );
    assert!(paquete::verify(&ruta).unwrap().ok());
    // Y no queda el temporal por ahí.
    assert!(!ruta.with_extension("tmp").exists());
}

#[test]
fn el_umbral_de_zip64_es_el_de_4_gib() {
    use teatroplayer::paquete::{necesita_zip64, UMBRAL_ZIP64};
    assert_eq!(UMBRAL_ZIP64, 4 * 1024 * 1024 * 1024);
    assert!(!necesita_zip64(UMBRAL_ZIP64 - 1));
    assert!(necesita_zip64(UMBRAL_ZIP64));
    assert!(necesita_zip64(UMBRAL_ZIP64 * 2));
}

/// T-FMT-006 de verdad: una obra de 5 GB. Se marca `#[ignore]` porque necesita
/// 5 GB de disco y unos minutos; en CI no corre.
///
///   cargo test --test paquete -- --ignored --nocapture
#[test]
#[ignore = "necesita 5 GB de disco; se ejecuta con --ignored"]
fn zip64_con_una_obra_de_5_gb() {
    const GIGA: u64 = 1024 * 1024 * 1024;
    let tamano = 5 * GIGA;

    let dir = std::env::temp_dir().join("tp_zip64");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("audio")).unwrap();

    // Un WAV de 5 GB "a mano": cabecera RIFF + datos. No hace falta que sea
    // audible; lo que se prueba es el contenedor, no el decodificador.
    let wav = dir.join("audio").join("largo.wav");
    let mut f = std::io::BufWriter::new(fs::File::create(&wav).unwrap());
    f.write_all(b"RIFF").unwrap();
    f.write_all(&((tamano - 8) as u32).to_le_bytes()).unwrap();
    f.write_all(b"WAVEdata").unwrap();
    f.write_all(&((tamano - 20) as u32).to_le_bytes()).unwrap();
    let bloque = vec![7u8; 1024 * 1024];
    let mut escrito: u64 = 20;
    while escrito < tamano {
        let n = std::cmp::min(bloque.len() as u64, tamano - escrito) as usize;
        f.write_all(&bloque[..n]).unwrap();
        escrito += n as u64;
    }
    f.flush().unwrap();
    drop(f);
    println!("  wav de {} bytes", fs::metadata(&wav).unwrap().len());

    let salida = dir.join("grande.tpshow");
    paquete::pack(&dir, &salida).unwrap();
    println!("  paquete de {} bytes", fs::metadata(&salida).unwrap().len());

    // Los 5 GB siguen dentro y el CRC cuadra.
    let informe = paquete::verify(&salida).unwrap();
    assert!(informe.ok(), "problemas: {:?}", informe.problemas);

    // Y se pueden recuperar leyendo por ZipEntryReader.
    let (offset, size) = paquete::lector::localizar(&salida, "audio/largo.wav").unwrap();
    assert_eq!(size, tamano, "el tamaño no se conservó en ZIP64");
    let mut lector = paquete::ZipEntryReader::new(&salida, "audio/largo.wav").unwrap();
    let mut comprobar = [0u8; 64];
    use std::io::{Read, Seek, SeekFrom};
    lector.seek(SeekFrom::Start(tamano - 32)).unwrap();
    lector.read_exact(&mut comprobar[..32]).unwrap();
    assert!(comprobar[..32].iter().all(|b| *b == 7));

    let _ = offset;
    // Limpieza: 5 GB es mucho para dejarlo tirado.
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_tpshow_corrupto_se_detecta_y_no_rompe() {
    let p = paquete_de_prueba("corrupto");

    // Voltear un bit dentro de los datos del audio, no en la estructura.
    let (offset, _) = paquete::lector::localizar(&p, "audio/tone_a.wav").expect("localizar");
    let mut bytes = fs::read(&p).expect("leer");
    bytes[offset as usize + 20] ^= 0b0000_0001;
    let corrupto = p.with_extension("corrupto.tpshow");
    fs::write(&corrupto, &bytes).expect("escribir");

    let informe = paquete::verify(&corrupto).expect("verify no debe fallar, debe informar");
    assert!(!informe.ok(), "un byte corrupto debería detectarse");
    assert!(
        informe.problemas.iter().any(|x| x.contains("tone_a.wav")),
        "debe nombrar la entrada corrupta: {:?}",
        informe.problemas
    );
}
