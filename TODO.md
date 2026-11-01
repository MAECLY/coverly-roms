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
   2.1. [Actualizaciones, DLC, demos y revisiones dentro de su juego](#21-actualizaciones-dlc-demos-y-revisiones-dentro-de-su-juego)
3. [Biblioteca](#3-biblioteca)
4. [Sincronizar con dispositivos](#4-sincronizar-con-dispositivos)
5. [Partidas guardadas (saves)](#5-partidas-guardadas-saves)
6. [Organización y exportación para frontends](#6-organización-y-exportación-para-frontends)

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

- [x] PlayStation 2 (`ps2`). El disco ya se detecta (`BOOT2` en `SYSTEM.CNF`, volumen `PLAYSTATION` mayor que un CD) y hoy sale como `Unsupported("PlayStation 2")`. Formatos: ISO, CHD, CSO, ZSO. Redump.
- [x] 3DO (`3do`). El disco ya se detecta y hoy sale como `Unsupported("3DO")`. Formatos: ISO, CUE/BIN, CHD.
- [x] Atari Jaguar y Jaguar CD (`jaguar`, `jaguarcd`). Formatos: `.j64`, `.jag`, `.rom`, `.abs`, `.cof`, CUE/BIN y CDI para el CD.
- [x] ColecoVision (`colecovision`). Formatos: `.col`, `.rom`.
- [x] Intellivision (`intellivision`). Formatos: `.int`, `.bin`, `.rom`.
- [x] Vectrex (`vectrex`). Formatos: `.vec`, `.gam`, `.bin`.
- [x] Magnavox Odyssey² / Videopac (`odyssey2`). Formato: `.bin`.
- [x] Fairchild Channel F (`channelf`). Formatos: `.bin`, `.chf`.
- [x] Emerson Arcadia 2001 (`arcadia`). Formato: `.bin`.
- [x] Watara Supervision (`supervision`). Formatos: `.sv`, `.bin`.
- [x] Sega SG-1000 / SC-3000 (`sg1000`). Formatos: `.sg`, `.sc`.
- [x] Sega Pico (`pico`). Formato: `.md`.
- [x] PC Engine SuperGrafx (`supergrafx`). Formatos: `.sgx`, `.pce`.
- [x] NEC PC-FX (`pcfx`). Formatos: CUE/BIN, CHD.
- [x] Philips CD-i (`cdi`). Formatos: CHD, CUE/BIN.
- [x] Amiga CD32 y CDTV (`amigacd32`, `cdtv`). Formatos: CUE/BIN, ISO, CHD.
- [x] Nintendo 64DD (`n64dd`). Formato: `.ndd`.
- [x] Satellaview y Sufami Turbo (`satellaview`, `sufami`). Formatos: `.bs`, `.st`.
- [x] Nintendo Famicom/NES en variantes de catálogo: comprobar si hacen falta entradas aparte para Famicom (BS-X, Disk Writer) o basta con alias.
- [x] Tiger Game.com (`gamecom`). Formatos: `.tgc`, `.bin`.
- [x] Mega Duck / Cougar Boy (`megaduck`). Formato: `.bin`.
- [x] Epoch Super Cassette Vision (`scv`). Formatos: `.bin`, `.0`.
- [x] Ordenadores:
  - [x] ZX Spectrum (`zxspectrum`). Formatos: `.tzx`, `.tap`, `.z80`, `.sna`, `.dsk`, `.scl`, `.trd`.
  - [x] ZX81 (`zx81`). Formatos: `.p`, `.81`.
  - [x] Amstrad CPC (`amstradcpc`). Formatos: `.dsk`, `.cdt`, `.sna`.
  - [x] Sharp X68000 (`x68000`). Formatos: `.dim`, `.xdf`, `.hdm`, `.hdf`, `.2hd`, `.m3u`.
  - [x] Sharp X1 (`x1`). Formatos: `.2d`, `.2hd`, `.dx1`, `.tap`.
  - [x] NEC PC-98 (`pc98`). Formatos: `.d88`, `.fdi`, `.hdi`, `.nhd`.
  - [x] NEC PC-88 (`pc88`). Formatos: `.d88`, `.t88`.
  - [x] FM Towns (`fmtowns`). Formatos: CUE/BIN, CHD, `.d88`.
  - [x] MSX2 y MSX turboR, si los perfiles los separan de `msx`.
  - [x] Apple II (`apple2`). Formatos: `.dsk`, `.do`, `.po`, `.nib`, `.woz`, `.2mg`.
  - [x] BBC Micro (`bbcmicro`). Formatos: `.ssd`, `.dsd`, `.uef`.
  - [x] Commodore Plus/4, PET y C128 (`plus4`, `pet`, `c128`). Formatos: `.prg`, `.d64`, `.t64`, `.tap`, `.crt`.
  - [x] Atari XE / XEGS, si los perfiles los separan de `a800`.
  - [x] TI-99/4A (`ti99`). Formatos: `.rpk`, `.ctg`, `.bin`.
  - [x] Thomson MO/TO (`thomson`). Formatos: `.fd`, `.k7`, `.m7`, `.rom`.
  - [x] Dragon 32/64 y TRS-80 CoCo (`dragon`, `coco`). Formatos: `.cas`, `.dsk`, `.ccc`.
- [x] Consolas de fantasía y homebrew:
  - [x] TIC-80 (`tic80`). Formato: `.tic`.
  - [x] WASM-4 (`wasm4`). Formato: `.wasm`.
  - [x] Arduboy (`arduboy`). Formatos: `.hex`, `.arduboy`.
  - [x] Uzebox (`uzebox`). Formatos: `.uze`, `.hex`.
  - [x] LowRes NX (`lowresnx`). Formato: `.nx`.
  - [x] Vircon32 (`vircon32`). Formato: `.v32`.
- [x] Ports y motores: PrBoom/Doom (`.wad`), Quake (`.pak`), Cave Story (NXEngine), Wolfenstein 3D (ECWolf), Mr. Boom, Dinothawr, Lutro, Solarus, Flashback (REminiscence), Rick Dangerous (XRick). Necesitan reglas de «juego = carpeta» como ScummVM.
- [ ] Arcade adicional: Model 2, Model 3, Naomi 2, Triforce, Sega System 16/18/24/32, Taito Type X, Konami, Cave, CHD de MAME por juego. Hecho en el PR #32: Model 2, Model 3, Naomi 2, Triforce y CHD de MAME por juego; System 16/18/24/32 y Konami siguen en `mame`/`fbneo`. Pendiente de decisión: Taito Type X y Cave (`cave3rd`).

### Tanda 2: discos grandes y portátiles con formato propio

Hace falta un lector de su formato y una identificación propia.

- [x] Nintendo GameCube (`gamecube`). Formatos: ISO, GCM, CISO, GCZ, RVZ, WIA. Id de juego en la cabecera del disco (6 caracteres en el offset 0).
- [x] Nintendo Wii (`wii`). Formatos: ISO, WBFS, CISO, GCZ, RVZ, WIA, y WAD de WiiWare. Misma cabecera que GameCube con la magia Wii en 0x18.
- [x] Nintendo Wii U (`wiiu`). Formatos: WUD, WUX, WUA, RPX y las carpetas descifradas (`code/`, `content/`, `meta/`). Id en `meta/meta.xml`.
- [x] Nintendo 3DS (`3ds`). Formatos: 3DS, CCI, CIA, CXI, 3DSX, ZCCI, ZCIA. Product code en la cabecera NCSD/NCCH.
- [x] Nintendo Switch (`switch`). Formatos: XCI, NSP, NSZ, XCZ. Title ID en la cabecera; los NSP/XCI van cifrados y solo se puede leer lo que no lo está.
- [x] Sony PS Vita (`psvita`). Formatos: VPK, carpetas `app/` descifradas, `.psvimg`. Title ID en `sce_sys/param.sfo`.
- [x] Sony PS3 (`ps3`). Formatos: ISO, carpetas JB (`PS3_GAME/`), PKG. Title ID en `PS3_GAME/PARAM.SFO`.
- [x] Microsoft Xbox (`xbox`). Formatos: ISO XISO, carpetas con `default.xbe`. Title ID en la cabecera XBE.
- [x] Microsoft Xbox 360 (`xbox360`). Formatos: ISO, GOD, XEX, carpetas XBLA. Title ID en la cabecera XEX.
- [x] Nintendo DSi y DSiWare, si los perfiles los separan de `nds`.

### Tanda 3: generación actual (PS4, PS5, Xbox One, Xbox Series)

Revisado el 2026-10-31, comprobando cada fuente (repos oficiales, código, listas de compatibilidad). PS4 y PS5 ya tienen emuladores reales y activos, así que se tratan **como el resto de formatos**: identificación, carpeta en los perfiles que los usen, portadas, duplicados, Organizar y, más adelante, sincronizar. Xbox One está en una fase temprana y Xbox Series no tiene ningún emulador público.

**Estado de la emulación (2026-10-31):**

| Sistema         | Proyectos reales                                                                                                                                                                                                                                                                                                                                                                                                     | Madurez                                                                                                                                                               | Qué cargan                                                                                                                                                                                      |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| PS4             | [shadPS4](https://github.com/shadps4-emu/shadPS4) (GPL-2.0, Windows/Linux/macOS 26+, v0.21.0 del 2026-10-30); [fpPS4](https://github.com/red-prig/fpPS4) (reescribiendo el núcleo); Kyty (parado desde 2024); Obliteration (aún no ejecuta juegos)                                                                                                                                                                   | shadPS4: 415 jugables, 616 en juego, 339 en menús, 354 arrancan, 595 nada ([lista](https://github.com/shadps4-compatibility/shadps4-game-compatibility))              | Carpetas descifradas `CUSAxxxxx` (con `-patch`/`-UPDATE` para actualizaciones). Ya **no** carga PKG/fPKG (quitado en v0.8.0)                                                                    |
| PS5             | [KytyPS5](https://github.com/KytyPS5/KytyPS5) (GPL-2.0, Windows/Linux, macOS experimental); [SharpEmu](https://github.com/sharpemu/sharpemu) (GPL-2.0, C#, Windows/Linux; [puerto a Mac](https://github.com/sharpemu/sharpemu-mac)); [Prosperity](https://github.com/Force67/prosperity) (GPL-2.0, LLE PS4+PS5, Linux y Android); [AnyPS5](https://github.com/boykopovar/AnyPS5) (GPL-2.0, reenlazador, no emulador) | KytyPS5: 368 títulos probados, 199 en juego ([lista](https://kytyps5.github.io/)). SharpEmu: 103 probados, 45 jugables ([lista](https://sharpemu.app/compatibility/)) | Carpetas descifradas `PPSAxxxxx` con `eboot.bin` y `sce_sys/param.json`; además `.zar` (SharpEmu), `.ffpkg` y ZIP/RAR de la app (Prosperity). Nadie carga el `.pkg` de tienda ni imágenes exFAT |
| Xbox One        | [WinDurango](https://github.com/WinDurango/WinDurango) (LGPL-3.0, capa de compatibilidad para Windows); XWine1 (anunciado, sin binario ni código del núcleo)                                                                                                                                                                                                                                                         | WinDurango: 3 jugables, 4 arrancan                                                                                                                                    | Carpeta ERA montada con `AppxManifest.xml`; no carga XVC cifrados                                                                                                                               |
| Xbox Series X/S | Ninguno público                                                                                                                                                                                                                                                                                                                                                                                                      | —                                                                                                                                                                     | —                                                                                                                                                                                               |

**Estafas y malware (importante para la seguridad de la app y de la landing):** el 2026-10-28 NordVPN informó de repos de GitHub falsos que imitan a KytyPS5 y AnyPS5 y reparten un ladrón de contraseñas ([Engadget](https://www.engadget.com/2299912/fake-ps5-emulators-on-github-are-spreading-password-stealing-malware/)). PCSX4/PCSX5, «PS5 emulator APK», «Rexbox» y las webs de «Xbox One/Series emulator» con encuestas o pagos son estafas conocidas o no verificables. Coverly nunca enlaza, recomienda ni descarga emuladores; si alguna vez se nombran, solo con los repos oficiales de la tabla.

Tareas:

- [ ] **Sony PS4 (`ps4`).** Dos formatos:
  - Carpeta `CUSAxxxxx` descifrada: leer `sce_sys/param.sfo` (PSF, texto plano, sin claves: cabecera `\0PSF`, campo `fmt` en big-endian y el resto en little-endian; claves `TITLE_ID`, `TITLE`, `CONTENT_ID`, `CATEGORY`, `APP_VER`). Portada incrustada: `sce_sys/icon0.png`; fondo: `pic1.png`. El nombre de la carpeta suele ser el title ID (respaldo).
  - `.pkg`/fPKG: magia `0x7F434E54`; content ID en `0x40` (title ID en `0x47`); en la tabla de entradas (offset en `0x18`, número en `0x10`) la entrada `0x1000` es el `param.sfo` en claro y la `0x1200` el `icon0.png`. Se puede catalogar sin descifrar, pero shadPS4 no lo arranca: avisar «hay que extraerlo» en Organizar.
  - Región por el prefijo del content ID (`UP` América, `EP` Europa, `JP` Japón, `HP` Asia); el código `CUSA` no basta porque se comparte entre regiones. Confirmar la tabla de prefijos con una fuente primaria antes de usarla.
  - Actualizaciones `CUSAxxxxx-patch`/`-UPDATE` y DLC: agruparlas con su juego, no como juegos aparte (y no contarlas como duplicados).
- [ ] **Sony PS5 (`ps5`).** Primer nivel, igual que PS4:
  - Carpeta `PPSAxxxxx` con `eboot.bin` y `sce_sys/param.json` (JSON, no `param.sfo`): leer `titleId`, `contentId`, `contentVersion` (o `appVersion`) y `localizedParameters.<idioma>.titleName`. Portada incrustada `sce_sys/icon0.png`, fondo `pic0.png`.
  - Contenedores aceptados: `.zar` (SharpEmu), `.ffpkg` y ZIP/RAR de la app (Prosperity), leídos con el lector de comprimidos actual cuando se pueda.
  - `.pkg` de tienda e imágenes exFAT: reconocerlos y dejarlos con la nota «ningún emulador lo carga así», nunca como jugables.
  - Detectar PS5 Pro por `ATTRIBUTE3` como etiqueta informativa.
- [ ] **Microsoft Xbox One (`xboxone`).** Temprano pero real:
  - Carpeta ERA con `AppxManifest.xml` (formato de WinDurango): `Identity Name/Publisher/Version` y `Properties/DisplayName` (puede ser `ms-resource:` resuelto con `resources.pri`). No confiar en que lleve el TitleId.
  - Carpeta GDK con `MicrosoftGame.config` y `Executable/@TargetDeviceFamily="XboxOne"`: `TitleId` (8 hex; `FFFFFFFF` es un marcador), `MSAAppId`, `StoreId`, `ShellVisuals/DefaultDisplayName`.
  - Contenedor XVC/MSIXVC: magia `msft-xvd` en `0x200`, ContentId en `0x220`, ContentType en `0x284`, ProductId en `0x39C`, PackageVersion en `0x3BC` (referencia: [xvdtool](https://github.com/emoose/xvdtool)). Solo da GUIDs, sin nombre ni TitleId, y el contenido va cifrado: catalogar por GUID y pedir el nombre a una fuente externa.
- [ ] **Microsoft Xbox Series X/S (`xboxseries`).** Detectar y catalogar, marcado como «sin emulador»:
  - Solo cuando `Executable/@TargetDeviceFamily="Scarlett"` (o `TargetDeviceFamilyForDLC="Scarlett"` para DLC). **No** usar `XboxCompatibility="Scarlett"`: eso es un juego de Xbox One con mejoras en Series, y sigue siendo Xbox One.
  - `TargetDeviceFamily="PC"` es una build de PC (GDK/MSIXVC), no de consola: no meterla en ningún sistema de consola.
- [ ] **Perfiles:** investigar qué frontends tienen carpeta para estos sistemas (ES-DE, Batocera, RetroBat, LaunchBox, Pegasus, los lanzadores de shadPS4/KytyPS5/SharpEmu) y dónde esperan las carpetas `CUSA*`/`PPSA*`. Los juegos aquí son **carpetas**, no archivos: usar las reglas de «juego = carpeta» como ScummVM.
- [ ] **Portadas:** libretro-thumbnails no tiene estos sistemas. Usar la portada incrustada (`icon0.png`/`pic0.png`/`pic1.png`) como primera opción y como respaldo SteamGridDB, ScreenScraper (si los cubre) o una base pública de title IDs.
- [ ] **Duplicados:** clave de contenido por title ID + versión (`APP_VER`/`contentVersion`), y las actualizaciones nunca como copias del juego base.
- [ ] **Tamaño:** estas carpetas pesan decenas o cientos de GB. Escanear sin leer el contenido entero (solo `sce_sys/` y cabeceras), avisar del espacio en Organizar y no proponer extraer ni copiar sin confirmar el tamaño.

### Decisiones abiertas

- [ ] Nombres de las carpetas por perfil (Anbernic stock, muOS, Knulli, Batocera, ES-DE, RetroArch) para cada sistema nuevo. Investigarlo antes de cada tanda.
- [ ] Qué sistemas aparecen por defecto y cuáles solo si el perfil los usa.
- [ ] Licencias de los iconos que no estén en RetroArch Assets.

### Claves aportadas por el usuario (aparcado)

Decidido con Miguel el 2026-11-01: por ahora Coverly identifica los juegos **solo sin claves**. Lee lo que está en claro (3DS descifrado y sección Meta de los CIA; en Switch, el title ID sacado del ticket, del nombre del archivo o del id del NCA) y busca el nombre en `blawar/titledb` (MIT) por title ID. Lo que no se resuelve queda marcado como «cifrado».

El modo «usa tus propias claves» (`prod.keys`, `keys.txt`) queda como posible extra para más adelante, con estas condiciones:

- Desactivado por defecto; el usuario elige el archivo.
- Las claves solo se usan en memoria para leer el nombre y el icono.
- Nunca se incluyen, descargan, enlazan, copian ni registran.
- Nunca se ofrece descifrar ni exportar un juego.
- Antes de hacerlo, una revisión legal.

Contexto: el caso Nintendo contra Yuzu (2024), el bloqueo de Dolphin en Steam (2023) y que Azahar ya no carga juegos cifrados desde la versión 2120.

## 2.1. Actualizaciones, DLC, demos y revisiones dentro de su juego

> **Dependencias:** depende de 2 (la detección de base, actualización, DLC y demo que traen las tandas). **Lo usan 3, 4 y 5**, así que se hace antes que ellos o como primer paso del 3: la biblioteca guarda los juegos con esta estructura (3), la sincronización la compara y la traduce a cada emulador (4) y las partidas viven en la misma carpeta y se asocian por el mismo title ID (5, incluido 5.3.1).

No entra en el loop de las tandas: se hace después, cuando Miguel lo diga.

### Qué hay hoy (Tanda 2, `feat/systems-batch-2`)

- Se distingue base / actualización / DLC / demo por el title ID en claro (3DS por la parte alta, `0004000E` actualización; Switch por los bits bajos, `…800` actualización y DLC = base + `0x1000`; Wii U igual que 3DS; PS3/Vita por `CATEGORY`; Xbox 360 por el title ID). Tipo `TitleKind { Base, Update, Dlc, Demo, Unknown }` en `crates/coverly-core/src/consoles/mod.rs`.
- Se enlaza cada actualización o DLC con su juego base cuando los dos están en la pila, y Organizar los lista debajo del base con la nota «Actualización de…» / «Contenido descargable de…» y un recuento en el resumen.
- Duplicados: una actualización nunca es copia de su base; de dos versiones del mismo juego se queda la mayor.
- **Falta:** se mueven a la carpeta del sistema junto al juego (`3DS/Pokemon.cia`, `3DS/Pokemon update 1.cia`), no dentro de una carpeta del juego; las revisiones retro (`(Rev 1)`, `(v1.1)`) no se agrupan como versiones de un juego; las Tandas 1 y 3 no lo aplican aún (la 3 sí para PS4/PS5).

### Estructura maestra de la biblioteca (decidida con Miguel, 2026-11-01)

```
ROMS/PS4/Hatsune Miku Project DIVA Future Tone/
    Hatsune Miku Project DIVA Future Tone.pkg          ← juego base
    DLC/
        Hatsune Miku Project DIVA Future Tone - Song Pack 1.pkg
    Updates/
        Hatsune Miku Project DIVA Future Tone (v1.05).pkg
    Mods/          ← mods, packs de texturas, parches de traducción
    Cheats/        ← trucos y parches del emulador
    Extras/        ← banda sonora, manuales, arte, vídeos, temas… (semiautomática, ver abajo)
    Saves/         ← partidas (bloque 5)
ROMS/PS4/Hatsune Miku Project DIVA Future Tone (Demo)/  ← la demo es un juego aparte
```

Reglas:

- **Una carpeta por juego** con el juego base en su raíz, y dentro `DLC/` y `Updates/` (nombres fijos en inglés para que emuladores y herramientas los reconozcan; la interfaz los muestra traducidos).
- **Demos aparte**, como juego independiente, con el sufijo `(Demo)` de No-Intro/Redump (no `DEMO`) para que se ordenen junto al juego; relacionadas con el base en la ficha, pero no dentro de su carpeta.
- **Versión en el nombre** de cada actualización (`… (v1.05)`), sacada de la cabecera, para saber cuál es la más nueva.
- **Regiones:** si hay el mismo juego en varias regiones, una carpeta por región con la región en el nombre (`… (USA)`, `… (Japan)`); cada actualización y DLC va con la región de su title ID.
- **Revisiones retro** (`(Rev 1)`, `(v1.1)`) como versiones del mismo juego en la ficha (no como juegos distintos); en disco siguen siendo archivos sueltos salvo que el perfil pida carpetas.
- La misma carpeta del juego es la que usará `Saves/` del bloque 5.

### Carpetas fijas y `Extras/` libre (decidido con Miguel, 2026-11-01)

- **Carpetas fijas del juego:** `Updates/`, `DLC/`, `Mods/`, `Cheats/`, `Extras/` y `Saves/`. Siempre con ese nombre y en ese sitio, porque de ellas dependen la organización automática, la traducción a cada emulador (4) y las partidas (5). El usuario no crea carpetas nuevas a ese nivel; la interfaz las muestra traducidas.
- **`Mods/`:** mods y packs de texturas (p. ej. carpeta `-mods` de shadPS4, `atmosphere/contents/<id>/romfs` de Switch, LayeredFS de Azahar, Riivolution de Dolphin, texturas HD de Dolphin/PCSX2) y parches de traducción o hacks (`.ips`, `.bps`, `.ups`, `.xdelta`, `.ppf`), hoy detectados como «no es una ROM (parche)» pero sin asociar a su juego.
- **`Cheats/`:** trucos (`.cht`, Action Replay, Game Genie, `pnach`) y parches del emulador (`patches.yml` de RPCS3, parches de PCSX2 y Dolphin).
- **Licencias** (`.rap`, zRIF, `work.bin`, DLC de solo licencia `AL`): se reconocen y se dejan junto al archivo al que pertenecen, sin leerlas nunca.
- **No son juegos:** actualizaciones de sistema y firmware (particiones de actualización de cartuchos 3DS/Wii, `PS3UPDAT.PUP`, `0004001B…`), cachés de shaders y configuraciones por juego del emulador: se reconocen y se dejan donde están.
- **Variantes relacionadas** (como las demos): `(Proto)`, `(Beta)`, `(Kiosk)`, `(Sample)`, ediciones (GOTY, Complete, remasterizaciones), recopilatorios con varios juegos e inyecciones de Virtual Console: juegos aparte enlazados al original en la ficha.

**`Extras/`, semiautomática:**

```
Juego/Extras/
    Soundtrack/   ← oficial detectado (p. ej. el .cia de banda sonora en 3DS) + canciones del usuario
    Manuals/      ← manual electrónico oficial + escaneos
    Artwork/      ← arte oficial, escaneos de caja y cartucho, fan art
    Videos/       ← tráileres, partidas grabadas
    Themes/       ← temas y avatares oficiales (PS3/Vita/PS4/3DS)
    Guides/       ← guías, mapas, PDF
    (del usuario) ← categorías que el usuario crea desde la app
```

- **Lo oficial se detecta y se coloca solo** (banda sonora, manual, temas y avatares distribuidos como paquete del sistema).
- **Lo del usuario se gestiona desde la app (CRUD):** añadir, renombrar, mover y borrar con deshacer, arrastrar y soltar en la ficha del juego, crear categorías propias dentro de `Extras/` y etiquetar cada archivo («tema del jefe final», «versión orquestal»).
- **`extras.json` junto a las carpetas:** tipo (audio, vídeo, imagen, documento), oficial o del usuario, origen y etiquetas, para buscar y filtrar aunque se muevan archivos; la app reproduce audio y vídeo y muestra imágenes y PDF.
- **Sincronizar:** `Extras/` no se copia al dispositivo por defecto (casi ningún frontend la usa y ocupa espacio); se puede activar por categoría (p. ej. solo `Manuals/`).

Tareas:

- [ ] Crear y respetar las carpetas fijas en Organizar y en la biblioteca, con deshacer.
- [ ] Detectar y colocar mods, parches de traducción, trucos, licencias y extras oficiales por sistema (según la investigación de abajo).
- [ ] CRUD de `Extras/` en la ficha del juego: categorías sugeridas y propias, etiquetas, arrastrar y soltar, deshacer.
- [ ] `extras.json` con tipo, origen y etiquetas; reproductor de audio y vídeo y visor de imágenes y PDF en la app.
- [ ] Traducir `Mods/` y `Cheats/` a la ruta de cada emulador al organizar o sincronizar.

### Investigación de mods, trucos, licencias y extras por sistema

Hecha el 2026-11-01 (4 investigadores y un verificador crítico por grupo, 30 sistemas). Detalle completo con rutas, formatos, claves de enlace y fuentes: [`docs/02-research/mods-cheats-extras-per-system.md`](docs/02-research/mods-cheats-extras-per-system.md). Lo más útil para `Mods/`, `Cheats/` y `Extras/`:

- **Parches de traducción y hacks** (retro): el mejor para enlazar automáticamente es **BPS** (los últimos 12 bytes guardan el CRC32 de la ROM de origen, que se compara con el DAT de No-Intro); UPS también lleva CRC32. IPS no lleva nada: se enlaza por nombre de archivo y por el `readme`. RetroArch aplica solo el parche que se llama como la ROM (`rom.bps`, `rom.ips`, `rom.ips1`…). Ojo con las cabeceras: NES (iNES) y SNES (copier) cambian si el parche espera la ROM con o sin cabecera.
- **Clave de enlace por emulador** (lo que hay que leer para saber de qué juego es un mod o un truco):
  - **por serial del disco:** DuckStation (PS1) y PCSX2 (PS2) para texturas (`textures/<SERIAL>/replacements/`) y trucos (`.cht`, `SERIAL_CRC.pnach`);
  - **por id de juego:** Dolphin (GameCube/Wii; `Load/Textures/<GAMEID>/`, `GameSettings/<GAMEID>.ini`, Riivolution por `<id game>`), Flycast (Dreamcast), PPSSPP (`PSP/TEXTURES/<GAMEID>/`, `PSP/Cheats/<GAMEID>.ini`);
  - **por title ID:** Azahar (3DS; `load/mods/<TitleID>/`, trucos `<TitleID>.txt`), Cemu (Wii U; graphic packs con `titleIds` en `rules.txt`), Switch (`atmosphere/contents/<id>/romfs|exefs`; trucos `cheats/<build_id>.txt`), RPCS3 (`patches.yml`), Vita (rePatch `ux0:rePatch/<TITLEID>/`), shadPS4 (trucos `SERIAL_VERSION.json`), Xenia (`<TITLEID> - <Juego>.patch.toml`, plugins);
  - **por nombre de archivo:** RetroArch (`.cht` con el nombre de No-Intro), Mesen HD Packs (`HdPacks/<nombre de la ROM>/hires.txt`), MSU-1 de SNES (`.msu` + `-N.pcm` con el nombre de la ROM).
- **Extras oficiales:** solo existen en algunos sistemas: temas `.p3t` y PKG de tema/avatar en PS3, PKG de tema (`0x1F`) en Vita, temas `.PTF` en PSP, paquetes STFS de Xbox 360 (temas, imágenes de jugador, avatares; clasificables por su tipo de contenido sin claves). Los manuales electrónicos van **dentro** del juego en 3DS, Wii U, Switch y Vita (se muestran como parte del juego, no se separan). Cartucho y disco retro no tienen extras oficiales: ahí `Extras/` es del usuario, con sugerencias por las etiquetas de los rips de música (SPC ID666, NSF, GBS, VGM GD3, PSF).
- **Licencias y claves, solo por nombre y nunca abiertas:** `.rap`/`.rif`/`.edat`/`act.dat` (PS3/PSP), zRIF/`work.bin`/`.rif` (Vita), `.tik`/`ticket.db`/`seeddb.bin` (3DS), `title.tik`/`keys.txt`/`otp.bin` (Wii U), `prod.keys`/`title.keys`/`*.tik` (Switch), BIOS y NAND de DS/DSi, `eeprom.bin` de Xbox.
- **No son juegos:** BIOS y firmware de cada sistema, partidas y estados, opciones por juego de RetroArch (`.opt`, `.cfg`, `.rmp`, presets de shaders), cachés de texturas y shaders, VMU de Dreamcast (partidas o DLC, no juegos).
- **Sin sistema de mods** (se modifican los archivos del juego o vienen como imagen ya parcheada): Saturn, Sega CD, PC Engine CD, Xbox, PS3, PS4 (salvo carpetas tipo `dvdroot_ps4/MODS`), PS5 y Xbox One; ahí `Mods/` guarda copias del usuario enlazadas por serial.

### Traducción por dispositivo (al organizar para un frontend o al sincronizar)

La estructura maestra es la de la biblioteca. Cada perfil la traduce a lo que espera su emulador:

- **shadPS4 (PS4):** no carga `.pkg`, solo carpetas extraídas `CUSAxxxxx`, y la actualización va **al lado**: `CUSAxxxxx-UPDATE` (o `-patch`), no dentro.
- **RPCS3 (PS3), Azahar (3DS), Cemu (Wii U), Vita3K (Vita):** actualizaciones y DLC se **instalan** en el emulador (NAND / su carpeta de datos). Coverly los guarda en la biblioteca y muestra «Instálalo en el emulador» (nota `InstallFirst`, ya existe).
- **Emuladores de Switch:** cargan el base y aceptan actualizaciones/DLC desde una carpeta aparte; la estructura maestra sirve casi tal cual.
- **ES-DE y otros frontends:** una carpeta con subcarpetas puede verse como carpeta y no como juego; usar su modo «directorio como juego» (o la carpeta con el nombre del archivo, p. ej. `Juego.pkg/`) según el perfil.

### Portadas y arte de actualizaciones, DLC y demos

Muchas actualizaciones, DLC y demos traen su propia portada o arte, distinto del juego base (el icono de un pack de canciones, la portada de una demo, el banner de una expansión).

- **Demos:** son juegos aparte, con su propia portada y arte. Se buscan como cualquier juego (libretro-thumbnails y ScreenScraper tienen entradas `(Demo)`), y si no hay, se usa el icono incluido o, como último recurso, la del juego completo marcada como «del juego completo».
- **DLC:** usar el arte incluido en el propio paquete cuando está en claro (por ejemplo `icon0.png` de un `addcont` de PS4/PS5/Vita, `ICON0.PNG` de un PKG de PS3, el SMDH de un CIA de 3DS si lo trae; en Switch el icono va dentro de un NCA cifrado y no se lee). Si no trae, buscar por nombre en ScreenScraper o SteamGridDB, y si tampoco, mostrar la del base con la marca DLC.
- **Actualizaciones:** casi siempre comparten la portada del base; si traen un icono propio (algunas cambian el icono o el banner), guardarlo como arte de esa versión sin pisar el del juego base.
- **Dónde se guarda:** en la carpeta del juego, junto a cada archivo (`DLC/…`, `Updates/…`), con el mismo esquema de slots que el resto (portada, título, captura, banner) y su origen anotado («incluido en el paquete», «ScreenScraper»…).
- **Frontends:** al organizar para un dispositivo, solo se copia el arte que su frontend sabe mostrar (la mayoría solo muestra el del juego base); el resto queda en la biblioteca.
- **Interfaz:** en la ficha del juego, cada actualización y DLC con su miniatura; la demo con su propia portada y un enlace al juego completo.

- [ ] Leer el arte incluido en actualizaciones, DLC y demos (sin claves, solo lo que va en claro) y buscar el que falte en las fuentes existentes.
- [ ] Slots de arte por archivo hijo (actualización, DLC) además del del juego, con el origen anotado.
- [ ] Mostrarlo en la ficha y en Organizar, y copiarlo al dispositivo solo cuando su frontend lo use.

### Cómo se ve en las seis vistas (pendiente de diseñar)

> **Nota:** todavía no está decidido cómo mostrar en cada vista que un juego tiene varias actualizaciones, DLC, una demo o varias revisiones. Antes de implementarlo:
>
> 1. **Investigar cómo lo resuelve la industria hoy**, con capturas y notas de cada una: Steam (DLC y «Gestionar» en la biblioteca), PlayStation 5 (centro del juego, complementos y versión), Xbox (Gestionar juego y complementos), Nintendo Switch (Gestionar software, versión y contenido adicional), GOG Galaxy, Epic, Playnite, LaunchBox / Big Box, ES-DE, RomM, Ryujinx/Citra/RPCS3 (cómo listan actualizaciones y DLC instalados) y gestores de música o fotos que agrupan versiones (Apple Music, Lightroom).
> 2. **Diseñarlo con el skill `frontend-design:frontend-design` con mucho peso en UX y UI**: plan de jerarquía, densidad y movimiento revisado contra los diseños genéricos, y validado con capturas en las seis vistas, en claro y oscuro, a 1440 y 390 px.
>
> Debe resolver, como mínimo:
>
> - **Portadas y Galería:** un indicador discreto en la portada (p. ej. un recuento «+3 DLC · v1.05») que no tape el arte, y el detalle al abrir el juego.
> - **Detalles (tabla):** columnas de versión y de complementos; la fila del juego que se despliega para listar actualizaciones y DLC (como un árbol), sin que cuenten como juegos en los totales.
> - **Columnas:** sistema → juego → (base / Updates / DLC / Demo) → vista previa, aprovechando la carpeta maestra de 2.1.
> - **Huecos de arte:** mostrar también los huecos de arte de cada DLC y actualización cuando tienen arte propio.
> - **Estantes:** una caja por juego, con los DLC como «complementos» al lado o detrás, sin llenar la estantería de cajas sueltas.
> - **Ficha del juego (panel lateral):** la versión instalada frente a la más nueva que hay en la biblioteca, la lista de DLC con su miniatura, la demo relacionada, las revisiones y las partidas (5).
> - **Filtros:** «tiene actualizaciones», «le falta la última actualización», «tiene DLC», «solo demos».
> - **Accesibilidad:** el recuento y el estado también en texto para lectores de pantalla, nunca solo con color o icono; navegación con teclado dentro del árbol.

### Investigación por sistema

Hecha el 2026-11-01 (4 investigadores y un verificador crítico por grupo, que abrió cada fuente). Detalle completo, con offsets, máscaras de id, capas de cada emulador, convenciones de nombre y fuentes: [`docs/02-research/updates-dlc-per-system.md`](docs/02-research/updates-dlc-per-system.md). Resumen:

| Sistema                  | Cómo se distingue (sin claves)                                                                                                                                                  | Cómo se llega al base                                                                                          | Qué espera el emulador                                                                                                                |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| 3DS                      | Parte alta del title ID: `00040000` base, `00040002` demo, `0004000E` actualización, `0004008C` DLC (cabecera del CIA y TMD en claro)                                           | Mismos 32 bits bajos: `0x0004000000000000 \| (id & 0xFFFFFFFF)`; la demo tiene id propio, se enlaza por nombre | Azahar: el base se carga o instala; actualización y DLC se **instalan** (Install CIA)                                                 |
| DS / DSi                 | No hay actualizaciones ni DLC; DSiWare es independiente                                                                                                                         | —                                                                                                              | —                                                                                                                                     |
| Switch                   | Bits bajos del title ID: `…000` base, `…800` actualización, DLC = base + `0x1000` + n (nombre del ticket y del `cnmt`, en claro)                                                | `id & ~0x1FFF`                                                                                                 | El base se carga; actualizaciones y DLC se añaden desde una carpeta                                                                   |
| Wii U                    | Parte alta: `00050000` base, `0005000E` actualización, `0005000C` DLC, `00050002` demo (`meta.xml`/`app.xml`)                                                                   | Mismos 32 bits bajos                                                                                           | Cemu: actualización y DLC se **instalan** en su `mlc`                                                                                 |
| Wii / GameCube           | Disco: id de juego y revisión en la cabecera; WAD: id del canal; DLC de WiiWare aparte                                                                                          | Id de juego del disco                                                                                          | Dolphin: discos tal cual; WAD se instalan en su NAND                                                                                  |
| PS3                      | `CATEGORY` del `PARAM.SFO` y tipo del PKG: `DG`/`HG` base, `GD` actualización, `AC` DLC (cabecera PKG en claro)                                                                 | Mismo title ID (`BLUS30001`)                                                                                   | RPCS3: PKG se **instalan**; licencias `.rap` fuera de alcance                                                                         |
| PS Vita                  | `CATEGORY`: `gd` base, `gp` actualización, `ac` DLC (carpetas `app/`, `patch/`, `addcont/`)                                                                                     | Mismo title ID                                                                                                 | Vita3K: se **instalan**                                                                                                               |
| PSP                      | `CATEGORY` `UG` disco, `PG` actualización (`PBOOT.PBP`)                                                                                                                         | `DISC_ID`                                                                                                      | PPSSPP: la actualización va en `PSP/GAME/<id>/`                                                                                       |
| PS4                      | Tipo de contenido del PKG y `CATEGORY`: base, `gp` parche, `ac` DLC; carpetas `CUSAxxxxx`, `-patch`/`-UPDATE`, `addcont`                                                        | Mismo `CUSAxxxxx`                                                                                              | shadPS4: carpetas extraídas; actualización **al lado** (`CUSAxxxxx-UPDATE`, o `-patch` si no hay `-UPDATE`); DLC en `addcont/<CUSA>/` |
| PS5                      | `param.json` en claro en las carpetas: un parche lleva `originContentVersion`/`targetContentVersion`; el DLC se reconoce por estar en `addcont/<PPSA>/<label>/` sin `eboot.bin` | Mismo `PPSAxxxxx`                                                                                              | KytyPS5/SharpEmu: carpeta descifrada; el `param.json` de los paquetes va cifrado, solo el de las carpetas está en claro               |
| Xbox 360                 | Tipo de contenido STFS: `0x000B0000` actualización, `0x00000002` DLC; title ID y media ID en claro                                                                              | Mismo title ID                                                                                                 | Consola y Xenia: `Content/0000000000000000/<TitleID>/000B0000/` (actualización) y `…/00000002/` (DLC)                                 |
| Xbox One / Series        | Tipo de contenido XVD y `MicrosoftGame.config`                                                                                                                                  | Identidad del paquete                                                                                          | Sin emulador jugable (WinDurango temprano)                                                                                            |
| Retro (cartucho y disco) | Revisiones `(Rev 1)`, `(v1.1)` en No-Intro/Redump y el byte de versión de la cabecera                                                                                           | Mismo juego, otra versión                                                                                      | Se juegan sueltas; agruparlas solo en la ficha                                                                                        |

### Tareas

- [ ] Opción de perfil «carpeta por juego con `DLC/` y `Updates/`» para la biblioteca y los perfiles que la acepten.
- [ ] Organizar: mover base, actualizaciones y DLC a la carpeta del juego con deshacer; demos a su carpeta `(Demo)`.
- [ ] Nombres con versión y región sacadas de las cabeceras.
- [ ] Traductores por emulador (shadPS4, RPCS3, Azahar, Cemu, Vita3K, Switch, ES-DE) usados por Organizar con perfil y por la sincronización del bloque 4.
- [ ] Extender la detección a todos los sistemas con actualizaciones o DLC (Tandas 1–3 y PSP, Wii/WiiWare…).
- [ ] Revisiones retro agrupadas como versiones de un juego en la biblioteca y en duplicados.
- [ ] Interfaz: el juego con sus actualizaciones y DLC plegados debajo, versión instalada y la más nueva disponible.

## 3. Biblioteca

> **Dependencias:** depende de 2.1 (el juego como entidad con sus actualizaciones, DLC, demo, versiones y revisiones, y la estructura de carpetas maestra). **Lo usan 4 y 5.**

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

> **Dependencias:** depende de 3 (la biblioteca que se compara) y de 2.1 (saber que una actualización o DLC pertenece a un juego, las carpetas fijas del juego —cuyo contenido el usuario elige sincronizar— y los traductores por emulador). Se coordina con 5.4 (sincronizar partidas).

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

### Qué se lleva a cada dispositivo (elección del usuario)

Cada juego de la biblioteca tiene sus carpetas fijas (2.1): el juego base, `Updates/`, `DLC/`, `Mods/`, `Cheats/`, `Extras/` (y sus categorías) y `Saves/`. El usuario decide qué se sincroniza, con tres niveles que se heredan de arriba abajo:

1. **Por dispositivo (valor por defecto):** p. ej. en la Anbernic RG SP «base + Updates + DLC + Saves», sin `Mods/` ni `Extras/`; en la Ayaneo «todo menos `Extras/Videos`».
2. **Por sistema:** sobrescribe el del dispositivo (p. ej. en PS2 llevar también `Cheats/`).
3. **Por juego:** sobrescribe los anteriores (p. ej. este juego con su pack de texturas de `Mods/` y su manual de `Extras/Manuals`, aquel sin DLC).

- [ ] Matriz de selección en la interfaz: casillas por carpeta (y por categoría de `Extras/`), con el estado heredado visible («como el dispositivo», «como el sistema», «personalizado») y un botón para volver al heredado.
- [ ] Selección por elementos sueltos dentro de una carpeta (este DLC sí, aquel no; esta actualización concreta, no todas).
- [ ] Ver el espacio que ocupa cada elección antes de sincronizar y avisar si no cabe.
- [ ] Respetar lo que el emulador del dispositivo puede usar: si su frontend no admite mods o extras, la opción aparece pero se explica que no se usará, en vez de copiar sin sentido.
- [ ] Guardar la elección con el dispositivo (por su identidad, no por la ruta) para que se mantenga al volver a conectarlo.
- [ ] Plantillas de selección reutilizables («mínimo: solo base y partidas», «completo», «para portátil con poca SD»).

### Comparar y sincronizar

- [ ] Comparación juego a juego (por identidad de contenido, no por nombre): **en ambos**, **solo en la biblioteca**, **solo en el dispositivo**, **distinta versión** (región, revisión, formato: p. ej. CHD en la biblioteca y CUE/BIN en el dispositivo).
- [ ] Vista de diferencias: reutilizar las vistas actuales (Detalles, Columnas) con un filtro por estado de sincronización, o una pantalla nueva de dos columnas «Biblioteca ↔ Dispositivo». Investigar UX de herramientas de sincronización (FreeFileSync, Syncthing, Beyond Compare, la de iTunes / Finder con iPhone) antes de decidir.
- [ ] Selección de qué llevar: por colección («lo que quiero en la Anbernic»), por sistema o juego a juego. Avisar del espacio libre.
- [ ] Llevar al dispositivo con su formato: carpetas del perfil, arte en el sitio del frontend, `gamelist.xml` o equivalentes, y conversión si el dispositivo no acepta el formato (p. ej. extraer un 7z).
- [ ] Traer del dispositivo a la biblioteca los juegos que solo están allí (importación del bloque 3).
- [ ] Dirección y conflictos: biblioteca → dispositivo, dispositivo → biblioteca, o en los dos sentidos; nunca borrar en el destino sin confirmación; todo con deshacer y registro, como los trabajos actuales.
- [ ] Sincronizar varios dispositivos con la misma biblioteca y ver de un vistazo qué tiene cada uno.

## 5. Partidas guardadas (saves)

> **Dependencias:** depende de 2.1 (la carpeta del juego donde va `Saves/`, el title ID para asociar la partida, la relación demo → juego completo para 5.3.1 y la versión del juego para restaurar) y de 3 (la ficha del juego). 5.4 depende además de 4.

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

### 5.3.1 Partidas de demos

Algunas demos generan partidas, y en varios sistemas la partida (o lo desbloqueado en ella) pasa al juego completo: demos de PS4/PS5 con progreso transferible, demos de Switch y de 3DS cuyo avance o objetos se heredan en el juego final, demos de Xbox con «continúa tu progreso». La demo es un juego aparte en la biblioteca (2.1), pero su partida es del mismo juego.

- [ ] Detectar las partidas de una demo y asociarlas a la demo y, a la vez, al juego completo relacionado (mismo título sin `(Demo)`, o el title ID base cuando el sistema lo enlaza).
- [ ] Guardarlas en la carpeta de la demo (`… (Demo)/Saves/`) y mostrarlas también en la ficha del juego completo como «Partida de la demo».
- [ ] No mezclarlas nunca con las del juego completo: son archivos distintos aunque el emulador las lea desde la misma carpeta o con el mismo id.
- [ ] Al borrar o apartar una demo (duplicados, Organizar), avisar si tiene partida y conservarla en la copia de seguridad.
- [ ] Investigar por sistema qué demos dejan heredar la partida y cómo (mismo id de partida, archivo de transferencia, desbloqueo por cuenta en línea que no se puede copiar) y anotarlo junto a la tabla de 5.2.

### 5.4 Sincronizar partidas

- [ ] Llevar y traer partidas con cada dispositivo (bloque 4), traduciendo la ruta y el nombre según el emulador de cada lado (p. ej. RetroArch `saves/` en la Anbernic ↔ junto a la ROM en la Ayaneo).
- [ ] Conflictos: si la partida cambió en los dos lados, quedarse con las dos versiones y preguntar; nunca sobrescribir sin copia.
- [ ] Convertir formatos cuando haga falta y se pueda (p. ej. extensiones `.srm` ↔ `.sav`, tamaños de memoria distintos según el emulador).

## 6. Organización y exportación para frontends

> **Dependencias:** usa los perfiles de consola actuales (`crates/coverly-core/src/profiles/builtin.rs`) y las tablas de carpetas investigadas en las Tandas 1–3. Se coordina con 4 (sincronizar), que usará estos mismos traductores al llevar la biblioteca a cada dispositivo, y con 2.1 (carpetas maestras del juego).

Hoy Coverly organiza y rellena portadas para varios perfiles (Anbernic stock, muOS, Knulli, Batocera, ES-DE, MinUI, Onion…), sobre todo de consolas portátiles, y no todos los detalles de cada frontend están cubiertos. La idea es ampliarlo a los frontends populares de Windows, Linux, macOS, Android e iOS. Es una tarea grande que necesita su propia investigación.

### Frontends a cubrir (Windows, Linux, macOS, Android e iOS)

Objetivo: dar soporte a la mayor cantidad posible de frontends populares en cada plataforma, no solo a los de consolas portátiles. Antes de implementar hay que **investigar uno por uno**: carpeta de ROMs y nombre de cada sistema, dónde y con qué nombre espera cada medio, el formato de la lista de juegos o metadatos y cómo lanza los juegos. La lista es el punto de partida y hay que confirmarla y ampliarla con fuentes oficiales al empezar. Como siempre, Coverly no enlaza, recomienda ni descarga emuladores; solo escribe lo que cada frontend lee.

- **Windows:** LaunchBox / Big Box, Playnite, RetroBat, ES-DE, Pegasus, RetroArch (playlists y thumbnails), Attract-Mode Plus, Steam ROM Manager (accesos y arte en Steam), HyperSpin y CoinOPS como legado.
- **Linux:** ES-DE, Pegasus, RetroArch, Attract-Mode Plus, Steam ROM Manager, RetroDECK y EmuDeck (Steam Deck), y las distribuciones Batocera, Knulli, ROCKNIX, Recalbox, RetroPie, Lakka y AmberELEC.
- **macOS:** OpenEmu (biblioteca propia con importación), ES-DE, Pegasus, RetroArch y Attract-Mode Plus.
- **Android:** Daijishō, ES-DE para Android, LaunchBox para Android, Pegasus, Beacon, Reset Collection, Console Launcher, DIG y los lanzadores propios de las portátiles Android (Retroid, AYN, Anbernic).
- **iOS / iPadOS:** no hay frontends separados porque las apps no pueden lanzar otras apps; cada app tiene su propia biblioteca (Delta, Provenance, RetroArch, Folium…). Aquí «soporte» significa exportar una carpeta lista para importar desde Archivos, con los nombres y las portadas que lee cada app.
- **Portátiles Linux (ya soportadas en parte):** Anbernic stock, muOS, Knulli, Onion, MinUI / NextUI, SpruceOS, CrossMix, Allium y ArkOS. Revisar que cada una esté completa.

Tareas:

- [ ] Investigación por frontend con fuentes primarias: documentación oficial y código. Por cada uno, una ficha en `docs/02-research/frontends/` con carpetas de sistema, medios, formato de metadatos, lanzadores, plataformas y licencia.
- [ ] Priorizar por uso real: primero ES-DE, RetroArch, LaunchBox, Playnite, Batocera/Knulli, Daijishō, Pegasus y RetroBat; después el resto.
- [ ] Un perfil (o traductor) por frontend en `profiles/`, con pruebas que comparen la estructura generada con la que espera la documentación.

### Portadas y medios en la carpeta que espera cada frontend

- **Problema detectado:** para ES-DE las portadas se escriben en `downloaded_media/{id de Coverly}/covers/` (`builtin.rs`), pero ES-DE las busca con **su** nombre de sistema (`gc`, `n3ds`, `atarijaguar`, `cdimono1`, `sg-1000`…), así que en esos sistemas no las muestra. Pasa también con la Atari 2600 desde antes y con los catálogos de muOS.
- [ ] Por cada frontend de la lista de arriba: dónde espera cada tipo de medio (portada, captura, título, marquesina, caja 3D, vídeo, manual) y con qué nombre de carpeta de sistema.
- [ ] Sustituir `{system_id}` por un `{system_folder}` del frontend en las plantillas de rutas.
- [ ] Migración para usuarios actuales: mover las portadas ya escritas a la carpeta nueva, con deshacer y sin duplicarlas.

### Metadatos y listas de juegos

- [ ] Exportar `gamelist.xml` (ES-DE, Batocera, Knulli, RetroBat, Recalbox, RetroPie), playlists `.lpl` de RetroArch, `metadata.pegasus.txt`, los XML de LaunchBox, la biblioteca de Playnite, los `.json` de plataforma de Daijishō, las listas de Attract-Mode y los accesos de Steam ROM Manager, con nombre, descripción, fecha, género, jugadores, valoración y rutas de medios.
- [ ] Respetar lo que ya haya escrito el usuario o el frontend (fusionar, no sobrescribir), como ya se hace con `gamelist.xml`.

### Lanzadores para juegos instalados

Algunos frontends lanzan juegos de PS3, Vita y otros sistemas «instalados» en el emulador mediante un archivo lanzador en la carpeta de ROMs en vez del juego: `.psvita` (ES-DE/Batocera con Vita3K, contiene el title ID), `.psn`/`.ps3` (RPCS3 para juegos instalados en `dev_hdd0`), y equivalentes para Switch o Wii U según el frontend.

- [ ] Investigar por frontend qué lanzadores usa cada sistema, su contenido y su nombre.
- [ ] Opción para crear el lanzador al organizar o sincronizar cuando el juego está instalado en el emulador y no hay archivo jugable en la carpeta.
- [ ] Reconocer los lanzadores existentes en la biblioteca como entrada del juego (no como archivo raro).

### Exportar la biblioteca a un frontend

- [ ] «Exportar para…»: generar desde la biblioteca la estructura completa de un frontend (carpetas de sistema, ROMs, medios, listas de juegos, lanzadores) en una carpeta o tarjeta de destino, con vista previa y deshacer. Es la base de la sincronización del bloque 4.
