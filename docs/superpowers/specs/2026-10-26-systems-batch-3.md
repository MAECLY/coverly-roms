# Coverly ROMs — Missing systems, batch 3: current generation (PS4, PS5, Xbox One, Xbox Series) (Design Spec)

> Status: **Design** (self-reviewed §17) · Date: 2026-10-26 (research run 2026-11-01) · Owner: Miguel Angel Esparza Calero (MAECLY)
> Roadmap item: `TODO.md` §2 «Sistemas que faltan», **Tanda 3 only**. Tandas 1 and 2 are merged (PR #32, PR #33; specs [`2026-10-26-systems-batch-1.md`](2026-10-26-systems-batch-1.md), [`2026-10-26-systems-batch-2.md`](2026-10-26-systems-batch-2.md)). TODO §2.1 (per-game folders with `DLC/`, `Updates/`, `Mods/`, `Cheats/`, `Extras/`) is **not** part of this batch.
> Branch: `feat/systems-batch-3`. `docs/TRACKER.md` and `TODO.md` are updated on `main` after merge, not on this branch.
> Builds on: batch 2 in full: `consoles/` (`ConsoleId`, `TitleKind`, `ArtLocator`, `Budgeted`, `is_key_material`, `open_rom`, `identify_dir`/`DirProbe`), folder games (`ItemKind::Folder`, `rename_dir_noclobber`, D25–D27, one `RomEntry` per folder game), updates and DLC listed under their base (D11, `updates_after_bases`), embedded covers (`sources/embedded.rs`, D17), title-id dedupe keys, `LeftReason::{KeyMaterial, EmulatorData}`, `ItemNote::{InstallFirst, FolderSuffix}`; research [`docs/02-research/updates-dlc-per-system.md`](../../02-research/updates-dlc-per-system.md) (PS4, PS5, Xbox One/Series sections); brand [`docs/08-brand/brand.md`](../../08-brand/brand.md).

## 1. Purpose

After batch 2 Coverly knows 127 systems. The four of Tanda 3 add three things the earlier batches did not have to face:

1. **Two of them run and two barely do.** PS4 and PS5 have real, active emulators, so the owner wants them **first-class**: identification, profile folders, covers, duplicates and Organizar, like every other format. Xbox One is early (WinDurango, three playable games). Xbox Series has no public emulator at all and is catalogued only, marked «sin emulador».
2. **The playable form is a folder, and the folder is huge.** A decrypted `CUSA00900/` or `PPSA01234/` holds tens or hundreds of GB. Coverly identifies it from `sce_sys/` (or `MicrosoftGame.config`, `AppxManifest.xml`) and a few header bytes, never from the contents, and asks before copying that much (D19).
3. **Scams are part of the landscape.** Fake PS5 and Xbox emulator repos spread password stealers (NordVPN, 2026-10-28). Coverly never names, links, recommends or downloads an emulator for these systems (D4), and never asks for, reads or decrypts a key or licence.

The batch keeps every rule of batches 1 and 2 (no silent reclassification, folder names from sources, ambiguous files to review, headers only within budget, keys untouchable) and adds one: **an emulator's own sibling layout is part of the game** (shadPS4's `-UPDATE`/`-patch`/`-mods` folders follow their base, D11).

## 2. The owner's asks (verbatim from `TODO.md` §2, Tanda 3)

> Revisado el 2026-10-31, comprobando cada fuente (repos oficiales, código, listas de compatibilidad). PS4 y PS5 ya tienen emuladores reales y activos, así que se tratan **como el resto de formatos**: identificación, carpeta en los perfiles que los usen, portadas, duplicados, Organizar y, más adelante, sincronizar. Xbox One está en una fase temprana y Xbox Series no tiene ningún emulador público.
>
> **Estado de la emulación (2026-10-31):** PS4: shadPS4 (GPL-2.0, v0.21.0), fpPS4, Kyty (parado), Obliteration (no ejecuta juegos); shadPS4 carga carpetas descifradas `CUSAxxxxx` (con `-patch`/`-UPDATE` para actualizaciones) y ya **no** carga PKG/fPKG (quitado en v0.8.0). PS5: KytyPS5, SharpEmu, Prosperity, AnyPS5 (reenlazador, no emulador); cargan carpetas descifradas `PPSAxxxxx` con `eboot.bin` y `sce_sys/param.json`; además `.zar` (SharpEmu), `.ffpkg` y ZIP/RAR de la app (Prosperity). Nadie carga el `.pkg` de tienda ni imágenes exFAT. Xbox One: WinDurango (LGPL-3.0), carpeta ERA montada con `AppxManifest.xml`; no carga XVC cifrados. Xbox Series X/S: ninguno público.
>
> **Estafas y malware:** repos de GitHub falsos que imitan a KytyPS5 y AnyPS5 reparten un ladrón de contraseñas. PCSX4/PCSX5, «PS5 emulator APK», «Rexbox» y las webs de «Xbox One/Series emulator» con encuestas o pagos son estafas conocidas o no verificables. Coverly nunca enlaza, recomienda ni descarga emuladores; si alguna vez se nombran, solo con los repos oficiales de la tabla.
>
> - **Sony PS4 (`ps4`).** Carpeta `CUSAxxxxx` descifrada: leer `sce_sys/param.sfo` (claves `TITLE_ID`, `TITLE`, `CONTENT_ID`, `CATEGORY`, `APP_VER`). Portada incrustada `sce_sys/icon0.png`; fondo `pic1.png`. El nombre de la carpeta suele ser el title ID (respaldo). `.pkg`/fPKG: magia `0x7F434E54`; content ID en `0x40` (title ID en `0x47`); en la tabla de entradas (offset en `0x18`, número en `0x10`) la entrada `0x1000` es el `param.sfo` en claro y la `0x1200` el `icon0.png`. Se puede catalogar sin descifrar, pero shadPS4 no lo arranca: avisar «hay que extraerlo» en Organizar. Región por el prefijo del content ID (`UP` América, `EP` Europa, `JP` Japón, `HP` Asia); el código `CUSA` no basta. Confirmar la tabla de prefijos con una fuente primaria. Actualizaciones `CUSAxxxxx-patch`/`-UPDATE` y DLC: agruparlas con su juego, no como juegos aparte (y no contarlas como duplicados).
> - **Sony PS5 (`ps5`).** Carpeta `PPSAxxxxx` con `eboot.bin` y `sce_sys/param.json`: leer `titleId`, `contentId`, `contentVersion` (o `appVersion`) y `localizedParameters.<idioma>.titleName`. Portada `sce_sys/icon0.png`, fondo `pic0.png`. Contenedores aceptados: `.zar`, `.ffpkg` y ZIP/RAR de la app, leídos con el lector de comprimidos actual cuando se pueda. `.pkg` de tienda e imágenes exFAT: reconocerlos y dejarlos con la nota «ningún emulador lo carga así», nunca como jugables. Detectar PS5 Pro por `ATTRIBUTE3` como etiqueta informativa.
> - **Microsoft Xbox One (`xboxone`).** Carpeta ERA con `AppxManifest.xml`: `Identity Name/Publisher/Version` y `Properties/DisplayName` (puede ser `ms-resource:`). Carpeta GDK con `MicrosoftGame.config` y `Executable/@TargetDeviceFamily="XboxOne"`: `TitleId` (8 hex; `FFFFFFFF` es un marcador), `MSAAppId`, `StoreId`, `ShellVisuals/DefaultDisplayName`. XVC/MSIXVC: magia `msft-xvd` en `0x200`, ContentId en `0x220`, ContentType en `0x284`, ProductId en `0x39C`, PackageVersion en `0x3BC`. Solo GUIDs, contenido cifrado: catalogar por GUID y pedir el nombre a una fuente externa.
> - **Microsoft Xbox Series X/S (`xboxseries`).** Solo cuando `Executable/@TargetDeviceFamily="Scarlett"` (o `TargetDeviceFamilyForDLC="Scarlett"` para DLC). **No** usar `XboxCompatibility="Scarlett"`. `TargetDeviceFamily="PC"` es una build de PC: no meterla en ningún sistema de consola. Marcado como «sin emulador».
> - **Perfiles:** investigar qué frontends tienen carpeta para estos sistemas y dónde esperan las carpetas `CUSA*`/`PPSA*`. Los juegos aquí son **carpetas**: reglas de «juego = carpeta».
> - **Portadas:** libretro-thumbnails no tiene estos sistemas. Usar la portada incrustada como primera opción y como respaldo SteamGridDB, ScreenScraper o una base pública de title IDs.
> - **Duplicados:** clave de contenido por title ID + versión, y las actualizaciones nunca como copias del juego base.
> - **Tamaño:** escanear sin leer el contenido entero (solo `sce_sys/` y cabeceras), avisar del espacio en Organizar y no proponer extraer ni copiar sin confirmar el tamaño.

(The emulator table is abridged here; the full table with links is in `TODO.md` and is not reproduced in the app.)

