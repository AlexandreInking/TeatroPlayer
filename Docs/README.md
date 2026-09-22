# TeatroPlayer — Diseño de producto

> Reproductor de audio para teatro con **fade in / fade out / crossfades en tiempo real**, pensado para que lo opere alguien que "sabe poco y no tiene mucho".
> Local, instalable, se abre en el escritorio, sin consola, ultraligero y visualmente simple.

**Estado:** diseño cerrado + **hitos SPIKE (8/8), ENG (5/5), FMT (7/7), SES (6/6), SHOW (6/6), OPS (5/5), REL (6/6),
UI (10/10) y EVT (7/7) completados**. **La app ya se puede usar en una obra.** Se abre con doble clic desde
`TeatroPlayer.lnk`, carga una carpeta de audios, los suena con fades y crossfades, monta **eventos
predefinidos** (escenas montadas con un audio, guardadas y reutilizables), y **guarda la obra
entera en un único `.tpshow`** con los audios dentro: se manda por WhatsApp y funciona en cualquier
máquina. **Versión 0.3.0. 151 tests en verde** (más 1 ignorado: el de 5 GB).

Los tres artefactos de la release se generan y se verifican (instalador NSIS de 2,83 MB, ZIP
portable de 3,4 MB, ejecutable de 7,7 MB sin consola), pero **el release todavía no se ha
publicado**: falta configurar el remoto de Git y decidir el presupuesto de RAM (riesgo R16).
Detalle en `14-como-publicar.md` §6 y §7.

**Desarrollo completo: todos los hitos cerrados y los huecos técnicos resueltos** (ZIP64
probado con 5 GB, y guardar una obra abierta desde un paquete).

El guion paso a paso para publicar está en **`Docs/14-como-publicar.md`**: hay un script que
automatiza tests, release, aviso de terceros, ZIP portable, instalador y el SHA-256 que pide
winget. Lo que queda a mano es compilar el instalador (hace falta NSIS), probarlo en una
cuenta sin administrador, y subir el release con su código fuente (obligación GPL-3.0).

> **Validación real (2026-09-20, Windows/WASAPI):** 3 pistas simultáneas durante 300 s sin errores
> de stream (CPU 7.51 % de un núcleo en debug); fade de 0.1 s y de 60 s sin clics y con la curva exacta
> al 0.02 %; crossfade de 5 s con energía constante (0.00 % frente al 21.99 % de una rampa lineal);
> loop de 600 s sin errores y con +0.3 MB de RAM; corte de pista desde otro hilo en 8 ms.
> Ejecutable de release: **107 KB**.

---

## Resumen ejecutivo

Un único ejecutable que:

1. Carga una carpeta de audios y los muestra como una **lista de entradas** (playlist).
2. Permite configurar **cómo entra** cada audio: de golpe, con fade in (0.1 s – 60 s), o **crossfade** contra el audio que está sonando. La rampa va **de cualquier porcentaje a cualquier otro**, no sólo de 0 a 100.
3. Permite configurar **cómo sale**: de golpe, con fade out, o que se quede sonando.
4. Opcionalmente **hace loop** de un audio (infinito o N veces) para ambientes.
5. Monta **eventos predefinidos** en una segunda lista: un audio que entra (fade in), un **intercambio** con el que está sonando (crossfade), un **fade out** que baja lo que suena sin elegir nada, o un **disparo único** sin fade para efectos de golpe. Se guardan con la obra y se editan sobre la marcha: alargar una escena es mover un número.
6. Guarda todo eso en una **sesión** (`.tpshow`) que se abre con doble clic y **lleva los audios dentro**, de modo que el operador solo tiene que apretar botones.
7. Sale por la **salida de audífonos** de la laptop (que en teatro va con cable directo a la consola).

Dos modos de uso: **Diseño** (quien arma la sesión) y **Función** (quien la ejecuta: botones gigantes, sin menús).

---

## Decisión técnica de un vistazo

| Capa | Elección | Motivo |
|---|---|---|
| Lenguaje | **Rust** | Binario único y estático, sin runtime que instalar, sin recolector de basura (no hay cortes de audio por GC), seguro en concurrencia. Es lo más cercano a "C++ para audio" sin pagar el costo de C++. |
| Audio | **rodio 0.22** (sobre **cpal** / WASAPI, decodificador **Symphonia**) | Ya trae `fade_in`, `fade_out`, `linear_gain_ramp`, `take_crossfade_with`, `Decoder::new_looped` (loop), `Mixer` (muchas pistas simultáneas) y **selección de dispositivo de salida**. No reinventamos nada. |
| Interfaz | **egui / eframe 0.36** con backend **glow** | GUI en Rust puro, sin WebView, sin Chromium, sin .NET. Es lo más liviano que da una UI cómoda. |
| Sesiones | **JSON** (`serde_json`) en carpeta autocontenida | Legible, depurable, portable, sin base de datos. |
| Instalador | **NSIS** vía `cargo-packager` (o Inno Setup) | Un `.exe` de instalación por-usuario, sin permisos de administrador. |

Presupuesto: **ejecutable 6–12 MB** ✅ (6,7 MB reales), instalador comprimido **3–6 MB**,
arranque **< 1 s**. **RAM < 60 MB: ❌ no se cumple**, la app con interfaz usa 141 MB (riesgo R16).

