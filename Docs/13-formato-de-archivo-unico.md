# 13 — Formato de archivo único `.tpshow`

Una obra de TeatroPlayer es **un único archivo**: `MiObra.tpshow`. Contiene la sesión completa y todas las pistas de audio adentro. Se manda por WhatsApp, se copia en un pendrive, se sube a Drive — sin carpetas que se rompan, sin audios que queden sueltos.

---

## 1. Estructura del archivo

`.tpshow` es un **ZIP** (PKWARE spec) con esta organización:

```
MiObra.tpshow
├── manifest.json          ← metadatos del paquete (versión, fecha, generador)
├── sesion.json            ← el modelo de sesión (ver `04-modelo-de-datos.md`)
├── audio/
│   ├── 01_ambiente.wav
│   ├── 02_trueno.mp3
│   └── ...
└── thumbnails/            ← opcional (P2): PNG chico por pista para la fila
    ├── 01_ambiente.png
    └── ...
```

Reglas:

- **Una sola pista = una sola entrada.** La asociación `sesion.cues[i].audio.fileName ↔ audio/<fileName>` se valida al abrir y se rechaza si falta alguna.
- Los nombres de archivo son UTF-8 NFC.
- No hay directorios anidados dentro de `audio/` (lista plana).

## 2. Por qué ZIP, por qué STORE

| Decisión | Razón |
|---|---|
| ZIP en lugar de TAR | Lectura aleatoria por entrada (seek por nombre) → reproducible y rápido para el primer sample |
| **Método STORE (sin compresión)** | (1) el audio ya está comprimido (MP3/OGG/FLAC) o es PCM (WAV) — comprimir de nuevo no ahorra nada; (2) STORE permite `File::seek` directo a la muestra → loops y `startAtMs` sin re-decodificar; (3) determinismo entre versiones del compresor: con DEFLATE distintos compresores dan bytes distintos; con STORE es 1:1 |
| CRC-32 verificado al abrir | Detecta corrupción en pendrive |
| ZIP64 si el total > 4 GB | Para obras largas con WAV sin comprimir (90 min estéreo 44.1k 16-bit ≈ 900 MB; 3h ≈ 3 GB; ok, pero ZIP64 si se pasa) |

## 3. Determinismo del paquete (bit-idéntico en cualquier máquina)

Para que dos builds produzcan el mismo archivo, el writer aplica:

| Parámetro | Valor fijo |
|---|---|
| Método de compresión | `STORE` (0) |
| Timestamp de cada entrada | `1980-01-01 00:00:00` (DOS time) |
| Atributos externos | ninguno |
| Permisos UNIX | 644 (rw-r--r--) |
| Comentarios por archivo | ninguno |
| Comentario del ZIP | ninguno |
| Orden de las entradas | ordenadas lexicográficamente por nombre UTF-8 NFC |
| Nombre del paquete | `MiObra.tpshow` (sin timestamp en el nombre) |
| Versión del ZIP | 2.0 |
| Bytes extra por entrada | solo `0x0000 0x0000 0x0000` (sin NTFS, sin UNIX) |

Resultado verificable:

```bash
teatroplayer pack ./ejemplos/MiObra/ /tmp/a.tpshow
teatroplayer pack ./ejemplos/MiObra/ /tmp/b.tpshow
sha256sum /tmp/a.tpshow /tmp/b.tpshow   # idem
```

## 4. Cómo se lee un `.tpshow` sin descomprimir

No se extrae a disco (mantiene la portabilidad del "un solo archivo"). `rodio::Decoder::new(reader)` necesita `Read + Seek`, así que implementamos `ZipEntryReader`:

```
struct ZipEntryReader {
    file:        File,                  // el .tpshow abierto
    data_offset: u64,                   // byte 0 de los datos de esta entrada
    size:        u64,                   // tamaño en bytes
    pos:         u64,                   // posición de lectura actual
}

impl Read for ZipEntryReader { /* seek interno a data_offset+pos, lee n bytes */ }
impl Seek for ZipEntryReader { /* seek absoluto o relativo, clamp a size */ }
```

