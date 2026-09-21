# 10 — Investigación de competencia y matriz de adopción

Investigación hecha en septiembre de 2026. Precios y características verificados en las fuentes citadas; cambian seguido, así que cada dato tiene fecha.

---

## 1. Competencia DIRECTA (software de cues para teatro)

### 1.1 QLab 5 — Figure 53 — macOS — estándar de la industria

**Precio:** gratis para audio estéreo y video básico; licencia paga para salidas múltiples, efectos de audio/video y show control.

**Funciones relevantes**

- Lista de cues con número, nombre, notas, color.
- Cues de audio, video, micrófono, cámara, texto, grupo, wait, start/stop, **Fade**, **Script**, MIDI, OSC, oscuridad/luces.
- **Fade cue**: en QLab "fade" = "cambio de un valor en el tiempo". Puede apuntar al volumen de un cue, a un patch de salida, al pan, al playback rate (hasta 33×), o a efectos.
- **5 formas de curva**: S-Curve (default, ease-in/ease-out), Custom (puntos de control arrastrables), Parametric (con campo "Intensity"), Linear, 2D Path.
- **Dominios de audio** para el fade: *slider* (emula una consola física, maximiza el rango audible útil), *decibel*, *linear*. Equal-power = curva paramétrica + dominio lineal; equal-gain = lineal + lineal.
- **Fades absolutos y relativos** (suman/restan al nivel actual). Techo de nivel configurable, default +12 dB.
- **"Stop target when done"**: el cue objetivo se detiene al terminar el fade.
- **Revert Fade** (⇧⌘R): deshace el fade y vuelve al estado previo.
- Pre-wait / post-wait, auto-follow, continue mode.

**Lo mejor de QLab (y qué nos llevamos)**
1. La **curva S por defecto** — confirmado: es la que suena natural.
2. El concepto de **dominio "slider"**: una escala 0–100 que imita una consola física, en vez de dB crudos. Ideal para quien no es ingeniero.
3. **Techo de nivel** para que nada explote.
4. **"Stop target when done"**.

**Qué NO nos llevamos:** el Fade cue como objeto independiente (requiere pensar en "cue que apunta a otro cue"), fades relativos, 5 curvas, automatización de parámetros, rate, efectos, video, MIDI, OSC. Todo eso es la complejidad que el usuario pide explícitamente evitar.

---

### 1.2 Go Button — Figure 53 — iOS — **el antecedente de "QLab simplificado"**

**Lo que hace:** construir playlists y correrlas tocando "GO".

**Funciones:** múltiples sonidos a la vez; lista en secuencia con GO; **"Hit buttons" siempre listos** para efectos improvisados; auto-stop, fade in, fade out, loop, **duck**; volumen, pan, velocidad y tono por cue; **modo pantalla completa con texto grande** (para leer desde lejos); control por Bluetooth y MIDI; OSC; **bloqueo con contraseña**; edición no destructiva.

**Lo mejor de Go Button**
1. **Los "Hit buttons"**: effects sueltos, siempre disponibles, que no forman parte de la secuencia. Re-suelve el 80 % de los imprevistos en una función.
2. **Duck**: bajar lo que suena sin apagarlo.
3. **Modo pantalla completa con texto gigante** — valida exactamente nuestro Modo Función.
4. **Bloqueo con contraseña** para que nadie edite por error.

---

### 1.3 Show Cue System (SCS) — Windows — el referente pago en Windows

**Precio (ene 2026, Lambert Studios):** Lite US$ 62 · Standard 107 · Professional 161 · Professional Plus 214 · Platinum 268.

**Funciones:** armar cues en orden; **nivel, pan, duración y selección de parlante por cue**; Hot Keys para disparar en cualquier momento; **Level Change cues** (cambiar el nivel dinámicamente); Control Send cues (MIDI); **Playlist cues** para música de pre-función e intervalo; cues de video hasta 4 proyectores.

