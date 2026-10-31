# Coverly ROMs — Missing systems, batch 1: retro without new formats (Design Spec)

> Status: **Draft, self-reviewed (§14)** · Date: 2026-10-26 (research run 2026-10-31) · Owner: Miguel Angel Esparza Calero (MAECLY)
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
| Header sniff | `identify/sniff.rs` | Discs: Saturn, Dreamcast, PSP, PlayStation (`BOOT2` → `unsupported = "PlayStation 2"`, a `PLAYSTATION` volume over `CD_MAX` → same). Carts: GB/GBC/GBA/N64/NDS/Lynx/7800/NES/SNES/Mega Drive family; `SEGA PICO` at 0x100 → `unsupported = "Sega Pico"`. |
| Disc layer | `disc/mod.rs`, `disc/signatures.rs`, `disc/iso9660.rs` | Opera header (3DO) → `DiscSystem::Unsupported("3DO")`; PS2 → `Unsupported("PlayStation 2")`. Sheets (`cue`, `gdi`, `ccd`, `m3u`), CHD, CSO, PBP readers. |
| Hashing | `identify/hash.rs` | Files over `DEFAULT_HASH_MAX_BYTES` (64 MiB) are not hashed; `cue gdi m3u ccd chd 7z pbp cso rvz` are identified by name only. DATs are clrmamepro text, ≤ `MAX_DAT_BYTES` (64 MiB), serials parsed (`identify/dat.rs`). |
| Profiles | `profiles/builtin.rs`, `profiles/naming.rs` | 8 built-ins: `anbernic-stock`, `garlicos`, `onion` (Imgs family, `FolderNaming::FirstAlias`), `muos`, `minui` (FirstAlias), `esde`, `batocera` (Batocera/Knulli/ES), `retroarch` (`FolderNaming::Id`). Only Anbernic has `known_folders` (→ `ItemNote::NoFolderOnDevice`). `host_folders` maps a system into another's folder. `disc_layout` and `container_overrides` are per profile. No per-system folder table exists; ES-DE and Batocera names that differ from the id cannot be expressed. |
| Folder resolution | `resolve_folder` callers: `scan/mod.rs:248` (has the profile), `organize/walk.rs:161`, `organize/plan.rs:2148, 2695`, `settings/validate.rs`, CLI `dedupe.rs:206` | Global only. |
| Folder games | `dedupe/units.rs` `OUT_OF_SCOPE_DIRS` (`ports bios apps saves states`); ScummVM | ScummVM is claimed by its folder alias and by its launcher extensions (`.scummvm`, `.svm`); organise never splits a game folder, dedupe skips `PORTS/`. That is the «ScummVM-style rule» this batch reuses. |
| Icons | `scripts/icons/systemIconsLib.mjs` (`COMMIT d9f96905…`, `SYSTEM_ICON_SOURCES`), `pnpm system-icons`, `src/components/system/SystemIcon.tsx`, `systemFallbacks.ts` (`FALLBACK_ABBR`, `WORDMARK_IDS`), `THIRD-PARTY-NOTICES.md` (41 files listed) | Silhouette when mapped, otherwise the Slot mount with an abbreviation (≤ 4 chars). |
| UI | `LeftInPlace.tsx`, `ReviewBuckets.tsx` (`NativeSelect` with every system), `SystemInfo { id, name }` | A flat list of 54 names today; 118 after this batch. |

## 4. Decisions

Open questions and the default chosen. Each can be overturned by the owner without changing the rest of the design.

