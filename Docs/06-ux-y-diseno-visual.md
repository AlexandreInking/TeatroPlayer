# 06 — UX y diseño visual (dirección clonada de la competencia)

## 1. Lo que se ve en los demás — y que vamos a clonar

Miré capturas reales de **ZasCue**, **LivePlay**, **Go Button (iPad/iPhone)**, **QLab 5** y **MultiPlay**. La dirección visual del nicho es consistente en todos:

| Elemento | Aparece en | Característica común |
|---|---|---|
| **Fondo oscuro** | Todos | Negro / gris muy oscuro. Teatro se opera a oscuras. |
| **Un GO enorme** | Todos | El botón más grande de la app, verde, arriba o abajo, dominando visualmente. |
| **STOP rojo adyacente** | Todos | Inmediatamente al lado del GO. E-STOP en MultiPlay, Parar Todo en ZasCue, red stop en Go Button. |
| **Tabla de cues con columnas** | Todos | #, nombre, archivo, tiempos. A veces con flechitas entre columnas (pre ▶ dur ▶ post). |
| **Color por cue** | LivePlay (fondo entero), Go Button (badge del número), ZasCue (puntito), QLab (status) | Cada pista tiene un color asociado. |
| **Cart / pads en grilla** | LivePlay, Go Button, Sound Show | Cuadrados grandes con número + nombre + transporte, en grilla de 2+ columnas, colores fuertes. |
| **Inspector con tabs** | ZasCue, LivePlay | Panel derecho con secciones (Número, Color, Archivo, Fade In/Out, Loop…). |
| **Transport band** | Todos | Una franja con controles + contador/medidor, arriba o abajo. |
| **Countdown grande + "qué sigue"** | Go Button, ZasCue | Siempre visible qué falta y qué viene. |

**Decisión:** clonamos esa dirección, no la reinventamos. Cualquiera que haya operado con un programa de teatro va a reconocer la app en 5 segundos. El "look" no es el problema: el problema es que los demás pesan 68 MB o son cerrados.

> **Aviso legal:** clonamos la **dirección gráfica** (paleta, layout, convenciones), no los **activos** (logos, fuentes, capturas, marcas). La dirección general no es protegible; copiar el logo de QLab sí lo sería.

## 2. Paleta (calibrada contra las capturas)

Tokens centrales, inspirados en ZasCue / LivePlay / Go Button, con contraste suficiente para teatro a oscuras:

```
Fondos
  bg-app          #1F2228      /* fondo general de la ventana */
  bg-panel        #15181D      /* paneles (inspector, transport) */
  bg-row          #252A33      /* fila de cue en reposo */
  bg-row-alt      #2A2F39      /* fila alterna */
  bg-row-active   #323847      /* fila seleccionada o sonando */
  bg-pad          #2C313B      /* fondo de un pad del cart */

Texto
  fg-strong       #ECEEF2      /* títulos, números grandes */
  fg-base         #C7CBD3      /* texto normal */
  fg-mute         #8A8F99      /* subtítulos, ejes */

Acentos de estado
  go-green        #3DDC84      /* botón GO, "sonando" (idéntico en todos) */
  stop-red        #FF4D4D      /* STOP, "audio faltante" */
  warn-amber      #FFB020      /* advertencia, fade en curso */
  duck-orange     #EF9F27      /* ducking activo */
  info-blue       #5B9BFF      /* selección sin conflicto */

Bordes / separadores
  sep-thin        #2C313B 0.5 px  /* entre filas y columnas */
  sep-strong      #444A57 1 px    /* bordes de paneles */

Color de cue (paleta de 8, asignada al crear la pista en orden)
  cue-1 red        #E04848
  cue-2 orange     #F08C2E
  cue-3 amber      #E8B339
  cue-4 green      #5DBE6F
  cue-5 teal       #38B5A6
  cue-6 blue       #4F8FE0
  cue-7 indigo     #7B6FE0
  cue-8 magenta    #C56FD0

Limitador / ceiling
  limit-red       #E04848      /* picos cerca del techo */
```

