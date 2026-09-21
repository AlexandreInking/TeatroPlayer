//! Spikes de validación (T-SPIKE-001 … T-SPIKE-006).
//!
//! Binario desechable: existe para comprobar que rodio + cpal aguantan el uso
//! real que le va a dar TeatroPlayer antes de escribir la aplicación. No forma
//! parte del ejecutable final.
//!
//! Uso:
//!   tp-spike outputs                       T-SPIKE-001  lista las salidas
//!   tp-spike open <idx>                    T-SPIKE-001  tono de 440 Hz 1 s
//!   tp-spike multi [secs] [idx]            T-SPIKE-002  3 pistas a la vez
//!   tp-spike fade <ms> [idx]               T-SPIKE-003  fade in en vivo
//!   tp-spike render-fade <ms> <salida.wav> T-SPIKE-003  fade in a archivo
//!   tp-spike xfade <secs> [idx]            T-SPIKE-004  crossfade A->B en vivo
//!   tp-spike render-xfade <secs> <salida.wav>
//!                                          T-SPIKE-004  crossfade a archivo
//!   tp-spike loop <mins> [idx]             T-SPIKE-005  loop infinito
//!   tp-spike stop <ms> [idx]               T-SPIKE-006  parar desde otro hilo
//!   tp-spike pkg-loop <tpshow> <entrada> [mins] [idx]
//!                                          T-FMT-004    loop desde un .tpshow

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use eframe::egui;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use teatroplayer::engine::envelope::{Curve, GainRamp};
use teatroplayer::paquete::ZipEntryReader;

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> PathBuf {
    Path::new("tests").join("fixtures").join(name)
}

/// Abre la salida de audio. `index` = None usa el dispositivo por defecto de
/// Windows. Devuelve el sink y un contador de errores del stream (un underrun
/// o un fallo de WASAPI pasaría por aquí).
fn open_sink(index: Option<usize>) -> Result<(MixerDeviceSink, Arc<AtomicUsize>)> {
    let errors = Arc::new(AtomicUsize::new(0));
    let err_counter = Arc::clone(&errors);
    let on_error = move |e: cpal::StreamError| {
        err_counter.fetch_add(1, Ordering::SeqCst);
        eprintln!("  [stream] {e}");
    };

    let mut sink = match index {
        Some(i) => {
            let device = cpal::default_host()
                .output_devices()
                .map_err(|e| anyhow!("no se pudieron enumerar las salidas: {e}"))?
                .nth(i)
                .ok_or_else(|| anyhow!("no existe una salida con indice {i}"))?;
            let name = device
                .description()
                .map(|d| d.to_string())
                .unwrap_or_else(|_| "<sin nombre>".into());
            println!("  salida      : [{i}] {name}");
            DeviceSinkBuilder::from_device(device)?
                .with_error_callback(on_error)
                .open_sink_or_fallback()?
        }
        None => {
            println!("  salida      : dispositivo por defecto de Windows");
            DeviceSinkBuilder::from_default_device()?
                .with_error_callback(on_error)
                .open_sink_or_fallback()?
        }
    };
    sink.log_on_drop(false);

    let config = sink.config();
    println!(
        "  formato     : {} Hz / {} canales",
        config.sample_rate(),
        config.channel_count()
    );
    Ok((sink, errors))
}

fn decode(name: &str) -> Result<Decoder<BufReader<File>>> {
    let path = fixture(name);
    Decoder::new(BufReader::new(
        File::open(&path).with_context(|| format!("no se pudo abrir {}", path.display()))?,
    ))
    .with_context(|| format!("no se pudo decodificar {}", path.display()))
}

fn list_outputs() -> Result<()> {
    let devices = cpal::default_host()
        .output_devices()
        .map_err(|e| anyhow!("no se pudieron enumerar las salidas de audio: {e}"))?;
    let mut found = 0;
    for (i, device) in devices.enumerate() {
        let name = device
            .description()
            .map(|d| d.to_string())
            .unwrap_or_else(|_| "<sin nombre>".to_string());
        println!("{i}\t{name}");
        found += 1;
    }
    if found == 0 {
        return Err(anyhow!("el sistema no reporta ninguna salida de audio"));
    }
    Ok(())
}