| # | Question | Decision | Why |
|---|---|---|---|
| D1 | Scope of «Tanda 1» | Every bullet of Tanda 1 except the ones in §12 (blocked or deliberately not a system). Tanda 2/3 untouched. | Brief. |
| D2 | Ids | **The owner's ids where given.** Ports (no id given): `doom`, `quake`, `cavestory`, `wolf3d`, `mrboom`, `dinothawr`, `lutro`, `solarus`, `flashback`, `xrick`. Arcade: `model2`, `model3`, `naomi2`, `triforce`. Computers split by the owner's «si los perfiles los separan»: `msx2`, `msxturbor`, `atarixe` (ES-DE and Batocera both separate them, §6). | Ids are stable keys for overrides and settings; frontend spellings go in the per-profile tables (D5), so the id never has to bend to one frontend. |
| D3 | Extensions an existing system already owns (`.st` atarist, `.md` megadrive, `.pce`/`.sgx` pcengine, `.cdi` dreamcast, `.cso` psp, `.pak` openbor, `.atr/.xex/.xfd/.car` a800, `.mx2` msx, `.d64/.prg…` c64) | New field **`weak_extensions`** in `systems.toml`: accepted inside the system's folder and as a candidate once a header or a DAT hash names the system, but **never counted by `extension_class`**. The listed existing owner keeps its class. Deliberate class changes (Unique → Shared because the ambiguity is real) are only: `.dsk` (msx → msx, zxspectrum, amstradcpc, apple2, dragon, coco, msx2, msxturbor), `.dim` (atarist, x68000), `.hdf` (amiga, x68000), `.fdi` (amiga, pc98), `.sgx` moves from `pcengine` to `supergrafx` and `.bs` from `snes` to `satellaview` (each is that system's own format; the Anbernic `PCE` and `SFC` folders still accept them through `weak_extensions` of `pcengine` and `snes`). | A Sufami cart has a 14-byte magic, a Pico cart `SEGA PICO`: their header is the proof, not the extension. Turning `.st` Shared would send every Atari ST disk to review. The four real collisions follow the brief («never by extension alone»). A snapshot test (§14.1) pins every pre-batch extension's class with exactly this diff. |
| D4 | Extensions that say little even when unique (`.wasm`, `.hex`, `.p`, `.0`, `.fd`, `.do`, `.po`, `.2d`, `.abs`, `.cof`, `.gam`, `.lua`, `.game`) | Listed as **`weak_extensions`** of their system too. | `.wasm`, `.hex`, `.lua` are everyday developer files; `.p`, `.0`, `.fd`, `.do`, `.po` are too short to mean anything. Inside `ZX81/` a `.p` file is a ZX81 tape; on the desktop it is not. |
| D5 | Where per-profile folder names live | New `Profile.folders: BTreeMap<String, ProfileFolder>` with `ProfileFolder::Named(String)` or `ProfileFolder::Absent`, filled in `builtin.rs` for **every Tanda 1 system on every built-in profile** (except `retroarch`, which keeps `Id`). Order in `system_folder`: user override → `Named` → `host_folders` → naming rule. `Absent` keeps the naming rule's folder and sets `on_device = Some(false)`, so organise adds the existing `NoFolderOnDevice` note («Onion OS no tiene carpeta para Atari Jaguar; se crea JAGUAR.»). Existing systems are not added to the tables: no behaviour change for them. | Matches the brief («leave that profile without it») without inventing names, and reuses the note and copy that Anbernic stock already uses. Moving the item anyway, with the note, keeps one rule for «device lacks the folder»; leaving it in place would be a second, inconsistent rule. |
| D6 | A folder name two systems want (`pico` = Sega Pico on Batocera/Knulli, PICO-8 on Anbernic/Onion/muOS; `msx2`) | **Profile-scoped resolution**: `SystemDb::resolve_folder_in(profile: Option<&Profile>, name)` checks the profile's `Named` folders first, then global aliases. Every caller that knows the layout passes it (scan, organise walk and plan, settings validation, CLI). `pico` stays a global alias of `pico8`; Sega Pico's global aliases are `SEGAPICO`, `segapico`. `msx2` **moves** from `msx` to the new `msx2` system (libretro has a separate `Microsoft - MSX2` DAT and thumbnails; ES-DE, Batocera and muOS all have an `msx2` folder). | Without a profile there is no way to know which `pico` is meant; the current owner keeps it. A test asserts that every `Named` folder resolves back to its system under its profile, and that global aliases stay unique. |
| D7 | Batocera vs Knulli (one built-in, «Batocera / Knulli / EmulationStation») | **Batocera's names are written**; Knulli's different names are added as **global aliases** so a Knulli card scans: `o2em` (odyssey2), `bbc` (bbcmicro), `tyrquake` (quake). Where Knulli has no folder (`sc3000`, `dragon64`) nothing changes: the shared profile follows Batocera. A separate Knulli profile is out of scope (§13). | Knulli is a Batocera fork; 9 of the batch's folders differ. Scanning both is free; writing one set keeps the profile single. |
| D8 | First alias (used by the Imgs family and MinUI as their default name) | Onion's folder when Onion has one (`COLECO`, `FAIRCHILD`, `ODYSSEY`, `SGFX`, `SEGASGONE`, `ZXS`, `TIC`, `CPC`…), else NextUI's tag (`C128`, `PET`, `PLUS4`, `PRBOOM`), else the upper-case id (`PS2`, `JAGUAR`…). | Onion is the only sourced folder list of the Imgs family (`layouts.md` row 3, R1-10); the tables (D5) still mark the profiles that lack the system. |
| D9 | GarlicOS folders | **Same table as Onion** (closed source, FAQ 404). | Same inference as `layouts.md` R1-10, which already uses Onion's lists for GarlicOS. |
| D10 | MinUI vs NextUI | The `minui` profile is «MinUI / NextUI»; NextUI tags are used (`COLECO`, `CPC`, `C128`, `PET`, `PLUS4`, `PRBOOM`, `SG1000`). Base MinUI has none of the batch's systems; the note says so on organise. | NextUI ships those paks at the pinned commit; MinUI base does not. |
| D11 | Anbernic stock | `Absent` for every Tanda 1 system. | `ANBERNIC_FOLDERS` mirrors the owner's card (55 folders), and none of the batch is there. |
| D12 | muOS | muOS resolves a ROM folder by the `friendly` names of each core manifest (`share/info/manifest/core/**.json`); the table uses those names (`spectrum` for ZX Spectrum, `segapico`, `pc8000`, `wolfenstein3d`…). Systems muOS folds into another catalogue (`n64dd` → «Nintendo N64», `satellaview`/`sufami` → «Nintendo SNES - SFC», `msx2` → «Microsoft MSX») keep their own folder (muOS accepts it). | muOS has no fixed folder list; its auto-assign reads these names. The cover path keeps today's `{system_id}` rule (see §13: the catalogue name is a separate, pre-existing question). |
| D13 | Disc layout for new disc systems | `multi_file = true` for `ps2`, `3do`, `jaguarcd`, `pcfx`, `cdi`, `amigacd32`, `cdtv`, `fmtowns`; each profile's existing `disc_layout` applies (Flat on the Imgs family/muOS, GameFolder + `.m3u` on MinUI and ES-DE, Flat + `.m3u` on Batocera and RetroArch). **`ps2` joins `NO_ARCHIVE_SYSTEMS`** (no zip/7z taken as-is on any profile). | Disc systems already never take an archive as-is when they hold a sheet. PCSX2/AetherSX2 read no zip/7z (ES-DE lists only `.gz`), like PPSSPP for `psp` (R1-20). |
| D14 | PS2 / 3DO identification | PS2: serial from `SYSTEM.CNF` `BOOT2 = cdrom0:\SLUS_200.62;1` → `SLUS-20062`, matched against the Redump DAT's serial field, then the name. 3DO: Opera header, then the name (no serial on 3DO discs). Neither is hashed above 64 MiB (unchanged default). | Same ladder as PS1; hashing a 4 GB DVD is the user's choice (`hash max bytes`). |
| D15 | Ports / engines | **ScummVM-style**: claimed by their folder alias, and by a launcher/data extension only where the format proves it (`.wad` with `IWAD`/`PWAD`, `.solarus`, `.lutro`). A loose game folder outside a system folder is **left in place** with the new left reason `folderGame`; organise never splits it and dedupe skips it (each port id is added to the folder-game list beside `ports`). | The brief. Cave Story, Wolfenstein 3D, Mr. Boom, Flashback and Rick Dangerous ship as data folders with no launcher Coverly could trust. |
| D16 | Extra arcade | New systems `model2`, `model3`, `naomi2`, `triforce` (folders on ES-DE and Batocera). **System 16/18/24/32, Konami, Cave: no new systems**: no frontend has a folder for them; their sets are in the MAME/FBNeo DATs and already go to `mame`/`fbneo`. Model 2/3 sets found loose also go to `mame` (MAME runs them; the libretro MAME DAT has no driver field to tell them apart); inside a `model2/` folder they stay. **MAME CHD per game**: a `<set>/` folder holding `.chd` beside `<set>.zip` becomes a companion of the set and moves with it. | Avoids empty systems; keeps one place per set. |
| D17 | Famicom variants | **Aliases only, no new entries.** `famicom` and `fc` already resolve to `nes` (ES-DE's separate `famicom` folder scans as NES); BS-X is the Satellaview (new system); Disk Writer games are FDS images. libretro has one NES DAT and thumbnail set. | One DAT and one cover set per system. |
| D18 | Which systems appear by default | All systems are always available in pickers; lists are **grouped by family** (§9.1). Shelves and counts only show systems that have games. | The owner's open question; no hiding, so a user is never unable to pick the right system. |
| D19 | Icon licences | Only RetroArch Assets XMB Monochrome (CC BY 4.0, already attributed), at the **same pinned commit** `d9f969054dc7fbb6fa89519036d2b971e0855b51` (it is also `master` on 2026-10-31, 849 files). No other icon source. Systems without a neutral silhouette use the Slot mount. | No new licence to review; the decision the owner left open is avoided. |
| D20 | Game logos in the icon set (`DOOM`, `Quake`, `Cave Story`, `Rick Dangerous`, `Flashback`, `TIC-80`, `Uzebox`, `Arduboy`, `MrBoom`, `Dinothawr`, `Lutro`) | Audited on the contact sheet like the first 41: a logo or wordmark goes to `WORDMARK_IDS` and uses the Slot mount; a neutral device or pictogram is used. | `TRADEMARKS.md` rule already applied to CPS, MAME, DOS, ScummVM. |
| D21 | `family` for grouping | New `family` field on every system (existing ones too): `console`, `handheld`, `computer`, `arcade`, `engine`. Sent in `SystemInfo`. | Needed for the grouped picker (D18); data, not UI logic. |

## 5. Architecture and files touched

### 5.1 Rust

| File | Change |
|---|---|
| `systems/systems.toml` | ≈ 70 entries (§7), `family` on all, `weak_extensions`, `msx2` alias moved, `sgx` moved, Knulli aliases. Header comment documents both new fields. |
| `systems/mod.rs` | `System { family: Family, weak_extensions: Vec<String> }` (serde defaults: `console`, empty); validation: a weak extension may not also be in `extensions`; `extension_class` ignores `weak_extensions`; `systems_for_extension` keeps returning only strong owners; new `systems_accepting(ext)` (strong + weak) for folder and header paths; `resolve_folder_in(profile, name)`. |
| `profiles/mod.rs`, `profiles/naming.rs` | `Profile.folders`, `ProfileFolder`; `system_folder` order (D5); `validate_folder_override` accepts a `Named` folder of the profile. |
| `profiles/builtin.rs` | Per-profile tables (§6.1) as `const` arrays of `(id, Option<&str>)`, converted once. `NO_ARCHIVE_SYSTEMS` gains `ps2`. |
| `profiles/custom.rs` | `folders` copied when a built-in is cloned; optional in the profile file (`#[serde(default, skip_serializing_if = BTreeMap::is_empty)]`); validated like `host_folders`. |
| `scan/mod.rs`, `organize/walk.rs`, `organize/plan.rs`, `settings/validate.rs`, `coverly-cli/src/dedupe.rs` | Call `resolve_folder_in` with the active profile. |
| `identify/sniff.rs` | New hits (§8.1); `PlayStation 2` and `Sega Pico` become hits (`ps2` 0.95 with `BOOT2`, 0.80 for a large `PLAYSTATION` volume without it; `pico` 0.95); `DECISIVE` gains `vectrex`, `pico`, `sufami`, `uzebox`, `vircon32`. |
| `disc/mod.rs`, `disc/signatures.rs`, `disc/iso9660.rs` | `DiscSystem::Known("ps2" / "3do" / "pcfx" / "cdi" / "amigacd32" / "cdtv" / "jaguarcd")`; PS2 serial from `BOOT2`; new signatures (§8.2). `Unsupported(...)` stays for any future system. |
| `organize/evidence.rs` | Weak-extension evidence: 0.50 inside a non-system folder **only** when another signal agrees; detail `ext~` («weak»). |
| `organize/target.rs`, `organize/plan.rs` | `LeftReason::FolderGame { system }`; MAME CHD companion (D16). |
| `dedupe/units.rs` | Folder-game systems (`scummvm` plus D15's ports) treated like `ports`. |
| `src-tauri/src/…` (system list command) | `SystemInfo` gains `family`. |

### 5.2 Front end

| File | Change |
|---|---|
| `src/ipc/types.ts`, `contract.sample.json`, `contract.test.ts` | `SystemInfo.family: 'console' \| 'handheld' \| 'computer' \| 'arcade' \| 'engine'`; `LeftReason` `folder-game`. |
| `src/components/organize/LeftInPlace.tsx`, `ReviewBuckets.tsx` | System options grouped with `<optgroup>` by family (§9.1), names sorted with `Intl.Collator(locale)`. One shared helper `systemOptionGroups(systems, t)` in `src/lib/systems.ts`. |
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
- Every `Named` folder is also a global alias of its system unless it collides (`pico`, D6).

### 6.2 Disc layout per profile

No per-system override: D13. ES-DE, MinUI → `PS2/<stem>/<stem>.cue` style folders for CUE/BIN sets; a single `.iso`/`.chd` moves as a file. FAT32 cards: a PS2 ISO over 4 GiB is left with the existing `TooBigForFat` reason.

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

Changes to existing entries: `msx` loses alias `msx2`; `pcengine` moves `sgx` and `snes` moves `bs` to their `weak_extensions` (D3); `nes` gains nothing (D17); all 54 gain `family`. `arcade = true` on the four arcade rows. Systems without any strong extension (`plus4`, `pet`, `c128`, `atarixe`, `quake`, ports without a launcher, `triforce`) are claimed by folder, header or hash only, which is the point.

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

Anything marked «verify» is checked against a real dump or the emulator source during its task; a rule that cannot be verified is dropped (the system keeps folder + hash), and the drop is listed in the PR.

No header (folder + hash only): intellivision, odyssey2, channelf, arcadia, supervision, sg1000, supergrafx, n64dd, satellaview, gamecom, megaduck, scv, zx81, x68000, x1, pc88, msx2, msxturbor, atarixe, thomson, dragon, coco, tic80, wasm4 (the `\0asm` magic only proves WebAssembly), lowresnx.

### 8.2 Discs (`disc/`)

| System | Rule | Source |
|---|---|---|
| ps2 | Existing: `SYSTEM.CNF` with `BOOT2`, or a `PLAYSTATION` PVD larger than a CD. New: serial from the `BOOT2` path (`cdrom0:\SLUS_200.62;1` → `SLUS-20062`, `normalise_serial`). CSO/ZSO: the decompressed first blocks go through the same check. | rcheevos `rc_hash_ps2` (`BOOT2`, `cdrom0:`) |
| 3do | Existing Opera volume header at sector 0 of track 1. | rcheevos `rc_hash_3do` |
| pcfx | `PC-FX:Hu_CD-ROM` at byte 0 of sector 0 of a data track; checked **before** the PC Engine CD text (some PC-FX discs also carry it). | rcheevos `rc_hash_pcfx_cd`; Beetle SuperGrafx `pce.cpp` |
| jaguarcd | `ATARI APPROVED DATA HEADER ATRI ` (or its byte-swapped form) in the first sector of the first data track of session 2; a CUE without `REM SESSION 02` cannot be checked → folder/name only. | rcheevos `rc_hash_jaguar_cd` |
| cdi | Green Book disc label at sector 16: record type `01`, `CD-I `, version `01`. | CD-i Full Functional Specification; verify on a Redump dump |
| amigacd32 / cdtv | ISO 9660 root holds `CD32.TM` / `CDTV.TM`. | UAE / FS-UAE detection; verify |
| fmtowns | No signature: folder and name. | — |

### 8.3 Hash and name

Every row with a DAT in §7 is hashed (files ≤ 64 MiB) and name-matched. `dat_sources` keeps its rule; explicit `dats` cover the TOSEC, homebrew and `dat/` files. `system_for_dat` picks the system with the fewest DAT sources and keeps database order on a tie: `mame`, `model2` and `model3` each list only `MAME.dat`, so the entries for `model2` and `model3` are placed **after** `mame` in `systems.toml` and `MAME.dat` keeps mapping to `mame` (D16). `naomi2` lists two DATs, so it never wins `MAME.dat`. A test pins both.

## 9. UX and UI design

The visible change is small: more systems, grouped pickers, and icons. No new screen, no new token.

### 9.1 Design plan (frontend-design pass)

- **Colour**: existing semantic tokens only (`--cr-text`, `--cr-text-secondary`, `--cr-border`, `--cr-manila` for the Slot mount's label tape). No new hex values; `pnpm tokens:check` stays green.
- **Type**: Atkinson Hyperlegible Next for names and group labels (`type.app.label` for `<optgroup>` where the platform renders it; native otherwise); Atkinson Mono for the abbreviation inside the Slot mount (unchanged). No Archivo in this batch (nothing ≥ 24px changes).
- **Layout**: the review row keeps `filename · [Elige un sistema…]`. The native select's options become:

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

Review against generic defaults: the obvious default would be a searchable combobox with icons per option. Rejected: a native `<select>` is what the screen uses today, is keyboard- and screen-reader-native on all three OSes, and grouping solves the length problem (118 options in 5 groups). A custom combobox is a separate item if the owner wants it (§13). Group labels are plain sentence-case words, no all-caps eyebrows, no counts in the label.

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

`?mockPick=retro` (beside `downloads|loose|mixed`): a pile with `Gran Turismo 4 (Europe).iso` (PS2, `BOOT2` serial `SCES-51719`), `Tempest 2000 (World).j64`, `Manic Miner (1983)(Bug-Byte).tzx`, `Arkanoid (1987).dsk` (CPC magic → review: Amstrad CPC or ZX Spectrum), `Sailor Moon (Japan).st` with the Sufami magic, `Prince of Persia.st` without it (Atari ST), `doom/DOOM.WAD`, `Cave Story/` (a loose folder game → left, `folderGame`), `sfiii.zip` beside `sfiii/` holding a `.chd` (one item with its CHD companion), `game.bin` (generic → left, unknown). Shelves fixture gains `PS2`, `ZXS`, `WASM4` so a silhouette and a Slot mount render side by side.

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
| `organize.left.folderGame` | Juego en carpeta de {system}: muévela entera a la carpeta {folder} | Folder game for {system}: move the whole folder to {folder} |

Existing keys reused unchanged: `organize.noFolderOnDevice` («{layout} no tiene carpeta para {system}; se crea {folder}.»), `organize.why.disc`, `why.header`, `why.hash`, `why.extensionShared`. `organize.unsupportedSystem` stays for user systems; PS2, 3DO and Pico no longer produce it. The parity test covers the new keys.

## 11. Data flow

1. **Load**: `SystemDb` parses `family` and `weak_extensions`; validation rejects a weak extension that is also strong, an unknown family, and an alias that two systems claim globally.
2. **Scan** (`scan_library_with_progress(root, profile, …)`): each top folder → `resolve_folder_in(Some(profile), name)` → system; files filtered by `systems_accepting` (strong + weak) for that system.
3. **Organise walk**: folders that resolve (profile-scoped) are skipped as already sorted; a loose folder whose name or content matches a folder-game system → `LeftReason::FolderGame`.
4. **Evidence**: sniff hits (§8) + extension evidence (strong only) + DAT hash/serial + name. Weak extensions add evidence only when a folder or header already names the system.
5. **Target**: `system_folder(profile, system, db, overrides)` → override, `Named`, host, rule; `Absent` → note. Disc systems follow the profile's `disc_layout`; `ps2` never takes an archive as-is.
6. **Covers**: libretro playlist name (§7) → thumbnails; absent → ScreenScraper when `screenscraper` is set and the user enabled it; otherwise none.
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
| PS2 CSO/ZSO first blocks unreadable | No header hit; extension `zso` (strong) still says ps2; `cso` (weak) stays psp unless the header says ps2. |
| Jaguar CD single-session CUE | Folder or name only; never auto by `.cdi`. |
| DAT missing on the server (404) | Remembered for the TTL (existing); identification falls back to header/name. A new DAT path that 404s at review time is a bug: test (§14.1) asserts every path exists in the pinned tree listing committed as a fixture. |
| Two systems with the same libretro folder (`model2`, `model3`, `mame`, `pgm2` → `MAME`) | Allowed today (pgm2); cover lookup works for each. |
| A custom profile cloned before this batch | Has no `folders`; behaves like today (rule-based names). Re-cloning picks the tables up. |
| An old settings file | No new keys; nothing to migrate. |
| Windows paths | Fixture paths compared by component; fixture mtimes pinned. |

## 13. Blocked, deferred and out of scope

Needs an owner decision (implemented around; listed in the PR):
- **Taito Type X**: Windows executables per game, no DAT, no libretro thumbnails, no ScreenScraper id found; only ES-DE has a `type-x` folder. Catalogue-only like Tanda 3? Owner to decide.
- **Cave CV1000 (`cave3rd`)**: a Batocera-only folder; MAME sets today. New system or stay in `mame`?
- **ScreenScraper ids for C128, PET, NAOMI 2 and Triforce**: not found in ES-DE's or Skyscraper's maps; `systemesListe.php` needs developer credentials (`docs/02-research/screenscraper-api.md`). Left absent until checked with credentials.

Observed, pre-existing, not changed here:
- The muOS cover path uses `{system_id}` as the catalogue folder, while muOS catalogues are named like «Atari Jaguar» (manifest `catalogue`). New systems follow the existing rule for consistency; fixing the catalogue name is its own item.
- MinUI/NextUI folders are «Display Name (TAG)»; the profile writes the bare tag. Unchanged.

Out of scope: Videopac+ (G7400), Knulli as its own profile, a searchable system combobox, Tanda 2 and 3.

## 14. Test plan

### 14.1 Rust (`cargo test --workspace`)

- `systems`: every new entry loads; families valid; weak/strong disjoint; global aliases unique; **extension-class snapshot** (`insta`) for every pre-batch extension, whose diff from `main` is exactly D3's list; `.sgx` → supergrafx; `msx2` alias → msx2.
- `profiles`: for every built-in × every Tanda 1 system, `system_folder` returns the §6.1 name or the rule name with `on_device = Some(false)`; every `Named` folder resolves back through `resolve_folder_in(Some(profile))`; `pico` resolves to pico8 without a profile and to Sega Pico on batocera; custom profile round-trip with `folders`.
- `sniff` (`tests/sniff.rs`): one positive and one negative fixture per §8.1 rule (synthetic byte heads, no copyrighted data); Pico and PS2 become hits.
- `disc_identify`: PS2 head (existing `disc/testing.rs` builder) → `Known("ps2")` with serial; 3DO → `Known("3do")`; PC-FX before PC Engine CD; CD-i, CD32, CDTV, Jaguar CD builders.
- `dat`: parse trimmed fixtures of a TOSEC, a `dat/` and a homebrew DAT; a committed listing of libretro-database paths at `2c93952f` contains every `dats` path and derived path.
- `organize_target`: per family, a ROM lands in the right folder on esde, batocera, muos, onion and minui, with the `NoFolderOnDevice` note where `Absent`; ps2 archive is extracted, never moved as-is.
- `organize_plan` snapshots: new `organize_plan__systems_batch_1__{esde,batocera,onion}.snap` over the §9.3 pile (built under `target/` with `cp -RXp`, pinned mtimes, removed afterwards).
- `organize_walk`: a loose `Cave Story/` folder → `FolderGame`; `sfiii.zip` + `sfiii/*.chd` move together.
- `dedupe_units`: port folders are out of scope like `PORTS/`.
- `libretro` (cover lookup): for every system with `libretro`, the thumbnail URL is built from the exact §7 name (committed copy of the live root listing; a test asserts each name is in it).

### 14.2 Vitest (`pnpm check-all`)

- `systemOptionGroups`: fixed group order, locale sorting (es: «Ámbar» before «Bravo»), empty groups dropped.
- `LeftInPlace` / `ReviewBuckets`: options inside `<optgroup label="Consolas">` etc.
- `SystemIcon`: a new silhouette (`ps2`) renders an `svg`; `wasm4` renders the mount with `W4`; `systemIcons.test.ts` passes on the regenerated module.
- `organize.ts` copy for `folder-game` and `extension-weak`; i18n parity.
- Contract test with `family`.

### 14.3 E2E (Chrome DevTools MCP, isolated context `systems-batch-1`, dev server on port 1465)

1. `?mockPick=retro` → organise offer → plan: PS2 group with its silhouette, «Disco de Sony PlayStation 2 … (SCES-51719)» in the why line; ZX Spectrum and Jaguar groups; `Sailor Moon` under Sufami Turbo, `Prince of Persia` under Atari ST.
2. Review: `Arkanoid (1987).dsk` → select shows groups; keyboard-only: Tab to the select, arrow to «Amstrad CPC» in «Ordenadores», apply.
3. Left in place: `Cave Story` with the folder-game line; `game.bin` unknown.
4. Switch profile to Onion: the PS2 item shows «Onion OS no tiene carpeta para Sony PlayStation 2; se crea PS2.»
5. Library shelves: `PS2` silhouette and `WASM4` mount, light and dark (`data-theme`), screenshots attached to the PR.
Fallback: Playwright MCP, then `playwright-core` + Chrome for Testing in the scratchpad.

### 14.4 Gates

`pnpm check-all`, `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`.

## 15. Tasks (one reviewable commit series each)

1. Core model: `family`, `weak_extensions`, `Profile.folders`, `resolve_folder_in`, custom-profile serde, extension-class snapshot (no new systems yet).
2. Disc family: ps2, 3do, pcfx, cdi, amigacd32, cdtv, jaguarcd, fmtowns (+ `NO_ARCHIVE_SYSTEMS`).
3. Early consoles: colecovision, intellivision, vectrex, odyssey2, channelf, arcadia, scv, jaguar.
4. Sega / NEC / Nintendo add-ons: sg1000, pico, supergrafx, n64dd, satellaview, sufami; Famicom check (D17).
5. Handhelds: supervision, gamecom, megaduck.
6. Computers I: zxspectrum, zx81, amstradcpc, bbcmicro, thomson, dragon, coco, ti99.
7. Computers II: x68000, x1, pc98, pc88.
8. Computers III: plus4, pet, c128, atarixe, msx2, msxturbor, apple2.
9. Fantasy consoles: tic80, wasm4, arduboy, uzebox, lowresnx, vircon32.
10. Ports and engines + folder-game rule.
11. Extra arcade + MAME CHD companions.
12. Icons: sources, audit, regenerate, notices.
13. UI: `SystemInfo.family`, grouped pickers, copy, mock pile.
14. E2E pass, PR with the blocked list (§13).

Each system task adds its §7 rows, its §6.1 table rows, its §8 rules and its tests together.

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
