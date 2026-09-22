# 01 — Producto

## 1. El problema

En teatro independiente el diseño sonoro lo arma una persona y lo ejecuta otra (o la misma, apurada, durante la función). Hoy eso se resuelve con:

- Un reproductor genérico (Windows Media Player / VLC / Spotify): **no tiene fades ni crossfades controlables**, y menos "en el momento".
- Un DAW (Audacity, Reaper, Ableton): hace todo, pero **es pesado, complejo, y da miedo**. Nadie quiere que un asistente toque un proyecto de Reaper a mitad de función.
- Software de cue comercial (QLab, Show Cue System): es la solución correcta, pero **es de pago, en inglés, y pesado** para alguien que "no tiene mucho".

Lo que falta es el mínimo: **una lista de audios con entradas y salidas configuradas, guardable, que se abre y se aprieta un botón.**

## 2. Usuarios

### Persona A — "Diseñador" (arma)
- Sabe de audio. Entiende qué es un fade, un crossfade, un dB.
- Prepara la sesión en su casa / ensayo.
- Quiere: cargar carpeta, configurar entradas rápido, guardar, y que quede a prueba de errores.
- Tolerancia a la complejidad: **media** (acepta un panel de edición).

### Persona B — "Operador" (ejecuta)
- Es asistente, actor, o el mismo diseñador en modo supervivencia. **No sabe de audio.**
- Está a oscuras, con la laptop en la falda o en una mesa, con cable de audífonos a la consola.
- Tiene **una oportunidad** por función: si se equivoca, se nota.
- Quiere: ver la lista, apretar "lo que sigue", y un botón de pánico.
- Tolerancia a la complejidad: **cero**.

> **Consecuencia de diseño:** el producto tiene dos caras. El Diseñador configura; el Operador solo ve **nombre + botón GO**.

## 3. Propuesta de valor

| Para | Valor |
|---|---|
| Diseñador | Armar el diseño sonoro de una obra en minutos y **dejarlo guardado y transportable**. |
| Operador | Ejecutar una sesión sin saber nada de audio: abrir, ver la lista, apretar botones. |
| Producción | Costo cero, sin internet, sin licencias, sin instalar runtimes, corre en cualquier laptop. |

## 4. Principios de diseño (ordenados por prioridad)

1. **A prueba de tontos, no tonto.** La interfaz del operador no puede permitir el error común: botones grandes, una acción por botón, confirmación solo en lo destructivo.
2. **Ultraligero.** Arranca en menos de un segundo, no come batería, no necesita internet, no instala runtimes. Si pesa, no se usa en la laptop prestada.
3. **Ultra simple visualmente.** Una lista y controles obvios. Nada de menús anidados, nada de modos escondidos. Si una función no se ve, no existe.
4. **Sesión antes que archivo.** El usuario no guarda "un audio", guarda **la obra completa**: audios + cómo entran + cómo salen.
5. **Nada se lee de internet, nada se escribe fuera de la carpeta de la sesión.** Previsible y portable.
6. **No reinventar la rueda.** Cada función se apoya en una librería probada. Código propio solo donde no hay alternativa.

## 5. Alcance del MVP (v0.1)

**Sí incluye**

- [ ] Abrir una carpeta de audios y listarlos (WAV, MP3, FLAC, OGG, M4A).
- [ ] Playlist ordenable y renombrable; cada fila es una **entrada**.
- [ ] Reproducir una entrada con clic o con atajo de teclado.
- [ ] **Fade in** por entrada: 0.1 s – 60 s, con curva (lineal / exponencial / equal-power).
- [ ] **Fade out** manual por pista (botón SALIR) y automático al terminar.
- [ ] **Crossfade**: al entrar B, sacar A con fade, con tiempos independientes.
- [ ] **Combinaciones**: entrar de golpe, entrar con fade, entrar pisando (sin sacar nada).
- [ ] **Loop**: sin loop / infinito / N veces.
- [ ] Volumen por entrada (dB) y volumen máster.
- [ ] **STOP de emergencia** (corta todo con fade corto de 50 ms, configurable).
- [ ] **Salida de audífonos**: selector de dispositivo, recordado entre usos.
- [ ] **Sesiones**: nueva / abrir / guardar / guardar como, en carpeta autocontenida.
- [ ] **Vinculación a archivos**: verificación al abrir y relocalización guiada si un audio se movió.
- [ ] **Modo Función**: pantalla simplificada, botones gigantes, siguiente entrada con ESPACIO.
- [ ] Instalador Windows por-usuario + versión portable (ZIP).

**No incluye (v0.1) — candidatos para después**