**Lo mejor de SCS**
1. **Playlist cues**: una lista aparte para la música de antes del espectáculo y del intervalo, que no contamina la lista de cues. Simple y muy pedido.
2. **Hot keys** para disparar cualquier cue en cualquier orden.
3. **Level Change cues**: cambiar el volumen de lo que suena como acción explícita.

---

### 1.4 SFX — Stage Research / Timeline Theatrics — Windows

**Precio:** US$ 395 de lista; al momento de esta investigación el sitio indica que **toda la suite es gratuita por tiempo limitado**.

**Funciones:** drag-and-drop de audio a la lista de cues; cues especiales de **Wait** y **Volume Change**; múltiples efectos simultáneos e independientes; **hasta 16 salidas** (o 64 en estaciones dedicadas); ASIO; usado en Broadway y el West End londinense.

**Lo mejor de SFX:** la idea de que los cues **se superponen, son independientes y fáciles de configurar**; y que "casi cualquiera puede operarlo" — ese es el estándar de simplicidad que buscamos. El modelo de cues de Wait y Volume Change como ciudadanos de primera clase.

---

### 1.5 MultiPlay — Windows — **gratis** (donación voluntaria)

**Funciones:** audio mono/estéreo; lista de audios en secuencia (pre-función, intervalo); **pausas temporizadas**; cues de control que actúan sobre otros cues; strings serie/red; OSC; MIDI; imágenes; video; cues vinculados para sonar juntos o en sucesión, y para **detener o hacer fade de otros cues**; **cada cue asignado a un grupo de audio y cada grupo a una tarjeta de salida distinta**; **función de preview enrutable a otra salida**; producciones con nombre, guardables, cargables, **imprimibles** y exportables.

**Lo mejor de MultiPlay**
1. **Imprimir la hoja de cues.** En teatro el papel sigue siendo el backup real. Baratísimo de implementar y muy valorado.
2. **Grupos de audio → salida**: ruteo simple por grupo.
3. **Preview en otra salida**: escuchar el próximo cue sin mandarlo a la sala. (Lo diferimos: nuestro MVP tiene una sola salida.)

---

### 1.6 QPlayer — Windows — **open source** (C#/.NET + NAudio)

**Funciones:** listas de cues; **múltiples cues simultáneos**; fade in y fade out; **pausar y precargar cues**; **pre-delays por cue**; **EQ por cue y limitador global**; OSC. Además tiene **ShowMode** (deshabilita la edición para reproducción segura), **sistema de undo/redo completo**, multi-selección, y plugins (MagicQ CTRL, OSC).

**Lo mejor de QPlayer**
1. **Precargar cues** — garantiza arranque instantáneo sin depender del disco. Invisible para el usuario, enorme en confiabilidad.
2. **Pre-delay por cue** ("esperá 3 s y entrá").
3. **Limitador global** — protection contra picos.
4. **ShowMode** — confirma nuestro Modo Función.
5. **Undo/redo**.

---

### 1.7 LivePlay — Windows/macOS/Linux — **free + open source (AGPL-3.0)** ⚠️

**Es el competidor más importante del proyecto.** Hay que leerlo bien antes de seguir.

**Funciones**
- Motor de audio **C++ hablando directo con WASAPI / Core Audio / ALSA**. Cada cue es una instancia de reproducción independiente con su propio decoder y su propia envolvente de fade.
- Motor como **servidor standalone**: puede vivir dentro de la app o correr headless en otra máquina; **auto-descubrimiento en red**, múltiples clientes, **se relanza solo si se cae y retoma donde estaba**.
- **Limitador brick-wall en cada canal** + objetivos de loudness (EBU R128, streaming, radio, Netflix, live).
- **Metering broadcast** por cue, por canal y en el máster.
- **Ruteo multi-dispositivo** con matrix: un estéreo se puede partir y mandar cada canal a una tarjeta distinta con su ganancia.
- **Preview en dispositivo dedicado** (auriculares o bus de cue) sin tocar las salidas principales.
- **Generador de timecode SMPTE LTC** en su propio canal.
- **Editor de forma de onda** con trim, in/out, volumen, fades visuales, auto-trim por silencio.
- **Cart player**: los sonidos del cart **duckean automáticamente** el fondo al reproducirse.
- **Ducking automático** configurable.
- Loop, auto-trim de silencio, proyecto autocontenido portable, descarga de YouTube.
- Sin firmar: Windows muestra el aviso de SmartScreen.

