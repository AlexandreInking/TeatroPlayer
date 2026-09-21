# 11 — Licencia: GPL-3.0

## 1. El requisito del autor

> "Dejalo Open Source, pero lo que salga de acá también debe ser Open Source."

Eso es, palabra por palabra, la definición de una licencia **copyleft fuerte**.

## 2. Decisión: **GNU GPL v3.0**

| Criterio | GPL-3.0 | AGPL-3.0 | LGPL-3.0 | MIT / Apache |
|---|---|---|---|---|
| Es open source (OSI) | ✅ | ✅ | ✅ | ✅ |
| **Las obras derivadas deben ser open source** | ✅ | ✅ | ⚠️ (solo la librería) | ❌ |
| Cubre software de escritorio local | ✅ | ✅ (cláusula de red innecesaria) | ✅ | ✅ |
| Permite donaciones | ✅ | ✅ | ✅ | ✅ |
| Permite cobrar | ✅ | ✅ | ✅ | ✅ |
| Impide que un tercero venda | ❌ | ❌ | ❌ | ❌ |

**GPL-3.0, no AGPL-3.0:** la cláusula de red del AGPL (§13) existe para servicios que se usan sin recibir el binario — un servidor web. TeatroPlayer es un ejecutable local que se instala en una laptop; la cláusula no aporta nada y solo añade fricción y confusión.

**No LGPL:** el LGPL es para bibliotecas que se enlazan desde software cerrado. Acá el producto *es* la app; el LGPL permitiría que alguien la envuelva y la cierre.

## 3. Lo que GPL-3.0 garantiza (el objetivo)

1. **Cualquiera puede usar el programa** para lo que quiera, incluido correr el sonido de una obra con entradas pagas. Sin permisos, sin registros, sin límites.
2. **Cualquiera puede modificarlo.**
3. **Cualquiera puede compartirlo**, modificado o no.
4. **Si alguien distribuye una versión modificada, tiene que publicar el código fuente bajo GPL-3.0.** ← Esta es la cláusula pedida. Un fork cerrado es imposible: en el momento en que lo distribuye, debe abrirlo.
5. **No se le pueden agregar restricciones:** nadie puede tomar el código y sumarle un EULA, una prohibición de uso comercial ni una cláusula de no-redistribución.

## 4. Lo que GPL-3.0 NO hace (y conviene saberlo antes de firmar)

**No impide que otros vendan el programa.** Ninguna licencia open source lo impide. Si alguien quiere "que nadie más lo venda", la respuesta honesta es: eso no es open source.

Lo que sí pasa si alguien vende una copia:
- Está obligado a entregar el código fuente completo junto con el binario (o una oferta escrita válida 3 años).
- No puede impedir que su comprador lo vuelva a distribuir gratis.
- No puede añadir restricciones.

En la práctica esto desarma el modelo de "build privado exclusivo": en el momento en que se lo vendés a alguien, esa persona es libre de publicarlo. **Lo que sigue en pie:**

- **Donaciones** (modelo FSF / Wikipedia).
- **Cobrar por el binario en sí** (permitido; no es exclusivo).
- **Cobrar por servicios alrededor:** soporte prioritario, instalación y configuración en la sala, capacitación del operador, build firmado y verificado, desarrollo de funciones a medida.
- **Dual licensing**, pero **solo si usted es titular de todo el copyright** — ver §7 sobre el CLA.

## 5. Obligaciones al distribuir binarios (checklist de release)

- [ ] Incluir el texto completo de la GPL-3.0 como `LICENSE`.
- [ ] Publicar el **código fuente completo correspondiente** (repo + tag del commit exacto) en el mismo release. Un enlace al repo alcanza si el tag es inequívoco.
- [ ] Aviso prominente en el README y en "Acerca de": copyright, GPL-3.0, "sin garantía" (§15/§16).
- [ ] Si se modificó respecto de una versión anterior, indicarlo con fecha (§5(a)).
- [ ] Incluir `LICENSES/` con las licencias de terceros (MIT, Apache-2.0, MPL-2.0).
- [ ] **No** añadir restricciones extra: ni "no comercial", ni EULA, ni cláusulas de uso.
- [ ] **No** exigir regalías ni patentes por encima de la licencia (§10: al distribuir bajo GPL-3.0 se concede automáticamente licencia de patente a los receptores).
- [ ] "Installation information" (§6): solo aplica a productos de consumo cerrados. Una laptop no lo es — no aplica.

## 6. Compatibilidad de la cadena de dependencias

| Dependencia | Licencia | ¿Compatible con GPL-3.0? |
|---|---|---|
| `rodio`, `cpal` | MIT OR Apache-2.0 | ✅ (Apache-2.0 es compatible en un sentido con GPLv3) |
| `egui`, `eframe` | MIT OR Apache-2.0 | ✅ |
| `serde`, `serde_json` | MIT OR Apache-2.0 | ✅ |
| `thiserror`, `anyhow` | MIT OR Apache-2.0 | ✅ |
| `tracing`, `tracing-appender` | MIT | ✅ |
| `uuid`, `directories`, `rfd`, `blake3` | Permisivas | ✅ (verificar con `cargo deny`) |
| `Symphonia` (vía `symphonia-all`) | **MPL-2.0** | ✅ — ver nota |

**MPL-2.0 + GPL-3.0:** la MPL 2.0 tiene compatibilidad explícita con la GPL. Al distribuir el binario combinado bajo GPL-3.0:
1. Los **archivos** cubiertos por MPL siguen disponibles bajo MPL-2.0 (se cumple publicando el repo y `LICENSES/`).
2. La obra combinada se distribuye bajo GPL-3.0.
3. Se informa a los receptores cómo obtener el código MPL.

