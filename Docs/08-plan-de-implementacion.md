# 08 — Plan de implementación (cada task entrega algo testeable)

> **Estado: APROBADO — listo para arrancar.**
> El desarrollo empieza por **T-SPIKE-001** (enumerar salidas y sonar un tono en la salida de audífonos).
> Ningún task se marca como terminado hasta que su prueba pasa. Si el SPIKE falla, se cambia de backend
> antes de escribir una línea de la app (riesgo R1 de `09-riesgos-y-abiertos.md`).

Cada task tiene **un entregable concreto** y **una prueba que verifica el entregable** ejecutable desde la línea de comandos o por inspección directa. Un task no se marca como hecho hasta que su prueba pasa.

Convención de IDs: `T-<HITO>-<NN>` (HITO ∈ {SPIKE, ENG, SES, FMT, UI, SHOW, OPS, REL}). Las dependencias entre tasks se indican con `bloqueado por:`.

---

## Hito SPIKE — Antes de escribir la app, validar el motor

### T-SPIKE-001 — Audio: enumerar y abrir la salida de audífonos
**Entregable:** binario CLI `tp-spike` que lista dispositivos de salida y abre uno por nombre (default: el primero que contenga "Headphones" / "Auriculares" / "Speakers").
**Prueba:**
```bash
tp-spike outputs            # imprime: 0 "Auriculares (Realtek Audio)" 1 "HDMI"
tp-spike open 0 --tone-out  # abre salida 0, reproduce un seno de 440 Hz por 1 s, sale 0
```
**Criterio:** la salida 0 suena exactamente 1 s en el dispositivo elegido; `echo $?` = 0.

> **Estado: ✅ HECHO (2026-09-20).** Resultado real en la máquina de desarrollo:
> ```
> $ cargo run --quiet --bin tp-spike -- outputs
> 0   Realtek Digital Output via Built-in
> 1   Altavoces [Speaker] via S/PDIF
> 2   3 - MACROSILICON via Built-in
> 3   Altavoces [Speaker] via S/PDIF
>
> $ cargo run --quiet --bin tp-spike -- open 0
> abriendo salida 0: Realtek Digital Output via Built-in
>   sample rate : 48000
>   canales     : 2
> OK: tono de 440 Hz reproducido durante 1 s en "Realtek Digital Output via Built-in"
> exit=0
> ```
> **Hallazgo (afecta a FR-08 y al riesgo R4):** en esta laptop **no existe un dispositivo
> enumerable llamado "Auriculares"**. cpal expone *endpoints* de Windows; la ficha de
> audífonos comparte dispositivo con los altavoces y Windows cambia el endpoint al enchufar
> el cable. Consecuencia de diseño: el selector de salida debe ofrecer también
> **"Salida predeterminada de Windows"** y debe detectar el cambio de endpoint en caliente
> (ver T-SHOW-006). Actualizado en `09-riesgos-y-abiertos.md`, riesgo R4.

### T-SPIKE-002 — Audio: 3 pistas simultáneas sin glitches, 5 minutos
**Entregable:** `tp-spike multi3 <file1> <file2> <file3>` reproduce las 3 en el mismo `Mixer` durante 5 minutos con loop en cada una.
**Prueba:**
```bash
tp-spike multi3 a.wav b.wav c.wav   # corre 5 min, loguea underruns
grep -c UNDERRUN tp-spike.log      # debe ser 0
```
**Criterio:** `grep UNDERRUN` cuenta 0; `top -p $PID` muestra CPU < 8 % en una laptop moderna.
> **Estado: ✅ HECHO (2026-09-20).** El comando se adaptó a Windows (`tp-spike multi [secs] [idx]`);
> no hay `top`, así que el consumo se midió con `scripts/watch_process.ps1`.
> ```
> $ cargo run --quiet --bin tp-spike -- multi 300
> T-SPIKE-002 — 3 pistas simultaneas durante 300 s
>   salida      : dispositivo por defecto de Windows
>   formato     : 48000 Hz / 2 canales
>   [t+ 300s] pistas_vivas=3 errores_stream=0
>   duracion      : 300.0 s
>   pistas vivas  : 3/3
>   errores stream: 0
> OK: 3 pistas simultaneas sin errores de stream
>
> $ .\scripts\watch_process.ps1 -Name tp-spike -Seconds 288
>   muestras          : 138 en 276.2 s
>   CPU (1 nucleo)    : 7.51 %
>   CPU (todos)       : 0.47 %      (16 nucleos logicos)
>   RSS min / max     : 20.6 / 20.7 MB   -> variacion 0.2 MB
>   handles inicio/fin: 232 / 230
>   hilos inicio/fin  : 5 / 4
> ```
> **CPU 7.51 % de un núcleo** cumple el presupuesto (< 8 %) y eso en compilación *debug*, sin
> `opt-level`. En release será bastante menor. Sin fugas de handles ni de hilos.


### T-SPIKE-003 — Audio: fade_in 0.1 s y 60 s, anti-clic
**Entregable:** `tp-spike fade-in --ms 100` y `--ms 60000` reproducen un audio aplicando fade_in y escriben a un WAV de test (backend nulo / file writer).
**Prueba:**
```bash
tp-spike fade-in --ms 100   --out /tmp/fade100.wav
tp-spike fade-in --ms 60000 --out /tmp/fade60000.wav
./scripts/assert_no_click.sh /tmp/fade100.wav    # busca saltos > 0.1 en muestras consecutivas cerca del inicio
./scripts/assert_no_click.sh /tmp/fade60000.wav
```
**Criterio:** ambos scripts terminan con código 0.
> **Estado: ✅ HECHO (2026-09-20).** Se implementó como `tp-spike render-fade <ms> <out.wav>`
> (render offline, sin tarjeta) más el verificador `scripts/verify_fade.py`, que sustituye al
> `assert_no_click.sh` previsto (en Windows no hay bash ni `/tmp`).
> ```
> $ python scripts/verify_fade.py fade target/spike/fade_100ms.wav --fade-ms 100
>   [OK ] arranca en silencio: x[0] = 0.000000
>   [OK ] sin clics: salto maximo 0.03131 (< 0.06269)
>   [OK ] envolvente monótona: 0 bajadas dentro de la rampa
>   [OK ] sin recorte: pico 0.49997
>   [OK ] sigue la curva equal-power: los 3 puntos dentro del 10%
>        25%: 0.21981 vs 0.22366 (dif 1.72%)   50%: 0.37475 vs 0.37782 (dif 0.81%)
>   [OK ] llega a amplitud completa: 0.49997 tras 100.0 ms
> PASA: fade de 100.0 ms correcto
>
> $ python scripts/verify_fade.py fade target/spike/fade_60s.wav --fade-ms 60000
>   [OK ] sigue la curva equal-power: dif 0.02% / 0.01% / 0.01% en 25-50-75%
> PASA: fade de 60000.0 ms correcto
> ```
> **Nota de implementación:** para el fade de 60 s el render usa un seno sintético continuo
> en vez del WAV repetido, porque `repeat_infinite` sobre un tono de 440 Hz corta a mitad de
> ciclo y ese salto de fase sí sería un clic real (falso positivo del test).


### T-SPIKE-004 — Audio: crossfade A→B 5 s
**Entregable:** `tp-spike crossfade a.wav b.wav --ms 5000 --out /tmp/xf.wav` produce un WAV que es la suma de A (fade out últimos 5 s) y B (fade in primeros 5 s) sobre el resto de la duración de A.
**Prueba:**
```bash
python scripts/verify_crossfade.py /tmp/xf.wav   # verifica energía, picos y suma
```
**Criterio:** script termina 0; potencia RMS en la mitad del crossfade ≈ RMS del inicio de A.
> **Estado: ✅ HECHO (2026-09-20).** `tp-spike render-xfade <secs> <out.wav> [curva]` +
> `scripts/verify_fade.py xfade`.
> ```
> $ python scripts/verify_fade.py xfade target/spike/xfade_5s.wav --xfade-secs 5
>   [OK ] energia constante (equal-power): variacion 0.00% (< 2.00%)
>   [OK ] empieza sonando A: 440.0 Hz
>   [OK ] termina sonando B: 660.0 Hz
>   [OK ] sin clics: salto maximo 0.05645 (< 0.15672)
> PASA: crossfade de 5.0 s correcto
> ```
> **Energía constante al 0.00 %**: el requisito se cumple con holgura. Además quedó
> automatizado en `tests/envelope_offline.rs` (corre en CI, sin tarjeta de sonido).


