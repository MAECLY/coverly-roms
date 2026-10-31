# Coverly ROMs — Missing systems, batch 1: retro without new formats (Design Spec)

> Status: **Implemented on `feat/systems-batch-1`** (self-reviewed §17, adversarially reviewed against the code §18) · Date: 2026-10-26 (research run 2026-10-31) · Owner: Miguel Angel Esparza Calero (MAECLY)
> Roadmap item: `TODO.md` §2 «Sistemas que faltan», **Tanda 1 only**. Tanda 2 (GameCube, Wii, 3DS…) and Tanda 3 (PS4, PS5, Xbox One/Series) are separate later runs and are not started here.
> Branch: `feat/systems-batch-1`. `docs/TRACKER.md` and `TODO.md` are updated on `main` after merge, not on this branch.
> Builds on: [`2026-10-23-organize-archives-and-discs.md`](2026-10-23-organize-archives-and-discs.md) (disc sets, layouts, containers), [`2026-10-18-system-icons-and-views.md`](2026-10-18-system-icons-and-views.md) (SystemIcon, Slot mount), [`docs/02-research/layouts.md`](../../02-research/layouts.md) (profile evidence), brand [`docs/08-brand/brand.md`](../../08-brand/brand.md).

## 1. Purpose

`crates/coverly-core/src/systems/systems.toml` defines 54 systems. Anything else in a library is either invisible (no folder alias), misfiled (a `.st` Sufami Turbo cart counts as Atari ST, a `.sgx` as PC Engine, an MSX2 folder as MSX) or left as `Unsupported` (PlayStation 2, 3DO and Sega Pico are detected by their header and then dropped). Tanda 1 adds the retro systems that need **no new file-format reader**: cartridges, tapes, floppies and CD/DVD images that the scanner, the disc layer (`disc/`) and the archive layer already open.

The batch is large (64 new systems, 118 in all). The design keeps it safe in three ways:

1. **No existing classification moves silently.** An extension a current system already owns stays its evidence unless the change is deliberate and listed (§4 D3, §14.1 snapshot).
2. **Folder names come from sources, never from guesses.** Every built-in profile gets an explicit per-system table, with «no folder» as a first-class value (§4 D5, §6).
3. **Ambiguous files go to review.** `.bin`, `.rom`, `.dsk`, `.tap`, `.d88`, `.hex`… are never auto-assigned by extension alone; only a folder alias, a header or a DAT hash can do that.

## 2. The owner's asks (verbatim from `TODO.md` §2)

> Hoy `crates/coverly-core/src/systems/systems.toml` define 54 sistemas. Faltan los de esta lista. Por cada sistema hay que añadir:
>
> 1. Su entrada en `systems.toml`: `id`, nombre, alias de carpeta, extensiones, carpeta de libretro-thumbnails si existe, DAT de libretro-database si existe e id de ScreenScraper.
> 2. Su carpeta en cada perfil de consola (`crates/coverly-core/src/profiles/builtin.rs`) donde el frontend lo soporte, y `disc_layout` si es de disco.
> 3. Identificación: por cabecera cuando la haya (`identify/`, `disc/`) y por hash con su DAT.
> 4. Su icono en `SystemIcon` (silueta de RetroArch Assets si existe; si no, una propia).
> 5. Tests: detección por cabecera, carpeta de destino en Organizar, búsqueda de portada y snapshot del plan.
>
> ### Tanda 1: retro sin formatos nuevos
>
> Cartuchos, cintas y discos que ya entran en los lectores actuales.
>
> - [ ] PlayStation 2 (`ps2`). El disco ya se detecta (`BOOT2` en `SYSTEM.CNF`, volumen `PLAYSTATION` mayor que un CD) y hoy sale como `Unsupported("PlayStation 2")`. Formatos: ISO, CHD, CSO, ZSO. Redump.
> - [ ] 3DO (`3do`). El disco ya se detecta y hoy sale como `Unsupported("3DO")`. Formatos: ISO, CUE/BIN, CHD.
> - [ ] Atari Jaguar y Jaguar CD (`jaguar`, `jaguarcd`). Formatos: `.j64`, `.jag`, `.rom`, `.abs`, `.cof`, CUE/BIN y CDI para el CD.
> - [ ] ColecoVision (`colecovision`). Formatos: `.col`, `.rom`.
> - [ ] Intellivision (`intellivision`). Formatos: `.int`, `.bin`, `.rom`.
> - [ ] Vectrex (`vectrex`). Formatos: `.vec`, `.gam`, `.bin`.
> - [ ] Magnavox Odyssey² / Videopac (`odyssey2`). Formato: `.bin`.
> - [ ] Fairchild Channel F (`channelf`). Formatos: `.bin`, `.chf`.
> - [ ] Emerson Arcadia 2001 (`arcadia`). Formato: `.bin`.
> - [ ] Watara Supervision (`supervision`). Formatos: `.sv`, `.bin`.
> - [ ] Sega SG-1000 / SC-3000 (`sg1000`). Formatos: `.sg`, `.sc`.
> - [ ] Sega Pico (`pico`). Formato: `.md`.
> - [ ] PC Engine SuperGrafx (`supergrafx`). Formatos: `.sgx`, `.pce`.
> - [ ] NEC PC-FX (`pcfx`). Formatos: CUE/BIN, CHD.
> - [ ] Philips CD-i (`cdi`). Formatos: CHD, CUE/BIN.
> - [ ] Amiga CD32 y CDTV (`amigacd32`, `cdtv`). Formatos: CUE/BIN, ISO, CHD.
> - [ ] Nintendo 64DD (`n64dd`). Formato: `.ndd`.
> - [ ] Satellaview y Sufami Turbo (`satellaview`, `sufami`). Formatos: `.bs`, `.st`.
> - [ ] Nintendo Famicom/NES en variantes de catálogo: comprobar si hacen falta entradas aparte para Famicom (BS-X, Disk Writer) o basta con alias.
> - [ ] Tiger Game.com (`gamecom`). Formatos: `.tgc`, `.bin`.
> - [ ] Mega Duck / Cougar Boy (`megaduck`). Formato: `.bin`.
> - [ ] Epoch Super Cassette Vision (`scv`). Formatos: `.bin`, `.0`.
> - [ ] Ordenadores: ZX Spectrum (`zxspectrum`) `.tzx .tap .z80 .sna .dsk .scl .trd`; ZX81 (`zx81`) `.p .81`; Amstrad CPC (`amstradcpc`) `.dsk .cdt .sna`; Sharp X68000 (`x68000`) `.dim .xdf .hdm .hdf .2hd .m3u`; Sharp X1 (`x1`) `.2d .2hd .dx1 .tap`; NEC PC-98 (`pc98`) `.d88 .fdi .hdi .nhd`; NEC PC-88 (`pc88`) `.d88 .t88`; FM Towns (`fmtowns`) CUE/BIN, CHD, `.d88`; MSX2 y MSX turboR, si los perfiles los separan de `msx`; Apple II (`apple2`) `.dsk .do .po .nib .woz .2mg`; BBC Micro (`bbcmicro`) `.ssd .dsd .uef`; Commodore Plus/4, PET y C128 (`plus4`, `pet`, `c128`) `.prg .d64 .t64 .tap .crt`; Atari XE / XEGS, si los perfiles los separan de `a800`; TI-99/4A (`ti99`) `.rpk .ctg .bin`; Thomson MO/TO (`thomson`) `.fd .k7 .m7 .rom`; Dragon 32/64 y TRS-80 CoCo (`dragon`, `coco`) `.cas .dsk .ccc`.
> - [ ] Consolas de fantasía y homebrew: TIC-80 (`tic80`) `.tic`; WASM-4 (`wasm4`) `.wasm`; Arduboy (`arduboy`) `.hex .arduboy`; Uzebox (`uzebox`) `.uze .hex`; LowRes NX (`lowresnx`) `.nx`; Vircon32 (`vircon32`) `.v32`.
> - [ ] Ports y motores: PrBoom/Doom (`.wad`), Quake (`.pak`), Cave Story (NXEngine), Wolfenstein 3D (ECWolf), Mr. Boom, Dinothawr, Lutro, Solarus, Flashback (REminiscence), Rick Dangerous (XRick). Necesitan reglas de «juego = carpeta» como ScummVM.
> - [ ] Arcade adicional: Model 2, Model 3, Naomi 2, Triforce, Sega System 16/18/24/32, Taito Type X, Konami, Cave, CHD de MAME por juego.
>
> ### Decisiones abiertas
>
> - [ ] Nombres de las carpetas por perfil (Anbernic stock, muOS, Knulli, Batocera, ES-DE, RetroArch) para cada sistema nuevo. Investigarlo antes de cada tanda.
> - [ ] Qué sistemas aparecen por defecto y cuáles solo si el perfil los usa.
> - [ ] Licencias de los iconos que no estén en RetroArch Assets.

Workflow brief additions: research first and record the sources here; never guess a folder name (a frontend without a folder for a system leaves that profile without it); `.bin`/`.rom`/`.dsk` and other shared extensions are never auto-assigned by extension alone; folder-per-game ports follow the existing ScummVM-style rules; anything that needs an owner decision is implemented around and listed (§12), not allowed to block the batch.

## 3. What exists today (read before changing)

