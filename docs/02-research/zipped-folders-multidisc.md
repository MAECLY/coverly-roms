# Agrupación: carpetas-juego dentro de zip/7z y juegos multidisco en archivos sueltos (revisado)

## Qué cambió en esta revisión

Abrí cada fuente citada, descargué los archivos de código y los DAT, y revisé el repo `coverly-roms` (commit `3caea59`, 2026-11-01). Estas son las correcciones al borrador:

| # | El borrador decía | Lo verificado |
|---|---|---|
| C1 | Recuento del DAT de PS1: 2952 `(Disc 1)`, 2942 `(Disc 2)`… | **Contó cada juego dos veces.** Contaba también las líneas `rom ( name …)`. Además el DAT de libretro repite entradas por cada variante de serial. Nombres únicos: **607** `(Disc 1)`, 611 `(Disc 2)`, 163 `(Disc 3)`, 81 `(Disc 4)`, 8 `(Disc 5)`, 1 `(Disc A)`, 1 `(Disc B)`. |
| C2 | La unión de discos PS1 se haría con el serial del DAT | **En el DAT de libretro los tres discos de FF7 llevan `serial "SCUS-94163"`**, más variantes `SCUS-94163-0/-1/-2`. Los seriales 94164 y 94165 no aparecen. Los seriales por disco (los de `BOOT=`) son los que usa DuckStation. El DAT solo une por **hash → nombre del juego**, nunca por el serial de `SYSTEM.CNF`. |
| C3 | Saturn: «el n.º de producto suele variar entre discos» | **Sin respaldo, y el DAT dice lo contrario.** En las 85 series de Saturn con discos numerados y serial, el serial base es el mismo en todos los discos y solo cambia el sufijo `-N` (p. ej. `T-21301G`, `T-21301G-0`, `T-21301G-1`). Retirado. |
| C4 | «No hace falta nada nuevo de formatos»; PS4 se probaría como los demás | **No existe probe de PS4.** `DIR_PROBES` no incluye ninguno. `probe_vita_app` **descarta** los `CUSA*` (`sony.rs:155`; el comentario de `sony.rs:58-59` lo deja para la Tanda 3). Además, Vita y PS4 comparten `sce_sys/param.sfo` y `eboot.bin`. **Por la estructura no se distinguen**: hace falta leer el SFO. |
| C5 | ES-DE PS3: extraer como `Juego.ps3` | Solo vale para **juegos de disco**. ES-DE: "This approach is only intended for disc-based games as for HDD/pkg games you should use game serials or shortcuts instead". También hay que elegir el emulador alternativo *RPCS3 Directory (Standalone)*. El emulador por defecto es *RPCS3 Shortcut*. |
| C6 | ES-DE PS4: extraer `CUSAxxxxx` | ES-DE dice que lanzar `eboot.bin` "is not really recommended". Recomienda accesos directos o archivos de serial. Además, por velocidad, aconseja instalar los juegos fuera del árbol de ROMs. |
| C7 | ZIP cifrado: «los nombres llegan enmascarados» | Según §4.4.17.2, lo enmascarado es el nombre de la **cabecera local**. El directorio central va cifrado con Strong Encryption. La conclusión no cambia: no se puede detectar. |
| C8 | ES-DE «recomienda» la carpeta `Juego.m3u/` | ES-DE recomienda mucho el `.m3u` ("highly recommended"). La carpeta `Juego.m3u/` es solo "a possible approach" y "entirely optional". |
| C9 | Paralelo con 3DS: RPCS3 y `keys.txt` en Azahar | Lo de RPCS3 está verificado. **Azahar (2126.2, 2026-10-08) ya no carga juegos cifrados**: "Encrypted applications are not supported." Rechaza también los CIA cifrados. La premisa del usuario sobre Azahar está desfasada. |
| C10 | (no lo mencionaba) | **El repo ya tiene casi todas las señales de cabecera:** n.º de disco GC/Wii (`nintendo_disc.rs:42`, también CISO, GCZ, WIA y RVZ), IP.BIN de Sega con `(n, m)` (`disc/`), `SYSTEM.CNF`, PBP multidisco y `.m3u` en `dedupe/disc.rs`. Falta usarlas para agrupar. Tampoco mencionaba que el **CHD no se lee** (plan DD-2) y que con CHD solo queda el nombre o el DAT. |
| C11 | TOSEC: `(Disk x of y)` | El tipo de soporte puede ser `Disc`, `Disk`, `Tape`, `File` o `Side`. Por ejemplo, la convención usa "Disc 1 of 6" y "Disk 06 of 13". |
| C12 | muOS: «sin verificar» | La página `muos.dev/help/addcontent` sigue dando 404. El resultado del buscador describe `.hidden/`, pero queda **sin verificar**. |