El cue color se usa para: badge del número, barra lateral de la fila (4 px), y como fondo del pad si la pista está en la franja inferior.

## 3. Tipografía

- **Familia:** Segoe UI Variable en Windows, SF Pro Text en macOS (egui las carga via `default_fonts`).
- **Tamaños:**
  - Modo Diseño — base 14 px, headers 15 px, números en filas 13 px.
  - Modo Función — base 18 px, **números grandes 48 px**, countdown **72 px**.
  - Botones GO/STOP — 32 px en Diseño, **64 px en Función**.
- **Mono:** Cascadia Mono / SF Mono para duraciones, timecodes y números con padding.

Sin sombras. Sin degradados. Las separaciones vienen del color y de las líneas finas.

## 4. Layout principal — Modo Diseño

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│  TeatroPlayer  ·  MiObra.tpshow  ·  ●guardado    [Salida: Auriculares ▾]  Master▮▮▮▮ │ ← top bar 56 px
├──────────────────────────────────────────────────────────┬─────────────────────────────┤
│  #    NOMBRE                              PRE  ▶ DUR ▶ POST  GO    │ INSPECTOR            │
│  ────────────────────────────────────────────────────────     │ ─────────              │
│  1●   INTRO                               ─   4.0  ─    ▶   │   ▸ Transición            │
│  2●   TRUENO                              ─   5.0  3.0  ▶   │   Desde    0 %          │
│ ▸3●   PREDANZA — VIENTO DEL ESTE          ─   2.0  2.0  ▶   │   Hasta  100 %          │
│   4●   PARAGUAS — BALLET I                ─   3.0  ─    ▶   │   Duración 4.0 s        │
│   5●   LLUVIA (∞)                         ─  10.0  ─    ▶   │   Curva    lineal       │
│  ────────────────────────────────────────────────────────     │   ╱ rampa dibujada      │
│                                                              │   ▸ Lo que suena        │
│                                                              │                        │
│  [+ Agregar audios…]   [↑][↓][⧉][🗑]   [ Imprimir ]          │   Pre-espera  0 ms    │
│                                                              │   Post-espera 0 ms    │
│                                                              │   Inicio       0 ms    │
│                                                              │                        │
│                                                              │   Volumen   0 ─●──────  │
│                                                              │   Tecla      [ F3 ▾ ]   │
│                                                              │   Nota       [_________]│
├──────────────────────────────────────────────────────────┴─────────────────────────────┤
│  [ ▶ ANTERIOR ]   [  ▶ SIGUIENTE (ESPACIO)  ]   [ SALIR ]   [  ■ STOP  ]   0:00:00.0 │ ← transport 72 px
└────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Top bar (56 px):** nombre del programa, archivo, indicador de guardado, selector de salida, volumen máster, toggle Diseño/Función.
- **Cue list (centro-izquierda):** filas de 48 px, color lateral de 4 px, número grande coloreado, columnas con la convención `PRE ▶ DURACIÓN ▶ POST` (la flecha es la línea de tiempo, robada de ZasCue).
- **Inspector (centro-derecha, 420 px):** **una sola columna con scroll, sin pestañas**. Las pestañas se probaron dos veces y las dos veces hubo que adivinar en cuál vivía el control que se buscaba; ahora cada bloque es una sección con icono y título (Transición, Lo que suena, Salida, Repetición, Volumen, Tecla y pads) y todo está a la vista. Los campos llevan label arriba y control grande abajo, y "Elegir audio…" va centrado, estilo ZasCue.
- **Dos listas en el panel central:** una pestaña por lista, "**Audios**" (los archivos sueltos) y "**Eventos**" (escenas montadas: un audio que sube, que baja, un intercambio, o un disparo único de efecto). Son dos cosas distintas — el audio es el material, el evento es lo que se lanza — y por eso no van mezcladas. Cada lista tiene su inspector.
- **En el editor de un evento, el lado que sale no se elige.** El crossfade y el fade out muestran una sección "Lo que está sonando" con un solo control — hasta qué volumen baja — y ninguna lista de audios: el que se va es el que suena en ese momento. Sólo el fade in, el crossfade y el disparo único piden elegir un audio, y lo piden en una sección aparte, "Audio que entra".
- **Transport (72 px abajo):** los 4 botones (Anterior, Siguiente grande, Salir chico, STOP rojo a la derecha). A la derecha del todo, reloj de tiempo absoluto de la función (mm:ss.cs).