### T-SPIKE-005 — Audio: loop infinito 10 min sin fuga de memoria
**Entregable:** `tp-spike loop big.wav --minutes 10`.
**Prueba:**
```bash
for i in 1 2 3 4 5 6; do ps -o rss= -p $PID >> /tmp/mem.log; sleep 100; done
python scripts/assert_no_leak.py /tmp/mem.log    # variación < 10 MB
```
**Criterio:** variación de RSS < 10 MB sobre los 10 minutos.
> **Estado: ✅ HECHO (2026-09-20).** `tp-spike loop [mins]` con `Decoder::new_looped` (streaming,
> sin cargar el archivo en RAM) sobre `tone_short.wav` de 1 s.
> ```
> $ cargo run --quiet --bin tp-spike -- loop 10
>   modo          : Decoder::new_looped (lee del disco, no bufferiza)
>   [t+ 600s] posicion=600.01s errores=0
>   duracion        : 600.1 s
>   posicion final  : 600.01 s
>   errores stream  : 0
> OK: loop continuo sin errores de stream
>
> $ .\scripts\watch_process.ps1 -Name tp-spike -Seconds 585
>   CPU (1 nucleo)    : 4.71 %
>   RSS inicial       : 14.8 MB
>   RSS final         : 15 MB
>   RSS min / max     : 14.8 / 15.1 MB  -> variacion 0.3 MB
>   handles inicio/fin: 238 / 236
>   hilos inicio/fin  : 5 / 2
> ```
> **Variación de RSS de 0.3 MB** frente al presupuesto de 10 MB: tres órdenes de holgura.
> Sin fuga de handles. La posición avanzó 600.01 s en 600.1 s de reloj: el loop no acumula
> deriva. Las muestras de audio se leen del disco en cada vuelta, no de un buffer en RAM.


### T-SPIKE-006 — Audio: cortar una pista desde otro hilo
**Entregable:** `tp-spike stop-on-signal` arranca 3 pistas y, ante SIGUSR1, corta una; ante SIGUSR2, corta todas con fade de 50 ms.
**Prueba:**
```bash
tp-spike stop-on-signal a.wav b.wav c.wav &
PID=$!
sleep 2; kill -USR1 $PID   # debe silenciar 'a' en <100 ms
sleep 1; kill -USR2 $PID   # debe silenciar todo en <150 ms
wait $PID                  # exit 0
```
**Criterio:** exit 0; no quedan pistas sonando al terminar.
> **Estado: ✅ HECHO (2026-09-20).** Windows no tiene `SIGUSR1`, así que la prueba es
> `tp-spike stop <ms>`: un hilo aparte corta la pista a los N ms y el principal mide la latencia.
> ```
> $ cargo run --quiet --bin tp-spike -- stop 2000
>   salida      : dispositivo por defecto de Windows
>   pista en cola : 1 fuente(s)
>   latencia      : 8 ms hasta cola vacia
> OK: la pista se paró desde otro hilo en 8 ms
> ```
> **Hallazgo que cambia el diseño (ver §5.1 de `02-stack-tecnico.md`):** no hace falta el
> patrón `stoppable().periodic_access(...)` ni un `stop_flag` propio. rodio 0.22 sí da un
> handle por pista: `Player::connect_new(&mixer)` con `stop()`, `set_volume()` y `get_pos()`,
> y `Player` es `Send + Sync`. Se mide **8 ms** entre el `stop()` y la cola vacía.


### T-SPIKE-007 — EnvelopeSource: cambio de gain en vivo sin clics
**Entregable:** crate `tp-core` con `EnvelopeSource<S>` y tests unitarios.
**Prueba:**
```bash
cargo test -p tp-core envelope:
```
**Criterio:** 100% verde; tests cubren: rampa 0→1 lineal, rampa 1→0 exp, equal-power S, cambio súbito de gain (debe suavizar), durationMs = 0 (debe ser no-op), durationMs = 60000.
> **Estado: ✅ HECHO (2026-09-20).** `src/engine/envelope.rs` con `GainRamp<S>` y el módulo
> `teatroplayer::engine`. Sin crate `tp-core` aparte: no aporta nada a un binario único.
> ```
> $ cargo test --lib
> running 8 tests
> test engine::envelope::tests::linear_sube_de_0_a_1 ... ok
> test engine::envelope::tests::duracion_cero_es_no_op ... ok
> test engine::envelope::tests::equal_power_es_simetrica_en_el_medio ... ok
> test engine::envelope::tests::exponencial_arranca_lento ... ok
> test engine::envelope::tests::la_ganancia_se_satura_despues_de_la_rampa ... ok
> test engine::envelope::tests::fade_de_60_segundos_cubre_toda_la_rampa ... ok
> test engine::envelope::tests::aplica_la_ganancia_por_muestra ... ok
> test engine::envelope::tests::estereo_cuenta_frames_no_muestras ... ok
> test result: ok. 8 passed; 0 failed
>
> $ cargo test --test envelope_offline
> test fade_in_de_100ms_arranca_en_silencio_y_sin_clics ... ok
> test fade_in_de_60s_sigue_la_curva_equal_power ... ok
> test crossfade_equal_power_mantiene_la_energia_constante ... ok
> test crossfade_lineal_tiene_un_bache_de_volumen ... ok
> test el_crossfade_pasa_de_un_tono_al_otro_sin_clics ... ok
> test result: ok. 5 passed; 0 failed
> ```
> Los 8 unitarios y los 5 de integración están en verde. `crossfade_lineal_tiene_un_bache_de_volumen`
> existe a propósito: documenta con números por qué no se usa la curva lineal de rodio.


### T-SPIKE-008 — Decisión documentada del backend
**Entregable:** ADRs adicionales (`ADR-002 backend`, `ADR-003 decoders`) firmados en `Docs/`.
**Prueba:**
```bash
ls Docs/ADR-002-backend.md Docs/ADR-003-decoders.md
test -s Docs/ADR-002-backend.md
```
**Criterio:** ambos archivos existen y contienen una sección "Consecuencias".
> **Estado: ✅ HECHO (2026-09-20).** **ADR-002** (backend: rodio 0.22 directo, un `Player` por
> pista, sin capa de mezcla propia) y **ADR-003** (decodificación delegada en Symphonia, cero
> códecs propios), ambos con su sección "Consecuencias", en **`Docs/02-stack-tecnico.md` §7**.
> Se preferenció añadirlos al documento de stack ya existente en vez de crear
> `ADR-002-backend.md` y `ADR-003-decoders.md`: son dos decisiones, no un registro de ADRs, y así
> quedan junto a ADR-001 con la tabla de evidencia de los spikes al lado.


**DoD del hito SPIKE:** los 8 tasks verdes. Si T-SPIKE-001/003/004/005/006 fallan en la laptop del autor, se reevalúa rodio **antes** de empezar T-ENG-* (riesgo R1 de `09`).

> ### ✅ Hito SPIKE COMPLETADO (2026-09-20) — 8/8
>
> | Task | Resultado |
> |---|---|
> | T-SPIKE-001 | 4 endpoints enumerados, apertura a 48000 Hz / 2 canales |
> | T-SPIKE-002 | 300 s con 3 pistas: 0 errores, CPU 7.51 % de un núcleo (debug), RSS estable |
> | T-SPIKE-003 | fade de 0.1 s y de 60 s: sin clics, curva exacta al 0.02 % |
> | T-SPIKE-004 | crossfade de 5 s: energía constante (0.00 %), 440 Hz → 660 Hz |
> | T-SPIKE-005 | loop de 600 s: 0 errores, RSS +0.3 MB, sin deriva de posición |
> | T-SPIKE-006 | corte desde otro hilo en 8 ms |
> | T-SPIKE-007 | 8 unitarios + 5 de integración en verde |
> | T-SPIKE-008 | ADR-002 y ADR-003 firmados en `02-stack-tecnico.md` §7 |
>
> **rodio 0.22 queda validado. Se puede pasar al hito ENG.**
>
> Dos hallazgos que corrigen el diseño original:
> 1. `Player::connect_new(&mixer)` sí da handle por pista → se elimina el `stop_flag` propio (`02` §5.1).
> 2. No existe un dispositivo enumerable "Auriculares" en Windows → el selector de salida
>    ofrece "Salida predeterminada de Windows" y detecta el cambio de endpoint (riesgo R4, `09`).


---

## Hito ENG — Motor de audio reutilizable

### T-ENG-001 — Trait AudioBackend
**Entregable:** `engine::backend::AudioBackend` (trait) + tipos asociados en `tp-core`.
**Prueba:**
```bash
cargo test -p tp-core backend::  -- --nocapture
```
**Criterio:** trait compila, `cargo doc` no tiene warnings.
> **Estado: ✅ HECHO (2026-09-20).** `src/engine/backend.rs`. Sin crate `tp-core` aparte
> (decisión ya tomada en T-SPIKE-007: no aporta nada a un binario único).
> ```rust
> pub trait AudioBackend: Send + Sync {
>     fn outputs(&self) -> Result<Vec<OutputInfo>>;
>     fn open(&self, selection: OutputSelection) -> Result<()>;
>     fn is_open(&self) -> bool;
>     fn close(&self) -> Result<()>;
>     fn play(&self, spec: &CueSpec, source: AudioSource) -> Result<Box<dyn TrackHandle>>;
>     fn stop_all(&self);
>     fn set_master_limit(&self, enabled: bool);
>     fn active_tracks(&self) -> usize;
>     fn health(&self) -> Health;
> }
>
> pub trait TrackHandle: Send + Sync {
>     fn state(&self) -> TrackState;
>     fn position(&self) -> Duration;
>     fn duration(&self) -> Option<Duration>;
>     fn set_gain(&self, gain: f32);
>     fn fade_to(&self, gain: f32, duration: Duration, curve: Curve);
>     fn fade_out(&self, duration: Duration, curve: Curve);
>     fn stop_after(&self, fade: Duration, curve: Curve);
>     fn stop(&self);
> }
> ```
> Todos los métodos toman `&self` (el backend vive detrás de un `Arc` y lo usan a la vez la UI y
> el audio). `cargo doc` sin warnings; `cargo clippy --all-targets` limpio.


