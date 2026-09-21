# 12 — Determinismo y prohibición de IA

Este proyecto es, por requisito explícito, **100 % determinista y 0 IA** (ni generativa ni tradicional). Este documento define qué significa eso, qué se prohíbe, y cómo se garantiza.

---

## 1. Lo que NO se incluye en el producto

| Categoría | Ejemplos | Estado |
|---|---|---|
| Modelos de lenguaje / LLM | llamadas a OpenAI, Claude, Gemini, etc. | ❌ Prohibido |
| Modelos generativos | TTS, generación de música, audio neural | ❌ Prohibido |
| Redes neuronales en el binario | Candle, tch, ONNX Runtime, TensorFlow Lite | ❌ Prohibido |
| Clasificación / clustering | detección automática de género, mood, similaridad | ❌ Prohibido |
| Búsqueda semántica | embeddings, "audio parecido a este" | ❌ Prohibido |
| Reconocimiento / transcripción | speech-to-text, detección de silencios por ML | ❌ Prohibido |
| Telemetría / "phone home" | analytics, reportes de uso, crash reports automáticos | ❌ Prohibido |
| Cuentas / nube | login, sync, servidor del autor | ❌ Prohibido |
| Decisiones "inteligentes" | auto-mix, sugerencia de fader, recomendación de pista | ❌ Prohibido |

**Todo lo que el programa hace se puede explicar como instrucciones explícitas sobre muestras de audio y un modelo de datos determinista.**

Las únicas "decisiones automáticas" que existen son las que el usuario configuró de forma explícita (fades, loops, ducking, auto-follow, limitación de salida). Ningún sistema aprende del uso del usuario.

---

## 2. Qué significa "determinista" aquí

### 2.1 Determinismo de reproducción

Misma sesión + mismo dispositivo + misma entrada del reloj de audio = **idéntica secuencia de muestras hasta el bit**.

- Las curvas de fade se evalúan **por muestra** con aritmética de punto flotante determinista (`f32`, suma en orden fijo). No se usa `Instant::now()` para calcular ganancias — eso sería no-determinista (depende del scheduler del SO).
- Los tiempos (`preDelayMs`, `durationMs`, `autoFollow`) se convierten a **cantidad de muestras** al arrancar el cue, y se cuenta con un contador de frames dentro del callback de audio. La unidad de verdad es la **muestra**, no el milisegundo de pared.
- El orden de inserción en el `Mixer` de rodio es estable: las pistas se agregan en el orden en que fueron disparadas; el mezclado es por suma en punto fijo (rodio convierte a f32 y suma en orden de iteración).
- La precarga del próximo cue (B8) se hace en un hilo de I/O separado con un decoder por pista; el orden de decodificación es por `cues[i]` índice.

### 2.2 Determinismo del archivo de sesión

`sesion.json` se serializa con `serde_json` con:
- Claves **ordenadas alfabéticamente** (BTreeMap, no HashMap).
- Sin campos nulos: `Option::None` se omite, no se escribe `null`.
- Sin números con precisión flotante (`f32`/`f64` se evitan en el modelo: tiempos son enteros en `ms`; ganancias son `i32` milésimas de dB; volumen es `i32` 0–1000; porcentajes 0–100; curvas son enums).
- Fechas en formato RFC 3339 con zona fija o UTC, no `localtime()`.

El archivo `.tpshow` (contenedor ZIP) se construye con:
- Entradas ordenadas por nombre (`BTreeMap` o sort).
- Método **STORE** (sin compresión) — el audio ya está comprimido y JSON es chico, así que no se gana nada con DEFLATE y se pierde determinismo entre compresores.
- Timestamp fijo `1980-01-01 00:00:00` (formato DOS del ZIP) para todas las entradas.
- Sin permisos UNIX, sin atributos extendidos, sin campos NTFS.
- Nombres de archivo en UTF-8 NFC normalizado.
- CRC-32 calculado por el writer (ya lo hace cualquier impl ZIP).