Workflow brief additions for this run: reuse batch 1–2 infrastructure; PS4/PS5 first-class; Xbox One early (ERA, GDK, XVC GUIDs only); Xbox Series only by the Scarlett rule, «sin emulador»; never require, read or decrypt keys or licences (`.rif`, `act.dat`, licence blobs; by name only); never link, recommend or download an emulator; huge folders identified from `sce_sys/` and headers only, within budgets; profile folders researched per frontend, never guessed; embedded `icon0.png`/`pic0.png`/`pic1.png` first, then SteamGridDB/ScreenScraper, libretro verified live; TODO 2.1 not implemented (Tanda 2's grouping kept); owner decisions implemented around and listed (§13).

## 3. What exists today (read before changing)

Batch 2's table (its §3 and §18) holds on `main` at `eaf7190`. What matters in addition for this batch:

| Layer | Where | What matters here |
|---|---|---|
| Systems | `systems/systems.toml` (127 entries) | `pkg` is weak on `ps3`, `psvita`; `zar` is **strong** on `xbox360` (Xenia ZArchive). `embedded` is a global source, last in the default order. |
| PSF | `consoles/psf.rs` | Reads `fmt` as LE u16 (`0x0204` …). TODO says «`fmt` en big-endian»: shadPS4 declares `param_fmt` as `u16_be` **but compares `.Raw()`** (the unswapped stored value) with `0x0204` (`src/core/file_format/psf.{h,cpp}` at `55a97e46`), so the bytes on disk are `04 02` and Coverly's LE read is right. The reader is reused unchanged. |
| Sony | `consoles/sony.rs` | `probe_vita_app` and `from_vpk` **reject `CUSA` title ids** («left for Tanda 3»); `pkg()` knows only `7F 50 4B 47` (PS3/Vita); `from_psf` maps `CATEGORY` for `ps3`/`psvita` only. |
| Probes | `consoles/mod.rs` `DIR_PROBES` | Fixed order, first match wins, two systems → `DirConflict` (review). |
| Grouping | `organize/plan.rs` `title_group`, `updates_after_bases`; `consoles::base_title_id` | `base_title_id` handles `switch 3ds wiiu ps3 psvita xbox360` only. |
| ConsoleId | `consoles/mod.rs` | No `content_id`, no `region`. `IdSource` has no `ParamJson`, `MicrosoftGameConfig`, `AppxManifest`, `XvdHeader`. |
| Dedupe | `dedupe/keys.rs` `title_key`, `title_of`, `title_version_key`, `TITLE_ID_SYSTEMS` | Key = title id + kind + version. For PS4/PS5 every DLC of a game shares the base's `TITLE_ID` and kind `Dlc`, so two **different** DLCs would be "duplicates" (D14). |
| Embedded | `sources/embedded.rs` `SYSTEMS = 3ds ps3 psvita wiiu xbox360`, `MAX_SIDE` 1024, Cover only | Always tried in the user's source order (last by default). |
| Walk | `organize/walk.rs` `EMULATOR_DATA_ROOTS = ux0 dev_hdd0 mlc01 nand` | No shadPS4 user dir. A folder with no probe hit is walked as loose files (a `CUSA00900-mods/` would be scattered). |
| Warnings | `organize/plan.rs` `PlanWarning::NoSpace` | No confirmation before a large copy; the apply button applies at once (`OrganizeScreen.tsx`). |
| SystemInfo | `src/ipc/types.ts` `{ id, name, family }` | No way to say «sin emulador». |
| Facts | `FACTS_VERSION = 7` | No Tanda 3 verdicts. |
| Libretro | `tests/fixtures/research/thumbnails-root.html` (2026-10-31) | **Has `Sony - PlayStation 4/`**, contrary to TODO's «libretro-thumbnails no tiene estos sistemas». Checked live on 2026-11-01: the folder exists with 20 `Named_Boxarts` (Japanese releases) and no snaps or titles; no PS5, Xbox One or Series folder. |
| DATs | libretro-database tree at `764ef77a` | No PS4, PS5, Xbox One or Series DAT. |

## 4. Decisions

Open questions and the default chosen. Each can be overturned by the owner without changing the rest of the design.

| # | Question | Decision | Why |
|---|---|---|---|
| D1 | Scope | The four systems, their folder/file forms, profile rows, icons, covers, dedupe keys, the size confirmation. Not TODO 2.1 (no `DLC/`/`Updates/` subfolders per game, no Mods/Cheats/Extras), not sync, no PS4/PS5 disc images (Redump discs are encrypted Blu-rays no emulator loads). | Brief. |
| D2 | Ids | `ps4`, `ps5`, `xboxone`, `xboxseries`. | Owner's ids; profile spellings go to §6. |
| D3 | New crates | **None.** `serde_json` (param.json), `quick-xml` (manifests), `zip`, `ruzstd` and the batch 2 ZArchive reader cover everything. | Smallest surface. |
| D4 | Keys, licences and emulators (legal/security rule) | Batch 2 D4 applies unchanged (`is_key_material` before any open). Additions: `cik` joins `KEY_EXTS` (Xbox content instance keys). PS4/PS5 `.rif` is already listed; `act.dat` too. `sce_sys/license.dat` inside a game folder moves with the folder and is never read (it is not added to `KEY_NAMES`: the name is too generic and a `.dat` is never rom-like). A PKG entry or `param.json` flagged encrypted is **never read** (§8.1, §8.3). **No emulator is named in any new copy** (not even the official ones): notes state facts about the file («hay que extraerlo», «ningún emulador lo abre así»), never which program to use, never a URL, never how to decrypt or extract. A test guards the locale files (§14.2). | Brief and TODO's malware warning; a name in the UI is one search away from a fake repo. |
| D5 | PS4 folder probe | New `sony::probe_ps4_app`: `sce_sys/param.sfo` (one listing of `sce_sys`, as Vita) whose PSF says PS4: `TITLE_ID` starts with `CUSA`, **or** the PSF has `PUBTOOLINFO` or `APP_TYPE` (PS4-only keys) and no `PSP2_SYSTEM_VER` (Vita-only). Verify both key sets against LibOrbisPkg `ParamSfo.cs` and Vita3K's SFO reader in the task; if a key cannot be confirmed, the rule shrinks to the `CUSA` prefix. `probe_vita_app`/`from_vpk` reject the same PS4 signals. Order: after the PS3 probes, before Vita. | A Vita app folder and a PS4 app folder have the same shape. |
| D6 | PS5 folder probe | New `sony::probe_ps5_app`: `sce_sys/param.json` (≤ 64 KiB, UTF-8, BOM stripped, `serde_json`, no recursion beyond what `serde_json` does on a 64 KiB input). Fields (KytyPS5 `src/loader/systemContent.cpp` at `893b0355`): `titleId`, `contentId`, `contentVersion` else `appVersion`, `attribute3` (int), `localizedParameters.defaultLanguage` and `<lang>.titleName` (title: default language, else `en-US`, else the first). Kind: `originContentVersion` or `targetContentVersion` present → `Update`; no `eboot.bin` at the folder root → `Dlc` (research: DLC sits under `addcont/<PPSA>/<label>/` without `eboot.bin`); else `Base`. No demo rule (none known). `applicationCategoryType` is **not** used (sources disagree). | The owner's field list, checked against a real PS5 emulator's loader. |
| D7 | PS4 kind and version | `CATEGORY` (LibOrbisPkg `ParamSfo.cs`): `gd` → Base, `gp`/`gpc`/`gpd`/`gpe`/`gpk`/`gpl` → Update, `ac` → Dlc; `APP_TYPE = 3` → Demo (overrides Base). No `CATEGORY`: a folder name ending `-UPDATE`/`-patch` → Update, else Unknown. Version `APP_VER`, else `VERSION`. | Research doc, high confidence. |
| D8 | Region | New `ConsoleId.region: Option<String>` from the **first character of `CONTENT_ID`/`contentId`**: `U` USA, `E` Europe, `J` Japan, `H` Asia, `I` World; anything else → none. Primary source: shadPS4's own launcher, `GameListUtils::GetRegion(content_id[0])` (`shadps4-emu/shadps4-qtlauncher` `src/qt_gui/game_list_utils.h` at `24fd1248`). psdevwiki was not reachable (Cloudflare); Sony publishes no table. The region names are Coverly's existing `NameTags` region names, so the keeper's region order and the duplicates screen's region label (TODO 1.5) use it when the name has no region tag. | Owner: «Confirmar la tabla de prefijos con una fuente primaria». The emulator's code is the most primary source available; `K` (Korea) is not in it, so it is not guessed. |
| D9 | PS4 PKG | `sony::identify` gains the CNT magic `7F 43 4E 54` (header big-endian, LibOrbisPkg `PkgReader.ReadHeader`): content id at 0x40 (36 bytes; title id = bytes 7..16), entry count u32 at 0x10, entry table offset u32 at 0x18, content type u32 at 0x74, content flags u32 at 0x78. Entry table: 32-byte entries (id, filename offset, flags1, flags2, data offset, data size, 8 padding; **verify the entry layout and the encrypted bit in flags1 against LibOrbisPkg `MetaEntry` in the task**), at most 64 entries scanned. Entry `0x1000` (`param.sfo`): read when not flagged encrypted and ≤ 64 KiB, parsed by `psf::parse` → title, version, `CATEGORY`. Entry `0x1200` (`icon0.png`): an `ArtLocator::Range` when not flagged encrypted (the research marks it «generally plaintext, not verified»: the embedded source's PNG decode is the check; a non-PNG range yields no candidate). Kind by content type: `0x1A` GD → Base, unless flags have `FIRST_PATCH 0x00100000`, `SUBSEQUENT_PATCH 0x40000000`, `DELTA_PATCH 0x41000000` or `CUMULATIVE_PATCH 0x60000000` → Update; `0x1E` DP → Update; `0x1B` AC, `0x1C` AL → Dlc; the PSF `CATEGORY` wins when read. System: the PSF says PS4 (D5) or the title id starts with `CUSA` → `ps4`; `PPSA` → `ps5` (a standalone CNT envelope can be PS5, PkgToolBox); neither → magic-only `ps4` at 0.80 (review). New note `ItemNote::ExtractFirst` («Este paquete no se abre así: hay que extraerlo antes»). The body (PFS image) is never read. | Owner's offsets, completed from the research doc; catalogued, never called playable. |
| D10 | PS5 files | **Store PKG**: magic `7F 46 49 48` (`\x7FFIH`); embedded CNT offset LE u64 at FIH+0x58, the CNT header there must start with `7F 43 4E 54`; read content id (CNT+0x40) and content type (CNT+0x74) only. The entry table and `param.json` entry are **not** read in this batch (their ids are not verified; PkgToolBox refuses encrypted ones, and so does Coverly by not reading at all). Note `ItemNote::NoEmulator`. **`.exfat`** (exFAT image, ps5upload `client/src/api/fpkg.ts` at `21f40fd1`): weak extension; `ps5` only with the exFAT OEM name `EXFAT   ` at byte 3 **and** a `PPSA\d{5}` token in the file name, or inside a `ps5` folder; note `NoEmulator`. **`.ffpfs`, `.ffpfsc`** (PFS images for console-side mounting, same source): strong extensions, extension evidence only, note `NoEmulator`. **`.ffpkg`** (UFS2 image; loaded by Prosperity per TODO): strong, playable; UFS2 superblock magic `0x19540119` at 0x10000 + 0x55C proves it (FreeBSD `ufs/ffs/fs.h`; verify the offset in the task, else extension only). **`.zar`**: D12. **ZIP/RAR of the app**: D13. | Owner: «reconocerlos y dejarlos con la nota «ningún emulador lo carga así», nunca como jugables». They still move to `ps5/` (the item is identified correctly); the note says they will not run. |
| D11 | shadPS4 sibling folders (`-UPDATE`, `-patch`, `-mods`) | shadPS4 finds overlays **by folder name**: `OverlayPath(base, "-UPDATE")` is the base path + suffix (`src/core/file_sys/fs.cpp` at `55a97e46`; `-UPDATE` first, `-patch` only without `-UPDATE`; `-mods` always). So: (a) an `-UPDATE`/`-patch` folder with its own `sce_sys/param.sfo` is a folder game of kind Update (D7) grouped under its base (Tanda 2 D11) and moved beside it; (b) its target name is always **the base's final target name + the suffix**, so a collision rename (`CUSA00900 (2)`) or a frontend suffix (D16) carries over to the sibling; if that name is taken the base and its siblings stay together (`CollisionSet`); (c) a sibling whose base is not in the plan keeps its own name; (d) a `-mods` folder (loose replacement files, no `sce_sys/`) or an `-UPDATE`/`-patch` folder without a readable `param.sfo`, whose base sits beside it, is **left in place** with the new `LeftReason::GameOverlay { of }` («Carpeta de mods o parches de {title}: se deja donde está») and the walker does not descend into it. Mods move with their game in TODO 2.1. | Moving the base without renaming its sibling the same way breaks updates silently; walking a `-mods` folder would scatter its files into other systems. |
| D12 | `.zar` (ZArchive) | `zar` becomes **Shared** by `xbox360`, `ps4`, `ps5` (Batocera and RetroBat list it for PS4; shadPS4 `ResolveGameRoot` opens `<root>.zar`; SharpEmu for PS5; Xenia for 360). The batch 2 ZArchive reader decides from the tree (names only, one block for the metadata file, D24 budgets): `default.xex` at the root → `xbox360` (as today); `eboot.bin` + `sce_sys/param.sfo` → `ps4` (PSF through one block, ≤ `MAX_FILE_DECODED`); `eboot.bin` + `sce_sys/param.json` → `ps5`; `sce_sys/param.sfo` without `eboot.bin` → `ps4` DLC (shadPS4 accepts `.zar` DLC). Nothing decisive → review between the three. | A strong `.zar` on 360 would misfile every PS4 `.zar`. |
| D13 | Archives holding an app folder | Only for PS5 (Prosperity plays the ZIP/RAR as is): a zip/7z/rar whose entries hold `eboot.bin` and `sce_sys/param.json` at the root or under one top folder → a `ps5` **archive item taken as is** (`ps5` accepts `zip`, `7z`, `rar` containers), `param.json` read from the entry (≤ 64 KiB decoded, existing archive budgets; a RAR the current reader cannot open stays name-only). The same archive shape for PS4 (`param.sfo`) keeps batch 2's rule (§13 «Archives holding a whole folder game»): shadPS4 reads no zip, and unpacking a folder game is TODO 2.1-adjacent work. | Owner: «leídos con el lector de comprimidos actual cuando se pueda». |
| D14 | Dedupe keys | `title_of` uses, for `ps4`/`ps5`, the **full content id** as the title id of the key when present (`UP9000-CUSA00900_00-BLOODBORNE000000`), else the title id: same region and same label → same game; another region (`EP…`) or another DLC label → different keys. Kind and version stay in the key (Tanda 2): an update is never a copy of its base. Xbox: GDK `TitleId` (skip `FFFFFFFF`), else `StoreId`; ERA `Identity Name`; XVC `ContentId` GUID; version from `Identity Version` or `PackageVersion`. `TITLE_ID_SYSTEMS` gains the four ids. Folder-game duplicates stay **reported only** (batch 2 D28). | Owner: «clave de contenido por title ID + versión»; the content id is the title id plus the two things the owner says the title id alone misses (region, and here the DLC label). |
| D15 | Grouping key | `base_title_id` for `ps4`/`ps5` = the content id up to its label (`UP9000-CUSA00900_00`, the «service label 0» shadPS4 uses), else `TITLE_ID`; Xbox GDK DLC → the base whose `StoreId` is in the DLC's `AllowedProducts` (verified in Microsoft `Xbox-GDK-Samples` DLC configs at `cdbec9e9`); ERA and XVC → no grouping (no parent field in clear, research). | Same rule as Tanda 2, fed by the fields each format really has. |
| D16 | Frontend folder suffix and DLC folder (Batocera) | Batocera's `ps4` comment (`es_systems.yml` at `cc793b75`): a game folder renamed to end in `.ps4` shows in the frontend; `.zar` files go straight in `roms/ps4`; DLC goes in `roms/ps4/DLC` (its shadPS4 launcher sets `addon_install_dir` to it, `batocera_launch/emulators/shadps4.py`), and shadPS4 reads DLC from `<addon_install_dir>/<TITLE_ID>/<any folder or .zar>/`. So, on the Batocera profile only: `folder_game_suffix ps4 → ".ps4"` (batch 2 D13 mechanism, `FolderSuffix` note), the D11 siblings follow (`CUSA00900.ps4-UPDATE`, which Batocera does not list because its extension is not `.ps4`), and a PS4 DLC item goes to `ps4/DLC/<TITLE_ID>/<its folder name>` with the new note `ItemNote::DlcFolder { layout }` («Va a la carpeta DLC, donde {layout} busca el contenido descargable»). ES-DE: no suffix (its PS4 entries are `.ps4` serial files, shortcuts or a browsed `eboot.bin`; a renamed folder would be launched as a file), DLC beside the game as Tanda 2. This is a frontend's required path, not TODO 2.1's per-game structure. | Without it a moved PS4 game is invisible in Batocera and its DLC never loads. |
| D17 | Launcher files | `.ps4` (ES-DE «game serial» file: one line with `CUSA07010`) is text-native (batch 2 D14): read ≤ 64 bytes, a title id → `ps4`. Coverly never creates one. An empty `.ps4` marker inside a game folder moves with the folder. | Same rule as `.psvita`/`.psn`. |
| D18 | Xbox One and Series folders | **GDK** (`xbox::probe_gdk`): `MicrosoftGame.config` at the root (case-insensitive, ≤ 64 KiB, quick-xml, no DTD, no entities). Read `Identity/@Name`, `@Version`, `TitleId`, `StoreId`, `MSAAppId`, `ShellVisuals/@DefaultDisplayName`, `@Square480x480Logo`, `@StoreLogo`, `@Square150x150Logo`, every `ExecutableList/Executable/@TargetDeviceFamily`, the element `TargetDeviceFamilyForDLC` and `AllowedProducts/AllowedProduct`. In the samples `TargetDeviceFamilyForDLC` is a **child element** (`<TargetDeviceFamilyForDLC>Scarlett</…>`), not an attribute as TODO writes; Coverly reads the element and, defensively, an attribute of that name. System: every executable `XboxOne` → `xboxone`; every executable `Scarlett` → `xboxseries`; both → `DirConflict` (review); every executable `PC` → left `NotARom { what: PcGameData }` (existing copy «datos de un juego de PC»; never a console system); no executables (DLC): `TargetDeviceFamilyForDLC` `XboxOne`/`Scarlett`/`PC` likewise, absent → review. `XboxCompatibility` and `VirtualMachine/DlcCompatibility` are **never read for the system**. Kind: executables → Base, DLC config → Dlc. **ERA** (`xbox::probe_era`): `AppxManifest.xml` at the root whose root element `Package` declares the Xbox manifest namespace `http://schemas.microsoft.com/appx/2013/xbox/manifest` (verified in Microsoft `Xbox-ATG-Samples` XDK manifests at `ff629a6e`); without it, it is a Windows app, not a game folder. Read `Identity/@Name`, `@Publisher`, `@Version`, `Properties/DisplayName`, `Properties/Logo`, `Applications/Application/VisualElements/@DisplayName`, `@Logo`. `ms-resource:` names are **not** resolved (no `resources.pri` parser in this batch): title falls back to `VisualElements/@DisplayName` when not `ms-resource:`, else the folder name. ERA → `xboxone` Base (ERA is Xbox One only). | Owner's rules; the Scarlett rule is applied literally and nothing else makes a Series game. |
| D19 | XVC / MSIXVC files | `xbox::identify`: `msft-xvd` at 0x200 (file offset = 0x200 + xvdtool struct offset, research). Read ContentId (VDUID) GUID at 0x220, ContentType u32 at 0x284, ProductId GUID at 0x39C, PackageVersion 4 × u16 at 0x3BC, VolumeFlags at 0x208 (bit 2 = encryption disabled). Candidates: extensions `xvc`, `xvd`, `msixvc` (weak, Shared by `xboxone` and `xboxseries`) and **extensionless files named exactly like a GUID** (8-4-4-4-12 hex), ≥ 4 KiB, peeked for 0x208 bytes only. ContentType `0x01` Title → Base; `0x18`/`0x19`/`0x1A` → Dlc; `0x08`/`0x09`/`0x0A` (Xbox 360 backward compatibility) → Unknown kind; anything else (OS volumes `0x02 0x03 0x07`, updaters `0x0B 0x0C`, `0x06` apps, `0x1E` kiosk) → left `NotARom { what: ConsoleSystem }` (new, «archivo del sistema de una consola»). The plaintext header cannot tell One from Series and the Scarlett rule needs the encrypted config: an XVC goes to **review between `xboxone` and `xboxseries`** (equal header hits, the existing tie rule), unless it sits in one of their system folders («ya ordenado»). Note `NoEmulator`. Name: the file name (a GUID); title from no external source (D20). | Owner: «Solo da GUIDs … catalogar por GUID». |
| D20 | External name sources | **None added.** No public, licensed ProductId → name catalogue was found for Xbox (dbox.tools is a third-party site, no stated licence or API terms); no PS4/PS5 title-id DAT exists in libretro-database. Names come from the plaintext metadata (`TITLE`, `titleName`, `DefaultDisplayName`), else the folder or file name; covers by name from ScreenScraper and SteamGridDB (D21). Listed in §13. | Owner: «pedir el nombre a una fuente externa»; none passes the licence bar, so the owner decides. |
| D21 | Covers | (1) **Embedded first** for these four systems (owner's ask): new `systems.toml` boolean `embedded_first`; when the `embedded` source is enabled, fill tries it **before** the user's order for systems with the flag, and in the user's order otherwise. Turning `embedded` off still turns it off. Locators: PS4/PS5 folder `sce_sys/icon0.png` (512×512), PS4 PKG entry `0x1200`, GDK `Square480x480Logo` → `StoreLogo` → `Square150x150Logo`, ERA `VisualElements/@Logo` → `Properties/Logo`. Manifest paths use `\`, are relative, must stay inside the folder (no `..`, no root, no drive; else ignored) and may name an MRT base (`Logo.png` stored as `Logo.scale-200.png`): exact name first, then the largest `<stem>.scale-*.png` in that one directory (one listing, ≤ 256 entries). `embedded::SYSTEMS` gains the four ids. (2) libretro: `ps4` → `Sony - PlayStation 4` (verified live, 20 box arts; costs one lookup, finds Japanese releases); none for the others. (3) ScreenScraper `ps4` 60, `ps5` 284 (Skyscraper `platforms_idmap.csv` at `c253bfe0`), `xboxone` 34 (screenscraper.fr `systemeinfos.php?plateforme=34`, «Xbox One», checked 2026-11-01); `xboxseries`: no id found (ids 1–70 and 250–330 checked), left unset (§13). (4) SteamGridDB by name, as for any system. **Backgrounds** (`pic0.png`, `pic1.png`): Coverly has no background media slot (`Media = Cover | Screenshot | Title`); the locators are recorded in `ConsoleId.art` with a `role: Background` but not used until a slot exists (§13). Using `pic1` as a screenshot would be false. | Owner: «la portada incrustada como primera opción». |
| D22 | «Sin emulador» | New `systems.toml` boolean `no_emulator` (true for `xboxseries` only) → `SystemInfo.no_emulator` (Rust, IPC, TS, contract sample). The UI shows it as a quiet label beside the system's name (§9). `ps5` is not flagged: its folders run; only some of its files get the per-item `NoEmulator` note. `xboxone` is not flagged: WinDurango runs ERA folders. | Owner: «marcado como «sin emulador»». |
| D23 | Size confirmation | New `PlanWarning::LargeTransfer { bytes, items }` when the bytes the plan **copies** (copy mode, or a move across volumes, batch 2 D25) plus the bytes it **extracts** reach **10 GB** (decimal). A same-volume move is a rename and does not count. The apply button then opens a confirmation dialog (§9.1) that states the size, the item count and the free space; nothing is written until it is confirmed. Applies to every system (a 25 GB PS3 copy is the same risk), so batch 2 §13's «extra confirmation above a size» is answered by the owner's Tanda 3 ask. CLI: `coverly organize apply` refuses such a plan without `--yes-large` (prints the same sentence in English). | Owner: «no proponer extraer ni copiar sin confirmar el tamaño». 10 GB is roughly one large PS4 game and above every retro set. |
| D24 | Budgets | Batch 2 D24 unchanged (4 MiB read, 1 MiB decoded, 16 seeks, one named `read_dir` ≤ 256 entries). The tree summary of a folder item (batch 2 D27: metadata only, ≤ 100 000 entries, depth 16, «más de» past it) is the only pass over a big folder, and it reads no file content. `eboot.bin` is never opened (only its presence is checked by name). | Brief: «solo `sce_sys/` y cabeceras». |
| D25 | Emulator data | `EMULATOR_DATA_ROOTS` gains the shadPS4 user directory, recognised by holding both `sys_modules/` and `game_data/` (`src/common/path_util.h` at `55a97e46`: `SYSMODULES_DIR`, `METADATA_DIR`), and a loose `sys_modules/` folder (decrypted console firmware modules): both left `EmulatorData`. The `addcont/` tree **inside** that user dir is therefore never moved; an `addcont/` folder elsewhere is ordinary (its DLC label folders are folder games, D6/D7). | Moving an emulator's user dir breaks it; firmware is never Coverly's to move. |
| D26 | Default visibility | Batch 1 D18: always in pickers. Family `console` for all four. | Unchanged rule. |
| D27 | Icons | RetroArch Assets XMB Monochrome at the pinned `d9f96905` (CC BY 4.0, already attributed): `Sony - PlayStation 4`, `Microsoft - Xbox One`, `Non-Redump - Microsoft - Xbox Series X` (in the committed listing). PS5: no file → Slot mount fallback `FALLBACK_ABBR ps5 PS5`. Audit (batch 1 D20) may move a wordmark-only file to `WORDMARK_IDS` with `PS4`, `XB1`, `XBSX`. | No new licence. |
| D28 | Memoised facts | `FACTS_VERSION` 7 → 8 (`ConsoleId` gains `content_id`, `region`; new verdicts). | Batch 2 D23. |

## 5. Architecture and files touched

### 5.1 Rust

| File | Change |
|---|---|
| `consoles/mod.rs` | `ConsoleId.content_id`, `ConsoleId.region` (serde default, skipped when none); `ArtLocator` gains an optional `role` (`Cover` default, `Background`); `IdSource::{ParamJson, MicrosoftGameConfig, AppxManifest, XvdHeader, PkgParamSfo}`; `KEY_EXTS` + `cik`; `DIR_PROBES` order: ps3 JB, ps3 HDD, **ps4 app, ps5 app**, vita app, wiiu ×2, xbox xbe/xex/god, **xbox gdk, xbox era**, nx split; `base_title_id` arms for ps4/ps5 (D15); `region_from_content_id` (D8); `identify` order unchanged with `sony` and `xbox` extended. |
| `consoles/sony.rs` | `probe_ps4_app`, `probe_ps5_app`, PS4 signals in Vita probe/VPK (D5); `from_psf` PS4 categories (D7); CNT PKG (D9), FIH PKG (D10); `.ps4` launcher (D17); `param_json` parser (D6). Optional split into `consoles/orbis.rs` if `sony.rs` passes ~900 lines. |
| `consoles/xbox.rs` | `probe_gdk`, `probe_era` (D18), XVD header (D19), GUID-name check; MRT logo resolution (D21). |
| `consoles/zarchive.rs` | Tree classification helper for `zar` (D12). |
| `consoles/testing.rs` | Builders: PS4 PSF with `PUBTOOLINFO`/`APP_TYPE`, PS5 `param.json`, CNT/FIH PKG heads, XVD head, GDK/ERA manifests, PS4/PS5 ZArchive trees. |
| `systems/systems.toml`, `systems/mod.rs` | Four entries (§7); `embedded_first`, `no_emulator` fields (lenient user-file merge like batch 1's fields); `zar` Shared (xbox360 keeps it in its list); `pkg` weak on ps4/ps5. |
| `identify/sniff.rs` | Head-only hits: CNT, FIH, `msft-xvd` at 0x200, exFAT OEM name (only with the name token), UFS2 magic; `TEXT_NATIVE_EXTS` + `ps4`; `NotARom::ConsoleSystem`. |
| `organize/walk.rs` | GUID-named extensionless candidates (D19); shadPS4 user dir and `sys_modules` (D25); `-mods`/unreadable overlay siblings left `GameOverlay` without descent (D11). |
| `organize/evidence.rs` | Notes `ExtractFirst` (PS4 PKG), `NoEmulator` (PS5 store PKG, `.exfat`, `.ffpfs`, `.ffpfsc`, XVC), `ProEnhanced` (PS5 `attribute3 & (1 << 22)`, KytyPS5 `title_supports_trinity`, `videoOut.cpp` at `893b0355`); XVC tie → review. |
| `organize/plan.rs`, `organize/target.rs` | `LeftReason::GameOverlay { of }`; `ItemNote::{ExtractFirst, NoEmulator, ProEnhanced, DlcFolder { layout }}`; `PlanWarning::LargeTransfer { bytes, items }` (D23); sibling naming (D11); Batocera DLC path (D16); `FACTS_VERSION` 8. |
| `organize/apply.rs` | Refuses a `LargeTransfer` plan unless the request carries `confirmed_large: true` (IPC and CLI), so a stale UI cannot skip the dialog. |
| `profiles/builtin.rs` | Rows of §6; `ps4` suffix on Batocera only (D16); `dlc_folder` map (`ps4 → "DLC/{title_id}"`, Batocera only); `ps5` container overrides `zip 7z rar` as is (D13). |
| `dedupe/keys.rs` | `TITLE_ID_SYSTEMS` + four; `title_of` content-id rule (D14); Xbox keys. |
| `dedupe/keeper.rs` | Region fallback from `ConsoleId.region` (D8). |
| `sources/embedded.rs`, `fill/mod.rs` | `SYSTEMS` + four; `embedded_first` ordering (D21); MRT logo lookup; `role: Background` locators skipped. |
| `scan/mod.rs` | Folder games of the new probes are one `RomEntry` (batch 2 D26, no new code beyond the probes); `ps4/DLC/` holds folder games, not a system. |
| `crates/coverly-cli/src/organize.rs` | English text for `GameOverlay`, `ConsoleSystem`, the four notes, `LargeTransfer`; `--yes-large`. |
| `src-tauri/src/ipc.rs` | `SystemInfo.no_emulator`; `confirmed_large` on the apply request. |

### 5.2 Front end

| File | Change |
|---|---|
| `src/ipc/types.ts`, `contract.sample.json`, `contract.test.ts` | `SystemInfo.no_emulator: boolean`; `LeftReason` `game-overlay`; `NotARom` `console-system`; `ItemNote` `extract-first`, `no-emulator`, `pro-enhanced`, `dlc-folder`; `PlanWarning` `large-transfer`; apply request `confirmed_large`. |
| `src/lib/organize.ts` | Copy for the new reasons, notes and `why` sources (§10). |
| `src/screens/OrganizeScreen.tsx` | `large-transfer` warning line; apply opens the confirmation (§9.1) when the plan has the warning. |
| `src/components/organize/LargeTransferDialog.tsx` (new) | Built on `src/components/ui/alert-dialog.tsx`. |
| `src/components/organize/SystemGroups.tsx`, library shelf header | «Sin emulador» label when `no_emulator` (§9.1). |
| `src/components/system/systemFallbacks.ts`, `scripts/icons/systemIconsLib.mjs`, generated icons, `THIRD-PARTY-NOTICES.md` | Three silhouettes, one fallback (D27). |
| `locales/{es,en}/ui.json` | §10 keys, sorted in place (parallel workflows edit these files: resolve key by key). |
| `src/ipc/mock/mockOrganize.ts`, `fixture.ts`, `mockSystems` | `?mockPick=current` pile (§9.3); `no_emulator` on `xboxseries`; shelves fixture gains `PS4`, `XBOXSERIES`. |

## 6. Per-profile folders (research result)

Legend as batch 2 §6: a name = `Named`; `—` = `Absent` (the existing «{layout} no tiene carpeta para {system}; se crea {id}.» note). RetroArch keeps `Id` (no core for any of the four). Onion/GarlicOS, MinUI/NextUI, Anbernic stock and muOS: `—` for all four (no package, pak, card folder or manifest; all are ARM handheld systems; muOS manifests at `0784148f` have none).

| id | ES-DE | Batocera [Knulli] | RetroBat (reference, no profile) | Notes |
|---|---|---|---|---|
| ps4 | `ps4` | `ps4` (+ `.ps4` folder suffix, + `DLC/<TITLE_ID>/`, D16) [Knulli `ps4`, extensions `ps4`] | `ps4` | ES-DE extensions `.bin .ps4` (+ `.desktop`/`.lnk` shortcuts); RetroBat `.ps4 .zar .m3u .lnk` |
| ps5 | — | — | — | No frontend has a PS5 system. On every profile the folder is created as `ps5` with the existing note. |
| xboxone | `xboxone` | — | — | ES-DE: «Placeholder» system, extensions `.7z .zip` only: a moved ERA/GDK folder is stored correctly but ES-DE does not list it (PR mismatch list). |
| xboxseries | — | — | — | Folder `xboxseries`, «sin emulador» (D22). |

Sources: ES-DE `resources/systems/{linux,macos,windows}/es_systems.xml` and `USERGUIDE.md` «Sony PlayStation 4» at `ef267ae6` (master unchanged since batch 2); Batocera `es_systems.yml` and `python-src/batocera-launch/batocera_launch/emulators/shadps4.py` at `cc793b75`; Knulli `es_systems.yml` at `ab3a6809` (`knulli-main`); RetroBat `system/templates/emulationstation/es_systems.cfg` at `54e5e31d`. LaunchBox and Pegasus have no Coverly profile; their names are not used. Emulator «launchers» (shadPS4, KytyPS5, SharpEmu GUIs) are not frontends with a ROMs folder convention: shadPS4 takes any game directory (`MntPoints`) and an `addcont` directory set in its config; nothing to encode.

## 7. System entries (research result)

| id | name | aliases (first = batch 1 D8) | extensions | weak | Shared | libretro | dats | SS | flags |
|---|---|---|---|---|---|---|---|---|---|
| ps4 | Sony PlayStation 4 | PS4, playstation4 | ps4 | pkg | zar | Sony - PlayStation 4 | — | 60 | `embedded_first` |
| ps5 | Sony PlayStation 5 | PS5, playstation5 | ffpkg, ffpfs, ffpfsc | pkg, exfat | zar | — | — | 284 | `embedded_first` |
| xboxone | Microsoft Xbox One | XBOXONE, xb1 | — | — | xvc, xvd, msixvc | — | — | 34 | `embedded_first` |
| xboxseries | Microsoft Xbox Series X\|S | XBOXSERIES, xsx, xboxseriesx | — | — | xvc, xvd, msixvc | — | — | — | `embedded_first`, `no_emulator` |

Changes to existing entries: `xbox360` keeps `zar` but it becomes Shared (D12). Global aliases: `ps4`, `ps5`, `xboxone`, `xboxseries`; scoped (batch 1 D22): none (no everyday word). `multi_file = false` for all four (no disc images, D1). Folder games carry no extension. Total after the batch: **131 systems**.

## 8. Identification

### 8.0 Shared shape

`ConsoleId` (batch 2 §8.0) plus:

```rust
pub content_id: Option<String>, // PS4/PS5 36-char content id; XVC ContentId GUID
pub region: Option<String>,     // D8: "USA" | "Europe" | "Japan" | "Asia" | "World"
```

Kind table additions (D6, D7, D9, D18, D19):

| System | Base | Update | DLC | Demo |
|---|---|---|---|---|
| ps4 (PSF) | `CATEGORY gd` | `gp*`, folder `-UPDATE`/`-patch` | `ac` | `APP_TYPE 3` |
| ps4 (PKG) | type `0x1A` | `0x1A` + patch flag, `0x1E` | `0x1B`, `0x1C` | via PSF |
| ps5 (param.json) | `eboot.bin` present | `originContentVersion`/`targetContentVersion` | no `eboot.bin` | — |
| xboxone / xboxseries | GDK executables, ERA, XVC `0x01` | — (an update replaces the package; same key, higher version) | GDK DLC config, XVC `0x18–0x1A` | — |

### 8.1 PS4

| Form | Probe / magic | Read | Id |
|---|---|---|---|
| App folder `CUSA…/` | `sce_sys/param.sfo` + PS4 signal (D5) | that file (≤ 64 KiB) | `TITLE_ID`, `CONTENT_ID` (region D8), `TITLE`, `APP_VER`, kind D7; art `sce_sys/icon0.png` (Cover), `sce_sys/pic1.png` (Background, recorded only) |
| Overlay `CUSA…-UPDATE/`, `-patch/` | same probe | same | kind Update; D11 naming |
| `-mods/`, overlay without `param.sfo` | name + base sibling | names only | left `GameOverlay` |
| DLC label folder | `sce_sys/param.sfo` with `CATEGORY ac` | same | kind Dlc; Batocera D16 |
| PKG / fPKG | `7F 43 4E 54` | header 0x80, entry table ≤ 64 × 32 B, entry `0x1000` ≤ 64 KiB | D9; note `ExtractFirst` |
| `.zar` | ZArchive footer | names, tree, one block | D12 |
| `.ps4` text | text-native | ≤ 64 bytes | D17 |

### 8.2 PS5

| Form | Probe / magic | Read | Id |
|---|---|---|---|
| App folder `PPSA…/` | `sce_sys/param.json` | that file (≤ 64 KiB) | D6; art `sce_sys/icon0.png`, `pic0.png` (Background); `ProEnhanced` note |
| Store PKG | `7F 46 49 48` → CNT | 0x60 + 0x80 bytes | content id, type; `NoEmulator` |
| `.ffpkg` | UFS2 magic (D10) | 4 bytes at 0x1055C | name; playable |
| `.ffpfs`, `.ffpfsc` | extension | nothing | name; `NoEmulator` |
| `.exfat` | `EXFAT   ` at 3 + `PPSA` token | 11 bytes | name; `NoEmulator` |
| `.zar` | ZArchive | D12 | as folder |
| zip/7z/rar of the app | archive entries (D13) | `param.json` entry | as folder; taken as is |

### 8.3 Xbox One and Series

| Form | Probe / magic | Read | Id |
|---|---|---|---|
| GDK folder | `MicrosoftGame.config` | that file (≤ 64 KiB) | D18; `TitleId` (8 hex, `FFFFFFFF` ignored), `StoreId`, `MSAAppId`, `Identity Version`, `DefaultDisplayName`; logos D21 |
| ERA folder | `AppxManifest.xml` + Xbox namespace | that file (≤ 64 KiB) | D18; `Identity Name/Version`, display name rule; logos D21 |
| XVC / MSIXVC | `msft-xvd` at 0x200 | 0x3C4 bytes | D19 |

`resources.pri`, the XVC metadata pages (`XvcInfo`), and everything inside an XVC are never read.

### 8.4 Budgets and safety

Batch 2 §8.6 applies to every reader. Additions: `param.json` and the manifests are parsed only after the size check; JSON numbers are read as `i64` and narrowed with checks; XML with a DOCTYPE is rejected; manifest-relative paths are normalised component by component (`\` and `/`), and any `..`, root, prefix or symlink component drops the locator. `eboot.bin` and `.exe` files are never opened. A GUID-named extensionless file is opened only after `is_key_material` and only for 0x208 bytes.

### 8.5 Hash and name

Nothing of this batch is hashed (no DAT exists; every file is far over the 64 MiB cap). `NAME_ONLY_EXTS` gains `ps4 ffpkg ffpfs ffpfsc exfat xvc xvd msixvc`. Matching is header id, then name.

## 9. UX and UI design

Visible change: four systems with silhouettes (one Slot mount), PS4/PS5/Xbox folder rows that already render as batch 2 folder games, a handful of notes, one quiet «Sin emulador» label, and one confirmation dialog before a large copy. No new screen, no new token.

### 9.1 Design plan (frontend-design pass)

- **Colour** (existing tokens only; light values shown): `--cr-bg #f2f3ef` (greyboard page), `--cr-surface #fbfbf9` (dialog panel), `--cr-text #1b1e1b`, `--cr-text-secondary #535a52` (notes and the «Sin emulador» label), `--cr-primary #0e5e6f` (the confirm button only), `--cr-border #d3d7cf`. Dark mode uses the same roles. No `--cr-warning` and no Manila tag for «sin emulador» or «ningún emulador lo abre así»: they describe the world, not a fault in the user's files. `pnpm tokens:check` stays green.
- **Type**: Atkinson Hyperlegible Next for names, notes and dialog body; **Atkinson Hyperlegible Mono only for real data**: content ids (`UP9000-CUSA00900_00-BLOODBORNE000000`), title ids, GUIDs, sizes (`187,42 GB`), folder names in notes (`CUSA00900.ps4-UPDATE`). The dialog title is set in the UI face at the existing dialog title size, not Archivo display: a confirmation is a question, not a shelf label.
- **Layout**:

  ```
  Sony PlayStation 4                                            3
    ▢ Bloodborne (CUSA00900)              48,12 GB  → ps4/CUSA00900.ps4
      Title ID CUSA00900 en PARAM.SFO
      Se renombra a CUSA00900.ps4 para que Batocera lo muestre
    ▢ Bloodborne (CUSA00900-UPDATE)       14,80 GB  → ps4/CUSA00900.ps4-UPDATE
      Actualización de Bloodborne
    ▤ UP9000-CUSA03173_00-….pkg            9,02 GB  → ps4/
      Este paquete no se abre así: hay que extraerlo antes

  Microsoft Xbox Series X|S   Sin emulador                      1
    ▢ Forza Horizon 5                     110,40 GB  → xboxseries/…
  ```

  The «Sin emulador» label sits on the system heading's baseline, after the name, in secondary text at caption size, sentence case, no box, no icon. It appears once per group (and on the library shelf header), never on each row. Notes stay one sentence per fact under the row, as batch 2.

  Confirmation dialog (AlertDialog, focus on «Cancelar» first, Escape cancels):

  ```
  ┌───────────────────────────────────────────────┐
  │ ¿Copiar 187,42 GB?                            │
  │                                               │
  │ Organizar va a copiar 3 juegos a TinyExt.     │
  │ Quedan 412,10 GB libres en ese disco.         │
  │                                               │
  │                   [Cancelar] [Copiar 187,42 GB]│
  └───────────────────────────────────────────────┘
  ```

  Left-aligned text, buttons right-aligned in the existing dialog footer order. The primary button repeats the size, so the action names exactly what happens.
- **Motion**: the dialog uses the existing AlertDialog open/close (`--cr-ease-standard`); nothing new, and `prefers-reduced-motion` already removes it.
- **Principle**: say what the file is and what will happen, never what to install. Trust stays legible as in batch 2 («en PARAM.SFO», «en param.json», «en MicrosoftGame.config»).

Review against generic defaults: the default would be a red «Unsupported» pill on Xbox Series, a warning triangle on every store PKG, and a modal with a big number, a progress-bar illustration and «Are you sure?». Rejected: a pill and triangle wall turns facts into alarms (and «Unsupported» would be wrong: Coverly supports the files, emulators do not exist); a big-number hero is the generated-dashboard tell; «¿Seguro?» says nothing. The chosen dialog title is the question with its exact number, the body names the destination and the free space, and the button repeats the number. The one deliberate typographic move is the mono size inside the sentence, which the brand already does for data.

Accessibility: «Sin emulador» is text (part of the heading's accessible name: «Microsoft Xbox Series X|S, sin emulador»); notes are plain text in the row description; the dialog is a labelled `alertdialog` with the title as its name and the body as its description, keyboard-trapped, Escape and «Cancelar» both cancel, focus returns to the apply button. Sizes are read as text.

### 9.2 Icons

D27. The audit contact sheet (batch 1 D20) covers the three RetroArch files. «Xbox» and «PlayStation» appear only as the systems' display names; no other branding (`TRADEMARKS.md`).

### 9.3 Mock

`?mockPick=current` (beside `downloads|loose|mixed|retro|modern`), synthetic names and sizes:
- `CUSA00900/` (PS4 folder, 48,12 GB, `UP9000-CUSA00900_00-…`, region USA), `CUSA00900-UPDATE/` (Update, 14,80 GB), `CUSA00900-mods/` (left, game overlay);
- `EP0002-CUSA04447_00-….pkg` (PS4 PKG, Europe, extract-first), `Gravity Rush.zar` (PS4 ZArchive);
- `PPSA01284/` (PS5 folder, Pro-enhanced note), `PPSA01284 DLC/` (no `eboot.bin`: DLC), `UP9000-PPSA01284_00-….pkg` (store PKG, no-emulator), `PPSA09519.exfat` (no-emulator);
- `Forza Horizon 5/` (GDK, `Scarlett` → xboxseries), `Halo 5 Guardians/` (GDK, `XboxOne`), `Sunset Overdrive/` (ERA), `Microsoft Flight Simulator/` (GDK, `PC` → left, PC game data), `4f0b3e0a-…-c1d2e3f4a5b6` (XVC → review between Xbox One and Series);
- `shadPS4/` holding `sys_modules/`, `game_data/`, `addcont/` (left: emulator data); `UP9000-CUSA00900_00.rif`, `x.cik` (key material).

The plan copies more than 10 GB across volumes in the mock target, so the confirmation dialog opens.

## 10. Copy (es / en)

Spanish: sentence case, no exclamation marks, exact counts, numbers and codes in mono by the renderer. No emulator names, no URLs (D4).

| Key | es | en |
|---|---|---|
| `organize.note.extractFirst` | Este paquete no se abre así: hay que extraerlo antes | This package does not open as is: it has to be extracted first |
| `organize.note.noEmulator` | Ningún emulador lo abre así | No emulator opens it in this form |
| `organize.note.proEnhanced` | Mejorado para PS5 Pro | Enhanced for PS5 Pro |
| `organize.note.dlcFolder` | Va a la carpeta DLC, donde {layout} busca el contenido descargable | Goes to the DLC folder, where {layout} looks for downloadable content |
| `organize.left.gameOverlay` | Carpeta de mods o parches de {title}: se deja donde está | Mods or patch folder of {title}: left where it is |
| `organize.left.consoleSystem` | archivo del sistema de una consola | console system file |
| `organize.why.contentIdHeader` | Content ID {id} en la cabecera del paquete | Content ID {id} in the package header |
| `organize.largeTransfer` | Va a copiar {bytes}; se pedirá confirmación antes de empezar | It will copy {bytes}; you will be asked to confirm before it starts |
| `organize.largeTransfer.title` | ¿Copiar {bytes}? | Copy {bytes}? |
| `organize.largeTransfer.body` | Organizar va a copiar {count} juegos a {volume}. Quedan {free} libres en ese disco. | Organize will copy {count} games to {volume}. {free} is left free on that drive. |
| `organize.largeTransfer.bodyOne` | Organizar va a copiar 1 juego a {volume}. Quedan {free} libres en ese disco. | Organize will copy 1 game to {volume}. {free} is left free on that drive. |
| `organize.largeTransfer.confirm` | Copiar {bytes} | Copy {bytes} |
| `organize.largeTransfer.cancel` | Cancelar | Cancel |
| `system.noEmulator` | Sin emulador | No emulator |

Reused unchanged: `organize.why.titleIdFile` with `{file}` from the fixed set (adds `param.json`, `MicrosoftGame.config`, `AppxManifest.xml`; `PARAM.SFO` also for the PKG entry), `organize.why.titleIdName`, `organize.note.update`, `organize.note.dlc`, `organize.note.folderSuffix`, `organize.left.notARom` with `pcGameData` (PC builds) and the new `consoleSystem`, `organize.left.keyMaterial`, `organize.left.emulatorData`, `organize.noSpace`, `organize.item.folder`. `{volume}` is the target volume's display name from the existing plan target; `{bytes}`/`{free}` use `formatDecimalSize`. Singular forms use the `…One` key convention (`translate` has no ICU plural). «¿Copiar …?» opens with the Spanish inverted mark; it is a question, not an exclamation.

## 11. Data flow

1. **Load**: `SystemDb` parses `embedded_first`, `no_emulator` (lenient for user files); validation checks `zar`, `xvc`, `xvd`, `msixvc` as Shared and `pkg`/`exfat` as weak.
2. **Walk**: key material by name (D4) → left; shadPS4 user dir / `sys_modules` → `EmulatorData` (D25); probes in the batch 2 D12 order (+ D5, D6, D18) → folder items; `-mods` or unreadable overlay beside a base → `GameOverlay` (D11); GUID-named extensionless files → candidates (D19).
3. **Inspect**: sniff (CNT, FIH, `msft-xvd`, UFS2, exFAT) → `consoles::identify` (budgeted); archives: ZArchive tree (D12), PS5 app archives (D13).
4. **Evidence**: header hits (0.95/0.90), the XVC tie → review, PC builds → `NotARom`, notes `ExtractFirst`/`NoEmulator`/`ProEnhanced`.
5. **Grouping**: `base_title_id` (D15) → updates and DLC after their base (Tanda 2), summary count.
6. **Target**: §6 folders; Batocera `.ps4` suffix and `DLC/<TITLE_ID>/` (D16); sibling names from the base's final name (D11); D27 gates per contained file; `NoSpace`; `LargeTransfer` (D23).
7. **Apply**: UI confirmation → `confirmed_large` → batch 2 D25 transfers.
8. **Library scan, fill, gamelists**: one `RomEntry` per folder game; fill tries `embedded` first for the four systems (D21), then libretro (PS4 only), ScreenScraper, SteamGridDB.
9. **Dedupe**: content-id keys (D14), reported only for folder games.
10. **UI**: rows, notes, «Sin emulador», dialog (§9).

## 12. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| Vita app with a non-`CUSA` id and no PS4 keys | Vita, as today (D5). |
| PS4 PSF with both `PUBTOOLINFO` and `PSP2_SYSTEM_VER` | Contradiction → `DirConflict` (review). |
| `CUSA00900-UPDATE` without its base in the pile | Moved with its own name, note «Actualización de CUSA00900» (title id when the base is absent, Tanda 2). |
| Base collides at the target (`CUSA00900` exists) | Base → `CUSA00900 (2)`, sibling → `CUSA00900 (2)-UPDATE`; if that is taken too, the group stays (`CollisionSet`). |
| `-mods` folder with no base beside it | Ordinary folder: walked (it holds no ROM extensions in practice → other files). |
| Both `-UPDATE` and `-patch` present | Both move (shadPS4 uses `-UPDATE` and ignores `-patch`; Coverly does not decide which the user wants); dedupe reports two updates of one base if their versions match. |
| PS4 DLC on ES-DE | Moved beside the game in `ps4/`, note «Contenido descargable de …»; whether ES-DE users' shadPS4 sees it depends on its `addcont` setting (§13). |
| PS4 DLC on Batocera whose base title id is unknown | `ps4/DLC/<its own TITLE_ID>/…` (the DLC's `TITLE_ID` is the base's). No `TITLE_ID` → left in `ps4/` with the plain DLC note. |
| PKG entry `0x1000` flagged encrypted or over 64 KiB | Not read; header-only verdict (content id, type). |
| PKG with a non-PSF `0x1000` entry | `psf::parse` fails → header-only verdict. |
| CNT PKG with neither `CUSA` nor `PPSA` and no PSF | `ps4` at 0.80 → review. |
| `param.json` invalid, BOM, UTF-16, > 64 KiB | Not a PS5 probe hit; the folder is walked normally (no ROMs inside → other files). |
| `param.json` with `attribute3` as a string | Ignored (no Pro note). |
| PS5 folder without `eboot.bin` that is a base (corrupt dump) | Classified DLC (D6 heuristic); the note says DLC of the base id. Listed as a known limit. |
| `MicrosoftGame.config` with `TitleId FFFFFFFF` | Title id ignored; `StoreId` keys dedupe. |
| GDK config with `XboxCompatibility` or `DlcCompatibility` = `Scarlett` and executables `XboxOne` | `xboxone` (never read for the system, D18). |
| GDK config with PC and XboxOne executables | Mixed families → review (`DirConflict` naming `xboxone` and «PC»). |
| `AppxManifest.xml` of a Windows UWP app (no Xbox namespace) | Not a game folder. |
| ERA `DisplayName` `ms-resource:AppName` | Title from `VisualElements/@DisplayName` if plain, else the folder name. |
| Manifest logo `..\..\secret.png` or `C:\x.png` | Locator dropped. |
| GUID-named file that is not an XVC (a cache blob) | No magic at 0x200 → ignored (other files). |
| XVC under `xboxone/` | «Ya ordenado» (existing). |
| XVC ContentType `0x0B` (system updater) | Left `NotARom { console-system }`. |
| `.zar` with `default.xex` and `eboot.bin` | Two systems → review. |
| `.zar` over the ZArchive budget | Magic only → review among `xbox360`, `ps4`, `ps5`. |
| `.exfat` without a `PPSA` token, outside `ps5/` | Unknown (other files). |
| 300 GB of PS5 folders copied to a card | `LargeTransfer` → dialog; `NoSpace` too when it does not fit (the dialog still shows; apply stays blocked by `NoSpace` as today). |
| Same-volume move of a 100 GB folder | Rename: no dialog. |
| Plan exported and applied from the CLI | Refused without `--yes-large`. |
| A user's UI applies a plan whose `LargeTransfer` appeared after a refresh | The backend refuses an apply without `confirmed_large` → the dialog opens. |
| Folder with > 100 000 entries | Size «más de» (batch 2 D27); the dialog shows the same lower bound the folder row shows. |
| `sce_sys/license.dat`, `*.rif` inside a game folder | Move with the folder, never read. |
| Fake emulator file in the pile (`PCSX4_setup.exe`) | Not rom-like: other files; Coverly says nothing about it (no name-shaming list in the app). |
| Windows | Probe paths with `Path::join`; manifest `\` paths split by component; no new folder name is reserved; `.ps4` and `-UPDATE` suffixes valid on FAT32/exFAT/NTFS; fixtures pin mtimes and compare paths by component. |
| Old facts | `FACTS_VERSION` 8 re-reads lazily. |
| Plans written by this version opened by an older one | Unknown variants refuse the plan (existing). |

## 13. Blocked, deferred and out of scope

Needs an owner decision (implemented around; listed in the PR):
- **External name source for Xbox GUIDs and PS4/PS5 ids** (D20): none added.
- **Xbox Series ScreenScraper id**: not found; covers by name from SteamGridDB only.
- **Background art** (`pic0`/`pic1`, Xbox splash): recorded, unused until a background media slot exists (D21).
- **Embedded first** changes the cover chosen for these four systems even when a user put another source first (D21, owner's ask); a per-system override in Settings is not built.
- **Size threshold** 10 GB for the confirmation, and its application to every system (D23).
- **PS4 DLC on ES-DE and RetroArch**: placed beside the game (Tanda 2); the standalone shadPS4 reads DLC from its own `addcont` folder. Whether Coverly should target that (or TODO 2.1's `DLC/` subfolder) is 2.1.
- **Overlay folders on Batocera** named `CUSA00900.ps4-UPDATE` (D16): derived from shadPS4's code; to be confirmed on a Batocera install by the owner.
- **Updates and DLC as library entries**: like Tanda 2, the library scan lists each folder game, so ES-DE shows `CUSA00900-UPDATE` as its own folder and fill may look for its cover. Hiding them is 2.1.
- **Zipped PS4 app folders** (D13): not unpacked.
- **ERA `ms-resource:` names**: no `resources.pri` parser.
- **ES-DE `xboxone` placeholder** lists only `.7z .zip`: moved folders are invisible there.
- **Unverified items** (each checked in its task; dropped and listed if they cannot be): PS4-only/Vita-only PSF keys (D5), the PKG entry layout and encrypted bit (D9), the UFS2 superblock offset (D10), the `.ffpfs`/`.ffpfsc` formats (extension only by design).

Out of scope: TODO 2.1 (per-game `DLC/`, `Updates/`, `Mods/`, `Cheats/`, `Extras/`), sync, PS4/PS5/Xbox One disc images, extracting or installing PKGs, resolving `resources.pri`, reading `XvcInfo`, any decryption, licences, firmware or `sys_modules`, naming or linking emulators, the landing page.

## 14. Test plan

### 14.1 Rust (`cargo test --workspace`)

Fixtures are synthetic bytes and XML/JSON built in the tests (no copyrighted data, no keys), read through the in-memory `SparseAt` where offsets are far; disk tests use `tempfile` or `target/` with pinned mtimes and paths compared by component.

- `consoles::sony` (PS4/PS5): PS4 folder by `CUSA`; PS4 folder by `PUBTOOLINFO`; Vita folder unchanged; contradiction → conflict; kinds `gd`/`gp`/`ac`/`APP_TYPE 3`; `-UPDATE` without `CATEGORY`; region from each prefix letter and an unknown one; CNT PKG: content id, title id, type/flag kinds, `0x1000` PSF read, encrypted `0x1000` **not read** (a `RangeLog` proves the range was never touched), `0x1200` locator; FIH PKG: CNT at the LE offset, entry table never read (`RangeLog`); `param.json`: fields, default language, `en-US` fallback, BOM, invalid JSON, oversize, `attribute3` bit 22 → Pro; DLC by missing `eboot.bin`; patch by `targetContentVersion`; `.ps4` launcher text.
- `consoles::xbox`: GDK `XboxOne`, `Scarlett`, `PC`, mixed → conflict; DLC config element and attribute forms; `XboxCompatibility="Scarlett"` with `XboxOne` executables stays `xboxone`; `FFFFFFFF`; `AllowedProducts` grouping; ERA with and without the Xbox namespace; `ms-resource:` fallback; DOCTYPE rejected; logo path traversal dropped; MRT `scale-200` resolution; XVD header fields, ContentType mapping, GUID-name rule, non-XVC GUID file.
- `consoles::zarchive`: PS4, PS5, Xbox 360, mixed and over-budget trees.
- `consoles::is_key_material`: `x.cik`; `license.dat` not listed.
- **Budgets**: each new reader on its largest fixture stays within D24; an adversarial PKG entry count (2^31) stops at 64 entries.
- `systems`: four entries load; `embedded_first`/`no_emulator` lenient merge; extension-class snapshot diff from `main` is exactly §7 (`zar` Shared on three systems, `pkg` weak on four, `xvc xvd msixvc` Shared on two); 131 systems.
- `profiles`: every built-in × every new system returns §6's folder or the `Id` rule with `on_device = Some(false)`; Batocera `ps4` suffix and `dlc_folder` only on Batocera.
- `organize_walk`: shadPS4 user dir and `sys_modules` → `EmulatorData`; `-mods` beside a base → `GameOverlay`, not descended (the open log shows no file under it opened); GUID file peeked for 0x208 bytes only.
- `organize_target`: Batocera `CUSA00900` → `ps4/CUSA00900.ps4`, sibling → `ps4/CUSA00900.ps4-UPDATE`, DLC → `ps4/DLC/CUSA00900/<name>`; ES-DE no suffix; collision carries to the sibling; group stays when the sibling name is taken; PS5 on every profile → `ps5/` with the no-folder note; PS5 zip taken as is.
- `organize_evidence`: XVC → review between `xboxone` and `xboxseries`; PC build → `NotARom::PcGameData`; notes `ExtractFirst`, `NoEmulator`, `ProEnhanced`.
- `organize_plan`: `LargeTransfer` at 10 GB across volumes (size faked through the metadata probe, never written), absent for a same-volume move, present for copy mode and extraction; `apply` refuses without `confirmed_large`; facts version 7 → re-read.
- `dedupe`: same content id + kind + version → duplicates (reported only for folders); two DLC labels → not duplicates; `UP` vs `EP` → not duplicates; update never a copy of its base; keeper uses `region` when the name has none.
- `sources::embedded`: PS4 `icon0.png`, PS5 `icon0.png`, GDK `Square480x480Logo`, ERA logo, PKG `0x1200` range; a `Background` locator yields no candidate; `embedded_first` order in fill and the user's order for other systems; disabled `embedded` stays off.
- `sources::libretro`: `Sony - PlayStation 4` is in the committed listing (refreshed in the task from the live root; README date updated).
- `organize_plan` snapshots: `organize_plan__systems_batch_3__{esde,batocera}.snap` over the §9.3 pile (built under `target/` with `cp -RXp`, no `._` files, removed afterwards).
- CLI: text for every new variant; `--yes-large` gate.

### 14.2 Vitest (`pnpm check-all`)

- `organize.ts`: copy for `game-overlay`, `console-system`, each new note, `contentIdHeader`, `titleIdFile` with `param.json`/`MicrosoftGame.config`/`AppxManifest.xml`.
- `LargeTransferDialog`: title and button carry the formatted size (`187,42 GB` es, `187.42 GB` en); `bodyOne` at one game; Escape and «Cancelar» cancel without calling apply; confirm calls apply with `confirmed_large`; focus starts on «Cancelar» and returns to the apply button.
- `SystemGroups`: «Sin emulador» renders for `no_emulator` systems only and is part of the heading's accessible name.
- **Copy guard**: no value in `locales/{es,en}/ui.json` contains `http`, `PCSX4`, `PCSX5`, `Rexbox`, `KytyPS5`, `SharpEmu`, `AnyPS5`, `Prosperity`, `shadPS4` or `WinDurango` (Tanda 2's `Azahar`, `RPCS3`, `Vita3K` values are `{emulator}` code constants, not locale text, and are untouched).
- Contract test with the new members; i18n parity; Spanish copy check (sentence case, no «!»).
- `mockOrganize`: `?mockPick=current` resolves and produces the §9.3 rows and the `large-transfer` warning.
- `SystemIcon`: `ps4`, `xboxone`, `xboxseries` silhouettes (or audited mounts), `ps5` Slot mount `PS5`.

### 14.3 E2E (Chrome DevTools MCP, isolated context `systems-batch-3`, dev server on port **1467**)

1. `?mockPick=current` → plan: PS4, PS5, Xbox One, Xbox Series groups with icons; `CUSA00900` folder row with its size, `-UPDATE` directly under it with «Actualización de Bloodborne»; the PS4 PKG with «hay que extraerlo»; the PS5 store PKG and `.exfat` with «Ningún emulador lo abre así»; `PPSA01284` with «Mejorado para PS5 Pro».
2. Xbox Series heading shows «Sin emulador»; the XVC sits in review with Xbox One and Xbox Series as choices; Flight Simulator is left as «datos de un juego de PC».
3. Left in place: `CUSA00900-mods` (overlay), `shadPS4/` (emulator data), `.rif` and `.cik` (key material).
4. Switch layout to Batocera: `CUSA00900.ps4`, `CUSA00900.ps4-UPDATE`, DLC under `ps4/DLC/CUSA00900/`; to ES-DE: no suffix.
5. Apply → the confirmation dialog: keyboard only (Tab, Escape cancels, Enter on the confirm button applies); `take_snapshot` shows an `alertdialog` named «¿Copiar 187,42 GB?».
6. Library shelves with `PS4` and `XBOXSERIES`: light and dark, `prefers-reduced-motion: reduce`, 1280 px and the narrowest supported width; screenshots in the PR.

Stop the server only by port (`lsof -ti tcp:1467 -sTCP:LISTEN | xargs -r kill`). Fallback: Playwright MCP, then `playwright-core` + Chrome for Testing in the scratchpad (`…/scratchpad/systems-batch-3`).

### 14.4 Gates

`pnpm check-all`, `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`. `~/Documents/ROMS`, `/Volumes/TinyExt/*` and `/Volumes/System` are read-only: any real-header check copies a file's head with `cp -RXp` under the worktree's `target/` and deletes it afterwards.

## 15. Tasks (granular conventional commits, pushed after every commit)

1. Core types: `ConsoleId.content_id`/`region`, `ArtLocator.role`, `IdSource` members, `cik`, `FACTS_VERSION` 8, `NotARom::ConsoleSystem`, `LeftReason::GameOverlay`, the four notes, `PlanWarning::LargeTransfer` (+ CLI text, TS types, contract).
2. Systems and profiles: four `systems.toml` entries, `embedded_first`, `no_emulator`, `zar` Shared, `pkg`/`exfat` weak; §6 rows, Batocera suffix and `dlc_folder`; `SystemInfo.no_emulator`.
3. PS4: folder probe, PSF kinds, region, CNT PKG, `.ps4` launcher, overlays (D11), shadPS4 user dir.
4. PS5: folder probe, `param.json`, FIH PKG, `.ffpkg`/`.ffpfs`/`.ffpfsc`/`.exfat`, app archives (D13).
5. `.zar` classification (D12).
6. Xbox One and Series: GDK, ERA, XVC, GUID candidates, Scarlett rule, PC builds.
7. Dedupe keys and region fallback (D14, D8); grouping (D15).
8. Covers: `embedded_first`, new locators, MRT logos, libretro listing refresh, ScreenScraper ids.
9. Size confirmation: warning, backend gate, CLI flag, dialog.
10. Icons: sources, audit, regenerate, notices.
11. UI: copy, «Sin emulador», mock pile, copy guard test.
12. E2E pass; PR (English, no AI attribution) with §13, the per-frontend mismatches of §6 and every dropped «verify» rule.

Other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel: `git fetch origin && git merge origin/main` before tasks 9–12, conflicts resolved by hand, never a force-push.

## 16. Sources (fetched 2026-11-01)

| What | Source | Pinned at |
|---|---|---|
| libretro-thumbnails | <https://thumbnails.libretro.com/> live root and `Sony - PlayStation 4/Named_Boxarts/` (20 files) | 2026-11-01 |
| libretro-database (no PS4/PS5/Xbox One/Series DAT) | `libretro/libretro-database` git tree | `764ef77a9f5247fe170c71b5368dee0cc9a3a04f` |
| ScreenScraper ids | Skyscraper `platforms_idmap.csv` (`ps4 60`, `ps5 284`); screenscraper.fr `systemeinfos.php?plateforme=34` (Xbox One) | `c253bfe0ea359aa4272f47694bd601bd53205d2b`; 2026-11-01 |
| ES-DE | `resources/systems/*/es_systems.xml`, `USERGUIDE.md` | `ef267ae693caeee89ca8ca888a38e9eab9bce90a` |
| Batocera | `es_systems.yml` (`ps4` comment), `python-src/batocera-launch/batocera_launch/emulators/shadps4.py` | `cc793b75261c92d3b2a490c351e758ba155e9936` |
| Knulli | `es_systems.yml` | `ab3a6809286b3cff1648fb8c96a0200bc869beb6` |
| RetroBat | `system/templates/emulationstation/es_systems.cfg` | `54e5e31d459adab8a10f29224af0069d20baa891` |
| shadPS4 (overlays, PSF, user dir) | `src/core/file_sys/fs.cpp`, `src/core/file_format/psf.{h,cpp}`, `src/common/path_util.{h,cpp}` | `55a97e46955b2ece7c7ff8bddeb043ca57a506c6` |
| shadPS4 launcher (region, art files) | `shadps4-qtlauncher` `src/qt_gui/game_list_utils.h`, `game_info.h` | `24fd1248155dcfce61437b0ec0a3b0a4d0320386` |
| KytyPS5 (`param.json`, `attribute3` bit 22) | `src/loader/systemContent.cpp`, `src/graphics/presentation/videoOut.cpp` (GPL-2.0; facts read, no code copied) | `893b03552699dc873474023bc1f4c69c57fb36b2` |
| PS5 image formats | `phantomptr/ps5upload` `client/src/api/fpkg.ts`, `engine/crates/ps5upload-fpkg/src/source.rs` (licence not asserted; facts only) | `21f40fd17092efbdb3b9d43a4bba03288c94a90a` |
| GDK `MicrosoftGame.config` | `microsoft/Xbox-GDK-Samples` `Samples/Live/DownloadableContent/**` | `cdbec9e9b95fdf931b0802fc429fc3719c2efa47` |
| ERA `AppxManifest` namespace | `microsoft/Xbox-ATG-Samples` `XDKSamples/System/Gamepad/Package.appxmanifest` | `ff629a6e4876e38fe8761ead116a0e53f634ca67` |
| WinDurango (ERA folder, register to run) | `WinDurango/WinDurango` `README.md` (LGPL-3.0); `WinDurango/ImmersiveLoader` `ImmersiveLoaderCore.cpp` | `739c0b611f57c191ce5df72f6de7335a605ac8e7`; `36fe94bc7b6eceb8b8f2ff9fa6bed3ea005bea56` |
| PS4/PS5 PKG, XVD offsets and enums | `docs/02-research/updates-dlc-per-system.md` (LibOrbisPkg, PkgToolBox, xvdtool, verified there) | 2026-11-01 |
| Icons | `libretro/retroarch-assets` `src/xmb/monochrome` (committed listing) | `d9f969054dc7fbb6fa89519036d2b971e0855b51` |

## 17. Self-review

- **Placeholders**: none open. Every «verify» names its source and its fallback and is listed in §13.
- **Corrections to the brief, stated where they apply**: PSF `fmt` is stored `04 02` (§3, shadPS4 compares the raw value); libretro **does** have a PS4 folder (§3, D21); `TargetDeviceFamilyForDLC` is an element (D18); the region table comes from shadPS4's launcher, with `I` World added and no `K` guessed (D8).
- **Contradictions checked**: D4 (no emulator names) vs the owner's «avisar «hay que extraerlo»» — the note keeps the owner's words and drops «shadPS4 no lo arranca» (the emulator name); D11 (siblings move with the base) vs «no implementar 2.1» — siblings keep Tanda 2's flat placement in the system folder, only their names follow the base, and mods are left, not structured; D16 (Batocera DLC path) vs 2.1 — a frontend's required path, not a per-game folder; D21 (embedded first) vs batch 2 D17 (embedded last) — per system, owner's ask, the user's switch still wins; D22 vs D10 — `ps5` is not flagged system-wide because its folders run; D19 (XVC to review) vs the Scarlett rule — nothing but the config's executable family makes a Series game; D23 applies to every system and answers batch 2 §13.
- **Counts**: 127 + 4 = 131 systems; §6 and §7 have four rows; `FALLBACK_ABBR` entries ≤ 4 characters (`PS5`, `PS4`, `XB1`, `XBSX`).
- **Brief rules**: folders researched per frontend with pins (§6, §16), PS5/Series left Absent where no frontend has them; headers and `sce_sys/` only (D24); keys and licences by name only, encrypted entries never read (D4, D9, D10); no emulator linked, recommended or named, guarded by a test (D4, §14.2); covers embedded first with libretro checked live (D21); 2.1 untouched (D1, §13); owner decisions listed without blocking (§13).
- **Fixed during self-review**: the PS5 store PKG first read the entry table for `param.json`; dropped (ids unverified) so nothing possibly encrypted is touched. The size dialog first counted same-volume moves; renames write nothing, so they no longer count. The XVC first went to `xboxone` by default; that would file Series packages as One, so it goes to review. The Batocera suffix first ignored the update sibling; shadPS4's name-based overlay lookup would then break updates, hence D11 (b).