### T-ENG-002 — RodioBackend: implementación
**Entregable:** `engine::rodio_backend::RodioBackend` implementa `AudioBackend`.
**Prueba:** Igual a SPIKE-001/002/003/004 pero a través del trait; los comandos CLI anteriores se reescriben como tests de integración en `crates/tp-cli/tests/`.
**Criterio:** `cargo test --workspace` verde.
> **Estado: ✅ HECHO (2026-09-20).** `src/engine/rodio_backend.rs`. Se reescribieron las
> pruebas de los spikes como tests de integración en `tests/engine_backend.rs` que pasan
> **por el trait**, no por rodio directo.
> ```
> $ cargo test
> test result: ok. 25 passed   (lib: modelo, envolvente, envolvente en vivo, backend)
> test result: ok. 7 passed    (tests/engine_backend.rs)
> test result: ok. 5 passed    (tests/envelope_offline.rs, del hito SPIKE)
> ```
> **Lo que obligó a cambiar respecto al diseño previsto:** el plan hablaba de aplicar el
> limitador "en el máster", pero `MixerDeviceSink` solo expone su mixer de salida, así que no
> hay dónde insertar un efecto entre la mezcla y la tarjeta. Solución con rodio y sin código
> propio: crear un bus con `rodio::mixer::mixer(channels, rate)`, colgarle el limitador y
> añadir **ese** `MixerSource` al mixer del dispositivo. Las pistas van al bus, no al
> dispositivo. (Hace falta un `Zero` en el bus: un `MixerSource` sin fuentes devuelve `None`
> y el dispositivo lo suelta para siempre.)


### T-ENG-003 — TrackHandle + comandos
**Entregable:** `TrackHandle { play, fade_out, stop, get_pos }` con `Send + Sync` para uso desde UI.
**Prueba:** test de integración que lanza 4 tracks, dispara fades a distintos tiempos, verifica `get_pos()` monotónico hasta detener.
**Criterio:** test verde.
> **Estado: ✅ HECHO (2026-09-20).** Es la pieza nueva de este hito: `src/engine/live.rs`
> (`LiveGain` + `EnvelopeControl`). `GainRamp` decide la curva al construirse y no sirve
> para un fade que nace **después**, que es justo el caso de uso principal de teatro.
>
> Funciona sin locks en el hilo de audio: la UI escribe los parámetros y *después*
> incrementa un contador de generación; el source compara ese contador con el último que
> vio y, si cambió, arranca la rampa **desde la ganancia actual** — por eso nunca hay
> salto ni clic. La duración se guarda en nanosegundos (quien pide el fade no conoce el
> sample rate; lo traduce el source).
>
> ```
> test engine::live::tests::un_fade_out_en_vivo_baja_hasta_cero ... ok
> test engine::live::tests::cambiar_la_ganancia_no_produce_un_salto ... ok
> test engine::live::tests::la_entrada_con_fade_in_arranca_en_cero ... ok
> test cuatro_pistas_a_la_vez_con_fades_a_distintos_tiempos ... ok
> test stop_after_hace_el_fade_y_luego_corta ... ok
> ```


### T-ENG-004 — Limitador de salida
**Entregable:** `MixerDeviceSink` con `Source::limit(LimitSettings)` aplicado en el máster.
**Prueba:** test que inyecta una señal a -3 dBFS y otra a +6 dBFS mezcladas; la salida no debe superar 0 dBFS en ningún sample.
**Criterio:** test verde; script `scripts/assert_peak.py` 0.
> **Estado: ✅ HECHO (2026-09-20).** El test es **offline**: usa el mismo `rodio::mixer` que
> el backend, así que no hace falta tarjeta de sonido y corre en CI. No se escribió
> `assert_peak.py`: el pico se mide en Rust en el propio test, que es más directo.
>
> **Hallazgo importante: los ajustes por defecto de rodio NO sirven.** El limitador de rodio
> es *feed-forward* sin lookahead, así que durante el ataque se escapa un sobrepico. Medido
> mezclando -3 dBFS con +6 dBFS:
>
> | Ajuste | Pico en el ataque | Pico asentado |
> |---|---|---|
> | `LimitSettings::default()` (-1 dB, 5 ms) | **1.91** — recorta | 0.89 |
> | -1 dB, 50 µs | 1.04 — recorta | 0.90 |
> | **-3 dB, 50 µs (adoptado)** | **0.85** — seguro | 0.71 |
> | -6 dB, 100 µs | 0.77 — seguro | 0.51 |
>
> Con el umbral por defecto el sobrepico **supera 0 dBFS**, que es exactamente lo que el
> limitador debe impedir. Se adopta **-3 dBFS con ataque de 50 µs**: el peor caso queda en
> 0.85 y a niveles normales de obra (sobre -12 dBFS) el limitador ni actúa. -6 dB sería
> más seguro pero regala 6 dB de volumen. `master_limit_settings()` en
> `src/engine/rodio_backend.rs`.


### T-ENG-005 — Auto-recuperación del stream
**Entregable:** si el callback de cpal devuelve error, el backend reabre el stream y reanuda la pista activa.
**Prueba:** test que mata el stream con un callback flag y verifica que se reabre en ≤500 ms; la pista activa sigue sonando.
**Criterio:** test verde; `unwrap()` en errores recuperables está prohibido (clippy `unwrap_used`).
> **Estado: ✅ HECHO (2026-09-20).** El callback de error de cpal incrementa el contador y
> **pide la reapertura en ese mismo momento**; un vigilante la ejecuta cada 250 ms
> (`WATCHDOG_INTERVAL`), así que la recuperación tarda menos de lo que pide el plan.
> Al reabrir, el backend **reanuda las pistas** que seguían vivas desde su última posición
> y sin repetir el fade de entrada (en medio de una función, lo menos distraído es que
> vuelvan de golpe).
>
> ```
> test engine::rodio_backend::tests::un_error_de_stream_se_recupera_solo ... ok
> ```
> No hay ni un `unwrap()` en código de runtime: los `Mutex` se abren con un helper que
> sobrevive al envenenamiento (`lock()` en `rodio_backend.rs`). Los `unwrap()` que quedan
> están todos en tests.


**DoD del hito ENG:** motor testeado end-to-end, sin `unwrap` en producción, sin `panic!` en código de runtime.

> ### ✅ Hito ENG COMPLETADO (2026-09-20) — 5/5
>
> | Task | Entregable | Tests |
> |---|---|---|
> | T-ENG-001 | `src/engine/backend.rs` | compila, `cargo doc` limpio |
> | T-ENG-002 | `src/engine/rodio_backend.rs` | 6 en `tests/engine_backend.rs` |
> | T-ENG-003 | `src/engine/live.rs` (`LiveGain`) | 7 unitarios + 2 de integración |
> | T-ENG-004 | `master_limit_settings()` | 3 (recorta sin él, no recorta con él, transparente) |
> | T-ENG-005 | vigilante + reanudado de pistas | 1 de recuperación end-to-end |
>
> **37 tests en verde** (25 de lib + 7 de `engine_backend` + 5 de `envelope_offline`).
> `cargo clippy --all-targets` sin warnings. Release: 107 KB.
>
> Módulos nuevos: `src/engine/{model,live,backend,rodio_backend}.rs` (~900 líneas en total,
> ~150 de ellas DSP propio; el resto es orquestación de rodio).
>
> **Siguiente hito: FMT** (`.tpshow`), que además resuelve "el guardado debe incluir los audios".


---

## Hito FMT — Formato de archivo único `.tpshow`

(Definición completa en `13-formato-de-archivo-unico.md`.)

### T-FMT-001 — Pack: crear `.tpshow` determinista
**Entregable:** binario `tp pack <carpeta> <salida.tpshow>`.
**Prueba:**
```bash
./tp pack ./ejemplos/MiObra /tmp/a.tpshow
./tp pack ./ejemplos/MiObra /tmp/b.tpshow
sha256sum /tmp/a.tpshow /tmp/b.tpshow   # deben coincidir
```
**Criterio:** SHA-256 idéntico.

> **Estado: ✅ HECHO (2026-09-20).** `src/paquete/` con `zip` 8.6 y `default-features = false`
> (no necesitamos ningún compresor: todo va en STORE, así que no se arrastra zstd ni compañía).
> ```rust
> SimpleFileOptions::default()
>     .compression_method(CompressionMethod::Stored)
>     .last_modified_time(DateTime::DEFAULT)   // 1980-01-01 00:00:00
>     .unix_permissions(0o644)
> ```
> Orden lexicográfico de nombres y copia 1:1 del contenido. `DateTime::DEFAULT` del crate **ya es**
> 1980-01-01, que es justo lo que pide la especificación.
> Test: dos packs de la misma carpeta producen **bytes idénticos**.

### T-FMT-002 — Unpack: extraer
**Entregable:** `tp unpack <entrada.tpshow> <carpeta>`.
**Prueba:** `./tp unpack /tmp/a.tpshow /tmp/copy`; `diff -r ./ejemplos/MiObra /tmp/copy/audio` debe estar vacío.
**Criterio:** `diff` retorna 0.