- **STORE**: `seek(SeekFrom::Start(p))` se traduce a `file.seek(SeekFrom::Start(data_offset + p))` y `read` lee desde ahí. Coste: un syscall por seek, sin decodificación intermedia.
- **CRC**: al terminar la lectura se verifica con el CRC guardado en el header central (rodio consume todo el archivo del decoder, así que el check es barato).

Para abrir el archivo:

1. Abrir el `.tpshow`, leer el **final** para localizar el **End of Central Directory Record** (EOCD, últimos ~22 bytes + comentario de hasta 65535 bytes).
2. Con el offset del Central Directory, parsear las entradas hasta encontrar la buscada (búsqueda lineal; <100 entradas → trivial).
3. Para cada entrada guardar: `data_offset` (de la Local File Header), `size`, `crc32`.
4. Cerrar. Ya no se necesita el CD.

Si el archivo está en un ZIP64: usar la firma `0x0708` del ZIP64 End of Central Directory.

## 5. `manifest.json`

```json
{
  "format": "teatroplayer-package",
  "version": 1,
  "generator": {
    "name": "TeatroPlayer",
    "version": "0.1.0",
    "build": "abc1234"
  },
  "createdAt": "2026-09-20T21:00:00Z",
  "sessionId": "8f1c..."
}
```

Sirve para:

- Detectar paquetes hechos con una versión más nueva (no se abre si `version` local < `version` del paquete → solo-lectura + aviso, como en `04-modelo-de-datos.md`).
- Diagnosticar de dónde salió un `.tpshow` raro.
- `sessionId` para deduplicar al compartir.

## 6. Comandos de paquete (CLI del propio programa)

```
teatroplayer pack   <carpeta_sesion>  <salida.tpshow>     # crea
teatroplayer unpack <entrada.tpshow>   <carpeta_destino>   # extrae (debug)
teatroplayer verify <archivo.tpshow>                       # valida CRCs y refs
teatroplayer diff   <a.tpshow>         <b.tpshow>          # compara dos paquetes
```

`verify` valida:
- ZIP bien formado, CRCs OK.
- Todas las entradas de `sesion.cues[*].audio.fileName` existen en `audio/`.
- Formatos soportados por los decoders activos.
- Tamaños no cero.

`diff` compara dos paquetes y muestra qué cues/audio cambiaron — útil antes de mandar una actualización al teatro.

## 7. Vinculación a archivos (`04` §3), adaptada al contenedor

Cuando el archivo es un `.tpshow`, **la vinculación es trivial**: los audios están adentro, no hay ruta absoluta ni relativa al sistema de archivos del usuario. La resolución se reduce a:

1. ¿Está `audio/<fileName>` en el paquete? → OK.
2. ¿Coincide `sizeBytes`? → OK.
3. ¿Coincide `hash`? → OK.
4. Si no → la pista se marca **FALTANTE** (raro: solo si el ZIP está corrupto o se editó a mano).

Cuando el usuario abre la sesión por primera vez, se ofrece **"Exportar como carpeta"** (`unpack`) por si quiere tener los WAVs a mano para otra cosa.

## 8. Modo carpeta (legado)

Por compatibilidad y para casos raros (edición de audios en estudio), se mantiene la posibilidad de abrir una **carpeta** con `sesion.json` + `audio/` sueltos, igual que en `04-modelo-de-datos.md`. Pero:

- El modo nativo es el archivo único.
- Al guardar, si la sesión se abrió desde carpeta, se guarda en carpeta; si se abrió desde `.tpshow`, se guarda como `.tpshow`.
- El instalador y los tutoriales enseñan `.tpshow` exclusivamente.

## 9. Pruebas del formato (T-FMT-*)

Ver el plan `08-plan-de-implementacion.md` §`T-FMT-*` para las tareas testeables que verifican:

- T-FMT-001: pack → unpack → pack produce el mismo SHA-256 (round-trip).
- T-FMT-002: dos packs de la misma fuente producen el mismo SHA-256.
- T-FMT-003: `ZipEntryReader` lee el primer sample de un WAV de 100 MB en <50 ms.
- T-FMT-004: loop sobre MP3 desde `.tpshow` 10 minutos sin fuga.
- T-FMT-005: CRC mismatch → la pista se marca FALTANTE, no se rompe el programa.
- T-FMT-006: ZIP64 con una obra de 5 GB funciona.