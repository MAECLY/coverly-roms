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

## Sistemas que faltan

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
