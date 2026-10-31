# TODO — Coverly ROMs

## Landing

La landing (https://coverly-roms.maecly.com) vive en su propio repositorio, https://github.com/MAECLY/coverly-roms-landing, y sus tareas pendientes están en su [TODO.md](https://github.com/MAECLY/coverly-roms-landing/blob/main/TODO.md).

## Release y pruebas que dependen de Miguel

El detalle está en `RELEASING.md` y `docs/TRACKER.md`:

- [x] Generar el par de claves minisign, cargar los 2 secretos y sustituir `updater.pub` (M7-01/M7-05).
- [x] Elegir el icono definitivo (Lean-To, v05 de la ronda 2).
- [x] Etiquetar y publicar `v0.1.0` (https://github.com/MAECLY/coverly-roms/releases/tag/v0.1.0).
- [ ] Copia de seguridad fuera del Mac de `~/.coverly-release/updater.key` y su contraseña (responsable: Miguel).
- [ ] Credenciales de SteamGridDB y ScreenScraper para las pruebas reales (responsable: Miguel).
- [ ] Pruebas manuales de la app Tauri en macOS, Windows y Linux, y en la consola real (M2-12) (responsable: Miguel).

## Roadmap

Orden de prioridad. Cada bloque se empieza solo cuando Miguel lo diga; la lista de tareas de cada uno es el punto de partida para su spec y su plan.

0. [Nombres de los archivos de cada release y Windows ARM](#0-nombres-de-los-archivos-de-cada-release-y-windows-arm)
1. [Mejora de la interfaz de «Organizar por sistema»](#1-mejora-de-la-interfaz-de-organizar-por-sistema)
   1.5. [Mejora de la interfaz de Duplicados](#15-mejora-de-la-interfaz-de-duplicados)
   1.6. [Reordenar los ajustes de Biblioteca](#16-reordenar-los-ajustes-de-biblioteca)
2. [Sistemas que faltan](#2-sistemas-que-faltan)
3. [Biblioteca](#3-biblioteca)
4. [Sincronizar con dispositivos](#4-sincronizar-con-dispositivos)
5. [Partidas guardadas (saves)](#5-partidas-guardadas-saves)

## 0. Nombres de los archivos de cada release y Windows ARM

Hoy los archivos se llaman como los genera Tauri (`Coverly.ROMs_0.2.0_universal.dmg`, `_x64-setup.exe`, `_amd64.AppImage`, `_aarch64.AppImage`, `_arm64.deb`…) y la CLI `coverly-0.2.0-darwin-aarch64`. Mezclan `amd64`/`x64` y `aarch64`/`arm64` y no dicen el sistema con palabras normales. Nombres propuestos, iguales para la app y la CLI:

| Plataforma            | Sufijo                                     |
| --------------------- | ------------------------------------------ |
| macOS Apple silicon   | `_mac_arm` (o `_mac_silicon`; decidir uno) |
| macOS Intel           | `_mac_intel`                               |
| Linux x64             | `_linux_x64`                               |
| Linux ARM64           | `_linux_arm`                               |
| Windows x64           | `_windows_x64`                             |
| Windows ARM64 (nuevo) | `_windows_arm`                             |

Ejemplos: `Coverly-ROMs_0.3.0_mac_arm.dmg`, `Coverly-ROMs_0.3.0_windows_x64-setup.exe`, `Coverly-ROMs_0.3.0_linux_arm.AppImage`, `coverly_0.3.0_linux_x64`.

- [x] Decidir `_mac_arm` o `_mac_silicon`, y si se mantiene además el `.dmg` universal (un solo archivo para los dos Mac) o se cambia por dos builds separados, más pequeños. Si se separan, el job `app` de macOS pasa a dos (`aarch64-apple-darwin` y `x86_64-apple-darwin`). Hecho en PR #29: `_mac_arm` y dos builds separados, sin `.dmg` universal.
- [x] Renombrar en el paso de staging de `release.yml` (`release_tools.py stage-builds`), no en Tauri, para no tocar los nombres que espera el updater por dentro.
- [x] Actualizar `latest.json` (las URLs de cada plataforma), `SHA256SUMS`, `.github/release-notes.md` (tabla de descargas en/es), `RELEASING.md` §5 (lista de archivos a revisar), README en/es y la landing (`site/assets/release.js` elige el archivo según el sistema del visitante).
- [ ] Compatibilidad: las versiones ya instaladas leen `latest.json` por clave de plataforma (`darwin-aarch64`, `windows-x86_64`…), no por nombre de archivo, así que el cambio de nombres no rompe las actualizaciones. Comprobarlo con una actualización real de 0.2.0 a la siguiente. PR #29 lo prueba con tests del manifiesto (las claves no cambian); falta la actualización real tras la próxima release.
- [ ] **Windows ARM64**: añadir el build `aarch64-pc-windows-msvc` de la app (NSIS y MSI) y de la CLI en el runner `windows-11-arm` (gratis en repositorios públicos), su entrada `windows-aarch64` en `latest.json` y en las notas, y probar que WebView2 y el instalador funcionan en ARM. Build, entrada y notas hechos en PR #29; falta probar el instalador y WebView2 en un Windows 11 ARM real tras la próxima release.

## 1. Mejora de la interfaz de «Organizar por sistema»

- [x] Animación de progreso al extraer en «Organizar por sistema». Ahora solo muestra la carpeta y una línea de texto («Extrayendo Next Tetris, The.zip: 0 de 1 archivos…»), y se queda corta para extracciones que duran minutos. Propuesta:
  - Un movimiento propio, en la línea de la landing «The Lean-To»: por ejemplo, el comprimido que se abre y las pistas que salen hacia la carpeta del sistema, con su `SystemIcon`.
  - Barra por bytes (`OrganizePhase::Extract { bytes_done, bytes_total }` ya llega del núcleo), velocidad y tiempo restante.
  - Lista de las pistas del juego en curso, con su CRC verificado (✓) a medida que terminan.
  - Respetar `prefers-reduced-motion` y usar los tokens de `design/tokens.json`.
- [x] El contador «0 de 1 archivos» cuenta elementos del plan, no archivos dentro del comprimido, y se lee mal. Separar «juego 1 de 3» de «pista 4 de 12».
- [x] Revisar otras pantallas de progreso que también están demasiado simples (relleno de portadas, duplicados, escaneo) para darles el mismo tratamiento.

## 1.5. Mejora de la interfaz de Duplicados

La pantalla «estante y bandeja» da la información, pero se queda simple. Mejoras:

- [x] **Mostrar en Finder / Explorador / gestor de archivos** para cada copia (la que se queda y las que se apartan) y para la carpeta `_Duplicados/` tras aplicar. En Tauri, `tauri-plugin-opener` (`revealItemInDir`) cubre macOS, Windows y Linux; texto según el sistema: «Mostrar en Finder», «Mostrar en el Explorador», «Mostrar en la carpeta».
- [x] **Etiqueta de región** en cada copia (USA, Europe, Japan, World…, con su bandera o código), sacada del nombre del catálogo o de las etiquetas del nombre del archivo (`NameTags`). Cuando las copias son el mismo juego, la región es lo que más ayuda a decidir; también revisión (`Rev 1`), idioma (`En,Fr,De`) y marcas de volcado (`[!]`, `[b]`, hack, traducción) como etiquetas.
- [x] **Portadas**: hoy la pantalla no muestra ninguna. Enseñar la portada de cada grupo (y de cada copia si tiene la suya), reutilizando las de la biblioteca y las de libretro-thumbnails; con hueco de portada claro cuando falte.
- [x] Más detalle por copia: tamaño, formato (zip, 7z, CHD, CUE/BIN), fecha de modificación, si tiene partida guardada y de cuándo, y por qué regla se eligió la que se queda.
- [x] Repaso visual con el skill de diseño: jerarquía entre el juego, la copia que se queda y las apartadas; estados (conflicto, solo informado, otra versión) más visibles; atajos de teclado para elegir copia y aplicar; animación al apartar en la línea de la landing.

## 1.6. Reordenar los ajustes de Biblioteca

Configuración › Biblioteca (`src/components/settings/LibrarySection.tsx` + `DedupeSettings.tsx`, unas 770 líneas) mezcla tres cosas distintas en una sola pestaña y se hace larga:

- **Escaneo**: perfil por defecto, seguir enlaces simbólicos, archivos ocultos, carpetas y extensiones ignoradas, bibliotecas recientes.
- **Organizar**: modo (mover / copiar / automático), disposición por defecto, nombres (catálogo / conservar), anidar en `ROMS/`, programas externos para RAR, descargar catálogos.
- **Duplicados**: acción (cuarentena / Papelera), carpeta de cuarentena, niveles, una ROM por juego, reglas para elegir la copia que se queda y su orden, orden de regiones, orden de contenedores por sistema, sistemas excluidos, paso dentro de Organizar.

Tareas:

- [x] Separar en pestañas propias: **Biblioteca** (solo escaneo y bibliotecas recientes), **Organizar** y **Duplicados**. Valorar si «Coincidencias» (regiones) absorbe el orden de regiones de duplicados para no tenerlo dos veces.
- [x] Dentro de cada pestaña, lo habitual arriba y lo avanzado plegado («Opciones avanzadas»): por ejemplo, en Duplicados dejar a la vista la acción y la carpeta, y plegar niveles, reglas, contenedores y exclusiones.
- [x] Revisar textos de ayuda: uno corto por opción y el detalle en un «Más información», en vez de párrafos largos.
- [x] Buscador de ajustes (⌘/Ctrl+F dentro de Configuración) y enlaces directos desde cada pantalla («Ajustes de Duplicados» desde la pantalla de duplicados, «Ajustes de Organizar» desde Organizar).
- [x] Preparar el sitio para lo que viene del roadmap (Biblioteca con colecciones, Dispositivos, Partidas, RetroAchievements): cada uno con su pestaña, no apilado en Biblioteca.
- [x] Mantener los tests de `Sections.test.tsx` y la paridad i18n; E2E de la navegación por pestañas con teclado.

## 2. Sistemas que faltan

Hoy `crates/coverly-core/src/systems/systems.toml` define 54 sistemas. Faltan los de esta lista. Por cada sistema hay que añadir:

1. Su entrada en `systems.toml`: `id`, nombre, alias de carpeta, extensiones, carpeta de libretro-thumbnails si existe, DAT de libretro-database si existe e id de ScreenScraper.
2. Su carpeta en cada perfil de consola (`crates/coverly-core/src/profiles/builtin.rs`) donde el frontend lo soporte, y `disc_layout` si es de disco.
3. Identificación: por cabecera cuando la haya (`identify/`, `disc/`) y por hash con su DAT.
4. Su icono en `SystemIcon` (silueta de RetroArch Assets si existe; si no, una propia).
5. Tests: detección por cabecera, carpeta de destino en Organizar, búsqueda de portada y snapshot del plan.

### Tanda 1: retro sin formatos nuevos

Cartuchos, cintas y discos que ya entran en los lectores actuales.

- [ ] PlayStation 2 (`ps2`). El disco ya se detecta (`BOOT2` en `SYSTEM.CNF`, volumen `PLAYSTATION` mayor que un CD) y hoy sale como `Unsupported("PlayStation 2")`. Formatos: ISO, CHD, CSO, ZSO. Redump.
- [ ] 3DO (`3do`). El disco ya se detecta y hoy sale como `Unsupported("3DO")`. Formatos: ISO, CUE/BIN, CHD.
- [ ] Atari Jaguar y Jaguar CD (`jaguar`, `jaguarcd`). Formatos: `.j64`, `.jag`, `.rom`, `.abs`, `.cof`, CUE/BIN y CDI para el CD.
- [ ] ColecoVision (`colecovision`). Formatos: `.col`, `.rom`.
- [ ] Intellivision (`intellivision`). Formatos: `.int`, `.bin`, `.rom`.
- [ ] Vectrex (`vectrex`). Formatos: `.vec`, `.gam`, `.bin`.
- [ ] Magnavox Odyssey² / Videopac (`odyssey2`). Formato: `.bin`.
- [ ] Fairchild Channel F (`channelf`). Formatos: `.bin`, `.chf`.
- [ ] Emerson Arcadia 2001 (`arcadia`). Formato: `.bin`.
- [ ] Watara Supervision (`supervision`). Formatos: `.sv`, `.bin`.
- [ ] Sega SG-1000 / SC-3000 (`sg1000`). Formatos: `.sg`, `.sc`.
- [ ] Sega Pico (`pico`). Formato: `.md`.
- [ ] PC Engine SuperGrafx (`supergrafx`). Formatos: `.sgx`, `.pce`.
- [ ] NEC PC-FX (`pcfx`). Formatos: CUE/BIN, CHD.
- [ ] Philips CD-i (`cdi`). Formatos: CHD, CUE/BIN.
- [ ] Amiga CD32 y CDTV (`amigacd32`, `cdtv`). Formatos: CUE/BIN, ISO, CHD.
- [ ] Nintendo 64DD (`n64dd`). Formato: `.ndd`.
- [ ] Satellaview y Sufami Turbo (`satellaview`, `sufami`). Formatos: `.bs`, `.st`.
- [ ] Nintendo Famicom/NES en variantes de catálogo: comprobar si hacen falta entradas aparte para Famicom (BS-X, Disk Writer) o basta con alias.
- [ ] Tiger Game.com (`gamecom`). Formatos: `.tgc`, `.bin`.
- [ ] Mega Duck / Cougar Boy (`megaduck`). Formato: `.bin`.
- [ ] Epoch Super Cassette Vision (`scv`). Formatos: `.bin`, `.0`.
- [ ] Ordenadores:
  - [ ] ZX Spectrum (`zxspectrum`). Formatos: `.tzx`, `.tap`, `.z80`, `.sna`, `.dsk`, `.scl`, `.trd`.
  - [ ] ZX81 (`zx81`). Formatos: `.p`, `.81`.
  - [ ] Amstrad CPC (`amstradcpc`). Formatos: `.dsk`, `.cdt`, `.sna`.
  - [ ] Sharp X68000 (`x68000`). Formatos: `.dim`, `.xdf`, `.hdm`, `.hdf`, `.2hd`, `.m3u`.
  - [ ] Sharp X1 (`x1`). Formatos: `.2d`, `.2hd`, `.dx1`, `.tap`.
  - [ ] NEC PC-98 (`pc98`). Formatos: `.d88`, `.fdi`, `.hdi`, `.nhd`.
  - [ ] NEC PC-88 (`pc88`). Formatos: `.d88`, `.t88`.
  - [ ] FM Towns (`fmtowns`). Formatos: CUE/BIN, CHD, `.d88`.
  - [ ] MSX2 y MSX turboR, si los perfiles los separan de `msx`.
  - [ ] Apple II (`apple2`). Formatos: `.dsk`, `.do`, `.po`, `.nib`, `.woz`, `.2mg`.
  - [ ] BBC Micro (`bbcmicro`). Formatos: `.ssd`, `.dsd`, `.uef`.
  - [ ] Commodore Plus/4, PET y C128 (`plus4`, `pet`, `c128`). Formatos: `.prg`, `.d64`, `.t64`, `.tap`, `.crt`.
  - [ ] Atari XE / XEGS, si los perfiles los separan de `a800`.
  - [ ] TI-99/4A (`ti99`). Formatos: `.rpk`, `.ctg`, `.bin`.
  - [ ] Thomson MO/TO (`thomson`). Formatos: `.fd`, `.k7`, `.m7`, `.rom`.
  - [ ] Dragon 32/64 y TRS-80 CoCo (`dragon`, `coco`). Formatos: `.cas`, `.dsk`, `.ccc`.
- [ ] Consolas de fantasía y homebrew:
  - [ ] TIC-80 (`tic80`). Formato: `.tic`.
  - [ ] WASM-4 (`wasm4`). Formato: `.wasm`.
  - [ ] Arduboy (`arduboy`). Formatos: `.hex`, `.arduboy`.
  - [ ] Uzebox (`uzebox`). Formatos: `.uze`, `.hex`.
  - [ ] LowRes NX (`lowresnx`). Formato: `.nx`.
  - [ ] Vircon32 (`vircon32`). Formato: `.v32`.
- [ ] Ports y motores: PrBoom/Doom (`.wad`), Quake (`.pak`), Cave Story (NXEngine), Wolfenstein 3D (ECWolf), Mr. Boom, Dinothawr, Lutro, Solarus, Flashback (REminiscence), Rick Dangerous (XRick). Necesitan reglas de «juego = carpeta» como ScummVM.
- [ ] Arcade adicional: Model 2, Model 3, Naomi 2, Triforce, Sega System 16/18/24/32, Taito Type X, Konami, Cave, CHD de MAME por juego.

### Tanda 2: discos grandes y portátiles con formato propio

Hace falta un lector de su formato y una identificación propia.

- [ ] Nintendo GameCube (`gamecube`). Formatos: ISO, GCM, CISO, GCZ, RVZ, WIA. Id de juego en la cabecera del disco (6 caracteres en el offset 0).
- [ ] Nintendo Wii (`wii`). Formatos: ISO, WBFS, CISO, GCZ, RVZ, WIA, y WAD de WiiWare. Misma cabecera que GameCube con la magia Wii en 0x18.
- [ ] Nintendo Wii U (`wiiu`). Formatos: WUD, WUX, WUA, RPX y las carpetas descifradas (`code/`, `content/`, `meta/`). Id en `meta/meta.xml`.
- [ ] Nintendo 3DS (`3ds`). Formatos: 3DS, CCI, CIA, CXI, 3DSX, ZCCI, ZCIA. Product code en la cabecera NCSD/NCCH.
- [ ] Nintendo Switch (`switch`). Formatos: XCI, NSP, NSZ, XCZ. Title ID en la cabecera; los NSP/XCI van cifrados y solo se puede leer lo que no lo está.
- [ ] Sony PS Vita (`psvita`). Formatos: VPK, carpetas `app/` descifradas, `.psvimg`. Title ID en `sce_sys/param.sfo`.
- [ ] Sony PS3 (`ps3`). Formatos: ISO, carpetas JB (`PS3_GAME/`), PKG. Title ID en `PS3_GAME/PARAM.SFO`.
- [ ] Microsoft Xbox (`xbox`). Formatos: ISO XISO, carpetas con `default.xbe`. Title ID en la cabecera XBE.
- [ ] Microsoft Xbox 360 (`xbox360`). Formatos: ISO, GOD, XEX, carpetas XBLA. Title ID en la cabecera XEX.
- [ ] Nintendo DSi y DSiWare, si los perfiles los separan de `nds`.

### Tanda 3: generación actual (catalogar, no emular)

En estas consolas casi no hay emulación jugable: shadPS4 para PS4 está en desarrollo y no hay nada usable para PS5, Xbox One ni Series. Aquí Coverly sirve para catalogar copias de seguridad: poner portada, ordenar por sistema y detectar duplicados. Antes de empezar hay que decidir qué perfiles de frontend los muestran: Batocera/ES-DE o una biblioteca de PC.

- [ ] Sony PS4 (`ps4`). Formatos: PKG y carpetas `CUSA*`. Title ID (`CUSAxxxxx`) en `sce_sys/param.sfo` dentro del PKG; cabecera PKG legible sin descifrar.
- [ ] Sony PS5 (`ps5`). Formatos: PKG y carpetas `PPSA*`. Title ID (`PPSAxxxxx`) en `sce_sys/param.json`.
- [ ] Microsoft Xbox One (`xboxone`). Formatos: paquetes XVC/MSIXVC y carpetas con `appxmanifest.xml`. Identidad en el manifiesto.
- [ ] Microsoft Xbox Series X/S (`xboxseries`). Mismos formatos que Xbox One y también `MicrosoftGame.config`.
- [ ] Portadas para estas consolas: libretro-thumbnails no las tiene. Hay que usar SteamGridDB, ScreenScraper (si las cubre) o una fuente nueva por title ID, como las bases de datos públicas de title IDs de PS4/PS5.

### Decisiones abiertas

- [ ] Nombres de las carpetas por perfil (Anbernic stock, muOS, Knulli, Batocera, ES-DE, RetroArch) para cada sistema nuevo. Investigarlo antes de cada tanda.
- [ ] Qué sistemas aparecen por defecto y cuáles solo si el perfil los usa.
- [ ] Licencias de los iconos que no estén en RetroArch Assets.

## 3. Biblioteca

Hoy la app abre carpetas: en Inicio aparecen como «Unidades conectadas» y «Bibliotecas recientes», pero solo son rutas, y la «biblioteca» es lo que haya en esa ruta en ese momento. La idea es que exista **una biblioteca propia, la definitiva**, que vive en el ordenador y tiene identidad y funciones propias. Los dispositivos (la SD de la Anbernic, la Ayaneo…) pasan a ser destinos que se comparan y sincronizan con ella (bloque 4).

### Qué suele tener una biblioteca para considerarse como tal

Referencias revisadas: [Playnite](https://api.playnite.link/docs/manual/gettingStarted/comparisonWithOtherSoftware.html), [LaunchBox](https://mundobytes.com/en/Organize-your-game-library-with-Launchbox-or-Playnite/), [RomM](https://github.com/rommapp/romm/) ([docs](https://docs.romm.app/4.9.2/)), ES-DE y Batocera. En común:

- **Identidad propia.** Un nombre, una ubicación fija (una o varias carpetas raíz) y una base de datos que persiste entre sesiones. No depende de que la unidad esté conectada para ver qué hay.
- **El juego como entidad, no el archivo.** Un juego identificado (hash / serial / catálogo) con sus archivos: ROM o set de disco, partidas, portadas, manual, parches. Varias versiones o regiones del mismo juego agrupadas.
- **Metadatos ricos y editables.** Título, sistema, región, idioma, año, desarrollador, editor, género, jugadores, descripción, valoración. Fuentes: catálogos (No-Intro, Redump), ScreenScraper, IGDB, MobyGames, SteamGridDB. Corrección manual que no se pierde al volver a escanear.
- **Arte completo.** Portada, título, captura, logo, fondo, caja 3D, vídeo de muestra; lo que ya hace el relleno, pero guardado en la biblioteca y no solo en la carpeta de cada frontend.
- **Colecciones y etiquetas.** Favoritos, listas manuales («para la Anbernic», «RPG pendientes»), colecciones inteligentes por filtro (sistema, género, región, «sin portada») y etiquetas libres.
- **Estado del jugador.** Jugado / sin jugar, completado, valoración propia, notas, tiempo de juego y última partida cuando se puedan leer (`gamelist.xml`, historial de RetroArch, Playnite).
- **Integridad.** Verificación contra DAT (bien / mala / desconocida), duplicados (ya existe), huecos de arte (ya existe), archivos que faltan o cambiaron desde el último escaneo.
- **Historial y deshacer.** Registro de cada cambio que hace la app, como los trabajos actuales.
- **Copia de seguridad y exportación.** Exportar / importar la base de datos, informes (ya hay CSV/JSON), y backup de partidas (bloque 5).
- **Varias raíces y volúmenes.** Una biblioteca puede repartirse entre discos (p. ej. arcade en un externo); se reconocen por UUID de volumen aunque cambie la letra o el punto de montaje.

### Tareas

- [ ] Investigar a fondo y escribir la spec: modelo de datos (biblioteca → juegos → archivos → arte / partidas), dónde vive la base de datos (la caché SQLite actual u otra), migración desde «bibliotecas recientes».
- [ ] Crear biblioteca: nombre, carpetas raíz, perfil de organización interno (el formato «maestro» de la biblioteca, independiente del de cada dispositivo).
- [ ] Importar a la biblioteca desde una carpeta o un dispositivo (copiar o mover, con deshacer), reutilizando Organizar y duplicados.
- [ ] Ficha de juego con metadatos editables y fuentes nuevas (IGDB, MobyGames) además de las de arte.
- [ ] Colecciones manuales, inteligentes y favoritos; etiquetas.
- [ ] Estado del jugador y lectura de tiempo de juego donde exista.
- [ ] Rediseñar Inicio: «Mi biblioteca» como centro, y debajo «Dispositivos» (bloque 4) en lugar de rutas sueltas.
- [ ] Exportar / importar la biblioteca y su base de datos.

### RetroAchievements

Revisado: se puede integrar, y encaja con la biblioteca porque los logros son por juego y por usuario.

- **API web pública**: cada usuario saca su clave en RetroAchievements → Settings → Keys y la usa junto a su usuario ([docs de la API](https://api-docs.retroachievements.org/v1/get-game-info-and-user-progress.html)). Hay endpoints para la ficha del juego con el progreso del usuario (`API_GetGameInfoAndUserProgress`), el progreso por juegos (`API_GetUserProgress`) y el rango y puntuación por juego. Clientes oficiales en JS y Python (MIT); para Rust habría que escribir el cliente (HTTP + JSON, sencillo).
- **Identificar el juego**: RetroAchievements no usa el hash del archivo entero sino su propio hash por sistema (sin cabecera iNES en NES, sin la cabecera de 512 bytes en SNES, el ejecutable de `SYSTEM.CNF` en PlayStation, orden de bytes en N64, sectores concretos en discos, nombre del archivo en arcade). Lo implementa la librería C `rcheevos` (`rc_hash_generate`), que ya usan RetroArch y DuckStation ([rcheevos en DuckStation](https://git.morozoff.pro/aiden/duckstation/src/branch/master/dep/rcheevos), [node-rcheevos](https://github.com/JZimz/node-rcheevos)). Opciones: enlazar `rcheevos` o portar a Rust el hash de los sistemas que soportamos. Comprobar antes su licencia en el repositorio oficial (creemos que es MIT, no está confirmado).
- **Usuarios**: desde 2025 el nombre de usuario puede cambiar; guardar su ULID, no el nombre.

Tareas:

- [ ] Ajuste «Cuenta de RetroAchievements»: usuario y clave de la API web en el llavero del sistema, como las credenciales de SteamGridDB y ScreenScraper (nunca en el disco ni en los registros). «Probar clave» igual que en Fuentes.
- [ ] Calcular el hash de RetroAchievements de cada juego al escanear (guardado en la caché, como las claves de contenido) y averiguar su id de juego en RA.
- [ ] En la ficha de cada juego: si tiene logros, cuántos, cuántos has conseguido (normal y hardcore), puntos y el último desbloqueo, con los iconos de los logros.
- [ ] En la biblioteca: insignia «tiene logros» / «sin logros» / «versión no compatible» (cuando el hash no coincide con ninguna versión soportada, avisar y proponer la versión buena), filtro por logros y colección inteligente «con logros pendientes».
- [ ] Al elegir qué copia se conserva en duplicados y al sincronizar (bloque 4), preferir la versión compatible con RetroAchievements como regla opcional.
- [ ] Respetar los límites de uso de la API: caché local, sin peticiones masivas, y todo opcional y desactivado sin clave.

## 4. Sincronizar con dispositivos

Origen de la idea: tener una Anbernic RG SP y una Ayaneo Pocket DS y poder llevar la biblioteca a cada una, sabiendo qué juegos hay en la biblioteca, cuáles hay en el dispositivo y cuáles solo en uno de los dos.

### Cómo lo resuelve hoy la gente (investigación)

- **Syncthing** es lo más usado entre portátiles retro, pero exige firmware personalizado (Knulli lo trae instalado; en el firmware de fábrica de Anbernic no se puede) y se recomienda sincronizar solo las partidas, no la carpeta de ROMs entera, porque bloquea la SD durante minutos u horas ([Knulli](https://knulli.org/configure/syncthing/), [Retro Game Corps](https://retrogamecorps.com/2024/08/11/guide-using-syncthing-with-retro-handhelds/)).
- **RetroArch** tiene sincronización en la nube por WebDAV solo para sus propias partidas.
- **EmuDeck** sincroniza partidas en la nube (Google Drive, Box…) y está pensado para Steam Deck y PC ([EmuSync](https://github.com/emu-sync/EmuSync/wiki/Setting-up-a-game-sync) es otra herramienta parecida).
- **RomM** es un gestor de ROMs en servidor propio con apps para Android y portátiles y un protocolo de sincronización de dispositivos en desarrollo, pero exige montar un servidor ([RomM](https://github.com/rommapp/romm/)).
- No se ha encontrado ninguna app de escritorio que compare juego a juego una biblioteca con varios dispositivos y los sincronice con el formato de carpetas de cada uno. Ese es el hueco que puede cubrir Coverly.

### Dispositivos

- [ ] Añadir dispositivo: una ruta (SD montada, carpeta) o un dispositivo Android por USB-C.
- [ ] **Seudónimo**: nombre propio para mostrar («Mi Ayaneo»), guardado solo en Coverly. Nunca se renombra el dispositivo, el volumen ni la carpeta.
- [ ] **Icono**: elegir cualquier icono del pack de `SystemIcon` (siluetas de RetroArch Assets) o uno de consola portátil.
- [ ] Reconocer el dispositivo al volver a conectarlo: UUID de volumen para SD y discos; número de serie USB / identificador MTP para Android.
- [ ] Perfil por dispositivo: qué frontend usa (Anbernic stock, muOS, Knulli, Batocera, ES-DE, RetroArch Android…), qué sistemas acepta y dónde guarda las partidas (bloque 5). Detección automática como la actual de perfiles, con corrección manual.
- [ ] **Android por USB (MTP)**: un dispositivo Android conectado por USB no aparece como disco en macOS, así que hace falta MTP. Opción en Rust puro: [`mtp-rs`](https://docs.rs/crate/mtp-rs/0.32.0) (sin dependencias de C, listar / subir / bajar / mover; no cubre Android anterior a 5.0). Alternativa: [libmtp](https://manpages.debian.org/bookworm/libmtp-doc/mtp_basic.3), que además lee el «friendly name» del dispositivo para proponerlo como nombre inicial. Investigar permisos en macOS (Android File Transfer bloquea el acceso exclusivo), Windows y Linux.
- [ ] Alternativas a MTP para Android: carpeta compartida por red (SMB/WebDAV) o un modo «cable» con ADB, si MTP resulta poco fiable.

### Comparar y sincronizar

- [ ] Comparación juego a juego (por identidad de contenido, no por nombre): **en ambos**, **solo en la biblioteca**, **solo en el dispositivo**, **distinta versión** (región, revisión, formato: p. ej. CHD en la biblioteca y CUE/BIN en el dispositivo).
- [ ] Vista de diferencias: reutilizar las vistas actuales (Detalles, Columnas) con un filtro por estado de sincronización, o una pantalla nueva de dos columnas «Biblioteca ↔ Dispositivo». Investigar UX de herramientas de sincronización (FreeFileSync, Syncthing, Beyond Compare, la de iTunes / Finder con iPhone) antes de decidir.
- [ ] Selección de qué llevar: por colección («lo que quiero en la Anbernic»), por sistema o juego a juego. Avisar del espacio libre.
- [ ] Llevar al dispositivo con su formato: carpetas del perfil, arte en el sitio del frontend, `gamelist.xml` o equivalentes, y conversión si el dispositivo no acepta el formato (p. ej. extraer un 7z).
- [ ] Traer del dispositivo a la biblioteca los juegos que solo están allí (importación del bloque 3).
- [ ] Dirección y conflictos: biblioteca → dispositivo, dispositivo → biblioteca, o en los dos sentidos; nunca borrar en el destino sin confirmación; todo con deshacer y registro, como los trabajos actuales.
- [ ] Sincronizar varios dispositivos con la misma biblioteca y ver de un vistazo qué tiene cada uno.

## 5. Partidas guardadas (saves)

Rescatar, ordenar, respaldar y sincronizar las partidas del usuario. Es parte de lo que hace que la biblioteca sea mejor que una carpeta de ROMs. Ninguna herramienta revisada lo hace para emuladores junto con la biblioteca de juegos: [Ludusavi](https://github.com/mtkennerly/ludusavi) respalda partidas de juegos de PC con un manifiesto de rutas (YAML, sacado de PCGamingWiki) y entradas propias, pero no cubre emuladores; RetroArch, EmuDeck y Syncthing solo copian carpetas.

### 5.1 Un juego, una carpeta

- [ ] Opción en Organizar: guardar cada juego en su propia carpeta, junto a sus partidas.
  - Ahora: `ROMS/SFC/Tetris 2.zip`
  - Propuesta: `ROMS/SFC/Tetris 2/Tetris 2.zip` (y la partida `ROMS/SFC/Tetris 2/Tetris 2.srm` al lado).
- [ ] Comprobar en cada frontend antes de activarla: algunos muestran una carpeta como subcarpeta en vez de como juego (ES-DE la trata como juego solo si la carpeta se llama `Tetris 2.zip/` o con su configuración de directorios como juegos; el firmware de fábrica de Anbernic y muOS pueden mostrarla como carpeta). Debe ser una opción por perfil, no la regla general.
- [ ] Mover también el arte y `gamelist.xml` para que el frontend no pierda la portada.

### 5.2 Dónde guarda cada emulador

Primera lista, a completar con una investigación por emulador y plataforma (escritorio, Android, firmwares de portátiles). Fuente inicial: [Joey's Retro Handhelds](https://joeysretrohandhelds.com/guides/where-to-find-save-files-for-every-emulator-and-custom-firmware/).

- **RetroArch**: carpeta `saves/` propia por defecto; con `savefiles_in_content_dir` la partida va junto a la ROM; con `sort_savefiles_by_content_enable` / por núcleo se crean subcarpetas. Cambiar la opción no mueve las partidas que ya existen.
- **melonDS** y **mGBA**: por defecto junto a la ROM (`.sav`).
- **PPSSPP**: `PSP/SAVEDATA/` (partidas) y `PSP/PPSSPP_STATE/` (estados).
- **DuckStation**: `memcards/` y `savestates/` en su carpeta de datos.
- **PCSX2 / AetherSX2 / NetherSX2**: `memcards/` y `sstates/` en su carpeta de datos.
- **Dolphin**: `GC/<región>/` para GameCube y `Wii/title/` para Wii, en sus datos de usuario.
- **Azahar (3DS)**: `sdmc/Nintendo 3DS/<id>/<id>/title/…`.
- Pendiente de investigar: Flycast, Redream, Mupen64Plus / ParaLLEl, Snes9x y bsnes independientes, Citra, Yuzu / Ryujinx / sus sucesores, Vita3K, RPCS3, Xemu, Cemu, Lime3DS, Drastic, y las rutas de cada firmware (Anbernic stock, muOS, Knulli, ROCKNIX, ArkOS, Batocera, Android).
- [ ] Guardar todo esto como datos (un manifiesto por emulador y plataforma, al estilo del de Ludusavi), no en el código, para poder ampliarlo sin publicar versión.

### 5.3 Rescatar y respaldar

- [ ] Buscar partidas junto a las ROMs y en las carpetas conocidas de cada emulador del perfil.
- [ ] Carpeta personalizada: el usuario elige la carpeta de partidas de un emulador (muchos la permiten cambiar); Coverly trae las partidas y recuerda la carpeta para sincronizarlas después.
- [ ] Asociar cada partida a su juego de la biblioteca (por nombre de la ROM, id del juego o serial; memorias de PS1/PS2 compartidas como caso aparte).
- [ ] Copia de seguridad versionada en la biblioteca: varias versiones por partida, con fecha y dispositivo de origen; restaurar una versión anterior.
- [ ] Distinguir partidas (`.srm`, `.sav`, memorias) de estados (`.state`, savestates): los estados dependen de la versión del emulador y se tratan aparte.

### 5.4 Sincronizar partidas

- [ ] Llevar y traer partidas con cada dispositivo (bloque 4), traduciendo la ruta y el nombre según el emulador de cada lado (p. ej. RetroArch `saves/` en la Anbernic ↔ junto a la ROM en la Ayaneo).
- [ ] Conflictos: si la partida cambió en los dos lados, quedarse con las dos versiones y preguntar; nunca sobrescribir sin copia.
- [ ] Convertir formatos cuando haga falta y se pueda (p. ej. extensiones `.srm` ↔ `.sav`, tamaños de memoria distintos según el emulador).