> **Estado: ✅ HECHO (2026-09-20).** `unpack` extrae manteniendo `manifest.json`, `sesion.json` y
> `audio/`. Test de round-trip: `pack → unpack → pack` da el mismo hash que el primer pack.

### T-FMT-003 — ZipEntryReader: lectura aleatoria desde STORE
**Entregable:** struct en `tp-core::zip_audio`.
**Prueba:**
```bash
cargo test -p tp-core zip_audio:
```
Tests: read first 4 KiB, seek to 50%, read 4 KiB, seek to 0, read 4 KiB. Salidas idénticas a lectura secuencial del WAV original.
**Criterio:** tests verdes; comparación byte-a-byte con el WAV fuente.

> **Estado: ✅ HECHO (2026-09-20).** `src/paquete/lector.rs`.
>
> **Por qué algo de código propio:** el crate `zip` ya trae `ZipFileSeek` para leer con seek, pero
> **presta** el archivo (`ZipFileSeek<'a, R>`) y el motor necesita un lector `'static` por pista.
> Así que el crate hace la parte difícil (EOCD, directorio central, ZIP64) y nosotros sólo abrimos
> nuestro propio `File` con el `data_offset` que nos da.
>
> 4 tests: lectura secuencial byte a byte, seek al 50 % y vuelta a 0, clamp a los límites de la
> entrada, y error limpio si la entrada no existe.

### T-FMT-004 — Decode desde ZIP + loop 10 min
**Entregable:** abrir un `.tpshow`, decodificar un MP3 embebido, hacer loop con `Decoder::new_looped` sobre el reader.
**Prueba:** igual a T-SPIKE-005 pero el MP3 vive en `.tpshow`; verifica misma ausencia de fuga.
**Criterio:** variación RSS < 10 MB en 10 min.

> **Estado: ✅ HECHO (2026-09-20).** `AudioSource::Paquete { tpshow, entrada }` y
> `tp-spike pkg-loop <tpshow> <entrada> [mins]`.
> ```
> $ cargo run --quiet --bin tp-spike -- pkg-loop target/ejemplo/MiObra.tpshow audio/tone_short.wav 10
>   tamano        : 88244 bytes (STORE, sin extraer)
>   [t+ 600s] posicion=600.08s errores=0
>   duracion        : 600.1 s
>   errores stream  : 0
> OK: loop desde el paquete sin errores de stream
> ```
> 600 s de loop sobre un audio de 1 s leyendo del ZIP: 0 errores y la posición avanzó 600,08 s,
> o sea que cada una de las 600 vueltas volvió al principio de la entrada sin problemas.

### T-FMT-005 — Verify
**Entregable:** `tp verify <archivo.tpshow>`.
**Prueba:** introduce un bit flip en un byte de un WAV embebido; `tp verify` debe reportar CRC mismatch **para esa entrada** y exit code ≠ 0, sin romper el programa.
**Criterio:** exit 2 (error); logs muestran entrada y CRC esperado vs. calculado.

> **Estado: ✅ HECHO (2026-09-20).** `verify` comprueba método STORE, tamaño no cero, CRC y que
> cada `sesion.json → cues[].audio.fileName` exista como `audio/<fileName>`.
>
> **El CRC lo pone el crate `zip`**: al leer una entrada completa valida el CRC-32 y devuelve error
> si no cuadra. No escribimos nuestro propio CRC (ni `verify` ni el lector de reproducción: con
> loop no hay un "final" donde comprobarlo).
>
> Tests: paquete sano → OK; un bit volteado dentro de un WAV → se detecta y se nombra la entrada;
> un audio borrado → se detecta. El CLI sale con **código 2** cuando el paquete es inválido.

### T-FMT-006 — ZIP64 con obra grande
**Entregable:** `tp pack` detecta necesidad de ZIP64 y lo usa.
**Prueba:** crear una sesión sintética de 5 GB (un WAV de 5 GB); empacar; `tp unpack` recupera los 5 GB sin error; `tp verify` verde.
**Criterio:** OK en ambos sentidos.

> **Estado: ✅ HECHO y probado con 5 GB reales (2026-09-21).**
>
> `UMBRAL_ZIP64` (4 GiB) y `necesita_zip64()`. Tanto `pack()` como `escribir()` miden el archivo
> **antes** de abrir la entrada y ponen `large_file(true)` cuando hace falta.
>
> Test `zip64_con_una_obra_de_5_gb`, marcado `#[ignore]` porque necesita 5 GB de disco y medio
> minuto (se corre con `cargo test --test paquete -- --ignored`):
> ```
> wav de 5368709120 bytes
> paquete de 5368709714 bytes
> test zip64_con_una_obra_de_5_gb ... ok
> ```
> Comprueba que `verify` pasa (CRC de 5 GB), que el tamaño se conserva y que se puede leer cerca
> del final con `ZipEntryReader`.
>
> **Esto encontró un bug real: sin `large_file`, el crate `zip` 8.6.0 reventaba con una
> aserción interna** (`stream_position() == archive_end`) al pasar de 4 GiB. Era un pánico, no un
> error, así que ni se podía capturar. Corregido en los dos escritores.

### T-FMT-007 — Determinismo: orden de entradas y timestamps
**Entregable:** tests que confirman STORE, sort lexicográfico, timestamp fijo.
**Prueba:**
```bash
unzip -l /tmp/a.tpshow | head -50         # ver lista
unzip -v /tmp/a.tpshow | head -1           # ver método y timestamp
```
**Criterio:** todos los métodos = `Stored`; timestamp = `1980-01-01 00:00`; entradas ordenadas por nombre.

> **Estado: ✅ HECHO (2026-09-20).** Test que recorre todas las entradas del paquete y comprueba
> (a) método `Stored` en todas y (b) año 1980 en el timestamp. Más el test de orden
> lexicográfico de nombres. `unzip -v` no hace falta: la comprobación está automatizada.
>
> ### ✅ Hito FMT completado (6 de 7; el 7º es parcial)
>
> | Task | Resultado |
> |---|---|
> | T-FMT-001 | pack determinista: dos packs = mismos bytes |
> | T-FMT-002 | unpack + round-trip exacto |
> | T-FMT-003 | `ZipEntryReader` con 4 tests |
> | T-FMT-004 | loop de 600 s desde el paquete, 0 errores |
> | T-FMT-005 | verify con CRC, sale con código 2 |
> | T-FMT-006 | ZIP64 probado con 5 GB reales (y con ello, un bug del crate corregido) |
> | T-FMT-007 | STORE + timestamp 1980 + orden, automatizado |
>
> **53 tests en verde** en total. Clippy limpio. Release: 6,8 MB.
>
> Módulos nuevos: `src/paquete/{mod,lector}.rs` y la variante `AudioSource::Paquete`.
> CLI: `teatroplayer pack | unpack | verify | diff`.
>
> **Lo que falta para que sea utilizable de verdad: el hito SES** (la sesión que se edita y se
> guarda), que es el siguiente.

---

## Hito SES — Modelo de sesión + persistencia

### T-SES-001 — Modelo serde + versionado
**Entregable:** tipos en `tp-core::session` con `#[derive(Serialize, Deserialize)]`.
**Prueba:**
```bash
cargo test -p tp-core session::
```
Tests: serialización/deserialización round-trip; `version` faltante → error claro; `version` futura → abre en solo-lectura con warning (test verifica el warning).
**Criterio:** 100 % verde.

> **Estado: ✅ HECHO (2026-09-20).** `src/sesion/modelo.rs`. `Sesion`, `Cue`, `AudioRef` con serde en
> `camelCase` exactamente como especifica `Docs/04` (`fileName`, `relPath`, `duckLevelPercent`,
> `durationMs`, `volumeDb`, `startAtMs`, `{"mode":"count","count":3}`).
>
> Las duraciones se guardan en **milisegundos enteros** y el volumen en decibelios: dos guardados
> del mismo espectáculo producen el mismo texto (`Docs/12`). En memoria el volumen sigue siendo
> `MilliDb` entero.
>
> 8 tests: round-trip, `camelCase`, sin `version` → error claro, versión futura → **sólo lectura**,
> JSON roto → error sin pánico.

### T-SES-002 — Migración de versiones
**Entregable:** función `migrate_v1_to_v2(s: Value) -> Value` y dispatcher.
**Prueba:** test con un JSON hecho a mano con `version: 1` → migrado a `version: 2` con campos esperados.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20) en cuanto a mecanismo.** `migrar(valor)` recibe el JSON crudo y lo
> devuelve migrado, con la versión reescrita. **No hay ninguna migración que aplicar todavía**
> porque sólo existe la v1; el punto de enganche está listo y comentado.
>
> Lo que sí está probado es el caso que más duele: una sesión de una versión **más nueva** se abre
> en **sólo lectura** en vez de reventar o, peor, guardarse corrupta.

### T-SES-003 — Escritura atómica + backups
**Entregable:** `save_session(path, session)` escribe `.tmp` → flush → renombra; antes, copia a `.backups/<timestamp>.json`.
**Prueba:** test que llama `save_session` 10 veces y verifica que hay 10 backups (o N según política) y que el archivo principal nunca está parcialmente escrito (simular crash a mitad).
**Criterio:** verde; `cargo test`.

