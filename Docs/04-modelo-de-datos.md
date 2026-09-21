# 04 — Modelo de datos y vinculación a archivos de audio

## 1. La sesión: un único archivo `.tpshow`

Una sesión completa es **un único archivo `MiObra.tpshow`** que adentro contiene el JSON de configuración y todas las pistas de audio. Esto resuelve de raíz el "guardado vinculante": el archivo y los audios son el mismo archivo. Se manda por WhatsApp, se sube a Drive, se copia en un pendrive — sin carpetas que se rompan, sin audios que queden sueltos.

Detalle completo del formato (estructura del ZIP, determinismo, lectura sin extraer, comandos CLI) en **`13-formato-de-archivo-unico.md`**. Resumen:

```
MiObra.tpshow    (ZIP, método STORE, determinista)
├─ manifest.json
├─ sesion.json   ← este documento describe su esquema
└─ audio/        ← todos los audios, copiados
   ├─ 01_ambiente.wav
   ├─ 02_trueno.mp3
   └─ 03_musica.flac
```

**Modo carpeta (legado):** para casos raros (edición de audios en estudio, integración con herramientas externas) se mantiene la posibilidad de abrir una carpeta suelta con `sesion.json` + `audio/`. Al guardar, si la sesión se abrió desde `.tpshow` se guarda como `.tpshow`; si se abrió desde carpeta, se guarda en carpeta. El instalador y los tutoriales enseñan `.tpshow` exclusivamente.

## 2. `sesion.json`

Versión de formato: `1`.

```jsonc
{
  "format": "teatroplayer-session",
  "version": 1,
  "id": "8f1c…",
  "name": "MiObra",
  "createdAt": "2026-09-20T15:00:00-05:00",
  "updatedAt": "2026-09-20T16:10:00-05:00",

  "output": {
    "deviceName": "Auriculares (Realtek Audio)",  // dispositivo preferido
    "deviceHint": "headphones",                    // fallback si cambia el nombre
    "masterVolumeDb": 0.0,
    "emergencyFadeMs": 50
  },

  "cues": [
    {
      "id": "c-01",
      "number": "1",              // lo que ve el operador
      "name": "Ambiente plaza",
      "color": "neutral",         // neutral | red | amber | green | blue
      "notes": "Sube despacio, dejar sonar",

      "audio": {
        "fileName": "01_ambiente.wav",
        "relPath": "audio/01_ambiente.wav",
        "absPath": "C:\\…\\MiObra\\audio\\01_ambiente.wav",  // solo referencia, puede faltar
        "sizeBytes": 44123456,
        "hash": "b3:9f2c8e…",     // blake3, primeros 16 bytes
        "durationMs": 92000,
        "channels": 2,
        "sampleRate": 44100
      },

      "volumeDb": -3.0,

      "entrance": {
        "kind": "fadeIn",         // hit | fadeIn
        "durationMs": 4000,
        "curve": "equalPower"     // linear | exponential | equalPower
      },

      "onPrevious": {
        "kind": "fadeOut",        // keep | fadeOut | stop | duck
        "durationMs": 3000,
        "curve": "equalPower",
        "duckLevelPercent": 30    // solo si kind = "duck": baja a ese % y se queda
      },

      "exit": {
        "kind": "fadeOut",        // hit | fadeOut | untilEnd
        "durationMs": 2000,
        "curve": "equalPower"
      },

      "loop": { "mode": "infinite" },   // none | infinite | count
      // "loop": { "mode": "count", "count": 3 }

      "trigger": { "key": "F1" },       // opcional
      "startAtMs": 0,                   // punto de inicio dentro del audio

      "preDelayMs": 0,                  // esperar antes de entrar (B5)
      "autoFollow": {                   // disparar la siguiente sola (B6)
        "kind": "none"                  // none | afterMs | whenThisEnds
      },
      "pad": false                      // true = también aparece en la franja de efectos (B3)
    }
  ]
}
```

### Notas del esquema

- `entrance` + `onPrevious` son **independientes**. Eso es lo que permite todas las combinaciones que pide el usuario:
  - `hit` + `keep` → entra de golpe y se encima (dos ambientes a la vez).
  - `fadeIn` + `keep` → **"A entra con fade in y B entra de golpe"** (B es otra entrada con `hit`).
  - `fadeIn` + `fadeOut` → **crossfade clásico**.
  - `hit` + `stop` → cambio seco de una pista por otra (corte de escena).
