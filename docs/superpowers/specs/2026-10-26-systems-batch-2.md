# Coverly ROMs — Missing systems, batch 2: big discs and handhelds with their own format (Design Spec)

> Status: **Design, ready to implement on `feat/systems-batch-2`** (self-reviewed §17) · Date: 2026-10-26 (research run 2026-10-31) · Owner: Miguel Angel Esparza Calero (MAECLY)
> Roadmap item: `TODO.md` §2 «Sistemas que faltan», **Tanda 2 only**. Tanda 1 is merged (PR #32, spec [`2026-10-26-systems-batch-1.md`](2026-10-26-systems-batch-1.md)); Tanda 3 (PS4, PS5, Xbox One, Xbox Series) is a separate later run and is not started here.
> Branch: `feat/systems-batch-2`. `docs/TRACKER.md` and `TODO.md` are updated on `main` after merge, not on this branch.
> Builds on: batch 1 (per-profile folder tables, `weak_extensions`, `scoped_aliases`, folder-game rule D15, D24 contradiction, D25 packages, D26 text-native), [`2026-10-23-organize-archives-and-discs.md`](2026-10-23-organize-archives-and-discs.md) (disc sets, budgets), [`2026-10-18-system-icons-and-views.md`](2026-10-18-system-icons-and-views.md) (SystemIcon), brand [`docs/08-brand/brand.md`](../../08-brand/brand.md).

## 1. Purpose

After batch 1 Coverly knows 118 systems, all of them cartridges, tapes, floppies and CD/DVD images that the existing readers open. The nine systems of Tanda 2 are different in three ways:

1. **Each one has its own container.** GameCube and Wii ship as ISO, GCM, CISO, GCZ, WBFS, WIA and RVZ; the 3DS as NCSD, NCCH and CIA (and Azahar's zstd `Z3DS` wrappers); the Switch as XCI and NSP (and their `NSZ`/`XCZ` compressed forms); Wii U as WUD, WUX and Cemu's WUA; Xbox and Xbox 360 as XDVDFS images, XBE, XEX and STFS/GOD packages. None of them is ISO 9660 with a `SYSTEM.CNF`. Each needs a small reader that finds the **plaintext** id and title in a few kilobytes of header.
2. **Many games are folders, not files.** PS3 JB folders (`PS3_GAME/PARAM.SFO`), PS Vita app folders (`sce_sys/param.sfo`), Wii U loadiine folders (`code/`, `content/`, `meta/`), Xbox folders with `default.xbe`, Xbox 360 Games-on-Demand trees. A folder is one game and must never be split.
3. **They are big and partly encrypted.** Images run from 1.4 GB (GameCube) to 25 GB (Wii U, PS3); several formats are encrypted with console keys. Coverly reads headers only, within a fixed budget, and **never** asks for, ships, downloads or reads a console key, and never decrypts anything.

The batch keeps batch 1's three safety rules (no silent reclassification, folder names from sources, ambiguous files to review) and adds two of its own: **headers only, within budget** (§8.6) and **keys are untouchable** (§4 D4).

## 2. The owner's asks (verbatim from `TODO.md` §2)

> Hoy `crates/coverly-core/src/systems/systems.toml` define 54 sistemas. Faltan los de esta lista. Por cada sistema hay que añadir:
>
> 1. Su entrada en `systems.toml`: `id`, nombre, alias de carpeta, extensiones, carpeta de libretro-thumbnails si existe, DAT de libretro-database si existe e id de ScreenScraper.
> 2. Su carpeta en cada perfil de consola (`crates/coverly-core/src/profiles/builtin.rs`) donde el frontend lo soporte, y `disc_layout` si es de disco.
> 3. Identificación: por cabecera cuando la haya (`identify/`, `disc/`) y por hash con su DAT.
> 4. Su icono en `SystemIcon` (silueta de RetroArch Assets si existe; si no, una propia).
> 5. Tests: detección por cabecera, carpeta de destino en Organizar, búsqueda de portada y snapshot del plan.
>
> ### Tanda 2: discos grandes y portátiles con formato propio
>
> Hace falta un lector de su formato y una identificación propia.
>
> - [ ] Nintendo GameCube (`gamecube`). Formatos: ISO, GCM, CISO, GCZ, RVZ, WIA. Id de juego en la cabecera del disco (6 caracteres en el offset 0).
> - [ ] Nintendo Wii (`wii`). Formatos: ISO, WBFS, CISO, GCZ, RVZ, WIA, y WAD de WiiWare. Misma cabecera que GameCube con la magia Wii en 0x18.
> - [ ] Nintendo Wii U (`wiiu`). Formatos: WUD, WUX, WUA, RPX y las carpetas descifradas (`code/`, `content/`, `meta/`). Id en `meta/meta.xml`.
> - [ ] Nintendo 3DS (`3ds`). Formatos: 3DS, CCI, CIA, CXI, 3DSX, ZCCI, ZCIA. Product code en la cabecera NCSD/NCCH.
> - [ ] Nintendo Switch (`switch`). Formatos: XCI, NSP, NSZ, XCZ. Title ID en la cabecera; los NSP/XCI van cifrados y solo se puede leer lo que no lo está.
> - [ ] Sony PS Vita (`psvita`). Formatos: VPK, carpetas `app/` descifradas, `.psvimg`. Title ID en `sce_sys/param.sfo`.
> - [ ] Sony PS3 (`ps3`). Formatos: ISO, carpetas JB (`PS3_GAME/`), PKG. Title ID en `PS3_GAME/PARAM.SFO`.
> - [ ] Microsoft Xbox (`xbox`). Formatos: ISO XISO, carpetas con `default.xbe`. Title ID en la cabecera XBE.
> - [ ] Microsoft Xbox 360 (`xbox360`). Formatos: ISO, GOD, XEX, carpetas XBLA. Title ID en la cabecera XEX.
> - [ ] Nintendo DSi y DSiWare, si los perfiles los separan de `nds`.
>
> ### Decisiones abiertas
>
> - [ ] Nombres de las carpetas por perfil (Anbernic stock, muOS, Knulli, Batocera, ES-DE, RetroArch) para cada sistema nuevo. Investigarlo antes de cada tanda.
> - [ ] Qué sistemas aparecen por defecto y cuáles solo si el perfil los usa.
> - [ ] Licencias de los iconos que no estén en RetroArch Assets.

Workflow brief additions for this run: a reader per format that reads **only plaintext metadata**; evaluate the `nod` crate or others and record the licence against AGPL-3.0; the Switch legal/security rule (never require, bundle, download or read `prod.keys`/`title.keys`, never decrypt; use what is in clear and fall back to the file name and a title-id catalogue, recording exactly what is and is not possible); headers only, never whole contents, within budgets; covers from libretro-thumbnails where they exist, else embedded plaintext art (`icon0.png` and the like) and the existing SteamGridDB/ScreenScraper fallbacks; profile folders researched per frontend, never guessed; anything needing an owner decision is implemented around and listed (§13), never allowed to block the batch.

## 3. What exists today (read before changing)

Batch 1's table (its §3) still holds. What matters in addition for this batch:

| Layer | Where | What matters here |
|---|---|---|
| Systems | `systems/systems.toml` (118 entries), `systems/mod.rs` | `family`, `weak_extensions`, `scoped_aliases` exist. One `libretro` folder per system. `nds` has no `.dsi` and no DSi DAT. `doom` owns `.wad` (and is `DECISIVE`); `gamecom` owns `.tgc`; `psp` owns `.cso` (whose magic `CISO` is also the GameCube/Wii CISO magic). |
| Sniff | `identify/sniff.rs` | `PREFIX` = 128 KiB read from the head, `STREAM_CAP` = 8 MiB inside archives; `DECISIVE` has 14 ids; `TEXT_NATIVE_EXTS` 14. NDS header (logo CRC + header CRC) → `nds` 0.95; the unit code at 0x12 (DSi) is not read. |
| Inspect | `organize/inspect.rs` `inspect_file_with` | Raw file: head (`PREFIX`) + tail, `sniff` on the head, then `sniff` on a `FileAt` reader for longer files. `PACKAGE_EXTS` = `arduboy rpk solarus lutro uef` (zip/gzip read, never extracted). Disc sets go to `DiscReader`. |
| Disc | `disc/mod.rs` `DiscReader::identify_with`, `disc/iso9660.rs` (`find_root_file`, root only, `MAX_ROOT_RECORDS`), `disc/cso.rs` (PSP CISO), `disc/blocks.rs` (`MAX_INFLATED`) | No GameCube/Wii/Xbox/PS3 rule. `DiscIdentity { system, serials, disc_no, volume, source, confidence, detail }` is the shape organise and dedupe consume. |
| Serials | `disc/serial.rs` `normalise_serial` (needs a digit, PlayStation `AAAA-NNNNN`, Sega forms), `identify/dat.rs` `normalise_serial` (DAT side: four letters + five digits → `AAAA-NNNNN`, anything alphanumeric with dashes kept upper-cased) | `PARAM.SFO` `TITLE_ID` `BLUS30001` and the DAT's `BLUS-30001` already meet in the DAT normaliser. A GameCube `GALE01` passes both. A Wii U GameTDB serial `SNKP` (no digit) passes the DAT side only. |
| Hash | `identify/hash.rs` `DEFAULT_HASH_MAX_BYTES` = 64 MiB, `NAME_ONLY_EXTS` (includes `rvz`) | Every image of this batch is over the cap: identification is header serial, then name. |
| PARAM.SFO | — | No PSF reader exists (`disc/pbp.rs` deliberately ignores the PBP's `PARAM.SFO`). Tanda 3 (PS4) needs the same reader. |
| Folder games | `organize/walk.rs` `FOLDER_GAME_SYSTEMS` = `doom solarus lutro scummvm`, `folder_game_in(dir)` reads the directory's own files only (one level), `LeftReason::FolderGame { system }` | A JB folder holds its proof one level down (`PS3_GAME/PARAM.SFO`); the rule must look at named sub-paths, not walk. |
| Art | `sources/{libretro,screenscraper,steamgriddb}.rs`, `ArtSource` trait, `ArtQuery { system, media, canonical, rom, stem, … file_name }` | No local/embedded source. `ArtQuery` has no path to the ROM. `image` features: `png jpeg webp gif bmp` (no TGA). |
| Plan notes | `organize/plan.rs` `ItemNote` (`MultiRomZip`, `NoFolderOnDevice`, `ExtensionContradicted`, `CatalogueMissing`, `Encrypted` = an *archive entry* is password-protected, …), `PlanWarning::NoSpace { needed, free }`, `LeftReason::TooBigForFat` | Big items already get the free-space warning and the FAT32 reason. |
| Facts | `organize/plan.rs` `FACTS_VERSION = 6` | Facts of version 6 hold no Tanda 2 verdicts. |
| Profiles | `profiles/builtin.rs` `SYSTEM_FOLDERS` (columns `esde batocera muos onion minui`), `ANBERNIC_FOLDERS` (55, none of this batch) | Same mechanism; new rows only. |
| Crates | `ruzstd` (pure Rust zstd), `lzma-rust2`, `flate2` (zlib-rs), `quick-xml`, `zip` | Everything a header reader needs is already a dependency (§4 D2). Workspace licence `AGPL-3.0-only`. |

## 4. Decisions

Open questions and the default chosen. Each can be overturned by the owner without changing the rest of the design.

| # | Question | Decision | Why |
|---|---|---|---|
| D1 | Scope | The nine systems of Tanda 2 plus the DSi check. Items in §13 are implemented around. Tanda 3 untouched; the PSF reader (§8.4) is written so Tanda 3 can reuse it, nothing more. | Brief. |
| D2 | Use `nod` (GameCube/Wii) or another crate? | **No new crate. Own header-only readers** in a new module `crates/coverly-core/src/consoles/`. `nod` 1.4.4 (encounter/nod, `MIT OR Apache-2.0`, compatible with AGPL-3.0) was evaluated and rejected: (a) its `disc/wii.rs` embeds the Wii common keys and decrypts partitions, which D4 forbids shipping; (b) its default features vendor C `liblzma-sys`, `zstd-sys` and `bzip2-sys`, and it pulls `rayon`, `aes`, `cbc`; (c) Coverly needs 0x80 bytes of disc header, which every one of the seven formats stores in clear (RVZ/WIA keep a raw copy in their header, §8.1). Switch crates (`nx-archive` 0.1.2, MIT) and `xex` 0.2.0 (MIT) were looked at the same way: both decrypt or require keys for anything beyond the plain tables, so they bring nothing. Nothing is added to `THIRD-PARTY-NOTICES.md` (no new dependency); the evaluation is recorded in §16. | Smallest surface, no key material in the binary, no C toolchain change for the Windows ARM build, and every read stays under Coverly's own budget accounting. |
| D3 | Ids | The owner's: `gamecube`, `wii`, `wiiu`, `3ds`, `switch`, `psvita`, `ps3`, `xbox`, `xbox360`. `3ds` starts with a digit like `3do` (already valid). | Stable keys; frontend spellings go to the per-profile table (§6). |
| D4 | Keys and encryption (legal/security rule) | **Coverly never asks for, ships, downloads, opens, hashes, uploads or moves console keys or licences, and never decrypts.** Concretely: (1) no reader takes a key parameter, and no code path derives one; (2) **key material files** are recognised by name before any read and are left in place with the new reason `KeyMaterial`: `prod.keys`, `title.keys`, `dev.keys`, `keys.txt`, `aes_keys.txt`, `seeddb.bin`, `*.rap`, `*.rif`, `*.dkey`, `*.zrif`, `act.dat`, `work.bin` and a loose `*.tik`/`*.cetk` (§12). Inside a game folder they move with the folder, untouched and unread (the folder is one item); inside an NSP the `.tik` **entry name** is read from the PFS0 table (it is the rights id), never its bytes; (3) encrypted content is never "fixed": an encrypted 3DS or Wii U image is identified from its clear header and gets an informational note (§10), and Coverly never says how to decrypt; (4) these files are excluded from ScreenScraper hash lookups and from dedupe. What each format allows in clear is listed per system in §8. | The brief's rule, applied to every system of the batch, not only Switch, because PS3 (`.rap`, `.dkey`), Vita (`work.bin`, zRIF), 3DS (`seeddb.bin`, `aes_keys.txt`) and Wii U (`title.tik`) have the same kind of files. |
| D5 | Where the readers live | New `consoles/` module: one file per format family (§5.1), each a pure function over a bounded `ReadAt` returning `ConsoleId` (§8.0). `identify/sniff.rs` calls the cheap head-only checks (they also work inside archives on the first `STREAM_CAP` bytes); `organize/inspect.rs` calls the seeking readers on raw files; `organize/walk.rs` calls the folder probes. `disc/` is not changed except for a GameCube/Wii branch in `DiscReader` (a `.iso`/`.gcm` that is not ISO 9660) and the PS3/Xbox ISO checks. | Keeps `disc/` about CD/DVD sector images; console formats are not sectors. |
| D6 | DSi / DSiWare | **No separate system.** No researched profile separates it: ES-DE puts `.dsi` in `nds`, muOS's `Nintendo DS` manifest lists `dsi`, `nintendodsi` and `nintendonintendodsi` as friendly names of the same catalogue, Batocera, Knulli and RetroBat have no DSi folder (§6). `nds` gains: extension `dsi`, global aliases `dsi`, `nintendodsi`, the No-Intro DSi DAT (`metadat/no-intro/Nintendo - Nintendo DSi.dat`, serials such as `KJUJ`, DSiWare dumps of a few MB, so hashable) beside its NDS DAT, and the DSi thumbnails folder as a second cover folder (D9). The sniff reads the unit code at 0x12 (0x00 DS, 0x02 DSi-enhanced, 0x03 DSi-only) into the detail (`NDS header (DSi)`), never into the system. | Brief: «si los perfiles los separan de `nds`». They don't. |
| D7 | Shared extensions | Real collisions become **Shared** (header decides, review otherwise): `gcz`, `rvz`, `wia`, `ciso` (gamecube, wii). **Weak** (batch 1 D3/D4 rule: counted only in the system's folder or with its header): `wad` on `wii` (`doom` keeps it strong; a WAD header proves Wii and contradicts doom per batch 1 D24), `tgc` on `gamecube` (`gamecom` keeps it), `pkg` on `ps3` and `psvita` (also every macOS installer: magic decides), `xbe` on `xbox`, `nro`, `nso`, `nca` on `switch`, `psvimg` on `psvita`, `tmd` on `wiiu`. **`3ds` joins `DECISIVE`** (an Autodesk 3D Studio `.3ds` has no `NCSD` magic, so it drops to 0.60 and is never Auto). Never listed: `app` (macOS bundles, ES-DE lists it for 3DS and NDS), `elf`, `dol`, `axf`, `json`, `squashfs`, `desktop`, `lnk`, `sh`. | Batch 1's rule: never by extension alone when the extension means something else elsewhere. |
| D8 | Aliases | First alias by batch 1 D8 (no Onion/NextUI folder for any of these, so the upper-case id): `GAMECUBE`, `WII`, `WIIU`, `3DS`, `SWITCH`, `PSVITA`, `PS3`, `XBOX`, `XBOX360`. Others: `gc`, `ngc`, `gcn` (gamecube); `n3ds`, `nintendo3ds` (3ds); `nintendoswitch`, `nsw` (switch); `playstation3` (ps3); `x360` (xbox360). **Scoped** (batch 1 D22, everyday words): `switch` and its first alias `SWITCH`, `vita`. Global: `gamecube`, `gc`, `wii`, `wiiu`, `3ds`, `n3ds`, `psvita`, `ps3`, `xbox`, `xbox360`. | A `Switch/` folder in Downloads (network gear, a game's mods) must not stop being organised; `xbox` and `ps3` name nothing else. |
| D9 | One libretro folder per system is not enough (DSiWare, 3DS eShop, PSN) | New optional `libretro_also: Vec<String>` in `systems.toml`: extra thumbnail folders tried **after** `libretro` for cover lookup only. `{libretro}` in templates keeps the primary. Values: `nds` → `Nintendo - Nintendo DSi`; `3ds` → `Nintendo - Nintendo 3DS (Digital)`; `ps3` → `Sony - PlayStation 3 (Downloadable)`. All three are in the live root listing (§7). | Same data shape as the primary, no template change. |
| D10 | Switch title id when it is only inside encrypted data | Order: (1) NSP/NSZ: the PFS0 table's `<rightsid>.tik` entry name (the first 16 hex digits of a rights id are the title id); (2) NSP/NSZ: a plaintext `*.cnmt.xml` entry when present (some dumpers add it; ≤ 64 KiB, `<Id>`, `<Version>`, `<Type>`); (3) any format: a `[0100XXXXXXXXXXXX]` 16-hex token in the file name, and `[vNNN]` for the version; (4) none → name only. XCI/XCZ never carry a clear title id (their HFS0 lists NCA content ids, which are hashes, and the gamecard info at 0x190 is encrypted with a key): only (3)/(4). **No title-id catalogue is downloaded in this batch** (§13): the only open one found, `blawar/titledb` (MIT), ships ~90 MB JSON per region; GameTDB's Switch list keys on product codes (`LA-H-A224A`), not title ids. Names come from the file name, covers from ScreenScraper (system 225) and SteamGridDB by name. | The brief: «record exactly what is and isn't possible» (§8.3 table). A 90 MB download per plan is out of proportion; the owner decides (§13). |
| D11 | Updates, DLC, demos | Readers return `kind: Base | Update | Dlc | Demo | Unknown` from the title id (§8.0 table). Organise moves them to the same system folder as any game, with a note «Actualización de {title}» / «Contenido descargable de {title}» when the base is known in the same plan, otherwise with the id. The plan summary states how many rows are updates or DLC (§10); group counts stay row counts. Dedupe keys add `kind` and version: an update is never a duplicate of its base; two copies of the same base and version are duplicates; same base with different versions are duplicates whose keeper is the higher version (existing keeper rules decide otherwise). | Same ask as Tanda 3 (PS4 `-patch`), applied now so both batches share one rule. |
| D12 | Folder games (JB, Vita app, Wii U loadiine, Xbox `default.xbe`, Xbox 360 extracted/GOD/XBLA, split NSP) | Batch 1 D15 extended with **named sub-path probes**: `folder_game_in(dir)` also checks a fixed list of relative paths with `symlink_metadata` (no directory walk): `PS3_GAME/PARAM.SFO` and `PARAM.SFO`+`USRDIR/` (ps3), `sce_sys/param.sfo` (psvita), `code/app.xml` or `meta/meta.xml` or `title.tmd` (wiiu), `default.xbe` (xbox), `default.xex` (xbox360), an 8-hex folder holding an 8-hex content-type folder (xbox360 GOD/XBLA, §8.5), and a folder named `*.nsp`/`*.xci` holding `00`, `01`… (a split «archive bit» Switch file). A match → one item. Unlike batch 1's ports, these folders are **moved** (not left) because the probe file proves system and id: the item is a folder move to the system folder (existing `Target` for directories, the same path batch 1 uses for disc `GameFolder` sets). The probe file (and only it) is then read for the id (§8). | Splitting a JB folder destroys the game. The ports of batch 1 stay «left» because their proof is weak; here the proof is the console's own metadata file. |
| D13 | PS3 folders on ES-DE and Batocera | Both require a PS3 directory to end in `.ps3` (Batocera's `es_systems.yml` comment: «PS3 roms are directories must be renamed to end with .ps3»; ES-DE lists `.ps3` and `.ps3dir` directory extensions; RetroBat lists `.ps3`). New per-profile `folder_game_suffix` map in the built-in tables: `ps3 → ".ps3"` on `esde` and `batocera`. Organise appends it to the moved folder's name when absent (existing rename reason `catalogue` style, new note «Se renombra a {name} para que {layout} lo muestre»). The folder's contents are never touched. Other profiles: unchanged name. | Without it the moved game is invisible in both frontends. |
| D14 | Launcher text files (`.psvita`, `.psn`, `.xbox360`) | Recognised (text-native, batch 1 D26) and identified from their content when it is a title id (≤ 64 bytes read). **Coverly does not create them.** | ES-DE and Batocera expect a `.psvita`/`.psn` file naming a game installed in Vita3K/RPCS3; creating launchers is a new feature (§13). |
| D15 | Disc layout | `multi_file = true` for `gamecube`, `wii`, `ps3`, `xbox`, `xbox360` (disc systems with multi-disc games and `.m3u` in ES-DE/RetroBat). Each profile's existing `disc_layout` applies; GameCube's two-disc games use the disc number at 0x06 as `disc_no`. `multi_file` also means no archive is taken as-is (Dolphin, RPCS3, xemu and Xenia read no zip/7z). `3ds`, `switch`, `wiiu`, `psvita` are not `multi_file`: they keep default container rules, except `psvita` where `.vpk` is a package (D16). Explicit `dats` everywhere, so `multi_file`'s derived Redump default is never used. | Batch 1 D13 reasoning. |
| D16 | Zip-based packages | `vpk` joins `PACKAGE_EXTS` (a VPK is a zip: read `sce_sys/param.sfo` and `sce_sys/icon0.png` entries, never extract). `wua` and `zar` are ZArchive (not zip; §8.2, §8.5), raw items. | Batch 1 D25: extracting a VPK breaks the install. |
| D17 | Embedded covers | New art source `embedded` (`sources/embedded.rs`), **last** in the default source order and on by default, used only when the item's reader recorded an `EmbeddedArt` locator (§8.7): PS3 `ICON0.PNG`, Vita `sce_sys/icon0.png`, Wii U `meta/iconTex.tga`, Xbox 360 STFS thumbnail PNG, 3DS SMDH large icon (CIA meta section or a `NoCrypto` NCCH's ExeFS). Only for `Media::Cover`. Candidates rank below any libretro/ScreenScraper/SteamGridDB box art (`MatchKind::Canonical` but the source's order decides first). Reading: at fetch time, ≤ 2 MiB, re-located by the stored locator, the file's size+mtime must still match the plan's facts. `image` gains its `tga` feature (same crate, pure Rust, already MIT/Apache) for Wii U. The 3DS RGB565 tiled icon is decoded by a 30-line function into a 48×48 PNG. GameCube/Wii banners (96×32, RGB5A3), XBE `$$XTIMAGE` (XPR, DXT) are **not** used: low value, extra decoders. | Brief: «use embedded art when it is plaintext». Icons are square or landscape and small: a last resort, never preferred over real box art. |
| D18 | Which covers exist (libretro) | From the live root listing (127 folders, 2026-10-31): GameCube, Wii, Wii U, 3DS (+Digital), DSi, PS3 (+Downloadable), Vita, Xbox, Xbox 360 all have a folder; **Switch has none** (and no libretro-database DAT). | §7. |
| D19 | Profiles where Batocera and Knulli differ (`switch`) | Batocera dropped Switch (no `switch` key in `es_systems.yml` at `a3d49969`); Knulli and RetroBat have `switch`. The shared «Batocera / Knulli / EmulationStation» profile writes **`Named("switch")`**: Knulli users see it, Batocera ignores an unknown folder, and the folder name equals the `Id` rule anyway. This is the opposite default from batch 1 D7 (which followed Batocera) because here the note «no tiene carpeta» would be wrong for Knulli users and changes no path for Batocera users. | Smallest wrong-message surface. Listed in §13. |
| D20 | Which systems appear by default | Batch 1 D18 unchanged: always in pickers, grouped by family (`console`: gamecube, wii, wiiu, switch, ps3, xbox, xbox360; `handheld`: 3ds, psvita). | No hiding. |
| D21 | Icons | RetroArch Assets XMB Monochrome at the **same pinned commit** `d9f96905…` (CC BY 4.0, already attributed): every system of the batch has a file (§9.2). No other icon source. Audit as batch 1 D20 (logo vs silhouette). | No new licence. |
| D22 | Serial forms per system | Readers emit the DAT's form directly: GameCube/Wii **ID6** (`GALE01`, matches GameTDB `dat/` serials), 3DS **product code** (`CTR-P-BP4J`, matches No-Intro), DSi game code (`KJUJ`), Vita/PS3 `AAAA-NNNNN` (via the existing DAT normaliser), Xbox `XX-NNN` from the XBE title id (§8.5, matches Redump `EA-013`), Wii U the 4-character code of `WUP-P-XXXX` (matches GameTDB `SNKP`) plus the full code, Xbox 360 the 8-hex title id. `disc/serial.rs` gains `normalise_console_serial(system, raw)` with these per-system shapes (no "needs a digit" rule for ID6/ID4); the PlayStation path is unchanged. The Wii Redump serial `RVL-SP3E-USA-B0` is matched by its 4-character middle part as a second key. | Each catalogue already uses one of these forms; no translation table. |
| D23 | Memoised facts | `FACTS_VERSION` 6 → 7 (no Tanda 2 verdicts in version 6). `organize_choices` kept (a remembered pick still wins). `HASH_OUTCOME_VERSION` unchanged. | Batch 1 D27. |
| D24 | Budgets | Per item: at most **4 MiB read** from disk in total and **1 MiB decoded** (one GCZ block, one zstd frame of a `Z3DS` file, one ZArchive block), at most **16 seeks**, never a full-file read; directory probes use `symlink_metadata` on named paths only and at most one `read_dir` of one named directory (`PS3_GAME`, the GOD content-type folder) capped at 256 entries. Cloud placeholders (`MetaProbe::cloud_only`) are never read. A reader that would exceed the budget returns what it has (system by magic, no serial), and identification falls back to the name. | §8.6. The disc images are 1.4–25 GB; reading them would take minutes and, on a card, wear it. |

## 5. Architecture and files touched

### 5.1 Rust

| File | Change |
|---|---|
| `consoles/mod.rs` (new) | `ConsoleId`, `TitleKind`, `EmbeddedArt { media, locator }`, `Budgeted<R: ReadAt>` (counts bytes, seeks, decode; §8.6), `identify_raw(path, len, head, ext, db) -> Option<ConsoleId>`, `identify_dir(dir, probe) -> Option<ConsoleId>`, `is_key_material(name) -> bool` (D4). |
| `consoles/nintendo_disc.rs` | GameCube/Wii disc header; ISO/GCM, TGC, CISO (with the PSP CSO discriminator), GCZ (one zlib block), WBFS, WIA/RVZ (§8.1). |
| `consoles/wad.rs` | Wii WAD: header, TMD title id, TMD-section CRC for the No-Intro Wii (Digital) DAT (§8.1). |
| `consoles/ctr.rs` | 3DS: NCSD, NCCH, CIA (TMD, content chunk, meta SMDH), 3DSX, `Z3DS` wrapper, SMDH titles and icon (§8.2). |
| `consoles/nx.rs` | Switch: XCI `HEAD`, HFS0, PFS0, `.tik` names, `.cnmt.xml`, file-name title id (§8.3). |
| `consoles/psf.rs` | `PARAM.SFO` (PSF) reader shared by PS3, Vita (and Tanda 3) (§8.4). |
| `consoles/sony.rs` | PS3 JB/HDD folders, PS3 ISO (via `disc/iso9660.rs` plus one sub-directory), `PS3_DISC.SFB`, PKG header (PS3/Vita), Vita app folders, VPK entries, `.psvita`/`.psn` text (§8.4). |
| `consoles/xbox.rs` | XDVDFS (partition offsets, bounded tree walk), XBE certificate, XEX2 optional headers, STFS/GOD metadata (§8.5). |
| `consoles/wiiu.rs` | WUD, WUX, `meta.xml`/`app.xml` (quick-xml, ≤ 64 KiB), TMD, RPX/WUHB magic (§8.2). |
| `consoles/zarchive.rs` | ZArchive footer, names, file tree, one block via `ruzstd` (for `.wua` and Xenia `.zar`). |
| `systems/systems.toml`, `systems/mod.rs` | Nine entries (§7), `nds` changes (D6), `libretro_also` (D9) with the same lenient user-file merge as batch 1's fields; `wad`, `tgc` weak moves; `3ds` header id. |
| `identify/sniff.rs` | Head-only hits (GameCube/Wii magics, WBFS, WIA/RVZ, `HEAD` at 0x100, PFS0, NCSD/NCCH, `Z3DS`, `3DSX`, `XEX2`, `CON `/`LIVE`/`PIRS`, `XBEH`, PKG, WAD, WUX); NDS unit code (D6); `DECISIVE` gains `3ds`; `TEXT_NATIVE_EXTS` gains `psvita`, `psn`, `xbox360`. |
| `disc/serial.rs` | `normalise_console_serial` (D22). |
| `disc/mod.rs` | `DiscReader`: before the ISO 9660 checks, a GameCube/Wii header at byte 0 of an `.iso`/`.gcm`; after the PVD, PS3 (`PS3VOLUME`, `PS3_DISC.SFB`) and XDVDFS at its offsets for Xbox/Xbox 360 (§8.1, §8.4, §8.5). |
| `identify/dat.rs` | Serial index also stores the Wii Redump middle code (D22); prefix lookup for Xbox 360 digital rows (§8.5). |
| `organize/inspect.rs` | Raw files: after the sniff, `consoles::identify_raw` when the sniff named a Tanda 2 system or the extension is one of theirs; `vpk` in `PACKAGE_EXTS`; key-material check before opening (D4). |
| `organize/walk.rs` | Folder probes (D12), key-material files left before any read (D4), emulator-data folders left (§12). |
| `organize/plan.rs`, `organize/target.rs` | `LeftReason::KeyMaterial`, `LeftReason::EmulatorData { folder }`; `ItemNote::Update { of }`, `ItemNote::Dlc { of }`, `ItemNote::EncryptedContent { system }`, `ItemNote::InstallFirst { emulator }`, `ItemNote::FolderSuffix { name }`; folder-game items moved as a directory (D12); `folder_game_suffix` (D13); size of folder items (sum of file sizes from metadata, ≤ 100 000 entries, else «más de»…, §9.1); `FACTS_VERSION` 7. |
| `profiles/builtin.rs`, `profiles/mod.rs` | Rows of §6; `folder_game_suffix` per profile (D13). |
| `dedupe/keys.rs`, `dedupe/units.rs` | Title id + kind + version keys (D11); folder games are units (not out of scope like `ports`). |
| `sources/embedded.rs` (new), `sources/mod.rs`, `fill/mod.rs` | Source `embedded` (D17); `ArtQuery.rom_path: Option<&Path>` and the item's `EmbeddedArt` locator; `libretro_also` lookups in `sources/libretro.rs`. |
| `settings/model.rs` | `embedded` in the default source order (last, enabled). Existing settings files: the missing id is appended on load (existing rule for new sources). |
| `src-tauri/src/ipc.rs` | New reasons/notes serialise through the existing enums; `SystemInfo` unchanged. |
| `Cargo.toml` (workspace) | `image` features + `tga`. No new crate. |

### 5.2 Front end

| File | Change |
|---|---|
| `src/ipc/types.ts`, `contract.sample.json`, `contract.test.ts` | `LeftReason` `key-material`, `emulator-data`; `ItemNote` `update`, `dlc`, `encrypted-content`, `install-first`, `folder-suffix`; `OrganizeItem.size_bytes` for folder items (already present for files? the task checks; if absent it is added for both). |
| `src/lib/organize.ts` | Copy for the new reasons and notes (§10), the `why.titleId*` lines. |
| `src/components/organize/SystemGroups.tsx` | Folder items: a folder glyph from the existing icon set and the size in mono after the name (§9.1). |
| `src/components/settings/*` (sources list) | The `embedded` source row (name and hint), no key field. |
| `src/components/system/systemFallbacks.ts`, `scripts/icons/systemIconsLib.mjs`, generated icons, `THIRD-PARTY-NOTICES.md` | Nine silhouettes (§9.2). |
| `locales/{es,en}/ui.json` | §10 keys. |
| `src/ipc/mock/mockOrganize.ts`, `fixture.ts` | `?mockPick=modern` pile (§9.3); shelves fixture gains `GAMECUBE`, `PS3`, `SWITCH`. |

## 6. Per-profile folders (research result)

Legend as batch 1 §6: a name = `Named`; `—` = `Absent`. `retroarch` keeps `Id` (RetroArch has cores only for GameCube/Wii (Dolphin) and 3DS; the folder is still the id). Anbernic stock: `—` for every row (none of the 55 `ANBERNIC_FOLDERS`). Onion/GarlicOS: `—` (no package among Onion's 54 `Emu` packages at `07505ea5`). MinUI/NextUI: `—` (no pak among NextUI's at `5291344b`).

| id | ES-DE | Batocera [Knulli] | muOS | RetroBat (reference, no profile) | Onion / Garlic / MinUI / Anbernic |
|---|---|---|---|---|---|
| gamecube | gc | gamecube | — | gamecube | — |
| wii | wii | wii | — | wii | — |
| wiiu | wiiu | wiiu | — | wiiu | — |
| 3ds | n3ds | 3ds | 3ds | 3ds | — |
| switch | switch | switch (D19: Batocera has none, Knulli has `switch`) | — | switch | — |
| psvita | psvita | psvita | — | psvita | — |
| ps3 | ps3 (+ `.ps3` dir suffix, D13) | ps3 (+ `.ps3`, D13) | — | ps3 | — |
| xbox | xbox | xbox | — | xbox | — |
| xbox360 | xbox360 | xbox360 | — | xbox360 | — |
| nds (DSi, D6) | nds (`.dsi` listed) | nds | nds (friendly `dsi`, `nintendodsi`) | nds | existing |

Notes:
- muOS: the only manifest of this batch at `0784148f` is `Nintendo/Nintendo 3DS.json` (friendly `nintendo3ds`, `n3ds`, `3ds`; Azahar standalone, `require.device = ["rg-vita-pro"]`). The table writes `3ds`; `n3ds` and `nintendo3ds` are global aliases so all three scan.
- ES-DE's GameCube folder is `gc`, not `gamecube`: the ES-DE media-path question of batch 1 §13 applies to it too (`downloaded_media/{system_id}` writes `gamecube`, ES-DE reads `gc`), and to `3ds` (`n3ds`). Listed, not changed (§13).
- Extension lists per frontend (for the PR's mismatch list, never used to rename): ES-DE `gc`/`wii` `ciso dff dol elf gcm gcz iso json m3u rvz tgc wad wbfs wia 7z zip`; `n3ds` `3ds 3dsx app axf cci cxi desktop elf z3dsx zcci zcxi 7z zip` (**no `.cia`**, no `.zcia`); `switch` `nca nro nso nsp xci` (**no `.nsz`/`.xcz`**); `wiiu` `elf rpx tmd wua wud wuhb wux`; `ps3` `desktop iso ps3 ps3dir`; `psvita` `psvita` only (a `.vpk` moved there is not shown); `xbox` `iso xiso`; `xbox360` `. desktop iso sh xex zar`. Knulli: `gamecube` `gcm iso gcz ciso wbfs rvz elf dol m3u json`, `wii` same + `wad`, `ps3` `ps3 psn squashfs`, `3ds` `3ds 3dsx cxi axf elf app squashfs`, `wiiu` `wua wup wud wux rpx squashfs wuhb`, `xbox` `iso squashfs`, `switch` `xci nsp`, `xbox360` `iso xex xbox360 zar`, `psvita` `zip psvita`. RetroBat: `3ds` adds `cia zcia`, `switch` `nso nro nca xci nsp kip`, `xbox` adds `xbe`, `psvita` `m3u psvita vpk`.
- Batocera's `es_systems.yml` at the pin has no extension lists for these keys (they are generated elsewhere); its folder names are the keys themselves.

## 7. System entries (research result)

`libretro` = exact folder in the live root listing of <https://thumbnails.libretro.com/> (127 folders, 2026-10-31). `dats` = paths at libretro-database `99421ed9` (sizes from the git tree; all under `MAX_DAT_BYTES` = 64 MiB, the largest is `metadat/no-intro/Microsoft - Xbox 360.dat` at 4.7 MB). SS = ScreenScraper id (Skyscraper `platforms_idmap.csv`). Family: C console, H handheld.

| id | name | aliases (first = D8) | extensions | weak | libretro (+ also) | dats | SS | fam |
|---|---|---|---|---|---|---|---|---|
| gamecube | Nintendo GameCube | GAMECUBE, gc, ngc, gcn | gcm, iso, ciso, gcz, rvz, wia, m3u | tgc, dol | Nintendo - GameCube | `dat/Nintendo - GameCube.dat` (GameTDB, ID6 serials), `metadat/redump/Nintendo - GameCube.dat`, `metadat/homebrew/Nintendo - GameCube.dat` | 13 | C, disc |
| wii | Nintendo Wii | WII | wbfs, iso, ciso, gcz, rvz, wia, m3u | wad, dol | Nintendo - Wii | `dat/Nintendo - Wii.dat` (GameTDB, ID6), `metadat/redump/Nintendo - Wii.dat` (`RVL-XXXX-…` serials), `metadat/no-intro/Nintendo - Wii (Digital).dat` (TMD CRCs) | 16 | C, disc |
| wiiu | Nintendo Wii U | WIIU | wud, wux, wua, rpx, wuhb | tmd | Nintendo - Wii U | `dat/Nintendo - Wii U.dat` (GameTDB, ID4), `metadat/no-intro/Nintendo - Wii U (Digital).dat` | 18 | C |
| 3ds | Nintendo 3DS | 3DS, n3ds, nintendo3ds | 3ds, cci, cxi, cia, 3dsx, zcci, zcxi, zcia, z3dsx | — | Nintendo - Nintendo 3DS (+ Nintendo - Nintendo 3DS (Digital)) | `metadat/no-intro/Nintendo - Nintendo 3DS.dat`, `… 3DS (Digital).dat`, `… New Nintendo 3DS.dat`, `… New Nintendo 3DS (Digital).dat` (product-code serials) | 17 | H |
| switch | Nintendo Switch | SWITCH, nintendoswitch, nsw, switch (scoped: SWITCH, switch) | xci, nsp, nsz, xcz | nro, nso, nca | — | — | 225 | C |
| psvita | Sony PlayStation Vita | PSVITA, vita (scoped) | vpk, psvita | psvimg, pkg | Sony - PlayStation Vita | `metadat/no-intro/Sony - PlayStation Vita.dat` (serials), `metadat/no-intro/Sony - PlayStation Vita (PSN).dat` (names) | 62 | H |
| ps3 | Sony PlayStation 3 | PS3, playstation3 | iso, ps3, psn, m3u | pkg | Sony - PlayStation 3 (+ Sony - PlayStation 3 (Downloadable)) | `dat/Sony - PlayStation 3.dat` (GameTDB serials), `metadat/redump/Sony - PlayStation 3.dat`, `metadat/no-intro/Sony - PlayStation 3 (PSN).dat` (names; its `.rap` rows are never matched against files, D4) | 59 | C, disc |
| xbox | Microsoft Xbox | XBOX | xiso, iso, m3u | xbe | Microsoft - Xbox | `metadat/redump/Microsoft - Xbox.dat` (`XX-NNN` serials) | 32 | C, disc |
| xbox360 | Microsoft Xbox 360 | XBOX360, x360 | xex, zar, xbox360, iso, m3u | — | Microsoft - Xbox 360 | `metadat/redump/Microsoft - Xbox 360.dat`, `metadat/no-intro/Microsoft - Xbox 360.dat`, `metadat/no-intro/Microsoft - Xbox 360 (Digital).dat` (rom names start with title id + content type) | 33 | C, disc |

Changes to existing entries: `nds` (D6) gains extension `dsi`, aliases `dsi`, `nintendodsi`, `dats = ["metadat/no-intro/Nintendo - Nintendo DS.dat", "metadat/no-intro/Nintendo - Nintendo DSi.dat"]` (the first is today's derived path), `libretro_also = ["Nintendo - Nintendo DSi"]`. `doom` and `gamecom` are unchanged (their `wad`/`tgc` stay strong; the new owners list them weak). Total after the batch: 127 systems.

`system_for_dat`: every new DAT is listed by one system only, so no tie (batch 1 §8.3 rule unchanged). `MAX_DAT_DOWNLOADS = 4` per plan: a pile with all nine systems needs 21 DAT files; the existing «catálogos no consultados» note covers what is not fetched, and the DATs of systems whose items have a header serial are fetched first (existing priority by item count is kept; the task checks the order and lists it in the PR).

## 8. Identification

### 8.0 Shared shape

```rust
pub struct ConsoleId {
    pub system: &'static str,        // "gamecube", "3ds", …
    pub confidence: f32,             // 0.95 magic + structure, 0.90 magic only
    pub serial: Option<String>,      // D22 form
    pub title_id: Option<String>,    // 16 hex (Nintendo), 8 hex (Xbox 360), AAAA00000 (Sony)
    pub title: Option<String>,       // plaintext title, cleaned (sources::clean_text, ≤ 128 chars)
    pub version: Option<String>,
    pub kind: TitleKind,             // Base | Update | Dlc | Demo | Unknown (D11)
    pub disc_no: Option<u8>,
    pub encrypted: Option<bool>,     // content encryption, when the header says
    pub art: Vec<EmbeddedArt>,       // D17 locators, never bytes
    pub detail: String,              // fixed vocabulary, e.g. "RVZ disc header", "PFS0 ticket name"
}
```

It becomes a sniff hit (`IdLevel::Header`) plus the serial that the existing DAT serial lookup uses; `detail` and `title_id` feed the «why» lines (§10). The `title` is used only as a name hint for matching and covers, never written to disk as a file name without the existing safe-name rules.

Kind from title ids (verify each against 3dbrew / switchbrew / wiiubrew title-id pages in the task; an unverified rule returns `Unknown`):

| System | Base | Update | DLC | Demo |
|---|---|---|---|---|
| 3ds | high `00040000` | `0004000E` | `0004008C` | `00040002` |
| wiiu | `00050000` | `0005000E` | `0005000C` | `00050002` |
| wii (WAD) | `00010001` (channel/WiiWare/VC) | — | `00010005` | — |
| switch | low 12 bits `000`, bit 12 clear | low 12 bits `800` | bit 12 set (base = id & ~0x1FFF) | — |
| ps3 / psvita | `CATEGORY` `DG`, `HG` / `gd` | `GD` / `gp` | — | — |
| xbox360 | content type `00007000` (GOD), `000D0000` (XBLA), XEX | `000B0000` | `00000002` | — |

### 8.1 GameCube and Wii (`consoles/nintendo_disc.rs`, `consoles/wad.rs`)

Disc header (first 0x80 bytes of the disc, in clear on every disc, Wii included): ID6 at 0x00 (system code, two-char game code, region, two-char maker), disc number 0x06, version 0x07, Wii magic `5D 1C 9E A3` (BE) at 0x18, GameCube magic `C2 33 9F 3D` at 0x1C, title at 0x20 (64 bytes, ASCII or Shift-JIS: kept only if it cleans to printable text). A header with neither magic is not a GameCube/Wii disc.

| Format | Where the 0x80-byte header is | Read | Notes / source |
|---|---|---|---|
| ISO / GCM | byte 0 | 128 bytes (already in `PREFIX`) | Dolphin `DiscIO/VolumeDisc`; NKit images keep the header (name/serial only, never Redump hashes) |
| TGC (GameCube) | magic `AE 0F 38 A2` at 0; embedded GCM header at the offset in the TGC header | ≤ 4 KiB | weak `tgc` (D7): a Game.com `.tgc` lacks the magic. Verify the header-offset field against Dolphin `TGCBlob` |
| CISO | `CISO` at 0, block size u32 LE at 4, block map (0x7FF8 bytes of 0/1) at 8, data at 0x8000 | header at 0x8000 when map[0] = 1 | **Discriminator from PSP CSO** (same magic): Nintendo CISO when u32@4 is a power of two ≥ 0x8000 and bytes 8..0x18 are all 0/1; otherwise `disc/cso.rs` as today. Source: Dolphin `CISOBlob` |
| GCZ | u32 LE `B1 0B C0 01` at 0; sub-type u32 (0 GC, 1 Wii) at 4; block size u32 at 0x18; block count at 0x1C; u64 pointers at 0x20 | pointer 0 and 1 (16 bytes), then block 0: stored if its pointer's top bit is set, else one zlib inflate capped at the block size and at 1 MiB | Dolphin `CompressedBlob` |
| WBFS | `WBFS` at 0; `hd_sec_sz_s` u8 at 8 | the disc slot's header copy at `1 << hd_sec_sz_s` (0x200 on every tool output seen) | split `.wbf1`…`.wbf3` are companions (§12). Source: libwbfs `wbfs_head_t` |
| WIA / RVZ | `WIA\x01` / `RVZ\x01` at 0; header 1 is 0x48 bytes; header 2 at 0x48 starts with `disc_type` (u32 BE: 1 GC, 2 Wii), compression, level, chunk size, then **`dhead[0x80]`, a raw copy of the disc header, at 0x58** | 0xD8 bytes, no decompression | Dolphin `docs/WiaAndRvz.md`. zstd/LZMA are not needed for the id |
| WAD (Wii) | header size u32 BE `0x20` at 0, type `Is\0\0` or `ib\0\0` at 4; section sizes (cert, ticket, TMD, data, footer) at 8…0x1F; sections start at 0x40, each 64-byte aligned | TMD (≤ 0x2000 bytes): title id u64 BE at TMD + 0x18C (RSA-2048 signature type `0x10004`), ID4 = its low 4 bytes | Title names are in the encrypted content 0: **name from the DAT**. The No-Intro Wii (Digital) DAT lists `tmd.<n>` rows by CRC and size: the CRC of the TMD section (a few KB) is matched through the existing CRC index (verify on one real WAD; if the bytes differ, drop to name only). The ticket is never read (D4) |

Split by magic: Wii magic → `wii`; GameCube magic → `gamecube`; WIA/RVZ `disc_type` and GCZ sub-type must agree with the magic (else the header wins, detail notes the disagreement). Encryption: Wii game partitions are encrypted with the common key; Coverly reads nothing past the clear header (D4). Wii discs report `encrypted: None` (normal for the format, no note).

### 8.2 Nintendo 3DS and Wii U (`consoles/ctr.rs`, `consoles/wiiu.rs`, `consoles/zarchive.rs`)

3DS (sources: 3dbrew NCSD, NCCH, CIA, Title metadata, SMDH, 3DSX pages; Azahar `src/common/file_derived.h` at `6e42de80` for `Z3DS`):

| Format | Rule | Id fields (all in clear) |
|---|---|---|
| NCSD (`.3ds`, `.cci`) | `NCSD` at 0x100; partition table at 0x120 (8 × offset/length u32 LE, media units of 0x200) | media id u64 LE at 0x108 (title id); partition 0's NCCH header: `NCCH` at +0x100, program id +0x118, **product code +0x150** (16 ASCII bytes, `CTR-P-BP4J`), flags +0x188 (`flags[7] & 0x04` = NoCrypto) |
| NCCH (`.cxi`) | `NCCH` at 0x100 | same fields at offset 0 |
| CIA | header size u32 LE `0x2020` at 0; cert/ticket/TMD/content/meta sizes at 8…0x1F; sections 64-byte aligned after the header | TMD: title id u64 BE, version u16, content records (type bit 0 = encrypted). If content 0 is not CIA-encrypted, its NCCH header gives the product code; if it is, only the TMD title id (name from DAT by product code is then impossible: name match). **Meta section** (when present): SMDH at meta + 0x400 → titles (UTF-16LE, 16 languages × short/long/publisher) and the 48×48 icon, in clear |
| 3DSX | `3DSX` at 0; header size u16 at 4; extended header (> 0x20): SMDH offset/size at 0x20/0x24 | homebrew: title from SMDH, no DAT |
| `Z3DS` (`.zcci`, `.zcxi`, `.zcia`, `.z3dsx`) | `Z3DS` at 0, underlying magic at 4, version u8 at 8, header size u16 at 0xA, metadata size u32 at 0xC, compressed/uncompressed sizes u64 at 0x10/0x18; then the metadata and zstd **seekable** frames | decode frames with `ruzstd` until the underlying header is covered (NCSD needs 0x4000 + 0x200 bytes; capped at 1 MiB decoded), then the rules above. The underlying magic value used for CIA is read from Azahar's writer in the task (not assumed) |

SMDH and ExeFS: the ExeFS `icon` file is an SMDH, but ExeFS is encrypted unless NoCrypto. So a decrypted image yields title and icon from ExeFS; an encrypted one yields product code and title id only, plus `encrypted: Some(true)` → note «Copia cifrada: Azahar solo abre copias descifradas» (D4: a fact about the emulator, no instruction). `seeddb.bin`, `aes_keys.txt` are key material.

Wii U (sources: wiiubrew `meta.xml`, `app.xml`, title ids; Cemu `WUD`/`WUX` readers; Exzap/ZArchive `zarchivecommon.h`, MIT-0):

| Format | Rule | Id fields |
|---|---|---|
| Folder (loadiine) | `code/app.xml` or `meta/meta.xml` (D12) | `meta.xml`: `title_id` (16 hex), `title_version`, `product_code` (`WUP-P-ARPE`), `company_code`, `longname_en`/`shortname_en` (fallback `_ja` etc.); ≤ 64 KiB, quick-xml, no DTD/entities. Icon `meta/iconTex.tga` (D17) |
| NUS (`title.tmd` + `*.app` + `title.tik`) | folder probe `title.tmd` | TMD title id and version (same TMD layout as 3DS/Wii, big-endian). Contents encrypted: no title; name from DAT by title id (the Wii U (Digital) DAT names rows by title id), else folder name. `title.tik`/`title.cetk` never read |
| WUD | raw disc image; the disc header starts with the product code string (`WUP-P-ARPE-0-…`) | verify on a real WUD header in the task; without it, `.wud` is extension evidence only (unique extension 0.85) |
| WUX | `WUX0` at 0 and `2E D0 99 10` at 4; sector size u32 at 8; uncompressed size u64 at 0x10; index table (u32 per sector) at 0x20; sectors at the next sector-aligned offset | sector 0 through the index → same as WUD |
| WUA | ZArchive: 0x90-byte footer at the end (BE; magic `0x169F52D6`, version `0x61BF3A01`), sections compressed-data / offset records / **names** / **file tree** / meta; only file data is compressed (64 KiB zstd blocks) | top-level folders are `<titleid16>_v<version>` (base, update, DLC in one archive → one item with kinds listed); `meta/meta.xml` of the base by decompressing its block(s), ≤ 256 KiB decoded |
| RPX | ELF magic with `e_ident[7..9] = CA FE` (Wii U ABI) | `wiiu` 0.90, no id: name only |
| WUHB | `WUHB` at 0 | `wiiu` 0.90, homebrew, name only |

### 8.3 Nintendo Switch (`consoles/nx.rs`) — what is and isn't possible

| Format | Recognised by | In clear (read) | Encrypted (never read) | Title id from | Name from |
|---|---|---|---|---|---|
| XCI | `HEAD` at 0x100 | XCI header fields up to 0x18F (secure-area start, size class, package id, HFS0 offset u64 at 0x130 and size at 0x138); root HFS0 (`HFS0`, file count, string table: `update`, `normal`, `secure`, `logo`); the secure partition's HFS0 entry names (`<32hex>.nca`, `<32hex>.cnmt.nca`) | gamecard info at 0x190 (header key), every NCA header and body (header key / title keys) | **file name only** (`[0100…]`) | file name |
| XCZ | as XCI | as XCI (entries `*.ncz`) | NCZ sections | file name only | file name |
| NSP | `PFS0` at 0 | PFS0 table: entry names and sizes; a `*.cnmt.xml` entry body (≤ 64 KiB) when present | NCAs, the `.tik` and `.cert` bodies (never opened, D4) | `.tik` entry name (rights id, first 16 hex), else `cnmt.xml` `<Id>`, else file name | `cnmt.xml` has no name; file name |
| NSZ | `PFS0` at 0 with `*.ncz` entries | as NSP | as NSP | as NSP | file name |
| Split «archive bit» folder (`Game.nsp/00`, `01`…) | D12 probe | the `00` part's head = PFS0/XCI head | as above | as above | folder name |
| NRO / NSO / NCA (weak) | `NRO0` at 0x10 / `NSO0` at 0 / none for NCA (header encrypted) | NRO homebrew: optional `ASET` icon/NACP at the end (in clear) — not used in this batch | NCA | — | file name |

Confidence: `HEAD` or `PFS0` + a plausible table (counts ≤ 1024, string table inside the file) → 0.95. Version: `cnmt.xml` `<Version>` or `[vNNN]` in the name. Kind: from the title id bits (§8.0). DLC and updates are grouped with a base when the base's id is in the same plan (D11). No catalogue download (D10). The Switch row of `systems.toml` has no `libretro` and no `dats`: identification is header (system) + name; covers from ScreenScraper 225 (by name and size; its romnom search) and SteamGridDB, as for any system without libretro thumbnails.

### 8.4 PS3 and PS Vita (`consoles/psf.rs`, `consoles/sony.rs`)

PSF (`PARAM.SFO`): magic `\0PSF` at 0, version u32 LE at 4, key-table start u32 LE at 8, data-table start at 0xC, entry count at 0x10; entries of 16 bytes (key offset u16, `fmt` u16 LE: `0x0004` UTF-8 special, `0x0204` UTF-8, `0x0404` u32; length u32; max length u32; data offset u32). Limits: file ≤ 64 KiB, ≤ 256 entries, each string ≤ 1 KiB, keys ASCII; anything out of bounds → not a PSF. Keys read: `TITLE_ID`, `TITLE`, `CONTENT_ID`, `APP_VER`, `VERSION`, `CATEGORY`. Source: psdevwiki `PARAM.SFO`. (Tanda 3's note calls `fmt` big-endian; read as LE u16 the values are `0x0204` etc., which is what psdevwiki documents. The PS4 task re-checks with a real file.)

| Form | Probe | Read | Id |
|---|---|---|---|
| PS3 JB folder | `PS3_GAME/PARAM.SFO` (D12) | that file; `PS3_DISC.SFB` at the root if present (`.SFB` magic, `TITLE_ID` field, verify offsets on psdevwiki) | `TITLE_ID` → `BLUS-30001`; `CATEGORY` `DG` base, `GD` update; art `PS3_GAME/ICON0.PNG` (320×176), `PIC1.PNG` not used |
| PS3 HDD/PSN folder | `PARAM.SFO` + `USRDIR/` at the folder root | `PARAM.SFO` | `HG` base, `GD` update; `ICON0.PNG` |
| PS3 ISO | ISO 9660 PVD with volume id `PS3VOLUME`; root `PS3_DISC.SFB` and `PS3_GAME/` | root via `find_root_file`, then **one** sub-directory read of `PS3_GAME` (≤ `MAX_ROOT_RECORDS`), then `PARAM.SFO` (≤ 64 KiB) | as JB. A Redump image keeps parts of the disc encrypted (disc key, `.dkey`, D4): if the bytes read for `PARAM.SFO` do not start with `\0PSF`, fall back to the disc's plaintext id at sector 1 (`PlayStation3` then the `BLUS-30001` code; offsets verified against a real Redump head in the task, else the fallback is dropped and the name decides) |
| PKG (PS3 and Vita) | magic `7F 50 4B 47` at 0; revision u16 BE at 4; type u16 BE at 6 (`1` PS3, `2` PSP/Vita) | content id at 0x30 (0x24 bytes, `UP0001-NPUB30001_00-…`) | title id = content id chars 7–15; system from the type; note `InstallFirst { emulator: "RPCS3" | "Vita3K" }` (PKGs are installed, not played from the folder). Licences (`.rap`, zRIF) are never read (D4) |
| Vita VPK | zip (D16) with `sce_sys/param.sfo` | that entry (≤ 64 KiB decoded) and the `sce_sys/icon0.png` locator | `TITLE_ID` → `PCSE-00844` (matches the No-Intro Vita DAT serial); `CATEGORY` `gd` base, `gp` patch |
| Vita app folder | `sce_sys/param.sfo` (D12); folder name is usually the title id | that file | as VPK; `sce_sys/package/work.bin` is key material, moved with the folder, never read |
| `.psvita` / `.psn` | text-native (D14) | ≤ 64 bytes | a title id when the text is one |
| `.psvimg` | weak extension | nothing (encrypted backup) | name only |

### 8.5 Xbox and Xbox 360 (`consoles/xbox.rs`, `consoles/zarchive.rs`)

XDVDFS: the volume descriptor `MICROSOFT*XBOX*MEDIA` (20 bytes) at partition + 0x10000, root directory sector u32 LE at +0x14 and size at +0x18; checked at partition offsets 0 (XISO), `0x18300000` (Redump XGD1), `0x0FD90000` (XGD2), `0x02080000` (XGD3) — four 2 KiB reads at most. The root is a binary tree of entries (left u16, right u16, sector u32, size u32, attributes u8, name length u8, name); walked iteratively, ≤ 2048 entries, ≤ 64 KiB, cycle-safe (visited set). `default.xbe` → `xbox`; `default.xex` → `xbox360`. Sources: xboxdevwiki XDVDFS, extract-xiso.

| Form | Rule | Id |
|---|---|---|
| XBE (in an image or a folder) | `XBEH` at 0; base address u32 at 0x104; certificate address u32 at 0x118 (offset = address − base, must be < 64 KiB) | certificate: title id u32 at +0x8, title name 40 × UTF-16LE at +0xC, version at +0xAC (verify). **Serial** = the two ASCII publisher bytes, a dash, the low 16 bits as three decimal digits: `0x4541000D` → `EA-013` (Redump's form). The XBE is not encrypted |
| XEX2 (loose, in an image, in a folder) | `XEX2` at 0; module flags u32 BE at 4; optional-header count at 0x14; headers (key u32, value u32) from 0x18, ≤ 64 | key `0x00040006` (execution info, value = offset): media id, version, base version, **title id u32** at +0xC, platform, type, disc number, disc count. The PE image is encrypted/compressed; never read. XEX1 and earlier: system by magic, no id |
| STFS / GOD (`CON `, `LIVE`, `PIRS` at 0) | metadata in clear | content type u32 BE at 0x344, title id u32 at 0x360, display name UTF-16BE at 0x411, title name at 0x1691, thumbnail size at 0x1712 and PNG at 0x171A (≤ 16 KiB, D17). Verify offsets on free60 STFS in the task |
| GOD / XBLA folder | D12 probe: `<titleid8>/<contenttype8>/<40hex>` (GOD adds `<40hex>.data/Data0000…`) | the 40-hex STFS header only | the top `<titleid8>` folder is one item; content type gives kind (§8.0) |
| ZAR (Xenia) | ZArchive (§8.2) | names and tree only; `default.xex` header through one block when the archive holds one at its root | as XEX |

Xbox 360 digital rows in the No-Intro DAT name the rom `<titleid8><contenttype8><hash>`: an STFS/GOD item is looked up by that 16-char prefix (new `dat_by_name_prefix`, indexed). Redump X360 serials (`AV-2079`) are catalogue numbers not present on the disc: images match by name.

### 8.6 Budgets and safety (all readers)

- Every reader takes a `Budgeted` reader (D24): counts bytes, seeks and decoded bytes; exceeding any cap returns `Err(Budget)` and the caller keeps the magic-only verdict.
- No offset or length from a file is trusted: every one is checked against the file length and the cap before a read; multiplications are checked; tables are capped (counts above stay unread).
- Text from headers goes through `sources::clean_text` (controls and bidi removed, ≤ 128 chars) before reaching the UI, logs or the cache.
- Readers never follow symlinks inside folder games (`symlink_metadata`); a probe path that is a symlink is ignored.
- Nothing here runs an external tool, and nothing is decompressed beyond the D24 cap.
- Inside archives (zip/7z of an ISO), only head-only rules apply (first `STREAM_CAP` bytes): GameCube/Wii ISO, WBFS, WIA/RVZ, CISO, NCSD/NCCH/CIA head, PFS0, XCI, XEX, STFS. Seeking rules (Xbox at 0x18300000, PS3 sub-directory, GCZ block) are not applied in archives: name only.

### 8.7 Covers

Order per item (existing fill flow plus D9 and D17): libretro `libretro` then `libretro_also` folders by canonical name, then by name; ScreenScraper (if enabled; by hash where the item was hashed, else by name and size); SteamGridDB (if a key is stored); then `embedded`. Media: `embedded` answers `Cover` only. For Switch the first step is empty by design (D18).

### 8.8 Hash and name

No image of this batch is hashed at the default cap (64 MiB). Small files are: DSiWare dumps (DSi DAT), 3DSX homebrew (no DAT), WAD TMD sections (§8.1). Everything else matches by header serial (DAT serial index) and then by name. `NAME_ONLY_EXTS` gains `gcz wia wbfs ciso nsp nsz xci xcz cia zcci zcia wua wux wud zar vpk pkg` (hashing compressed or container bytes matches nothing in these DATs, except VPK/CIA rows whose sizes are hundreds of MB anyway).

## 9. UX and UI design

The visible change is small: nine systems with silhouettes, folder games that move as one row with their size, a few new notes and reasons, and one new cover source. No new screen, no new token.

### 9.1 Design plan (frontend-design pass)

- **Colour**: existing semantic tokens only. Notes use the component and tokens the existing `organize.noFolderOnDevice` note uses (secondary text, no fill); `KeyMaterial` and `EmulatorData` rows are ordinary «Se quedan donde están» rows. No `--cr-warning` for encrypted content: it is information about the emulator, not a fault. No new hex values; `pnpm tokens:check` stays green.
- **Type**: Atkinson Hyperlegible Next for names, notes and reasons; **Atkinson Mono for codes and sizes only**: title ids (`0100A0D004FB0000`, `BLUS-30001`, `GALE01`), file names inside reasons (`PARAM.SFO`, `prod.keys`) and the folder size (`23,4 GB`), as the brand already does for numbers and paths. Sizes use the locale's decimal separator and one decimal above 1 GB.
- **Layout**: a folder game is one row in its system group, like a file, with the existing folder glyph before the name and the size right after it:

  ```
  Sony PlayStation 3                                   3
    ▢ Demon's Souls (BLUS-30443)        23,4 GB   → ps3/Demon's Souls (BLUS-30443).ps3
      La cabecera es de Sony PlayStation 3 (PARAM.SFO)
      Se renombra a Demon's Souls (BLUS-30443).ps3 para que ES-DE lo muestre
    ▤ Super Mario Odyssey [0100000000010000][v0].nsp   5,6 GB   → switch/
      Title ID 0100000000010000 tomado del nombre del archivo
    ▤ Super Mario Odyssey [0100000000010800][v393216].nsp
      Actualización de Super Mario Odyssey
  ```

  Left-aligned, notes indented under the row as today. Updates and DLC sit directly under their base (sorted after it). The group count (`3` above) stays a row count, as today; the plan summary sentence says how many rows are updates or DLC (§10 `organize.summary.updates`).
- **Motion**: none added.
- **Principle**: the console's own metadata is the evidence, so the row says where the id came from (`PARAM.SFO`, the ticket name, the file name). Trust is legible: «tomado del nombre del archivo» is weaker than «en PARAM.SFO», and the copy says which.

Review against generic defaults: the default would be coloured pills per note («Encrypted», «Update», «DLC») and a lock icon on key files. Rejected: pills turn every row into a badge wall, all-caps tags are a known tell, and a lock suggests Coverly could unlock something. One sentence per fact, in the same voice as the existing notes, keeps the review list scannable and screen-reader friendly (each note is plain text in the row's accessible description, as today). Sizes and ids in mono are the only typographic change, and they follow the brand's existing rule rather than adding one. The embedded source is named for what it is, «Imagen incluida en el juego», not «Embedded».

Accessibility: no information is carried by the glyph alone (the folder glyph is `aria-hidden`, the row's accessible name already includes «carpeta» through the existing pattern for directory items; the task checks and adds it if missing). Sizes are read as text («23,4 GB»).

### 9.2 Icons

RetroArch Monochrome files at `d9f96905`: `Nintendo - GameCube`, `Nintendo - Wii`, `Nintendo - Wii U`, `Nintendo - Nintendo 3DS`, `Nintendo - Switch`, `Sony - PlayStation Vita`, `Sony - PlayStation 3`, `Microsoft - Xbox`, `Microsoft - Xbox 360` (all present in the tree listing). Audit on the contact sheet (batch 1 D20): a wordmark-only file goes to `WORDMARK_IDS` with a Slot-mount fallback (`FALLBACK_ABBR`: `gamecube GC`, `wii WII`, `wiiu WIIU`, `3ds 3DS`, `switch NSW`, `psvita VITA`, `ps3 PS3`, `xbox XBOX`, `xbox360 X360`). `THIRD-PARTY-NOTICES.md`: count and list updated, licence paragraph unchanged. «Xbox» appears only as the system's own display name (`Microsoft Xbox`, `Microsoft Xbox 360`); no Microsoft branding beyond the RetroArch silhouette (`TRADEMARKS.md` rule).

### 9.3 Mock

`?mockPick=modern` (beside `downloads|loose|mixed|retro`), all synthetic names and sizes:
- `Metroid Prime (USA).rvz` (GameCube, `GM8E01`) and `Resident Evil 4 (USA) (Disc 1).rvz` / `(Disc 2).rvz` (disc set, `.m3u` on ES-DE);
- `Super Mario Galaxy (Europe).wbfs` + `.wbf1` (one item with its split part);
- `Excitebike - World Rally (Europe) (WiiWare).wad` (Wii by WAD header, `.wad` weak so doom is contradicted);
- `Super Mario 3D World [0005000010145D00]/` with `code/`, `content/`, `meta/meta.xml` (Wii U folder game, 1,7 GB);
- `Mario Kart 7 (Europe).3ds` (product code `CTR-P-AMKP`), `Pokemon Y (Europe).cia` (encrypted: note), `model.3ds` (an Autodesk file: no NCSD → left, unknown);
- `Super Mario Odyssey [0100000000010000][v0].nsp` (ticket name), its update `…[0100000000010800][v393216].nsp`, `Zelda BotW.xci` (no id in name: name only);
- `PCSE00844/` with `sce_sys/param.sfo` (Vita folder game), `Persona 4 Golden.vpk`;
- `Demon's Souls (BLUS-30443)/` with `PS3_GAME/PARAM.SFO` (JB; `.ps3` suffix on ES-DE and Batocera), `NPUB30024.pkg` (install-first note);
- `Halo (USA).iso` (Xbox, Redump XGD1 offset, `MS-004`), `Halo 3 (USA).iso` (Xbox 360), `58410889/000D0000/…` (XBLA folder game);
- `prod.keys`, `title.keys`, `NPUB30024.rap` (all left: key material);
- `ux0/app/PCSB00245/` (left: emulator data).

## 10. Copy (es / en)

Spanish: sentence case, no exclamation marks, exact counts, numbers and codes in mono by the renderer.

| Key | es | en |
|---|---|---|
| `organize.left.keyMaterial` | Archivo de claves o licencia: Coverly no lo lee ni lo mueve | Key or licence file: Coverly never reads or moves it |
| `organize.left.emulatorData` | Está dentro de los datos de un emulador ({folder}); se deja tal cual | Inside an emulator's data ({folder}); left as it is |
| `organize.why.titleIdFile` | Title ID {id} en {file} | Title ID {id} in {file} |
| `organize.why.titleIdName` | Title ID {id} tomado del nombre del archivo | Title ID {id} taken from the file name |
| `organize.note.update` | Actualización de {title} | Update for {title} |
| `organize.note.dlc` | Contenido descargable de {title} | Downloadable content for {title} |
| `organize.note.encryptedContent` | Copia cifrada: {emulator} solo abre copias descifradas | Encrypted copy: {emulator} only opens decrypted copies |
| `organize.note.installFirst` | {emulator} lo instala; no se abre desde la carpeta | {emulator} installs it; it does not open from the folder |
| `organize.note.folderSuffix` | Se renombra a {name} para que {layout} lo muestre | Renamed to {name} so {layout} shows it |
| `organize.summary.updates` | {count, plural, one {Incluye # actualización o contenido descargable} other {Incluye # actualizaciones o contenidos descargables}} | {count, plural, one {Includes # update or DLC} other {Includes # updates or DLC}} |
| `settings.sources.name.embedded` | Imagen incluida en el juego | Image from the game files |
| `settings.sources.hint.embedded` | Usa el icono que traen algunos juegos (PS3, PS Vita, Wii U, 3DS, Xbox 360) cuando no hay carátula en otra fuente | Uses the icon some games carry (PS3, PS Vita, Wii U, 3DS, Xbox 360) when no other source has a cover |

`{emulator}` values are fixed strings from the code (`Azahar`, `RPCS3`, `Vita3K`), never from a file. `{file}` is one of a fixed set (`PARAM.SFO`, `meta.xml`, `app.xml`, `default.xbe`, `default.xex`, `cnmt.xml`, `el nombre del ticket` / `the ticket name`): the ticket case uses its own key variant `organize.why.titleIdTicket` «Title ID {id} en el nombre del ticket» / «Title ID {id} in the ticket name», so no Spanish word is interpolated. Existing keys reused unchanged: `organize.why.header` (detail strings such as `RVZ disc header`, `NCCH`, `PFS0`), `organize.why.disc` (with the serial), `organize.left.folderGame` (not used for Tanda 2 folder games, which move; kept for batch 1 ports), `organize.left.tooBigForFat`, `organize.noFolderOnDevice`, `organize.why.extensionContradicted` (`.wad`/`.tgc`). Plural syntax follows the existing ICU usage in `ui.json` (the task matches the exact form other plural keys use). Other workflows edit `locales/*.json` in parallel: add keys in sorted place and resolve conflicts key by key. Parity test covers the new keys.

## 11. Data flow

1. **Load**: `SystemDb` parses `libretro_also` (lenient for user files, like batch 1's fields); validation checks the new weak/strong disjointness and scoped aliases.
2. **Walk** (`organize/walk.rs`): for each entry, key-material names → `KeyMaterial`, never opened; a folder under an emulator-data root (`ux0`, `dev_hdd0`, `mlc01`, `sdmc` with `Nintendo 3DS/`, `nand`) → `EmulatorData`; a folder matching a D12 probe → one folder item with the probe path recorded; batch 1 rules otherwise.
3. **Inspect**: raw file → sniff (head) → `consoles::identify_raw` (seeking, budgeted) when relevant; folder item → `consoles::identify_dir` reads only the probe file(s); archives → head-only rules on the first entry bytes.
4. **Evidence**: header hit (0.90/0.95) + extension (strong/Shared/weak per D7, D24 contradiction for `.wad`/`.tgc`) + DAT serial (D22) + name. Kind and title id travel with the facts (`FACTS_VERSION` 7).
5. **Target**: `system_folder(profile, system)` from §6; folder items keep their folder name (+ D13 suffix); disc systems follow `disc_layout`; free-space warning and `TooBigForFat` as today; the folder size is the metadata sum.
6. **Grouping**: updates/DLC listed under their base (same title id base) within a system group; summary counts.
7. **Covers**: §8.7; the embedded locator is stored with the item's facts and re-validated at fetch.
8. **Dedupe**: title id + kind + version keys (D11); folder games are units; key material and emulator data are out of scope.
9. **UI**: rows, notes and reasons (§9.1, §10).

## 12. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| `.iso` that is GameCube, Wii, PS3, Xbox, Xbox 360, PS2 or PSP | One `DiscReader` pass: GameCube/Wii magic at 0 first (not ISO 9660), then the PVD rules (PlayStation family, `PS3VOLUME`), then XDVDFS offsets. Generic extension: no extension evidence. |
| `.ciso` from a PSP tool | The discriminator (§8.1) sends it to `disc/cso.rs`; a `.cso` is never read as Nintendo. |
| `.wad` | `IWAD`/`PWAD` → doom (as today); Wii WAD header → wii, doom contradicted; neither → doom 0.60 (batch 1), never Auto. |
| `.tgc` | TGC magic → gamecube, gamecom contradicted; else gamecom as today. |
| `.3ds` without `NCSD` (Autodesk) | `3ds` is `DECISIVE` → 0.60 → left as unknown in Downloads, never Auto. |
| `.pkg` macOS installer (`xar!` magic) | Weak extension only, no PKG magic → nothing; left as unknown (it is not a ROM). |
| Encrypted 3DS / CIA | Identified by product code or title id; note `encryptedContent`; moved normally. |
| XCI with no title id in its name | Switch at 0.95 by header; no serial; name from the file name; covers by name. |
| NSP whose ticket name has a rights id with a non-zero key generation | Title id = first 16 hex digits regardless; key generation ignored. |
| Split Switch folder `Game.nsp/00` | One item; on FAT32 it is the supported way to keep > 4 GiB, so no `TooBigForFat`. |
| Split WBFS `.wbfs` + `.wbf1…3` | Companions move together (same stem); a `.wbf1` without its `.wbfs` → left, `setIncomplete`. |
| Wii ISO 4.7 GB on FAT32 | `TooBigForFat` (existing); the copy says exFAT, as today. |
| PS3 JB folder already ending in `.ps3` | No double suffix. On a profile without D13, the suffix is kept as the user named it. |
| PS3 JB folder missing `PS3_GAME/USRDIR/EBOOT.BIN` | Still a PS3 folder game (the probe is `PARAM.SFO`); no completeness check (would need a walk). |
| Folder games nested (`PS3/` holding several JB folders) | The pile's `PS3/` resolves as a system folder → «already sorted» (existing). A non-system folder holding several JB folders → each child is an item. |
| A folder that matches two probes (`default.xbe` and `PS3_GAME/`) | First match in the fixed order of D12 wins, detail names both; listed in review instead when the two systems differ (it is not a real game). |
| `ux0/app/PCSB00245/` (Vita3K data), `dev_hdd0/game/` (RPCS3), `mlc01/usr/title/` (Cemu), `sdmc/Nintendo 3DS/` (Azahar), Ryujinx/yuzu `nand/` | `EmulatorData`: moving them would break the emulator's install. |
| `prod.keys`, `title.keys`, `.rap`, `.rif`, `.dkey`, `.zrif`, `seeddb.bin`, `aes_keys.txt`, `act.dat`, loose `work.bin`, loose `.tik`/`.cetk` | `KeyMaterial`, never opened (test asserts no read: §14.1). |
| Key material inside a game folder (`work.bin`, `title.tik`) | Moves with the folder, unread. |
| A `.rap` row in the PS3 PSN DAT | Never matched against a file (key material is never hashed, never an item). |
| Cloud placeholder (iCloud/OneDrive) | Not read (existing `cloud_only`); left `cloudOnly`. |
| Truncated or corrupt header (offset past EOF, counts absurd) | Magic-only verdict (0.90) or nothing; never a panic; fuzz-style tests (§14.1). |
| Budget exceeded (huge PFS0 table, deep XDVDFS tree) | Partial: system by magic, no id; name decides. |
| Title text in Shift-JIS / UTF-16 with control characters | Cleaned (`clean_text`); if empty after cleaning, not used. |
| Two NSPs of the same base and version | Dedupe duplicates; different versions: duplicates, keeper = higher version unless the user's keeper rule says otherwise; update vs base: never duplicates. |
| `MAX_DAT_DOWNLOADS` reached | Existing «catálogos no consultados» note; header serials still give system and id. |
| Old plans / facts | `FACTS_VERSION` 7 re-reads lazily; plans with the new reasons are read only by this version (downgrade unsupported, as before). |
| Windows | Fixture paths compared by component, mtimes pinned; no new folder name is reserved (`CON`, `AUX`…) or ends in a dot/space; `.ps3` suffix is valid on FAT32/exFAT/NTFS; probe paths built with `Path::join`, never `/`-joined strings. |
| Case-sensitive cards (Linux) | Probe paths are matched case-insensitively within their one directory read (`PS3_GAME` vs `ps3_game`), never by guessing a spelling. |

## 13. Blocked, deferred and out of scope

Needs an owner decision (implemented around; listed in the PR):
- **Switch title-id catalogue.** `blawar/titledb` (MIT, ~90 MB JSON per region) is the only open title-id → name source found; GameTDB's Switch list uses product codes and states no licence on its download page. Default: no catalogue (D10). Options: a trimmed id→name file built by a script and shipped in the repo (licence check of the eShop-derived data first), or a user-supplied file.
- **GameTDB data licence** for other systems: Coverly already consumes GameTDB-derived `dat/` files through libretro-database (as published there); fetching GameTDB directly is not done.
- **Batocera vs Knulli for `switch`** (D19): Named by default.
- **Launcher files** (`.psvita`, `.psn`) that ES-DE and Batocera need for installed Vita/PS3 games: not created (D14). ES-DE's `psvita` folder therefore shows nothing Coverly moves there; listed in the PR with the per-frontend extension mismatches of §6.
- **ES-DE media folder** for `gc` and `n3ds` (batch 1 §13 `{system_folder}` token question, now with two more systems).
- **Extracting archived big games** (a zip holding a 25 GB ISO): the existing extract path with the free-space warning applies; whether to require an extra confirmation above a size is an owner choice (Tanda 3 asks the same).
- **Encrypted-copy note wording** (`encryptedContent`): factual, no instructions; the owner may prefer no note.
- **Unverified offsets** marked «verify» in §8 (TGC header offset, WUD product code, PS3 sector-1 id, XBE version offset, STFS thumbnail offsets, Wii WAD TMD CRC match, 3DS/Wii U/Switch kind bits): each is checked against the cited source and a real header during its task; a rule that cannot be verified is dropped (system by magic, name for the rest) and the drop is listed in the PR.

Out of scope: decrypting anything, reading NCA/NCZ contents, NRO `ASET` icons, GameCube/Wii banners, XBE title images, `.psvimg` contents, converting formats (ISO→RVZ, NSP→NSZ), installing PKGs, a Knulli-only profile, a RetroBat profile (RetroBat's folders are recorded in §6 and equal the Batocera column except `switch`), Tanda 3.

## 14. Test plan

### 14.1 Rust (`cargo test --workspace`)

Fixtures are **synthetic byte heads built in the tests** (no copyrighted data, no keys), small sparse files for the far offsets (Xbox `0x18300000` via `set_len`, never written in full), created under `tempfile` or `target/` with pinned mtimes and removed afterwards.

- `consoles::nintendo_disc`: one positive and one negative per format (ISO/GCM GC and Wii, TGC, CISO, GCZ stored and zlib block, WBFS, WIA, RVZ); the CISO/CSO discriminator both ways; disc number → `disc_no`; GCZ sub-type disagreeing with the magic.
- `consoles::wad`: header, TMD title id, kind; a TMD-section CRC matches a DAT row built in the test.
- `consoles::ctr`: NCSD (partition 0 NCCH, product code, NoCrypto), CXI, CIA (title id from TMD, encrypted flag, SMDH from meta), 3DSX with SMDH, `Z3DS` around NCSD with a real zstd frame produced by the dev-dependency encoder; SMDH RGB565 tile decode against a known 8×8 pattern.
- `consoles::nx`: XCI head + HFS0 names (no title id), NSP with a `.tik` name, NSP with `cnmt.xml`, NSZ, file-name token parsing (`[0100…][v65536]`), kind bits; **a test that the `.tik` and NCA entry bytes are never read** (a `ReadAt` wrapper records ranges; only the PFS0 table and the XML range are touched).
- `consoles::psf`: valid file, each bound violated (entry count, offsets, string length, non-ASCII key) → `None`; `fmt` values.
- `consoles::sony`: JB folder, HDD folder, PS3 ISO (ISO 9660 builder from `disc/testing.rs` with a `PS3_GAME` sub-directory), PKG PS3 and Vita, VPK zip entries, Vita folder, `.psvita` text.
- `consoles::xbox`: XDVDFS at each of the four offsets (sparse files), tree with a cycle (terminates), XBE certificate → `EA-013`, XEX2 execution info, STFS metadata and thumbnail locator, GOD folder probe.
- `consoles::wiiu` / `zarchive`: `meta.xml` (with an entity attempt: rejected), `app.xml`, TMD, WUX index, ZArchive footer + names + one block, RPX/WUHB magic.
- **Budgets**: every reader under a counting reader stays within D24 caps on its largest fixture; an adversarial table (count 2^31) stops at the cap.
- **Key material**: on Unix, `prod.keys`, `title.keys`, `x.rap` created with mode `000`: the plan leaves them with `KeyMaterial` (not `Unreadable`), proving no open was attempted; on Windows the same files with a deny-read ACL is not attempted (test is `#[cfg(unix)]`), and a pure-function test of `is_key_material` runs everywhere.
- `systems`: nine entries load; `libretro_also` lenient merge; extension-class snapshot diff from `main` is exactly D7 (`gcz rvz wia ciso` Shared; `wad tgc pkg xbe nro nso nca psvimg tmd` weak only; `dsi` new on nds); `switch`/`vita` scoped; `3ds` in `DECISIVE`.
- `profiles`: for every built-in × every Tanda 2 system, `system_folder` returns §6's name or the rule name with `on_device = Some(false)`; every `Named` resolves back under its profile; `ps3` suffix on esde and batocera only; `3ds` on muOS; Anbernic, Onion, GarlicOS, MinUI Absent.
- `organize_walk`: each D12 probe → one folder item; emulator-data roots left; a folder with both probes → review; split NSP folder → one item; probes are case-insensitive within one `read_dir`; symlinked probe ignored.
- `organize_evidence`: `.wad` Wii header contradicts doom; `.tgc` TGC magic contradicts gamecom; `.3ds` Autodesk → 0.60; `.pkg` xar → nothing; `.gcz` without header → Shared review.
- `organize_target`: per system the right folder on esde, batocera, muos; JB folder → `ps3/<name>.ps3` on esde; Wii ISO > 4 GiB on a FAT32 target → `TooBigForFat`; `.wbfs` + `.wbf1` move together.
- `organize_plan` snapshots: `organize_plan__systems_batch_2__{esde,batocera,muos}.snap` over the §9.3 pile (built under `target/` with `cp -RXp` from a fixture tree, pinned mtimes, no `._` files, removed afterwards).
- `organize_plan` facts: a version 6 row is re-read and becomes `gamecube` (D23).
- `dedupe`: base + update never duplicates; two equal bases duplicates; folder games are units.
- `sources::libretro`: every §7 `libretro` and `libretro_also` name is in the committed copy of the live root listing; `sources::embedded`: PNG from a VPK entry, TGA from a Wii U folder, SMDH icon → PNG; a changed mtime invalidates the locator.
- `dat`: every §7 DAT path exists in a committed listing of libretro-database at `99421ed9`; parse trimmed fixtures of `dat/Nintendo - GameCube.dat` (GameTDB layout) and of the Wii Redump serial form (`RVL-SP3E-USA-B0` → middle key `SP3E`); Xbox 360 name-prefix lookup.
- Existing snapshots that change (`list_systems`, any plan snapshot with `.wad`, `.tgc`, `.iso` candidates): each diff reviewed and explained in its commit.

### 14.2 Vitest (`pnpm check-all`)

- `organize.ts`: copy for `key-material`, `emulator-data`, each new note, `titleIdFile`/`titleIdName`/`titleIdTicket`; summary plural.
- `SystemGroups`: a folder item renders the folder glyph (`aria-hidden`) and the size in mono with the locale separator (`23,4 GB` in es, `23.4 GB` in en); updates listed right after their base.
- Sources settings: the `embedded` row renders its name and hint and has no key field.
- `SystemIcon`: `gamecube` and `xbox360` render silhouettes (or their mount if audited out); `systemIcons.test.ts` passes on the regenerated module.
- Contract test with the new enum members; i18n parity; Spanish copy check (sentence case, no «!»).
- `mockOrganize`: `?mockPick=modern` resolves; the pile produces the §9.3 rows.

### 14.3 E2E (Chrome DevTools MCP, isolated context `systems-batch-2`, dev server on port **1466**)

1. `?mockPick=modern` → organise offer → plan: GameCube, Wii, Wii U, 3DS, Switch, PS Vita, PS3, Xbox and Xbox 360 groups with silhouettes; the RE4 disc set as one item; `Demon's Souls` as one folder row with `23,4 GB` and the `.ps3` rename note on ES-DE.
2. The Switch group: the NSP with «Title ID … tomado del nombre del archivo» or «… en el nombre del ticket», its update right under it with «Actualización de Super Mario Odyssey»; the summary line counts the update.
3. Left in place: `prod.keys`, `title.keys`, `NPUB30024.rap` with the key-material reason; `ux0/app/PCSB00245` with emulator data; `model.3ds` unknown.
4. Switch profile to Batocera: Switch goes to `switch/` with no «no tiene carpeta» note (D19); to muOS: Wii shows «muOS no tiene carpeta para Nintendo Wii; se crea wii.».
5. Keyboard-only: Tab through the review rows, open the candidates of `Halo 3 (USA).iso` if listed in review, and check the accessibility tree (`take_snapshot`) reads each note as text and the size as «23,4 GB».
6. Library shelves with `GAMECUBE`, `PS3`, `SWITCH`: light and dark (`data-theme`), `prefers-reduced-motion: reduce`, at 1280 px and the narrowest supported width; screenshots attached to the PR.

Stop the server only by port (`lsof -ti tcp:1466 -sTCP:LISTEN | xargs -r kill`). Fallback: Playwright MCP, then `playwright-core` + Chrome for Testing in the scratchpad (`…/scratchpad/systems-batch-2`).

### 14.4 Gates

`pnpm check-all`, `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`. Read-only sources (`~/Documents/ROMS`, `/Volumes/TinyExt/*`, `/Volumes/System`) are never written; any real-header check copies a file's head with `cp -RXp` under the worktree's `target/` and deletes it afterwards.

## 15. Tasks (one reviewable commit series each, pushed after every commit)

1. Core: `consoles/mod.rs` (`ConsoleId`, `Budgeted`, key material), `libretro_also`, `normalise_console_serial`, new reasons/notes enums, `FACTS_VERSION` 7, walk key-material and emulator-data rules, folder-item move plumbing (D12) with a dummy probe.
2. GameCube + Wii: `nintendo_disc.rs`, `wad.rs`, `DiscReader` branch, `.wad`/`.tgc` weak, systems + profile rows, DAT serial middle key.
3. 3DS: `ctr.rs`, `Z3DS`, SMDH; systems + profile rows; `3ds` `DECISIVE`.
4. Wii U: `wiiu.rs`, `zarchive.rs`; folder probes; systems + rows.
5. Switch: `nx.rs`; split folder probe; systems + rows (D19).
6. PSF + PS3 + Vita: `psf.rs`, `sony.rs`; JB/HDD/Vita probes; PKG; VPK package; `.ps3` suffix (D13); systems + rows.
7. Xbox + Xbox 360: `xbox.rs` (XDVDFS, XBE, XEX, STFS/GOD), ZAR; probes; systems + rows; DAT name-prefix lookup.
8. DSi (D6): `nds` changes, unit code detail.
9. Dedupe kinds and versions (D11).
10. Embedded covers (D17): `sources/embedded.rs`, `image` `tga`, settings order.
11. Icons: sources, audit, regenerate, notices.
12. UI: types/contract, copy, folder rows with size, notes, sources row, mock pile.
13. E2E pass; PR (English, no AI attribution) with §13's list, the per-frontend extension mismatches and every dropped «verify» rule.

Other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel: `git fetch origin && git merge origin/main` before tasks 11–13 and resolve conflicts by hand; never force-push.

## 16. Sources (fetched 2026-10-31)

| What | Source | Pinned at |
|---|---|---|
| libretro-thumbnails folders | <https://thumbnails.libretro.com/> live root listing (127 folders) | 2026-10-31 |
| libretro-database DAT paths, sizes and serial forms | <https://github.com/libretro/libretro-database> git tree and raw files | `99421ed934c3f40260693f0b749d2ab442968cf4` |
| ES-DE folders and extensions | `resources/systems/linux/es_systems.xml` | `ef267ae693caeee89ca8ca888a38e9eab9bce90a` |
| Batocera folders (keys and comments) | `package/batocera/emulationstation/batocera-es-system/es_systems.yml` | `a3d49969295830ff1f136d393e33839f858fa8d7` |
| Knulli folders and extensions | same path in `knulli-cfw/distribution` | `ab3a6809286b3cff1648fb8c96a0200bc869beb6` |
| RetroBat folders and extensions | `RetroBat-Official/retrobat` `system/templates/emulationstation/es_systems.cfg` | `54e5e31d459adab8a10f29224af0069d20baa891` |
| muOS manifests | `MustardOS/internal` `share/info/manifest/core/**` (130 files; only 3DS and DS relevant) | `0784148faeeeb43703922958a03bcd45905b9639` |
| Onion packages | `OnionUI/Onion` `static/packages/Emu` (54) | `07505ea58c7bba698d6b9220ff43946a43cac76b` |
| NextUI paks | `LoveRetro/NextUI` | `5291344b3aceae5fa197437a70824afb41e2d338` |
| ScreenScraper ids | Skyscraper `platforms_idmap.csv` | master, 2026-10-31 |
| `nod` evaluation | crates.io `nod` 1.4.4 (`MIT OR Apache-2.0`), `encounter/nod` `nod/Cargo.toml` (default features vendor `liblzma-sys`, `zstd-sys`), `nod/src/disc/wii.rs` (`COMMON_KEYS`) | `ebd80cac99b48a323200d84365e07a58cc27412d` |
| Other crates looked at | crates.io `nx-archive` 0.1.2 (MIT), `xex` 0.2.0 (MIT) | 2026-10-31 |
| `Z3DS` header | `azahar-emu/azahar` `src/common/file_derived.h` (GPL-3.0; format read, no code copied) | `6e42de80de9f74023a0e14a8cbbfd8926ea394da` |
| ZArchive layout | `Exzap/ZArchive` `include/zarchive/zarchivecommon.h` (MIT-0) | master, 2026-10-31 |
| Switch title-id data | `blawar/titledb` (MIT; region JSON ~90 MB), GameTDB `switchtdb.txt` (product codes) | 2026-10-31 |
| Format references for «verify» items | Dolphin `Source/Core/DiscIO` and `docs/WiaAndRvz.md`; 3dbrew; switchbrew; wiiubrew; psdevwiki; xboxdevwiki; free60 | read in each task |
| Icons | `libretro/retroarch-assets` `src/xmb/monochrome` | `d9f969054dc7fbb6fa89519036d2b971e0855b51` |

## 17. Self-review

- Placeholders: none left open; every «verify» in §8 names its source and its fallback (drop the rule, keep magic + name), and §13 lists them for the PR.
- Contradictions checked: D7 vs §7 (every weak extension is listed under its new owner and stays strong under its old one; `gcz rvz wia ciso` appear as strong on both GameCube and Wii, which is what Shared means); D12 vs batch 1 D15 (ports stay «left», console folder games move: stated with the reason); D15 vs §6 (`multi_file` systems use each profile's existing `disc_layout`; no per-system override); D10 vs §8.3 (XCI never has a clear title id; no catalogue); D17 vs §8.7 (embedded is last and Cover only); D19 vs batch 1 D7 (opposite default, reason stated); D4 vs §8 (no reader reads a `.tik`, NCA, `.rap`, `work.bin` body; `title.tik` in Wii U NUS folders is never read).
- Counts: 118 existing + 9 new = 127 systems; §6 and §7 both have nine new rows; `FALLBACK_ABBR` entries ≤ 4 characters.
- Brief rules: no folder guessed (each `Named` cell traces to §16, RetroBat recorded but no profile added); ambiguous extensions never auto-assigned by extension alone; headers only, within budget (D24, §8.6); no key ever required, bundled, downloaded or read (D4); what Switch allows in clear is tabulated (§8.3); owner decisions listed without blocking (§13).
- Fixed during self-review: D24's partial-read wording (a budget overrun simply yields no serial, no special detail string); §9.1's group-count sentence (the group count stays a row count, the summary sentence states updates); the ticket case in `organize.why.titleIdFile` would have interpolated a Spanish phrase, so it has its own key.
