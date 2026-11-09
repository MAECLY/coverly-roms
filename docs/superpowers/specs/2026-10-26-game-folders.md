# Coverly ROMs — The game as a folder: base, Updates/, DLC/, demos, versions and revisions (Design Spec, part A)

> Status: **Design, ready to implement on `feat/game-folders`** (self-reviewed §17) · Date: 2026-10-26 (brief decided with Miguel on 2026-11-01; spec written 2026-11-08) · Owner: Miguel Angel Esparza Calero (MAECLY)
> Roadmap item: `TODO.md` §2.1 «Actualizaciones, DLC, demos y revisiones dentro de su juego», **part A only**. Parts B (Mods/ and Cheats/ detection, per-emulator translators), C (the six views and art slots for updates/DLC, which need industry research and a design Miguel approves first) and D (Extras/ CRUD, `extras.json`, media viewer) are later runs and are not started here.
> Branch: `feat/game-folders`. `docs/TRACKER.md` and `TODO.md` are updated on `main` after merge, not on this branch.
> Builds on: systems batch 2 ([`2026-10-26-systems-batch-2.md`](2026-10-26-systems-batch-2.md): `TitleKind`, D4 keys, D11 base linking, D25 folder moves, D26 folder games in the scan), systems batch 3 ([`2026-10-26-systems-batch-3.md`](2026-10-26-systems-batch-3.md): `ConsoleId.region`, D11 shadPS4 overlays, D16 Batocera DLC folder), duplicates ([`2026-10-18-duplicates-design.md`](2026-10-18-duplicates-design.md): level 2 «versions», keeper rules), research [`docs/02-research/updates-dlc-per-system.md`](../../02-research/updates-dlc-per-system.md) and [`docs/02-research/mods-cheats-extras-per-system.md`](../../02-research/mods-cheats-extras-per-system.md), brand [`docs/08-brand/brand.md`](../../08-brand/brand.md).

## 1. Purpose

Today Coverly knows that a file is a base game, an update, a DLC or a demo (by the title id or `CATEGORY` it reads in clear), links an update or DLC to its base when both are in the pile, and lists it under the base in Organizar. On disk, though, everything still lands flat in the system folder (`3DS/Pokemon.cia`, `3DS/Pokemon update 1.cia`), retro revisions are unrelated files, and the library scan would list every update as a game of its own.

Part A turns the game into a folder, as Miguel decided on 2026-11-01:

```
ROMS/PS4/Hatsune Miku Project DIVA Future Tone (USA)/
    Hatsune Miku Project DIVA Future Tone (USA).pkg
    DLC/
        Hatsune Miku Project DIVA Future Tone (USA) - Song Pack 1.pkg
    Updates/
        Hatsune Miku Project DIVA Future Tone (USA) (v01.05).pkg
ROMS/PS4/Hatsune Miku Project DIVA Future Tone (USA) (Demo)/
```

It delivers: the profile option that turns the layout on; Organizar moving base, updates and DLC into it with full undo; versions and regions read from headers into the names; detection extended to every system that has updates or DLC; licences and system files recognised by name only and left where they belong; retro revisions grouped as versions of one game; and the minimal UI to see and switch it. Three rules carry over unchanged: **keys are untouchable** (never read, include, download or decrypt), **nothing is overwritten**, and **every change is undoable**.

## 2. The owner's asks (verbatim from `TODO.md` §2.1)

> ### Estructura maestra de la biblioteca (decidida con Miguel, 2026-11-01)
>
> - **Una carpeta por juego** con el juego base en su raíz, y dentro `DLC/` y `Updates/` (nombres fijos en inglés para que emuladores y herramientas los reconozcan; la interfaz los muestra traducidos).
> - **Demos aparte**, como juego independiente, con el sufijo `(Demo)` de No-Intro/Redump (no `DEMO`) para que se ordenen junto al juego; relacionadas con el base en la ficha, pero no dentro de su carpeta.
> - **Versión en el nombre** de cada actualización (`… (v1.05)`), sacada de la cabecera, para saber cuál es la más nueva.
> - **Regiones:** si hay el mismo juego en varias regiones, una carpeta por región con la región en el nombre (`… (USA)`, `… (Japan)`); cada actualización y DLC va con la región de su title ID.
> - **Revisiones retro** (`(Rev 1)`, `(v1.1)`) como versiones del mismo juego en la ficha (no como juegos distintos); en disco siguen siendo archivos sueltos salvo que el perfil pida carpetas.
> - La misma carpeta del juego es la que usará `Saves/` del bloque 5.
>
> ### Carpetas fijas y `Extras/` libre (decidido con Miguel, 2026-11-01)
>
> - **Carpetas fijas del juego:** `Updates/`, `DLC/`, `Mods/`, `Cheats/`, `Extras/` y `Saves/`. Siempre con ese nombre y en ese sitio, porque de ellas dependen la organización automática, la traducción a cada emulador (4) y las partidas (5). El usuario no crea carpetas nuevas a ese nivel; la interfaz las muestra traducidas.
> - **Licencias** (`.rap`, zRIF, `work.bin`, DLC de solo licencia `AL`): se reconocen y se dejan junto al archivo al que pertenecen, sin leerlas nunca.
> - **No son juegos:** actualizaciones de sistema y firmware (particiones de actualización de cartuchos 3DS/Wii, `PS3UPDAT.PUP`, `0004001B…`), cachés de shaders y configuraciones por juego del emulador: se reconocen y se dejan donde están.
>
> ### Tareas (las de la parte A)
>
> - [ ] Crear y respetar las carpetas fijas en Organizar y en la biblioteca, con deshacer.
> - [ ] Opción de perfil «carpeta por juego con `DLC/` y `Updates/`» para la biblioteca y los perfiles que la acepten.
> - [ ] Organizar: mover base, actualizaciones y DLC a la carpeta del juego con deshacer; demos a su carpeta `(Demo)`.
> - [ ] Nombres con versión y región sacadas de las cabeceras.
> - [ ] Extender la detección a todos los sistemas con actualizaciones o DLC (Tandas 1–3 y PSP, Wii/WiiWare…).
> - [ ] Revisiones retro agrupadas como versiones de un juego en la biblioteca y en duplicados.
> - [ ] Interfaz: el juego con sus actualizaciones y DLC plegados debajo, versión instalada y la más nueva disponible.

Workflow brief for this run (part A): the option is **on for the library (Coverly) profile and off by default for handheld profiles** whose frontend does not take folders as games; folder names fixed in English on disk and translated in the UI; **create only folders that get content**; scanning, Organizar and Duplicados never treat files inside the fixed folders as loose games nor as duplicates of the base; paths `<System>/<Game>/<Game>.<ext>`, `Updates/<Game> (vX.YY).<ext>`, `DLC/<Game> - <DLC name>.<ext>`; demos as `<Game> (Demo)` linked by name only; versions from headers (3DS/Wii U title version, Switch cnmt version when in clear, PS3/PS4/Vita/PSP `APP_VER`/`VERSION`, Xbox 360 STFS); region from the title id or product code; licences recognised **only by file name or extension, never opened**, moved with their file when unambiguous, else left in place; system updates, firmware, shader caches and per-game emulator configs recognised as «not a game» and left; retro revisions grouped in the library model and Duplicados (newest suggested, never both deleted), loose on disk unless the profile asks for folders; minimal UI in Organizar and Ajustes → Biblioteca. Not here: the six-views redesign, Mods/Cheats association, emulator translators, Extras CRUD.

## 3. What exists today (read before changing)

