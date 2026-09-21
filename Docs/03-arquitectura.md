# 03 — Arquitectura

## 1. Vista de capas

```
┌──────────────────────────────────────────────────────────────┐
│  UI (egui / eframe)                                          │
│  ├─ Modo Diseño : playlist editable + panel de entrada        │
│  └─ Modo Función: lista gigante + GO / SALIR / STOP          │
│  Emite comandos: GoCue(id), FadeOut(id), StopAll, SetMaster… │
└───────────────┬──────────────────────────────────────────────┘
                │  canal MPSC  (UI → Engine)  +  Arc<TrackControls> compartido
┌───────────────▼──────────────────────────────────────────────┐
│  Engine  (hilo de control, sin audio)                        │
│  ├─ Session      : entradas, orden, estado                   │
│  ├─ CueRunner    : interpreta una entrada → plan de audio    │
│  ├─ Crossfader   : coordina "sale A / entra B"               │
│  └─ AudioBackend : trait (implementación rodio)              │
└───────────────┬──────────────────────────────────────────────┘
                │
┌───────────────▼──────────────────────────────────────────────┐
│  Backend rodio (hilo de audio del SO, propiedad de cpal)     │
│                                                              │
│   Decoder(Symphonia) → EnvelopeSource → ┐                    │
│   Decoder(Symphonia) → EnvelopeSource → ├→ Mixer → WASAPI    │
│   LoopedDecoder      → EnvelopeSource → ┘      (audífonos)   │
└──────────────────────────────────────────────────────────────┘
```

**Regla de oro:** el hilo de audio **nunca** hace I/O de disco, ni allocations, ni locks. Solo mezcla y aplica ganancia. Todo lo que decodifica va pre-buffereado (`.buffered()`) en un hilo propio.

## 2. Módulos

```
src/
├─ main.rs              # arranque, sin consola en release
├─ app.rs               # bucle egui, estado de la UI
├─ ui/
│  ├─ design_view.rs    # modo Diseño
│  ├─ show_view.rs      # modo Función
│  ├─ cue_row.rs        # fila de la playlist
│  ├─ cue_editor.rs     # panel de configuración de una entrada
│  └─ theme.rs          # paleta, tipografías, tamaños
├─ session/
│  ├─ model.rs          # Session, Cue, EntranceKind, Curve…
│  ├─ store.rs          # cargar/guardar/auto-guardado/backups
│  ├─ links.rs          # vinculación y relocalización de audios
│  └─ migrate.rs        # versionado del formato
├─ engine/
│  ├─ backend.rs        # trait AudioBackend
│  ├─ rodio_backend.rs  # implementación con rodio
│  ├─ track.rs          # TrackHandle + EnvelopeSource + TrackControls
│  ├─ runner.rs         # ejecuta una entrada según su configuración
│  └─ commands.rs       # enum de comandos UI → Engine
└─ platform/
   ├─ paths.rs          # carpetas de config / sesiones (directories)
   └─ logging.rs        # tracing a archivo
```

## 3. Hilos

| Hilo | Quién lo crea | Responsabilidad | Frecuencia |
|---|---|---|---|
| UI | eframe/winit | Dibujar, capturar teclado/mouse | 60 fps |
| Engine | nosotros | Recibir comandos, armar y despachar pistas | por evento |
| Audio (callback) | cpal / WASAPI | Mezclar y escribir al dispositivo | cada ~5–11 ms |
| Decodificadores | rodio (`.buffered()`) | Leer y decodificar cada pista por adelantado | background |

Comunicación UI → Engine: `std::sync::mpsc` (comandos discretos).
Comunicación UI → audio: **`Arc<TrackControls>` con atómicos** (nunca locks en el callback).
Comunicación audio → UI: atómicos de solo lectura (`frames_played`, `finished`).

## 4. Estados de una pista

```
        ┌─────────┐
        │  Idle   │
        └────┬────┘
      GoCue  │
             ▼
     ┌───────────────┐   fade_in = 0     ┌──────────────┐
     │   FadingIn    │ ────────────────► │   Playing    │
     │ (0.1s – 60s)  │                   │              │
     └───────────────┘                   └──┬────────┬──┘
                                  FadeOut()  │        │ Loop
                                            ▼        ▼  (vuelve a Playing)
                                   ┌──────────────┐
                                   │  FadingOut   │
                                   └──────┬───────┘
                                          ▼
                                   ┌──────────────┐
                                   │   Stopped    │  (recurso liberado)
                                   └──────────────┘
```

Transiciones válidas desde cualquier estado: `StopAll()` → `FadingOut(50 ms)` → `Stopped`.

## 5. Flujo de una entrada (cuando se aprieta GO)