| Layer | Where | What matters for this batch |
|---|---|---|
| System DB | `systems/systems.toml`, `systems/mod.rs` `System` | `id`, `name`, `aliases` (global, case-insensitive, **first match in file order wins** in `resolve_folder`), `extensions`, `libretro`, `arcade`, `multi_file`, `header`, `dats` (repo-relative paths; default derived as `metadat/no-intro/<libretro>.dat` or `metadat/redump/<libretro>.dat` when `multi_file`), `screenscraper`. |
| Extension class | `systems/mod.rs` `extension_class` | `Container` (zip, 7z), `Generic` (bin, iso, img, rom, dat, chd), `Unique` (one system lists it), `Shared` (several do), `Unknown` (incl. everyday png/exe/bat/conf). |
| Evidence | `organize/evidence.rs` | Unique extension = 0.85 (auto threshold 0.85, needs content support), Shared = 0.50 (review), Generic = nothing. `DECISIVE` systems (from `identify/sniff.rs`) lose a unique-extension claim to 0.60 when the bytes lack their signature. |
| Header sniff | `identify/sniff.rs` | Discs: Saturn, Dreamcast, PSP, PlayStation (`BOOT2` → `unsupported = "PlayStation 2"`, a `PLAYSTATION` volume over `CD_MAX` → same). The PS2 serial is **already** parsed from `BOOT2` (`s.serial = boot_file_serial(boot2)`); only the verdict is missing. Carts: GB/GBC/GBA/N64/NDS/Lynx/7800/NES/SNES/Mega Drive family; `SEGA PICO` at 0x100 → `unsupported = "Sega Pico"` (checked before the Mega Drive `SEGA` rule). `DECISIVE` = `gb gbc gba nds n64 nes fds sega32x`. `TEXT_NATIVE_EXTS` (`p8 scummvm svm conf bat bas cue m3u gdi ccd`) are the only extensions whose text content is not `NotARom { text }`. |
| Containers | `organize/inspect.rs` `container_for` | **Magic first**: any file whose bytes start with a zip/gzip/7z magic is inspected as an archive whatever its extension. |
| Memoised facts | `organize/plan.rs` `FACTS_VERSION = 5`, `cache` `organize_ids` (`path + size + mtime → facts`), `organize_choices` (`crc32 + size → system_id`) | Facts hold the sniff and disc verdicts (`FoundSystem::Unsupported { "PlayStation 2" }`); a row of another version is read again. |
| Templates | `profiles/template.rs` | `{system_id}` renders the **Coverly id**, not the folder name; ES-DE (`downloaded_media/{system_id}`) and muOS (`catalogue/{system_id}`) use it. |
| DAT download | `identify/dat.rs` `DATABASE_BASE` | Fetched from libretro-database **`master`**, not a pinned commit; `MAX_DAT_DOWNLOADS = 4` per plan. |
| Disc layer | `disc/mod.rs`, `disc/signatures.rs`, `disc/iso9660.rs` | Opera header (3DO) → `DiscSystem::Unsupported("3DO")`; PS2 → `Unsupported("PlayStation 2")`. Sheets (`cue`, `gdi`, `ccd`, `m3u`), CHD, CSO, PBP readers. |
| Hashing | `identify/hash.rs` | Files over `DEFAULT_HASH_MAX_BYTES` (64 MiB) are not hashed; `cue gdi m3u ccd chd 7z pbp cso rvz` are identified by name only. DATs are clrmamepro text, ≤ `MAX_DAT_BYTES` (64 MiB), serials parsed (`identify/dat.rs`). |
| Profiles | `profiles/builtin.rs`, `profiles/mod.rs`, `profiles/naming.rs`, `profiles/custom.rs` | 8 built-ins: `anbernic-stock`, `garlicos`, `onion` (Imgs family, `FolderNaming::FirstAlias`, `host_folders` `ngpc → ngp`, `wonderswancolor → wonderswan`), `muos` (`Id`), `minui` (FirstAlias), `esde`, `batocera` (Batocera/Knulli/ES), `retroarch` (all three `Id`). Only Anbernic has `known_folders` (→ `ItemNote::NoFolderOnDevice`). `disc_layout` and `container_overrides` are per profile; `NO_ARCHIVE_SYSTEMS = ["psp"]` lives in `profiles/mod.rs`, and `Profile::accepts_as_is` already refuses every archive for `multi_file` systems. A built-in has no per-system folder table; a **custom** profile file already has `folder_names: BTreeMap<id, folder>` (merged under the user's `settings.organize.folder_names[profile]` by the planner and validated with `validate_folder_override`); cloning a built-in writes it empty. Unknown fields in a profile file are ignored (no `deny_unknown_fields`). |
| Folder resolution | `resolve_folder` callers: `scan/mod.rs:248` (has the profile), `organize/walk.rs:161` (every folder of a pile at any depth: a match is «already sorted» and **not walked**), `organize/plan.rs:2148` (quarantine), `:2695` (probe), `profiles/naming.rs:68, 91` (`default_name`, `validate_folder_override`), `profiles/custom.rs:418` (preview), `profiles/fit.rs:35` and `profiles/detect.rs:90` (profile detection, **no profile by definition**), `settings/validate.rs:278, 338`, CLI `dedupe.rs:206` | Global only. |
| Folder games | `dedupe/units.rs` `OUT_OF_SCOPE_DIRS` (`ports bios apps saves states`); ScummVM | ScummVM is claimed by its folder alias and by its launcher extensions (`.scummvm`, `.svm`, text-native). That is all the «ScummVM-style rule» is today: there is **no** organise rule against splitting a game folder (a loose `.scummvm` moves alone) and dedupe skips `ports`, not `scummvm`. D15's folder-game rule is new code, not reuse. |
| User systems file | `SystemDb::with_overrides` | Merges by id: the user's aliases win (removed from every other system), extensions are appended, `name` only required for new ids. |
| Icons | `scripts/icons/systemIconsLib.mjs` (`COMMIT d9f96905…`, `SYSTEM_ICON_SOURCES`), `pnpm system-icons`, `src/components/system/SystemIcon.tsx`, `systemFallbacks.ts` (`FALLBACK_ABBR`, `WORDMARK_IDS`), `THIRD-PARTY-NOTICES.md` (41 files listed) | Silhouette when mapped, otherwise the Slot mount with an abbreviation (≤ 4 chars). |
| UI | `LeftInPlace.tsx` (`NativeSelect` with every system), `ReviewBuckets.tsx` (`NativeSelect` with the **bucket's candidate systems** only, plus «Dejar»), `SystemInfo { id, name }` from `src-tauri/src/ipc.rs` `list_systems` (built-in systems only) | LeftInPlace: a flat list of 54 names today; 118 after this batch. Review: 2–8 candidates, unchanged in size. |

## 4. Decisions

Open questions and the default chosen. Each can be overturned by the owner without changing the rest of the design.

| # | Question | Decision | Why |
|---|---|---|---|
| D1 | Scope of «Tanda 1» | Every bullet of Tanda 1 except the ones in §12 (blocked or deliberately not a system). Tanda 2/3 untouched. | Brief. |
| D2 | Ids | **The owner's ids where given.** Ports (no id given): `doom`, `quake`, `cavestory`, `wolf3d`, `mrboom`, `dinothawr`, `lutro`, `solarus`, `flashback`, `xrick`. Arcade: `model2`, `model3`, `naomi2`, `triforce`. Computers split by the owner's «si los perfiles los separan»: `msx2`, `msxturbor`, `atarixe` (ES-DE and Batocera both separate them, §6). | Ids are stable keys for overrides and settings; frontend spellings go in the per-profile tables (D5), so the id never has to bend to one frontend. |
| D3 | Extensions an existing system already owns (`.st` atarist, `.md` megadrive, `.pce`/`.sgx` pcengine, `.cdi` dreamcast, `.cso` psp, `.pak` openbor, `.atr/.xex/.xfd/.car` a800, `.mx2` msx, `.d64/.prg…` c64) | New field **`weak_extensions`** in `systems.toml`: accepted inside the system's folder and as a candidate once a header or a DAT hash names the system, but **never counted by `extension_class`**. The listed existing owner keeps its class. Deliberate class changes (Unique → Shared because the ambiguity is real) are only: `.dsk` (msx → msx, zxspectrum, amstradcpc, apple2, dragon, coco, msx2, msxturbor), `.dim` (atarist, x68000), `.hdf` (amiga, x68000), `.fdi` (amiga, pc98), `.sgx` moves from `pcengine` to `supergrafx` and `.bs` from `snes` to `satellaview` (each is that system's own format; the Anbernic `PCE` and `SFC` folders still accept them through `weak_extensions` of `pcengine` and `snes`). | A Sufami cart has a 14-byte magic, a Pico cart `SEGA PICO`: their header is the proof, not the extension. Turning `.st` Shared would send every Atari ST disk to review. The four real collisions follow the brief («never by extension alone»). A snapshot test (§14.1) pins every pre-batch extension's class with exactly this diff. |
| D4 | Extensions that say little even when unique (`.wasm`, `.hex`, `.p`, `.0`, `.fd`, `.do`, `.po`, `.2d`, `.abs`, `.cof`, `.gam`, `.lua`, `.game`) | Listed as **`weak_extensions`** of their system too. | `.wasm`, `.hex`, `.lua` are everyday developer files; `.p`, `.0`, `.fd`, `.do`, `.po` are too short to mean anything. Inside `ZX81/` a `.p` file is a ZX81 tape; on the desktop it is not. |
| D5 | Where per-profile folder names live | New `Profile.folders: BTreeMap<String, ProfileFolder>` with `ProfileFolder::Named(String)` or `ProfileFolder::Absent`, filled in `builtin.rs` for **every Tanda 1 system on every built-in profile** (except `retroarch`, which keeps `Id`). Order in `system_folder`: user override → `Named` → `host_folders` → naming rule. `Absent` keeps the naming rule's folder and sets `on_device = Some(false)`, so organise adds the existing `NoFolderOnDevice` note («Onion OS no tiene carpeta para Atari Jaguar; se crea JAGUAR.»). Existing systems are not added to the tables: no behaviour change for them. **Custom profiles reuse the existing `ProfileFile.folder_names`** (no new file field): cloning a built-in copies its `Named` entries there; `Absent` is not persisted (a custom profile has no `known_folders`, so `on_device` stays `None`). A custom profile cloned before this batch has an empty map and keeps rule-based names; the profile editor offers «Actualizar carpetas desde {built-in}» only if the owner asks (§13). | Matches the brief («leave that profile without it») without inventing names, and reuses the note and copy that Anbernic stock already uses. Moving the item anyway, with the note, keeps one rule for «device lacks the folder»; leaving it in place would be a second, inconsistent rule. |
| D6 | A folder name two systems want (`pico` = Sega Pico on Batocera/Knulli, PICO-8 on Anbernic/Onion/muOS; `msx2`) | **Profile-scoped resolution**: `SystemDb::resolve_folder_in(profile: Option<&Profile>, name)` checks the profile's `Named` folders (for a custom profile: its `folder_names`) first, then global aliases. Every caller that knows the layout passes it (scan, organise walk, quarantine and probe, `default_name`, `validate_folder_override` and so custom-profile validation and settings validation, the custom-profile preview, CLI). `detect.rs` and `fit.rs` have no profile and stay global. `pico` stays a global alias of `pico8`; Sega Pico's global aliases are `SEGAPICO`, `segapico`. `msx2` **moves** from `msx` to the new `msx2` system (libretro has a separate `Microsoft - MSX2` DAT and thumbnails; ES-DE, Batocera and muOS all have an `msx2` folder). | Without a profile there is no way to know which `pico` is meant; the current owner keeps it. A test asserts that every `Named` folder resolves back to its system under its profile, and that global aliases stay unique. |
| D7 | Batocera vs Knulli (one built-in, «Batocera / Knulli / EmulationStation») | **Batocera's names are written**; Knulli's different names are added as **global aliases** so a Knulli card scans: `o2em` (odyssey2), `bbc` (bbcmicro), `tyrquake` (quake). Where Knulli has no folder (`sc3000`, `dragon64`) nothing changes: the shared profile follows Batocera. A separate Knulli profile is out of scope (§13). | Knulli is a Batocera fork; 9 of the batch's folders differ. Scanning both is free; writing one set keeps the profile single. |
| D8 | First alias (used by the Imgs family and MinUI as their default name) | Onion's folder when Onion has one (`COLECO`, `FAIRCHILD`, `ODYSSEY`, `SGFX`, `SEGASGONE`, `ZXS`, `TIC`, `CPC`…), else NextUI's tag (`C128`, `PET`, `PLUS4`, `PRBOOM`), else the upper-case id (`PS2`, `JAGUAR`…). | Onion is the only sourced folder list of the Imgs family (`layouts.md` row 3, R1-10); the tables (D5) still mark the profiles that lack the system. |
| D9 | GarlicOS folders | **Same table as Onion** (closed source, FAQ 404). | Same inference as `layouts.md` R1-10, which already uses Onion's lists for GarlicOS. |
| D10 | MinUI vs NextUI | The `minui` profile is «MinUI / NextUI»; NextUI tags are used (`COLECO`, `CPC`, `C128`, `PET`, `PLUS4`, `PRBOOM`, `SG1000`). Base MinUI has none of the batch's systems; the note says so on organise. | NextUI ships those paks at the pinned commit; MinUI base does not. |
| D11 | Anbernic stock | `Absent` for every Tanda 1 system, except the D23 hosts (`supergrafx → PCE`, `satellaview → SFC`, `msx2 → MSX`, `atarixe → A800`), which keep today's destinations. | `ANBERNIC_FOLDERS` mirrors the owner's card (55 folders), and none of the batch is there. |
| D12 | muOS | muOS resolves a ROM folder by the `friendly` names of each core manifest (`share/info/manifest/core/**.json`); the table uses those names (`spectrum` for ZX Spectrum, `segapico`, `pc8000`, `wolfenstein3d`…). Systems muOS folds into another catalogue (`n64dd` → «Nintendo N64», `satellaview`/`sufami` → «Nintendo SNES - SFC», `msx2` → «Microsoft MSX») keep their own folder (muOS accepts it). | muOS has no fixed folder list; its auto-assign reads these names. The cover path keeps today's `{system_id}` rule (see §13: the catalogue name is a separate, pre-existing question). |
| D13 | Disc layout for new disc systems | `multi_file = true` for `ps2`, `3do`, `jaguarcd`, `pcfx`, `cdi`, `amigacd32`, `cdtv`, `fmtowns`; each profile's existing `disc_layout` applies (Flat on the Imgs family/muOS, GameFolder + `.m3u` on MinUI and ES-DE, Flat + `.m3u` on Batocera and RetroArch). **`NO_ARCHIVE_SYSTEMS` is not changed**: `Profile::accepts_as_is` already returns `false` for every `multi_file` system, so `ps2` never takes a zip/7z as-is. Adding it would change `default_container_overrides()`, which `ProfileFile` uses as its serde default and skip-if-equal test, for no behaviour gain. | PCSX2/AetherSX2 read no zip/7z (ES-DE lists only `.gz`), like PPSSPP for `psp` (R1-20); `multi_file` already gives that. |
| D14 | PS2 / 3DO identification | PS2: serial from `SYSTEM.CNF` `BOOT2 = cdrom0:\SLUS_200.62;1` → `SLUS-20062` (already parsed by `sniff.rs` and `disc/iso9660.rs`; this batch only turns the `Unsupported` verdict into `ps2`), matched against the Redump DAT's serial field, then the name. `.zso` (ZISO, LZ4 blocks) has **no reader** today and Tanda 1 adds none: `zso` is a strong unique extension of `ps2` and joins `NAME_ONLY_EXTS` in `identify/hash.rs` (hashing the compressed bytes matches nothing). 3DO: Opera header, then the name (no serial on 3DO discs). Neither is hashed above 64 MiB (unchanged default). | Same ladder as PS1; hashing a 4 GB DVD is the user's choice (`hash max bytes`). |
| D15 | Ports / engines | **ScummVM-style**: claimed by their folder alias, and by a launcher/data extension only where the format proves it (`.wad` with `IWAD`/`PWAD`: `doom` joins `DECISIVE`, so a `.wad` without the magic, such as a future WiiWare WAD, drops to 0.60 and is never Auto; `.solarus`, `.lutro` per D25). New rule (there is none today, §3): a pile folder that is **not** a system folder but directly holds a file proven for a folder-game system (`IWAD`/`PWAD`, `.solarus`, `.lutro`, `.scummvm`/`.svm`) is **left in place as one item** with the new left reason `FolderGame { system }`; its other files are not organised one by one. A folder whose name is a folder-game alias is «already sorted», as today. Dedupe treats folders named by a folder-game system's aliases (and `scummvm`) like `ports`. | The brief. Cave Story, Wolfenstein 3D, Mr. Boom, Flashback and Rick Dangerous ship as data folders with no launcher Coverly could trust: by folder alias or DAT only; loose, their files stay `unknown` (no content rule is invented). |
| D16 | Extra arcade | New systems `model2`, `model3`, `naomi2`, `triforce` (folders on ES-DE and Batocera). **System 16/18/24/32, Konami, Cave: no new systems**: no frontend has a folder for them; their sets are in the MAME/FBNeo DATs and already go to `mame`/`fbneo`. Model 2/3 sets found loose also go to `mame` (MAME runs them; the libretro MAME DAT has no driver field to tell them apart); inside a `model2/` folder they stay. **MAME CHD per game**: a `<set>/` folder holding `.chd` beside `<set>.zip` becomes a companion of the set and moves with it. | Avoids empty systems; keeps one place per set. |
| D17 | Famicom variants | **Aliases only, no new entries.** `famicom` and `fc` already resolve to `nes` (ES-DE's separate `famicom` folder scans as NES); BS-X is the Satellaview (new system); Disk Writer games are FDS images. libretro has one NES DAT and thumbnail set. | One DAT and one cover set per system. |
| D18 | Which systems appear by default | All systems are always available in pickers; lists are **grouped by family** (§9.1). Shelves and counts only show systems that have games. | The owner's open question; no hiding, so a user is never unable to pick the right system. |
| D19 | Icon licences | Only RetroArch Assets XMB Monochrome (CC BY 4.0, already attributed), at the **same pinned commit** `d9f969054dc7fbb6fa89519036d2b971e0855b51` (it is also `master` on 2026-10-31, 849 files). No other icon source. Systems without a neutral silhouette use the Slot mount. | No new licence to review; the decision the owner left open is avoided. |
| D20 | Game logos in the icon set (`DOOM`, `Quake`, `Cave Story`, `Rick Dangerous`, `Flashback`, `TIC-80`, `Uzebox`, `Arduboy`, `MrBoom`, `Dinothawr`, `Lutro`) | Audited on the contact sheet like the first 41: a logo or wordmark goes to `WORDMARK_IDS` and uses the Slot mount; a neutral device or pictogram is used. | `TRADEMARKS.md` rule already applied to CPS, MAME, DOS, ScummVM. |
| D21 | `family` for grouping | New `family` field on every system (existing ones too): `console`, `handheld`, `computer`, `arcade`, `engine`. Sent in `SystemInfo`. When a user systems file omits it for a new id, the default is `arcade` if `arcade = true`, else `console`. | Needed for the grouped picker (D18); data, not UI logic. |
| D22 | Aliases that are everyday words or game titles (`doom`, `quake`, `flashback`, `solarus`, `lutro`, `xrick`, `mrboom`, `dragon`, `dragon32`, `dragon64`, `pet`, `coco`, `moto`, `theodore`, `x1`, `bbc`, `arcadia`, `cdi`, `turbor`) | New `scoped_aliases` list in `systems.toml`: resolved by `resolve_folder_in(Some(_), …)` (any profile) but **not** by profile-less `resolve_folder`, so `detect.rs`/`fit.rs` never call `~/Games` with `Doom/` and `Quake/` an organised library. The list is compared case-insensitively, so first aliases (D8) such as `DRAGON`, `PET`, `X1`, `QUAKE`, `ARCADIA`, `CDI`, `SOLARUS`, `LUTRO`, `XRICK`, `MRBOOM` are scoped too; `default_name` and `validate_folder_override` resolve under the profile, so they still name and accept those folders. | The organise walk skips every folder that resolves (§3): a `Dragon/` or `PET/` folder in Downloads would silently stop being organised, and detection would flip. Distinctive names (`zxspectrum`, `amstradcpc`, `colecovision`…) stay global. |
| D23 | A new sub-system on a profile that has only its parent's folder (`supergrafx` → PCE, `satellaview`/`sufami` → SFC, `msx2`/`msxturbor` → MSX, `atarixe` → A800) | A `host_folders` entry only where the system is `Absent` (§6.1) **and** the parent's folder is sourced on that profile: anbernic-stock (`PCE`, `SFC`, `MSX`, `A800` are in `ANBERNIC_FOLDERS`): `supergrafx → pcengine`, `satellaview → snes`, `msx2 → msx`, `atarixe → a800`; garlicos and onion (Onion ships an `MSX` package, no `MSX2`): `msx2 → msx`; minui: only for parents whose NextUI pak exists at the pinned commit (checked in the task, listed in the PR), else `Absent`. Profiles with the system's own folder (§6.1) use it. `sufami`, `msxturbor` never get a host (no evidence the parent core runs them). | Today a loose `.sgx` goes to `PCE/` and a `.bs` to `SFC/` (strong extensions of `pcengine` and `snes`); moving them into a new `SGFX/`/`SATELLAVIEW/` folder the device does not show would be a regression for installed users. A host keeps the old destination. Owner may revert (§13). |
| D24 | A header proves a system whose extension another system owns (`.md` + `SEGA PICO`, `.st` + Sufami magic, `.pce` + nothing, `.cso` + PS2 volume, `.cdi` + Jaguar CD header) | In `extension_evidence`, when the sniff or disc verdict names system X with weight ≥ 0.95 and X lists `ext` in `weak_extensions`, the strong owner's extension evidence is **contradicted** (0.60, detail `ext!`, existing copy `why.extensionContradicted`). | Without it the verdict rests on a 0.95 vs 0.85 lead that passes `MARGIN = 0.10` only through `EPS`; with it the lead is 0.35 and the existing «la cabecera no coincide» line explains the move. |
| D25 | Zip-based game packages (`.arduboy`, `.rpk`, `.solarus`, `.lutro`) and gzip `.uef` | Listed in a new `PACKAGE_EXTS` beside the container rules: magic-first inspection still reads them (to check `info.json`, `layout.xml`, `main.lua`, `UEF File!`), but the item is a **raw file**: never extracted, never «several games», moved as-is on every profile. | `container_for` is magic first (§3): without this an `.arduboy` would be treated as an archive and its `.hex` extracted, breaking the game. |
| D26 | Text game files (`.hex` Intel HEX, `.nx` LowRes NX source, `.lua`, `.game`) | Added to `TEXT_NATIVE_EXTS`. | Otherwise the sniff marks them `NotARom { text }` and they are left as «No es una ROM (texto)». |
| D27 | Facts and choices memoised before the upgrade | `FACTS_VERSION` 5 → 6 (facts of version 5 call PS2, 3DO and Pico discs/carts `Unsupported` and lack the new sniff hits), so they are read again lazily. `organize_choices` (user picks by `crc32 + size`) are kept: a remembered `msx` or `pcengine` pick still wins over the new systems, as a user choice should. `HASH_OUTCOME_VERSION` is unchanged (hash shape untouched). | Without the bump an installed user's PS2 ISO stays «Sistema no compatible» until its mtime changes. |

## 5. Architecture and files touched

### 5.1 Rust

| File | Change |
|---|---|
| `systems/systems.toml` | ≈ 70 entries (§7), `family` on all, `weak_extensions`, `scoped_aliases` (D22), `msx2` alias moved, `sgx` moved, Knulli aliases. Header comment documents the three new fields. |
| `systems/mod.rs` | `System { family: Family, weak_extensions: Vec<String>, scoped_aliases: Vec<String> }` (serde defaults per D21, empty); validation of the **built-in** file: a weak extension may not also be in `extensions`. A **user systems file is never rejected for the new fields**: an extension the user lists as strong is removed from that system's `weak_extensions` (the user wins, like aliases), an unknown `family` falls back to D21's default with a warning, and the user's aliases also remove matching `scoped_aliases` elsewhere; `extension_class` ignores `weak_extensions`; `systems_for_extension` keeps returning only strong owners; new `systems_accepting(ext)` (strong + weak) for folder and header paths; `resolve_folder_in(profile, name)`. |
| `profiles/mod.rs`, `profiles/naming.rs` | `Profile.folders`, `ProfileFolder`; `system_folder` order (D5); `default_name` and `validate_folder_override` resolve with `resolve_folder_in(Some(profile))`. |
| `profiles/builtin.rs` | Per-profile tables (§6.1) as `const` arrays of `(id, Option<&str>)`, converted once; D23 hosts for anbernic-stock and minui. `NO_ARCHIVE_SYSTEMS` (in `profiles/mod.rs`) is **not** changed (D13). |
| `profiles/custom.rs` | No new file field: cloning a built-in fills the existing `folder_names` from its `Named` entries (D5); `folder_names` doubles as the custom profile's scoped table in `resolve_folder_in`; `preview_paths` resolves under the profile. |
| `scan/mod.rs`, `organize/walk.rs`, `organize/plan.rs` (quarantine, probe), `settings/validate.rs`, `coverly-cli/src/dedupe.rs` | Call `resolve_folder_in` with the active profile. `profiles/detect.rs` and `profiles/fit.rs` keep the profile-less `resolve_folder` (D22). |
| `identify/sniff.rs` | New hits (§8.1); `PlayStation 2` and `Sega Pico` become hits (`ps2` 0.95 with `BOOT2`, 0.80 for a large `PLAYSTATION` volume without it; `pico` 0.95); `DECISIVE` gains `vectrex`, `pico`, `sufami`, `uzebox`, `vircon32`, `doom`; `TEXT_NATIVE_EXTS` gains `hex`, `nx`, `lua`, `game` (D26). |
| `identify/hash.rs` | `NAME_ONLY_EXTS` gains `zso` (D14). |
| `organize/inspect.rs` | `PACKAGE_EXTS` (D25): zip/gzip packages are raw items. |
| `disc/mod.rs`, `disc/signatures.rs`, `disc/iso9660.rs` | `DiscSystem::Known("ps2" / "3do" / "pcfx" / "cdi" / "amigacd32" / "cdtv" / "jaguarcd")`; PS2 serial from `BOOT2`; new signatures (§8.2). `Unsupported(...)` stays for any future system. |
| `organize/evidence.rs` | Weak-extension evidence: 0.50 inside a non-system folder **only** when another signal agrees; detail `ext~` («weak»). Strong owner contradicted by a weak owner's ≥ 0.95 header (D24). |
| `organize/plan.rs`, `organize/walk.rs`, `organize/target.rs` | `LeftReason::FolderGame { system }` (the enum lives in `plan.rs`); folder-game detection in the walk (D15); MAME CHD companion (D16); `FACTS_VERSION` 5 → 6 (D27). |
| `dedupe/units.rs` | Folder-game systems (`scummvm` plus D15's ports) treated like `ports`. |
| `src-tauri/src/ipc.rs` (`list_systems`) | `SystemInfo` gains `family`. |

### 5.2 Front end

| File | Change |
|---|---|
| `src/ipc/types.ts`, `contract.sample.json`, `contract.test.ts` | `SystemInfo.family: 'console' \| 'handheld' \| 'computer' \| 'arcade' \| 'engine'`; `LeftReason` `folder-game`. |
| `src/components/organize/LeftInPlace.tsx` | The full system list grouped with `<optgroup>` by family (§9.1), names sorted with `Intl.Collator(locale)`. Helper `systemOptionGroups(systems, t)` in `src/lib/systems.ts`. `ReviewBuckets.tsx` lists only a bucket's 2–8 candidates and stays a flat list in its current order; «Dejar» stays the last option. |
| `src/lib/organize.ts` | Copy for `folder-game` and the weak-extension reason. |
| `src/components/system/systemFallbacks.ts` | `FALLBACK_ABBR` and `WORDMARK_IDS` for the batch (§9). |
| `scripts/icons/systemIconsLib.mjs` | New `SYSTEM_ICON_SOURCES` rows; regenerate with `pnpm system-icons` (writes `src/assets/system-icons/*`, `systemIcons.generated.ts`, README, contact sheet). |
| `THIRD-PARTY-NOTICES.md` | File count and id list updated; same licence paragraph. |
| `locales/{es,en}/ui.json` | Keys in §10. |
| `src/ipc/mock/mockOrganize.ts`, `fixture.ts` | `?mockPick=retro` pile (§9.3); a few new systems in `FIXTURE_SYSTEMS` so shelves show silhouettes and mounts. |

## 6. Per-profile folders (research result)

Legend: a name = `Named`; `—` = `Absent` (no folder in that frontend at the pinned source). `retroarch` uses the id everywhere and is omitted. Garlic = Onion (D9); Anbernic stock = `—` for every row (D11). Knulli differences in brackets (D7).

### 6.1 Table

| id | ES-DE | Batocera [Knulli] | muOS | Onion / Garlic | MinUI / NextUI |
|---|---|---|---|---|---|
| ps2 | ps2 | ps2 | — | — | — |
| 3do | 3do | 3do | 3do | — | — |
| jaguar | atarijaguar | jaguar | jaguar | — | — |
| jaguarcd | atarijaguarcd | jaguarcd | — | — | — |
| colecovision | colecovision | colecovision | colecovision | COLECO | COLECO |
| intellivision | intellivision | intellivision | intellivision | INTELLIVISION | — |
| vectrex | vectrex | vectrex | vectrex | VECTREX | — |
| odyssey2 | odyssey2 | odyssey2 [o2em] | odyssey2 | ODYSSEY | — |
| channelf | channelf | channelf | channelf | FAIRCHILD | — |
| arcadia | arcadia | arcadia | — | — | — |
| supervision | supervision | supervision | supervision | SUPERVISION | — |
| sg1000 | sg-1000 | sg1000 (SC-3000 games also in `sc3000`, Batocera only) | sg1000 | SEGASGONE | SG1000 |
| pico | — | pico | segapico | — | — |
| supergrafx | supergrafx | supergrafx | supergrafx | SGFX | — |
| pcfx | pcfx | pcfx | pcfx | — | — |
| cdi | cdimono1 | cdi | cdi | — | — |
| amigacd32 | amigacd32 | amigacd32 | — | — | — |
| cdtv | cdtv | amigacdtv | — | — | — |
| n64dd | n64dd | n64dd | n64dd | — | — |
| satellaview | satellaview | satellaview | satellaview | SATELLAVIEW | — |
| sufami | sufami | sufami | sufami | SUFAMI | — |
| gamecom | gamecom | gamecom | — | — | — |
| megaduck | megaduck | megaduck | megaduck | MEGADUCK | — |
| scv | scv | scv | scv | — | — |
| zxspectrum | zxspectrum | zxspectrum | spectrum | ZXS | — |
| zx81 | zx81 | zx81 | zx81 | — | — |
| amstradcpc | amstradcpc | amstradcpc | amstradcpc | CPC | CPC |
| x68000 | x68000 | x68000 | x68000 | — | — |
| x1 | x1 | x1 | x1 | — | — |
| pc98 | pc98 | pc98 | pc98 | — | — |
| pc88 | pc88 | pc88 | pc8000 | — | — |
| fmtowns | fmtowns | fmtowns | — | — | — |
| msx2 | msx2 | msx2 | msx2 | — | — |
| msxturbor | msxturbor | msxturbor | — | — | — |
| apple2 | apple2 | apple2 | — | — | — |
| bbcmicro | bbcmicro | bbcmicro [bbc] | — | — | — |
| plus4 | plus4 | cplus4 | plus4 | — | PLUS4 |
| pet | — | pet | pet | — | PET |
| c128 | — | c128 | c128 | — | C128 |
| atarixe | atarixe | xegs | — | — | — |
| ti99 | ti99 | ti99 | — | — | — |
| thomson | moto | thomson | thomson | — | — |
| dragon | dragon32 | dragon64 [—] | dragon | — | — |
| coco | coco | coco | coco | — | — |
| tic80 | tic80 | tic80 | tic80 | TIC | — |
| wasm4 | wasm4 | wasm4 | wasm4 | — | — |
| arduboy | arduboy | arduboy | arduboy | — | — |
| uzebox | uzebox | uzebox | uzebox | — | — |
| lowresnx | lowresnx | lowresnx | lowresnx | — | — |
| vircon32 | vircon32 | vircon32 | vircon32 | — | — |
| doom | doom | prboom | doom | — | PRBOOM |
| quake | quake | quake [tyrquake] | quake | — | — |
| cavestory | — | cavestory | cavestory | — | — |
| wolf3d | — | ecwolf | wolfenstein3d | — | — |
| mrboom | — | mrboom | — | — | — |
| dinothawr | — | — | — | — | — |
| lutro | lutro | lutro | — | — | — |
| solarus | solarus | solarus | — | — | — |
| flashback | — | reminiscence | — | — | — |
| xrick | — | xrick | — | — | — |
| model2 | model2 | model2 | — | — | — |
| model3 | model3 | model3 | — | — | — |
| naomi2 | naomi2 | naomi2 | — | — | — |
| triforce | triforce | triforce | — | — | — |

Notes on the table:
- Onion's `TIC` package lists `tic|fd|sap|k7|m7|rom` (Thomson extensions); the folder name is still `TIC`. Recorded as found, not «fixed».
- muOS `odyssey2` and `videopac` both map to «Magnavox Odyssey - VideoPac»; ES-DE's separate `videopac` (G7000) folder becomes a global alias of `odyssey2` (same games). Videopac+ (G7400, `videopacplus`) is not part of the ask (§13).
- Every `Named` folder is also an alias of its system unless it collides (`pico`, D6); everyday words and game titles are `scoped_aliases`, never global (D22).
- Onion extension lists differ from §7 in places (its `SUFAMI` package lists `smc|zip|7z` only, so a `.st` cart moved there is not shown by Onion; its `SATELLAVIEW` lists `bs|sfc|smc|swc|fig|st`). Each system task compares the Onion `extlist` with §7 and lists every mismatch in the PR; Coverly never renames a file to fit (§13).
- Batocera has a separate `gx4000` folder (Amstrad GX4000 `.cpr` carts) and ES-DE a separate `to8` and `gx4000`; `to8` is a global alias of `thomson`, `gx4000` is not part of the ask (§13).

### 6.2 Disc layout per profile

No per-system override: D13. ES-DE (`ps2/<stem>/<stem>.cue`) and MinUI (`PS2/…`, `Absent`, with the note) get a folder per CUE/BIN set; a single `.iso`/`.chd` moves as a file. FAT32 cards: a PS2 ISO over 4 GiB is left with the existing `TooBigForFat` reason.

## 7. System entries (research result)

`libretro` = exact folder on <https://thumbnails.libretro.com/> (live root listing, 127 folders, 2026-10-31); `—` = no folder there (covers from ScreenScraper or the user only). `dats` = path in libretro-database at `2c93952f` (`—` = none; «derived» = the default rule gives the right path). SS = ScreenScraper `systemeid`. Weak = `weak_extensions` (D3, D4). All non-disc cartridge systems also list `zip` and `7z`. Family: C console, H handheld, PC computer, A arcade, E engine.

| id | name | aliases (first = D8) | extensions | weak | libretro | dats | SS | fam |
|---|---|---|---|---|---|---|---|---|
| ps2 | Sony PlayStation 2 | PS2, playstation2 | zso, iso, chd, cue, bin, m3u | cso | Sony - PlayStation 2 | derived (redump) | 58 | C, disc |
| 3do | 3DO Interactive Multiplayer | 3DO, 3d0 | iso, cue, bin, chd | — | The 3DO Company - 3DO | derived (redump) | 29 | C, disc |
| jaguar | Atari Jaguar | JAGUAR, atarijaguar | j64, jag, rom, bin | abs, cof | Atari - Jaguar | derived | 27 | C |
| jaguarcd | Atari Jaguar CD | JAGUARCD, atarijaguarcd | cue, bin, chd | cdi | — | metadat/redump/Atari - Jaguar CD.dat | 171 | C, disc |
| colecovision | ColecoVision | COLECO, colecovision | col, rom, bin | — | Coleco - ColecoVision | derived | 48 | C |
| intellivision | Mattel Intellivision | INTELLIVISION, intv | int, bin, rom | — | Mattel - Intellivision | no-intro + metadat/homebrew/Mattel - Intellivision.dat | 115 | C |
| vectrex | GCE Vectrex | VECTREX, gcevectrex | vec, bin | gam | GCE - Vectrex | no-intro + metadat/homebrew/GCE - Vectrex.dat | 102 | C |
| odyssey2 | Magnavox Odyssey² / Videopac | ODYSSEY, odyssey2, videopac, o2em | bin | — | Magnavox - Odyssey2 | derived | 104 | C |
| channelf | Fairchild Channel F | FAIRCHILD, channelf | chf, bin | — | Fairchild - Channel F | derived | 80 | C |
| arcadia | Emerson Arcadia 2001 | ARCADIA, arcadia2001 | bin | — | Emerson - Arcadia 2001 | derived | 94 | C |
| supervision | Watara Supervision | SUPERVISION | sv, bin | — | Watara - Supervision | derived | 207 | H |
| sg1000 | Sega SG-1000 / SC-3000 | SEGASGONE, sg1000, sg-1000, sc3000 | sg, sc, bin | — | Sega - SG-1000 | derived | 109 | C |
| pico | Sega Pico | SEGAPICO, segapico | bin | md | Sega - PICO | derived | 250 | C |
| supergrafx | PC Engine SuperGrafx | SGFX, supergrafx | sgx | pce | NEC - PC Engine SuperGrafx | derived | 105 | C |
| pcfx | NEC PC-FX | PCFX, pc-fx | cue, bin, chd, ccd | — | NEC - PC-FX | derived (redump) | 72 | C, disc |
| cdi | Philips CD-i | CDI, cdimono1, cd-i | chd, cue, bin, iso | — | Philips - CD-i | derived (redump) | 133 | C, disc |
| amigacd32 | Commodore Amiga CD32 | CD32, amigacd32 | cue, bin, iso, chd | — | Commodore - CD32 | derived (redump) | 130 | C, disc |
| cdtv | Commodore CDTV | CDTV, amigacdtv | cue, bin, iso, chd | — | Commodore - CDTV | derived (redump) | 129 | C, disc |
| n64dd | Nintendo 64DD | N64DD, 64dd | ndd | — | Nintendo - Nintendo 64DD | derived | 122 | C |
| satellaview | Nintendo Satellaview | SATELLAVIEW, bsx, bs-x | bs | sfc, smc | Nintendo - Satellaview | derived | 107 | C |
| sufami | Bandai Sufami Turbo | SUFAMI, sufamiturbo | — | st, sfc, smc | Nintendo - Sufami Turbo | derived | 108 | C |
| gamecom | Tiger Game.com | GAMECOM | tgc, bin | — | Tiger - Game.com | derived | 121 | H |
| megaduck | Mega Duck / Cougar Boy | MEGADUCK, cougarboy | bin | — | — | — | 90 | H |
| scv | Epoch Super Cassette Vision | SCV, supercassettevision | bin, rom | 0 | Epoch - Super Cassette Vision | derived | 67 | C |
| zxspectrum | Sinclair ZX Spectrum | ZXS, zxspectrum, spectrum | tzx, z80, rzx, scl, trd, szx, tap, dsk, sna | — | Sinclair - ZX Spectrum | metadat/tosec/Sinclair - ZX Spectrum.dat, metadat/no-intro/Sinclair - ZX Spectrum +3.dat, dat/Sinclair - ZX Spectrum.dat | 76 | PC |
| zx81 | Sinclair ZX81 | ZX81 | 81 | p, tzx | Sinclair - ZX 81 | dat/Sinclair - ZX 81.dat | 77 | PC |
| amstradcpc | Amstrad CPC | CPC, amstradcpc | cdt, cpr, kcr, dsk, sna, m3u | — | Amstrad - CPC | metadat/tosec/Amstrad - CPC.dat, dat/Amstrad - CPC.dat | 65 | PC |
| x68000 | Sharp X68000 | X68000, x68k | xdf, dim, hdf, hdm, 2hd, m3u | — | Sharp - X68000 | metadat/no-intro/Sharp - X68000.dat, metadat/tosec/Sharp - X68000.dat | 79 | PC |
| x1 | Sharp X1 | X1 | dx1, 2hd, tap, d88 | 2d | Sharp - X1 | metadat/no-intro/Sharp - X1.dat, metadat/tosec/Sharp - X1.dat | 220 | PC |
| pc98 | NEC PC-98 | PC98, pc9800 | hdi, nhd, d88, fdi, hdm, m3u | — | NEC - PC-98 | metadat/tosec/NEC - PC-98.dat, metadat/redump/NEC - PC-98.dat | 208 | PC |
| pc88 | NEC PC-88 | PC88, pc8800, pc8000 | t88, cmt, d88 | — | NEC - PC-88 | metadat/tosec/NEC - PC-88.dat | 221 | PC |
| fmtowns | Fujitsu FM Towns | FMTOWNS, fm-towns | cue, bin, chd, iso, d88 | — | — | — | 253 | PC, disc |
| msx2 | MSX2 | MSX2 | rom, dsk, cas | mx2 | Microsoft - MSX2 | derived | 116 | PC |
| msxturbor | MSX turbo R | MSXTURBOR, turbor | rom, dsk | — | — | — | 118 | PC |
| apple2 | Apple II | APPLE2, appleii | woz, 2mg, nib, dsk | do, po | — | metadat/tosec/Apple - II.dat | 86 | PC |
| bbcmicro | Acorn BBC Micro | BBCMICRO, bbc | ssd, dsd, uef | — | — | — | 37 | PC |
| plus4 | Commodore Plus/4 | PLUS4, cplus4, commodoreplus4 | — | prg, d64, t64, tap, crt | Commodore - Plus-4 | derived | 99 | PC |
| pet | Commodore PET | PET, commodorepet | — | prg, d64, t64, tap | Commodore - PET | metadat/tosec/Commodore - PET.dat | — | PC |
| c128 | Commodore 128 | C128, commodore128 | — | prg, d64, d71, d81, t64, tap, crt | — | — | — | PC |
| atarixe | Atari XE / XEGS | ATARIXE, xegs | — | atr, xex, xfd, car, rom, bin | Atari - 8-bit | metadat/no-intro/Atari - 8-bit Family.dat | 43 | PC |
| ti99 | Texas Instruments TI-99/4A | TI99, ti994a | rpk, ctg, bin | — | — | — | 205 | PC |
| thomson | Thomson MO/TO | THOMSON, moto, to8, thomsonmoto, theodore | k7, m7, sap, rom | fd | Thomson - MOTO | metadat/tosec/Thomson - MOTO.dat | 141 | PC |
| dragon | Dragon 32/64 | DRAGON, dragon32, dragon64 | cas, dsk, ccc | — | — | — | 91 | PC |
| coco | Tandy TRS-80 Color Computer | COCO, trs80coco | cas, dsk, ccc | — | — | — | 144 | PC |
| tic80 | TIC-80 | TIC, tic80 | tic | — | TIC-80 | dat/TIC-80.dat | 222 | E |
| wasm4 | WASM-4 | WASM4 | — | wasm | WASM-4 | dat/WASM-4.dat | 262 | E |
| arduboy | Arduboy | ARDUBOY | arduboy | hex | Arduboy Inc - Arduboy | derived | 263 | H |
| uzebox | Uzebox | UZEBOX | uze | hex | — | dat/Uzebox.dat | 216 | C |
| lowresnx | LowRes NX | LOWRESNX | nx | — | LowRes NX | dat/LowRes NX.dat | 244 | E |
| vircon32 | Vircon32 | VIRCON32 | v32 | — | Vircon32 | dat/Vircon32.dat | 272 | E |
| doom | Doom (PrBoom) | PRBOOM, doom | wad | — | DOOM | dat/DOOM.dat | 135 | E |
| quake | Quake | QUAKE, tyrquake | — | pak | Quake | dat/Quake.dat | 135 | E |
| cavestory | Cave Story (NXEngine) | CAVESTORY, nxengine | — | — | Cave Story | dat/Cave Story.dat | — | E |
| wolf3d | Wolfenstein 3D (ECWolf) | ECWOLF, wolf3d, wolfenstein3d | — | — | Wolfenstein 3D | dat/Wolfenstein 3D.dat | — | E |
| mrboom | Mr. Boom | MRBOOM | — | — | MrBoom | dat/MrBoom.dat | — | E |
| dinothawr | Dinothawr | DINOTHAWR | — | game | Dinothawr | dat/Dinothawr.dat | — | E |
| lutro | Lutro | LUTRO | lutro | lua | Lutro | dat/Lutro.dat | 206 | E |
| solarus | Solarus | SOLARUS | solarus | — | — | — | 223 | E |
| flashback | Flashback (REminiscence) | REMINISCENCE, flashback | — | — | Flashback | dat/Flashback.dat | — | E |
| xrick | Rick Dangerous (XRick) | XRICK, rickdangerous | — | — | Rick Dangerous | dat/Rick Dangerous.dat | — | E |
| model2 | Sega Model 2 | MODEL2 | zip, 7z | — | MAME | metadat/mame/MAME.dat | 75 | A |
| model3 | Sega Model 3 | MODEL3 | zip, 7z | — | MAME | metadat/mame/MAME.dat | 75 | A |
| naomi2 | Sega NAOMI 2 | NAOMI2 | zip, 7z, chd | — | Sega - Naomi 2 | metadat/redump/Sega - Naomi 2.dat, metadat/mame/MAME.dat | — | A |
| triforce | Namco Sega Nintendo Triforce | TRIFORCE | — | iso | — | — | — | A |

Changes to existing entries: `msx` loses alias `msx2` (it keeps `msx1`, ES-DE's and Batocera's MSX folder); `pcengine` moves `sgx` and `snes` moves `bs` to their `weak_extensions` (D3); `nes` gains nothing (D17); all 54 gain `family`. `arcade = true` on the four arcade rows. Systems without any strong extension (`plus4`, `pet`, `c128`, `atarixe`, `quake`, ports without a launcher, `triforce`) are claimed by folder, header or hash only, which is the point.

The DAT paths and sizes were listed from the GitHub tree at `2c93952f`; the largest new one is `metadat/tosec/Sinclair - ZX Spectrum.dat` (17.9 MB, under `MAX_DAT_BYTES`). TOSEC and `dat/` files are clrmamepro text like the rest; a parse test on a trimmed copy of each new DAT family (`tosec`, `dat/`, `homebrew`) is part of §14.1.

## 8. Identification

### 8.1 Cartridge, tape and floppy headers (`identify/sniff.rs`)

| System | Rule | Weight | Source / status |
|---|---|---|---|
| pico | `SEGA PICO` at 0x100 (already checked; now a hit) | 0.95 | existing code |
| sufami | `BANDAI SFC-ADX` at 0x00 (and not `SFC-ADX BACKUP`, the BIOS) | 0.95 | Snes9x memmap; verify on a no-intro sample in task |
| vectrex | `g GCE` at 0x00 | 0.95 | Vectrex cartridge copyright header; verify |
| colecovision | `AA 55` or `55 AA` at 0x00 | 0.60 (16 bits: supports a folder/extension, never auto alone) | ColecoVision BIOS test bytes |
| zxspectrum | `ZXTape!\x1A` at 0x00 with `.tzx`; `.cdt` with the same magic → amstradcpc | 0.90 | TZX format spec |
| amstradcpc / zxspectrum | `MV - CPC` or `EXTENDED CPC DSK File` at 0x00 (`.dsk`): CPC **or** Spectrum +3 | 0.50 each (review) | CPCEMU DSK spec |
| amstradcpc | `MV - SNA` at 0x00 (`.sna`); a Spectrum `.sna` has no magic, so without it `.sna` stays a review | 0.90 | CPCEMU snapshot format |
| apple2 | `WOZ1`/`WOZ2` at 0x00; `2IMG` at 0x00 | 0.95 | WOZ and 2IMG specs |
| bbcmicro | `UEF File!` at 0x00 | 0.95 | UEF spec |
| pc98 | `T98HDDIMAGE.R0` at 0x00 (`.nhd`) | 0.95 | T98-Next HDD format |
| doom | `IWAD` / `PWAD` at 0x00 | 0.95 | Doom WAD format |
| quake | `PACK` at 0x00 with `.pak` **inside a quake folder only** (OpenBOR paks use the same magic) | folder evidence only | id PAK format |
| uzebox | `UZEBOX` at 0x00 | 0.95 | Uzebox `.uze` header; verify |
| vircon32 | `V32-CART` at 0x00 | 0.95 | Vircon32 cartridge spec; verify |
| jaguar | DRI `.abs` magic `60 1B`, COFF magic `01 50` | 0.50 (supports folder) | Jaguar SDK formats |
| ti99 / arduboy | zip with `layout.xml` (`.rpk`) / `info.json` (`.arduboy`) | 0.90 | read through the archive layer, no new reader |
| plus4 / c128 / pet | `.prg` load address `01 10` / `01 1C` / `01 04` (C64 `01 08`) | 0.40 (tie-break inside a shared folder only) | Commodore memory maps |
| amiga / x68000 (`.hdf`) | `RDSK` at 0 or `DOS` + flag byte at 0 → amiga; `X68SCSI1` at 0 → x68000 | 0.90 each | Amiga RDB; X68000 SCSI HD image; verify |
| amiga / pc98 (`.fdi`) | `Formatted Disk Image file` at 0 → amiga; else pc98 only by folder/hash | 0.90 | FDI 2.0 spec; verify |
| atarist / x68000 (`.dim`) | Atari ST FastCopy `42 42` at 0 → atarist | 0.85 | FastCopy DIM; verify |

The last three rows exist so the D3 class changes (`.hdf`, `.fdi`, `.dim` become Shared) do not send an installed Amiga or Atari ST user's loose disks to review when the bytes say which machine they are for. Anything marked «verify» is checked against a real dump or the emulator source during its task; a rule that cannot be verified is dropped (the system keeps folder + hash), and the drop is listed in the PR.

No header (folder + hash only): intellivision, odyssey2, channelf, arcadia, supervision, sg1000, supergrafx, n64dd, satellaview, gamecom, megaduck, scv, zx81, x68000, x1, pc88, msx2, msxturbor, atarixe, thomson, dragon, coco, tic80, wasm4 (the `\0asm` magic only proves WebAssembly), lowresnx.

### 8.2 Discs (`disc/`)

| System | Rule | Source |
|---|---|---|
| ps2 | Existing: `SYSTEM.CNF` with `BOOT2`, or a `PLAYSTATION` PVD larger than a CD, and the serial from the `BOOT2` path (`cdrom0:\SLUS_200.62;1` → `SLUS-20062`). New: the verdict is `Known("ps2")`. CSO: the existing CISO reader feeds the same check. ZSO: no reader (D14), extension and name only. | rcheevos `rc_hash_ps2` (`BOOT2`, `cdrom0:`) |
| 3do | Existing Opera volume header at sector 0 of track 1. | rcheevos `rc_hash_3do` |
| pcfx | `PC-FX:Hu_CD-ROM` at byte 0 of sector 0 of a data track; checked **before** the PC Engine CD text (some PC-FX discs also carry it). | rcheevos `rc_hash_pcfx_cd`; Beetle SuperGrafx `pce.cpp` |
| jaguarcd | `ATARI APPROVED DATA HEADER ATRI ` (or its byte-swapped form) in the first sector of the first data track of session 2; a CUE without `REM SESSION 02` cannot be checked → folder/name only. | rcheevos `rc_hash_jaguar_cd` |
| cdi | Green Book disc label at sector 16: record type `01`, `CD-I `, version `01`. | CD-i Full Functional Specification; verify on a Redump dump |
| amigacd32 / cdtv | ISO 9660 root holds `CD32.TM` / `CDTV.TM`. | UAE / FS-UAE detection; verify |
| fmtowns | No signature: folder and name. | — |

### 8.3 Hash and name

Every row with a DAT in §7 is hashed (files ≤ 64 MiB) and name-matched. `dat_sources` keeps its rule; explicit `dats` cover the TOSEC, homebrew and `dat/` files. `system_for_dat` picks the system with the fewest DAT sources and keeps database order on a tie: `mame`, `model2` and `model3` each list only `MAME.dat`, so the entries for `model2` and `model3` are placed **after** `mame` in `systems.toml` and `MAME.dat` keeps mapping to `mame` (D16). `naomi2` lists two DATs, so it never wins `MAME.dat`. The same tie applies to `atarixe`, whose only DAT is `a800`'s `Atari - 8-bit Family.dat`: `atarixe` is placed after `a800` and is **never identified by hash**, only by its folder (stated in the PR). A test pins all three. The batch adds 4 DAT-heavy computer systems (ZX Spectrum lists 3 DATs, 17.9 MB TOSEC): a mixed pile can exceed `MAX_DAT_DOWNLOADS = 4`; the existing «catálogos no consultados» report covers it, nothing new. Runtime downloads come from `master` (§3), so the pinned-tree test catches typos, not later renames upstream; a later 404 follows the existing TTL path.

## 9. UX and UI design

The visible change is small: more systems, grouped pickers, and icons. No new screen, no new token.

### 9.1 Design plan (frontend-design pass)

- **Colour**: existing semantic tokens only (`--cr-text`, `--cr-text-secondary`, `--cr-border`, `--cr-manila` for the Slot mount's label tape). No new hex values; `pnpm tokens:check` stays green.
- **Type**: Atkinson Hyperlegible Next for names and group labels (`type.app.label` for `<optgroup>` where the platform renders it; native otherwise); Atkinson Mono for the abbreviation inside the Slot mount (unchanged). No Archivo in this batch (nothing ≥ 24px changes).
- **Layout**: only the «Se quedan donde están» picker (`LeftInPlace`), which lists every system, changes. The review rows keep their short candidate list. The full list's options become:

  ```
  Elige un sistema…
  ── Consolas ──────────────
    3DO Interactive Multiplayer
    Atari Jaguar
    …
  ── Portátiles ────────────
  ── Ordenadores ───────────
  ── Recreativas ───────────
  ── Motores y consolas de fantasía
  ```

  Groups in a fixed order (the order a collector thinks in: home consoles, handhelds, computers, arcade, engines); names inside a group sorted by the user's locale. Left-aligned like the rest of the review list.
- **Motion**: none added.
- **Principle**: the system's identity is its name plus its silhouette; when no neutral silhouette exists the Slot mount with a short label is the brand's honest fallback, not a made-up drawing.

Review against generic defaults: the obvious default would be a searchable combobox with icons per option. Rejected: a native `<select>` is what the screen uses today, is keyboard- and screen-reader-native on all three webviews (WKWebView on macOS, WebView2 on Windows, WebKitGTK on Linux all render `<optgroup>` natively and VoiceOver/Narrator/Orca announce the group), type-ahead still jumps by name across groups, and grouping solves the length problem (118 options in 5 groups). The placeholder option stays outside any group and disabled; no option text relies on the icon (none is rendered inside `<option>`). A custom combobox is a separate item if the owner wants it (§13). Group labels are plain sentence-case words, no all-caps eyebrows, no counts in the label.

### 9.2 Icons

Silhouettes (RetroArch Monochrome file at the pinned commit), subject to the D20 audit on the contact sheet:

| id | file |
|---|---|
| ps2 | Sony - PlayStation 2 |
| 3do | The 3DO Company - 3DO |
| jaguar / jaguarcd | Atari - Jaguar / Atari - Jaguar CD |
| colecovision | Coleco - ColecoVision |
| intellivision | Mattel - Intellivision |
| vectrex | GCE - Vectrex |
| odyssey2 | Magnavox - Odyssey2 |
| channelf | Fairchild - Channel F |
| arcadia | Emerson - Arcadia 2001 |
| supervision | Watara - Supervision |
| sg1000 | Sega - SG-1000 |
| pico | Sega - PICO |
| supergrafx | NEC - PC Engine SuperGrafx |
| pcfx | NEC - PC-FX |
| cdi | Non-Redump - Philips - CD-i (the only CD-i file in the set) |
| amigacd32 / cdtv | Commodore - CD32 / Commodore - CDTV |
| n64dd | Nintendo - Nintendo 64DD |
| satellaview / sufami | Nintendo - Satellaview / Nintendo - Sufami Turbo |
| gamecom | Tiger - Game.com |
| megaduck | Welback - Mega Duck |
| scv | Epoch - Super Cassette Vision |
| zxspectrum / zx81 | Sinclair - ZX Spectrum / Sinclair - ZX 81 |
| amstradcpc | Amstrad - CPC |
| x68000 / x1 | Sharp - X68000 / Sharp - X1 |
| pc98 / pc88 | NEC - PC-98 / NEC - PC-88 |
| fmtowns | Fujitsu - FM Towns |
| msx2 | Microsoft - MSX2 |
| apple2 | Apple - II |
| bbcmicro | Acorn - BBC Micro |
| plus4 / pet | Commodore - Plus-4 / Commodore - PET |
| atarixe | Atari - XEGS |
| ti99 | Texas Instruments - TI-99-4A |
| thomson | Thomson - MOTO |
| arduboy, uzebox, tic80, doom, quake, cavestory, mrboom, dinothawr, lutro, flashback, xrick | candidate files of the same name; kept only if the audit finds a device or neutral pictogram, else `WORDMARK_IDS` |

Slot-mount fallbacks (`FALLBACK_ABBR`, ≤ 4 chars): `msxturbor TR`, `c128 C128`, `dragon DRGN`, `coco COCO`, `wasm4 W4`, `lowresnx LRNX`, `vircon32 V32`, `wolf3d W3D`, `solarus SOL`, `model2 MDL2`, `model3 MDL3`, `naomi2 NAO2`, `triforce TRI`, plus any audited-out logo above (`doom DOOM`, `quake QK`, `cavestory CAVE`, `xrick RICK`, `flashback FLBK`, `mrboom BOOM`, `dinothawr DINO`, `lutro LUTR`, `tic80 TIC`, `uzebox UZE`, `arduboy ARDU`).

`THIRD-PARTY-NOTICES.md`: the count and the id list change; the licence paragraph, commit and changes-made text stay. `src/assets/system-icons/README.md` (generated) records each file's audit.

### 9.3 Mock

`?mockPick=retro` (beside `downloads|loose|mixed`): a pile with `Gran Turismo 4 (Europe).iso` (PS2, `BOOT2` serial `SCES-51719`), `Tempest 2000 (World).j64`, `Manic Miner (1983)(Bug-Byte).tzx`, `Arkanoid (1987).dsk` (CPC magic → review: Amstrad CPC or ZX Spectrum), `Sailor Moon (Japan).st` with the Sufami magic, `Prince of Persia.st` without it (Atari ST), `DOOM.WAD` loose (`IWAD` → doom), `Freedoom Phase 1/` holding `freedoom1.wad` (`IWAD`) and `README.txt` (one item left, `folderGame`), `Cave Story/` with `Doukutsu.exe` and `data/` (no rule: its files are left `unknown`, D15), `sfiii.zip` beside `sfiii/` holding a `.chd` (one item with its CHD companion), `game.bin` (generic → left, unknown). Shelves fixture gains `PS2`, `ZXS`, `WASM4` so a silhouette and a Slot mount render side by side.

## 10. Copy (es / en)

Spanish: sentence case, no exclamation marks, exact counts, numbers set in mono by the renderer.

| Key | es | en |
|---|---|---|
| `systems.family.console` | Consolas | Consoles |
| `systems.family.handheld` | Portátiles | Handhelds |
| `systems.family.computer` | Ordenadores | Computers |
| `systems.family.arcade` | Recreativas | Arcade |
| `systems.family.engine` | Motores y consolas de fantasía | Engines and fantasy consoles |
| `organize.why.extensionWeak` | Extensión .{ext}: cuenta para {system} solo dentro de su carpeta o con su cabecera | .{ext} counts for {system} only inside its folder or with its header |
| `organize.left.folderGame` | Juego de {system} en su propia carpeta: muévela entera a {folder} | {system} game in its own folder: move the whole folder to {folder} |

`<optgroup label>` uses the `systems.family.*` keys; nothing else is needed. Existing keys reused unchanged: `organize.noFolderOnDevice` («{layout} no tiene carpeta para {system}; se crea {folder}.»), `organize.why.disc`, `organize.why.header`, `organize.why.hash`, `organize.why.extensionShared`, `organize.why.extensionContradicted` (D24). `organize.left.unsupportedSystem` stays for future systems; PS2, 3DO and Pico no longer produce it. `{folder}` in `folderGame` is the folder name in mono like other paths. Other workflows edit `locales/*.json` in parallel: add keys in their sorted place and resolve merge conflicts key by key, never by taking one side. The parity test covers the new keys.

## 11. Data flow

1. **Load**: `SystemDb` parses `family`, `weak_extensions` and `scoped_aliases`; validation of the built-in file rejects a weak extension that is also strong, an unknown family, and an alias that two systems claim; a user systems file is merged leniently (§5.1) and never fails on the new fields.
2. **Scan** (`scan_library_with_progress(root, profile, …)`): each top folder → `resolve_folder_in(Some(profile), name)` → system; files filtered by `systems_accepting` (strong + weak) for that system.
3. **Organise walk**: folders that resolve (profile-scoped, scoped aliases included) are skipped as already sorted; a pile folder directly holding a proven folder-game file (D15) → one item, `LeftReason::FolderGame`.
4. **Evidence**: sniff hits (§8) + extension evidence (strong only, contradicted per D24) + DAT hash/serial + name. Weak extensions add evidence only when a folder or header already names the system. Zip/gzip packages (D25) are raw items.
5. **Target**: `system_folder(profile, system, db, overrides)` → override, `Named`, host (incl. D23), rule; `Absent` → note. Disc systems follow the profile's `disc_layout`; `ps2` never takes an archive as-is (`multi_file`).
6. **Covers**: libretro playlist name (§7) → thumbnails; absent → ScreenScraper when `screenscraper` is set and the user enabled it; otherwise none. Where they are written follows the profile template unchanged: on ES-DE and muOS that is `{system_id}` (§13 owner ask).
7. **UI**: `listSystems()` → `SystemInfo { id, name, family }` → grouped options; plan items show `SystemIcon`.

## 12. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| `.st` file without the Sufami magic | atarist, as today (unique strong extension). |
| `.st` with the magic, outside any folder | sufami at 0.95 header → auto. |
| `.md` with `SEGA PICO` | pico (was `Unsupported`). Without it, megadrive as today. |
| `.pce` that is a SuperGrafx game | pcengine by extension; a hit in the SuperGrafx DAT moves it to supergrafx (HuCards are far below the hash cap, so the hash always runs). An unknown dump stays pcengine, which still boots most of the few SuperGrafx titles in SuperGrafx-capable cores. |
| `.dsk` loose | Shared → review. With the CPCEMU magic the likely list is Amstrad CPC, then ZX Spectrum (+3); without it every `.dsk` owner is listed. File size is never used as proof. |
| `.bin`, `.rom` loose | Generic → nothing unless header or hash. |
| Folder `pico` on a Batocera card | Sega Pico; on Anbernic/Onion/muOS or with no profile: PICO-8. |
| Folder `msx2` on an existing library | Now MSX2 (was MSX). Covers come from `Microsoft - MSX2`. A user override for `msx` naming `msx2` fails validation and is reported in settings (existing behaviour for invalid overrides). |
| User override naming an `Absent` profile folder | Allowed if it resolves back (existing rule); the note disappears because the user chose the name. |
| PS2 ISO over 4 GiB on FAT32 | Left, `TooBigForFat` (existing). |
| PS2 `.zso` | No reader: extension `zso` (strong, unique) says ps2, name-only for the DAT. |
| PS2 `.cso` | The CISO reader reaches the PVD: `BOOT2` → ps2 and psp's extension is contradicted (D24); otherwise psp as today. |
| Jaguar CD single-session CUE | Folder or name only; never auto by `.cdi`. |
| DAT missing on the server (404) | Remembered for the TTL (existing); identification falls back to header/name. A new DAT path that 404s at review time is a bug: test (§14.1) asserts every path exists in the pinned tree listing committed as a fixture. |
| Two systems with the same libretro folder (`model2`, `model3`, `mame`, `pgm2` → `MAME`) | Allowed today (pgm2); cover lookup works for each. |
| A custom profile cloned before this batch | Its `folder_names` lacks the new systems; behaves like today (rule-based names, e.g. `jaguar` instead of ES-DE's `atarijaguar`). Re-cloning picks the tables up. Listed in the release notes. |
| A user systems file that already defines a new id (`tic80`, `ps2`…) | Merged by id as today: the user's aliases win and their extensions are appended; a user strong extension removes the built-in weak one (§5.1). A user file defining the same machine under another id (`spectrum` with alias `ZXS`) keeps that alias; the built-in `zxspectrum` loses it, as today's alias rule says. |
| A remembered pick (`organize_choices`) for `msx`, `pcengine`, `snes` on a file now identified as `msx2`, `supergrafx`, `satellaview` | The user's pick wins (D27); nothing is silently re-filed. |
| An existing `MSX2/` folder on RetroArch, ES-DE or muOS | Now `msx2`: art paths with `{libretro}`/`{system_id}` move from `Microsoft - MSX`/`msx` to `Microsoft - MSX2`/`msx2`, so the art-gap count rises once and a fill re-downloads covers there. Old files are not deleted. Dedupe no longer pairs an MSX2 copy with an MSX copy. Release notes say so. |
| Loose `.hdf`, `.fdi`, `.dim` (now Shared, D3) | Amiga / Atari ST bytes keep them automatic through the §8.1 rows; without a recognised header they go to review (previously automatic). Release notes say so. |
| Folder named `Doom`, `Dragon`, `PET` in a pile | Resolves only with a profile (D22): with the target profile it is «already sorted» like any system folder; profile detection never counts it. |
| `.arduboy`, `.rpk`, `.solarus`, `.lutro`, gzip `.uef` | Raw items, moved as-is, never extracted (D25). |
| `.hex`, `.nx`, `.lua`, `.game` | Text-native (D26): never «No es una ROM (texto)». Loose `.hex`/`.lua` stay unknown (weak only). |
| Older Coverly reads a plan, job or profile written by this one | Profile files: the new data is in existing fields, nothing breaks. Plans and job records holding `FolderGame` or a new system id are only read by the version that wrote them (downgrade is unsupported, as for earlier `LeftReason` additions). |
| Facts memoised by the previous version | `FACTS_VERSION` 6 (D27): read again lazily. |
| An old settings file | No new keys; nothing to migrate. |
| Windows paths | Fixture paths compared by component; fixture mtimes pinned. No new folder name is a Windows reserved name (`CON`, `AUX`, `NUL`, `COM1`…) or ends in a dot or space; a test asserts it for every `Named` folder and first alias. `sg-1000`, `fm-towns`, `pc-fx` are valid on FAT32, exFAT, NTFS, APFS and ext4. |
| Case-only differences (`PICO` on Anbernic vs `pico` on Batocera) | Resolution is case-insensitive as today; on a case-sensitive Linux card the folder Coverly creates uses the table's exact spelling. |
| Extensions with digits only (`.0`, `.81`) | `Path::extension` returns them on all three OSes; `.0` is weak (D4), so a `foo.0` log file in Downloads is never claimed. |

## 13. Blocked, deferred and out of scope

Needs an owner decision (implemented around; listed in the PR):
- **Taito Type X**: Windows executables per game, no DAT, no libretro thumbnails, no ScreenScraper id found; only ES-DE has a `type-x` folder. Catalogue-only like Tanda 3? Owner to decide.
- **Cave CV1000 (`cave3rd`)**: a Batocera-only folder; MAME sets today. New system or stay in `mame`?
- **ScreenScraper ids for C128, PET, NAOMI 2 and Triforce**: not found in ES-DE's or Skyscraper's maps; `systemesListe.php` needs developer credentials (`docs/02-research/screenscraper-api.md`). Left absent until checked with credentials. `doom` and `quake` use 135 (PC DOS), as ES-DE does; owner may prefer none.
- **ES-DE media folder name.** ES-DE reads `downloaded_media/<ES-DE system name>/`, but the built-in template writes `downloaded_media/{system_id}/` (§3). For this batch's systems whose ES-DE folder differs from the id (`jaguar`→`atarijaguar`, `jaguarcd`→`atarijaguarcd`, `cdi`→`cdimono1`, `sg1000`→`sg-1000`, `thomson`→`moto`, `dragon`→`dragon32`) ES-DE will **not** show the covers Coverly writes; the same is already true today for existing systems such as `a2600` (`atari2600`). Fix proposed: a `{system_folder}` template token (the ROM folder's name) used by the ES-DE template. It moves where art is written for installed ES-DE users whose folder ≠ id (art-gap counts change once), so it is **not** done in this batch without the owner's yes. Same question for muOS `catalogue/{system_id}` (below).
- **Behaviour changes for installed users** (implemented, to be confirmed): D3's `.hdf`/`.fdi`/`.dim` class change (mitigated by §8.1 headers), `msx2` leaving `msx` (§12), D23's hosts keeping `.sgx`/`.bs`/MSX2/XE on the parent folder on Anbernic stock and MinUI. The owner may prefer the new folders with the «no tiene carpeta» note instead of D23.
- **Onion extension lists** that omit a §7 extension (`SUFAMI` has no `.st`, §6.1 notes): accept that the device hides those files, or exclude Onion from those systems? Default: move them, list them in the PR.
- **Refreshing old custom profiles** with the new folder tables (an «actualizar carpetas» action in the profile editor): not built; owner to decide.

Observed, pre-existing, not changed here:
- The muOS cover path uses `{system_id}` as the catalogue folder, while muOS catalogues are named like «Atari Jaguar» (manifest `catalogue`). New systems follow the existing rule for consistency; fixing the catalogue name is its own item.
- MinUI/NextUI folders are «Display Name (TAG)»; the profile writes the bare tag. Unchanged.

Out of scope: Videopac+ (G7400), Amstrad GX4000, Knulli as its own profile, a searchable system combobox, a ZSO reader, Tanda 2 and 3.

## 14. Test plan

### 14.1 Rust (`cargo test --workspace`)

- `systems`: every new entry loads; families valid; weak/strong disjoint; global and scoped aliases unique; **extension-class snapshot** (`insta`) for every pre-batch extension, whose diff from `main` is exactly D3's list; `.sgx` → supergrafx; `msx2` alias → msx2; `msx1` still → msx; no scoped alias resolves through profile-less `resolve_folder` (D22); a user systems file that lists a built-in weak extension as strong, or an unknown `family`, loads (§5.1); no `Named` folder or first alias is a Windows reserved name.
- `profiles`: for every built-in × every Tanda 1 system, `system_folder` returns the §6.1 name or the rule name with `on_device = Some(false)`; every `Named` folder resolves back through `resolve_folder_in(Some(profile))`; `pico` resolves to pico8 without a profile and to Sega Pico on batocera; cloning a built-in fills `folder_names` with its `Named` entries and the file round-trips; a custom Batocera clone's `folder_names.pico = "pico"` validates (scoped) instead of failing as «folder of PICO-8»; D23 hosts give `PCE`/`SFC`/`MSX`/`A800` on anbernic-stock; `default_container_overrides()` is unchanged (D13).
- `sniff` (`tests/sniff.rs`): one positive and one negative fixture per §8.1 rule (synthetic byte heads, no copyrighted data); Pico and PS2 become hits (the existing assertions on `unsupported = "PlayStation 2"`/`"Sega Pico"` in `tests/sniff.rs`, `tests/disc_identify.rs` and `tests/organize_inspect.rs` are rewritten, not deleted); a `.hex` and an `.nx` text file are not `NotARom` (D26); a `.wad` without `IWAD`/`PWAD` scores 0.60 (D15).
- `disc_identify`: PS2 head (existing `disc/testing.rs` builder) → `Known("ps2")` with serial; 3DO → `Known("3do")`; PC-FX before PC Engine CD; CD-i, CD32, CDTV, Jaguar CD builders.
- `dat`: parse trimmed fixtures of a TOSEC, a `dat/` and a homebrew DAT; a committed listing of libretro-database paths at `2c93952f` contains every `dats` path and derived path.
- `organize_target`: per family, a ROM lands in the right folder on esde, batocera, muos, onion and minui, with the `NoFolderOnDevice` note where `Absent`; ps2 archive is extracted, never moved as-is.
- `organize_plan` snapshots: new `organize_plan__systems_batch_1__{esde,batocera,onion}.snap` over the §9.3 pile (built under `target/` with `cp -RXp`, pinned mtimes, removed afterwards).
- `organize_walk`: `Freedoom Phase 1/` → one `FolderGame` item; `Cave Story/` → its files `unknown`; a `Dragon/` folder in a pile is «already sorted» with a profile and not counted by `detect` (D22); `sfiii.zip` + `sfiii/*.chd` move together.
- `organize_evidence`: `.md` + `SEGA PICO` → pico Auto with megadrive contradicted (D24); `.st` + Sufami magic → sufami Auto; `.hdf` with `RDSK` → amiga Auto; `.arduboy` (zip) is one raw item, never extracted (D25).
- `organize_plan` facts: a row stored with `FACTS_VERSION` 5 holding `Unsupported { "PlayStation 2" }` is re-read and becomes `ps2` (D27); a remembered `msx` pick for an MSX2 DAT hit stays `msx`.
- `dedupe_units`: folders named by a folder-game alias are out of scope like `PORTS/`.
- Existing snapshots that change (`list_systems` in `src-tauri/src/lib.rs`, any plan snapshot whose candidates include `.dsk`, `.sgx`, `.bs`, `.hdf`, `.fdi`, `.dim`): each diff is reviewed and explained in the commit, never accepted wholesale.
- `libretro` (cover lookup): for every system with `libretro`, the thumbnail URL is built from the exact §7 name (committed copy of the live root listing; a test asserts each name is in it).

### 14.2 Vitest (`pnpm check-all`)

- `systemOptionGroups`: fixed group order, locale sorting (es: «Ámbar» before «Bravo»), empty groups dropped.
- `LeftInPlace`: options inside `<optgroup label="Consolas">` etc., placeholder outside any group; `ReviewBuckets` unchanged (flat candidates, «Dejar» last).
- `SystemIcon`: a new silhouette (`ps2`) renders an `svg`; `wasm4` renders the mount with `W4`; `systemIcons.test.ts` passes on the regenerated module.
- `organize.ts` copy for `folder-game` and `extension-weak`; i18n parity; Spanish copy check (sentence case, no «!»).
- Contract test with `family`.

### 14.3 E2E (Chrome DevTools MCP, isolated context `systems-batch-1`, dev server on port 1465)

1. `?mockPick=retro` → organise offer → plan: PS2 group with its silhouette, «Disco de Sony PlayStation 2 … (SCES-51719)» in the why line; ZX Spectrum and Jaguar groups; `Sailor Moon` under Sufami Turbo, `Prince of Persia` under Atari ST.
2. Review: `Arkanoid (1987).dsk` → its candidate select lists Amstrad CPC and ZX Spectrum first; keyboard-only: Tab to the select, arrows to «Amstrad CPC», apply.
3. Left in place: `Freedoom Phase 1` with the folder-game line; `Cave Story` files and `game.bin` unknown; the «elige un sistema» picker there shows the five groups; keyboard-only: Tab, type «zx», the ZX Spectrum option in «Ordenadores» is selected; the accessibility tree (`take_snapshot`) shows the group labels.
4. Switch profile to Onion: the PS2 item shows «Onion OS no tiene carpeta para Sony PlayStation 2; se crea PS2.»
5. Library shelves: `PS2` silhouette and `WASM4` mount, light and dark (`data-theme`), with `prefers-reduced-motion: reduce` emulated (nothing new animates), at 1280 px and the narrowest supported window; screenshots attached to the PR.
Fallback: Playwright MCP, then `playwright-core` + Chrome for Testing in the scratchpad.

### 14.4 Gates

`pnpm check-all`, `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`.

## 15. Tasks (one reviewable commit series each)

1. Core model: `family`, `weak_extensions`, `scoped_aliases`, lenient user-file merge, `Profile.folders`, `resolve_folder_in` at every caller of §3, custom-profile `folder_names` on clone, D24 contradiction, D25 packages, D26 text-native, `FACTS_VERSION` 6, extension-class snapshot (no new systems yet).
2. Disc family: ps2, 3do, pcfx, cdi, amigacd32, cdtv, jaguarcd, fmtowns (`zso` in `NAME_ONLY_EXTS`; no `NO_ARCHIVE_SYSTEMS` change).
3. Early consoles: colecovision, intellivision, vectrex, odyssey2, channelf, arcadia, scv, jaguar.
4. Sega / NEC / Nintendo add-ons: sg1000, pico, supergrafx, n64dd, satellaview, sufami; Famicom check (D17).
5. Handhelds: supervision, gamecom, megaduck.
6. Computers I: zxspectrum, zx81, amstradcpc, bbcmicro, thomson, dragon, coco, ti99.
7. Computers II: x68000, x1, pc98, pc88.
8. Computers III: plus4, pet, c128, atarixe, msx2, msxturbor, apple2; D23 hosts; `.hdf`/`.fdi`/`.dim` headers (§8.1) land with the D3 class change, in the same commit.
9. Fantasy consoles: tic80, wasm4, arduboy, uzebox, lowresnx, vircon32.
10. Ports and engines + folder-game rule.
11. Extra arcade + MAME CHD companions.
12. Icons: sources, audit, regenerate, notices.
13. UI: `SystemInfo.family`, grouped pickers, copy, mock pile.
14. E2E pass, PR with the blocked list (§13).

Each system task adds its §7 rows, its §6.1 table rows, its §8 rules, its Onion `extlist` check (§6.1 notes) and its tests together, and is pushed right after its commit. Other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel: merge `origin/main` before tasks 12–14 and resolve conflicts by hand.

## 16. Sources (fetched 2026-10-31)

| What | Source | Pinned at |
|---|---|---|
| libretro-thumbnails folders | <https://thumbnails.libretro.com/> live root listing (127 folders) | 2026-10-31 |
| libretro-database DAT paths and sizes | <https://github.com/libretro/libretro-database> git tree | `2c93952f3cdf02837d2b34112aea5ee844d42b8b` |
| ES-DE folders, extensions, platform ids | <https://gitlab.com/es-de/emulationstation-de/-/blob/master/resources/systems/linux/es_systems.xml>, `es-app/src/scrapers/ScreenScraper.cpp` (ScreenScraper id map) | `ef267ae693caeee89ca8ca888a38e9eab9bce90a` |
| Batocera folders | <https://github.com/batocera-linux/batocera.linux/blob/master/package/batocera/emulationstation/batocera-es-system/es_systems.yml> | `a3d49969295830ff1f136d393e33839f858fa8d7` |
| Knulli folders | <https://github.com/knulli-cfw/distribution/blob/knulli-main/package/batocera/emulationstation/batocera-es-system/es_systems.yml> | `ab3a6809286b3cff1648fb8c96a0200bc869beb6` |
| muOS folder names | <https://github.com/MustardOS/internal/tree/main/share/info/manifest/core> (`friendly` lists, 113 manifests), `share/info/name/folder.json` | `0784148faeeeb43703922958a03bcd45905b9639` |
| Onion folders and extensions | <https://github.com/OnionUI/Onion/tree/main/static/packages/Emu> (`*/Emu/*/config.json`, 54 packages) | `07505ea58c7bba698d6b9220ff43946a43cac76b` |
| MinUI / NextUI paks | <https://github.com/shauninman/MinUI>, <https://github.com/LoveRetro/NextUI> (`*.pak`) | `dbf89435…`, `5291344b…` |
| ScreenScraper ids | ES-DE map above; Skyscraper <https://github.com/Gemba/skyscraper/blob/master/platforms_idmap.csv> (n64dd 122) | `c253bfe0ea359aa4272f47694bd601bd53205d2b` |
| Disc signatures | rcheevos `src/rhash/hash_disc.c` (vendored in RetroArch `deps/rcheevos`) | RetroArch `023bae90d7ca75874c49f1052c59751c8d3d7692` |
| Icons | <https://github.com/libretro/retroarch-assets/tree/d9f969054dc7fbb6fa89519036d2b971e0855b51/src/xmb/monochrome> | `d9f96905…` (= master) |
| Anbernic stock folders | `ANBERNIC_FOLDERS` in `builtin.rs` (owner's card) | repo |

## 17. Self-review

- Placeholders: none; every «verify» in §8 has a stated fallback (drop the rule, keep folder + hash) and is reported in the PR.
- Contradictions checked: D3 vs §7 (every weak extension is a strong extension of an existing or a sibling system, or a D4 short extension); D5 vs D11 (Anbernic `Absent` and `known_folders` both yield `on_device = Some(false)`; same note); D6 vs §6.1 (`pico` is the only scoped collision; `msx2` is moved, not scoped); D13 vs §6.2 (no per-system disc layout); D16 vs §7 (model2/model3 list `MAME.dat` but loose sets stay in `mame`: §8.3 explains the tie order).
- Counts: 54 existing + 64 new = 118 systems (§7 and §6.1 both have 64 rows); `FALLBACK_ABBR` entries ≤ 4 characters.
- Brief rules: no folder guessed (each `Named` cell traces to §16); ambiguous extensions never auto-assigned by extension; ports follow ScummVM; blocked items listed (§13) without blocking the batch.

## 18. Adversarial review (2026-10-31)

Checked against `TODO.md` §2 and the code at `b984478`. Re-verified live: every libretro-thumbnails name of §7 is in the 127-folder root listing; every DAT path of §7 exists at `2c93952f` with the stated sizes; every icon file of §9.2 exists at `d9f96905` (no `WASM-4`, `LowRes NX`, `Vircon32`, `Solarus`, `Wolfenstein 3D`, `Commodore - 128`, `Sega - Naomi 2`, matching the Slot-mount list); ES-DE and Batocera folder names of §6.1 match the pinned `es_systems` files. Fixed in this pass:

| Finding | Where fixed |
|---|---|
| `NO_ARCHIVE_SYSTEMS` is in `profiles/mod.rs`, and `multi_file` already blocks archives as-is; adding `ps2` would only change the custom-profile serde default. | §3, D13, §5.1, §15 |
| Custom profiles already have `folder_names`; a second `folders` field duplicated it. | §3, D5, §5.1 |
| Six `resolve_folder` callers were missing (naming, custom preview, fit, detect, quarantine, probe); `detect`/`fit` have no profile. | §3, D6, §5.1 |
| The walk skips every folder that resolves: aliases like `doom`, `dragon`, `pet`, `x1` would hide Downloads folders and flip profile detection. | D22, §12 |
| No «ScummVM never split» rule exists; the `Cave Story/` mock had no rule that could make it `FolderGame`. | §3, D15, §9.3, §14 |
| `.wad` unique and not `DECISIVE` → any `.wad` Auto to doom. | D15, §5.1 |
| Weak-owner header wins only by `EPS` over the strong extension (0.95 vs 0.85, `MARGIN` 0.10). | D24 |
| Magic-first containers would extract `.arduboy`/`.rpk`/`.solarus`/`.lutro` and decompress `.uef`. | D25 |
| `.hex`/`.nx`/`.lua`/`.game` are text → `NotARom`. | D26 |
| Memoised facts (`FACTS_VERSION` 5) keep PS2/3DO/Pico `Unsupported` after upgrade. | D27 |
| ZSO has no reader (LZ4 CISO); «decompressed first blocks» was a new format. | D14, §8.2, §12 |
| PS2 serial from `BOOT2` already exists. | §3, D14, §8.2 |
| `.sgx`/`.bs` would move from `PCE`/`SFC` to folders Anbernic stock does not show. | D23, D11 |
| `.hdf`/`.fdi`/`.dim` becoming Shared sends Amiga/ST disks to review. | §8.1, §12, §13 |
| `atarixe` shares `a800`'s only DAT: never wins a hash. | §8.3 |
| `ReviewBuckets` lists candidates, not every system; only `LeftInPlace` needs groups. | §3, §5.2, §9.1, §14 |
| User systems files could fail to load on the new validation. | §5.1, §11, §12, §14 |
| ES-DE writes media under the Coverly id, not the ES-DE system name: six new systems' covers would not show. | §13 owner ask |
| Onion `SUFAMI` lists `.smc` only. | §6.1, §13 |
| Locale key was `organize.unsupportedSystem`; real key is `organize.left.unsupportedSystem`. | §10 |
| Windows reserved names, case-sensitive cards, digit-only extensions, downgrade, MSX2 art relocation not covered. | §12, §14 |
| a11y: `<optgroup>` support per webview, type-ahead, placeholder; reduced motion and dark mode in E2E. | §9.1, §14.3 |