**Lo mejor de LivePlay**
1. **Auto-recuperación del motor**: si se cae, se relanza y sigue. En teatro es oro.
2. **Ducking automático** en el cart player.
3. **Proyecto autocontenido** — confirma nuestro modelo de carpeta.
4. **Auto-trim de silencio** — muy útil para limpiar audios sin editor.

**Por qué todavía tiene sentido nuestro proyecto:** LivePlay es potente pero es un *sistema* (servidor + cliente + red + metering + ruteo matrix + timecode + editor de onda). Su curva de aprendizaje y su peso son de categoría profesional. Nosotros apuntamos al otro extremo: un archivo de ~10 MB, sin servidor, sin red, sin instalación, en español, que arranca en una laptop de hace diez años y se opera con la barra espaciadora. **No competimos en funciones; competimos en que lo pueda usar cualquiera.**

---

### 1.8 ZasCue — Windows — **gratis, cerrado**

**Formato:** un único archivo portable de ~68 MB, sin instalación, sin cuenta, sin avisos; funciona offline. Versión de navegador idéntica para Mac/Linux. **Interfaz en español e inglés.** Importa archivos `.qlab4`.

**Funciones:** cues de audio con efectos por cue (EQ, filtro, compresor, delay, reverb); **fades y crossfades**; **automatización de volumen a lo largo del track**; proyección de imágenes y títulos a segunda pantalla; cues de control (start/stop/arm); **cadenas de auto-follow**; **soundboard / pads**; MIDI.

**Roadmap público (ZasCue 2):** video en segunda pantalla, pantalla de operador real, **"lo que no usás, apagalo"** (cada feature tiene su switch), y **"avisos antes de la función, no durante"** (detectar archivos pesados o ilegibles al cargarlos).

**Lo mejor de ZasCue**
1. **Auto-follow / auto-continue**: una entrada dispara la siguiente sola.
2. **Pantalla de operador**: qué está sonando, **cuenta atrás legible desde lejos**, qué sigue, y un GO imposible de errar.
3. **"Lo que no usás, no se ve"** — principio de simplicidad por ocultamiento, muy valioso.
4. **Validar antes de la función, no durante.**
5. **Español como ciudadano de primera clase.**

**Nota de posicionamiento:** es el competidor más cercano en espíritu. Pero pesa ~68 MB y es cerrado. Nuestro ángulo: **open source, ~10 MB, y aún más simple.**

---

### 1.9 Sound Show — Windows/macOS/Linux — **gratis con upgrade Pro**

Paradigma **soundboard** en vez de lista de cues: cada cue abre su propio reproductor (volumen, loop, botones de control), capas ilimitadas, sistema de *Instructions* (pilas de cues), organización por acto/escena con **tabs, paginado, fila de favoritos, etiquetas de color y búsqueda**, integración con QLC+ para luces, proyección de video/imagen/texto.

**Lo mejor de Sound Show**
1. **Fila de favoritos** para lo que se usa a cada rato.
2. **Etiquetas de color y búsqueda** en listas largas.
3. Reconoce honestamente sus límites: *"brilla cuando necesitás flexibilidad y reacción rápida"*, no es un reemplazo 1:1 de un software de cues guionados.

---

### Otros mencionados en el nicho (menor relevancia para nosotros)

| Programa | Plataforma | Precio | Nota |
|---|---|---|---|
| CuePlayer (Baxel Data) | Windows | versión gratuita limitada / paga | Alternativa clásica |
| CSC Show Control | Windows | paga | Show control |
| StageQ | Windows | paga | Companion: controla QLab en una Mac desde PC |
| Trigger | Mac/Windows | paga | Audio y video |
| SpotOn | Windows | freeware | **Desactualizado desde 2014 — no recomendable por seguridad** |
| Go Button | iOS | gratis con límites | Ver 1.2 |