fn open_and_tone(index: usize) -> Result<()> {
    let (sink, _errors) = open_sink(Some(index))?;
    let tone = rodio::source::SineWave::new(440.0)
        .take_duration(Duration::from_secs(1))
        .amplify(0.15);
    sink.mixer().add(tone);
    thread::sleep(Duration::from_millis(1300));
    println!("OK: tono de 440 Hz reproducido durante 1 s");
    Ok(())
}

// ---------------------------------------------------------------------------
// T-SPIKE-002 — tres pistas simultáneas
// ---------------------------------------------------------------------------

fn cmd_multi(secs: u64, dev: Option<usize>) -> Result<()> {
    println!("T-SPIKE-002 — 3 pistas simultaneas durante {secs} s");
    let (sink, errors) = open_sink(dev)?;
    let mixer = sink.mixer();

    // Volumen bajo: son tres senos puros a la vez durante minutos.
    const GAIN: f32 = 0.04;

    let names = ["tone_a.wav", "tone_b.wav", "tone_c.wav"];
    let mut players = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let source = decode(name)?.amplify(GAIN).repeat_infinite();
        let player = Player::connect_new(mixer);
        player.append(source);
        println!("  pista {i}     : {name} (loop, ganancia {GAIN})");
        players.push(player);
    }

    let start = Instant::now();
    let total = Duration::from_secs(secs);
    let mut next_tick = Duration::from_secs(5);
    while start.elapsed() < total {
        thread::sleep(Duration::from_millis(100));
        if start.elapsed() >= next_tick {
            let vivos = players.iter().filter(|p| !p.empty()).count();
            println!(
                "  [t+{:>4}s] pistas_vivas={vivos} errores_stream={}",
                start.elapsed().as_secs(),
                errors.load(Ordering::SeqCst)
            );
            next_tick += Duration::from_secs(5);
        }
    }

    let errs = errors.load(Ordering::SeqCst);
    let vivos = players.iter().filter(|p| !p.empty()).count();
    let elapsed = start.elapsed().as_secs_f64();
    println!();
    println!("  duracion      : {elapsed:.1} s");
    println!("  pistas vivas  : {vivos}/3");
    println!("  errores stream: {errs}");
    println!();
    if errs != 0 {
        return Err(anyhow!("FALLO: el stream reporto {errs} errores"));
    }
    if vivos != 3 {
        return Err(anyhow!("FALLO: solo {vivos} de 3 pistas siguen vivas"));
    }
    println!("OK: 3 pistas simultaneas sin errores de stream");
    Ok(())
}

// ---------------------------------------------------------------------------
// T-SPIKE-003 — fade in de 0.1 s y de 60 s
// ---------------------------------------------------------------------------

fn cmd_fade(ms: u64, dev: Option<usize>) -> Result<()> {
    println!("T-SPIKE-003 — fade in de {ms} ms sobre tone_a.wav (6 s)");
    let (sink, _errors) = open_sink(dev)?;
    let dur = Duration::from_millis(ms);

    let source = decode("tone_a.wav")?.amplify(0.08);
    let faded = GainRamp::new(source, dur, 0.0, 1.0, Curve::EqualPower);
    println!("  rampa         : {} frames", faded.ramp_frames());

    let player = Player::connect_new(sink.mixer());
    player.append(faded);
    player.sleep_until_end();
    println!("OK: fade in de {ms} ms reproducido completo");
    Ok(())
}

/// Renderiza el fade a un WAV para analizarlo offline (anti-clic).
///
/// Si la rampa no cabe en el fixture (6 s), la fuente es un seno sintético
/// continuo en vez del WAV repetido: `repeat_infinite` sobre un seno de 440 Hz
/// corta a mitad de ciclo y ese salto de fase sí sería un clic real, que el
/// test detectaría como un falso positivo.
fn cmd_render_fade(ms: u64, out: &str) -> Result<()> {
    let dur = Duration::from_millis(ms);
    println!("T-SPIKE-003 — render de fade in de {ms} ms -> {out}");

    let fixture_len = Duration::from_secs(6);
    if dur + Duration::from_secs(1) > fixture_len {
        let total = dur + Duration::from_secs(2);
        println!(
            "  fuente        : seno sintetico de 440 Hz ({:.1} s, la rampa no cabe en el fixture)",
            total.as_secs_f64()
        );
        let sine = rodio::source::SineWave::new(440.0)
            .amplify(0.5)
            .take_duration(total);
        let faded = GainRamp::new(sine, dur, 0.0, 1.0, Curve::EqualPower);
        println!("  rampa         : {} frames", faded.ramp_frames());
        rodio::wav_to_file(faded, out).map_err(|e| anyhow!("no se pudo escribir el wav: {e}"))?;
    } else {
        println!("  fuente        : tests/fixtures/tone_a.wav (6 s, 440 Hz)");
        let faded = GainRamp::new(decode("tone_a.wav")?, dur, 0.0, 1.0, Curve::EqualPower);
        println!("  rampa         : {} frames", faded.ramp_frames());
        rodio::wav_to_file(faded, out).map_err(|e| anyhow!("no se pudo escribir el wav: {e}"))?;
    }
    println!("OK: escrito {out}");
    Ok(())
}

