# 02 — Stack técnico y ADR-001

## 1. Criterios de decisión (pesos)

| # | Criterio | Peso | Por qué importa aquí |
|---|---|---|---|
| C1 | **Calidad de audio en tiempo real** (sin cortes, sin GC, latencia baja) | 25 | Un glitch en función es un error visible |
| C2 | **Ligereza real** (tamaño + RAM + dependencias a instalar) | 25 | "Tiene que pesar nada" |
| C3 | **Ya resuelve fades / crossfade / loop / mixer / dispositivos** | 20 | "No reinventar la rueda" |
| C4 | **Un solo ejecutable, sin runtime que instalar** | 15 | Laptop prestada, sin admin, sin internet |
| C5 | **Velocidad de desarrollo de la UI** | 10 | Equipo chico |
| C6 | **Costo/licencia** | 5 | "Ayudar al que no tiene mucho" |

## 2. Comparativa de opciones

| Opción | C1 Audio | C2 Ligero | C3 Ya lo hace | C4 Sin runtime | C5 UI rápida | C6 Licencia | Total |
|---|---|---|---|---|---|---|---|
| **Rust + rodio + egui** | 9 | 10 | 9 | 10 | 7 | 10 | **9.05** |
| C++ + JUCE | 10 | 5 | 10 | 9 | 4 | 4 (JUCE 8: licencia/splash) | 7.45 |
| C# + NAudio + WPF/WinForms | 8 | 3 (self-contained 60–90 MB) | 7 | 2 (requiere .NET) | 9 | 10 | 6.15 |
| Tauri 2 (Rust + WebView) + rodio | 9 | 6 (WebView2 + 100 MB RAM) | 9 | 5 (WebView2 en Win10) | 10 | 10 | 7.65 |
| C++ + miniaudio + Dear ImGui | 10 | 10 | 5 (todo a mano) | 10 | 3 | 10 | 7.85 |
| Python + sounddevice/pyglet | 4 (GIL, latencia) | 2 (runtime) | 6 | 1 | 8 | 10 | 4.05 |
| Godot (motor de juegos) | 8 | 2 (40 MB+) | 7 | 8 | 8 | 10 | 6.20 |

**Nota sobre C++ + miniaudio:** es la opción más liviana en papel y sería perfecta si el equipo fuera grande. Pierde en C3 y C5: habría que escribir a mano el mixer, los fades, el loop, la decodificación de MP3 y el manejo de dispositivos. Es exactamente "reinventar la rueda", que el proyecto prohibe explícitamente.

**Nota sobre JUCE:** es el estándar de la industria y sería la elección si esto fuera un producto de audio profesional con plugins, multicanal y MIDI. Para "una lista con fades que pese nada" es un camión para llevar una bolsa: CMake, 20 MB de binario, curva de aprendizaje alta y licenciamiento que en 2026 no es trivial para distribución comercial.

## 3. Decisión

### ADR-001: Rust + rodio (cpal/Symphonia) + egui/eframe

**Estado:** Aceptado.

**Contexto:** Necesitamos un ejecutable de escritorio, mínimo, que reproduzca múltiples audios simultáneamente con fades y crossfades controlables en tiempo real, que recuerde configuraciones en sesiones, y que se distribuya a personas sin conocimientos técnicos ni presupuesto.

**Decisión:** Implementar en **Rust**, con:
- **rodio 0.22** para reproducción, decodificación, mezcla y fades (apoyado en **cpal 0.17** para el dispositivo y **Symphonia** para decodificar).
- **egui 0.36 / eframe 0.36** (backend **glow**) para la interfaz.
- **serde + serde_json** para las sesiones.

**Consecuencias positivas**
- Binario único, estático, sin runtime. Se copia y funciona.
- Sin recolector de basura: no hay pausas que corten el audio.
- El compilador garantiza que no hay carreras de datos entre el hilo de la UI y el hilo de audio.
- rodio ya trae el 90 % del motor de audio (ver tabla siguiente).
- Tamaño y RAM predecibles y pequeños.