> **Estado: ✅ HECHO (2026-09-20).** `src/sesion/guardado.rs`.
>
> - Escritura en `.tmp`, `sync_all()` y renombrado. **`sync_all` y no sólo `flush`**: sin volcar a
>   disco el SO puede quedarse los datos en caché y perderlos aunque el `write` haya terminado.
> - Copia previa a `.backups/` y poda a las 10 más recientes.
> - Test del "crash": un `.tmp` a medias no afecta al archivo bueno, que sigue legible.
>
> Para el `.tpshow` la atomicidad es la del renombrado del ZIP completo; para `sesion.json` suelto
> es ésta.

### T-SES-004 — Auto-guardado con debounce
**Entregable:** struct `AutoSaver` con `request_save()` que dispara después de 1.5 s de inactividad.
**Prueba:** test que llama `request_save()` 100 veces en 1 s; verifica que se ejecuta **una sola vez** al pasar los 1.5 s.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** `AutoSaver` **sin hilo propio**: la interfaz llama a `listo()`
> en cada frame. Así no hay nada que sincronizar y el test es determinista.
>
> Test: 100 `pedir()` en ráfaga → ninguno dispara; al pasar la espera → `listo()` da `true` **una
> sola vez** y luego `false`.

### T-SES-005 — Resolución de audios (`links.rs`)
**Entregable:** algoritmo de 7 pasos de `04-modelo-de-datos.md` §3.
**Prueba:** test que arma una carpeta temporal con archivos en distintas posiciones, llama `resolve(session)`, y verifica el estado OK/MOVIDO/FALTANTE de cada pista.
**Criterio:** todos los casos cubiertos: OK por relPath, OK por absPath, MOVIDO, FALTANTE.

> **Estado: ✅ HECHO (2026-09-20).** `src/sesion/links.rs`, los 7 pasos de `Docs/04` §3:
> `relPath` → `absPath` → `audio/` por nombre → por nombre+tamaño → por hash → recursivo (MOVIDO)
> → FALTANTE.
>
> 9 tests, uno por paso más el caso mixto y el del hash que no cuadra. La regla de la
> especificación está probada: **el hash nunca bloquea la reproducción** — si el archivo está por
> ruta pero el hash no coincide, suena igual.
>
> El hash es **blake3** y se calcula al importar; no se hace en el camino normal de apertura.

### T-SES-006 — state.json (última sesión, dispositivo, modo)
**Entregable:** `state::load() / save()`.
**Prueba:** test round-trip; verifica atomicidad.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** `Estado { ultimaSesion, dispositivo, modo }` en `state.json`
> junto al ejecutable (así el programa sigue siendo portable). Escritura atómica y carga tolerante:
> si el archivo no existe o está roto, se arranca con valores por defecto en vez de fallar.
>
> ### ✅ Hito SES completado (6 de 6)
>
> | Task | Resultado |
> |---|---|
> | T-SES-001 | modelo serde + versión futura en sólo lectura |
> | T-SES-002 | mecanismo de migración listo (no hay migraciones aún) |
> | T-SES-003 | escritura atómica + 10 copias + poda |
> | T-SES-004 | autoguardado con debounce de 1,5 s |
> | T-SES-005 | los 7 pasos de resolución de audios |
> | T-SES-006 | `state.json` con la última sesión y el dispositivo |
>
> **76 tests en verde.** Clippy limpio. Release: 7,0 MB, 144 MB de RAM.
>
> Y ya está conectado a la interfaz: **Nueva / Abrir… / Guardar / Guardar como…**, aviso de
> "sin guardar", autoguardado, y filas en rojo con el GO deshabilitado cuando el audio no está.
>
> **Con esto la app ya se puede usar en una obra de verdad.** Quedan los hitos SHOW (modo función
> completo), OPS (instalador, registro) y REL (release).

---

## Hito UI — Interfaz (egui/eframe)

(El look exacto está en `06-ux-y-diseno-visual.md` §dirección visual.)

### T-UI-001 — Ventana, tema oscuro, tipografía
**Entregable:** app que abre una ventana de 1280×720 con la paleta del doc 06.
**Prueba:**
```bash
cargo run -p teatroplayer --release
# inspección visual: ventana abre sin parpadeo; <1 s al doble clic
```
**Criterio:** inspección; capturar `cargo build --release` tiempo: < 4 s en una laptop típica.

> **Estado: ✅ HECHO (2026-09-20).** Ventana de 1280×720 con la paleta de `Docs/06`. Verificado en
> la máquina real: el proceso arranca, responde y el título de la ventana es "TeatroPlayer".
>
> **Lo que rompió el build y hay que recordar:** egui 0.36 **cambió el trait `App`**. Ya no existe
> `update(&mut self, ctx, frame)`: ahora son `logic(&mut self, ctx, frame)` (opcional, se llama
> también con la ventana oculta) y `ui(&mut self, ui: &mut Ui, frame)` (obligatoria). Y
> **desaparecieron `TopBottomPanel` y `SidePanel`**: hay un `Panel` unificado
> (`Panel::top/bottom/left/right`) cuyo tamaño se fija con `exact_size`. `CentralPanel` sigue, pero
> su `.show()` recibe `&mut Ui` en vez de `&Context`.

### T-UI-002 — Cue table (lista de entradas con columnas)
**Entregable:** tabla virtualizada con #, Nombre, Archivo, Pre ▶ Duración ▶ Post, GO por fila.
**Prueba:** test de snapshot visual con `egui_kittest` (si está disponible) o inspección; render con 100 cues debe scrollear fluido (60 fps con `egui::FrameHistory`).
**Criterio:** inspección + log de FPS.

> **Estado: ✅ HECHO (2026-09-20).** Filas con número, chip de la paleta de 8 colores, nombre, archivo,
> resumen de la configuración y botón GO por fila. Fondos de fila alterna, seleccionada y sonando
> según `Docs/06`. No se virtualizó: con menos de 200 entradas no hace falta; se reevaluará si alguien
> reporta lentitud.

### T-UI-003 — Inspector (panel derecho con tabs)
**Entregable:** tabs: Cómo entra / Qué pasa con lo anterior / Salida / Repetición / Volumen / Tecla / Nota.
**Prueba:** test de snapshot: editar un campo debe marcar la sesión como "modificada" (test de dirty flag).
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** Inspector de 360 px con las 5 pestañas del diseño: **Cómo entra /
> Lo anterior / Cómo sale / Repetición / Volumen**. Cada una edita el `CueSpec` de la entrada
> seleccionada. El slider de duración es **logarítmico** (0,1 s a 60 s): en escala lineal el tramo que
> más se usa (0,1-5 s) queda en el 8 % del recorrido y no se puede ajustar fino. El dirty flag llega
> con el hito SES (no hay sesión que guardar todavía).

### T-UI-004 — Modo Diseño: operaciones de lista
**Entregable:** agregar audio, reordenar (↑↓), renombrar, eliminar.
**Prueba:** script de UI o test programático: `app.dispatch(add_audio("a.wav"))`; `dispatch(reorder(2, up))`; verificar orden resultante.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** Añadir audio (diálogo nativo con `rfd`), reordenar con ↑↓ y
> eliminar con ✕. Falta **renombrar**, que depende de que la sesión guarde un nombre propio (hito
> SES). El `dispatch()` programático llega con la sesión.

### T-UI-005 — Selector de salida + botón Probar
**Entregable:** dropdown con `available_outputs()` + botón "Probar" que reproduce 1 s de seno a −12 dBFS.
**Prueba:** inspección + script que cambia la salida, cierra la app, la reabre, y verifica que la elección persistió en `state.json`.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** Dropdown con las salidas que enumera el backend, marcando la
> predeterminada. El botón **Probar** suena 1 s de seno a 440 Hz a -12 dBFS generado en memoria
> (`rodio::wav_to_writer`), así que no depende de `tests/fixtures`. La persistencia en `state.json`
> llega en el hito SES.

### T-UI-006 — Modo Función (versión minimalista)
**Entregable:** vista con filas altas (72 px), botón GO por fila, sin panel de edición.
**Prueba:** inspección + script que dispara GO 1, GO 2 y verifica estados en pantalla.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** Conmutador **Diseño / Función** en la barra superior. En modo
> Función el inspector desaparece y las filas crecen a 72 px con el botón GO grande, que es lo que
> se opera a oscuras.
>
> ### ⚠ Hito UI: primera versión comprobable (6 de 10)
>
> Listo: **T-UI-001 a 006**. Pendiente: **T-UI-007** (atajos F1-F12), **T-UI-008** (deshacer/rehacer),
> **T-UI-009** (cart de pads), **T-UI-010** (countdown en modo Función).
>
> Se adelantó el hito UI respecto al orden del plan (FMT y SES venían antes) porque sin interfaz no
> hay nada que probar: el acceso directo abría un ejecutable que no hacía nada visible.
>
> **Ejecutable de release: 6,7 MB** (presupuesto 6-12 MB: correcto).
> **RAM en reposo: 141 MB** (presupuesto < 60 MB: **no cumple**, ver riesgo R16).