Lo demás quedó confirmado: offsets, licencias y código de los emuladores (fuentes en cada tabla).

---

## Resumen

1. **Carpetas-juego en zip/7z.** Se detectan con la lista de entradas, sin extraer nada.
   - `identify_dir` prueba PS3 JB, PS3 HDD, Vita, Wii U loadiine y NUS, Xbox (`xbe`), Xbox 360 (`xex` y GOD) y Switch troceado, todo sobre un `DirListing` (`consoles/mod.rs:448-458`).
   - Los probes hacen hoy `symlink_metadata`/`read_dir` sobre disco (`mod.rs:421-424`). Hay que darles un listado virtual.
   - Ya hay una plantilla: `vpk_console` (`organize/inspect.rs:560`). Lista las entradas por la capa de comprimidos y lee `sce_sys/param.sfo` sin distinguir mayúsculas, con `Budget::plan_head`. Hoy solo funciona en la raíz del zip.
   - **Falta un probe de PS4** (C4).
   - Qué hacer con cada sistema:
     - **Extraer**: PS3 de disco, Wii U en carpeta y PS4.
     - **Dejar comprimido**: Vita, que Vita3K instala desde `.zip`, `.vpk` o `.vci`.
     - **PS3 HDD/PSN**: no basta con extraer, se instala en RPCS3 (C5).
2. **Multidisco suelto.** El nombre por sí solo no basta. La clave de agrupación es el título sin la etiqueta de disco, conservando región, Rev y demás etiquetas. Encima de eso se confirma con una de estas señales:
   - **Cabecera**:
     - GC/Wii: byte `0x06`;
     - Saturn: `CD-n/m` en `0x38`;
     - Dreamcast: `GD-ROMn/m` en `0x020`.
   - **DAT de Redump por hash**: el nombre del juego, sin la etiqueta de disco, coincide.
   - **Lanzador**: `.m3u` con nombres de archivo sin ruta.
   - **DuckStation `discsets.yaml`**: no se puede incluir porque es CC BY-NC-ND 4.0.

---

## Parte 1: carpetas-juego dentro de zip/7z

### 1.1 Listar sin extraer