// ---------------------------------------------------------------------------
// T-SPIKE-004 — crossfade A -> B
// ---------------------------------------------------------------------------

fn cmd_xfade(secs: u64, dev: Option<usize>) -> Result<()> {
    println!("T-SPIKE-004 — crossfade tone_a -> tone_b de {secs} s");
    let (sink, _errors) = open_sink(dev)?;
    let dur = Duration::from_secs(secs);
    let mixer = sink.mixer();

    // A entra sonando y sale con fade out; B entra con fade in.
    let a = decode("tone_a.wav")?.amplify(0.08);
    let a_out = GainRamp::new(a, dur, 1.0, 0.0, Curve::EqualPower);
    let pa = Player::connect_new(mixer);
    pa.append(a_out);

    let b = decode("tone_b.wav")?.amplify(0.08);
    let b_in = GainRamp::new(b, dur, 0.0, 1.0, Curve::EqualPower);
    let pb = Player::connect_new(mixer);
    pb.append(b_in);

    pa.sleep_until_end();
    pb.sleep_until_end();
    println!("OK: crossfade de {secs} s reproducido completo");
    Ok(())
}

fn cmd_render_xfade(secs: u64, out: &str, curve: Curve) -> Result<()> {
    let dur = Duration::from_secs(secs);
    println!("T-SPIKE-004 — render de crossfade de {secs} s -> {out}");
    println!("  curva         : {curve:?}");

    let a = decode("tone_a.wav")?;
    let a_out = GainRamp::new(a.take_duration(dur), dur, 1.0, 0.0, curve);
    let b = decode("tone_b.wav")?;
    let b_in = GainRamp::new(b.take_duration(dur), dur, 0.0, 1.0, curve);
    let mixed = a_out.mix(b_in);

    rodio::wav_to_file(mixed, out).map_err(|e| anyhow!("no se pudo escribir el wav: {e}"))?;
    println!("OK: escrito {out}");
    Ok(())
}

// ---------------------------------------------------------------------------
// T-SPIKE-005 — loop infinito sin fuga de memoria
// ---------------------------------------------------------------------------

fn cmd_loop(mins: u64, dev: Option<usize>) -> Result<()> {
    println!("T-SPIKE-005 — loop infinito de tone_short.wav durante {mins} min");
    let (sink, errors) = open_sink(dev)?;

    let source = Decoder::new_looped(BufReader::new(File::open(fixture("tone_short.wav"))?))?
        .amplify(0.05);
    println!("  modo          : Decoder::new_looped (lee del disco, no bufferiza)");
    let player = Player::connect_new(sink.mixer());
    player.append(source);

    let start = Instant::now();
    let total = Duration::from_secs(mins * 60);
    let mut next_tick = Duration::from_secs(30);
    while start.elapsed() < total {
        thread::sleep(Duration::from_millis(200));
        if start.elapsed() >= next_tick {
            println!(
                "  [t+{:>4}s] posicion={:.2}s errores={}",
                start.elapsed().as_secs(),
                player.get_pos().as_secs_f64(),
                errors.load(Ordering::SeqCst)
            );
            next_tick += Duration::from_secs(30);
        }
    }

    let errs = errors.load(Ordering::SeqCst);
    println!();
    println!("  duracion        : {:.1} s", start.elapsed().as_secs_f64());
    println!("  posicion final  : {:.2} s", player.get_pos().as_secs_f64());
    println!("  errores stream  : {errs}");
    println!();
    if errs != 0 {
        return Err(anyhow!("FALLO: el stream reporto {errs} errores durante el loop"));
    }
    println!("OK: loop continuo sin errores de stream");
    Ok(())
}