Sin conflicto. Si en algún momento se quiere una cadena sin ningún copyleft parcial, el camino sigue siendo: `default-features = false` en rodio + decoders `hound` (WAV), `minimp3` (MP3), `claxon` (FLAC), `lewton` (Vorbis); se pierde M4A/AAC. **Decisión: mantener `symphonia-all`; la compatibilidad está confirmada.**

**Sobre LivePlay (AGPL-3.0):** el AGPL no se puede relicenciar como GPL-3.0. Su código **no se incorpora** bajo ninguna circunstancia. De la competencia se toman ideas de interacción, documentadas en `10-competencia-y-benchmark.md`.

## 7. Contribuciones: DCO o CLA

Con GPL-3.0 hay dos caminos, y la elección define qué puede hacer el autor después:

| Opción | Qué permite | Cuándo elegirla |
|---|---|---|
| **DCO** (`Signed-off-by`) | Contribuciones entran bajo GPL-3.0 (inbound = outbound). El autor **no** puede relicenciar el código de otros bajo licencia comercial. | Si el proyecto va a ser 100 % GPL para siempre, sin planes comerciales |
| **CLA** (cesión o licencia amplia al autor) | El autor conserva la posibilidad de **dual licensing** (ofrecer una licencia comercial alternativa además de la GPL) | Si en el futuro quiere vender licencias comerciales de la obra completa |

**Recomendación:** arrancar con **DCO** (más simple, es la norma en proyectos GPL comunitarios) y migrar a CLA el día que aparezca una oportunidad comercial concreta. Ojo: migrar después requiere pedirles la firma a todos los contribuyentes anteriores, así que si hay chance real de negocio, **ponga el CLA desde el día uno**. Queda documentado en `CONTRIBUTING.md`.

## 8. Donaciones — totalmente compatibles

La GPL no dice nada sobre pedir dinero. El modelo de la FSF es exactamente este: software GPL + donaciones.

| Canal | Nota |
|---|---|
| **GitHub Sponsors** | Estándar, integrado en el repo (`.github/FUNDING.yml`) |
| **Ko-fi** | Donaciones chicas, sin fricción |
| **Mercado Pago / PIX** | Para donantes de Latinoamérica sin tarjeta internacional |
| **PayPal.Me** | Alternativa conocida |

Lo que hay que dejar claro en el README para que nadie se confunda:

> Donar **no** compra una licencia: el programa es GPL-3.0 y lo puede usar cualquiera, haya donado o no. Las donaciones financian el desarrollo. Si necesitás soporte dedicado, capacitación o una función a medida, eso es trabajo pagado y se cotiza aparte.

## 9. Marca y nombre

- **TeatroPlayer** es marca del autor. La GPL-3.0 concede derechos sobre el **código**, no sobre la marca.
- Los forks pueden existir y distribuir bajo GPL-3.0, pero **deben usar otro nombre**. Se pide en el README y se reserva el nombre.
- Verificar colisión de nombres antes de la primera release pública.

## 10. Archivos del repositorio

```
/
├─ LICENSE                    # texto completo GPL-3.0, sin modificar
├─ NOTICE                     # aviso de copyright + mención a terceros
├─ LICENSES/                  # generado por cargo-about (MIT, Apache-2.0, MPL-2.0…)
├─ README.md                  # ES/EN: qué es, licencia GPL-3.0, cómo donar
├─ CONTRIBUTING.md            # DCO (o CLA) + cómo arrancar
├─ CODE_OF_CONDUCT.md         # Contributor Covenant
├─ SECURITY.md
├─ CHANGELOG.md
├─ .github/
│  ├─ FUNDING.yml
│  ├─ ISSUE_TEMPLATE/{bug.yml,feature.yml}
│  └─ workflows/ci.yml
└─ Cargo.toml                 # license = "GPL-3.0-or-later"
```

`license = "GPL-3.0-or-later"` permite a los receptores usar versiones futuras de la GPL. Para fijar la versión: `"GPL-3.0-only"`. **Recomendación: `GPL-3.0-or-later`** (lo que recomienda la FSF y simplifica combinaciones futuras).

CI:
- `cargo deny check licenses bans advisories` → falla si entra una licencia incompatible con GPL-3.0.
- `cargo about generate` → regenera `LICENSES/`.
- `cargo audit`.

## 11. Escenarios concretos

| Situación | ¿OK bajo GPL-3.0? | Nota |
|---|---|---|
| Una escuela lo usa para el acto | ✅ | Sin contactar al autor |
| Un teatro profesional cobra entradas y lo usa | ✅ | Sin límites |
| Alguien lo modifica para su sala | ✅ | No tiene que publicar nada si no distribuye |
| Alguien publica su versión modificada | ✅ | **Debe** publicar el código bajo GPL-3.0 |
| Alguien vende copias | ✅ | Debe entregar el código fuente junto con el binario |
| Alguien hace un fork cerrado | ❌ | Incumple; se le puede exigir el código |
| Alguien lo integra en un producto cerrado propio | ❌ | El producto entero pasaría a tener que ser GPL-3.0 |
| El autor pide donaciones | ✅ | Modelo estándar |
| El autor cobra por soporte / instalación / capacitación | ✅ | Servicios, no licencia |
| El autor cobra por el binario | ✅ | Pero el comprador puede redistribuirlo gratis |
| El autor vende una licencia comercial alternativa | ⚠️ | Solo posible si tiene CLA de todos los contribuyentes |