**Resultado:** `cargo run --bin pack -- sesion.json audio/ -o show.tpshow` produce **los mismos bytes** en cualquier máquina, en cualquier momento.

### 2.3 Determinismo de builds

Reproducible builds:
- `Cargo.lock` se commitea, no se regenera.
- `rust-toolchain.toml` pin exacto (ej. `1.87.0`).
- Variable de entorno `SOURCE_DATE_EPOCH=0` durante el build → fechas de compilación deterministas.
- Sin parches al árbol de fuentes durante el build (no `git apply` en CI).
- Hash del commit embebido en el binario (se calcula una vez, no en runtime).

Para verificar: dos builds consecutivos en máquinas distintas deben producir binarios **byte-idénticos** (salvo la diferencia que introduce el linker en Windows por las direcciones de carga — aceptable, se documenta).

### 2.4 Determinismo de UI

- Posición y tamaño de ventana: se restauran del último uso (egui `persistence`), no se eligen aleatoriamente.
- Orden de cues: siempre el orden del array `cues[]` en JSON.
- Orden de pads en la franja inferior: por `cues[].pad == true`, en orden de índice.
- Selección inicial: primera pista del array.
- Sin animaciones que dependan del reloj del sistema; las únicas animaciones son: barra de progreso de la pista (derivada de `TrackPosition::get_pos()`) y pulse de "sonando" (opcional, derivado de la posición del audio).

---

## 3. Verificación (tests de determinismo)

Cada release pasa por:

```bash
# 1. Build reproducible
cargo build --release
git diff --stat target/release/teatroplayer.exe  # comparar con último release

# 2. Pack determinista
./teatroplayer.exe pack ./ejemplos/MiObra/ /tmp/out1.tpshow
./teatroplayer.exe pack ./ejemplos/MiObra/ /tmp/out2.tpshow
sha256sum /tmp/out1.tpshow /tmp/out2.tpshow   # deben coincidir

# 3. Reproducción determinista (mismo cue, mismo dispositivo, dos veces)
#    → capturar 30 s de salida y comparar muestras
```

Además, **un test automatizado** corre el mismo plan de audio dos veces y compara byte-a-byte la salida capturada por el dispositivo nulo (cpal null backend cuando esté disponible, o un backend de test que escribe a WAV).

---

## 4. Por qué esto importa, no solo filosofía

1. **Confiabilidad en función.** Si dos ejecuciones con la misma sesión producen sonidos distintos, hay un bug o un no-determinismo oculto. Mejor que aparezca en un test que en una función.
2. **Archivos firmables.** Una obra se puede hashear y firmar; un audio + sesión reproducible es una pieza verificable, útil para críticas, archivo, repetición exacta.
3. **Seguridad.** Sin IA hay cero riesgo de prompt injection, alucinaciones, fuga de datos o dependencia de un servicio externo caído justo el día de la función.
4. **Tamaño y arranque.** Sin runtime de ML ni SDK de IA, el binario se mantiene en ~10 MB y arranca en <1 s.
5. **Portabilidad.** Funciona en una laptop de hace diez años porque no pide GPU ni NPU.

---

## 5. Política de PRs y de issues

- Cualquier PR que introduzca una dependencia con red, con un binario de modelo, o que llame a un servicio externo, **se rechaza sin discusión**.
- Issues del tipo "agregar IA para X" se cierran con referencia a este documento.
- Si algún día alguien quisiera una función "smart", el camino es: **primero publicarla como librería opcional descargable aparte**, no en el binario principal.

---

## 6. Lo que SÍ se incluye y es "automático" (pero no IA)

- Limitador de salida: `Source::limit(...)` de rodio — DSP clásico, determinista.
- Auto-trim de silencio (P3): un detector de silencio por umbral de RMS con histéresis. Determinista, sin red.
- Generación de formas de onda para la fila (P2): lectura de muestras en bloques y submuestreo para un PNG. Determinista.
- Conversión de dB ↔ lineal: `rodio::math::db_to_linear`. Determinista.

Todo lo que se hace automáticamente se hace con **reglas explícitas escritas en el código, sin parámetros aprendidos**.