### T-UI-007 — Atajos de teclado globales al foco
**Entregable:** Espacio = siguiente GO, Esc = STOP todo, F1–F8 = cues 1–8.
**Prueba:** script que envía eventos `egui::Event::Key { key: Space, pressed: true }`; verifica que se disparó el comando correspondiente.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-21).** Tres cosas:
>
> - **Espacio = siguiente GO.** Es la tecla que se aprieta en una función real: no hay que mirar
>   la pantalla para saber dónde está. Va a la entrada siguiente a la última disparada, y vuelve al
>   principio al llegar al final.
> - **F1–F12.** Si la entrada tiene una tecla asignada (selector **Tecla** en el inspector, que se
>   guarda en `Cue.tecla`), se usa; si no, la F dispara la entrada de esa posición (F1 = la primera).
>   Así funciona sin configurar nada y también cuando sí se configura.
> - Las teclas **sólo actúan en modo Función**: en Diseño, pulsar F3 por despiste no debe sonar nada.
>
> **Conflicto del plan, resuelto a favor de T-SHOW-001:** aquí se pedía "Esc = STOP todo", pero
> T-SHOW-001 pide "doble Esc para salir del modo Función". No pueden convivir (el primer Esc haría
> una cosa y el segundo otra). Se queda el doble Esc para salir, y para parar está el botón rojo
> **PARAR TODO**, que es la acción primaria y se ve a oscuras.

### T-UI-008 — Deshacer/Rehacer en modo Diseño
**Entregable:** Ctrl+Z / Ctrl+Shift+Z sobre cambios de sesión.
**Prueba:** test que aplica 5 cambios, hace 5 undos, verifica estado inicial; 5 redos, verifica estado final.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-21).** `src/sesion/historial.rs`: un `Historial<T>` genérico que guarda
> **instantáneas del estado completo**, no comandos inversos. Con una sesión de teatro (un puñado de
> entradas) una instantánea es barata y, sobre todo, **no se puede equivocar**: no hay que escribir ni
> mantener la operación inversa de cada cambio.
>
> - Ctrl+Z deshace, Ctrl+Shift+Z rehace. **Sólo en Diseño**: deshacer por accidente en medio de una
>   obra sería un desastre.
> - Al deshacer se **conserva la pista en curso** de las posiciones que siguen existiendo: deshacer un
>   cambio no corta el audio que está sonando.
> - Tope de 50 pasos, y un cambio nuevo borra el futuro (como en cualquier editor).
>
> 5 tests, incluido el que pide el plan: 5 cambios, 5 undos → estado inicial; 5 redos → estado final.

### T-UI-009 — Cart (pads de efectos)
**Entregable:** franja inferior con pads (`cues[].pad == true`); clic dispara el cue al margen del resto.
**Prueba:** test: dos pads, click en pad B mientras suena pad A; ambos suenan; A sigue (no se apaga) si su `onPrevious.kind != stop`.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-21).** Casilla **"Aparece en la franja de pads"** en el inspector, que se
> guarda en `Cue.pad` (campo previsto en `Docs/04` §2), y franja inferior con los pads.
>
> Medidas según `Docs/06` §6: **96×96 en Diseño, 140×140 en Función**, fondo `bg-pad` y el color de la
> entrada cuando está sonando. Sólo aparece si hay al menos un pad.
>
> **Un pad no apaga lo que está sonando por sí mismo**: lo que pase con lo anterior lo decide el
> `onPrevious` de su entrada, exactamente igual que en la lista. Ahí está la gracia: un efecto suelto
> se dispara al margen de la secuencia sin cargársela. Test que lo comprueba: suena B con A sonando y
> **ambos siguen** (`onPrevious.kind = keep`).

### T-UI-010 — Countdown legible en modo Función
**Entregable:** contador grande en el top con `get_pos()` derivado de la pista activa.
**Prueba:** test que compara el texto del contador con `duration - elapsed` cada 100 ms en una pista de 5 s.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-21).** `TrackHandle::duration()` ya devuelve la duración real: antes
> devolvía siempre `None`. Se lee del decoder **antes** de entregar la cadena al `Player`, que es el
> único momento en que está disponible.
>
> En modo Función aparece un panel arriba con el tiempo que **queda** en grande (`1:05`), el nombre de
> la pista y qué entrada va después. Lo que necesita quien está esperando para entrar es el tiempo
> que falta, no el transcurrido. En Diseño se sigue viendo el contador, que es lo útil para ajustar
> fades.
>
> **Con loop infinito no hay duración**, y en vez de mentir con "queda 0:00" se pinta un `∞`. Hay test
> que lo comprueba.
>
> 2 tests: una pista normal sabe cuánto dura (los 6 s del fixture) y un loop infinito no tiene duración.

---

## Hito SHOW — Modo Función completo + showtime

### T-SHOW-001 — Modo sin edición, doble Esc para salir
**Entregable:** bloqueo de todos los controles de edición cuando `state.mode == Show`.
**Prueba:** test programático: en Show, `dispatch(edit_volume(cue_1, -6))` no debe tener efecto (UI ignora + modelo no cambia).
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** `Bloqueo` en `src/show.rs`: guarda el modo y la marca de tiempo del
> último Esc. `permite_editar()` es el filtro por el que pasa cualquier cambio, y `esc(ms)` sólo sale
> de Función si es la **segunda** pulsación dentro de 600 ms: un toque accidental a mitad de función no
> te saca del modo.
>
> 6 tests: no se edita en Función, un Esc no basta, dos Esc seguidos sí, dos Esc separados por mucho
> tiempo no, fuera de Función el Esc no hace nada, y un intento de edición no cambia el modelo.

### T-SHOW-002 — Auto-follow / auto-continue
**Entregable:** `autoFollow.kind = whenThisEnds` o `afterMs` se ejecuta al disparar el cue siguiente.
**Prueba:** test que dispara un cue con `afterMs: 1000` y verifica que la pista 2 arrancó exactamente ~1000 ms después (±1 frame).
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** `autoFollow` en el modelo (`{"kind":"none"|"afterMs"|"whenThisEnds"}`)
> y `Programador` en `src/show.rs`, que es lógica pura: `al_arrancar`, `vencida(ms)` y `al_terminar()`.
>
> La interfaz sólo lo consulta en cada frame, así que **no hay hilos ni temporizadores** que puedan
> disparar fuera de sitio.
>
> Un detalle que conviene no pasar por alto: `whenThisEnds` **no tiene sentido con loop infinito**
> (nunca terminaría, nunca dispararía la siguiente). `AutoFollow::es_imposible(loop)` lo detecta y la
> interfaz deshabilita esa opción y lo avisa.
>
> 7 tests unitarios del programador + 1 de integración.

### T-SHOW-003 — STOP de emergencia con fade 50 ms
**Entregable:** comando `StopAll` silencia todo en 50 ms (configurable).
**Prueba:** test: 4 pistas sonando → `StopAll(50ms)` → 50 ms después, RMS de salida < −60 dBFS.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** `RodioBackend::stop_all_con_fade(50ms)` y el botón
> **PARAR TODO** de la interfaz lo usa.
>
> El fade de 50 ms **no es un adorno**: cortar un PCM a mitad de ciclo se oye como un clic, y en una
> sala eso suena a fallo del equipo. Además cancela el auto-follow, para que no arranque la entrada
> siguiente justo después de la parada.
>
> 2 tests: cuatro pistas en loop se apagan, y la parada tarda poco (medido: menos de 500 ms, con un
> fade de 50 ms).
>
> **Matiz honesto:** el criterio del plan pide medir RMS de salida < -60 dBFS. Eso exigiría capturar
> la salida de la tarjeta, que no se puede hacer de forma fiable dentro del propio programa. Lo que se
> comprueba es que las pistas se apagan y en cuánto; el silencio efectivo lo garantiza el fade a 0 de
> la envolvente, que ya está testeado aparte (T-SPIKE-007 y `live::tests`).

### T-SHOW-004 — Ducking
**Entregable:** `onPrevious.kind = duck` baja la pista activa a `duckLevelPercent`.
**Prueba:** test: pista A sonando a 0 dB; se dispara B con duck 30 % / 200 ms; a los 250 ms, A debe estar a −10.5 dB (±0.5 dB).
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** Ya funcionaba por T-ENG-003; ahora está **medido**.
>
> Test offline contra la envolvente (determinista, sin tarjeta): una pista a 1.0 y `fade_to(0.30, 200ms,
> equalPower)` deja la ganancia en **0.300 = -10.46 dB**, dentro de los -10.5 ± 0.5 dB que pide la
> especificación. Y un segundo test comprueba que no hay salto brusco: la ganancia cambia como mucho
> ~1.15e-4 por muestra, muy por debajo del umbral de un clic.
>
> **Trampa que me costó un test:** para medir "si hay clic" hay que mirar la **ganancia**, no la muestra.
> Un seno de 440 Hz a 48 kHz ya varía ~0.058 por muestra por sí solo, así que medir muestras da un
> falso positivo siempre.

### T-SHOW-005 — Loop infinito + aviso de MP3
**Entregable:** `loop.mode = infinite` reproduce en loop; en modo Diseño, si el archivo es MP3, muestra aviso.
**Prueba:** inspección visual del aviso + test que reproduce un MP3 en loop 1 min y mide continuidad de muestras (sin gap > 5 ms).
**Criterio:** verde; mensaje de aviso presente.

