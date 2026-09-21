# 05 — Especificación funcional

Prioridades: **P1** = indispensable para el MVP · **P2** = deseable · **P3** = futuro.

## FR-01 — Cargar audios de una carpeta (P1)

- Botón **"Agregar audios…"** → diálogo nativo multiselección.
- Botón **"Cargar carpeta…"** → importa todos los audios soportados.
- Formatos: WAV, MP3, FLAC, OGG, M4A/MP4 (los que da Symphonia).
- Al importar: preguntar **"¿Copiar los audios dentro de la sesión?"** — predeterminado **Sí**.
- Orden alfabético inicial, reordenable por arrastre y con botones ↑/↓ (arrastrar es el método principal; los botones existen por si el mouse de la laptop falla).
- Duplicar y eliminar entrada. Renombrar con doble clic.

## FR-02 — La entrada (P1)

Cada fila de la lista es una **entrada**. La fila muestra: número, nombre, y un resumen legible de su configuración.

Ejemplos de resumen que se muestran en la fila (texto, no íconos crípticos):

| Configuración | Texto en la fila |
|---|---|
| `hit` + `keep` | "Entra de golpe · se encima" |
| `fadeIn 4s` + `keep` | "Entra en 4 s · se encima" |
| `fadeIn 5s` + `fadeOutPrev 3s` | "Crossfade · entra 5 s / sale 3 s" |
| `hit` + `stopPrev` | "Cambia de golpe" |

## FR-03 — Fade in (P1)

- Rango: **0.1 s a 60.0 s**.
- Control: deslizador + campo numérico editable. Pasos: 0.1 s hasta 2 s · 0.5 s hasta 10 s · 1 s hasta 60 s.
- Curva: Lineal / Exponencial / Equal-power. Predeterminado: **Equal-power**.
- Valor 0 ⇒ "entra de golpe" (equivalente a `hit`).
- Siempre se aplica un **anti-clic** de 5 ms al arrancar, incluso en entradas de golpe.

## FR-04 — Fade out (P1)

- Se dispara de tres formas:
  1. Botón **SALIR** sobre la pista (o la tecla asignada).
  2. Automático al disparar otra entrada cuyo `onPrevious.kind = fadeOut`.
  3. **STOP** global (con `emergencyFadeMs`, predeterminado 50 ms).
- Mismo rango y curvas que el fade in.

## FR-05 — Crossfade (P1)

- Se configura **en la entrada que entra**, no en la que sale. Es más fácil de explicar: *"esta entrada entra en 5 s y saca a la anterior en 3 s"*.
- Cuando se dispara una entrada con `onPrevious.kind = fadeOut`:
  - Si **hay una pista activa**, se le aplica la rampa de salida con su duración y curva.
  - Si **hay varias**, se aplica a la **última que entró** (comportamiento predecible). *(P2: selector explícito de qué pista sacar.)*
  - Si **no hay ninguna**, la configuración se ignora sin error.
- Las duraciones de entrada y salida son **independientes** (5 s / 3 s es válido).

### Matriz de combinaciones (las que el usuario pidió textualmente)

| Lo que se quiere | `entrance` de la nueva | `onPrevious` de la nueva | Resultado |
|---|---|---|---|
| "Audio A entra con fade in y audio B entra de golpe" | A: `fadeIn 5s` / B: `hit` | ambos `keep` | A aparece despacio; después B irrumpe y quedan los dos sonando |
| Crossfade clásico | `fadeIn 5s` | `fadeOut 5s` | A se va mientras B llega |
| Crossfade asimétrico | `fadeIn 8s` | `fadeOut 2s` | B tarda en aparecer, A se va rápido |
| Cambio de escena seco | `hit` | `stop` | corte limpio |
| Ambiente que se queda | `fadeIn 10s` + loop infinito | `keep` | base sonora permanente |
| Pisar sin sacar | `hit` | `keep` | capas |

## FR-06 — Loop (P1)

- Tres modos: **Sin loop / Infinito / N veces** (N entre 2 y 999).
- Implementación: `Decoder::new_looped` (infinito) y `repeat_infinite().take_duration()` (N veces).
- Aviso automático si el audio con loop es MP3: *"Los MP3 pueden tener un salto al repetir. Si lo notás, exportalo como WAV."*
- *(P2)* Loop con fade por vuelta (para ambientes que respiran).