Regla de oro del proyecto: **si existe una librería que ya lo hace, se usa.** El único DSP propio es
`GainRamp` (~150 líneas, `src/engine/envelope.rs`), y solo por la **curva**: las que trae rodio son
lineales y un crossfade lineal tiene un bache de volumen del 29 % en el centro. El control por pista
**sí lo da rodio** (`Player::connect_new`), así que de eso no escribimos nada (ver `02-stack-tecnico.md` §5).

---

## Índice de documentos

| # | Documento | Contenido |
|---|---|---|
| 01 | [`01-producto.md`](01-producto.md) | Problema, usuarios, principios, alcance del MVP, qué queda fuera |
| 02 | [`02-stack-tecnico.md`](02-stack-tecnico.md) | Comparativa de lenguajes/frameworks, ADR-001, mapa de dependencias |
| 03 | [`03-arquitectura.md`](03-arquitectura.md) | Capas, módulos, hilos, flujo de audio, estados de una pista, crossfade |
| 04 | [`04-modelo-de-datos.md`](04-modelo-de-datos.md) | Esquema de la sesión JSON y **vinculación a archivos de audio** |
| 05 | [`05-especificacion-funcional.md`](05-especificacion-funcional.md) | Funciones detalladas: fades, crossfades, loop, salida, atajos, requisitos no funcionales |
| 06 | [`06-ux-y-diseno-visual.md`](06-ux-y-diseno-visual.md) | Wireframes, sistema visual, modo Diseño vs modo Función |
| 07 | [`07-empaquetado-instalacion.md`](07-empaquetado-instalacion.md) | Build release sin consola, instalador, tamaño, distribución |
| 08 | [`08-plan-de-implementacion.md`](08-plan-de-implementacion.md) | Hitos M0–M6, definition of done, plan de pruebas |
| 09 | [`09-riesgos-y-abiertos.md`](09-riesgos-y-abiertos.md) | Riesgos, mitigaciones, supuestos asumidos y preguntas abiertas |
| 10 | [`10-competencia-y-benchmark.md`](10-competencia-y-benchmark.md) | Investigación de competencia directa e indirecta, lo mejor de cada programa, matriz de adopción y anti-funciones |
| 11 | [`11-open-source-y-licencias.md`](11-open-source-y-licencias.md) | Licencia **GPL-3.0-or-later**, copyleft, obligaciones de release, CLA vs DCO, donaciones |
| 12 | [`12-determinismo-y-sin-ia.md`](12-determinismo-y-sin-ia.md) | Garantías de determinismo, prohibición de IA, reproducible builds |
| 13 | [`13-formato-de-archivo-unico.md`](13-formato-de-archivo-unico.md) | Formato `.tpshow` (ZIP STORE determinista con `sesion.json` + audio adentro) |

Acceso directo en la raíz del proyecto: **`TeatroPlayer.lnk`** → apunta al futuro `target\release\teatroplayer.exe`. Funcionará cuando el binario exista (T-REL-001); hasta entonces Windows avisa que el destino no está. Se genera con `tools/make_shortcut.py`.

---

## Competencia: por qué existe otro programa más

Investigado en profundidad en `10-competencia-y-benchmark.md`. El mapa del nicho en Windows hoy es:
software pago (Show Cue System desde US$ 62, SFX US$ 395 de lista), software libre pero grande y con servidor/red (**LivePlay**, AGPL, multiplataforma), software cerrado portable de ~68 MB (**ZasCue**), un soundboard y no una lista de cues (**Sound Show**), y proyectos open source crudos (**QPlayer**, C#/NAudio).

Lo que falta en Windows es exactamente el hueco de este proyecto: **algo de ~10 MB, libre, sin instalación, en español, que se opera con la barra espaciadora.**

Del benchmark se adoptaron 16 funciones (pads siempre listos, ducking, auto-continue, pre-delay, cuenta atrás legible, precarga, deshacer, limitador de salida, escala de volumen tipo consola, auto-recuperación del motor, color por entrada, etc.) y se **rechazaron explícitamente** MIDI, OSC, video, efectos por pista, multicanal, red y edición de audio: cada uno agrega un concepto que el operador tendría que entender.

**Licencia: GPL-3.0-or-later.** Open source con copyleft fuerte: cualquiera puede usar, modificar y compartir el programa — y si alguien distribuye una versión modificada, **tiene que publicar el código**. Eso garantiza que lo que salga de acá siga siendo open source. Detalle, obligaciones de release y compatibilidad de dependencias en `11-open-source-y-licencias.md`.

---

## Supuestos asumidos (a confirmar)

1. **Windows es la plataforma principal** (la laptop de teatro). El diseño no bloquea macOS/Linux, pero no se construye para ellos en el MVP.
2. Se instala **por usuario**, sin permisos de administrador.
3. Formatos de audio del MVP: **WAV, MP3, FLAC, OGG, M4A** (todos los que da Symphonia). Se recomendará WAV/FLAC para función.
4. Salida estéreo 2 canales por la salida de audífonos.
5. No se requiere control MIDI ni disparo por red en el MVP.

Estos supuestos están detallados y justificados en `09-riesgos-y-abiertos.md`.