| Layer | Where | What matters here |
|---|---|---|
| Title kind | `consoles/mod.rs` `TitleKind { Base, Update, Dlc, Demo, Homebrew, Unknown }`, `kind_3ds`, `kind_wiiu`, `kind_wii`, `kind_switch`, `switch_base`, `base_title_id(&ConsoleId)` | `base_title_id` covers switch, 3ds, wiiu, ps3, psvita, xbox360, ps4, ps5 and GDK Xbox; **not** Wii WADs or PSP. `kind_switch` takes `0x1801..` (DataPatch) as DLC and `0xC00` (Delta) as Unknown. `kind_3ds` leaves system titles (category bit `0x10`) Unknown. |
| Console header | `ConsoleId { system, serial, title_id, title, version, kind, region, content_id, product_id, parent_id, part, … }` | `version` is a raw string per reader: 3DS TMD u16 (`"1040"`), Switch cnmt (`"393216"`), Wii U `title_version` (`"32"`), Sony `APP_VER` (`"01.09"`), PS5 `contentVersion` (`"01.000.000"`), GameCube/Wii disc byte 0x007 (`"2"`), XBE (`le32 0xAC`). `region` only from a PS4/PS5 content id. |
| Readers | `ctr.rs` (3DS), `nx.rs` (Switch; `cnmt.xml` and `[vN]` names in clear), `wiiu.rs`, `wad.rs` (Wii WAD → `kind_wii`), `nintendo_disc.rs`, `sony.rs` (PS3/Vita/PS4 PSF; Vita `ac` not mapped to DLC), `orbis.rs` (PS4/PS5 PKG, `param.json`), `xbox.rs` (XBE, XEX, STFS `CT_GOD`/`CT_XBLA`/`CT_UPDATE`/`CT_DLC`; demo `0x00080000` not mapped; XVD content types `0x01`/`0x18..0x1A`) | PSP has no update reader (`PBOOT.PBP` / `CATEGORY PG` not read). |
| Keys | `consoles::is_key_material(name)`: `KEY_NAMES` (`prod.keys title.keys dev.keys aes_keys.txt seeddb.bin act.dat work.bin`), `KEY_EXTS` (`rap rif dkey zrif tik cetk cik`); `open_rom` refuses them; walk leaves them `LeftReason::KeyMaterial` («Coverly no lo lee ni lo mueve») | `keys.txt` deliberately absent; `otp.bin`, `ticket.db`, `eeprom.bin`, `.edat` absent. Batch 2 D4 says key material is **never moved**. |
| Linking | `organize/plan.rs` `title_group`, `updates_after_bases` (children right after their base, updates then DLC), `ItemNote::Update { of }`, `ItemNote::Dlc { of }` | Children are linked by title id only; the plan item carries no pointer to its base item and no version for the UI. |
| Targets | `organize/target.rs` `TargetLayout { target_root, roms_dir, profile, overrides, fat32, windows_paths, names, companion_exts }`, `assign_targets_in`, `plan_single`, `game_folder(dir, stem, index)` (disc layout), `DirIndex` (claimed/existing names), `dlc_target` (Batocera `DLC/{title_id}`), `overlay_pairs`/`sibling_targets` (shadPS4 `-UPDATE`) | The disc `GameFolder` layout already creates `<System>/<stem>/` and moves set members into it; `RenameReason::{Collision, SafeName, PathLength, Catalogue}`. |
| Walk | `organize/walk.rs` `filter_entry`: a folder resolving to a system is `sorted` (left `AlreadySorted`, never descended); emulator data roots; shadPS4 overlays; console folder probes; `is_key_material` before any byte | A pile folder holding `Updates/` or `DLC/` is walked like any folder: its files become loose items. A `Mods/` folder holding `.ips` gets `NotARom::Patch` rows. |
| Apply / undo | `organize/apply.rs`, `write/mod.rs` `Op::{Created, Moved, Copied, DirCreated, …}`, `write/noclobber.rs`, `write/undo_organize.rs` | Directory creation is journaled (`DirCreated`) and undone when empty; moves are exclusive renames; across volumes a move is copy + verify + remove. |
| Profiles | `profiles/mod.rs` `Profile { …, disc_layout, folders, folder_game_suffix, folder_stem_keeps_suffix, dlc_folder }`, `profiles/builtin.rs` (`anbernic-stock`, `garlicos`, `onion`, `muos`, `minui`, `esde`, `batocera`, `retroarch`), `profiles/custom.rs` TOML | **No library (Coverly) profile exists.** `TODO.md` §3 plans «perfil de organización interno (el formato maestro de la biblioteca)». |
| Settings | `settings/model.rs` `OrganizeSettings { default_layout, names, folder_names: profile → system → folder, … }`, `SETTINGS_VERSION = 3`; UI `src/components/settings/LibrarySection.tsx` (scan options; Organizar and Duplicados have their own tabs since 1.6) | Per-profile overrides already live in settings (`folder_names`, `arcade_order`). |
| Scan | `scan/mod.rs` `scan_systems`, `RomEntry { path, rel_dir, stem, ext, size, modified, companions, is_dir }` | Walks each system folder with no depth limit: a file in `Game/Updates/` becomes its own `RomEntry`, so the six views, fill, gamelists and audit would count an update as a game. |
| Dedupe | `dedupe/keys.rs` `ExactKey::Title { title_id, kind, version, … }` (an update never shares its base's key), `VersionKey` level 2 (catalogue `title_key`, or `by_title_id`), `dedupe/keeper.rs` `KeepRule::Revision`, `version_order(system, v)` | Revisions of one catalogue game already meet at level 2 and the keeper prefers the higher revision; the level only **acts** with «una ROM por juego» and is otherwise reported. Units come from the scan, so `Mods/`/`Extras/` files would be units today. |
| Names | `identify/names.rs` `parse_tags` → `NameTags { title_key, revision, revision_text, class, regions, disc, … }`, `Region` (19 values), `Region::from_no_intro` | Enough to strip tags and to tell a demo (`ReleaseClass`) and a revision apart. |
| Organizar UI | `src/lib/organize.ts` `updatesUnderBases` (children folded by matching `note.of` against base names), `noteText`, `ownerFirst`; `src/components/organize/SystemGroups.tsx`, `ItemName.tsx` (`→ target` in mono) | Folding by name is fragile (two bases with one title). No destination preview beyond each row's `→ path`. |
| CLI | `crates/coverly-cli/src/organize.rs` exhaustive `left_text`/note text | New reasons and notes need CLI text. |
| Facts | `organize/plan.rs` `FACTS_VERSION = 9`; `dedupe/keys.rs` `FACTS_VERSION = 3` | Memoised verdicts of earlier versions hold no regions, versions or new kinds. |

## 4. Decisions

| # | Topic | Decision | Why |
|---|---|---|---|
| D1 | Where the option lives | New field `Profile.game_folders: bool` (serde default `false`; custom TOML key `game_folders = true`, copied by «duplicar perfil»). User override per profile in `OrganizeSettings.game_folders: BTreeMap<profile_id, bool>` (serde default empty, like `folder_names`); `Profile::uses_game_folders(&OrganizeSettings) -> bool` = override, else the profile's own value. `TargetLayout.game_folders` carries the resolved value into planning; the scan reads the same resolved value. No `SETTINGS_VERSION` bump (additive, defaulted field). | The brief calls it a profile option; built-in profiles are code, so a user choice on them needs a settings override, the pattern `folder_names` already uses. |
| D2 | The library (Coverly) profile | New built-in profile `coverly`, display name «Biblioteca Coverly» / «Coverly library»: `roms_root = "ROMS"`, folder naming and host folders like `anbernic-stock` (`PS4`, `3DS`, `SFC`…, matching Miguel's `~/Documents/ROMS`), `disc_layout = GameFolder { m3u_for_multi: true }`, `accepts_containers` zip + 7z, `game_folders = true`, art slots inside the game's own folder under a hidden `.media/` (`{system_dir}/{rel_dir}/.media/{stem}.png`, `-screenshot`, `-title`; `art_dir_names = [".media"]`). It is **never auto-detected** in part A: `detect()` keeps its current scores; the user picks it as the Organizar layout or the library's profile. All other built-ins: `game_folders = false`. | Brief: on for the library profile, off for handheld profiles. ES-DE and Batocera show a folder with sub-folders as a folder, not a game (they need «directory as game» or a suffix, a part B translator); Onion, MinUI, muOS and the Anbernic stock launcher list files. Art in `.media/` is provisional until §3/§3.1 and part C decide the library's art store; it keeps images out of the scan and out of `Extras/` (which belongs to the user). |
| D3 | Fixed folder names | `consoles::FixedFolder { Updates, Dlc, Mods, Cheats, Extras, Saves }` with `dir_name()` = `Updates`, `DLC`, `Mods`, `Cheats`, `Extras`, `Saves` (exact English, this casing) and `FixedFolder::from_dir_name(&str)` matching **case-insensitively and exactly** (`updates`, `dlc` match; `Update`, `DLCs`, `Updates (old)` do not). A *game folder* is a directory directly under a system folder (or under a pile folder, D7) that holds at least one fixed folder **or** one base game file at its root (§8.1). | Fixed names are how part B translators and block 5 find content; case-insensitive matching because users and other tools write `dlc/`, and APFS/NTFS are case-insensitive anyway. |
| D4 | Create only what gets content | Organizar creates `<Game>/` only when the item needs it, `Updates/` only for an update, `DLC/` only for a DLC. Never creates `Mods/`, `Cheats/`, `Extras/` or `Saves/` in part A (nothing goes into them yet). Each created directory is a journaled `Op::DirCreated`; undo removes it only when empty (existing rule). | Brief: no empty folders. |
| D5 | Target paths | With `game_folders` on, for a system whose items have `TitleKind` (D14) or are file/set/archive games: base → `<System>/<Game>/<Game>.<ext>` (a disc set: its members inside `<Game>/`, as the disc `GameFolder` layout does now, with its `.m3u`); update → `<System>/<Game>/Updates/<Game> (v<version>).<ext>`; DLC → `<System>/<Game>/DLC/<Game> - <DLC name>.<ext>`; a **folder game** (PS3 JB, Vita app, Wii U loadiine, PS4 `CUSA…`) keeps its own folder as the game folder: base → `<System>/<Game>/` (the folder itself, renamed to `<Game>` only when names are «catálogo»), its update folder → `<System>/<Game>/Updates/<folder name>/`, its DLC folder → `<System>/<Game>/DLC/<folder name>/`. Profile rules that already place DLC elsewhere win (`dlc_folder` on Batocera, shadPS4 `-UPDATE` siblings: those profiles have `game_folders` off). With the option off, nothing changes. | Brief §2 paths. A folder game is already a folder: nesting `Game/Game/` would break PS3/Vita/PS4 loaders that look for `PS3_GAME/`, `sce_sys/` at the folder root. |
| D6 | `<Game>` and the names | `<Game>` = the base's final stem (after `NameMode`: kept name, or catalogue name, then the safe-name and path-length rules), with the region added by D10 when the stem has none. Children **always** get the canonical names of D5 (whatever `NameMode` says), recorded as a new `RenameReason::GameFolder { from }` so the row says «Se renombra desde …» and undo restores the old name. `<DLC name>`: the DLC's own plaintext title when it differs from the base title (PS3/PS4/Vita `TITLE`, Xbox 360 display name, `cnmt.xml`/`nacp.xml` in clear, GDK `DefaultDisplayName`), else the file stem with the base title and every `(…)`/`[…]` group stripped when anything is left, else `DLC <title id>` (`DLC 0100ABCD12341001`). Two DLC that end on one name get the existing collision suffix ` (2)`. | Dumpers name children with ids (`0004000E00055D00.cia`, `TU_10DABC.zip`, `[0100…800][v131072]`); the folder layout is only useful if they read as the game. The base keeps the user's naming choice. |
| D7 | Respect the fixed folders: walk | In `organize/walk.rs`, before the console probes: a pile directory whose name is a fixed folder and whose parent holds a game (D3) is handled by role: `Updates/` and `DLC/` are walked as usual, and each item found below gets `role_hint = Update | Dlc` (used when the header says nothing, D14) and its parent's base as the link candidate; `Mods/`, `Cheats/`, `Extras/`, `Saves/` are **not descended**: one left row each, `LeftReason::GameFolderContent { folder: FixedFolder, of: String }`. In the target library, system folders are already never descended (`sorted`), so nothing inside a game folder is moved out. | Brief: never treat files inside the fixed folders as loose games to move out. Mods/Cheats/Extras/Saves move with their game in parts B/D and block 5; leaving them in place is the safe default, and leaving `Saves/` alone can never lose a save. |
| D8 | Respect the fixed folders: scan | `scan/mod.rs`: inside a system folder, a game folder's `Mods/`, `Cheats/`, `Extras/`, `Saves/` are skipped like `art_dir_names`; files under `Updates/` and `DLC/` are **not** top-level entries: they become `RomEntry.children: Vec<ChildEntry { path, role: ChildRole { Update, Dlc }, size, modified }>` of the base entry found at the game folder's root (one base file, one base set, or the folder game itself). When the game folder has no base at its root (orphans), each child stays a top-level entry with `RomEntry.role = Some(ChildRole)` so it is still visible and counted apart. Fill, gamelists, audit and the six views keep using top-level entries only, so an update is never a game in the totals. | Brief: scanning must respect the folders. Children as data on the base is the minimal model part C will render; nothing in part A draws them in the library. |
| D9 | Respect the fixed folders: Duplicados | `dedupe/units.rs` builds units from top-level entries **and** their children, with a new `Unit.role: Option<ChildRole>`. A child never pairs with a unit of another role: exact and format links require equal roles, and a child whose header gives no kind (D14 fallback to the folder) gets `ExactKey::Title { kind }` from its role. Two updates of one base are versions (level 2, `by_title_id`), the higher version is the suggested keeper (`version_order`), and two byte-identical update copies are exact duplicates as today. Files under `Mods/`, `Cheats/`, `Extras/`, `Saves/` are never units. | Brief: never flag them as duplicates of the base. |
| D10 | Region | `ConsoleId.region` is filled by every reader that can: 3DS and Wii U product code last letter (`CTR-P-AXXE`, `WUP-P-AXXP`: `E` USA, `P` Europe, `J` Japan, `K` Korea, `W` Taiwan, `C` China, `A` World, `D` Germany, `F` France, `S` Spain, `I` Italy, `U` Australia, `H` Netherlands); GameCube/Wii disc and WAD game id 4th character (same letters); Sony serial prefix (`BLUS BCUS NPUB NPUA ULUS UCUS PCSA PCSE` → USA; `BLES BCES NPEB NPEA ULES UCES PCSB PCSF` → Europe; `BLJM BLJS BCJS NPJB NPJA ULJM ULJS UCJS PCSG PCSC` → Japan; `BLAS BCAS NPHB NPHA PCSH` → Asia; `BLKS NPKB PCSD` → Korea); PS4/PS5 content id (exists). Switch, Xbox 360 and Xbox One/Series have no region in their ids: none. The region joins `<Game>` only when the base stem has no region tag (`parse_tags(..).regions` empty), as `(USA)`, `(Europe)`, `(Japan)` in No-Intro words (`Region::no_intro_word`), and it is added **always when known**, not only when two regions meet. | Brief: one folder per region with the region in the name. Adding it whenever known keeps names stable: a second region imported later never forces renaming the first folder. Children inherit the folder's region because their title ids are region-specific (3DS, Wii U, Sony) or region-free (Switch). |
| D11 | Version display | `consoles::display_version(system, raw) -> Option<String>`: 3DS TMD u16 → `major.minor.micro` from 6/6/4 bits (`1040` → `1.1.0`, 3dbrew «Title version»); Wii U → `v` + raw decimal (`32`; no documented split); Switch → raw decimal (`131072`, the scene `[v131072]` form; `>>16` is medium confidence in the research, so not used); Sony `APP_VER`/`VERSION` as written (`01.05`); PSP `DISC_VERSION`/`APP_VER` as written; PS5 `contentVersion` as written; Xbox 360 STFS version u32 at 0x358 (new read, D14) → `major.minor.build.qfe` from 4/4/16/8 bits; Xbox One/Series `PackageVersion` 4 × u16 at 0x3BC printed high to low; GameCube/Wii disc byte 0x007 → `Rev N` when N > 0, none when 0. The name gets ` (v<display>)` for updates (` (Rev N)` stays the Redump form for discs). Comparison stays `dedupe::keeper::version_order`, extended to these forms. | Brief: version in the name from the header. Each form is the one its community and DATs already read. |
| D12 | Demos | A demo (`TitleKind::Demo` from 3DS `00040002`, Wii U `00050002`, PS3/Vita `APP_TYPE`, Xbox 360 content type `0x00080000` (new), or a `(Demo)`/`(Trial)` tag when the header says nothing) is its own game: with `game_folders` on → `<System>/<Name> (Demo)/<Name> (Demo).<ext>`; ` (Demo)` is appended only when the name has no `Demo`/`Trial` tag already. It is linked to a base **by name only**: same system, `parse_tags(stem).title_key` equal to a base's (in the plan, or a top-level entry in the target system folder), → `ItemNote::DemoOf { of }` and `GameLink.demo_of` (D16). Never moved into the base's folder. Off: unchanged flat placement, the note still shows. | Brief: demos apart, No-Intro `(Demo)` suffix, linked by name only (3DS and Wii U demos have their own ids; the research says link by name). |
| D13 | Orphan children | An update or DLC whose base is neither in the plan nor found in the target (a game folder named `<Game>` in the target system folder, or a top-level entry with the same `base_title_id` in the facts cache) goes to `<System>/<Game>/Updates|DLC/…` when a trusted game title is known (the update's own header title, the catalogue name for its base title id), as **Review** (never Auto) with `ItemNote::BaseMissing`. With no title, it stays flat in the system folder as today, Review, same note. | Creating `0004000000055D00/` folders would be worse than flat; Review makes the user see a folder the base will join later. |
| D14 | Detection extended (Tandas 1–3) | New or fixed in part A: (a) **3DS**: `kind_3ds` → `ConsoleSystem` for the System category bit (`high & 0x10`; `00040010`, `0004001B`, `00040030`, `0004009B`, `000400DB`, `00040130`, `00040138`), region from the product code. (b) **Switch**: low bits `0x1801..=0x1FD0` (DataPatch) → `Dlc` linked to `((id − 0x800) − 0x1000) & !0xFFF` (`switch_base` gives the same), `0xC00` (Delta) → `Update`; system titles (`id < 0x0100000000010000`) → `ConsoleSystem`. (c) **Wii U**: unchanged kinds, region from product code. (d) **Wii / WiiWare WAD**: `base_title_id` for `wii` = `00010001` + low 32 bits (DLC `00010005` meets its channel); `0001000[02]` system titles → `ConsoleSystem`. (e) **PS3**: unchanged; PKG `PATCH` flag already decides update vs DLC. (f) **Vita**: `CATEGORY ac` → `Dlc` (fix). (g) **PSP**: `PBOOT.PBP` or a PBP / folder with `CATEGORY PG` → `Update` with `base_title_id` = `DISC_ID` (new arm `psp`); `UG`/`EG` base. (h) **PS4/PS5**: unchanged. (i) **Xbox 360**: STFS content type `0x00080000` → `Demo`; STFS version at 0x358 into `ConsoleId.version`; base by title id (exists). Media id is not used for grouping in part A. (j) **Xbox One/Series**: XVD content types `0x0B`/`0x0C` (Updater) → `ConsoleSystem`; an update has no id of its own (same ProductId, higher PackageVersion): two XVDs with one ProductId are versions (D9), not base and update. (k) **Retro** (Tanda 1 and the retro systems): no updates or DLC; revisions only (D15). Unknown stays Unknown and the item is placed as today. A file inside `Updates/` or `DLC/` of a pile game folder whose header gives no kind takes the folder's role (D7), never the reverse: a header kind always wins over the folder. | Brief item 4 and the research table. Every rule is a plaintext id, category or content type; no key is needed. |
| D15 | Retro revisions | Grouping key for revisions: same system + `parse_tags(name).title_key` (catalogue name when hashed, else the file stem) with revision tags `(Rev 1)`, `(Rev A)`, `(v1.1)` stripped; region stays part of the key (another region is another folder/game). Library model: `scan` adds `RomEntry.version_group: Option<String>` (the key) and `RomEntry.revision: Option<String>` (as written), so part C can fold them; nothing is drawn yet. Duplicados: the existing level 2 already groups them; part A makes the revision link **report-only by default and never actionable for both**: the keeper is the newest revision (`KeepRule::Revision` moved first among keeper rules for `Relation::Version` groups when the user has not reordered it), and a group's apply refuses any action that would remove every member (new guard in `dedupe/apply.rs`, belt and braces over the planner's one-keeper invariant). On disk, revisions stay loose files unless the profile has `game_folders` on, in which case each revision is its own base in its own folder (revisions are not updates). | Brief item 6. An untagged name is the first release (research). Keeping revisions as separate games on disk is what every retro frontend expects. |
| D16 | Plan data for the UI | `OrganizeItem.game: Option<GameLink>` (serde default, skipped when none): `GameLink { role: GameRole { Base, Update, Dlc, Demo }, version: Option<String> /* D11 display */, region: Option<Region>, of: Option<usize> /* the base item's id in this plan */, folder: Option<PathBuf> /* the game folder, relative to the target root */, demo_of: Option<String> }`. `ItemNote::{Update, Dlc}` stay (text and CLI), and `of` is what the UI folds by. | Folding by item id instead of by name (two bases titled «Tetris» no longer swallow each other's children). |
| D17 | Licences | `is_key_material` gains `keys.txt`, `otp.bin`, `ticket.db`, `eeprom.bin` (exact names) and the `edat` extension. Recognition is **by name only**; nothing in the code path opens, hashes, sniffs or stats beyond `symlink_metadata` a file it names. A licence **moves with its file** only when all hold: (1) **move** mode and the target is on the **same volume** (a rename; a copy would read its bytes, so in copy mode or across volumes it stays); (2) it sits in the same directory as exactly one planned item it matches: its stem equals, case-insensitively, that item's content id (`.rap`/`.rif`/`.edat` named by PS3/PSP content id), its title id or the first 16 hex characters of a Switch rights id (`<rightsId>.tik` beside an NSP whose title id is the same), or the item's own stem (`Game.rap` beside `Game.pkg`); (3) no other item matches it. It then travels as a `Companion` with `licence: true` into the item's folder (beside the file, never into `Updates/` alone when its owner is the base), journaled and undone like any companion. Global key files (`prod.keys`, `title.keys`, `dev.keys`, `aes_keys.txt`, `seeddb.bin`, `ticket.db`, `keys.txt`, `otp.bin`, `act.dat`, `eeprom.bin`) never match an item and always stay. `work.bin` inside a Vita app folder moves with the folder (it is inside the moved tree, never opened). Everything else keeps `LeftReason::KeyMaterial`. | Brief item 5, which supersedes batch 2 D4's «never moved» for the unambiguous case: a rename does not read the file, so the keys rule still holds. `keys.txt` was left out in batch 2 for being a common name; the brief lists it, and treating it as key material only makes Coverly stricter (never opened); the cost is one extra left row when a readme has that name. |
| D18 | Not a game | Recognised by name or plaintext header and left where they are: firmware packages by exact name (`PS3UPDAT.PUP`, `PS4UPDATE.PUP`, `PS5UPDATE.PUP`, `PSP2UPDAT.PUP`; a PSP `EBOOT.PBP` is never classed by name, since every PSP game has one) → `NotARom::ConsoleSystem`; system titles by id (D14 a, b, d, j) → `NotARom::ConsoleSystem`; shader caches (any file under a folder named `shadercache`, `shader_cache`, `ShaderCache`, `GLCache`, `vulkan_pipeline_cache` case-insensitively, and the extensions `shadercache`, `pipelinecache`) → new `NotARom::EmulatorCache`; per-game emulator configs (`.opt`, `.rmp`, `.slangp`, `.glslp`, `.cgp`; Dolphin `GameSettings/*.ini`; Cemu `gameProfiles/*.ini`; RPCS3 `config/custom_configs/*.yml`; PPSSPP `*_ppsspp.ini`; yuzu-family `custom/*.ini`) → new `NotARom::EmulatorConfig`. Cartridge/disc update partitions (3DS NCSD partition 7, Wii update partition) live inside the image and move with it: nothing to do. | Brief item 5. Only the files that would otherwise become rows are affected: these extensions are not ROM-like, so the new reasons only replace a misleading row or appear for a file inside a moved tree's listing. |
| D19 | Restructuring an already sorted library | **Not in part A.** A game already in a system folder of the target stays where it is (`sorted`, as today), even with the option on; only items organised from a pile get the folder layout, plus children that join an existing game folder. A «reagrupar biblioteca» action is listed for the owner (§13 Q1). | Safest default: switching an option never moves thousands of files the user did not pick. |
| D20 | Existing target names | If `<System>/<Game>/` exists as a directory, it is reused (`DirIndex::existing`, casing kept); if a **file** named `<Game>` exists, or the directory is a symlink/junction (`UnsafeTarget`), the item gets the collision rule: `<Game> (2)/`. A child whose target name exists inside `Updates/` with the same size and fingerprint → `AlreadyPresent`; different content → ` (2)` and Review. | Existing no-overwrite rules, applied one level deeper. |
| D21 | Safety rules unchanged | Dry-run plan first; apply re-validates every source fingerprint; exclusive renames; moves only within a volume, otherwise copy + verify + remove (or copy mode); `LeftReason::TooBigForFat`, `PathTooLong` (the extra two levels count: `Updates/` and the game folder), volume-gone handling and resume as today. Undo restores children before removing folders. | Brief. |
| D22 | Memoised facts | `organize/plan.rs` `FACTS_VERSION` 9 → 10 (`ConsoleId.region` for more systems, Xbox 360 version, new kinds, `ConsoleSystem` verdicts); `dedupe/keys.rs` `FACTS_VERSION` 3 → 4 (roles in units, Switch DataPatch). | Old facts would keep wrong kinds. |
| D23 | CLI | `coverly organize … --game-folders` / `--no-game-folders` (override the resolved profile value for one run); English text for the new reason, notes and rename reason; the plan table prints the version column for children. | The CLI must compile (exhaustive matches) and the option must be scriptable. |

## 5. Architecture and files touched

### 5.1 Rust

| File | Change |
|---|---|
| `consoles/mod.rs` | `FixedFolder` (D3); `kind_3ds`/`kind_switch`/`kind_wii` changes and `is_console_system_title(system, id)` (D14); `base_title_id` arms `wii`, `psp` (D14); `display_version` (D11); `region_from_product_code`, `region_from_game_id`, `region_from_sony_serial` (D10); `is_key_material` list (D17); `licence_matches(name, item_keys) -> bool` (names only). |
| `consoles/ctr.rs`, `wiiu.rs`, `nintendo_disc.rs`, `wad.rs`, `sony.rs`, `orbis.rs`, `nx.rs`, `xbox.rs` | Fill `region` (D10); Vita `ac`; PSP `PG` (`sony.rs`, PBP `PARAM.SFO` read through the existing PSF parser, budgeted); Xbox 360 demo and version at 0x358; XVD updater types. |
| `identify/sniff.rs` | `NotARom::{EmulatorCache, EmulatorConfig}` (D18). |
| `scan/classify.rs` | `not_a_game(path) -> Option<NotARom>` by name and parent folder name (D18). |
| `organize/walk.rs` | Fixed-folder handling (D7), `.PUP` names and `not_a_game` before the size check. |
| `organize/plan.rs` | `GameLink`, `GameRole` (D16); `LeftReason::GameFolderContent { folder, of }`; `ItemNote::{DemoOf { of }, BaseMissing}`; `RenameReason::GameFolder { from }`; `Companion.licence: bool` (serde default false); linking writes `game.of`; `FACTS_VERSION` 10. |
| `organize/target.rs` | `TargetLayout.game_folders`; `plan_game_folder(...)` placing base, children and demos (D5, D6, D12, D13, D20); licence companions (D17); path-length check over the deeper paths. |
| `organize/apply.rs`, `write/undo_organize.rs` | No new op: `DirCreated` for `<Game>/`, `Updates/`, `DLC/`; ordering so that the game folder is created before its children and removed after them on undo; a licence companion never takes the copy path (asserted). |
| `profiles/mod.rs`, `builtin.rs`, `custom.rs` | `Profile.game_folders`, `uses_game_folders`; built-in `coverly` (D2); TOML key. |
| `settings/model.rs`, `patch.rs`, `validate.rs` | `OrganizeSettings.game_folders` map; validation: keys must be known profile ids (unknown ids dropped on normalise, like `folder_names`). |
| `scan/mod.rs` | `RomEntry.children`, `RomEntry.role`, `RomEntry.version_group`, `RomEntry.revision` (D8, D15); fixed folders skipped. |
| `dedupe/units.rs`, `keys.rs`, `keeper.rs`, `apply.rs` | Units from children with `role` (D9); role-aware keys; keeper order for version groups and the never-all guard (D15); `FACTS_VERSION` 4. |
| `crates/coverly-cli/src/organize.rs`, `main.rs` | D23. |
| `src-tauri/src/ipc.rs`, `tests/contract.rs`, `contract.sample.json` | New fields serialised; the plan IPC passes the resolved `game_folders`; settings patch accepts `organize.game_folders`. |

### 5.2 Front end

| File | Change |
|---|---|
| `src/ipc/types.ts`, `contract.test.ts` | `GameLink`, `GameRole`, `OrganizeItem.game?`, `LeftReason` `game-folder-content`, `NotARom` `emulator-cache`/`emulator-config`, `ItemNote` `demo-of`/`base-missing`, `RenameReason` `game-folder`, `Companion.licence?`, `Profile.game_folders`, `OrganizeSettings.game_folders`. |
| `src/lib/organize.ts` | `updatesUnderBases` folds by `game.of` (falls back to today's name match for plans without `game`); `noteText` for new notes and reason; `gameFolderPreview(items)` builds the small destination tree (§9.1); `fixedFolderLabel(folder)`. |
| `src/components/organize/SystemGroups.tsx`, `ItemName.tsx` | Version in the child row (mono), the folded destination preview under a base with children. |
| `src/components/organize/GameFolderPreview.tsx` (new) | The preview (§9.1). |
| `src/components/settings/LibrarySection.tsx` | The switch (§9.1), bound to `organize.game_folders[<profile>]`. |
| `locales/{es,en}/ui.json` | §10 keys, sorted in place (parallel workflows edit these files: resolve conflicts key by key). |
| `src/ipc/mock/*` | `?mockPick=gamefolders` (§9.3); built-in `coverly` profile in the mock; settings round-trip. |

## 6. Profiles

| Profile | `game_folders` default | Why |
|---|---|---|
| `coverly` (new) | **on** | The library's master format (D2). |
| `anbernic-stock`, `garlicos`, `onion`, `muos`, `minui` | off | Their launchers list files; a folder with sub-folders shows as a folder or not at all. |
| `esde`, `batocera` | off | Folders need «directory as game» or a suffix; their translation is part B. Batocera keeps its `DLC/{title_id}` (batch 3 D16). |
| `retroarch` | off | Playlists are built by scanning files; folders add nothing and break thumbnails by name. |
| Custom profiles | off unless `game_folders = true` in their TOML | User's choice. |

The user can turn it on for any profile in Ajustes; the switch explains what it does, not whether the frontend copes (that is part B's translators).

## 7. Naming examples

| Input (pile) | Header | Target (`coverly`, names «conservar») |
|---|---|---|
| `Pokemon Y.cia` | 3DS `0004000000055D00`, `CTR-P-EK2A` | `3DS/Pokemon Y (World)/Pokemon Y (World).cia` |
| `0004000E00055D00.cia` | update, TMD version 1040 | `3DS/Pokemon Y (World)/Updates/Pokemon Y (World) (v1.1.0).cia` |
| `Miku DLC1.pkg` | PS4 `ac`, `TITLE` «Song Pack 1», `UP0177-CUSA…` | `PS4/Hatsune Miku Project DIVA Future Tone (USA)/DLC/Hatsune Miku Project DIVA Future Tone (USA) - Song Pack 1.pkg` |
| `Zelda [0100…800][v131072].nsp` | Switch update | `Switch/Zelda/Updates/Zelda (v131072).nsp` (no region: Switch ids have none) |
| `TU_10DABC` | 360 STFS `0x000B0000`, version 2.0.12.0 | `XBOX360/Halo 3/Updates/Halo 3 (v2.0.12.0)` |
| `BLUS30001 DEMO.pkg` | PS3 `APP_TYPE` demo | `PS3/Resistance (USA) (Demo)/Resistance (USA) (Demo).pkg`, note «Demo de Resistance (USA)» |
| `Sonic the Hedgehog (USA, Europe) (Rev 1).md` | — | `MD/Sonic the Hedgehog (USA, Europe) (Rev 1)/Sonic the Hedgehog (USA, Europe) (Rev 1).md` with `coverly`; flat `MD/…md` with the option off |
| `BLUS30001.rap` beside `Resistance.pkg` (content id `UP9000-BLUS30001_00-…`) | never opened | moves with the PKG (move mode, same volume); otherwise stays, «Archivo de claves o licencia» |

## 8. Behaviour in detail

### 8.1 What is a game folder (scan and walk)

Directly under a system folder (scan) or anywhere in a pile (walk), a directory `G` is a game folder when either: it holds a fixed folder (D3) as a direct child; or `G` is a folder game a probe already recognises (PS3 JB, Vita, Wii U, PS4/PS5, Xbox). Its **base** is: the folder game itself; else the one ROM-like file or one sheet set at `G`'s root; with several root games (a user folder of loose games), `G` is not a game folder and its fixed folders are treated as orphan children (D8) and left rows (D7). One `read_dir` of `G` decides it (existing walker listing, no new pass).

### 8.2 Planning order

1. Survey and identify as today; children get `role_hint` from D7.
2. `updates_after_bases` links children to bases by `base_title_id`, now also writing `game.of`; demos link by name (D12); revisions get their group (D15).
3. Targets: bases first, so `<Game>` is known; then children into `folder/Updates|DLC`; then demos; orphans by D13.
4. Licence companions are matched after targets (they need the owner's final folder and the transfer kind).
5. Duplicates step as today, with role-aware keys.

### 8.3 Undo

The journal already records `DirCreated` and each `Moved`. Undo walks the journal backwards: children go back first, then the base, then empty `DLC/`, `Updates/`, `<Game>/` are removed (never a folder that holds anything else, for example a `Saves/` a user added after the run). A licence companion goes back like any companion.

## 9. UX and UI design

Visible change is small by design: child rows show their version and folder, a base with children shows how its folder will look, and Ajustes has one switch. The six views are untouched (part C).

### 9.1 Design plan (frontend-design pass)

- **Colour** (existing tokens only): `--cr-surface` for the preview panel's ground, `--cr-border` for its single left spine (`border-width.spine`), `--cr-text` for names, `--cr-text-secondary` for fixed folder labels and counts, `--cr-primary` only for the switch's on state (the existing `SwitchSetting`). No warning or Manila colour: a game folder is not an alert.
- **Type**: Atkinson Hyperlegible Next for names and labels; **Atkinson Hyperlegible Mono only for real data**: paths, versions (`v1.1.0`, `01.05`), title ids, sizes. Fixed folders show their translated name in the UI face (`Actualizaciones`) followed by the on-disk name in mono in the accessible description only once per preview («en disco: `Updates/`»), so the user learns the mapping without a wall of mono.
- **Layout** (Organizar, a base with children, folded under it as today):

  ```
  ▢ Pokemon Y (World)            1,72 GB  → 3DS/Pokemon Y (World)/Pokemon Y (World).cia
    ┃ Pokemon Y (World)/
    ┃   Pokemon Y (World).cia
    ┃   Actualizaciones (1)
    ┃     Pokemon Y (World) (v1.1.0).cia
    ┃   Contenido descargable (2)
    ┃     Pokemon Y (World) - Pack 1.cia
    ┃     Pokemon Y (World) - Pack 2.cia
    ▢ Actualización de Pokemon Y · v1.1.0    412,00 MB  → …/Updates/Pokemon Y (World) (v1.1.0).cia
    ▢ Contenido descargable de Pokemon Y      28,00 MB  → …/DLC/Pokemon Y (World) - Pack 1.cia
  ```

  The preview is a disclosure («Ver cómo queda la carpeta», collapsed by default, remembered per session) placed in the base row's detail area, indented under the name, with one 2 px spine on the left in `--cr-border` instead of tree glyphs or boxes. It lists only what this plan creates or fills; folders already on disk are shown in `--cr-text-secondary` with «ya existe». Child rows keep their checkbox and actions; their target is shortened to `…/Updates/<name>` with the full path in the title attribute and the accessible name.
- **Ajustes → Biblioteca**: one `SwitchSetting` after «Perfil de estructura por defecto»: label «Carpeta por juego con DLC y actualizaciones», hint one line «Cada juego en su carpeta, con sus actualizaciones y DLC dentro. Se aplica a {profile}.», `more` with the on-disk names. `{profile}` is the Organizar default layout's display name (the profile the switch edits); changing the default layout re-reads the switch.
- **Motion**: the disclosure uses the existing collapsible open/close (`--cr-duration-fast`, `--cr-ease-standard`); nothing else moves; `prefers-reduced-motion` removes it.
- **Principle**: show the folder as it will be on disk, in the user's language, before anything moves.

Review against generic defaults: the default would be a file-manager tree with folder and file icons on every line, chevrons per level, and a coloured «DLC» pill on each child. Rejected: icon-per-line trees read as a file browser and double the row height; pills turn a fact into a badge wall. The single spine and indentation say «inside» with one device, and the counts in parentheses carry the information a pill would. A second default, a full-width destination tree panel beside the list, was rejected for part A: it competes with the rows for width at 390 px and belongs to part C's Columns view.

Accessibility: the preview is a `ul` nested per level (real list semantics, `aria-label` «Carpeta de Pokemon Y (World)»), the disclosure is a button with `aria-expanded`; the version is text («versión 1.1.0»), never colour; the switch is the existing labelled switch with its hint as description; keyboard order: row checkbox, name, disclosure, child rows.

### 9.2 Ajustes wiring

The switch reads `settings.organize.game_folders[default_layout] ?? profile.game_folders` and writes the map entry; when the value equals the profile's own default, the entry is removed (no redundant overrides).

### 9.3 Mock

`?mockPick=gamefolders` (beside `downloads|loose|mixed|retro|modern|current`), synthetic names and sizes: a 3DS base with one update and two DLC (one named by id only), a PS4 base with an update and a DLC and a `.rif` named by its content id, a PS3 demo, a Switch DataPatch, an orphan Wii U update, `Sonic the Hedgehog (USA, Europe)` and `(Rev 1)`, a pile game folder holding `Updates/`, `DLC/`, `Mods/` and `Saves/`, `PS3UPDAT.PUP`, a `shadercache/` file, and `prod.keys`. The mock's layout list includes `coverly`.

## 10. Copy (es / en)

Spanish: sentence case, no exclamation marks, exact counts, numbers, versions and paths in mono by the renderer.

| Key | es | en |
|---|---|---|
| `settings.library.gameFolders` | Carpeta por juego con DLC y actualizaciones | One folder per game with DLC and updates |
| `settings.library.gameFoldersHint` | Cada juego en su carpeta, con sus actualizaciones y DLC dentro. Se aplica a {profile}. | Each game in its own folder, with its updates and DLC inside. Applies to {profile}. |
| `settings.library.gameFoldersDetails` | En disco las carpetas se llaman Updates y DLC, en inglés, para que emuladores y herramientas las reconozcan. Solo se crean cuando tienen algo dentro. | On disk the folders are named Updates and DLC so emulators and tools recognise them. They are only created when something goes in them. |
| `profiles.coverly` | Biblioteca Coverly | Coverly library |
| `gameFolder.updates` | Actualizaciones | Updates |
| `gameFolder.dlc` | Contenido descargable | Downloadable content |
| `gameFolder.mods` | Mods | Mods |
| `gameFolder.cheats` | Trucos | Cheats |
| `gameFolder.extras` | Extras | Extras |
| `gameFolder.saves` | Partidas | Saves |
| `organize.gameFolder.show` | Ver cómo queda la carpeta | Show the folder |
| `organize.gameFolder.hide` | Ocultar la carpeta | Hide the folder |
| `organize.gameFolder.label` | Carpeta de {title} | Folder of {title} |
| `organize.gameFolder.count` | {folder} ({count}) | {folder} ({count}) |
| `organize.gameFolder.exists` | ya existe | already there |
| `organize.gameFolder.onDisk` | en disco: {name} | on disk: {name} |
| `organize.version` | versión {version} | version {version} |
| `organize.note.demoOf` | Demo de {title} | Demo of {title} |
| `organize.note.baseMissing` | Su juego no está aquí ni en la biblioteca; revisa dónde va | Its game is not here or in the library; check where it goes |
| `organize.rename.gameFolder` | Se renombra desde {from} para su carpeta | Renamed from {from} for its folder |
| `organize.left.gameFolderContent` | Carpeta {folder} de {title}: se queda donde está por ahora | {folder} folder of {title}: stays where it is for now |
| `organize.left.emulatorCache` | caché de un emulador | emulator cache |
| `organize.left.emulatorConfig` | ajustes de un emulador | emulator settings |
| `organize.companion.licence` | Su licencia va con él; Coverly no la abre | Its licence goes with it; Coverly does not open it |

`organize.left.keyMaterial` changes to «Archivo de claves o licencia: Coverly no lo lee y lo deja aquí» / «Key or licence file: Coverly never reads it and leaves it here» (the old copy promised «ni lo mueve», no longer always true). `{folder}` in `gameFolderContent` is the translated fixed folder name. `emulatorCache`/`emulatorConfig` sit in `organize.left` beside `consoleSystem` and render through «No es una ROM ({what})». The i18n parity test covers every key.

## 11. Data flow

```
pile ─walk─▶ candidates (+ role_hint from Updates/ DLC/; Mods/Cheats/Extras/Saves → left)
     ─identify─▶ ConsoleId { kind, version, region } (plaintext only; licences never opened)
     ─plan─▶ link (base_title_id → game.of; demos by name) ─targets─▶ <System>/<Game>/[Updates|DLC]/…
     ─licences─▶ Companion { licence } (move + same volume + one owner) else left
     ─dedupe step (roles)─▶ OrganizePlan ─IPC─▶ Organizar (folded rows, preview)
apply ─journal─▶ DirCreated <Game>/, Updates/, DLC/; Moved base, children, licences ─undo─▶ reverse
library ─scan─▶ RomEntry { children, role, version_group, revision } ─▶ fill/gamelist/audit (top-level only), dedupe (units with roles)
```

## 12. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| Update and base in the pile, base left (unreadable, too big for FAT) | The update follows its base's action (existing sibling rule): left too, with the same reason, so a game folder never holds an update without its base by surprise. |
| Two bases with one title in one system (different title ids) | Two folders; the second gets the collision name `(2)`; region in the name usually separates them first (D10). Children fold by `game.of`, so each stays with its own base. |
| Base already in the library flat (`3DS/Pokemon Y.cia`), update in the pile | The facts cache finds the flat base by `base_title_id`: the update goes flat beside it, Auto as today, with no `BaseMissing` and no folder created around a file the user did not pick (D19). |
| `Updates/` in a pile folder holding several games | Not a game folder (§8.1): its files are planned on their own headers (orphans if the base is not found), and `Mods/`-like folders are still left. |
| A user `Extras/` holding a 3DS soundtrack `.cia` | Never descended in the pile, never scanned in the library, never a dedupe unit. |
| Case-different fixed folder on disk (`updates/`) | Matched (D3); Organizar reuses it with its casing instead of creating `Updates/` beside it (on case-sensitive volumes too). |
| `Updates` exists as a file inside `<Game>/` | Fixed names never get ` (2)`: the child stays in place with `LeftReason::UnsafeTarget`. |
| Path too long on Windows | `PathTooLong` per item as today, counted over the deeper path; the base may move while a long-named DLC stays: the DLC row says why. |
| FAT32 target and a 4 GiB+ update | `TooBigForFat` as today. |
| Copy mode | Children and bases copy into the game folder; licences stay (copying reads them, D17) with the key-material reason. |
| Volume unplugged mid-apply | Existing pause/resume: the journal knows which children moved; resume continues, undo reverses what moved. |
| Licence matches two items (`.rap` beside a base and its DLC with the same content id) | Ambiguous → left. |
| `keys.txt` that is a readme | Left with the key-material reason; never opened (cost accepted, D17). |
| Header says Base, file sits in a pile `Updates/` | Header wins (D14): planned as a base. |
| Unknown kind in `DLC/` with no base anywhere | Orphan DLC by role hint, Review, `BaseMissing`. |
| Revision `(Rev 1)` and untagged in Duplicados with «una ROM por juego» off | Reported as versions; no action. On: the keeper is `(Rev 1)`, the other goes to quarantine/Trash per settings; the guard refuses to act if the plan would remove both. |
| Switch DataPatch without its DLC | Linked to the base by `switch_base`, filed under `DLC/` with its own id as the name fallback. |
| Xbox One: two XVDs with one ProductId | Versions of one game (no update role); Duplicados suggests the higher `PackageVersion`. On disk each is a base: the second gets the collision folder `<Game> (2)/`, Review. |
| Undo after the user added a save to `<Game>/Saves/` | `<Game>/` is not empty: undo moves the files back and leaves the folder, reporting it as kept (existing «carpeta no vacía» outcome). |
| Option toggled between plan and apply | The plan stores its targets; apply follows the plan. |

## 13. Owner questions (implemented with the safe default, listed in the PR)

- **Q1** Restructure an already sorted library into game folders (D19)? Default: no; propose a separate «Reagrupar biblioteca» action with its own dry run.
- **Q2** Children always renamed to the canonical form even with «Conservar nombres» (D6)? Default: yes, recorded and undoable.
- **Q3** Region always added when known, or only when two regions meet (D10)? Default: always when known.
- **Q4** Switch version shown raw (`v131072`) or as patch number (`v2`)? Default: raw, until a verified source fixes the `>>16` convention.
- **Q5** Art of the `coverly` profile in `.media/` inside the game folder (D2)? Default: yes, provisional until §3/§3.1 and part C.
- **Q6** `keys.txt` as key material everywhere (D17)? Default: yes (stricter).
- **Q7** Should `coverly` be auto-detected (a library whose system folders hold game folders with `Updates/`/`DLC/`)? Default: no in part A.

Out of scope (later parts): Mods/Cheats detection and association, per-emulator translators (B); the six views, version/DLC indicators, art slots for children (C); Extras CRUD, `extras.json`, media viewer (D); saves (block 5).

## 14. Test plan

### 14.1 Rust (`cargo test --workspace --no-fail-fast`)

- `consoles`: `fixed_folder_names_match_exactly_ignoring_case`; `kind_3ds_system_titles_are_console_system`; `kind_switch_datapatch_and_delta`; `switch_system_titles`; `base_title_id_wii_wad_dlc_meets_channel`; `base_title_id_psp_update`; `display_version_per_system` (3DS 1040 → 1.1.0, 360 packed, XVD reversed, disc Rev); `region_from_product_code_and_serials` (table); `is_key_material_new_names`; `licence_matches_by_name_only`.
- Readers: Vita `ac` → Dlc; PSP `PBOOT.PBP` `PG` → Update with `DISC_ID`; 360 STFS demo and version; XVD updater → console system; region filled for 3DS/Wii U/GC/Wii/Sony fixtures.
- Keys rule: every new test that has a licence beside a game runs under `start_open_log` and asserts the licence path is **never** in the open log (plan and apply, move and copy).
- `organize` (integration, `tests/organize_game_folders.rs`): plan snapshot with `coverly` (base + update + 2 DLC → paths of D5, `DirCreated` only for used folders); option off → today's flat snapshot unchanged; folder games (PS3 JB with update folder); demo folder and `DemoOf`; orphan with and without title (D13); pile game folder with `Mods/`, `Saves/` left `GameFolderContent`; role hint applies only without header kind; existing `<Game>/` reused, file `<Game>` collision; case-different `updates/` reused; licence follows in move/same volume, stays in copy, stays when ambiguous; path length counted on the deep path; apply + undo round trip restores every path and removes only empty created folders; undo with a user file inside keeps the folder.
- `scan`: children attached to the base; orphans top-level with `role`; Mods/Cheats/Extras/Saves skipped; revision group and revision text.
- `dedupe`: update never paired with its base by role even with equal bytes in another role folder; two updates → versions, higher kept; `(Rev 1)` vs untagged → keeper `(Rev 1)`; the never-all guard refuses a crafted plan.
- `profiles`/`settings`: built-in `coverly` values; TOML round trip of `game_folders`; override map normalise drops unknown profile ids; `uses_game_folders` precedence.
- CLI: `--game-folders`/`--no-game-folders` change the plan; text exists for every new variant (exhaustive match compiles).
- Fixtures: mtimes pinned (`filetime`) before planning; paths compared by `Path::components()`, never by slash text (Windows CI).

### 14.2 Vitest (`pnpm check-all`)

- `organize.test.ts`: folding by `game.of` (two bases with one title), fallback by name for old plans; `gameFolderPreview` shape and counts; note and reason texts; version text.
- `GameFolderPreview.test.tsx`: list semantics, `aria-expanded`, «ya existe» state.
- `SystemGroups.test.tsx`: child row shows the version and shortened target.
- `LibrarySection` / `Sections.test.tsx`: switch reads the override or the profile default, writes and clears the map entry, hint names the default layout.
- `contract.test.ts`: sample round trip with the new fields; i18n parity test.

### 14.3 E2E (Chrome DevTools MCP, isolated context `game-folders`, dev server on port **1470**)

1. `/?mockPick=gamefolders`, layout `Biblioteca Coverly`: the 3DS base shows its update and DLC folded beneath with versions in mono; open «Ver cómo queda la carpeta»; screenshot at 1440 and 390 px, light and dark.
2. Rows for `Mods/` and `Saves/` read «Carpeta Mods de … se queda donde está por ahora»; `prod.keys` left; the `.rif` row says its licence goes with the PKG.
3. Ajustes → Biblioteca: the switch is on for `Biblioteca Coverly`; change the default layout to Anbernic stock: off; turn it on, back to Organizar, re-plan: the Anbernic plan now nests; keyboard-only pass over the switch and the disclosure; no console errors.
Fallback: Playwright MCP, then `playwright-core` with Chrome for Testing from the scratchpad. Stop the server only by port (`lsof -ti tcp:1470 -sTCP:LISTEN | xargs -r kill`).

Real-data check (read-only): plan only (no apply) with `coverly` against a `cp -RXp` copy of a few `~/Documents/ROMS` folders under `target/`, deleted afterwards, no `._` files; never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`.

### 14.4 Gates

`pnpm check-all`, `pnpm build`, `pnpm tokens:check` (no new token expected), `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`.

## 15. Tasks (granular conventional commits, `git push` after every commit)

1. `docs(spec): design game folders, part A` (this file).
2. `feat(core): name the fixed game folders` (D3).
3. `feat(core): extend title kinds, regions and versions to every console with updates` (D10, D11, D14) + reader tests.
4. `feat(core): recognise licences, firmware, shader caches and emulator configs by name` (D17 names, D18).
5. `feat(core): add the game folders option and the Coverly library profile` (D1, D2) + settings.
6. `feat(core): plan bases, updates, DLC and demos into game folders` (D5, D6, D12, D13, D16, D20).
7. `feat(core): move a licence with its game when unambiguous` (D17 companions).
8. `feat(core): leave fixed game folders alone when walking a pile` (D7).
9. `feat(core): attach updates and DLC to their game in the scan` (D8, D15 model).
10. `feat(core): keep roles and revisions apart in duplicates` (D9, D15 guard).
11. `feat(cli): game folders flags and texts` (D23).
12. `feat(ipc): carry game links and the option to the app` (contract).
13. `feat(ui): fold children by link and preview the game folder` (§9.1).
14. `feat(ui): game folders switch in library settings` (§9.1, §10).
15. `test(e2e): game folders mock pile` and evidence.

## 16. Sources

- `docs/02-research/updates-dlc-per-system.md` (2026-11-01; 3dbrew Titles and TMD, switchbrew CNMT/Ticket, nxdumptool `title.h`, wiiubrew Title metadata, psdevwiki PARAM.SFO, Xenia `xbox.h`/`stfs_xbox.h`, xvdtool `XVDStructs.cs`/`XVDEnums.cs`, No-Intro and Redump naming conventions).
- `docs/02-research/mods-cheats-extras-per-system.md` (licence and key file names, «not a game» lists).
- Code at `f681e1b3` (`feat/game-folders` base).

## 17. Self-review

- **Placeholders**: none left; every key, field and file is named. Three rows of §12 were tightened during review: the flat-base case follows D19 (no folder around a file not picked), a file named `Updates` makes the child stay with `UnsafeTarget` (fixed names never get ` (2)`), and two XVDs with one ProductId become a `(2)` folder for review.
- **Contradictions checked**: D4 (only used folders) vs D7 (Mods/ left) — consistent, part A creates neither. D17 (licences may move) vs batch 2 D4 (never moved) — superseded by the owner's brief for the unambiguous rename only, and copy mode still never touches them; the `keyMaterial` copy changes accordingly (§10). D10 (region always when known) vs the brief's «si hay el mismo juego en varias regiones» — a superset, listed as Q3. D19 (no restructure) vs brief item 2 — item 2 is about what Organizar moves from a pile; restructuring sorted games is a new action (Q1). D18 classes firmware by `.PUP` names only, never `EBOOT.PBP`, since a PSP game's own `EBOOT.PBP` must never be classed as firmware. D12 links demos by name only and D16's `demo_of` is a title string, not an item id, consistently.
- **Scope**: no six-views work, no Mods/Cheats association, no translators, no Extras CRUD; the only UI is the Organizar fold/preview and one switch.
- **Keys rule**: every new recognition is name-, id- or plaintext-header-based; licences are never opened, hashed or copied; the open-log assertion in §14.1 enforces it.
