# Preguntas

[Coverly ROMs](../../README.es.md) › Preguntas · **Español** · [English](faq.md)

### ¿Es gratis?

Sí. No cuesta nada, no pide cuenta y no tiene versión de pago. El código es
AGPL-3.0.

### ¿Trae ROMs o portadas?

No. Coverly ROMs no incluye ROMs, BIOS ni arte de juegos. Las imágenes se
descargan de libretro-thumbnails y, si pones tu propia clave, de SteamGridDB.
Cada imagen es de sus dueños.

### ¿Necesita claves de consola?

No. Cada sistema se identifica con lo que se puede leer sin claves: cabeceras
en claro, tablas de archivos, el title ID y el nombre del archivo. Un juego de
Switch o 3DS cuyo nombre no se puede leer así aparece como «Cifrado». Archivos
como `prod.keys` o `title.keys` nunca se abren ni se mueven.

### ¿Va a sobrescribir mis portadas?

No, salvo que lo pidas. Por defecto solo rellena lo que falta. Si eliges
sustituir, antes guarda una copia en la carpeta de datos de la app, no en la
tarjeta, y Deshacer la devuelve.

### ¿Necesito una cuenta?

No. libretro-thumbnails funciona sin cuenta. SteamGridDB es opcional: usa tu
propia clave, que se guarda en el llavero del sistema; añádela en
Configuración → Integraciones.

### ¿Abre comprimidos 7z y RAR?

Sí. Zip, 7z, tar, gzip, bzip2, xz, zstd y lz4 se abren sin instalar nada más.
RAR necesita un programa que ya está en muchos ordenadores, porque el código
oficial de RAR no puede ir dentro de un programa libre: `unrar`, 7-Zip (`7zz` o
`7z`) o `bsdtar`, que macOS ya trae (`/usr/bin/bsdtar`). En Windows encuentra
7-Zip o WinRAR en sus carpetas habituales; en Linux, instala `unrar`, `7zip` o
`libarchive-tools`. Si no hay ninguno, el RAR se queda donde está con el aviso
«Para abrir archivos RAR instala unrar» y se organiza lo demás. Configuración →
Organizar → _Opciones avanzadas_ → _Programas externos para RAR_ muestra lo que
ha encontrado y permite desactivarlos.

Los juegos de disco se **extraen**, porque las consolas cargan el `.cue`/`.gdi`
y las pistas, no el comprimido. Las ROMs de cartucho en zip o 7z se mueven tal
cual cuando el formato de la tarjeta los carga; los demás comprimidos se
extraen. Al mover, el comprimido va a la papelera solo cuando todos los
archivos se han comprobado y no queda nada más dentro (si trae una partida
guardada, el comprimido se queda). Deshacer borra lo extraído y devuelve el
comprimido de la papelera.

### ¿Con qué consolas funciona?

Con las que usan los formatos de Anbernic de fábrica, GarlicOS, Onion OS, MinUI
y NextUI, muOS, ES-DE, Batocera, Knulli y EmulationStation, o RetroArch. Para
otro firmware puedes crear tu formato con vista previa. Aún no se ha probado en
consolas reales.

### ¿Por qué avisan Gatekeeper y SmartScreen?

Porque las descargas aún no llevan certificados de Apple ni de Microsoft. La
primera vez, en macOS quita la marca de cuarentena o pulsa **Abrir
igualmente**; en Windows, elige **Más información** y **Ejecutar de todas
formas**. Hazlo solo con archivos de las versiones de este proyecto. Los pasos
están en [Descargar](../../README.es.md#descargar).

### ¿Envía datos a algún sitio?

No hay telemetría. La app solo se conecta a las fuentes de imágenes que
actives, a los servicios que configures en Configuración → Integraciones, al
catálogo de nombres de Switch si lo activas y, si no lo desactivas, a la
comprobación de actualizaciones al arrancar. Cada tarjeta de Integraciones dice
exactamente qué envía. Las imágenes se recodifican, así que no llevan
metadatos.

### ¿Cómo se actualiza?

Desde la propia app, con versiones de GitHub Releases verificadas con minisign
antes de instalarse. Hay canal estable y beta, y nunca se reinicia en mitad de
un trabajo. La línea de comandos usa `coverly update`.

## Lo que aún no hace

- ScreenScraper como fuente de imágenes: previsto, aún no disponible.
- Vídeos, mezclas de imágenes, marquesinas o logos.
- Editar metadatos, renombrar o verificar ROMs.
- Funcionar en la propia consola.
- Aún no se ha probado en consolas reales.
- Las versiones aún no están firmadas con un certificado de Apple ni de
  Microsoft.

## Frente a otras herramientas

Skraper, Skyscraper y ARRM usan ScreenScraper y traen vídeos, mezclas de
imágenes y metadatos; Coverly ROMs solo rellena imágenes. Mini Scraper también
funciona sin cuenta y ya tiene versiones publicadas. En lo que Coverly ROMs se
distingue: versión nativa para macOS (Apple silicon e Intel), Windows y Linux
(x64 y ARM64), el formato `Imgs/` de Anbernic de serie, sin cuenta, y
simulación y deshacer en cada trabajo. La
[comparación de la web](https://coverly-roms.maecly.com/#comparar) cita las
fuentes de cada herramienta.