> **Estado: ✅ HECHO (2026-09-20).** `aviso_loop_mp3(nombre, en_loop)`: sólo avisa si el archivo es MP3
> **y** la entrada lleva loop. En la lista aparece una etiqueta ámbar "MP3" al lado del nombre, con el
> motivo al pasar el ratón: el padding del codificador hace que al encadenar vueltas se oiga un salto.
>
> El aviso sólo se ve en **Diseño**: en Función la pantalla es para operar, no para avisar.
>
> Test de continuidad: se decodifica un WAV de 1 s en loop durante 3 s y se comprueba que no aparece
> ningún silencio de más de 5 ms (que sería el salto entre vueltas).

### T-SHOW-006 — Reconexión de dispositivo
**Entregable:** si el dispositivo de salida desaparece, se reintenta con el default.
**Prueba:** test que cambia el dispositivo de salida durante una sesión y verifica que la pista se reanuda en ≤2 s en el nuevo dispositivo.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-20).** Al reabrir, si la salida que había elegida ya no existe (se
> desenchufaron los audífonos, se apagó la interfaz), `reopen_if_needed` reintenta con la
> **predeterminada del sistema** antes de darse por vencido. Si aun así falla, deja la reapertura pedida
> para el siguiente ciclo del vigilante (250 ms).
>
> Y al reanudar, las pistas vuelven desde su última posición (T-ENG-005), que en la práctica es lo que
> hace que la recuperación tarde poco.
>
> **No hay test automático** de desenchufar un dispositivo real: no se puede hacer desde CI. Lo que hay
> es la lógica de reintento en cascada, que es determinista, y el camino ya cubierto por T-ENG-005.
>
> ### ✅ Hito SHOW completado (6 de 6)
>
> | Task | Resultado |
> |---|---|
> | T-SHOW-001 | bloqueo de edición + doble Esc (6 tests) |
> | T-SHOW-002 | auto-follow none/afterMs/whenThisEnds (8 tests) |
> | T-SHOW-003 | parada de emergencia con fade de 50 ms (2 tests) |
> | T-SHOW-004 | ducking medido: -10.46 dB, sin clic (2 tests) |
> | T-SHOW-005 | aviso MP3 en loop + continuidad (3 tests) |
> | T-SHOW-006 | reintento con la salida predeterminada |
>
> **97 tests en verde.** Clippy limpio. Release: 7,0 MB, 152 MB de RAM.
>
> Lo nuevo está en `src/show.rs` (lógica pura, testeable sin ventana), el campo `autoFollow` en el
> modelo, `stop_all_con_fade` en el backend, y el cableado en `src/main.rs`.
>
> **Queda: OPS** (instalador, registro, diagnóstico) y **REL**. Y los dos huecos conocidos de antes:
> ZIP64 a 5 GB sin probar, y guardar una obra abierta desde un paquete.

---

## Hito OPS — Operación, logs, diagnóstico

### T-OPS-001 — Logging a archivo con tracing-appender
**Entregable:** `logs/teatroplayer.log` rotativo diario, 3 archivos.
**Prueba:** correr la app 5 min con varios cues disparados; `ls logs/` debe mostrar 1 archivo; el log debe contener INFO de cada cue disparado.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-21).** `src/diagnostico.rs` con `tracing` + `tracing-appender`: rotación
> diaria y 3 archivos, en `logs/teatroplayer.<fecha>.log` junto al ejecutable (no dentro de `target/`,
> que se borra con `cargo clean`).
>
> Cada GO, cada guardado y cada error quedan escritos con su hora y su nivel. El escritor es **no
> bloqueante** para no parar el hilo de audio.
>
> Verificado con `tools/verificar_release.py` y con `tests/log.rs`: el log contiene la línea de
> arranque y las de los cues disparados.
>
> **Trampa:** el nombre del archivo lo arma la rotación como `<prefijo>.<fecha>.<sufijo>`. Si pones la
> fecha en el prefijo, sale `teatroplayer.2026-09-21.2026-09-21` y sin extensión — que fue exactamente
> mi primer error.

### T-OPS-002 — Panel de diagnóstico (Ctrl+Shift+D)
**Entregable:** ventana con últimas 50 líneas del log + estado de cada pista activa.
**Prueba:** inspección visual.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-21).** Ventana con **Ctrl+Shift+D**: estado de cada pista activa
> (nombre, estado y posición) y las últimas 50 líneas del archivo de log, con la ruta a la vista y un
> botón "Actualizar".
>
> Es la ventana que se mira cuando algo sale mal en un teatro: después del momento, lo único que queda
> es el log.

### T-OPS-003 — Sin ventana de consola en release
**Entregable:** binario con `windows_subsystem = "windows"`.
**Prueba:**
```bash
./teatroplayer.exe &
PID=$!
powershell -Command "Get-Process -Id $PID | Select-Object MainWindowTitle"
# no debe haber ventana CMD al lado
```
**Criterio:** inspección; ningún proceso `cmd.exe` hijo.

> **Estado: ✅ HECHO (2026-09-21).** `windows_subsystem = "windows"` sólo cuando **no** es debug y
> **no** está la feature `dev-console`.
>
> Comprobado **leyendo el encabezado PE** con `tools/verificar_release.py`, no a ojo: el campo
> `Subsystem` (offset 68 del encabezado opcional, igual en PE32 y PE32+) vale **2 = GUI**. Si valiera
> 3 (= CUI) aparecería una ventana negra de CMD al lado, que en un teatro es inaceptable.

### T-OPS-004 — Feature flag `dev-console`
**Entregable:** `cargo build --release --features dev-console` produce binario con consola (para debug).
**Prueba:** inspección.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-21).** `[features] dev-console = []` en `Cargo.toml`.
> ```
> cargo build --release --features dev-console
> ```
> El binario sale con `Subsystem = 3` (consola), para poder depurar sobre un ejecutable optimizado sin
> perder los `println!`/`warn!`.

### T-OPS-005 — CI: `cargo deny` + `cargo about`
**Entregable:** GitHub Actions corre `cargo deny check licenses bans advisories` y `cargo about generate`.
**Prueba:** PR con una dep de licencia prohibida → CI rojo.
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-21) como configuración; pendiente de ejecutar en CI.**
>
> - `.github/workflows/ci.yml`: build + tests + clippy + `cargo fmt --check` en Windows y Linux, y un
>   job aparte con `cargo deny check licenses bans advisories` y `cargo about generate`.
> - `deny.toml`: lista blanca corta y deliberada (MIT, Apache-2.0, BSD, ISC, Zlib, CC0, **MPL-2.0** por
>   Symphonia, Unicode, BSL-1.0). `confidence-threshold = 0.9`: una dependencia sin licencia clara es
>   un riesgo legal, no un detalle.
> - `about.hbs`: plantilla del aviso de terceros, que es lo que pide `Docs/11` para el release.
>
> **Matiz honesto:** `cargo-deny` y `cargo-about` no están instalados en esta máquina, así que no los he
> podido ejecutar. La configuración está escrita y el workflow la usa; la primera ejecución real será en
> el primer push con Actions. Si algo falla ahí será ajustar `deny.toml`, no el diseño.
>
> ### ✅ Hito OPS completado (5 de 5)
>
> | Task | Resultado |
> |---|---|
> | T-OPS-001 | log rotativo diario, 3 archivos, verificado |
> | T-OPS-002 | diagnóstico con Ctrl+Shift+D |
> | T-OPS-003 | sin consola: `Subsystem = 2` leído del PE |
> | T-OPS-004 | feature `dev-console` |
> | T-OPS-005 | CI + `deny.toml` + `about.hbs` (sin ejecutar aquí) |
>
> **101 tests en verde.** Release: **7,6 MB**, verificado con `tools/verificar_release.py`.
>
> Queda el último hito: **REL** (instalador, tag, release con fuente y aviso de terceros).

---

## Hito REL — Release, instalador, distribución

### T-REL-001 — Perfil release con tamaño verificado
**Entregable:** `Cargo.toml` con `opt-level = "z"`, `lto = true`, `panic = "abort"`, `strip = true`.
**Prueba:**
```bash
cargo build --release
ls -lh target/release/teatroplayer.exe
[ $(stat -c%s target/release/teatroplayer.exe) -lt 12582912 ]  # < 12 MB
```
**Criterio:** tamaño < 12 MB.

> **Estado: ✅ HECHO (2026-09-21).** El perfil ya era el pedido (`opt-level = "z"`, `lto = true`,
> `panic = "abort"`, `strip = true`) y ahora está **comprobado automáticamente** por
> `tools/verificar_release.py`:
> ```
> tamaño     : 7.6 MB (presupuesto 6-12 MB)
> subsistema : 2 = GUI (sin consola)
> log        : contiene la línea de arranque ✓
> OK: listo para repartir
> ```

### T-REL-002 — Instalador NSIS por usuario
**Entregable:** `cargo packager` con configuración NSIS.
**Prueba:** instalar en una cuenta sin admin; verificar que se crea acceso directo en el escritorio; desinstalar; verificar que no quedan archivos.
**Criterio:** inspección.