- `exit` es la salida cuando el operador aprieta SALIR sobre esa pista (o cuando se dispara la siguiente y esa pista no fue afectada por `onPrevious`).
- `loop.mode = infinite` + `entrance.fadeIn` = ambiente que aparece de a poco y se queda.
- `pad: true` hace que la entrada aparezca también en la franja inferior de **Efectos** (siempre lista, se dispara fuera de la secuencia). No la saca de la lista.
- `autoFollow` con `whenThisEnds` solo es válido si la entrada **no** tiene loop infinito (si no, nunca termina y nunca dispara la siguiente).
- Todo tiempo se guarda en **ms enteros** (evita ruido de punto flotante al re-guardar).

## 3. Vinculación a archivos de audio (el "guardado vinculante")

Al abrir una sesión, por cada audio se corre este algoritmo de resolución, en orden, y se detiene en el primero que funcione:

```
1. relPath dentro de la carpeta de la sesión      →  estado OK
2. absPath tal cual                                →  estado OK (usuario no copió)
3. Buscar en audio/ por fileName                   →  estado OK (se movió de nombre de carpeta)
4. Buscar en audio/ por fileName + sizeBytes       →  estado OK
5. Buscar en audio/ por hash (blake3)              →  estado OK (se renombró el archivo)
6. Buscar en la carpeta de la sesión (recursivo)   →  estado MOVIDO (se reescribe relPath)
7. Nada                                            →  estado FALTANTE
```

| Estado | Qué ve el usuario | Qué puede hacer |
|---|---|---|
| `OK` | Fila normal | — |
| `MOVIDO` | Fila normal + aviso discreto "reubicado" | Se reescribe `relPath` al guardar |
| `FALTANTE` | Fila en rojo con el botón **"Buscar audio…"** | Diálogo nativo; al elegir archivo se valida por `sizeBytes`+`hash` y se reescribe la ruta (y se ofrece copiarlo a `audio/`) |
| `REEMPLAZADO` | (futuro) coincide nombre pero no hash | Avisar antes de aceptar |

Reglas adicionales:

- **El hash nunca bloquea la reproducción.** Si el archivo existe por ruta pero el hash no coincide, suena igual y se avisa. En teatro, que suene es más importante que que coincida.
- La **verificación completa** corre al abrir la sesión y bajo demanda (botón "Verificar audios"). Con 30 pistas de 50 MB, blake3 tarda ~1 s; se hace en background y solo de los archivos marcados como faltantes o dudosos después de la comprobación rápida por nombre/tamaño.
- **Sesión nunca se abre rota**: si faltan audios, la lista se muestra igual, con las filas afectadas en rojo y el GO de esas entradas deshabilitado con explicación. El operador ve el problema **antes** de la función, no durante.

## 4. Guardado

| Mecanismo | Cuándo | Dónde |
|---|---|---|
| **Auto-guardado** | 1.5 s después del último cambio (debounce), y al cerrar | `sesion.json` |
| **Backup rotativo** | Antes de cada escritura | `.backups/sesion.<timestamp>.json` (se conservan 5) |
| **Guardar como** | Explícito | Copia la carpeta completa (opcionalmente sin `audio/` si no se copió) |
| **Empaquetar (.tpk)** | Explícito (P2) | ZIP de la carpeta |

El archivo se escribe de forma **atómica**: se escribe `sesion.json.tmp`, se hace flush y se renombra. Una laptop que se apaga a mitad de función no pierde la sesión.

## 5. Estado de la aplicación (no de la sesión)

Fuera de la carpeta de la sesión, en la carpeta de configuración del usuario (`directories::ProjectDirs`):

```
%APPDATA%/TeatroPlayer/
├─ state.json        # última sesión abierta, dispositivo elegido, modo, tamaño de ventana
└─ logs/teatroplayer.log
```

`state.json` guarda la ruta de la última sesión para que al abrir el programa aparezca **la obra de hoy** ya cargada, lista para apretar GO.

## 6. Migración de formato

- `version` en el JSON. `migrate.rs` contiene conversores `v1 → v2 …`.
- Si el archivo es de una versión **posterior** a la que el programa entiende, se abre en **solo lectura** con aviso claro ("Esta sesión se hizo con una versión más nueva"). Nunca se pierde el trabajo del diseñador.
