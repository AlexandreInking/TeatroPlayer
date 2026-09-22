# 09 — Riesgos, supuestos y decisiones abiertas

## 1. Riesgos técnicos

| # | Riesgo | Impacto | Prob. | Mitigación |
|---|---|---|---|---|
| R1 | **rodio 0.22 cambia de API** (partes marcadas `experimental`, historial de breaking changes 0.20 → 0.21 → 0.22) | Alto | Media | Versión fijada en `Cargo.lock`; rodio encapsulado detrás del trait `AudioBackend`; **hito SPIKE completado 8/8 (2026-09-20): rodio validado en la máquina real** (3 pistas 300 s sin errores, fades sin clics, loop 600 s sin fuga, corte en 8 ms). Ver `08-plan-de-implementacion.md` |
| R2 | **`Mixer` no permite controlar una pista ya agregada** | ~~Alto~~ **Resuelto** | ~~Cierto~~ **Falso, premisa corregida** | **Corregido en T-SPIKE-006 (2026-09-20):** `Player::connect_new(&mixer)` **sí** devuelve un handle por pista con `stop()`, `set_volume()` y `get_pos()`, y es `Send + Sync`. Medido: 8 ms hasta cola vacía. Ya no hace falta el `stop_flag` propio; el único código propio que queda es la **curva** del fade (`GainRamp`, ~150 líneas). Ver `02-stack-tecnico.md` §5.1 |
| R3 | **Glitches por decodificación** en laptops lentas con muchos MP3 | Alto | Media | `.buffered()` con pre-buffer de 200 ms; recomendar WAV/FLAC en la UI; **CPU medida en T-SPIKE-002: 7.51 % de un núcleo con 3 pistas en compilación debug** (presupuesto < 8 %), y 4.71 % en el loop de T-SPIKE-005 |
| R4 | **Salida de audífonos no seleccionable** si cpal/WASAPI no la expone aparte | Medio | **Confirmado** | **Verificado en T-SPIKE-001 (2026-09-20):** en la laptop de desarrollo cpal enumera 4 endpoints (Realtek Digital Output, Altavoces vía S/PDIF ×2, MACROSILICON) y **ninguno se llama "Auriculares"**. La ficha de audífonos comparte dispositivo con los altavoces y Windows cambia el endpoint al enchufar el cable. **Mitigación adoptada:** el selector de salida ofrece además **"Salida predeterminada de Windows"** (recomendada), y se debe detectar el cambio de endpoint en caliente (T-SHOW-006) reabriendo el stream. Documentar en el tutorial: enchufar el cable **antes** de abrir el programa |
| R5 | **Latencia alta** en WASAPI shared | Medio | Media | Buffer de 256–512 frames; medir en M0; si hiciera falta, evaluar WASAPI exclusivo (pierde compartir audio con el sistema, normalmente deseable en teatro) |
| R6 | **MP3 con loop saltan** (padding del codificador) | Bajo | Alta | Aviso en la UI; recomendar WAV/FLAC para loops |
| R7 | **egui no se ve "nativo"** en Windows | Bajo | Media | No importa: la prioridad es simplicidad y peso. Diálogos de archivo sí son nativos (`rfd`) |
| R8 | **Tamaño del binario** se escapa (wgpu, fonts, symphonia-all) | Medio | Media | Backend `glow` en vez de `wgpu`; `cargo bloat` en CI; si symphonia-all pesa mucho, recortar a `wav,mp3,flac,ogg` |
| R9 | **SmartScreen bloquea el exe sin firmar** | Medio | Alta | Documentar "Más información → Ejecutar de todas formas"; evaluar certificado OV más adelante |
| R10 | Alguien cierra la laptop / se suspende en función | Alto | Media | Aviso al entrar en modo Función sugiriendo desactivar suspensión (P2); reconexión automática de audio al volver |
| R11 | **Ya existe un competidor libre**: LivePlay (AGPL, Win/Mac/Linux) y ZasCue (gratis, 68 MB) cubren parte del hueco | Medio | Cierto | Diferenciarse por peso, arranque, ausencia de red/servidor, español y curva mínima (`10-competencia-y-benchmark.md` §4). **No competir en cantidad de funciones.** Su código AGPL no se incorpora |
| R12 | **Inflado de alcance**: el benchmark encontró 34 funciones tentadoras; sumarlas todas convierte esto en QLab | Alto | Alta | Matriz de adopción cerrada en `10` §3 con 16 adoptadas y 18 rechazadas; regla del proyecto: *si no se explica en una frase a un no-técnico, no entra* |
| R13 | **Fork cerrado** que venda el programa | — | — | **Resuelto por la licencia:** con GPL-3.0 (`11-open-source-y-licencias.md`) un fork cerrado es un incumplimiento; al distribuir debe publicar el código. Residual: la marca no la cede la licencia, así que los forks deben usar otro nombre |
| R14 | **`take_duration` no corta exacto**: corta por *spans*, así que puede pasarse unos samples (medido: `take_duration(5 s)` sobre un WAV de 44100 Hz da 220507 muestras en vez de 220500, +0.16 ms) | Bajo | Cierto (descubierto en T-SPIKE-004) | No afecta al MVP porque el fade no usa `take_duration`: la rampa es por índice de frame dentro del source completo. Si en el futuro se añade "parar a los N segundos", contar frames en el envolvente en vez de usar `take_duration` |
| R15 | **El limitador de rodio no garantiza 0 dBFS**: es *feed-forward* sin lookahead, así que durante el ataque se escapa un sobrepico. Medido con una mezcla de -3 dBFS y +6 dBFS: con `LimitSettings::default()` (-1 dB, 5 ms) el pico del ataque llega a **1.91**, es decir recorta justo en lo que debe evitar | Alto (afecta a la calidad) | Cierto, medido en T-ENG-004 | **Resuelto con ajuste, no con código propio:** umbral **-3 dBFS** y ataque **50 µs** (`master_limit_settings()`), con lo que el peor caso baja a **0.85** y a niveles normales de obra el limitador ni actúa. Si en el futuro se quiere garantía matemática, hace falta un limitador con lookahead, que rodio no trae |
| R16 | **RAM por encima del presupuesto**: la app con interfaz consume **141 MB** en reposo (181 MB privados) frente a los < 60 MB de `Docs/01`. Sin interfaz, `tp-spike` usa 15 MB, así que el coste es de eframe + glow/OpenGL, no del motor de audio | ~~Medio~~ **Cerrado** | ~~Cierto~~ **Resuelto por decisión** | **Investigado, medido y cerrado el 2026-09-22.** Se añadió `tp-spike ventana`, una ventana egui **vacía**: consume **139,9 MB** estables. La app completa se estabiliza en **135–141 MB** (arranca en ~98 MB y sube en los primeros segundos: medir demasiado pronto engaña), así que **todo lo construido está dentro del ruido de medida** y el coste es eframe + glow/OpenGL. De las tres salidas se eligió **(a): subir el presupuesto a < 150 MB** en `Docs/01` §7 con la razón documentada. Se descartó (b) probar `wgpu` —añade una dependencia pesada y probablemente no mejore— y (c) cambiar de toolkit, que es rehacer la interfaz entera. La promesa de "ultraligero" se reformula: el binario son 7,7 MB y el motor 15 MB; lo que pesa es la ventana. Nota: medido con una AMD RX 9060 XT; en otro equipo la cifra puede variar |




