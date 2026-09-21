//! Tests del logging a archivo (T-OPS-001).
//!
//! Está en su propio binario de test porque `iniciar()` instala el suscriptor
//! global de `tracing`, y eso sólo se puede hacer una vez por proceso.

use std::time::Duration;

use tracing::info;

#[test]
fn el_log_queda_escrito_en_disco() {
    let guardia = teatroplayer::diagnostico::iniciar();
    assert!(guardia.is_some(), "no se pudo abrir el archivo de log");

    info!("cue disparado: Ambiente");
    info!("cue disparado: Trueno");

    // El escritor es *no bloqueante* para no parar el hilo de audio: hay que
    // darle un momento para que vacíe la cola.
    std::thread::sleep(Duration::from_millis(400));

    let lineas = teatroplayer::diagnostico::ultimas_lineas(50);
    let texto = lineas.join("\n");

    assert!(
        texto.contains("cue disparado: Ambiente"),
        "el log no tiene el primer cue:\n{texto}"
    );
    assert!(
        texto.contains("cue disparado: Trueno"),
        "el log no tiene el segundo cue:\n{texto}"
    );

    // Cada línea lleva su marca de tiempo: sin hora, un log de teatro no sirve.
    assert!(
        lineas.iter().any(|l| l.contains("INFO")),
        "las líneas deben llevar el nivel:\n{texto}"
    );

    drop(guardia); // fuerza el volcado final
}

#[test]
fn la_rotacion_esta_configurada_a_tres_dias() {
    assert_eq!(teatroplayer::diagnostico::ROTACION_DIAS, 3);
}
