# TeatroPlayer 0.3.0

Reproductor de audio para teatro: carga una carpeta de audios, los suena con
fades y crossfades, monta escenas reutilizables y guarda la obra entera en un
único `.tpshow` que lleva los audios dentro.

## Qué trae esta versión

**Eventos predefinidos.** Una segunda lista, al lado de los audios, con escenas
ya montadas y reutilizables:

- **Fade in** — un audio que entra poco a poco.
- **Crossfade** — entra el nuevo mientras se va el que está sonando.
- **Fade out** — baja lo que esté sonando, sin tener que buscarlo en la lista.
- **Disparo único** — un efecto de golpe, sin fade, una sola vez: una
  explosión, un rayo, un portazo.

Sólo fade in, crossfade y disparo único piden elegir un audio. En el fade out y
en el lado que sale de un crossfade el audio **no se elige**: es el que está
sonando en ese momento.

**Fade con extremos libres.** La rampa puede ir de cualquier porcentaje a
cualquier otro, no sólo de 0 a 100. Un ambiente puede entrar de 0 a 60 % y
quedarse ahí; una escena puede bajar al 20 % sin apagarse del todo.

**Panel derecho rediseñado.** Antes eran cinco pestañas con icono y había que
adivinar en cuál vivía cada control. Ahora es una columna con secciones, con la
rampa dibujada tal como se va a oír.

**Ajuste sobre la marcha.** Cada evento tiene botones − / + para acortar o
alargar la escena, y funcionan **también en modo Función**: alargar una escena
es mover un número, no rehacer la configuración.

## Cambios en el formato de sesión

El formato sube a la **v3**. Las sesiones antiguas se abren y se migran solas:

- v1 → v2: no hay nada que convertir (sólo se añadían los eventos).
- v2 → v3: los eventos dejan de guardar el audio que sale.

Una versión anterior del programa abrirá una sesión v3 **en sólo lectura**, en
vez de guardarla por encima perdiendo los eventos.

## Cómo se instala

Dos formas, las dos **sin permisos de administrador**:

| Archivo | Para qué |
|---|---|
| `TeatroPlayer-Instalador.exe` | Instalación normal. Registra la extensión `.tpshow`, así que las obras se abren con doble clic. Se desinstala desde Programas y características. |
| `TeatroPlayer-portable.zip` | Sin instalar: se descomprime en un pendrive y funciona. No registra la asociación. |

`THIRD-PARTY.html` recoge los avisos de licencia de las dependencias.

## Un aviso honesto

El ejecutable no está firmado con un certificado de código, así que Windows
SmartScreen puede avisar la primera vez. Se abre con **Más información →
Ejecutar de todas formas**. Está en la lista de cosas por resolver antes de la
1.0.

## Licencia

GPL-3.0-or-later. El código fuente de esta versión está en el tag `v0.3.0`.