## 2. Supuestos asumidos en este diseño

Se tomaron para poder avanzar; **confirmar antes de M3**:

1. **Windows es la plataforma objetivo** (10 y 11). El diseño no impide macOS/Linux, pero no se construyen en el MVP.
2. **Idioma de la interfaz: español** (rioplatense/neutro).
3. **Estéreo, 2 canales**, una sola salida a la vez.
4. **La laptop es de gama baja a media** y puede no tener SSD → el presupuesto de recursos es conservador.
5. **No hay internet en la sala** → cero dependencia de red.
6. **Seis a cuarenta entradas** por obra típica. Más de 200 entradas no es un caso optimizado en el MVP.
7. **Un solo operador, una sola laptop.**

## 3. Decisiones abiertas (hay que elegir antes de programar)

| # | Decisión | Opciones | Recomendación |
|---|---|---|---|
| D1 | ¿El MVP es solo Windows o ya multiplataforma? | Solo Windows / Win + macOS | **Solo Windows**. Rust lo hace portable igual, pero empaquetar Mac requiere firma y notarización (costo real) |
| D2 | ¿La sesión es una carpeta o un único archivo? | Carpeta autocontenida / un solo `.tpshow` con los audios incrustados | **Carpeta** (más simple, audios accesibles); añadir `.tpk` (ZIP) como "empaquetar para mandar" en P2 |
| D3 | ¿Copiar los audios dentro de la sesión al importar? | Siempre copiar / preguntar / nunca | **Preguntar, con "Sí" por defecto** (hace la sesión portable, que es el punto) |
| D4 | ¿Se muestra el volumen en dB o en porcentaje? | dB / % | **dB** en modo Diseño, **sin número** en modo Función |
| D5 | ¿Crossfade se configura en la entrada que entra o en la que sale? | En la que entra / en la que sale | **En la que entra.** Es como se piensa en la práctica y evita editar la pista anterior |
| D6 | ¿Instalador NSIS (cargo-packager) o Inno Setup? | NSIS / Inno Setup | **NSIS vía cargo-packager** por automatización; Inno Setup es el plan B inmediato |
| D7 | ¿Se dibuja la forma de onda en la fila? | Sí / No | **No en el MVP** (P2): cuesta caché y CPU, no aporta a la operación |
| D8 | ¿Nombre del producto? | TeatroPlayer / Entradas / CueSimple | **TeatroPlayer** (ya es el nombre del proyecto; "Entradas" puede ser el término interno para una cue) |

## 4. Preguntas para el usuario

1. ¿Confirmamos **solo Windows** para la primera versión?
2. ¿La laptop de teatro tiene **salida de audífonos dedicada** o usás una interfaz USB? (afecta FR-08)
3. ¿Pensás distribuir el programa a **otras personas/compañías** o es para uso propio? (afecta firma de código y licencia)
4. ¿Necesitás que **dos audios suenen a la vez** sí o sí (se encimen), o siempre es uno que reemplaza al otro? (ya está soportado, pero define el valor por defecto de `onPrevious`)
5. ¿Te interesa que en el futuro dispare por **MIDI o por reloj** (tiempos automáticos)? (define si el trait `AudioBackend` crece o no)

## 5. Fuera de alcance explícito (no se hará)

- Editar/recortar audio, grabar, normalizar.
- Efectos (reverb, EQ, compresión).
- Multicanal, VST, MIDI, OSC, red.
- Video, subtítulos, control de luces.
- Cuentas de usuario, nube, sincronización.
- macOS / Linux en el MVP.