## FR-07 — Volumen (P1)

- Volumen por entrada: **−60 dB … +6 dB**, predeterminado 0 dB. Mostrado en dB (el diseñador lo entiende) con barra.
- Volumen máster en la barra superior, mismo rango.
- Cambios de volumen se aplican con *smoothing* de 20 ms (nunca escalones → nunca clics).

## FR-08 — Salida de audífonos (P1)

- Selector de dispositivo en la barra superior: lista de salidas detectadas (cpal/WASAPI).
- Se recuerda el dispositivo por **nombre**; si al abrir no está, se usa el predeterminado del sistema y se avisa.
- Botón **"Probar salida"**: emite un tono de 1 s para confirmar que el cable va a la consola correcta.
- Si el dispositivo desaparece durante la función (se desenchufó), reconexión automática al predeterminado + aviso en pantalla.

## FR-09 — STOP de emergencia (P1)

- Botón rojo siempre visible, en ambos modos, y atajo **Esc**.
- Aplica un fade de `emergencyFadeMs` (50 ms) a **todas** las pistas activas y las detiene.
- Nunca pide confirmación. Nunca se deshabilita.

## FR-10 — Sesiones (P1)

- Nueva / Abrir / Guardar / Guardar como / Recientes (últimas 5).
- Auto-guardado con respaldo rotativo (ver `04-modelo-de-datos.md`).
- Al abrir, verificación de audios y relocalización guiada de los faltantes.
- La última sesión se reabre sola al arrancar el programa.

## FR-11 — Modo Función (P1)

- Se entra con un botón **"Función"** (o F5). Se sale con Esc dos veces (para que un Esc aislado no saque del modo por error).
- Diferencias respecto al modo Diseño:
  - Sin edición posible: no hay campos, no hay arrastre, no hay menús de archivo.
  - Filas altas (64 px), tipografía grande, contraste alto.
  - El espacio de la derecha es un botón **GO** por fila.
  - Barra inferior con **◀ ANTERIOR / SIGUIENTE ▶** y **SALIR**.
- **ESPACIO** dispara la siguiente entrada de la lista (puntero visible en la fila). **Enter** dispara la seleccionada.
- Atajos F1–F12 asignables por entrada (P1 para F1–F8).

## FR-12 — Atajos de teclado (P1)

| Tecla | Acción |
|---|---|
| `Espacio` | Disparar la siguiente entrada |
| `Enter` | Disparar la entrada seleccionada |
| `Esc` | STOP de emergencia (doble Esc en modo Función sale del modo) |
| `F1`–`F8` | Entrada asignada |
| `↑` / `↓` | Mover selección |
| `Ctrl+S` | Guardar sesión |
| `F5` | Alternar modo Función |

## FR-13 — Funciones adoptadas del benchmark competitivo

Resultado de investigar QLab, Go Button, Show Cue System, SFX, MultiPlay, QPlayer, LivePlay, ZasCue y Sound Show (`10-competencia-y-benchmark.md`). Cada ítem entró solo si se explica en una frase y no agrega un control permanente en pantalla.

