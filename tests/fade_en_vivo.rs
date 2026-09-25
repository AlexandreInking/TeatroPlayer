//! El fade medido** sobre la cadena de verdad, sin tarjeta de sonido.
//!
//! Aquí no se prueba una envolvente aislada: se monta la cadena que construye
//! el programa (`cadena_de_audio`) sobre un `Player` conectado a un `Mixer`,
//! que es exactamente la topología de producción (ADR-002), y se miden las
//! muestras que salen. El mixer no necesita dispositivo: se le tira de las
//! muestras a mano.
//!
//! Importa porque el síntoma que se reportó era "el audio entra de golpe y no
//! se respeta el tiempo del fade". Con este test eso se ve o no se ve, sin
//! tener que estar en una sala: si alguien toca la cadena y el fade se pierde,
//! aquí salta.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rodio::mixer::mixer;
use rodio::{ChannelCount, Player, SampleRate};
use teatroplayer::engine::model::{AudioSource, CueSpec, Curve, Entrance, LoopMode};
use teatroplayer::engine::rodio_backend::cadena_de_audio;

fn fixture(nombre: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(nombre)
}

/// Monta la cadena real en un mixer y devuelve la ganancia medida cada `tramo`.
fn medir_ganancia(
    spec: &CueSpec,
    wav: &Path,
    segundos: f32,
    tramo_ms: u32,
    rate: u32,
    canales: u16,
) -> Vec<f32> {
    let (mix, mut salida) = mixer(
        ChannelCount::new(canales).expect("canales válidos"),
        SampleRate::new(rate).expect("rate válido"),
    );
    let player = Player::connect_new(&mix);
    let (cadena, _control) =
        cadena_de_audio(spec, AudioSource::File(wav.to_path_buf())).expect("la cadena se construye");
    player.append(cadena);

    // Muestras (no frames) por tramo: `salida.next()` devuelve una muestra de
    // **un** canal, así que en estéreo hay que multiplicar por dos o el tramo
    // duraría la mitad de lo pedido y las cuentas saldrían todas mal.
    let por_tramo = (rate as f64 * tramo_ms as f64 / 1000.0) as usize * canales as usize;
    let tramos = (segundos * 1000.0 / tramo_ms as f32) as usize;
    let mut rms: Vec<f32> = Vec::with_capacity(tramos);

    for _t in 0..tramos {
        let mut acc = 0.0f64;
        let mut n = 0usize;
        for i in 0..por_tramo {
            match salida.next() {
                Some(s) => {
                    // Sólo el primer canal: con estéreo los dos llevan la misma
                    // ganancia y valdría cualquiera.
                    if i % canales as usize == 0 {
                        acc += (s as f64) * (s as f64);
                        n += 1;
                    }
                }
                None => break,
            }
        }
        rms.push(if n == 0 { 0.0 } else { (acc / n as f64).sqrt() as f32 });
    }

    // Normalizado al techo: lo que interesa es la forma de la rampa, no el
    // volumen al que esté el wav de prueba.
    let techo = rms.iter().cloned().fold(0.0f32, f32::max);
    assert!(techo > 0.0, "no salió ni una muestra: la cadena está muda");
    for v in rms.iter_mut() {
        *v /= techo;
    }
    rms
}

fn spec_fade_in(duracion: Duration, en_bucle: bool) -> CueSpec {
    CueSpec {
        entrance: Entrance::FadeIn {
            duration: duracion,
            curve: Curve::Linear,
            from_percent: 0,
            to_percent: 100,
        },
        loop_mode: if en_bucle { LoopMode::Infinite } else { LoopMode::None },
        ..CueSpec::default()
    }
}

/// Un fade de 3 s tarda 3 s: ni entra de golpe ni se pasa de largo.
#[test]
fn el_fade_in_dura_lo_que_se_pide() {
    let wav = fixture("tone_a.wav"); // 6 s, 44.100 Hz, mono
    let rms = medir_ganancia(
        &spec_fade_in(Duration::from_secs(3), false),
        &wav,
        5.0,
        500,
        44_100,
        1,
    );

    // Tramos de 0,5 s: a los 0 s casi nada, a los 1,5 s la mitad, a los 3 s el
    // techo. Se tolera un 8 % porque el primer tramo arranca en 0 y el RMS
    // integra media rampa ya subida.
    let esperado = [0.09, 0.25, 0.42, 0.58, 0.75, 0.92, 1.0, 1.0, 1.0, 1.0];
    assert_eq!(rms.len(), esperado.len(), "se esperaban {} tramos", esperado.len());
    for (i, (medido, espera)) in rms.iter().zip(esperado.iter()).enumerate() {
        assert!(
            (medido - espera).abs() < 0.08,
            "tramo {i} (t={}s): se midió {medido:.3} y se esperaba {espera:.2}",
            i as f32 * 0.5,
        );
    }
}