**Consecuencias negativas / costos**
- Rust tiene curva de aprendizaje si el equipo no lo conoce.
- rodio 0.22 es una API reciente y con partes marcadas `experimental`; los cambios entre versiones menores rompen.  
  **Mitigación:** fijar versión exacta en `Cargo.lock`, encapsular rodio detrás de un trait `AudioBackend` propio (`03-arquitectura.md`), y un spike (M0) que valide los 5 puntos críticos antes de escribir la app.
- egui no tiene widgets "nativos" de Windows (diálogos de archivos, etc.).  
  **Mitigación:** usar `rfd` para diálogos nativos de abrir/guardar.

**Alternativa de escape:** si en el futuro se necesita multicanal, plugins VST o MIDI, el módulo de audio se reimplementa sobre JUCE sin tocar la UI ni el modelo de sesión (por eso el trait `AudioBackend`).

## 4. Mapa de dependencias (lo que ya está resuelto y NO hay que escribir)

| Necesidad | Librería | Versión | Qué aporte exacto |
|---|---|---|---|
| Salida de audio / WASAPI / enumerar dispositivos | `cpal` (transitivo de rodio) | 0.17 | Acceso a la salida de audífonos, buffer, sample rate |
| Reproducción + decodificación | `rodio` | 0.22 | `Decoder`, `Mixer`, `DeviceSinkBuilder` |
| Decodificar WAV/MP3/FLAC/OGG/M4A | `Symphonia` vía feature `symphonia-all` de rodio | — | No escribimos ni un decodificador |
| **Fade in** | `rodio::Source::fade_in(duration)` | — | Envolvente de subida |
| **Fade out** | `rodio::Source::fade_out(duration)` | — | Envolvente de bajada |
| Rampa de ganancia arbitraria | `rodio::Source::linear_gain_ramp(dur, start, end, clamp)` | — | Base para curvas custom |
| **Crossfade** | `rodio::Source::take_crossfade_with(other, dur)` → `Crossfade` | — | Mezcla de "uno sale / otro entra" |
| **Loop** | `rodio::Decoder::new_looped(file)` → `LoopedDecoder` | — | Loop infinito nativo |
| Loop N veces | `Source::repeat_infinite().take_duration(n * dur)` | — | Loop contado |
| Múltiples pistas simultáneas | `rodio::Mixer` / `MixerDeviceSink::mixer()` | — | Mezcla sin límite de pistas |
| **Elegir salida (audífonos)** | `DeviceSinkBuilder::from_device(dev)` / `.with_device(dev)` | — | Persistir el dispositivo elegido |
| Conversión dB ↔ lineal | `rodio::math::db_to_linear` | — | Volumen en dB |
| Cortar una pista | `Source::take_duration()` + `Source::stoppable()` | — | Ver §5 |
| Posición de reproducción | `Source::track_position()` → `TrackPosition::get_pos()` | — | Barra de progreso |
| Interfaz gráfica | `eframe` + `egui` | 0.36 | Ventana, widgets, renderizado (backend `glow`) |
| Diálogos de archivo nativos | `rfd` | 0.15 | Abrir/guardar sesión, elegir carpeta de audios |
| Persistencia de sesión | `serde` + `serde_json` | 1 | JSON legible |
| Rutas del sistema (config, documentos) | `directories` | 6 | Dónde guardar estado y sesiones |
| Hash de archivos de audio | `blake3` | 1 | Vinculación sesión ↔ audio (rápido en archivos grandes) |
| Log a archivo (release no tiene consola) | `tracing` + `tracing-appender` | 0.1 | Diagnóstico post-función |
| Errores tipados | `thiserror` (lib) / `anyhow` (app) | 2 / 1 | Manejo de errores claro |
| IDs | `uuid` | 1 | IDs estables de entradas |

**Total de dependencias directas: ~12.** Sin frameworks pesados, sin Chromium, sin .NET, sin Qt.

### `Cargo.toml` (referencia)

```toml
[package]
name = "teatroplayer"
version = "0.1.0"
edition = "2021"
rust-version = "1.87"   # MSRV de rodio 0.22

[dependencies]
rodio     = { version = "0.22", features = ["symphonia-all", "playback"] }
eframe    = { version = "0.36", default-features = false, features = ["glow", "default_fonts", "persistence", "x11", "wayland"] }
egui      = "0.36"
serde     = { version = "1", features = ["derive"] }
serde_json = "1"
rfd       = "0.15"
directories = "6"
blake3    = "1"
uuid      = { version = "1", features = ["v4"] }
tracing   = "0.1"
tracing-appender = "0.2"
thiserror = "2"
anyhow    = "1"

[profile.release]
opt-level  = "z"     # prioriza tamaño
lto        = true
codegen-units = 1
panic      = "abort"
strip      = true
```