- Edición/trim de audio, grabación, efectos (reverb, EQ).
- Múltiples salidas simultáneas / salida multicanal a interfaz USB.
- Disparo por MIDI, OSC, o por tiempo (reloj de show).
- Nubes, sincronización, colaboración.
- Pistas de video, subtítulos, luces.
- macOS / Linux (quedan posibles, no construidos).

### Qué añade la 0.2

- **Eventos predefinidos**: escenas montadas con un audio, guardadas con la
  obra y reutilizables. Cuatro tipos: fade in, crossfade, **fade out** (que baja
  lo que esté sonando, sin elegir audio) y **disparo único** (efecto de golpe,
  sin fade). Sólo los tres de entrada o intercambio piden elegir un audio. Ver
  FR-17.
- **Segunda lista** en el panel central: *Audios* y *Eventos*, cada una con su
  editor.
- **Extremos del fade en porcentaje**: la rampa puede ir de cualquier valor a
  cualquier otro, no sólo de 0 a 100.
- **Panel derecho rediseñado**: una columna con secciones en vez de pestañas,
  con la rampa dibujada tal como se va a oír.
- **Formato de sesión v2**. Las sesiones de la v1 se abren y se migran solas;
  una build vieja abre una v2 en sólo lectura en vez de borrar los eventos.

## 6. Posicionamiento competitivo

Investigación completa en `10-competencia-y-benchmark.md`. Resumen:

**El hueco.** En Mac el estándar es QLab. En Windows lo que hay es: software pago (Show Cue System desde US$ 62 hasta US$ 268; SFX, US$ 395 de lista), software enorme y con servidor/red (LivePlay, que además es AGPL), software cerrado de 68 MB (ZasCue), un soundboard y no una lista de cues (Sound Show), o proyectos open source muy crudos. Lo que **no** hay en Windows es algo que pese nada, sea libre y se pueda operar sin saber de sonido.

**Dónde nos paramos.** No competimos en funciones. Competimos en que lo pueda usar cualquiera:

| | QLab | SCS | LivePlay | ZasCue | Sound Show | **TeatroPlayer** |
|---|---|---|---|---|---|---|
| Windows | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Libre / open source | ✗ | ✗ | ✓ (AGPL) | ✗ | ✗ | ✓ (MIT/Apache) |
| Peso | grande | medio | medio–grande | ~68 MB | medio | **~10 MB** |
| Sin instalar | ✗ | ✗ | ✗ | ✓ | ✗ | ✓ |
| Español nativo | ✗ | ✗ | ✗ | ✓ | ✗ | ✓ |
| Curva para no-técnicos | media | media | alta | baja | baja | **mínima** |

**La promesa en una frase:** si sabés arrastrar un archivo y apretar la barra espaciadora, podés pasar el sonido de una obra.

**Lo que tomamos prestado (y de quién):** curva S y escala de volumen tipo consola (QLab); pads siempre listos y ducking (Go Button, LivePlay); auto-continue y pantalla de operador con cuenta atrás (ZasCue); precarga y pre-delay (QPlayer); imprimir la hoja de entradas (MultiPlay); auto-recuperación del motor (LivePlay); "lo que no usás, no se ve" (ZasCue).

**Lo que rechazamos:** MIDI, OSC, video, efectos por pista, multicanal, red, edición de audio. Lista completa en `10-competencia-y-benchmark.md` §5. Si alguien necesita eso, la respuesta honesta va a ser "usá LivePlay o QLab", y lo vamos a escribir en el README.

## 7. Métricas de éxito del MVP

| Métrica | Objetivo |
|---|---|
| Tiempo desde "abrir el programa" hasta "suena el primer audio con fade" | < 60 s |
| Tamaño del instalador | < 8 MB |
| RAM en reposo con 20 pistas cargadas | < 150 MB. **Presupuesto corregido el 2026-09-22:** el original de < 60 MB no es alcanzable con esta pila y se decidió subirlo en vez de cambiar de toolkit. Medido ~140 MB estables, y una ventana egui **vacía** ya son 139,9 MB, así que el motor, el paquete, la sesión y la interfaz de la app caben dentro del ruido de medida: el coste es de eframe + glow/OpenGL, no del código propio. El motor sin interfaz (`tp-spike`) usa 15 MB. Ver riesgo R16 en `09` |
| Arranque en frío | < 1 s |
| Una persona sin conocimiento de audio ejecuta una sesión de 10 entradas sin ayuda | Sí, en el primer intento |
| Caídas de audio (glitches/underruns) en una función de 90 min | 0 |