```
1. UI: GoCue(id)
        │
2. Engine: busca el Cue en la Session
   ├─ resuelve el archivo de audio (links.rs) → si FALTANTE, avisa y NO reproduce
   └─ abre Decoder (o Decoder::new_looped si tiene loop)
        │
3. Engine construye la cadena:
        decoder
          .convert_samples()          # unifica a la tasa del dispositivo
          .amplify(track_volume_db)
          .buffered()                 # decodifica en background
        → envuelto en EnvelopeSource(controls)
        → si fade_in > 0: EnvelopeSource rampa 0 → 1 en T segundos
        │
4. Engine: ¿qué hacemos con lo que ya está sonando?
   ├─ keep         → nada, se enciman
   ├─ fadeOutPrev  → Crossfader aplica rampa 1 → 0 a la pista activa (duración propia)
   └─ stopPrev     → corte seco de la pista activa
        │
5. Engine: mixer.add(cadena)   ← rodio se queda con la pista
   Engine guarda Arc<TrackControls> en la tabla de pistas activas
        │
6. UI: la fila pasa a "Sonando" con barra de progreso leyendo frames_played
```

## 6. Crossfade: la secuencia exacta

Caso: **A está sonando; se dispara B con crossfade de 5 s (B entra en 5 s, A sale en 3 s).**

```
t=0.0s   B: gain 0 → rampa ascendente 5s (curva equal-power)
         A: gain 1 → rampa descendente 3s (curva equal-power)
t=0.0s…3.0s   ambos suenan mezclados  → transición audible
t=3.0s   A: gain 0 → se marca finished → se libera el decoder
t=5.0s   B: gain 1 → Playing
```

Señal resultante (equal-power, suma de potencia constante):

```
A  ███████▄▄▄▄▄▄▂▂▂▂▁▁▁··················
B  ········▁▁▁▂▂▂▄▄▄▄▄███████████████████
   └──────── 3s ────────┘└──── 5s ─────┘
```

**Curvas disponibles** (se eligen en la entrada, no globalmente):

| Curva | Fórmula | Cuándo usar |
|---|---|---|
| Lineal | `g = t/T` | Fades cortos (< 1 s), efectos evidentes |
| Exponencial | `g = (2^(t/T) − 1)/1` aprox. | Entradas que deben "aparecer de la nada" |
| **Equal-power** (S) | `g_in = sin(πt/2T)`, `g_out = cos(πt/2T)` | **Predeterminada para crossfades** — evita el bache de volumen en la mitad |

Rodio da `fade_in` / `fade_out` (lineales) y `linear_gain_ramp`. Las curvas S y exponencial se implementan dentro de `EnvelopeSource` (~20 líneas de matemática), que es el único punto de DSP propio.

## 7. Loop

| Modo | Implementación | Nota |
|---|---|---|
| Sin loop | `Decoder` normal | termina y pasa a Stopped |
| Infinito | `Decoder::new_looped(file)` → `LoopedDecoder` | nativo de rodio, sin recargar el archivo |
| N veces | `decoder.repeat_infinite().take_duration(N * duración)` | requiere conocer la duración (se guarda en la sesión) |
| Infinito con fade por vuelta *(P2)* | `LoopedDecoder` + envolvente cíclica en `EnvelopeSource` | para ambientes que respiran |

**Advertencia conocida:** el MP3 codificado agrega padding silencioso al inicio/fin — los loops de MP3 pueden "saltar". Recomendación impresa en la UI: **usar WAV o FLAC para loops**. Si el archivo es MP3 y tiene loop, mostrar aviso suave.

## 8. Latencia y buffers

| Parámetro | Valor objetivo | Nota |
|---|---|---|
| Buffer de salida (WASAPI shared) | 256–512 frames | ~5–11 ms a 48 kHz |
| Pre-buffer de decodificación | 200 ms por pista | `.buffered()` con `buffered_capacity` generoso |
| Latencia percibida GO → sonido | < 30 ms | suficiente para teatro (la consola agrega más) |
| Actualización de UI de la ganancia | cada frame | el envolvente real corre por muestra en el hilo de audio |

El hilo de audio no bloquea: si el decodificador no alcanza, rodio reporta underrun y se registra en el log (nunca se silencia la función sin avisar).

## 9. AudioBackend: la costura de escape

```rust
pub trait AudioBackend {
    fn list_outputs(&self) -> Result<Vec<OutputDevice>>;
    fn open(&mut self, device: Option<&str>) -> Result<()>;   // None = default
    fn play(&mut self, plan: TrackPlan) -> Result<TrackHandle>;
    fn set_master_volume(&mut self, db: f32);
    fn stop_all(&mut self, fade: Duration);
}
```

Solo `rodio_backend.rs` conoce rodio. Si en el futuro rodio deja de servir (o se quiere JUCE), se cambia este archivo y nada más.

## 10. Errores: política

- **Nunca** se detiene la función por un error de una pista. Un audio faltante se muestra en rojo y el GO de esa entrada avisa, pero el resto sigue.
- Todo error se escribe en `teatroplayer.log` junto al ejecutable (release no tiene consola).
- Ante fallo del dispositivo de salida (se desconectó la interfaz): reintento automático al dispositivo por defecto y aviso en pantalla.