## 5. El único código de audio propio (y por qué)

### 5.1 Corrección tras el spike: `Player` sí da handle por pista

El diseño original partía de una premisa que **resultó ser falsa**: se creía que
`Mixer::add()` no devuelve un handle y que por eso había que escribir un
envolvente que también gestionara el corte de la pista.

Verificado sobre el código de rodio 0.22.2 y en la máquina real
(T-SPIKE-006): sí lo da, pero no a través de `Mixer::add()`, sino de
**`Player::connect_new(&mixer)`**:

```rust
let sink = DeviceSinkBuilder::from_default_device()?.open_sink_or_fallback()?;
let player = Player::connect_new(sink.mixer());   // una pista independiente
player.append(decoder);
player.set_volume(0.8);   // volumen por pista
player.get_pos();         // posición para la barra de progreso
player.stop();            // cortar desde cualquier hilo
```

Cada `Player` es `Send + Sync` (comparte `Arc<Controls>`), así que se puede
mover a un `Arc` y cortarlo desde el hilo de la UI sin locks propios.
Medido: **8 ms** entre el `stop()` y la cola vacía.

Esto **elimina** del diseño propio el `stop_flag`, el `paused` y el
`frames_played`. rodio ya los resuelve.

### 5.2 Lo que sí sigue habiendo que escribir: la curva del fade

`Player::set_volume()` fija el volumen **de golpe**, no lo rampa, y las curvas
que trae rodio (`fade_in`, `fade_out`, `take_crossfade_with`,
`linear_gain_ramp`) son **lineales**.

Lineal es malo para un crossfade: en el centro del cruce ambas pistas valen
0.5, la suma de potencias cae a 0.707 y se oye un bache de volumen justo en el
momento que el público más nota.

**Solución:** `GainRamp<S>` (~150 líneas, `src/engine/envelope.rs`), que
envuelve cualquier `Source` y multiplica muestra a muestra por una ganancia
calculada **por índice de frame**, nunca con el reloj de pared (requisito de
determinismo, ver `12-determinismo-y-sin-ia.md`). Curvas: `Linear`,
`Exponential`, `EqualPower` (por defecto).

Medido sobre audio real, crossfade de 5 s entre un tono de 440 Hz y uno de
660 Hz (T-SPIKE-004, `scripts/verify_fade.py`, reproducible en
`tests/envelope_offline.rs`):

| Curva del crossfade | Variación de energía (RMS) a lo largo del cruce | Nivel en el centro respecto a los bordes |
|---|---|---|
| `Linear` (la que trae rodio) | **21,99 %** | **−29,3 %** — bache audible |
| `EqualPower` (la nuestra) | **0,00 %** | **0 %** — energía constante |

La cifra de la última columna es la teórica: con rampa lineal la suma de
potencias vale `(1-t)² + t²`, que en el centro cae a 0.5, y la amplitud con
ella a `√0.5 = 0.707`. El oído lo nota justo en el momento de más atención.

Es composición sobre la API pública de rodio (`Source`, `Mixer`, `Player`,
`Decoder`), no una reimplementación del motor: son ~150 líneas contra las
decenas de miles de rodio + cpal + Symphonia.

## 6. Spike obligatorio (M0)

Antes de escribir la app, validar con un prototipo de consola (200 líneas):

1. Enumerar salidas con `DeviceSinkBuilder` y abrir **la salida de audífonos** por nombre.
2. Reproducir 3 pistas simultáneas en el `Mixer` sin glitches.
3. Aplicar `fade_in` de 0.1 s y de 60 s.
4. Hacer un crossfade real A→B de 5 s con `take_crossfade_with`.
5. Loop infinito con `Decoder::new_looped` durante 10 minutos sin fuga de memoria.
6. Cortar una pista desde otro hilo con el patrón `stoppable().periodic_access(50ms, …)`.
7. Medir: latencia de salida, CPU con 8 pistas, RAM.

