# 07 — Empaquetado, instalación y "sin ventana de consola"

## 1. Requisito: consola solo durante el desarrollo

En Windows, un binario compilado como *subsystem console* abre una ventana negra de CMD al lado de la aplicación. En el producto final **no debe existir**.

### Implementación

En `src/main.rs`, primera línea:

```rust
// Consola visible SOLO en builds de desarrollo.
// En release (--release) el ejecutable es de tipo "windows" y no abre CMD.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() { /* … */ }
```

- `cargo run` / `cargo build` (debug) → `debug_assertions` activo → **subsystem console** → se ven los `println!` y los `panic!`.
- `cargo build --release` → `debug_assertions` desactivado → **subsystem windows** → **cero ventanas de consola**.

### Consecuencias a tener en cuenta

| Qué | En debug | En release |
|---|---|---|
| `println!` / `eprintln!` | visible en consola | **descartado** (no hay consola) |
| `panic!` | mensaje en consola | termina sin avisar |
| Logs | consola | **archivo** `%APPDATA%/TeatroPlayer/logs/teatroplayer.log` |

Por eso el arranque instala un suscriptor de `tracing` **dual**:
- debug → salida a consola con colores.
- release → `tracing_appender::rolling` a archivo, nivel `INFO`, rotación diaria, 3 archivos.

Los errores graves se muestran además en la UI (banner o diálogo), nunca solo en el log.

### Cómo depurar un release si hiciera falta

- Opción A: compilar con `RUSTFLAGS="--cfg windows_subsystem=\"console\""` no aplica; la forma correcta es temporalmente `cargo build --release` con la feature `dev-console` que fuerza `windows_subsystem = "console"`. Se añade una feature explicitamente para esto:

```toml
[features]
dev-console = []
```
```rust
#![cfg_attr(all(not(debug_assertions), not(feature = "dev-console")), windows_subsystem = "windows")]
```

- Opción B (la que se usará casi siempre): leer el log y agregar un panel de diagnóstico oculto (Ctrl+Shift+D) que muestra el estado del motor y las últimas 50 líneas del log dentro de la propia ventana.

## 2. Perfil de compilación

```toml
[profile.release]
opt-level     = "z"     # optimizar tamaño
lto           = true    # link-time optimization
codegen-units = 1
panic         = "abort"
strip         = true    # quitar símbolos

[profile.release.package."*"]
opt-level = "z"
```

Verificación objetiva tras cada build de release:
- `teatroplayer.exe` ≤ 12 MB.
- Dependencias con `cargo tree --depth 1` revisadas (evitar que se cuele algo pesado).
- `cargo bloat --release --crates` para detectar quién engorda el binario.

## 3. Presupuesto de tamaño

| Componente | Estimado |
|---|---|
| `teatroplayer.exe` (Rust + rodio + symphonia + egui/glow) | 8–12 MB |
| Instalador NSIS (LZMA sólido) | 3–6 MB |
| Instalado en disco | ~12 MB + sesiones del usuario |
| RAM en reposo | 35–55 MB |
| RAM con 8 pistas sonando | 55–80 MB |

## 4. Instalador (Windows)

**Herramienta:** `cargo-packager` (crabnebula) generando **NSIS**. Alternativa equivalente y muy probada: **Inno Setup** con un script de ~40 líneas. Se elige NSIS vía cargo-packager para que el empaquetado salga del mismo `cargo` y se pueda automatizar en CI; si NSIS da problemas, Inno Setup es el plan B sin costo de diseño.

**Configuración del instalador**

- Instalación **por usuario** (`%LOCALAPPDATA%\Programs\TeatroPlayer`), **sin solicitar privilegios de administrador**.
- Accesos directos: **Escritorio** (sí, es el requisito: "que se abra en el escritorio") + Menú Inicio.
- Asociación de archivos: `.tpshow` (el `sesion.json` renombrado) y `.tpk` → abrir con TeatroPlayer. Permite **doble clic en la obra** y que arranque lista para usar.
- Desinstalador incluido; **no borra las sesiones del usuario** (se avisa explícitamente).
- Sin servicios, sin tareas programadas, sinarranque automático con Windows.
- Idioma del instalador: español.

**Firma y SmartScreen**

Un ejecutable sin firmar dispara el aviso de Windows Defender SmartScreen ("aplicación no reconocida"). Para un proyecto de teatro independiente:
- Corto plazo: documentar el aviso y explicar "Más información → Ejecutar de todas formas" en el README que se entrega con la obra.
- Medio plazo: firmar con un certificado de código (OV) si hay presupuesto; con `signtool` en CI.

**Versión portable**

Además del instalador, se publica `TeatroPlayer-portable.zip`: el `.exe` suelto que corre sin instalar (arrastra la configuración en `%APPDATA%`, o junto al `.exe` si hay un `portable.txt`). Clave para cuando la laptop es prestada o no se permite instalar.

## 5. Arranque por doble clic (el caso de uso real del operador)

Al abrir `MiObra.tpshow`:

```
1. Carga %APPDATA%/TeatroPlayer/state.json
2. Si la sesión venía por argumento → la abre
   Si no → reabre la última sesión usada
3. Verifica audios en background
4. Abre la ventana en el modo donde se dejó (si estaba en Función, entra en Función)
5. Listo para GO
```

Tiempo total objetivo: **< 1 s**.

## 6. Estructura del repositorio (referencia)

```
TeatroPlayer/
├─ Cargo.toml
├─ Cargo.lock          # fijar versiones exactas (rodio se mueve rápido)
├─ Docs/               # este diseño
├─ assets/
│  ├─ icon.ico         # icono de Windows (multi-resolución)
│  └─ fonts/           # opcional: Inter embebida
├─ packaging/
│  ├─ nsis/installer.nsi (si se usa Inno: setup.iss)
│  └─ windows/app.manifest
└─ src/ …
```

Para el icono en Windows sin dependencias: `winres` (o `embed-resource`) en `build.rs`, que además incrusta el manifiesto (DPI aware, nombre y versión en el Explorador).

## 7. CI mínimo (cuando exista)

- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`.
- Build release en `windows-latest`.
- Artefactos: `.exe` portable + instalador `.exe`.
- Verificación automática de tamaño (falla si el exe supera 15 MB).