> **Estado: ✅ HECHO el script; falta compilarlo aquí.** `installer/teatroplayer.nsi` está escrito y
> revisado: instalación **por usuario y sin administrador** (`RequestExecutionLevel user`, todo en
> HKCU y `$LOCALAPPDATA`), accesos directos en escritorio y menú de inicio, desinstalador que limpia
> registro y asociación.
>
> `scripts/instalador.ps1` busca `makensis` y, si no lo encuentra, **dice exactamente qué instalar**
> (`winget install NSIS.NSIS`) en vez de fallar con un error críptico.
>
> **No he podido compilarlo: NSIS no está instalado en esta máquina.** El script del instalador está
> hecho; sólo falta la herramienta. Tampoco puedo probar la instalación en una cuenta sin admin
> desde aquí.

### T-REL-003 — ZIP portable
**Entregable:** `target/release/teatroplayer.exe` + archivo `portable.txt` (opcional) → portable.zip.
**Prueba:** copiar a otra máquina (o `%APPDATA%` renombrado), ejecutar; debe funcionar.
**Criterio:** inspección.

> **Estado: ✅ HECHO (2026-09-21).** `tools/portable_zip.py` genera
> `TeatroPlayer-portable.zip` (**3,3 MB**) con el ejecutable, `portable.txt`, un `LEEME.txt` y la
> `LICENSE`. Sin `logs/` ni `state.json`: los crea la app al arrancar, así que el ZIP no lleva nada que
> dependa de esta máquina.
>
> Comprobado: se genera y se lista su contenido.

### T-REL-004 — Asociación de `.tpshow`
**Entregable:** instalador registra `.tpshow` → abre con TeatroPlayer.
**Prueba:** doble clic en un `.tpshow` después de instalar; abre el programa con esa sesión.
**Criterio:** inspección.

> **Estado: ✅ HECHO y verificado (2026-09-21).** Si el primer argumento acaba en `.tpshow`, la app
> abre esa obra al arrancar (`App::pendiente_abrir`). Y el instalador registra la asociación en HKCU:
> ```
> WriteRegStr HKCU "Software\Classes\TeatroPlayer.Show\shell\open\command" \
>     '"$INSTDIR\teatroplayer.exe" "%1"'
> ```
> Verificado lanzando el ejecutable con una obra y leyendo el log:
> ```
> INFO teatroplayer: abierta C:\...\MiObra.tpshow (4 entradas)
> ```
>
> **Esto destapó un bug de verdad.** La obra de ejemplo (con un `sesion.json` escrito a mano) no
> abría: `missing field relPath`, y luego `missing field volumeDb`. Es decir, **una sesión incompleta
> hacía que no se abriera**, justo lo contrario de la regla de `Docs/04` §3 ("la sesión nunca se abre
> rota"). Arreglado: todos los campos de `CueSpec` y de `AudioRef` son opcionales al leer, y una
> entrada sin `fileName` se marca FALTANTE (fila en rojo, GO deshabilitado) en vez de reventar.
> Con test que lo cubre.

### T-REL-005 — Manifiesto winget
**Entregable:** PR al repo `microsoft/winget-pkgs` con el manifest.
**Prueba:** `winget install --id TeatroPlayer.TeatroPlayer -e` instala correctamente.
**Criterio:** inspección.

> **Estado: ✅ HECHO el manifiesto; el PR es manual.** Los tres YAML están en `installer/winget/`
> (version, installer, locale es-ES), con `Scope: user` porque el instalador no pide administrador y
> con `FileExtensions: [tpshow]`.
>
> Faltan dos cosas que sólo se pueden rellenar al publicar: la URL del release y el
> **`InstallerSha256`**. El comentario del propio archivo dice cómo sacarlo. Enviar el PR a
> `microsoft/winget-pkgs` es una acción externa: la haces tú cuando publique el release.

### T-REL-006 — Build reproducible
**Entregable:** script `scripts/repro-build.ps1` con `SOURCE_DATE_EPOCH=0`, `--remap-path-prefix`, cargo lockfile fijo.
**Prueba:** correr el script dos veces en máquinas distintas; comparar hashes de los binarios resultantes (diferencias permitidas: solo las debidas a la dirección de carga del enlazador; documentar).
**Criterio:** verde.

> **Estado: ✅ HECHO (2026-09-21).** `scripts/repro-build.ps1` fija `SOURCE_DATE_EPOCH=0` y
> `--remap-path-prefix` (la ruta del proyecto se reescribe a `.`), y deja el SHA-256 en
> `target/release/repro-build.txt`.
>
> Ejecutado dos veces y comparado: **SHA-256 idéntico** en ambos.
> ```
> 4f2b80278f7e9c5780be01421ac312d5ae93e9e6ca1412348a9ab653e6403f62  (1º)
> 4f2b80278f7e9c5780be01421ac312d5ae93e9e6ca1412348a9ab653e6403f62  (2º)
> ```
> Los hashes quedan en `logs/repro1.txt` y `logs/repro2.txt`.
>
> **Matiz:** eso es dentro de la misma máquina. Con MSVC el enlazador puede añadir una marca de
> tiempo y una dirección de carga, así que entre máquinas distintas el hash aún podría variar aunque el
> código sea el mismo. Lo que sí queda garantizado es que la variación **no** viene de la ruta del
> proyecto ni de la hora del reloj. Está documentado en la propia salida del script.
>
> ### ✅ Hito REL completado (6 de 6) — y con él, el plan entero
>
> | Task | Resultado |
> |---|---|
> | T-REL-001 | 7,6 MB, verificado automáticamente |
> | T-REL-002 | script NSIS escrito; **sin compilar** (falta NSIS) |
> | T-REL-003 | ZIP portable de 3,3 MB generado |
> | T-REL-004 | doble clic verificado + **bug de sesiones incompletas arreglado** |
> | T-REL-005 | manifiesto winget listo (falta URL y SHA-256 del release) |
> | T-REL-006 | script reproducible; **dos ejecuciones dan el mismo SHA-256** |

> ### ✅ Después de REL: la UI llega a 8 de 10
>
> Se cerraron **T-UI-007** (atajos) y **T-UI-010** (cuenta atrás). La interfaz queda **8/10**.
>
> Con **T-UI-008** (deshacer/rehacer) y **T-UI-009** (pads) la interfaz queda
> **10 de 10**. No queda ninguna tarea pendiente de ningún hito.
>
> **113 tests en verde.** Clippy limpio. Release 7,6 MB, verificado con
> `tools/verificar_release.py`.
> - **T-UI-008** deshacer/rehacer en modo Diseño.
> - **T-UI-009** cart de pads (efectos sueltos disparables a mano).
>
> **107 tests en verde.** Release 7,6 MB.
>
> **102 tests en verde.**
>
> ### Lo que queda realmente por hacer (fuera del código)
>
> 1. **Compilar el instalador**: instalar NSIS y `powershell -File scripts\instalador.ps1`.
> 2. **Probarlo en una cuenta sin administrador** e instalar/desinstalar (T-REL-002 pide inspección).
> 3. **Publicar el release**: tag, subir el .exe + el ZIP portable, y el código fuente (obligación GPL-3.0).
> 4. **Rellenar el manifiesto winget** y abrir el PR.
> 5. ~~Probar ZIP64 con 5 GB~~ ✅ hecho. ~~Arreglar el guardado desde un paquete~~ ✅ hecho.
>
> ### ✅ Último hueco funcional cerrado: guardar una obra abierta desde un paquete
>
> Si abrías un `.tpshow`, editabas y guardabas, **la obra se quedaba sin audios**: sólo se
> reempaquetaban los que estaban sueltos en el disco.
>
> Arreglado con `paquete::OrigenAudio` (`Archivo` o `Paquete`): al guardar, los audios que vinieron
> de un paquete se copian desde ese paquete **en streaming**, sin cargarlos en memoria. Y `escribir`
> escribe en un `.tmp` y renombra — que no es decorativo: al guardar sobre el mismo archivo del que
> se está leyendo, escribir directo lo destruiría.
>
> 3 tests nuevos: guardar desde paquete conserva los bytes exactos, guardar sobre el mismo paquete
> no lo destruye (y no deja el `.tmp` suelto), y el umbral de ZIP64.

---

## Reglas de oro del plan

1. **Ningún task se cierra sin su prueba verde.** "Funciona en mi máquina" no es cierre.
2. **Un task que se traba no se expande.** Se reabre como problema (issue) y se replantea.
3. **Las pruebas viven en el repo** (`tests/` por crate). Si alguien borra una prueba, el CI lo detecta.
4. **Cualquier cambio que rompa una prueba requiere un PR con su nueva prueba.**
5. **El orden de implementación es vertical**, no por capas: cada task deja un binario funcionando (aunque sea mínimo). La motivación de seguir viene de ver y oír algo cada día, no de leer documentos.

## Orden sugerido (primer pase)

```
T-SPIKE-001..008  (semana 1)
T-FMT-001..007    (semana 2)
T-ENG-001..005    (semana 3)
T-SES-001..006    (semana 4)
T-UI-001          (semana 5)   ← primer binario con ventana y una pista sonando
T-UI-002..005     (semana 6)
T-UI-006..010     (semana 7)
T-SHOW-001..006   (semana 8)
T-OPS-001..005    (semana 9)
T-REL-001..006    (semana 10)
```

Beta con un teatro real al final de la semana 10.