Si cualquiera de los 7 falla, se reevalúa rodio **antes** de haber construido nada encima. Ver `08-plan-de-implementacion.md`.

## 7. ADR-002 y ADR-003 (cerradas por los spikes)

### ADR-002: el backend de audio es rodio 0.22 directo, sin capa propia de mezcla

**Estado:** Aceptado y validado (T-SPIKE-001 … T-SPIKE-007).

**Contexto:** El MVP necesita varias pistas simultáneas, cada una con control
individual de volumen, corte y fade, sobre la salida de audífonos de Windows.
La tentación natural es escribir un "motor" propio de mezcla; la regla del
proyecto es no reinventar la rueda.

**Decisión:** No hay motor de audio propio. La topología es la de rodio:

```
DeviceSinkBuilder  ->  MixerDeviceSink  ->  Mixer
                                              ├── Player (pista 0) -> GainRamp -> Decoder
                                              ├── Player (pista 1) -> GainRamp -> Decoder
                                              └── Player (pista N) -> GainRamp -> Decoder
```

Un único `MixerDeviceSink` por aplicación (una sola salida física), un `Player`
por pista de la sesión, y nuestro `GainRamp` entre los dos para la curva.
El módulo `engine` de TeatroPlayer sólo orquesta estas piezas.

**Evidencia medida en la máquina de desarrollo (Windows, WASAPI):**

| Prueba | Resultado |
|---|---|
| Enumerar salidas y abrir la elegida | 4 endpoints listados; apertura a 48000 Hz / 2 canales |
| 3 pistas simultáneas 300 s | 0 errores de stream, 3/3 pistas vivas |
| Fade in de 0.1 s | arranca en x[0]=0.000000, sigue la curva (±1.7 %), sin clics |
| Fade in de 60 s | sigue la curva (±0.02 %), sin clics, llega a amplitud plena |
| Crossfade A→B de 5 s | energía constante (0.00 % de variación), 440 Hz → 660 Hz |
| Loop `Decoder::new_looped` | ver T-SPIKE-005 |
| Cortar desde otro hilo | 8 ms hasta cola vacía |

**Consecuencias:**
- El trait `AudioBackend` de `03-arquitectura.md` se mantiene, pero ya no es
  una abstracción defensiva frente a rodio: es el punto de sustitución para el
  día que haga falta multicanal real o VST (escape a JUCE).
- Los únicos números que dependen del backend (sample rate, número de canales)
  se leen de `MixerDeviceSink::config()` y nunca se asumen.

### ADR-003: decodificación delegada en Symphonia, cero decodificadores propios

**Estado:** Aceptado.

**Contexto:** En teatro los audios llegan en el formato que sea: WAV de
exportación, MP3 descargado, FLAC de un banco de sonidos, M4A de un móvil.
Escribir o mantener decodificadores está fuera de discusión.

**Decisión:** Toda la decodificación va por `rodio::Decoder`, que usa
**Symphonia** mediante el feature `symphonia-all`. Formatos soportados sin
escribir una línea: **WAV, MP3, FLAC, OGG/Vorbis, M4A/AAC, ALAC, CAF, MKV**.
No se usa ningún decodificador propio ni bindings a códecs del sistema.

- Loop: `Decoder::new_looped(file)` (streaming, sin cargar el archivo en RAM).
- Repetición de efectos cortos: `Source::repeat_infinite()`, que sí bufferiza
  en memoria; sólo para clips de pocos segundos.
- No se activa `Decoder::builder().with_seekable(true)`: no aporta nada al MVP
  y cuesta una lectura extra. Se reevaluará si se añade "saltar a marca".

**Consecuencias:**
- Licencia: Symphonia es **MPL-2.0**, compatible con la GPL-3.0 del proyecto
  (analizado en `11-open-source-y-licencias.md`, §3.3). El resto del árbol de
  dependencias es MIT/Apache-2.0. Sin fricción de licencias.
- Riesgo asumido: un MP3 con cabeceras raras delega el fallo en Symphonia.
  **Mitigación:** el error de decodificación se muestra al usuario con el
  nombre del archivo, no se silencia (ver FR-11 en `05-especificacion-funcional.md`).