/// Con la salida a 48 kHz en estéreo —lo normal en una tarjeta— el fade sigue
/// durando lo mismo. Si el remuestreo se comiera la rampa, aquí se vería.
#[test]
fn el_fade_dura_igual_aunque_la_tarjeta_vaya_a_otro_rate() {
    let wav = fixture("tone_a.wav");
    let rms = medir_ganancia(
        &spec_fade_in(Duration::from_secs(3), false),
        &wav,
        4.0,
        500,
        48_000,
        2,
    );

    assert!(rms[0] < 0.20, "arranca casi en silencio, vale {}", rms[0]);
    // A los 3 s ya está en el techo...
    assert!(rms[6] > 0.90, "a los 3 s debe estar arriba, vale {}", rms[6]);
    // ...y a los 1,5 s va por la mitad.
    assert!(
        (rms[3] - 0.5).abs() < 0.15,
        "a los 1,5 s debe ir por la mitad, vale {}",
        rms[3]
    );
}

/// En bucle (que es como nacen los eventos) el fade también se hace.
///
/// Es la variante que más se usa: un ambiente en loop que entra con fade. Si el
/// decoder en bucle reportara mal su sample rate, `duration_to_frames` daría 0
/// y la rampa desaparecería: sonarían de golpe.
#[test]
fn el_fade_se_hace_tambien_en_bucle() {
    let wav = fixture("tone_a.wav");
    let rms = medir_ganancia(
        &spec_fade_in(Duration::from_secs(2), true),
        &wav,
        3.0,
        500,
        44_100,
        1,
    );

    assert!(rms[0] < 0.20, "el bucle también arranca en silencio, vale {}", rms[0]);
    assert!(rms[4] > 0.92, "a los 2 s debe estar en el techo, vale {}", rms[4]);
}

/// Un fade corto (300 ms) sigue siendo un fade, no un golpe.
#[test]
fn un_fade_corto_no_se_convierte_en_golpe() {
    let wav = fixture("tone_a.wav");
    let rms = medir_ganancia(
        &spec_fade_in(Duration::from_millis(300), false),
        &wav,
        1.0,
        100,
        44_100,
        1,
    );

    assert!(rms[0] < 0.45, "el primer tramo ya va subiendo, vale {}", rms[0]);
    assert!(rms[3] > 0.90, "a los 300 ms está arriba, vale {}", rms[3]);
}

/// Sin fade (`Hit`) el audio entra a pleno volumen desde la primera muestra.
///
/// Es el contrapunto: si este test y el primero dieran lo mismo, la envolvente
/// no estaría haciendo nada.
#[test]
fn sin_fade_el_audio_entra_de_golpe() {
    let wav = fixture("tone_a.wav");
    let rms = medir_ganancia(&CueSpec::simple(), &wav, 1.0, 100, 44_100, 1);
    assert!(rms[0] > 0.90, "sin fade, la primera muestra ya está al techo: {}", rms[0]);
    // Y no hay rampa: desde el segundo tramo la ganancia es plana. El primero
    // sale un poco por debajo por el ataque del propio wav de prueba, no por la
    // envolvente; si aquí hubiera un fade, la curva subiría tramo a tramo.
    for par in rms[1..].windows(2) {
        assert!(
            (par[1] - par[0]).abs() < 0.02,
            "sin fade la ganancia es plana: {rms:?}"
        );
    }
}

/// El lector de la cadena se usa de verdad: un wav que no existe da error y no
/// se cuelga.
#[test]
fn un_audio_que_no_existe_da_error_y_no_panica() {
    let err = cadena_de_audio(
        &CueSpec::simple(),
        AudioSource::File(fixture("no_existe.wav")),
    );
    assert!(err.is_err(), "debe fallar al abrir, no sonar en silencio");
}