---

## 2. Competencia INDIRECTA

| Categoría | Ejemplos | Por qué la gente los usa en teatro | Por qué no alcanzan |
|---|---|---|---|
| **DAWs** | Ableton Live, REAPER ($60 licencia con descuento / $225 comercial) | Sonido envolvente, loops, escenas, edición fina | Exigen saber de audio; no tienen "lista de entradas con crossfades" ni modo operador a prueba de errores |
| **Reproductores comunes** | VLC, foobar2000, Windows Media | Ya están instalados y son gratis | No hay fades, ni crossfades, ni lista de cues, ni hotkeys. Terminan en "play/pausa a mano" |
| **Soundboards genéricas** | Soundplant, varios | Muy simples, por pads | Sin fades, sin secuencia, sin persistencia de sesión |
| **Presentaciones** | PowerPoint, Google Slides | Todo el mundo lo sabe usar | Audio pobre, sin control de niveles ni crossfades, y el "modo presentación" no es un modo operador |
| **Software de luces con audio** | QLC+ | Ya manejan la función | El audio es ciudadano de segunda |

**Lectura:** la mayoría de las producciones chicas hoy resuelve con VLC + una persona atenta, o con un DAW si hay alguien que sepa. El hueco real es: **una lista de entradas con fades y crossfades, guardable, que no requiera saber nada.** Ese hueco está ocupado en Mac por QLab, y en Windows por software pago, por software enorme, o por nada.

---

## 3. Lo mejor de cada uno — tabla consolidada

| # | Idea | Viene de | ¿La adoptamos? |
|---|---|---|---|
| 1 | Curva S (equal-power) como predeterminada en crossfades | QLab | ✅ Ya estaba |
| 2 | Volumen en **escala de consola 0–100**, no en dB | QLab (dominio *slider*) | ✅ **Adoptar** |
| 3 | Techo de nivel / **limitador de salida** | QLab, QPlayer, LivePlay | ✅ **Adoptar** — rodio ya trae `Source::limit()` |
| 4 | "Stop target when done" (liberar al terminar el fade) | QLab | ✅ Ya implícito |
| 5 | **Pads / "Hit buttons"** siempre listos | Go Button, ZasCue, Sound Show, LivePlay cart | ✅ **Adoptar** |
| 6 | **Ducking** (bajar lo que suena sin apagarlo) | Go Button, LivePlay, SCS | ✅ **Adoptar** |
| 7 | Modo pantalla completa / pantalla de operador con texto gigante | Go Button, ZasCue | ✅ Ya estaba (Modo Función) |
| 8 | **Cuenta atrás legible desde lejos** | ZasCue | ✅ **Adoptar** |
| 9 | Bloqueo contra edición accidental | Go Button, QPlayer ShowMode | ✅ Ya estaba |
| 10 | **Playlist aparte para pre-función e intervalo** | SCS | ✅ **Adoptar (P2)** |
| 11 | Hot keys | SCS, todos | ✅ Ya estaba |
| 12 | Cues de espera (Wait) y cambio de volumen como acciones | SFX, SCS, QLab | ✅ **Adoptar** (pre-delay) |
| 13 | **Imprimir / exportar la hoja de entradas** | MultiPlay | ✅ **Adoptar (P2)** |
| 14 | **Precargar el próximo cue** | QPlayer | ✅ **Adoptar** (automático, invisible) |
| 15 | **Pre-delay por entrada** | QPlayer, QLab, SFX | ✅ **Adoptar** |
| 16 | Undo/redo en el modo diseño | QPlayer | ✅ **Adoptar** |
| 17 | Auto-recuperación si el motor de audio se cae | LivePlay | ✅ **Adoptar** (reinicio del stream + reanudar) |
| 18 | **Auto-follow / auto-continue** | ZasCue, SCS, QLab | ✅ **Adoptar** |
| 19 | **"Lo que no usás, no se ve"** | ZasCue 2 | ✅ **Adoptar como principio** |
| 20 | **Validar antes de la función, no durante** | ZasCue 2 | ✅ **Adoptar como principio** |
| 21 | Proyecto autocontenido portable | LivePlay | ✅ Ya estaba |
| 22 | Auto-trim de silencio | LivePlay | ⏳ **Diferir (P3)** |
| 23 | Fila de favoritos, etiquetas de color, búsqueda | Sound Show | ⏳ **Diferir (P2)** — color al MVP |
| 24 | Español de primera clase | ZasCue | ✅ Ya estaba |
| 25 | Ruteo multi-salida / grupos | MultiPlay, SFX, LivePlay | ❌ **Rechazar** (fuera del MVP; rompe "una salida, audífonos") |
| 26 | Preview en salida dedicada | MultiPlay, LivePlay | ❌ **Rechazar** (requiere 2da salida) |
| 27 | EQ, compresor, reverb por cue | QPlayer, ZasCue | ❌ **Rechazar** |
| 28 | Editor de forma de onda / trim visual | LivePlay | ❌ **Rechazar** |
| 29 | MIDI, OSC, timecode LTC, luces (QLC+) | QLab, LivePlay, ZasCue, Sound Show | ❌ **Rechazar** |
| 30 | Video, imágenes, proyección | QLab, SCS, ZasCue | ❌ **Rechazar** |
| 31 | Servidor + cliente en red | LivePlay | ❌ **Rechazar** |
| 32 | Metering broadcast, loudness EBU R128 | LivePlay | ❌ **Rechazar** |
| 33 | Pan, velocidad, tono por cue | Go Button, SCS | ❌ **Rechazar** |
| 34 | Fades absolutos/relativos, 5 curvas, automatización | QLab | ❌ **Rechazar** (3 curvas: lineal, exponencial, S) |

