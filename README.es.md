<p align="center"><img src="design/icon/app.svg" width="96" height="96" alt="Icono de Coverly ROMs"></p>

# Coverly ROMs

[English](README.md) · **Español**

Coverly ROMs es una app de escritorio gratuita y multiplataforma (Windows,
Linux, macOS) que audita una biblioteca de ROMs con cualquier estructura de
carpetas —la tarjeta SD de una consola portátil, EmulationStation/ES-DE,
Batocera, RetroArch o una carpeta normal—, muestra qué juegos no tienen
carátula, captura o pantalla de título, y rellena lo que falta en bloque, por
defecto desde la colección gratuita libretro-thumbnails, sin romper nada en la
tarjeta. Las herramientas de referencia solo funcionan en Windows o exigen una
configuración pesada; Coverly ROMs quiere ser la opción cuidada, segura y
multiplataforma.

> **Estado: previo a la primera versión** — se está preparando la v0.1.0;
> ver [RELEASING.md](RELEASING.md) (en inglés).

## Funciones

Previstas para la v1 (ver [la especificación de diseño](docs/superpowers/specs/2026-10-15-coverly-roms-design.md), en inglés):

- **Auditar** cualquier biblioteca: sistemas, ROMs (incluidos juegos de varios
  archivos como `.cue`/`.bin`, `.m3u` y `.gdi`) y el estado de las imágenes de
  cada juego por tipo (carátula, captura, título). Exportar el informe a CSV o
  JSON.
- **Perfiles de estructura** para Anbernic de serie, GarlicOS, Onion, muOS,
  MinUI/NextUI, ES-DE/Batocera, RetroArch y estructuras personalizadas,
  detectados automáticamente y modificables.
- **Rellenar en bloque las imágenes que faltan** desde libretro-thumbnails (sin
  cuenta), con identificación por hash y por nombre, traducción de nombres
  cortos de arcade, varias estrategias (prioridad, región de la ROM,
  popularidad), vista previa sin cambios, copias de seguridad y deshacer. Las
  coincidencias dudosas pasan a una cola de revisión.
- **Organizar una carpeta de ROMs sueltas** —una carpeta de descargas llena de
  zips— en una carpeta por sistema con la estructura que elijas (por defecto
  Anbernic de serie: `ROMS/GBA/`, `ROMS/GB/`, `ROMS/SFC/`…). Cada archivo se
  identifica por su contenido (CRC del catálogo y luego la cabecera) antes que
  por su extensión o su nombre, con una confianza y un motivo; los dudosos
  pasan a una lista de revisión. No se mueve nada hasta que lo confirmas, no
  se sobrescribe nada y se puede deshacer.
- **Nunca sobrescribe** imágenes existentes salvo que lo pidas; nunca escribe
  fuera de la raíz de la biblioteca.
- **Explorar** la biblioteca por carpeta, por sistema o por fecha, con filtros
  (imágenes que faltan, sistema, extensión, región) y búsqueda.
- Fuentes secundarias opcionales (SteamGridDB, más adelante ScreenScraper).
- Una **herramienta de línea de comandos**, `coverly`, para auditar, rellenar
  y organizar sin interfaz.
- Interfaz en inglés y español, temas claro y oscuro, actualizaciones
  automáticas desde GitHub Releases.

Coverly ROMs no incluye ROMs, BIOS ni imágenes de juegos.

## Organizar una carpeta de ROMs

Si eliges una carpeta que no sigue ninguna estructura conocida, Coverly te
ofrece organizarla en lugar de detenerse: **Organizar esta carpeta…** muestra
una propuesta —las ROMs por sistema, la carpeta de destino de cada archivo,
cuánta seguridad tiene Coverly y por qué, los archivos por revisar y los que se
quedan donde están (descargas sin terminar, BIOS, páginas web guardadas como
`.zip`, duplicados)—. Elige la estructura, el destino (esta carpeta u otra,
como una tarjeta SD), mover o copiar y los nombres, y pulsa **Organizar {n} archivos**. Al
terminar, **Abrir biblioteca** abre el resultado como cualquier otra
biblioteca, listo para rellenar sus carátulas; **Deshacer** devuelve cada
archivo a su sitio. Una biblioteca con ROMs sueltas fuera de sus carpetas de
sistema ofrece **Organizar archivos sueltos…** desde la pantalla de la
biblioteca.