> **Nota sobre el boceto.** El dibujo de arriba es la **dirección de diseño**, no una foto de la app: hay cosas que aún no existen (el botón `Imprimir`, las columnas `PRE ▶ DURACIÓN ▶ POST`, las esperas previas) y otras que salieron distintas al construirlas. Lo que **sí** refleja el estado real, y conviene no volver atrás:
>
> - el inspector es **una columna con secciones**, sin pestañas;
> - el panel central tiene **dos listas**, Audios y Eventos, con su pestaña;
> - el fade lleva **los dos extremos en porcentaje** (Desde / Hasta), no sólo 0 y 100.

## 5. Modo Función

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│  MiObra                                              Auriculares ✓  ·   [ SALIR ]  │
├────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                       │
│      ┌─[ 00:12.0  QUEDAN ]─────────────────────  ┌──[ SIGUIENTE:  3  PREDANZA ]─────┐  │
│      └────────────────────────────────────────  └─────────────────────────────────┘  │
│                                                                                       │
│   ┌─────────────────────────────────────────────────────────────────────────┐          │
│   │ ●1   INTRO                                          ●  4.0s / QUEDAN 0.0│          │
│   └─────────────────────────────────────────────────────────────────────────┘          │
│   ┌─────────────────────────────────────────────────────────────────────────┐          │
│   │ ▸2   TRUENO            crossfade · entra 5 / sale 3         [  GO 2  ] │          │
│   └─────────────────────────────────────────────────────────────────────────┘          │
│   ┌─────────────────────────────────────────────────────────────────────────┐          │
│   │  3   PREDANZA — VIENTO                entra 2 s · se encima [  GO 3  ] │          │
│   └─────────────────────────────────────────────────────────────────────────┘          │
│   ┌─────────────────────────────────────────────────────────────────────────┐          │
│   │  4   PARAGUAS                          de golpe · corta     [  GO 4  ] │          │
│   └─────────────────────────────────────────────────────────────────────────┘          │
│                                                                                       │
├────────────────────────────────────────────────────────────────────────────────────────┤
│   [ ◀ ANTERIOR ]      [  ▶ SIGUIENTE (ESPACIO)  ]      [ SALIR ]    [  ■ STOP  ]    │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

- Fila activa: borde izquierdo del color del cue + fondo `bg-row-active`.
- "QUEDAN" se calcula con `TrackPosition::get_pos()` del cue activo; cuenta atrás visible desde el otro extremo de la sala.
- Botón GO por fila, sólo en las que NO están sonando (la activa muestra "●" en lugar de GO). Robado de Go Button.
- "SIGUIENTE: N nombre" arriba a la derecha. Robado de ZasCue.

## 6. Cart (pads de efectos sueltos)

Grilla de pads en la parte inferior de la pantalla, visible siempre que haya al menos un cue con `pad: true`. Estilo LivePlay Cart Player + Go Button "Hits".

```
┌── EFECTOS ──────────────────────────────────────────────┐
│ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ │
│ │ 1● │ │ 2● │ │ 3● │ │ 4● │ │ 5● │ │ 6● │ │ 7● │ │ 8● │ │
│ │AMBI│ │TRU-│ │PRE-│ │PARA│ │LLUV│ │    │ │    │ │    │ │
│ │ENTE│ │ENO │ │DANZ│ │GUAS│ │  ∞ │ │    │ │    │ │    │ │
│ └────┘ └────┘ └────┘ └────┘ └────┘ └────┘ └────┘ └────┘ │
└─────────────────────────────────────────────────────────┘
```