| # | Función | Origen | Pri. | Cómo aparece en la interfaz | Implementación |
|---|---|---|---|---|---|
| B1 | **Volumen en escala de consola 0–100** | QLab (dominio *slider*) | P1 | Slider 0–100. En modo Diseño se ve el dB equivalente entre paréntesis | Mapear 0–100 → dB con curva perceptual; `rodio::math::db_to_linear` |
| B2 | **Techo de salida / limitador** | QLab, QPlayer, LivePlay | P1 | Invisible. "Protección de volumen" siempre activa | `rodio::Source::limit(LimitSettings)` en el máster |
| B3 | **Pads / efectos sueltos** | Go Button "Hit buttons", ZasCue, Sound Show | P1 | Franja inferior "Efectos": botones grandones, siempre listos | Disparo inmediato; no alteran la secuencia; cada uno con su propio fade out opcional |
| B4 | **Ducking (bajar lo que suena)** | Go Button, LivePlay, SCS | P1 | Opción nueva en "qué pasa con lo que suena": *Bajar a ___ %* | `onPrevious.kind = "duck"` con `levelPercent` y `durationMs` |
| B5 | **Espera antes de entrar (pre-delay)** | QPlayer, QLab, SFX | P1 | "Esperar ___ s y entrar" | Campo `preDelayMs` |
| B6 | **Auto-continuar** | ZasCue, SCS, QLab | P1 | "Después, disparar la siguiente sola" + tiempo opcional | `autoFollow: none \| afterMs(n) \| whenThisEnds` |
| B7 | **Cuenta atrás legible desde lejos** | ZasCue | P1 | Modo Función: número grande con lo que falta de la entrada actual | Derivado de la posición de la pista |
| B8 | **Precarga de la próxima entrada** | QPlayer | P1 | Invisible | Al seleccionar la siguiente, abrir decoder y pre-buffer en background |
| B9 | **Deshacer / rehacer** | QPlayer | P1 | Ctrl+Z / Ctrl+Mayús+Z, solo en modo Diseño | Snapshots del modelo de sesión (es chico, cabe en memoria) |
| B10 | **Auto-recuperación del motor** | LivePlay | P1 | Aviso breve; la música vuelve | Si el stream de cpal muere: reabrir y reanudar la pista activa |
| B11 | **Color por entrada** | Sound Show | P1 | Pastilla de color en la fila; siempre con palabra además del color | Campo `color` |
| B12 | **Imprimir / exportar la hoja de entradas** | MultiPlay | P2 | "Imprimir hoja" → PDF o texto | Sin dependencias pesadas: generar tabla simple |
| B13 | **Lista aparte para pre-función e intervalo** | SCS | P2 | Solapa "Música de antes y del intervalo" | Segunda lista, mismo motor |
| B14 | **Favoritos y búsqueda** | Sound Show | P2 | Campo de búsqueda + fila de favoritos | Solo si la lista pasa de ~25 entradas |
| B15 | **Bloqueo con PIN** | Go Button | P2 | "Bloquear edición con PIN" | Opcional, desactivado por defecto |
| B16 | **Auto-recorte de silencio** | LivePlay | P3 | — | Requiere análisis de audio; fuera del MVP |

### Rechazadas explícitamente (con motivo)

MIDI · OSC · timecode · luces · video/imágenes · EQ/compresión/reverb por pista · multicanal y matrices de ruteo · pan · velocidad y tono · editor de forma de onda · servidor y control por red · metering broadcast y loudness EBU R128 · fades relativos y automatización de parámetros.

Motivo único: **cada una agrega un concepto que el operador tendría que entender.** Si se necesitan, el usuario tiene que ir a LivePlay o QLab, y el README lo va a decir.

## FR-14 — Requisitos no funcionales (presupuesto)

| Requisito | Objetivo | Cómo se garantiza |
|---|---|---|
| Tamaño del ejecutable | 6–12 MB | `opt-level="z"`, `lto`, `strip`, backend `glow` de egui (no wgpu) |
| Instalador comprimido | 3–6 MB | NSIS con compresión LZMA |
| RAM en reposo | < 60 MB | sin WebView, sin runtime, decodificación bajo demanda |
| Arranque en frío | < 1 s | sin carga diferida innecesaria, sin escaneo de red |
| CPU con 8 pistas | < 5 % en laptop de gama baja | solo mezcla + ganancia, sin efectos |
| Latencia GO → audio | < 30 ms | buffer WASAPI 256–512 frames |
| **Sin ventana de consola en release** | obligatorio | `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` (ver `07`) |
| Sin conexión a internet | obligatorio | no hay red en el código |
| Sin permisos de administrador | obligatorio | instalación por usuario |

## FR-15 — Deseable (P2)

- Empaquetar sesión como `.tpk` (ZIP portable).
- Forma de onda dibujada en la fila (útil y barata: se precalcula y cachea).
- Pre-escuchar un audio en los auriculares de edición sin afectar la salida (solo si hay 2 dispositivos).
- Elegir explícitamente qué pista sacar en un crossfade.
- Temporizador: "reproducir la siguiente entrada automáticamente a los 30 s".
- Nota de texto libre por entrada visible para el operador ("esperar a que cierren la puerta").
- Historial de lo ejecutado en la función (para repetir el show igual).

## FR-16 — Futuro (P3)

- Disparo por MIDI / OSC / teclado global en background.
- Salida multicanal a interfaz USB (4+ canales).
- Salida simultánea a dos dispositivos (consola + retorno).
- macOS y Linux.