Desde el terminal:

```bash
coverly organize ~/Downloads/roms                 # simulación: propuesta por sistema
coverly organize ~/Downloads/roms --apply         # mover a ~/Downloads/roms/ROMS/<SIS>/
coverly organize ~/Downloads/roms --target /Volumes/SD --copy --apply
coverly undo                                      # devolver todo a su sitio
```

Cómo funciona (en inglés): [docs/04-architecture/organize.md](docs/04-architecture/organize.md).

## Instalar

Descarga el archivo para tu ordenador desde la
[última versión](https://github.com/MAECLY/coverly-roms/releases/latest):

| Sistema                                     | Archivo                                                                            |
| ------------------------------------------- | ---------------------------------------------------------------------------------- |
| macOS 11 o superior (Apple silicon e Intel) | `Coverly.ROMs_<versión>_universal.dmg`                                             |
| Windows 10/11 (x64)                         | `Coverly.ROMs_<versión>_x64-setup.exe` (recomendado) o `.msi`                      |
| Linux (x64, ARM64)                          | `.AppImage` o `.deb`                                                               |
| Línea de comandos (`coverly`)               | `coverly-<versión>-<os>-<arq>` (`darwin`, `windows`, `linux`; `x86_64`, `aarch64`) |

Comprueba el archivo con `SHA256SUMS` de la misma versión, por ejemplo
`shasum -a 256 -c SHA256SUMS --ignore-missing` (macOS),
`sha256sum -c SHA256SUMS --ignore-missing` (Linux) o
`Get-FileHash <archivo>` (PowerShell en Windows).

> [!WARNING]
> Las versiones todavía no están firmadas con un certificado de Apple ni de
> Microsoft, así que macOS y Windows te avisan la primera vez. Sigue estos pasos
> solo con un archivo que hayas bajado de las versiones de este repositorio y
> comprobado con `SHA256SUMS`.
>
> <details>
> <summary><b>macOS dice que la app está dañada, o que Apple no puede comprobarla.</b> No lo está.</summary>
>
> macOS pone en cuarentena las apps sin Apple Developer ID. Mueve Coverly ROMs a
> Aplicaciones y quita la marca una vez, en Terminal (las comillas importan: el
> nombre lleva un espacio):
>
> ```bash
> xattr -rd com.apple.quarantine "/Applications/Coverly ROMs.app"
> ```
>
> Esa marca es la comprobación que te protege de una descarga manipulada. En
> lugar de Terminal, también puedes intentar abrirla una vez, ir a **Ajustes del
> Sistema → Privacidad y seguridad**, bajar y pulsar **Abrir igualmente**. El
> binario `coverly` para macOS queda en cuarentena igual si lo baja un
> navegador: `xattr -d com.apple.quarantine coverly-<versión>-darwin-<arq>`.
>
> </details>
>
> <details>
> <summary><b>Windows muestra «Windows protegió su PC».</b></summary>
>
> Elige **Más información** y luego **Ejecutar de todas formas**. Si en su lugar
> ves una **Advertencia de seguridad** con **Editor: Editor desconocido**, elige
> **Ejecutar**. Con Smart App Control activado no hay **Ejecutar de todas
> formas**: bloquea las apps sin firmar, y puede que desactivarlo no tenga
> vuelta atrás sin reinstalar Windows. Coverly ROMs necesita Windows 10 u 11 en x64.
> El `-setup.exe` instala para tu usuario sin permisos de administrador; el
> `.msi` instala para todos los usuarios. Los dos usan Microsoft Edge WebView2:
> Windows 11 ya lo trae, y en Windows 10 el instalador lo descarga si falta
> (necesita internet).
>
> </details>
>
> <details>
> <summary><b>Linux</b></summary>
>
> Marca el AppImage como ejecutable (`chmod +x`) si tu gestor de archivos no lo
> ha hecho. El `.deb` depende de `libwebkit2gtk-4.1-0` y `libgtk-3-0`;
> `sudo apt install ./Coverly.ROMs_<versión>_amd64.deb` (o `_arm64.deb`) los
> instala. Los dos se compilan en Ubuntu 22.04 para x64 y ARM64, así que
> necesitan glibc 2.35 o posterior (Ubuntu 22.04, Debian 12 o posteriores).
>
> </details>

**Línea de comandos.** Renombra el binario a `coverly` (`coverly.exe` en
Windows), hazlo ejecutable (`chmod +x`) y ponlo en tu `PATH`.

**Actualizaciones.** La app busca actualizaciones al iniciarse (Configuración →
General; si lo desactivas no hace ninguna petición) y desde Configuración →
Acerca de, y nunca se reinicia mientras un trabajo de relleno está en marcha.
La CLI: `coverly update check` y `coverly update install`. Cada actualización
se verifica con la clave minisign del proyecto antes de instalarse; una
descarga que no pasa la comprobación no se instala.

## Compilar desde el código

Requisitos: [Rust](https://rustup.rs/) 1.90 o superior,
[Node.js](https://nodejs.org/) 22 o superior con [pnpm](https://pnpm.io/), y
las [dependencias de sistema de Tauri 2](https://v2.tauri.app/start/prerequisites/)
para tu sistema operativo.

```sh
make install                                   # instala las dependencias JS y Rust
make dev                                       # ejecuta la app de escritorio en desarrollo
make web                                       # solo la interfaz, en el navegador, con un backend simulado
make example                                   # crea una biblioteca de ejemplo y la audita
make example-fill                              # planifica rellenar sus portadas (simulación)
make audit ROOT=<ruta>                         # audita una biblioteca desde la terminal
make check                                     # lint, formato y todos los tests
make site                                      # sirve la página web en http://localhost:8080
```

`make help` lista todos los objetivos. La biblioteca de ejemplo vive en
`target/example` (`make example-apply`, `make example-undo`, `make example-clean`).

La página web de [coverly-roms.maecly.com](https://coverly-roms.maecly.com)
vive en `site/`. `make site` la enseña como se verá antes de la primera versión;
`make site RELEASE=scripts/site/fixtures/release.json`, con una versión
publicada, y `make site-check` pasa sus comprobaciones.
[docs/07-build-and-release/landing.md](docs/07-build-and-release/landing.md)
explica cómo se publica (en inglés).

## Contribuir

Lee [CONTRIBUTING.md](CONTRIBUTING.md) (en inglés). Las contribuciones se hacen
bajo el [Acuerdo de Licencia de Contribuidor](CLA.md). Los informes de
seguridad siguen [SECURITY.md](SECURITY.md).

## Licencia y marcas

Coverly ROMs es software libre, publicado bajo la
[GNU Affero General Public License v3.0 only](LICENSE) (`AGPL-3.0-only`).
Copyright © 2026 Miguel Angel Esparza Calero (MAECLY).

El nombre **Coverly ROMs**, su logotipo de texto y su marca son marcas de
MAECLY y no se licencian bajo la AGPL (sección 7(e)); los forks deben usar su
propio nombre. Ver [TRADEMARKS.md](TRADEMARKS.md) y
[docs/09-legal/](docs/09-legal/license.md).

Las imágenes de los juegos se descargan de fuentes de terceros a petición tuya
y pertenecen a sus propietarios; ver
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md). El resto de nombres de
productos mencionados son marcas de sus respectivos propietarios.