- Cada pad: 96 × 96 px (Diseño), 140 × 140 px (Función).
- Fondo del pad = color del cue, oscurecido un 30 %.
- Número arriba a la izquierda en blanco, 18 px medium.
- Nombre centrado, 13 px en Diseño / 16 px en Función.
- Borde de 1 px `sep-strong`; el pad activo (sonando) lleva un halo verde de 2 px.
- Loop (∞) como en Go Button.
- Click = dispara el cue con su `onPrevious.kind` configurado (que puede ser `keep`, `fadeOut` o `stop`). **No** se quita de la lista principal: el pad es solo un atajo grande.

## 7. Estados de una fila

Mismo patrón de LivePlay/Go Button: color + palabra + icono.

| Estado | Borde | Fondo | Etiqueta | Icono |
|---|---|---|---|---|
| Lista | sep-thin | bg-row | — | ▶ |
| Seleccionada (Diseño) | info-blue | bg-row-active | `▸` | ▶ |
| Entrando (fade in) | warn-amber | bg-row-active | `ENTRANDO` + barra de progreso | ▲ |
| Sonando | go-green | bg-row-active | `SONANDO` + tiempo restante | ● |
| Saliendo (fade out) | warn-amber | bg-row | `SALIENDO` | ▼ |
| Duckeada | duck-orange | bg-row-active | `BAJADA` | ⌄ |
| Audio faltante | stop-red | bg-row | `FALTA EL AUDIO · Buscar…` | ! |

Cada estado lleva **palabra + color** (no depende solo del color).

## 8. Textos (sin jerga)

| Concepto | Cómo se dice |
|---|---|
| Fade in | "Entra en ___ s" / "Sube con fade" |
| Fade out | "Sale en ___ s" / "Baja con fade" |
| Crossfade | "Entra ___ s mientras lo anterior sale en ___ s" / "Crossfade: yo subo y la anterior baja" |
| Extremos del fade | "Desde ___ %" y "Hasta ___ %" — los dos son libres, no sólo 0 y 100 |
| Lado que sale de un evento | "Lo que está sonando" + "Baja hasta ___ %" — nunca un audio que elegir |
| Audio que entra | "Audio que entra" — el único que se elige |
| Ducking | "Bajar lo que suena a ___ %" |
| Disparo único | "De golpe (sin fade)" / "Hacerlo disparo único (efecto)" |
| Loop | "Repetir" / "Para siempre (loop)" |
| Duración de una escena | "Duración de la escena" + botones − / + en la fila del evento |
| Pre-wait | "Esperar ___ s antes" |
| Auto-follow | "Después, disparar la siguiente sola" |
| Output device | "Salida" |
| Master volume | "Volumen general" |
| Stop all | "Parar todo" |
| Limitador | (invisible, no se nombra) |

Prohibido en la UI: *gain, dB, buffer, sample rate, codec, sink, mixer, pre-wait, post-wait, target, attenuation*.

## 9. Accesibilidad y entorno real

- 1366 × 768 soportado, escala al 100 % / 125 % / 150 %.
- Contraste mínimo 7:1 en texto (verificado contra `bg-app`).
- Sin dependencia del color: palabra + icono + color en cada estado.
- Modo Función pensado para leerse a 4 m de distancia (tipografía 18–72 px).
- Atajos de teclado completos: cualquier acción tiene una tecla.

## 10. Lo que NO hacemos (consistente con `10`)

- No copiamos el logo de QLab / LivePlay / ZasCue / ninguno.
- No usamos "QLab" ni "Go Button" como nombre de funciones en la UI.
- El parecido general del look es inevitable y querido (es la convención del nicho); el parecido específico de cada producto es evitado.