---

## 4. Posicionamiento: por qué existe otro programa más

```
                    POTENCIA
                       ▲
        QLab ●         │        ● LivePlay
                       │   ● SFX / SCS
                       │        ● ZasCue (cerrado, 68 MB)
                       │   ● Sound Show
                       │        ● QPlayer
                       │
      ─────────────────┼─────────────────► SIMPLICIDAD
                       │
                       │  ★ TeatroPlayer
                       │     ~10 MB · open source
                       │     un archivo · sin red
                       │     español · GO y nada más
       VLC ●           │
    PowerPoint ●       │
                       ▼
```

**Nuestra promesa, en una frase:**
> Si sabés arrastrar un archivo y apretar la barra espaciadora, podés pasar el sonido de una obra.

**Diferenciadores concretos frente a los que ya existen**

1. **Peso y arranque:** ~10 MB y menos de 1 s para estar listo, contra 68 MB (ZasCue) o suites con servidor y red (LivePlay).
2. **Open source permisivo** (MIT/Apache) frente a cerrado (ZasCue, SCS, SFX) o AGPL (LivePlay) — ver `11-open-source-y-licencias.md`.
3. **Cero configuración de audio:** una sola salida, la de audífonos, elegida una vez y recordada. Sin matrices, sin ASIO, sin grupos.
4. **Español como idioma principal**, no como traducción.
5. **Corre en hardware viejo:** presupuesto de 2 cores y 4 GB de RAM.
6. **Sesión que se manda por WhatsApp:** una carpeta con el JSON y los audios adentro.

---

## 5. Anti-funciones (declaración explícita)

Para que la simplicidad sea una decisión y no una casualidad, estas cosas **no van a entrar nunca** salvo que el proyecto cambie de rumbo explícitamente:

- MIDI, OSC, timecode, Art-Net, control de luces.
- Video, imágenes, subtítulos, proyección.
- Efectos de audio (EQ, compresión, reverb, delay) por pista.
- Edición de audio, forma de onda, recorte.
- Múltiples salidas, matrices de ruteo, pan.
- Red, servidor, control remoto, cuentas, nube.
- Plugins, scripting, macros.
- Pestañas ilimitadas, modos, workspaces.

Si alguien necesita cualquiera de esas cosas, la respuesta honesta es: **usá LivePlay o QLab.** Y lo vamos a decir en el README.