/// **La pausa congela el fade a medias.**
///
/// Es la regla que pidió el usuario: pausar en medio de un fade no puede
/// saltarse el fade ni cortar la pista. Se comprueba sobre la cadena real y con
/// el `Player` de rodio, que es quien recibe el `pause()` del reproductor.
///
/// El detalle que lo hace funcionar: `Player::pause` no saca el source de la
/// cola, deja de pedirle muestras. Como la envolvente cuenta **frames**, si no
/// se consumen muestras la rampa no avanza — y `rampa_terminada()` se queda en
/// `false`, que es lo que impide que el vigilante corte la pista por debajo.
#[test]
fn la_pausa_congela_el_fade_a_medias() {
    let wav = fixture("tone_a.wav"); // 6 s, para que no se acabe antes de tiempo
    let (mix, mut salida) = mixer(
        ChannelCount::new(1).unwrap(),
        SampleRate::new(44_100).unwrap(),
    );
    let player = Player::connect_new(&mix);
    let (cadena, control) =
        cadena_de_audio(&CueSpec::simple(), AudioSource::File(wav)).expect("cadena montada");
    player.append(cadena);

    // Consumir `ms` milisegundos de audio.
    let consumir = |ms: u32, salida: &mut rodio::mixer::MixerSource| {
        for _ in 0..(44_100 * ms / 1000) {
            if salida.next().is_none() {
                break;
            }
        }
    };

    consumir(250, &mut salida);
    assert!((control.gain() - 1.0).abs() < 0.01, "suena a plena ganancia");

    // Fade out de 1 s y dejar que se recorra medio segundo.
    control.fade_out(Duration::from_secs(1), Curve::Linear);
    consumir(500, &mut salida);
    let a_medias = control.gain();
    assert!(
        (a_medias - 0.5).abs() < 0.08,
        "a mitad de rampa la ganancia va por 0.5, vale {a_medias}"
    );
    assert!(!control.rampa_terminada(), "la rampa no ha terminado todavía");

    // PAUSA. Se tira de más muestras —el reloj de pared no cuenta— y la
    // ganancia no se puede mover.
    player.pause();
    consumir(300, &mut salida); // margen para que el pause surta efecto
    let en_pausa = control.gain();
    consumir(1_500, &mut salida); // 1,5 s "de reloj" con la pista congelada

    assert!(
        (control.gain() - en_pausa).abs() < 0.01,
        "en pausa la ganancia no se mueve: {en_pausa} -> {}",
        control.gain()
    );
    assert!(
        (control.gain() - 0.5).abs() < 0.12,
        "sigue a media rampa, vale {}",
        control.gain()
    );
    assert!(
        !control.rampa_terminada(),
        "en pausa la rampa NO puede darse por terminada: es lo que evita que la pista se corte sola"
    );

    // Reanudar: la rampa sigue donde estaba y ahora sí termina.
    player.play();
    consumir(1_000, &mut salida);
    assert!(
        control.rampa_terminada(),
        "al reanudar, el medio segundo que faltaba se recorre"
    );
    assert!(control.gain() < 0.02, "y la ganancia llega a cero, vale {}", control.gain());
}

/// Comprobación de sanidad del propio utilitario: si el medidor estuviera roto
/// y no midiera nada, los tests anteriores pasarían sin haber medido.
#[test]
fn el_medidor_realmente_mide() {
    let wav = fixture("tone_a.wav");
    // Un fade de 60 s sobre un wav de 6 s: la rampa nunca llega al techo, así
    // que la curva tiene que subir todo el rato y acabar muy por debajo de 1.
    let rms = medir_ganancia(
        &spec_fade_in(Duration::from_secs(60), false),
        &wav,
        4.0,
        500,
        44_100,
        1,
    );
    for par in rms.windows(2) {
        assert!(par[1] > par[0], "la rampa debe seguir subiendo: {rms:?}");
    }
    // A los 4 s de un fade de 60 s se va por el 7 % del recorrido, no por la
    // mitad: si esto diera cerca de 1, el medidor no estaría midiendo la rampa.
    assert!(rms[0] < 0.20, "el primer tramo está casi en silencio, vale {}", rms[0]);
}