**ZIP**
- El directorio central y el registro EOCD van al final, detrás de los datos ([APPNOTE 6.3.9 §4.3.6](https://pkwaredownloads.blob.core.windows.net/pkware-general/Documentation/APPNOTE-6.3.9.TXT)).
- Las rutas "MUST NOT contain a drive or device letter, or a leading slash. All slashes MUST be forward slashes" (§4.4.17.1). Aun así conviene aceptar `\`.
- Bit 13 (Central Directory Encryption): el nombre de la cabecera local "will not be the actual file name" (§4.4.17.2). Ese caso no se puede detectar y va a «no identificado».

**7z**
- La firma va seguida de `StartHeader { NextHeaderOffset, NextHeaderSize, NextHeaderCRC }`. La cabecera puede ir comprimida (`kEncodedHeader` = 0x17) ([7zFormat.txt](https://raw.githubusercontent.com/ip7z/7zip/main/DOC/7zFormat.txt)).
- `he` activa el cifrado de cabecera ("The default mode is he=off"). Con él no hay nombres sin contraseña ([7-Zip -m](https://7-zip.opensource.jp/chm/cmdline/switches/method.htm)).
- Limitar el bloque sólido "Decreases extraction time of a group of files (or just one file), so long as the group doesn't contain the entire archive" (misma fuente). En sentido inverso: en un 7z sólido, leer `PARAM.SFO` puede obligar a descomprimir GB.
- Con el presupuesto de 64 KiB (`Budget::plan_head`), la detección puede caer a «solo estructura».
  - Para PS3 y Wii U basta con la estructura.
  - Para **Vita y PS4 no basta**, porque la estructura es idéntica (C4). Ese caso va a «revisar».

**Dependencias**
- `zip = "9"` y `sevenz-rust2 = "=0.23.0"` están en `Cargo.toml:33-34` (verificado).

### 1.2 Marcadores por sistema

| Sistema | Marcador en el listado | ID | Fuente |
|---|---|---|---|
| PS3 disco (JB) | `PS3_GAME/USRDIR/EBOOT.BIN` + `PS3_GAME/PARAM.SFO` (+ `PS3_DISC.SFB`) | `PARAM.SFO` | [RPCS3 System.cpp](https://raw.githubusercontent.com/RPCS3/rpcs3/master/rpcs3/Emu/System.cpp): `boot_list` l.1049-1054, `PS3_DISC.SFB` l.1909, `/dev_bdvd/PS3_GAME/` l.1971-1972 |
| PS3 HDD/PSN | `<ID>/USRDIR/EBOOT.BIN` + `<ID>/PARAM.SFO` | `PARAM.SFO` | mismo archivo, l.1995-2008 (`m_cat == "DG"` → `/dev_hdd0/game/`) |
| Wii U loadiine | `code/*.rpx` con `content/` y `meta/` hermanas | `meta/meta.xml` | [Cemu TitleInfo.cpp](https://raw.githubusercontent.com/cemu-project/Cemu/main/src/Cafe/TitleList/TitleInfo.cpp) l.186-198 y l.278 |
| Wii U NUS | `title.tmd` | `title.tmd` | ídem, l.211-216 |
| PS Vita | ruta que contenga `sce_sys/param.sfo` (la raíz es el prefijo); abortar con `sce_module/steroid.suprx` | SFO con un ID que no empiece por `CUSA` | [Vita3K interface.cpp](https://raw.githubusercontent.com/Vita3K/Vita3K/master/vita3k/interface.cpp) l.266-296. Ojo: Vita3K usa `contains`, que **distingue mayúsculas**. También acepta `theme.xml` |
| PS4 | `eboot.bin` + `sce_sys/param.sfo` en la misma raíz | **Obligatorio leer el SFO** (`TITLE_ID` `CUSA…`) para separarlo de Vita | `TODO.md` l.177 y l.187; `sony.rs:58-61, 155` |

**Algoritmo de raíz (como Vita3K).**
- Cada entrada que acaba en el marcador da una raíz candidata, que es el prefijo.
- 0 raíces: no es una carpeta-juego.
- 1 raíz: un juego, con prefijo `""` o `Nombre/`.
- Varias raíces: Vita3K instala todas (l.325-345, bucle `for (auto &path : content_path)`). En Organizar se marca «revisar».
- Se compara sin distinguir mayúsculas.
- Si casan dos sistemas, `DirConflict` (`mod.rs:439`, `identify_dir` en l.492).

### 1.3 Qué hace cada emulador

| Sistema | ¿Carga el comprimido? | Qué entregar | Fuente |
|---|---|---|---|
| PS3 disco (RPCS3) | No | **Extraer.** En ES-DE la carpeta se renombra `Juego.ps3` y se elige *RPCS3 Directory (Standalone)*. Alternativa: ISO con *RPCS3 ISO (Standalone)*. Si la ISO está cifrada, RPCS3 necesita la clave en `get_redump_key_dir()` | [ES-DE USERGUIDE](https://gitlab.com/es-de/emulationstation-de/-/raw/master/USERGUIDE.md) «Sony PlayStation 3» (Directories / ISO files); [System.cpp](https://raw.githubusercontent.com/RPCS3/rpcs3/master/rpcs3/Emu/System.cpp) l.1174-1195: "is encrypted and no matching decryption key was found in '%s'" |
| PS3 HDD/PSN | No | **No se entrega como carpeta en ES-DE**: hay que instalarlo en RPCS3 (`dev_hdd0`) y usar archivo de serial o acceso directo. Es un lanzador, así que va al TODO | ES-DE: "for HDD/pkg games you should use game serials or shortcuts instead" |
| Wii U (Cemu) | No (`.wud`, `.wux`, `.iso`, `.wua`, `.wuhb`, `title.tmd`, carpeta) | **Extraer** `code/content/meta`. ES-DE prefiere `.wua` | Cemu l.203-216 y l.252; ES-DE: "The .wua archive format is the preferred method to use for Wii U games" |
| PS Vita (Vita3K) | **Sí**: `.zip`, `.vpk`, `.vci` (miniz). 7z no consta | **Dejar el comprimido.** Un 7z hay que recomprimirlo a zip. ES-DE además pide el `.psvita` con el Title ID (lanzador) | [Vita3K quickstart](https://vita3k.org/quickstart.html): "Use File > Install Archive (.zip / .vpk / .vci)…"; ES-DE «Sony PlayStation Vita» |
| PS4 (shadPS4) | No | **Extraer** `CUSAxxxxx`. En ES-DE el método `eboot.bin` "is not really recommended": son preferibles el acceso directo o el archivo de serial | ES-DE «Sony PlayStation 4»; `TODO.md` l.177 |
| PS5 (Prosperity) | ZIP/RAR desde la app | Dejar el comprimido | `TODO.md` l.178 y l.192 |

**Recomendación**
- Vita: «copiar tal cual».
- PS3 de disco, PS4 y Wii U: «extraer a carpeta».
  - Se conserva la raíz y se quita el envoltorio.
  - Antes de extraer se pide la confirmación «sí / sí a todo» (punto 4 del usuario).
- PS3 HDD: «revisar», con la nota «instalar en RPCS3».

---

## Parte 2: juegos multidisco en archivos sueltos

### 2.1 Convenciones de nombre

**Redump** (DAT de libretro-database)
- Formato: `Título (Región) (Disc N) [etiquetas]`. Ejemplos verificados:
  - `Final Fantasy VII (USA) (Disc 1)`
  - `Ace Combat 3 - Electrosphere (Japan, Asia) (Disc 1) (Rev 1)`
  - `Sister Princess 2 - Premium Fan Disc (Japan) (Disc A)`
- En Saturn existen `(Disc 0)`, por ejemplo `Enemy Zero (USA) (Disc 0)` y `Enemy Zero (Europe) (Disc 0) (Opening Disc)`, y `(Disc C)`, por ejemplo `Haunted Casino (Japan) (Disc C)`.
- Recuento de nombres únicos en PS1: ver C1. 340 nombres de PS1 llevan etiquetas detrás de `(Disc N)`.
- Fuentes: [Sony - PlayStation.dat](https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/redump/Sony%20-%20PlayStation.dat) (versión 2026.10.07), [Sega - Saturn.dat](https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/redump/Sega%20-%20Saturn.dat).

**TOSEC**
- Tipos de soporte `Disc`, `Disk`, `Tape`, `File` y `Side`.
- "if there are 10 or more disks then (Disk xx of yy) should be used".
- También `(Side A)` y `(Part 1-2 of 3)`.
- Media type y media label van al final de los paréntesis; los flags entre corchetes vienen detrás ([TOSEC](https://www.tosecdev.org/tosec-naming-convention)).

**No-Intro**
- Su página de convención no menciona discos ([wiki](https://wiki.no-intro.org/index.php?title=Naming_Convention)).

**Regla de agrupación por nombre**
- Se quita solo la etiqueta de disco, con el patrón `(Disc|Disk|CD|GD)\s+(\d+|[A-Z])(\s+of\s+\d+)?`.
- Se conserva todo lo demás.
- Un `(Disc 0)` (disco de apertura) es un miembro extra del grupo y no rompe la secuencia 1..m.

### 2.2 Señales de cabecera (y qué ya existe en coverly)

| Sistema | Campo | Qué da | Fuente | En el repo |
|---|---|---|---|---|
| GC/Wii | `0x000` ID (6 car.), **`0x006` disc number**, `0x007` versión, `0x018` magic Wii `0x5D1C9EA3`, `0x01C` magic GC `0xC2339F3D` | Mismo ID con distinto `0x06` → discos del mismo juego | [WiiBrew «Wii disc»](https://wiibrew.org/wiki/Wii_disc) | `nintendo_disc.rs:42` `disc_no = h[0x06]+1`, también CISO, GCZ, WIA y RVZ |
| Saturn | `0x00` "SEGA SEGASATURN ", `0x20-0x29` producto, `0x2A` versión, **`0x38-0x3F` device info** | `CD-n/m`. Sega: "In the case of a CD, enter the number of the set", con los ejemplos "CD-1/1" y "CD-2/3" | [Sega Programmer's Guide, Disc format](https://www.infochunk.com/saturn/segahtml_en/prgg/sofg/disc/hon/p04_10.htm); [Ymir saturn_header.cpp](https://raw.githubusercontent.com/StrikerX3/Ymir/HEAD/libs/ymir-core/src/ymir/media/saturn_header.cpp) l.18-27 (Ymir es GPL-3.0: [LICENSE](https://raw.githubusercontent.com/StrikerX3/Ymir/HEAD/LICENSE)) | `identify/sniff.rs:426`, `disc/sega.rs`; `DiscIdentity` guarda `n` y **descarta `m`** (`disc/mod.rs:228`) |
| Dreamcast | `0x020-0x02F` device info = CRC de 4 hex + "GD-ROM" + `n/m` (p. ej. "8B40GD-ROM2/3"); `0x040` producto; `0x080` título | `n/m` | [mc.pp.se IP0000.BIN](https://mc.pp.se/dc/ip0000.bin.html) | `sniff.rs:427`, `disc/sega.rs` |
| Firma Sega en el sector | Saturn y DC en 0 (2048 B/sector) o en 0x10 (2352 B/sector) | — | [file(1) console](https://raw.githubusercontent.com/file/file/master/magic/Magdir/console) l.413-421 y l.439-444 | ya implementado |
| PS1/PS2 | `SYSTEM.CNF` `BOOT=`/`BOOT2=` | Serial por disco (FF7: 94163, 94164 y 94165 según DuckStation). **El DAT de libretro no usa esos seriales** (C2) | [discsets.yaml](https://raw.githubusercontent.com/stenzek/duckstation/master/data/resources/discsets.yaml) l.1109-1113; DAT PS1 l.25094-25127 | `dedupe/disc.rs` (`SYSTEM.CNF`, PBP multidisco, m3u) |
| Sega CD | `SEGADISCSYSTEM` en 0 o 0x10; serial en `$180` con el formato `XX YYYYYYYY-ZZ` | **No hay campo de n.º de disco** | [Plutiedev](https://plutiedev.com/rom-header); file(1) l.239 y l.248 | `sniff.rs:1027` |
| CHD (cualquier sistema) | — | No se lee la cabecera | — | `dedupe/disc.rs`: "A CHD is not read (plan DD-2)" |

**Licencia de la base de DuckStation**
- `discsets.yaml` tiene 562 entradas (verificado).
- La licencia del repositorio es **Attribution-NonCommercial-NoDerivatives 4.0 International** ([LICENSE](https://raw.githubusercontent.com/stenzek/duckstation/master/LICENSE)).
- NC y ND chocan con AGPL-3.0, que permite el uso comercial y las obras derivadas. **No se incluye.**
- Alternativa: unir por hash contra el DAT de Redump que ya usa coverly (`identify/dat.rs`). Si los discos casan con entradas cuyo nombre, sin la etiqueta de disco, es igual, forman un juego.

### 2.3 `.m3u` por frontend y emulador

| Frontend / emulador | Comportamiento | Fuente |
|---|---|---|
| ES-DE | Una línea por disco con nombre de archivo. El `.m3u` es "highly recommended". La carpeta `Juego.m3u/` es opcional ("possible approach", "entirely optional"). Tabla de sistemas: psx, saturn y gc ".m3u playlist for multi-disc games"; dreamcast "In separate folder interpreted as a file, with .m3u playlist if multi-disc game" | [USERGUIDE](https://gitlab.com/es-de/emulationstation-de/-/raw/master/USERGUIDE.md) l.1314-1347, 5049, 5069, 5135, 5140 |
| RetroArch | `#` es comentario. Etiquetas `#LABEL:`, `#EXTINF:…,label` y `ruta\|label`. Rutas relativas resueltas contra el playlist. `#EXTM3U` se trata como comentario | [rm3u.c](https://raw.githubusercontent.com/libretro/RetroArch/master/libretro-common/formats/m3u/rm3u.c) l.24-61 |
| Beetle PSX | "an index file (a m3u file) should be made… enter your game's .cue files on it". La memory card y la carpeta de texturas toman el nombre del `.m3u` | [docs Beetle PSX HW](https://docs.libretro.com/library/beetle_psx_hw/) |
| RetroArch + zip | "zip support is provided by RetroArch and is done before passing the game to the core… when using a m3u file, the specified disk images must be in uncompressed form" (DamsRoxxx, 2018) | [foro libretro](https://forums.libretro.com/t/m3u-and-disk-control-support-for-classic-computers-amiga-cpc/18049) |
| DuckStation | `#` es comentario; las rutas relativas se resuelven con `BuildRelativePath` | [cd_image_m3u.cpp](https://raw.githubusercontent.com/stenzek/duckstation/master/src/util/cd_image_m3u.cpp) l.80-99 |
| Dolphin | Acepta `.m3u` y `.m3u8`. Quita el BOM con aviso. `#` es comentario. Las rutas son relativas a la carpeta. Si falta algún archivo: "Files specified in the M3U file … were not found" y el juego no arranca | [Boot.cpp](https://raw.githubusercontent.com/dolphin-emu/dolphin/master/Source/Core/Core/Boot/Boot.cpp) l.64-108 y l.217-219 |
| PCSX2 | No soporta m3u. `IsDiscFileName` acepta solo `.iso .bin .img .mdf .gz .cso .zso .chd` (l.2488-2499). Ningún archivo del árbol, de 3104 entradas, contiene «m3u». Una búsqueda de código en GitHub no encuentra referencias. **No se genera m3u** | [VMManager.cpp](https://raw.githubusercontent.com/PCSX2/pcsx2/master/pcsx2/VMManager.cpp) |
| Batocera | `filename.cue` → se oculta; `./filename.cue` → se ve. "It can only recognize and remove extra discs for a single game if they are in the same folder". Disc control confirmado en PSX, SegaCD, Saturn y Dreamcast; Libretro/Yabause no funciona. "target the .cue file, not the .bin file" | [Batocera cd_image_formats](https://wiki.batocera.org/cd_image_formats) |
| KNULLI | "Ignore Multi-File Disk Content (CUE/GDI/CCD/M3U)". "By default, hiding individual discs is turned on". En la misma carpeta, nombres "without any path" | [KNULLI](https://knulli.org/configure/multi-disc-games/) |
| muOS | `.hidden/` con el `.m3u` un nivel arriba. **Sin verificar**: la página oficial da 404 | `muos.dev/help/addcontent` (404 el 2026-11-01) |

**Formato común**
- UTF-8 sin BOM y sin `#EXTM3U`.
- Una línea por disco, apuntando al `.cue`, `.gdi` o `.chd`, nunca a los `.bin` de pista.
- Solo nombres de archivo, salvo en el perfil muOS.
- Coverly ya lo genera al extraer: `m3u_text` en `organize/extract.rs:683`, con `/`, UTF-8 sin BOM y `\n`, y prefiere `.gdi` sobre `.cue`. Falta aplicarlo a los discos sueltos.

### 2.4 Cómo agrupar con seguridad (propuesta)

1. **Candidatos**: mismo sistema, misma carpeta de origen y misma clave de nombre (2.1).
2. **Confirmación**, de mayor a menor fuerza:
   - GC/Wii: mismo ID de 6 caracteres y `disc_no` distinto. Ya está en `nintendo_disc.rs`.
   - Saturn/DC: mismo `m`, `n` distintos que cubren 1..m (`Disc 0` aparte). Para eso hay que **conservar `m` en `DiscIdentity`**, que hoy se descarta.
   - DAT Redump **por hash**: los nombres sin la etiqueta de disco coinciden. No sirve unir por el serial del DAT (C2).
   - Solo nombre (Sega CD, CHD, discos sin DAT): se propone en «revisar».
3. **Rechazar** si:
   - hay dos archivos con el mismo número de disco: es un duplicado y lo resuelve `dedupe/`;
   - difieren la región o la Rev;
   - la cabecera contradice el nombre;
   - falta un disco: el `.m3u` se escribe solo con los presentes y se avisa, porque Dolphin aborta si hay entradas inexistentes.
4. **Aplicar** según el perfil:
   - ES-DE: `Juego.m3u/` (opcional) o `.m3u` junto a los discos.
   - Batocera y KNULLI: la misma carpeta, con nombres sin ruta.
   - muOS: `.hidden/`, pendiente de verificar.
   - PS2: carpeta sin `.m3u`.

---

## Decisiones derivadas para el usuario

- **Detección dentro de comprimidos.** Se hace con los probes existentes sobre un listado virtual y la plantilla de `vpk_console`.
  - **Hace falta un probe nuevo para PS4.**
  - En un 7z sólido, Vita y PS4 no se separan sin leer el SFO.
  - Las acciones: Vita se deja comprimido; PS3 de disco, PS4 y Wii U se extraen con confirmación; PS3 HDD se instala en RPCS3.
- **Multidisco.** Las señales de cabecera ya están programadas para GC, Wii, Saturn y Dreamcast. PS1 y PS2 dependen del DAT por hash. Sega CD y CHD dependen del nombre o del DAT.
- **Analogía legal.**
  - RPCS3 sí sigue el patrón: abre ISO cifradas solo si el usuario pone la clave en su carpeta de claves, y no la incluye (`check_disc_key`).
  - **Azahar actual ya no lo sigue.** Rechaza las aplicaciones y los CIA cifrados ("Encrypted applications are not supported.", "Your CIA file is encrypted."; [citra_qt.cpp](https://raw.githubusercontent.com/azahar-emu/azahar/master/src/citra_qt/citra_qt.cpp) l.1511-1517 y l.2623-2628; release 2126.2 del 2026-10-08).
  - Para 3DS, coverly debería identificar los archivos cifrados y avisar de que hay que descifrarlos. No debe prometer que funcionan con una clave.
- **Lanzadores** `.psvita`, `.ps3` (serial), `.psn` y accesos directos de RPCS3/shadPS4: van al `TODO.md`.

## Archivos relevantes del repo

- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/consoles/mod.rs` (`DirListing` l.417, `DirConflict` l.439, `DIR_PROBES` l.448, `identify_dir` l.492)
- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/consoles/sony.rs` (`probe_vita_app` l.149, exclusión PS4 l.155, `from_vpk`)
- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/consoles/nintendo_disc.rs` (l.42 `disc_no`)
- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/organize/inspect.rs` (`vpk_console` l.560, `vpk_entry`)
- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/organize/walk.rs` (`folder_game_in` l.486)
- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/organize/extract.rs` (`m3u_text` l.683)
- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/organize/target.rs` (`multi_disc` l.169)
- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/disc/mod.rs` (`DiscIdentity`, `sega_identity` l.218-232) y `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/disc/sega.rs`
- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/dedupe/disc.rs`
- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/crates/coverly-core/src/archive/budget.rs`
- `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms/TODO.md` (l.170-193)