// ---------------------------------------------------------------------------
// T-SPIKE-006 — parar una pista desde otro hilo
// ---------------------------------------------------------------------------

fn cmd_stop(ms: u64, dev: Option<usize>) -> Result<()> {
    println!("T-SPIKE-006 — parar una pista de 6 s a los {ms} ms desde otro hilo");
    let (sink, _errors) = open_sink(dev)?;

    let player = Arc::new(Player::connect_new(sink.mixer()));
    player.append(decode("tone_a.wav")?.amplify(0.08));
    println!("  pista en cola : {} fuente(s)", player.len());

    // El hilo de parada anota el instante exacto del stop(); el hilo principal
    // mide desde ahí, no desde que arrancó el programa.
    let stop_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let p2 = Arc::clone(&player);
    let s2 = Arc::clone(&stop_at);
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(ms));
        *s2.lock().unwrap() = Some(Instant::now());
        p2.stop();
    });

    // El hilo principal vigila cuánto tarda la cola en vaciarse.
    let mut latency_ms: Option<u128> = None;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Some(at) = *stop_at.lock().unwrap() {
            if player.empty() {
                latency_ms = Some(at.elapsed().as_millis());
                break;
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    handle.join().map_err(|_| anyhow!("el hilo de parada murio"))?;

    match latency_ms {
        Some(l) => {
            println!("  latencia      : {l} ms hasta cola vacia");
            println!();
            if l > 500 {
                return Err(anyhow!("FALLO: tardó {l} ms en parar (maximo 500 ms)"));
            }
            println!("OK: la pista se paró desde otro hilo en {l} ms");
            Ok(())
        }
        None => Err(anyhow!("FALLO: la cola nunca se vació tras stop()")),
    }
}

// ---------------------------------------------------------------------------
// T-FMT-004 — loop de un audio que vive dentro de un .tpshow
// ---------------------------------------------------------------------------

/// Igual que T-SPIKE-005 pero leyendo del paquete: comprueba que se puede
/// hacer loop sobre un audio embebido sin extraerlo a disco y sin fugas.
fn cmd_pkg_loop(tpshow: &str, entrada: &str, mins: u64, dev: Option<usize>) -> Result<()> {
    println!("T-FMT-004 — loop de '{entrada}' desde {tpshow} durante {mins} min");
    let (sink, errors) = open_sink(dev)?;

    let lector = ZipEntryReader::new(Path::new(tpshow), entrada)?;
    println!("  tamano        : {} bytes (STORE, sin extraer)", lector.size());

    let source = Decoder::new_looped(lector)?.amplify(0.05);
    let player = Player::connect_new(sink.mixer());
    player.append(source);

    let start = Instant::now();
    let total = Duration::from_secs(mins * 60);
    let mut next_tick = Duration::from_secs(30);
    while start.elapsed() < total {
        thread::sleep(Duration::from_millis(200));
        if start.elapsed() >= next_tick {
            println!(
                "  [t+{:>4}s] posicion={:.2}s errores={}",
                start.elapsed().as_secs(),
                player.get_pos().as_secs_f64(),
                errors.load(Ordering::SeqCst)
            );
            next_tick += Duration::from_secs(30);
        }
    }

    let errs = errors.load(Ordering::SeqCst);
    println!();
    println!("  duracion        : {:.1} s", start.elapsed().as_secs_f64());
    println!("  posicion final  : {:.2} s", player.get_pos().as_secs_f64());
    println!("  errores stream  : {errs}");
    println!();
    if errs != 0 {
        return Err(anyhow!("FALLO: el stream reporto {errs} errores durante el loop"));
    }
    println!("OK: loop desde el paquete sin errores de stream");
    Ok(())
}

// ---------------------------------------------------------------------------
// R16 — cuánto cuesta la interfaz sola
// ---------------------------------------------------------------------------

/// Ventana egui mínima: sin motor de audio, sin sesión, sin nada.
///
/// Sirve para separar, al medir memoria, lo que gasta la interfaz de lo que
/// gasta nuestro código. Se mide desde fuera con `scripts/watch_process.ps1`.
struct VentanaMinima {
    hasta: Instant,
}

impl eframe::App for VentanaMinima {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("Ventana mínima de egui: solo para medir memoria.");
        });
        if Instant::now() >= self.hasta {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn cmd_ventana(secs: u64) -> Result<()> {
    println!("R16 — ventana egui mínima durante {secs} s (sin audio ni sesión)");
    eframe::run_native(
        "TeatroPlayer - minima",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 600.0]),
            ..Default::default()
        },
        Box::new(move |_cc| {
            Ok(Box::new(VentanaMinima {
                hasta: Instant::now() + Duration::from_secs(secs),
            }))
        }),
    )
    .map_err(|e| anyhow!("no se pudo abrir la ventana: {e}"))
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn parse<T: std::str::FromStr>(args: &[String], pos: usize, what: &str) -> Result<T> {
    args.get(pos)
        .ok_or_else(|| anyhow!("falta el argumento {what}"))?
        .parse()
        .map_err(|_| anyhow!("{what} debe ser un numero"))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");

    match cmd {
        "outputs" => list_outputs(),
        "open" => open_and_tone(parse(&args, 2, "indice")?),
        "multi" => {
            let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
            let dev = args.get(3).and_then(|s| s.parse().ok());
            cmd_multi(secs, dev)
        }
        "fade" => {
            let ms: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
            cmd_fade(ms, args.get(3).and_then(|s| s.parse().ok()))
        }
        "render-fade" => {
            let ms: u64 = parse(&args, 2, "milisegundos")?;
            let out = args
                .get(3)
                .ok_or_else(|| anyhow!("falta la ruta del wav de salida"))?;
            cmd_render_fade(ms, out)
        }
        "xfade" => {
            let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
            cmd_xfade(secs, args.get(3).and_then(|s| s.parse().ok()))
        }
        "render-xfade" => {
            let secs: u64 = parse(&args, 2, "segundos")?;
            let out = args
                .get(3)
                .ok_or_else(|| anyhow!("falta la ruta del wav de salida"))?;
            let curve = match args.get(4).map(|s| s.as_str()) {
                Some("linear") => Curve::Linear,
                Some("exponential") => Curve::Exponential,
                _ => Curve::EqualPower,
            };
            cmd_render_xfade(secs, out, curve)
        }
        "loop" => {
            let mins: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
            cmd_loop(mins, args.get(3).and_then(|s| s.parse().ok()))
        }
        "stop" => {
            let ms: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2000);
            cmd_stop(ms, args.get(3).and_then(|s| s.parse().ok()))
        }
        "ventana" => {
            let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
            cmd_ventana(secs)
        }
        "pkg-loop" => {
            let tpshow = args
                .get(2)
                .ok_or_else(|| anyhow!("falta la ruta del .tpshow"))?;
            let entrada = args
                .get(3)
                .ok_or_else(|| anyhow!("falta el nombre de la entrada"))?;
            let mins: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(10);
            let dev = args.get(5).and_then(|s| s.parse().ok());
            cmd_pkg_loop(tpshow, entrada, mins, dev)
        }
        _ => {
            eprintln!("Spikes de validación de TeatroPlayer");
            eprintln!();
            eprintln!("  tp-spike outputs                         lista las salidas de audio");
            eprintln!("  tp-spike open <idx>                      tono de 440 Hz 1 s");
            eprintln!("  tp-spike multi [secs] [idx]              3 pistas simultaneas");
            eprintln!("  tp-spike fade <ms> [idx]                 fade in en vivo");
            eprintln!("  tp-spike render-fade <ms> <out.wav>      fade in a archivo");
            eprintln!("  tp-spike xfade [secs] [idx]              crossfade A->B en vivo");
            eprintln!("  tp-spike render-xfade <secs> <out.wav> [curva]");
            eprintln!("                                          crossfade a archivo");
            eprintln!("                                          curva: equalpower|linear|exponential");
            eprintln!("  tp-spike loop [mins] [idx]               loop infinito");
            eprintln!("  tp-spike stop [ms] [idx]                 parar desde otro hilo");
            eprintln!("  tp-spike ventana [secs]                 ventana mínima (medir RAM)");
            eprintln!("  tp-spike pkg-loop <tpshow> <entrada> [mins] [idx]");
            eprintln!("                                          loop desde un .tpshow");
            std::process::exit(2);
        }
    }
}
