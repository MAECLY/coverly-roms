# Coverly ROMs — Adjustments after systems batches 1–3 (adjustments 1, 2, 4, 5, 6 and 7) (Design Spec)

> Status: **Draft for implementation on `feat/adjustments`** (self-reviewed §17) · Date: 2026-10-26 (research and spec run 2026-11-01) · Owner: Miguel Angel Esparza Calero (MAECLY)
> Roadmap item: «Tanda de ajustes tras las Tandas 1–3 (ajustes 1, 2, 4, 5, 6 y 7)». Systems batches 1–3 are merged (PR #32, #33, #34; specs [`2026-10-26-systems-batch-1.md`](2026-10-26-systems-batch-1.md), [`-2.md`](2026-10-26-systems-batch-2.md), [`-3.md`](2026-10-26-systems-batch-3.md)).
> Branch: `feat/adjustments`. `docs/TRACKER.md` and `TODO.md` are updated on `main` after merge, never on this branch.
> **Out of scope:** adjustment 3 (frontend media folders, `TODO.md` §6), the launchers `.psvita`/`.psn`/serial files (`TODO.md` §6 «Lanzadores»), every source of `TODO.md` §7.1 except `blawar/titledb`, and the user-keys mode («Claves aportadas por el usuario», parked).
> Research read: [`switch-titledb.md`](../../02-research/switch-titledb.md), [`user-keys-legal.md`](../../02-research/user-keys-legal.md), [`loose-executable-headers.md`](../../02-research/loose-executable-headers.md), [`zipped-folders-multidisc.md`](../../02-research/zipped-folders-multidisc.md), [`credits-inventory.md`](../../02-research/credits-inventory.md); brand [`docs/08-brand/brand.md`](../../08-brand/brand.md).

## 1. Purpose

Batches 1–3 taught Coverly 131 systems. Using them on real piles (the owner's `/Volumes/TinyExt/ROMs-Downloaded` and `/Volumes/TinyExt/Roms`) showed six gaps that are not new systems but make the existing ones trustworthy:

1. A Switch dump is identified by title id, but Coverly cannot say **which game** it is without keys. It should use the public catalogue, never keys.
2. Loose executables and odd files (`.nro`, `.xbe`, `.tmd`, `EBOOT.PBP`…) land in «no identificado» even though their headers say exactly what they are.
3. The 10 GB confirmation of batch 3 is one question for the whole plan; with three 40 GB PS4 folders the user wants to answer per game.
4. Duplicates found by title id or by folder are only reported; they must go through the duplicates system (set aside or Trash, with undo).
5. A PS3/PS4/Vita/Wii U game folder zipped whole is one opaque archive today, and loose multi-disc games are not grouped.
6. Nothing in the app credits the sources, datasets, docs, libraries, fonts and icons Coverly relies on, and the bundle misses licence texts.

Rules kept from earlier batches: no silent reclassification, headers only within budget (batch 2 D24), ambiguous to review, every write journaled and undoable, never read/request/ship a key. Rule added: **every network host and every dataset has a credit entry, enforced by a test** (D30).

## 2. The owner's asks

The roadmap section named in the brief («Tanda de ajustes tras las Tandas 1–3») is not present in `TODO.md` at `c497690`; only its references are (§7 «Autorizado por Miguel dentro de la tanda de ajustes (ajuste 7)», §7.1 «La tanda de ajustes solo integra `blawar/titledb` para Switch»). The asks below are therefore taken from the workflow brief of this run, which states the owner-approved list, plus the `TODO.md` sections it cites (quoted verbatim where they exist).

**From `TODO.md` §2 «Claves aportadas por el usuario (aparcado)»:**

> Decidido con Miguel el 2026-11-01: por ahora Coverly identifica los juegos **solo sin claves**. Lee lo que está en claro (3DS descifrado y sección Meta de los CIA; en Switch, el title ID sacado del ticket, del nombre del archivo o del id del NCA) y busca el nombre en `blawar/titledb` (MIT) por title ID. Lo que no se resuelve queda marcado como «cifrado».
>
> El modo «usa tus propias claves» (`prod.keys`, `keys.txt`) queda como posible extra para más adelante […] Nunca se incluyen, descargan, enlazan, copian ni registran. Nunca se ofrece descifrar ni exportar un juego. Antes de hacerlo, una revisión legal.

**From `TODO.md` §7 «Agradecimientos y créditos»** (verbatim list):

> - Pestaña «Agradecimientos» en Ajustes (es/en), con el lenguaje visual de la app, agrupada por tipo: fuentes de portadas y metadatos; bases de datos y DAT; documentación y wikis técnicos; convenciones de frontends y firmwares; bibliotecas; tipografías e iconos.
> - Cada entrada lleva el nombre, para qué la usa Coverly en una línea, un enlace a su web o repo oficial, su licencia y el texto de crédito exacto cuando la licencia o los términos lo exijan. Nunca enlaces a ROMs, claves ni emuladores.
> - Una fuente única de datos (un archivo de créditos en el repo) que alimenta la pestaña, el README y la landing, para no repetir la lista.
> - Un test que falle si un proveedor de red o un conjunto de datos nuevo no tiene su entrada de crédito.
> - Avisos de terceros de las dependencias (Rust y JS) generados al compilar […] Se muestran desde la misma pestaña («Ver licencias de terceros»).
> - Buscable desde el buscador de Ajustes, como el resto de secciones.
> - Corregir a la vez los fallos de cumplimiento que encontró el inventario: el bundle no incluye `LICENSES/*.txt` ni los OFL de las tipografías; `THIRD-PARTY-NOTICES.md` cita TheGamesDB, que no se usa, y remite a términos en `docs/09-legal/` que no existen; faltan los avisos de las dependencias JS y de varios crates (MPL de option-ext, CDLA de webpki-roots, ISC/BoringSSL de ring, Unicode-3.0); falta el enlace que pide GameTDB; falta el crédito CC BY 4.0 de los iconos de RetroArch Assets en la interfaz.

**From the brief (the adjustments, in the brief's words):**

- **(1) Keyless identification only.** Switch title id from ticket / file name `[0100…]` / NCA id cross-referenced with titledb `ncas.json`; names from blawar/titledb (MIT) as an optional on-demand download (one region/language by UI locale with US.en fallback, cached, refreshed at most weekly with ETag/If-Modified-Since, reduced to id → name/publisher/releaseDate/region; never store key/rightsId/cnmts/cheats; never bundle the catalogue in repo or installer; api.nlib.cc as configurable fallback; file-name fallback when offline). Switch icons from `iconUrl` only opt-in, fetched on demand, local cache only. 3DS name/icon only from decrypted/NoCrypto NCCH and the CIA Meta section. Anything unresolved is labelled «cifrado». Never read, request, bundle, download, link or log console keys; no user-keys mode.
- **(2)** Classify loose odd files by plaintext headers per `loose-executable-headers.md` (`.nro`, `.nso`, `main.npdm`, `.nca` by name, `.dol` by structure, `.xbe`, `.xex`, `.rpx`/`.rpl`, `.tmd` comparing **full** title-id high values — 3DS patch `0x0004000E` is not DLC —, `.psvimg`, `.self`/`eboot.bin`, PSP `~PSP`/PBP, `.3dsx`): homebrew vs game vs update vs DLC.
- **(4)** Huge items: extend batch 3's 10 GB confirmation into a per-item «Sí / Sí a todo / No» prompt showing size and free space on the target, also in the CLI.
- **(5)** Duplicates found by folder and by title id (+version) go through the existing duplicates system (set aside or Trash per settings, with undo), not just reported.
- **(6)** Folder games inside zip/7z detected from the entry listing via the existing `DirProbe`s on a virtual listing (template `vpk_console`); the PS4/Vita SFO distinction on that listing; actions per system (Vita stays compressed; PS3 disc, PS4, Wii U extract with confirmation; PS3 HDD/PSN install note). Loose multi-disc grouping by title-without-disc-tag, confirmed by headers (GC/Wii byte `0x06`, Saturn `CD-n/m`, Dreamcast `GD-ROMn/m`, XBE/XEX disc numbers) or DAT hash. Do not bundle DuckStation discsets (CC BY-NC-ND).
- **(7)** The «Agradecimientos» tab as in `TODO.md` §7 (above), with the required credits: ScreenScraper CC BY-NC-SA, RetroArch Assets CC BY 4.0 icons notice in the UI, GameTDB link, libretro-database CC BY-SA, titledb MIT; notices built with cargo-about (Rust) and `pnpm licenses --prod` (JS).

## 3. What exists today (read before changing)

| Layer | Where | What matters here |
|---|---|---|
| Switch reader | `consoles/nx.rs` | Title id from a ticket **entry name** (`<rightsId>.tik`), a plaintext `*.cnmt.xml`, else `[0100…]`/`[vN]` file-name tokens. XCI: card header to 0x13F and root HFS0 names only; the secure partition's `<ncaId>.nca` names are **not** listed today. `.nro`/`.nso` give a magic-only `switch` verdict with no kind and no title. «No title-id catalogue is downloaded» (module doc). |
| 3DS reader | `consoles/ctr.rs` | Already keyless: NCSD/NCCH headers, CIA header + TMD, CIA Meta SMDH, ExeFS icon on `NoCrypto` only, `encrypted: Some(true)` otherwise. 3DSX with SMDH. No change to what is read; only the «cifrado» label (D5). |
| Key guard | `consoles/mod.rs` `is_key_material`, `KEY_EXTS`; `LeftReason::KeyMaterial` | Key files are left by name, never opened. |
| ConsoleId | `consoles/mod.rs` | `kind: TitleKind { Base, Update, Dlc, Demo, Unknown }` — **no `Homebrew`**; `encrypted`, `title`, `id_source`, `content_id`, `region`, `product_id`, `parent_id`, `art`. |
| Folder probes | `consoles/mod.rs` `DirListing { dir, names }`, `DIR_PROBES`, `identify_dir`; `sony::sce_matches` (PS4, PS5, Vita in one `sce_sys` listing) | Every probe does its own disk I/O (`symlink_metadata`, `list_child`, `read_head`) from `l.dir`: they cannot run on an archive. The PS4/Vita split exists for disk folders (batch 3 D5). |
| Archive inspection | `organize/inspect.rs` `vpk_console`, `vpk_entry`, `package_hits`; `archive::open_archive_with`, `file_entries`, `read_entry`, `Budget::plan_head` (64 KiB) | Reads `sce_sys/param.sfo` at the archive **root** only, case-insensitive. |
| Multi-disc | `organize/target.rs` (`multi_disc`, m3u for **extracted** archives), `organize/extract.rs` `m3u_text`; `disc/mod.rs` `DiscIdentity { disc_no, … }` (the Sega `m` of `n/m` is dropped); `consoles/nintendo_disc.rs` `disc_no`; `dedupe/disc.rs` | Loose discs are never grouped and get no `.m3u`. |
| Large confirmation | `organize/apply.rs` `LARGE_TRANSFER` (10 GB), `large_transfer(plan) -> Option<LargeTransfer { bytes, items, copies, free, volume }>`; `State::organize_apply(…, confirmed_bytes)`; `IpcError::LargeTransfer` (code `large-transfer`); `LargeTransferDialog.tsx`; CLI `--yes-large` | One aggregate question; `Choice::Leave` already works on any item (`plan.rs` decision loop). |
| Duplicates | `dedupe/keys.rs` `ExactKey::Title { title_id, kind, version, disc }` — **`is_confirmed() == false`, report-only** (`ReportWhy::TitleId`); `dedupe/group.rs` `GroupAction::ReportOnly`; Organize `OrganizeDedupeMode::SetAside`, `QuarantineChoice::Trash`; `dedupe/units.rs` `UnitKind::Folder`, `units_from_folders` | Title and folder matches never move anything. |
| Network | `net/mod.rs` (`HttpRequest { url, if_none_match, max_bytes = 32 MiB, … }`, body in memory); hosts in code: `thumbnails.libretro.com`, `raw.githubusercontent.com` (libretro-database), `www.steamgriddb.com`/`cdn2.steamgriddb.com`, `api.screenscraper.fr`, `github.com` (updater) | No `If-Modified-Since`, no stream-to-file, no host registry. |
| Cache | `cache/mod.rs` (SQLite, `DatRow`), `identify/dat.rs` `DatStore` (30-day TTL, conditional GET) | Pattern to copy for the catalogue. |
| Settings UI | `src/components/settings/*Section.tsx`, `registry.ts` `SETTINGS_INDEX` (completeness test), `navStore.ts` `SettingsSection` (12 tabs), `SettingsSearch.tsx` | No credits tab. `AboutSection` links `LinkId::Notices` to the repo on GitHub. |
| Links | `src-tauri/src/prefs.rs` `LinkId` + `link_url` (allowlist of repo URLs; a test pins the prefix) | The front end never opens a URL it built itself. |
| Licences | `LICENSES/` (`CC-BY-4.0.txt`, `ISC-lucide-react.txt`), `src/assets/fonts/OFL-*.txt`, `THIRD-PARTY-NOTICES.md`, `scripts/check-licenses.sh` (allowlist on `cargo metadata`); `tauri.conf.json` `bundle.resources` = `LICENSE`, `THIRD-PARTY-NOTICES.md` only | Gaps in `credits-inventory.md` §6 hold. `cargo-about 0.9.2` is installed on the owner's machine. |
| Locales | `locales/{es,en}/ui.json`, nested; parity test; `…One` count keys | A key cannot be both a string and a parent. |

## 4. Decisions

Open questions got a default so nothing blocks; each owner-facing one is repeated in §13.

| # | Topic | Decision | Why |
|---|---|---|---|
| D1 | Keys | No code path reads, requests, bundles, downloads, links or logs console keys. A new test (`tests/no_keys.rs`) fails if any non-test source under `crates/`, `src-tauri/src` or `src/` contains `prod.keys`, `title.keys`, `keys.txt`, `aes_keys.txt`, `seeddb.bin` or `boot9` **outside** the `is_key_material` name list and its tests, or if a catalogue row type has a field named `key`, `rightsId`, `titleKey`. The UI never names where keys come from. | Owner rule; Yuzu judgement (`user-keys-legal.md` §3). |
| D2 | Catalogue opt-in | One setting `catalogs.switch.enabled` (default **off**). Off: nothing is downloaded, identification uses tickets/names only. On: the region file is downloaded on the first plan, library scan or the explicit «Descargar ahora». | Optional on-demand, never bundled. |
| D3 | Which file | UI locale `es` → `ES.es.json`, `en` → `US.en.json` (`https://raw.githubusercontent.com/blawar/titledb/master/<REGION>.<lang>.json`). If the locale's file answers 404 or fails to parse, `US.en.json` is used instead and the status says so. **Only one file is kept.** Ids missing from the locale file are not looked up in a second 90 MB file (owner decision §13 Q1). | Brief: one region/language, US.en fallback; 90 MB per file. |
| D4 | Refresh | At most once every **7 days** (setting `catalogs.switch.refresh_days`, 7–90). A conditional GET with the stored `ETag` (`If-None-Match`) **and** `Last-Modified` (`If-Modified-Since`, new `HttpRequest.if_modified_since`). 304 → only the check time moves. A failed refresh keeps the old index. Rate limit/offline pause like DATs. | Region files have not changed since 2026-09-14; weekly is the cap. |
| D5 | Reduction | The download streams to a temp file (new `Http::get_to_file`, cap **160 MB**), then a streaming serde visitor walks the top-level map and keeps, per entry with a valid 16-hex `id`, only `name`, `publisher`, `releaseDate` (int `YYYYMMDD` → ISO date), `region` and — only when icons are on (D8) — `iconUrl` if its host is `img-eshop.cdn.nintendo.net`. Everything else (`key`, `rightsId`, `description`, `screenshots`, `nsuId`, ratings…) is never deserialized into memory. Rows go into the cache DB table `titledb(id INTEGER PRIMARY KEY, name, publisher, release, region, icon_url)`; the temp file is deleted at once. 29 572 ids ≈ 3.6 MB. Duplicate ids keep the first entry whose `name` is non-empty. | Brief: id → name/publisher/releaseDate/region; never store key/rightsId/cnmts/cheats. |
| D6 | NCA ids | `ncas.json` (87.6 MB) is fetched **lazily**, only while the catalogue is on and an item has an NCA id but no title id: a loose `<32 hex>.nca`/`.ncz` (name), or an XCI whose secure HFS0 lists `<32 hex>.nca` names (new: `nx::xci` reads the secure partition's **names table only**, within D24 budgets). Reduced to `ncas(nca BLOB(16) PRIMARY KEY, title INTEGER, content_type INTEGER)` (≈ 5.7 MB); same refresh rule (D4). The title id it gives counts as `IdSource::NcaCatalog`. `cnmts.json`, `versions.json`, `cheats.json` are never fetched. | Brief: NCA id cross-referenced with `ncas.json`; research C13. |
| D7 | Name priority | Switch title: plaintext `*.cnmt.xml`/NRO NACP title → **titledb** (by base id `& 0xFFFFFFFFFFFFE000` for updates/DLC, which show «Actualización de {name}»/«DLC de {name}») → nlib fallback (D9) → file-name title. `ConsoleId.title_source` (new, `Header`/`Titledb`/`Nlib`/`FileName`) feeds the «why» line. | Keyless order of `switch-titledb.md` §3 minus the parked NACP-by-keys step. |
| D8 | Switch icons | Setting `catalogs.switch.icons` (default **off**, enabled only while the catalogue is on). Icons are fetched on demand from the stored `iconUrl` (host allowlist `img-eshop.cdn.nintendo.net`, ≤ 1 MiB, JPEG/PNG decoded and re-encoded to a 256 px PNG) into the art cache, and shown **only inside Coverly** (library tile, Organizar row thumbnail). Fill never writes them into the library (no new `KNOWN_SOURCES` id). Turning icons off deletes the cached icons and clears `icon_url`. | Brief: opt-in, on demand, local cache only. Official art is licensed to nobody (`TODO.md` §7.1 rules). |
| D9 | nlib fallback | Setting `catalogs.switch.fallback` (default **off**) with `catalogs.switch.fallback_url` (default `https://api.nlib.cc`, HTTPS only, validated like the proxy URL). When on, a title id the index does not resolve is asked as `GET {url}/nx/{TID}?lang={en|es}` (one request per id, the global limiter, results cached 30 days, misses 7 days). The response is parsed defensively (`name`, `publisher`, `releaseDate` if present; anything else ignored; the exact shape is verified against the live API in task T3 and pinned in a fixture). Off by default because it sends title ids to a third party. | Brief: configurable fallback. |
| D10 | Offline | No catalogue, no network, or a failed first download: names come from the file name (D7 last step); the plan never waits on the catalogue (a 30 s cap on the first download inside a plan; past it the plan continues and the status shows «Descarga del catálogo pendiente»). | Brief: file-name fallback. |
| D11 | «cifrado» | An item of `switch` or `3ds` whose title stays unresolved after D7 **and** whose `encrypted == Some(true)` (or a Switch NSP/XCI/NCA, always encrypted) shows the label «Cifrado» next to its name and the note «No se puede leer el nombre sin descifrarlo». A resolved one keeps the existing `ItemNote::Encrypted` note only where it already applies. | Owner: «Lo que no se resuelve queda marcado como "cifrado"». |
| D12 | Loose classifier | New `consoles/loose.rs`, called from `identify_one` **after** the Nintendo disc/WAD/3DS/Wii U/Sony readers and **before** `orbis`/`xbox`, for the formats of §8.2. It returns a `ConsoleId` with `kind` (new `TitleKind::Homebrew`) and, for pieces that cannot play alone, a new `ConsoleId.part: Option<PartKind>` (`ExeFs`, `Nca`, `Library`, `TitleMeta`, `Module`, `Backup`). | One module for the research table; existing readers keep priority. |
| D13 | Where they go | Homebrew, games, updates and DLC move like any ROM of their system (updates/DLC under their base, batch 2 D11). A `part` item is **left** with the new `LeftReason::GamePart { part, system, title_id }` («Parte de un juego; no se mueve suelta»), except a `.nca` whose catalogue entry says `content_type = Meta` alone (still left). `.psvimg` is left `GamePart { Backup }` with the title id from its path. PSP `EBOOT.PBP` of a PS1 classic goes to `psx`. | Moving one ExeFS file breaks the dump; research §11. |
| D14 | `.tmd` kinds | Compare the **full** upper 32 bits of the title id: 3DS `00040000` app, `00040002` demo, `0004000E` update, `0004008C` DLC, `00040001` Download Play child (base), `00040010`/`0004001B`/`00040030`/`0004009B`/`000400DB`/`00040130`/`00040138` system → `NotARom::ConsoleSystem`; Wii `00010000`/`00010001`/`00010004` base, `00010005` DLC, `00010002`/`00010008`/`00000001` system; Wii U `00050000` base, `00050002` demo, `0005000E` update, `0005000C` DLC, `00050010`/`0005001B`/`0005004E`/`00070002`/`00070008`/`00000007` system; DSi `0003xxxx` → `nds` base unless the low byte says system (`00030015`, `00030017` system). Unknown high values → `TitleKind::Unknown`, no guess. | Research C4/C5. A bit test would file every 3DS update as DLC. |
| D15 | Huge items | An item is **huge** when it alone writes ≥ `LARGE_TRANSFER` (10 GB) to the target (the same counting as `large_transfer`: extraction bytes always, as-is bytes when copying or crossing volumes), **or** when it extracts a folder game from an archive (D20, any size). `large_transfer` gains `huge: Vec<HugeItem { item, name, system, bytes, extract }>`. The aggregate rule of batch 3 D23 is unchanged for plans with no huge item. | Brief: per-item prompt; folder extraction «con confirmación». |
| D16 | Per-item gate | IPC `organize_apply(plan_id, decisions, confirm: Option<LargeConfirm { bytes, items: Vec<usize> }>)` (replaces `confirmed_bytes`; old shape still accepted as `{bytes, items: []}` for one release). The backend refuses (`IpcError::LargeTransfer`, now with `huge`) unless every huge item of the **decided** plan is in `confirm.items` and `confirm.bytes ≥ bytes`. «No» is sent as `Choice::Leave` for that item with the new `LeftReason::NotConfirmed`, so the decided plan the gate checks already excludes it; the UI computes `bytes − Σ declined` and sends it. «Sí a todo» accepts the current and every later huge item at once. A plan that grew after the answers is refused again. `resume_organize` never asks again. | One source of truth (batch 3 D23), no second numbering. |
| D17 | Free space in the prompt | Each per-item dialog shows the item's size, the target volume and the free space **after the items already accepted in this sequence** («Quedarán {free} libres»). When the item does not fit, the «Sí» buttons are disabled and the body says so («No cabe: faltan {missing}»); «No» stays. Unknown free space (network share) → the line is omitted. | The question is only useful if the number is current. |
| D18 | CLI prompt | `coverly organize … --apply` on a TTY asks per huge item `Copy "<name>" (<size>; <free> free on <volume> after this)? [y]es / [a]ll / [n]o:`; `--yes-large` answers «all» without asking; new `--skip-large` answers «no» to all (they are left `NotConfirmed`); without a TTY and without either flag it refuses exactly as today (exit code unchanged). `--yes-large` and `--skip-large` together are a usage error. | Brief: also in the CLI; scripts keep working. |
| D19 | Title and folder duplicates act | `ExactKey::Title` stops being report-only: `is_confirmed()` stays `false` (it is not a byte proof) but the new `ExactKey::acts_unconfirmed()` returns `true` for `Title` when **every** member has the same title id, kind, version (both `None` counts as equal) and disc, and no member is a `part`/`Update` of another kind. Such a group acts like a version group (keeper by the user's keep rules; loser **set aside to the quarantine or the Trash per `dedupe.quarantine`**, journaled, undoable), at `VERSION_CONFIDENCE`, with the reason «Mismo title ID y versión» visible on the group, and can be skipped per group as today. New `ExactKey::FolderTree { digest }` for folder games: SHA-1 of the sorted `(lower-case relative path, size)` list from the metadata-only tree summary (batch 2 D27); equal digests in one system act at `EXACT_CONFIDENCE` (no file is read). Folder games with the same title key but different trees act through `Title`. Both apply in the Duplicates screen, `coverly dedupe` and Organize's `SetAside` mode. `dedupe/keys.rs` `FACTS_VERSION` 2 → 3. | Brief: through the existing system, not just reported. Undo makes the default safe; the tree digest is the «by folder» signal. |
| D20 | Folder games in archives | New trait `ListingFs` (`list(rel) -> Option<Vec<String>>`, `meta(rel) -> Option<EntryMeta { dir, file, link, len }>`, `read(rel, cap, budget) -> Option<Vec<u8>>`); `DirListing` gains `fs: &dyn ListingFs` and `dir` becomes the root inside it. `DiskFs` wraps today's calls unchanged; new `ArchiveFs` (in `organize/inspect.rs`) builds a virtual tree from `file_entries` (names normalised `\` → `/`, entries with `unsafe_name` dropped, case kept, lookups case-insensitive as the probes already do) and reads entries through `read_entry` under `Budget::plan_head`. Every probe moves to the trait; no probe changes its logic. | Brief: existing probes on a virtual listing, template `vpk_console`. |
| D21 | Roots | Markers per §8.4. Each entry whose path ends in a marker gives a candidate root (its prefix). 0 roots → not a folder game (today's behaviour). 1 root → `identify_dir` on the virtual listing at that root; a `DirConflict` → review. ≥ 2 roots → review «El comprimido contiene {count} juegos». A root deeper than 3 levels is ignored. Encrypted zip names / `he` 7z → no listing → today's behaviour. | `zipped-folders-multidisc.md` §1.2 algorithm. |
| D22 | PS4 vs Vita in archives | The `sce_sys` probe already splits by `TITLE_ID`/`CATEGORY`; on `ArchiveFs` it works unchanged. When the `param.sfo` read exceeds the budget (solid 7z) the result is «structure only»: PS3 JB/HDD and Wii U loadiine/NUS still decide from names; an `sce_sys/param.sfo` + `eboot.bin` root goes to review with both `ps4` and `psvita` as alternatives («Hay que leer param.sfo para saber si es de PS4 o de Vita»). | Research C4: structure alone cannot tell them apart. |
| D23 | Actions | Vita (zip, vpk): **as is**, to `psvita/`; a Vita 7z gets the note «Vita3K solo instala .zip, .vpk o .vci» (Coverly does not recompress). PS3 disc (JB), PS4, Wii U loadiine/NUS: `Transfer::Extract` of the root subtree only into `<system>/<root folder name or archive stem>/`, the archive then handled by `ArchiveAfter` as for any extraction; always a huge item (D15). PS3 HDD/PSN (`<ID>/USRDIR/EBOOT.BIN` with `CATEGORY` `HG`/`DG` non-disc): review, default Leave, note `InstallFirst { emulator: "RPCS3" }`. PS5 keeps batch 3 D13 (as is). Profile container overrides still win when the user set them. | `zipped-folders-multidisc.md` §1.3. |
| D24 | Loose multi-disc | New `organize/discsets.rs` after identification: candidates = same system, same source folder, same name key (title with the disc tag `(Disc|Disk|CD|GD)\s+(\d+|[A-Z])(\s+of\s+\d+)?` removed, every other tag kept). Confirmed when one of: GC/Wii same 6-char id and distinct `disc_no`; Saturn/Dreamcast same `m` and distinct `n` (`DiscIdentity` keeps `count: Option<u8>`); XBE `DiscNumber` / XEX `disc_number`+`disc_count` with the same title id; DAT hash matches whose names minus the disc tag are equal. Name-only candidates (Sega CD, CHD, no DAT) → review «Parece un juego de varios discos». Rejected when two members share a disc number (dedupe handles it), region/Rev differ, or a header contradicts the name. Confirmed sets keep their file names, go to the same folder, and get `<title>.m3u` (reusing `m3u_text`) when the profile's `disc_layout.m3u_for_multi()`; missing discs → the m3u lists only those present and the set gets the note «Falta el disco {n}». PS2, Xbox and Xbox 360 sets get no m3u. DuckStation `discsets.yaml` is not used or bundled (CC BY-NC-ND). | `zipped-folders-multidisc.md` §2.4. |
| D25 | Credits file | One file `credits/credits.json` (+ `credits/credits.schema.json`), read by: the app (`import` in the front end, `include_str!` in `coverly-core` for the tests and `open_credit`), `scripts/credits/readme.mjs` (writes the block between `<!-- credits:start -->`/`<!-- credits:end -->` in `README.md` and `README.es.md`; `--check` in `check-all`), and the landing by copy (like `design/tokens.json`). Strings are in the file as `{ "es": …, "en": … }`; the tab's chrome stays in `locales/`. | Single source of truth. |
| D26 | Entry shape | `{ id, group, name, url, use: {es,en}, license: { spdx?, name, url? }, credit?: {es,en}, required: bool, hosts?: [..], datasets?: [..], sources?: [..] }`. `group` ∈ `art-sources`, `databases`, `docs`, `conventions`, `libraries`, `fonts-icons`. `credit` is the exact required text when `required`. URLs are HTTPS official site or repo; a schema test rejects query strings and a denylist of ROM, key and emulator hosts. | `TODO.md` §7. |
| D27 | Link opening | New IPC `open_credit(id)`: the backend looks the id up in its compiled copy of `credits.json` and opens that `url` (or `license.url`) with the opener; any other id → `BadRequest`. The front end never passes a URL. `LinkId::Notices` now opens the bundled notices in the app (D29) instead of GitHub. | Keeps the `LinkId` allowlist model. |
| D28 | Notices generation | `pnpm notices` (`scripts/licenses/notices.mjs`): `cargo about generate --locked about.hbs` with `about.toml` (targets macOS/Windows/Linux, `ignore-build-dependencies`, `ignore-dev-dependencies`, workspace crates excluded) → `LICENSES/generated/rust.txt`; `pnpm licenses list --prod --json` + each package's own licence file from `node_modules` → `LICENSES/generated/js.txt`. Each file starts with `lock-sha256: <sha256 of Cargo.lock | pnpm-lock.yaml>`. The generated files are committed. `pnpm notices:check` (in `check-all`, needs no tool) fails when the hash no longer matches the lock file. The release script runs `pnpm notices` before `tauri build`. | Build-time notices without making every `pnpm check-all` need cargo-about. |
| D29 | Bundle | `tauri.conf.json` `bundle.resources` adds `../LICENSES/` (all `*.txt` and `generated/`) and `../src/assets/fonts/OFL-*.txt`. The tab's «Ver licencias de terceros» opens an in-app viewer that loads both generated files as a lazy chunk (`?raw` dynamic import), so it works in the browser mock too. | Compliance gap §6.1. |
| D30 | Credit test | Rust `credits` test module: (a) every `https://<host>` literal in non-test code under `crates/*/src` and `src-tauri/src` maps to a credit with that host (test hosts `x`, `evil`, `127.0.0.1`, `localhost` excluded; hosts belonging to the project, `github.com/MAECLY`, excluded); (b) every `KNOWN_SOURCES` id and every catalogue (`titledb`, `titledb-ncas`, `nlib`) names a credit in `sources`/`datasets`; (c) every distinct DAT origin in `systems.toml` (`dats` paths' first segment: `no-intro`, `redump`, `tosec`, `mame`, `fbneo`, `dat`…) has a dataset entry; (d) required credits named by the brief exist with their exact licence (ScreenScraper `CC-BY-NC-SA-4.0`, RetroArch Assets `CC-BY-4.0`, GameTDB, libretro-database `CC-BY-SA-4.0`, titledb `MIT`). Vitest: es/en parity of every `use`/`credit`, and every group non-empty. | `TODO.md` §7 test. |
| D31 | Notices file fixes | `THIRD-PARTY-NOTICES.md`: drop TheGamesDB and the `docs/09-legal/` per-source pointer; add titledb (MIT, © 2019 Blake Warner), api.nlib.cc (only when the fallback is on), GameTDB link, libretro-database CC BY-SA, shadcn/ui MIT, the crates of `credits-inventory.md` §6.4 (wry, reqwest, brotli, option-ext MPL-2.0 with its source URL, webpki-roots CDLA-Permissive-2.0, ring Apache-2.0 AND ISC with BoringSSL, ICU4X Unicode-3.0) and a pointer to `LICENSES/generated/`. `docs/09-legal/license.md` loses its TheGamesDB line. | §6.2–6.5. |
| D32 | RetroArch notice in the UI | The tab shows the CC BY 4.0 notice verbatim in its «Tipografías e iconos» group (it is the entry's `credit`), and `SystemIcon`'s tooltip-free surfaces are unchanged. | Required in the UI; one place is enough. |
| D33 | Memoised facts | `organize/plan.rs` `FACTS_VERSION` 8 → 9 (`ConsoleId.part`, `title_source`, `TitleKind::Homebrew`, archive folder games, disc `count`). | Batch 2 D23. |

## 5. Architecture and files touched

### 5.1 Rust

| File | Change |
|---|---|
| `consoles/mod.rs` | `TitleKind::Homebrew`; `ConsoleId.part`, `ConsoleId.title_source`; `PartKind`; `IdSource::{NcaCatalog, Tmd, Npdm, Xbe, Xex, Self_, Pbp}`; `ListingFs`, `DiskFs`, `EntryMeta`; `DirListing.fs`; `identify_one` calls `loose::identify`. |
| `consoles/loose.rs` (new) | §8.2 readers: NPDM, `.nca` name, DOL structure, RPX/RPL ELF, TMD, `.psvimg` path, SCE/SELF, PSP `~PSP`/PBP. (`.nro` NACP title, `.xbe`/`.xex` loose and `.3dsx` stay in `nx.rs`/`xbox.rs`/`ctr.rs`, extended.) |
| `consoles/nx.rs` | NRO `ASET` NACP title and icon locator; XCI secure-partition NCA names; NCA ids on `ConsoleId` (`nca_ids: Vec<String>`, skipped when empty). |
| `consoles/xbox.rs` | Loose `.xbe` (homebrew rule §8.2) and `.xex` module flags → kind; `DiscNumber`/`disc_count` on `ConsoleId.disc_no` + new `disc_count`. |
| `consoles/sony.rs`, `wiiu.rs`, `xbox.rs` probes | Port to `ListingFs` (no logic change). |
| `disc/mod.rs`, `disc/sega.rs` | `DiscIdentity.count`. |
| `catalog/mod.rs`, `catalog/titledb.rs`, `catalog/nlib.rs` (new) | `SwitchCatalog { ensure(), title(tid), by_nca(id), icon_url(tid), status(), remove() }` over `Cache`; streaming reducer (D5, D6); nlib client (D9). |
| `net/mod.rs` | `HttpRequest.if_modified_since`, `HttpResponse.last_modified`, `Http::get_to_file(req, path)` (streams, cap, same retries); `net/hosts.rs` constants per host used by the credit test. |
| `cache/mod.rs` | Tables `titledb`, `ncas`, `catalog_meta(name, etag, last_modified, checked_at, file, rows)`, `nlib(tid, name, publisher, release, fetched_at, miss)`. |
| `settings/model.rs`, `validate.rs`, `patch.rs` | `CatalogSettings { switch: SwitchCatalogSettings { enabled, icons, refresh_days, fallback, fallback_url } }` (serde default, off). |
| `organize/inspect.rs` | `ArchiveFs`; `archive_folder_game(path, limits) -> Option<ArchiveFolderGame { root, matched, structure_only }>` generalising `vpk_console` (which becomes a call to it). |
| `organize/discsets.rs` (new) | D24. |
| `organize/plan.rs`, `target.rs`, `evidence.rs` | `LeftReason::{GamePart, NotConfirmed}`; `ItemNote::{Homebrew, Vita7z, MissingDisc { n }, Encrypted unresolved label}`; archive folder-game actions (D23); disc sets; `FACTS_VERSION` 9. |
| `organize/apply.rs` | `HugeItem`, `large_transfer().huge`, `LargeConfirm`, `gate(plan, confirm) -> Result<(), LargeTransfer>` shared by IPC and CLI. |
| `dedupe/keys.rs`, `group.rs`, `units.rs` | `acts_unconfirmed`, `FolderTree` digest, folder tree summary reuse, `FACTS_VERSION` 3. |
| `credits.rs` (new, `coverly-core`) | `include_str!("../../../credits/credits.json")`, `Credit`, `credit(id)`, tests D30. |
| `crates/coverly-cli/src/organize.rs` | D18 prompt (`std::io::IsTerminal`), `--skip-large`; English text for the new reasons and notes. |
| `crates/coverly-cli/src/main.rs`, `catalog.rs` (new) | `coverly catalog switch status|update|remove` (respects the settings switch; `update` with the switch off is an error that says how to turn it on). |
| `src-tauri/src/ipc.rs`, `state.rs`, `error.rs`, `prefs.rs` | `organize_apply(…, confirm)`; `IpcError::LargeTransfer { …, huge }`; `catalog_status`, `catalog_update`, `catalog_remove`; `open_credit(id)`; `LinkId::Notices` routed to the viewer. |
| `src-tauri/tauri.conf.json` | Resources (D29). |
| `about.toml`, `about.hbs` (new, repo root) | cargo-about config and template. |

### 5.2 Front end

| File | Change |
|---|---|
| `credits/credits.json`, `credits/credits.schema.json` (new) | D25, D26. |
| `src/lib/credits.ts` (new) | Typed import, grouping, search over name/use/licence in the active locale. |
| `src/components/settings/CreditsSection.tsx` (new), `NoticesDialog.tsx` (new) | §9.1. |
| `src/components/settings/SourcesSection.tsx` | «Catálogo de Switch» group (§9.2). |
| `src/stores/navStore.ts`, `src/screens/SettingsScreen.tsx`, `registry.ts`, `search.ts` | Section `credits` after `about`; registry entries `setting-credits-search`, `setting-credits-notices`, family `credit-` (one per entry), `setting-sources-switch-catalog*`. |
| `src/components/organize/LargeTransferDialog.tsx` → `HugeItemDialog.tsx` | Per-item sequence (§9.3); the aggregate variant stays for plans without huge items. |
| `src/stores/organizeStore.ts` | `pendingLarge` gains a cursor over `huge`, `answer(item, 'yes'|'all'|'no')`, builds `Leave` decisions and `LargeConfirm`. |
| `src/ipc/types.ts`, `backend.ts`, `tauriBackend.ts`, `contract.sample.json`, `contract.test.ts` | New fields and calls; `TitleKind` `homebrew`; `LeftReason` `game-part`, `not-confirmed`; notes; `IdSource` values. |
| `src/ipc/mock/*` | §9.4. |
| `src/lib/organize.ts`, `src/components/organize/*Row*.tsx`, library tile | «Cifrado» label (D11), «Homebrew» note, disc sets, archive folder games. |
| `locales/{es,en}/ui.json` | §10. |
| `scripts/licenses/notices.mjs`, `scripts/credits/readme.mjs` (new), `package.json` | `notices`, `notices:check`, `credits:readme`, `credits:check`; `check-all` gains `notices:check` and `credits:check`. |
| `THIRD-PARTY-NOTICES.md`, `docs/09-legal/license.md`, `README.md`, `README.es.md` | D31, D25. |

## 6. Identification flow (Switch, adjustment 1)

1. `nx::identify` as today → title id from ticket name, `cnmt.xml`, file-name tokens; new NCA id list (loose name or XCI secure names).
2. No title id and NCA ids present and catalogue on → `SwitchCatalog::by_nca` (first id with a hit; `content_type` Meta preferred). Title id + `IdSource::NcaCatalog`.
3. Title id present → title by D7. Kind from the id itself (`0x800` bit = update; low 13 bits ≠ 0 and not `0x800` = DLC; batch 2 rules unchanged).
4. Still no title → `encrypted = Some(true)` → «Cifrado» (D11).

Nothing in steps 1–4 opens a `.tik`, `.cert`, `.nca` or `.ncz` body.

## 7. Catalogue lifecycle

| State | Trigger | Result |
|---|---|---|
| Off | default | No request; status «Desactivado». |
| On, never downloaded | toggle on, or «Descargar ahora», or first plan/scan | Download (progress in the Sources group, bytes in mono), reduce, store; status «{count} títulos, actualizado el {date}» (§10). |
| On, fresh (< `refresh_days`) | any use | No request. |
| On, stale | any use, or «Buscar actualización» | Conditional GET (D4); 304 keeps rows. |
| On, failed | network or parse error | Old rows kept; status «No se pudo actualizar; se usan los datos del {date}» or, with no rows, «No se pudo descargar; se usan los nombres de archivo». |
| Removed | «Borrar catálogo» or toggle off | Tables `titledb`, `ncas`, cached icons and `nlib` rows deleted; the toggle off asks nothing (the data is a cache). |

## 8. Classification details

### 8.1 3DS (unchanged readers)

`ctr.rs` already reads only NoCrypto NCCH ExeFS icons and CIA Meta SMDH. This batch adds only D11's label and a regression test that an encrypted NCCH with a valid SMDH-looking ExeFS is **not** read.

### 8.2 Loose odd files (D12–D14)

| Input | Check | System | Kind / part |
|---|---|---|---|
| `.nro` | `NRO0` @0x10; `ASET` @ size; NACP title (first non-empty language), DisplayVersion | `switch` | Homebrew; icon locator (JPEG) |
| `.nso` | `NSO0` @0 | `switch` | part `ExeFs` |
| `main.npdm` (name) | `META` @0, ACI0 @AciOffset, ProgramId @+0x10 | `switch` | part `ExeFs`, title id (`IdSource::Npdm`) |
| `<32 hex>.nca`/`.ncz` (name) | name only; catalogue (D6) | `switch` | part `Nca`, title id when found |
| `.dol` | 0x100-byte header: each non-empty section `offset + AlignUp(size, 32) ≤ len`, load addresses and entry in `0x80000000–0x81800000`, ≥ 1 text section | `wii` if a text word matches `(w & 0xFC1FFFFF) == 0x7C13FBA6` within the first 1 MiB decoded budget, else `gamecube` | Homebrew (`boot.dol` under `apps/<name>/` → the folder is the item, D13) |
| `.xbe` (not `default.xbe` inside a probed folder) | `XBEH`; cert at `cert_addr − base_addr`; TitleId, TitleName, AllowedMedia, DiscNumber | `xbox` | Homebrew when TitleId is 0 **or** AllowedMedia has only `HARD_DISK`/`NONSECURE_HARD_DISK`; else Base, part `ExeFs` when siblings exist (it is a game folder the probe missed → review) |
| `.xex` | `XEX2`; module_flags; EXECUTION_INFO title id, disc n/m | `xbox360` | `PATCH_*` flags → Update; `DLL`/`MODULE` → part `Module`; `TITLE` → Base (part `ExeFs` unless a lone homebrew: title id `FFFE07D1`-style dev ids are not guessed, kind Unknown) |
| `.rpx` / `.rpl` | ELF, OSABI `0xCA`, ABIVer `0xFE` | `wiiu` | `.rpx` loose → Homebrew; `.rpl` → part `Library`; `code/*.rpx` beside `meta/` is the loadiine probe's |
| `.tmd` / `title.tmd` | signature type @0; RSA-2048 → title id @0x18C, version @0x1DC | by high value (D14) | part `TitleMeta` (the NUS folder probe owns `title.tmd` inside a folder) |
| `.psvimg` / `.psvmd` | name; title id from a path segment `^[A-Z]{4}\d{5}$` | `psvita` | part `Backup` («copia de seguridad cifrada») |
| `.self`, `eboot.bin` | `SCE\0` @0; header_type 1; version bytes `00 00 00 02` → PS3, `03 00 00 00` → Vita; app_info self_type; NPDRM content id | `ps3` / `psvita` | Vita authid `0x2F00000000000001/2/3` → Homebrew; `eboot.bin` → part `ExeFs`; lone `.self` APP → Homebrew |
| `EBOOT.PBP` / `.pbp` | `\0PBP`; PARAM.SFO `CATEGORY`; PSAR @0x24 `PSISOIMG0000` / `NPUMDIMG` | `psx` (ME / PSISOIMG) or `psp` | PS1 classic Base; PSN ISO Base; else PSP Base, Homebrew when the SFO `CATEGORY` is `MG` and DISC_ID is not `[A-Z]{4}\d{5}` |
| `.prx` with `~PSP` @0 | modname @0x0A | `psp` | part `Module` |
| `.3dsx` | as today | `3ds` | Homebrew |

Every reader stays within batch 2 D24 budgets (4 MiB read, 1 MiB decoded, 16 seeks).

### 8.3 Duplicates (D19)

Group reasons shown: «Mismo archivo» (payload), «Mismo title ID y versión» (title), «Misma carpeta» (tree digest). The keep rules, set-aside target, Trash choice and undo are the existing ones; nothing new is configurable.

### 8.4 Archive folder-game markers (D21)

| System | Marker (case-insensitive) | Root |
|---|---|---|
| PS3 disc | `PS3_GAME/PARAM.SFO` and `PS3_GAME/USRDIR/EBOOT.BIN` | prefix of `PS3_GAME/` |
| PS3 HDD/PSN | `<ID>/PARAM.SFO` and `<ID>/USRDIR/EBOOT.BIN` | prefix of `<ID>/` |
| PS4 / Vita / PS5 | `sce_sys/param.sfo` or `sce_sys/param.json` | prefix of `sce_sys/`; abort Vita on `sce_module/steroid.suprx` |
| Wii U loadiine | `code/*.rpx` with `content/` and `meta/` siblings | prefix of `code/` |
| Wii U NUS | `title.tmd` with `.app` siblings | its folder |
| Xbox / 360 | `default.xbe` / `default.xex` | its folder |

## 9. UX and UI design

### 9.1 «Agradecimientos» tab (frontend-design pass)

**Subject and job.** The audience is the collector who curates a handheld library; the tab's job is to say plainly whose work Coverly stands on and under which terms, and to let the user open the official page or the licence. It is the app's colophon: the page at the end of a library-bound book that lists typefaces, sources and permissions.

**Plan (first pass).**
- Colour: only app tokens. `--cr-bg`, `--cr-surface`, `--cr-text`, `--cr-text-secondary`, `--cr-border`, `--cr-primary` (Buckram) for links and the shelf-rule cap. No Manila here (it means «you can still undo»), no new colour.
- Type: Archivo for the tab heading and group headings (the brand's title face, set at the existing `h2`/`h3` sizes); Atkinson Hyperlegible Next for names (600) and the one-line use (400); Atkinson Hyperlegible Mono **only for numbers** (the group counts and the filter result count), per the Spanish copy rule.
- Layout: one left-aligned column inside the settings panel, max 72 ch. A search field at the top (filters live, shows «{count} de {total}»). Groups in the TODO's order, each headed by the shelf rule (1px border, 3px Buckram cap, brand §2). Inside a group, entries are rows of a definition list, not cards:

```
┃ Fuentes de portadas y metadatos                    5
────────────────────────────────────────────────────────
ScreenScraper                                CC BY-NC-SA 4.0
Medios e identificación por hash en Organizar, con tu cuenta.
│ Datos y medios de ScreenScraper (screenscraper.fr), CC BY-NC-SA 4.0.
Web oficial   Licencia
────────────────────────────────────────────────────────
libretro-thumbnails                           Sin licencia declarada
…
```
- Required credit text is a quotation with a 2px `--cr-border-strong` left rule, in body text (it must be copyable and exact); the row itself carries no «obligatorio» badge — the presence of the quotation is the signal.
- The licence name sits right-aligned on the name line as plain secondary text, linked when it has a URL.
- Footer of the tab: «Ver licencias de terceros» (secondary button) and one sentence: «Coverly no incluye ROMs, BIOS, claves ni arte de juegos».
- Motion: none on load. Filtering removes rows without animation; the notices dialog uses the existing dialog open/close (respects reduced motion).

**Review against generic defaults.** A first instinct was a grid of rounded cards with a licence chip in each (the SaaS-card kit, trait 4) and an all-caps «REQUIRED» eyebrow (trait 5): both rejected — cards chop a reading list into tiles and the badge adds a label above content. A second instinct, monospace licence identifiers (`CC-BY-NC-SA-4.0`), was replaced by the human licence name in body type, because mono is reserved for numbers in this app. Middle-dot meta strings («MIT · 2019 · Blake Warner») were replaced by a sentence in the use line. What stays distinctive is borrowed from the product's own world: the shelf rule as group header and the colophon as a reading page, not a gallery.

**Notices viewer.** A large dialog («Licencias de terceros»), two tabs «Rust» and «JavaScript» with the package counts in mono, a find field that highlights matches and jumps with Enter, and the text in Atkinson Hyperlegible Mono at caption size (it is code licence text, the one place where mono is the content). Focus starts on the find field; Escape closes; focus returns to the button.

**Accessibility.** The search field has a visible label; the result count is an `aria-live="polite"` region; each entry is an `<article>` labelled by its name; links say where they go («Web oficial de ScreenScraper»); keyboard order follows reading order; contrast from tokens (AA).

### 9.2 «Catálogo de Switch» (Sources tab)

A group after the source list, same `Field` primitives:
- Switch «Catálogo de nombres de Switch» + hint «Descarga la lista pública de blawar/titledb (unos 90 MB, se queda en tu equipo) para poner nombre a los juegos de Switch. No usa ni pide claves».
- Status line (§7) with counts and sizes in mono; buttons «Descargar ahora» / «Buscar actualización», «Borrar catálogo».
- Switch «Iconos de Switch» (disabled until the catalogue is on) + hint «Se descargan solo cuando se ven y solo se muestran en Coverly».
- In the Advanced fold: switch «Respaldo en línea» + field «Dirección del respaldo» (default `https://api.nlib.cc`) + hint «Si un juego no está en el catálogo, se pregunta su title ID a este servicio».

### 9.3 Per-item confirmation (D15–D17)

Built on `alert-dialog`. Title «¿Copiar {name}?» / «¿Mover {name} a otro disco?» / «¿Extraer {name}?» (name in body type, size in mono in the body). Body: «Ocupa {bytes}. Quedarán {free} libres en {volume}.» A progress caption «Elemento {index} de {count}» (numbers in mono). Buttons in this order: «No» (cancel style, initial focus), «Sí a todo», «Sí». Escape = «No» for this item. When it does not fit: body «No cabe en {volume}: faltan {missing}.», «Sí» and «Sí a todo» disabled. After the last answer the apply starts; if every item was declined and nothing else moves, the plan state returns with the toast «No se ha movido nada».

### 9.4 Mock

`?mockPick=` gains `huge` (three PS4 folders of 38, 52 and 61 GB on a 120 GB target, one zipped PS3 disc), `switch` (NSP/XCI with and without `[0100…]`, a loose `.nca`, an `.nro`), `odd` (one file per §8.2 row), `discs` (Saturn and GC sets, one name-only Sega CD set, one missing disc) and `dupes-title` (two NSPs with the same id and version, two `CUSA00900` folders with equal trees). `mockSettings` holds the catalogue status and simulates a download in 2 s. The notices viewer loads the real generated files.

## 10. Copy (es / en)

| Key | es | en |
|---|---|---|
| `settings.sections.credits` | Agradecimientos | Credits |
| `settings.credits.intro` | Coverly se apoya en el trabajo de estos proyectos. Aquí tienes qué usa de cada uno, su licencia y el crédito que piden. | Coverly builds on the work of these projects. Here is what it uses from each, its licence and the credit it asks for. |
| `settings.credits.search` | Buscar en agradecimientos | Search credits |
| `settings.credits.results` | {count} de {total} | {count} of {total} |
| `settings.credits.empty` | Nada coincide con «{query}». | Nothing matches “{query}”. |
| `settings.credits.groups.art-sources` | Fuentes de portadas y metadatos | Cover and metadata sources |
| `settings.credits.groups.databases` | Bases de datos y DAT | Databases and DATs |
| `settings.credits.groups.docs` | Documentación y wikis técnicos | Technical docs and wikis |
| `settings.credits.groups.conventions` | Convenciones de frontends y firmwares | Frontend and firmware conventions |
| `settings.credits.groups.libraries` | Bibliotecas | Libraries |
| `settings.credits.groups.fonts-icons` | Tipografías e iconos | Fonts and icons |
| `settings.credits.site` | Web oficial de {name} | {name} official site |
| `settings.credits.license` | Licencia de {name} | {name} licence |
| `settings.credits.noLicense` | Sin licencia declarada | No declared licence |
| `settings.credits.notices` | Ver licencias de terceros | View third-party licences |
| `settings.credits.noContent` | Coverly no incluye ROMs, BIOS, claves ni arte de juegos. | Coverly ships no ROMs, BIOS files, keys or game art. |
| `notices.title` | Licencias de terceros | Third-party licences |
| `notices.find` | Buscar en el texto | Find in text |
| `notices.tabRust` | Rust ({count}) | Rust ({count}) |
| `notices.tabJs` | JavaScript ({count}) | JavaScript ({count}) |
| `settings.sources.switchCatalog.label` | Catálogo de nombres de Switch | Switch name catalogue |
| `settings.sources.switchCatalog.hint` | Descarga la lista pública de blawar/titledb (unos {size}, se queda en tu equipo) para poner nombre a los juegos de Switch. No usa ni pide claves. | Downloads the public blawar/titledb list (about {size}, kept on your computer) to name Switch games. It never uses or asks for keys. |
| `settings.sources.switchCatalog.statusOff` | Desactivado | Off |
| `settings.sources.switchCatalog.status` | {count} títulos, actualizado el {date} | {count} titles, updated {date} |
| `settings.sources.switchCatalog.statusFallback` | {count} títulos de la lista de Estados Unidos, actualizado el {date} | {count} titles from the United States list, updated {date} |
| `settings.sources.switchCatalog.downloading` | Descargando {done} de {total} | Downloading {done} of {total} |
| `settings.sources.switchCatalog.pending` | Descarga del catálogo pendiente | Catalogue download pending |
| `settings.sources.switchCatalog.failedOld` | No se pudo actualizar; se usan los datos del {date}. | Could not update; using the data from {date}. |
| `settings.sources.switchCatalog.failedNone` | No se pudo descargar; se usan los nombres de archivo. | Could not download; using file names. |
| `settings.sources.switchCatalog.download` | Descargar ahora | Download now |
| `settings.sources.switchCatalog.check` | Buscar actualización | Check for update |
| `settings.sources.switchCatalog.remove` | Borrar catálogo | Delete catalogue |
| `settings.sources.switchCatalog.icons` | Iconos de Switch | Switch icons |
| `settings.sources.switchCatalog.iconsHint` | Se descargan solo cuando se ven y solo se muestran en Coverly. | Fetched only when shown, and shown only in Coverly. |
| `settings.sources.switchCatalog.fallback` | Respaldo en línea | Online fallback |
| `settings.sources.switchCatalog.fallbackUrl` | Dirección del respaldo | Fallback address |
| `settings.sources.switchCatalog.fallbackHint` | Si un juego no está en el catálogo, se pregunta su title ID a este servicio. | When a game is not in the catalogue, its title ID is looked up on this service. |
| `organize.encryptedLabel` | Cifrado | Encrypted |
| `organize.encryptedUnresolved` | No se puede leer el nombre sin descifrarlo | The name cannot be read without decrypting it |
| `organize.note.homebrew` | Homebrew | Homebrew |
| `organize.left.gamePart` | Parte de un juego; no se mueve suelta | Part of a game; not moved on its own |
| `organize.left.backup` | Copia de seguridad cifrada de Vita | Encrypted Vita backup |
| `organize.left.notConfirmed` | No confirmado; se queda donde está | Not confirmed; stays where it is |
| `organize.note.vita7z` | Vita3K solo instala .zip, .vpk o .vci | Vita3K only installs .zip, .vpk or .vci |
| `organize.review.archiveGames` | El comprimido contiene {count} juegos | The archive holds {count} games |
| `organize.review.ps4OrVita` | Hay que leer param.sfo para saber si es de PS4 o de Vita | param.sfo must be read to tell PS4 from Vita |
| `organize.review.discSet` | Parece un juego de varios discos | Looks like a multi-disc game |
| `organize.note.missingDisc` | Falta el disco {n} | Disc {n} is missing |
| `organize.why.titledb` | Nombre del catálogo de blawar/titledb | Name from the blawar/titledb catalogue |
| `organize.why.ncaCatalog` | Title ID por el id del NCA en el catálogo | Title ID from the NCA id in the catalogue |
| `organize.huge.titleCopy` | ¿Copiar {name}? | Copy {name}? |
| `organize.huge.titleMove` | ¿Mover {name} a otro disco? | Move {name} to another drive? |
| `organize.huge.titleExtract` | ¿Extraer {name}? | Extract {name}? |
| `organize.huge.body` | Ocupa {bytes}. Quedarán {free} libres en {volume}. | It takes {bytes}. {free} will be left free on {volume}. |
| `organize.huge.bodyNoFree` | Ocupa {bytes}. | It takes {bytes}. |
| `organize.huge.noRoom` | No cabe en {volume}: faltan {missing}. | It does not fit on {volume}: {missing} short. |
| `organize.huge.progress` | Elemento {index} de {count} | Item {index} of {count} |
| `organize.huge.yes` | Sí | Yes |
| `organize.huge.all` | Sí a todo | Yes to all |
| `organize.huge.no` | No | No |
| `organize.huge.nothing` | No se ha movido nada | Nothing was moved |
| `dedupe.reason.titleId` | Mismo title ID y versión | Same title ID and version |
| `dedupe.reason.folderTree` | Misma carpeta | Same folder |

Numbers, sizes and dates render in mono through the existing rich-text helper (`useRichT`), never as plain string concatenation. Credit-entry strings live in `credits.json` (D25); their Spanish follows the same rules (sentence case, no exclamation marks, exact counts).

## 11. Data flow

1. **Catalogue:** Settings switch → `catalog_update` (IPC) or first plan → `SwitchCatalog::ensure` → `Http::get_to_file` → streaming reducer → cache tables → status event → Sources group. Identification asks `SwitchCatalog::title/by_nca` synchronously against SQLite (indexed).
2. **Organize plan:** walk → per file `identify_all` (with `loose`) / per folder `identify_dir(DiskFs)` / per archive `archive_folder_game(ArchiveFs)` → evidence → `discsets` → dedupe (`acts_unconfirmed`, `FolderTree`) → targets → `large_transfer` (with `huge`) → plan.
3. **Apply:** UI → `organize_apply(plan, decisions, None)` → `LargeTransfer { huge }` error → per-item dialogs → `organize_apply(plan, decisions + Leave(NotConfirmed), Some(LargeConfirm))` → gate on the decided plan → apply job (journal, undo).
4. **Credits:** `credits.json` → front-end import → `CreditsSection`; link click → `open_credit(id)` → opener. Notices: button → dynamic import of `LICENSES/generated/*.txt` → viewer.

## 12. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| titledb answers 200 with HTML (GitHub outage page) | Streaming parse fails → old rows kept, status failed; temp file removed. |
| Region file > 160 MB | Refused by the cap; status failed; no partial import (the import runs in one SQLite transaction). |
| Disk full while downloading | Temp file removed, error status; library untouched. |
| Entry with `id` not 16 hex, or `name` empty | Skipped. |
| Same id in several entries | First non-empty name wins (D5). |
| Catalogue toggled off during a download | Download cancelled, tables cleared. |
| Locale changes es → en | The next refresh downloads `US.en.json` and replaces the rows (only one file kept); until then names stay Spanish. |
| nlib returns 429 | Limiter backs off; the plan continues with the file name. |
| nlib URL set to `http://` | Rejected by validation («Tiene que empezar por https://»). |
| `iconUrl` on another host | Dropped at import. |
| XCI with > 1024 secure entries | Distrusted (existing `MAX_ENTRIES`); no NCA ids. |
| Loose `.nca` with a non-hex name | No NCA id; left `GamePart { Nca }` without title. |
| `.tmd` with an ECDSA signature | Header offset from the signature-type table; unknown type → not identified. |
| `.dol` that passes structure but is data | Addresses outside the RAM window → not a DOL. |
| PBP whose PARAM.SFO is beyond budget | Kind Unknown, system `psp` from the magic only, review. |
| `.self` with an unknown version word | Not identified (no PS3/Vita guess). |
| Title-id duplicates where one copy is an NSZ and the other an NSP | Same key → acts; keeper by keep rules (format preference applies). |
| Title-id duplicates with different versions | Different keys → no group (versions are batch 2 D11's update grouping). |
| Two folder games with equal trees but different system probes | Different systems → never grouped. |
| User undoes an organize that set aside a title duplicate | Existing undo restores it. |
| Zip with `PS3_GAME/` at two depths | Two roots → review «El comprimido contiene 2 juegos». |
| Zip of a Vita game with a `sce_module/steroid.suprx` | Vita3K aborts on it → review with note. |
| Solid 7z of a PS4 game, SFO beyond 64 KiB decode | Structure only → review ps4/psvita (D22). |
| Archive folder game extracted into a target that already has the folder | Existing `rename_dir_noclobber` / collision rules. |
| All huge items declined | Nothing huge moves; the rest of the plan applies; aggregate gate re-evaluated on the rest. |
| Free space changes between dialog and apply | Apply-time `check_space` still refuses (`ApplyError::NoSpace`). |
| CLI `--skip-large` with no huge items | No effect. |
| Multi-disc set where disc 2 is a CHD and disc 1 a CUE | Allowed; m3u lists the `.cue` and the `.chd`. |
| `(Disc 0)` opening disc | Extra member; does not break 1..m. |
| Sega CD set by name only | Review; never auto. |
| `credits.json` entry missing `en` | Vitest parity fails. |
| A new provider host added without a credit | Rust D30 test fails naming the host and the file. |
| Lock file changed without `pnpm notices` | `notices:check` fails with «run pnpm notices». |
| cargo-about not installed when running `pnpm notices` | Script stops with the install command; committed files untouched. |

## 13. Owner decisions (implemented with a default, listed for the PR)

- **Q1** Ids missing from the locale file: also download `US.en.json` (a second ~90 MB) and merge? Default: no, one file only (D3).
- **Q2** Let Fill write titledb icons as covers into frontend folders? Default: no, in-app only (D8).
- **Q3** nlib fallback default off (D9); turn it on by default?
- **Q4** Title-id duplicates act by default (D19). Alternative: act only after the user ticks the group.
- **Q5** Folder-game extraction from archives is always asked per item (D15), whatever its size.
- **Q6** Homebrew goes into the system folder with a note (D13); some profiles (muOS, Batocera) have an apps/ports folder — map it there instead?
- **Q7** muOS `.hidden/` multi-disc layout stays unverified (research C12); no muOS-specific m3u layout is written.
- **Q8** SteamGridDB terms, No-Intro DAT-o-MATIC, Redump and TOSEC terms are still unverified (`credits-inventory.md` §7): their entries say «Crédito de cortesía» in `use` and carry no `credit` text until verified.
- **Q9** Notify GameTDB about the project (their FAQ asks for a link; done in the tab and notices).
- **Q10** LGPL notice for GTK/WebKitGTK inside the AppImage and WebView2Loader on Windows remain unverified; the generated notices cover crates only.

## 14. Test plan

### 14.1 Rust (`cargo test --workspace --no-fail-fast`)

- `catalog/titledb.rs`: reducer on a fixture of 6 entries (valid, no id, bad id, duplicate id, empty name, one with `key`/`rightsId`/`description`) → 3 rows, no stored `key`/`rightsId` column (schema test); `releaseDate` → ISO; icon URL only with icons on and the allowed host; 304 path keeps rows and moves `checked_at`; 404 on `ES.es` falls back to `US.en`; HTML body fails without touching rows; size cap; transaction rollback.
- `ncas` reducer: 32-hex keys, content type, bad rows skipped; lookup by NCA name.
- `net`: `If-Modified-Since`/`Last-Modified` round trip and `get_to_file` streaming with the loopback test server (`net/testing.rs`).
- `nlib`: fixture of the live response shape (captured in T3), misses cached.
- `nx.rs`: XCI secure names → NCA ids; NRO NACP title and version; nothing reads a `.tik`/`.nca` body (a `SparseAt` that panics on any read inside those ranges).
- `loose.rs`: one positive and one near-miss per §8.2 row (built in `consoles/testing.rs`); D14 table exhaustively, including `0004000E` → Update and `0004008C` → DLC; Vita authid homebrew; PBP PS1 vs PSP vs PSN.
- `no_keys.rs` (D1).
- `ListingFs`: every existing probe test runs twice, on `DiskFs` and on an `ArchiveFs` built from the same tree zipped (zip and 7z, fixture mtimes pinned, paths compared by component).
- `archive_folder_game`: PS3 JB, PS3 HDD, PS4, Vita, Wii U loadiine, two roots, root too deep, solid 7z beyond budget → structure only, steroid abort.
- `discsets.rs`: GC 2-disc, Saturn 3-disc with `CD-1/3…`, DC `GD-ROM1/2`, XEX 2-disc, DAT-hash PS1 set, name-only Sega CD → review, duplicate disc number → rejected, region mismatch → rejected, missing disc → note and m3u of present ones; `m3u_text` output byte-exact.
- `dedupe`: title group acts (set aside, Trash mode, undo restores), version `None`/`None` acts, different versions do not group, tree digest equal → acts, different systems never; `FACTS_VERSION` bump invalidates.
- `organize/apply.rs`: `huge` list (≥ threshold single item, folder extraction of any size), gate accepts full confirmation, refuses a missing id, refuses a lower `bytes`, accepts after declines (`Leave(NotConfirmed)`), legacy `confirmed_bytes` shape.
- `cli_organize.rs`: TTY simulation through a pty helper (`y`, `a`, `n` sequences), `--yes-large`, `--skip-large`, both flags → usage error, non-TTY refusal unchanged; `coverly catalog switch status` with the switch off.
- `credits.rs` (D30 a–d).
- Real data (read-only, copies under `target/` with `cp -RXp`, deleted afterwards, no `._` files): the Wii U loadiine folder and `.wua`, the `.3ds`/`.zip` pair of «Hatsune Miku - Project Mirai DX (USA)» (expected: one duplicate group), the PS1 cue/bin set (no false multi-disc grouping).

### 14.2 Vitest (`pnpm check-all`)

- `credits.test.ts`: schema, es/en parity, groups non-empty, no denied hosts, required entries present.
- `CreditsSection.test.tsx`: groups in order, search filters and announces the count, empty state, link buttons call `openCredit(id)` (never a URL), RetroArch notice text exact.
- `NoticesDialog.test.tsx`: tabs, find, focus return.
- `registry.test.tsx`: new entries; search lands on a credit row.
- `HugeItemDialog.test.tsx` + `organizeStore.test.ts`: sequence order, «Sí a todo», «No» adds `Leave`, `bytes` sent = total − declined, no-room disables «Sí», Escape = «No», all declined → toast.
- `SourcesSection.test.tsx`: catalogue states (off, downloading, ok, fallback, failed old/none), icons disabled while off, fallback URL validation.
- `contract.test.ts`: new IPC shapes; `localeGuard.test.ts`, locale parity.

### 14.3 E2E (Chrome DevTools MCP, isolated context `adjustments`, dev server on port **1468**)

1. `?mockPick=huge`: apply → three dialogs; answer «No», «Sí», then «Sí a todo» on the third; check free-space numbers decrease; plan result shows one item «No confirmado».
2. `?mockPick=switch` with the catalogue on (mock): names from the catalogue, the `[0100…]`-less XCI resolved by NCA id, one item labelled «Cifrado».
3. `?mockPick=odd`: each §8.2 row shows its system and kind or «Parte de un juego».
4. `?mockPick=discs`: Saturn set grouped with m3u target, Sega CD in review, missing-disc note.
5. `?mockPick=dupes-title`: duplicates set aside, undo restores.
6. Settings → Agradecimientos: search «retroarch», open notices, find a crate name, Escape returns focus; ⌘F settings search «ScreenScraper» lands on its row; es ↔ en switch.
7. Screenshots light/dark at 1280 and 760 px wide for the tab and the dialog; keyboard-only pass.
Fallback: Playwright MCP; if both profiles are locked, `playwright-core` in the session scratchpad driving Chrome for Testing. Servers stopped by port only.

### 14.4 Gates

`pnpm check-all` (now with `notices:check` and `credits:check`), `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`.

## 15. Tasks (granular conventional commits, `git push` after every commit)

| # | Commit scope |
|---|---|
| T1 | `docs: add the adjustments design spec` (this file) |
| T2 | `feat(core): conditional GET with Last-Modified and streaming downloads` |
| T3 | `feat(core): optional Switch name catalogue from blawar/titledb` (+ ncas, nlib fixture) |
| T4 | `feat(core): Switch NCA ids from XCI names and NRO NACP titles` |
| T5 | `feat(core): classify loose executables and title metadata by header` |
| T6 | `feat(core): label unresolved Switch and 3DS titles as encrypted` + `test(core): guard against key handling` |
| T7 | `refactor(core): run folder probes over a listing abstraction` |
| T8 | `feat(core): detect folder games inside zip and 7z archives` |
| T9 | `feat(core): group loose multi-disc games` |
| T10 | `feat(core): act on title id and folder duplicates` |
| T11 | `feat(core): per-item confirmation for huge transfers` + `feat(cli): ask per huge item` |
| T12 | `feat(ipc): catalogue, credits and per-item confirmation commands` |
| T13 | `feat(ui): Switch catalogue settings` |
| T14 | `feat(ui): per-item confirmation dialog` |
| T15 | `feat(credits): single credits data file with coverage test` |
| T16 | `feat(ui): Agradecimientos settings tab and notices viewer` |
| T17 | `build: generate third-party notices and bundle licence texts` |
| T18 | `docs: fix third-party notices and README credits` |
| T19 | `test(e2e): adjustments flows` (evidence in the PR) |

Every merge from `main` is `git fetch origin && git merge origin/main`, never a rebase or a force-push.

## 16. Sources

All sources are those of the five research files listed in the header, fetched 2026-11-01: switchbrew (NRO, NSO, NPDM, NCA, CNMT, XCI), wiibrew (DOL, TMD, Homebrew Channel), wiiubrew (TMD, RPL, meta.xml, Title database), 3dbrew (TMD, Titles, NCCH, CIA, SMDH, 3DSX), xboxdevwiki and Cxbx-Reloaded `Xbe.h`, Xenia `xex2_info.h`, decaf-emu `cafe_loader_rpl.h`, scetool `sce.h`/`sce.cpp`, vita-toolchain `vita-make-fself.c`, PPSSPP `Loaders.cpp`/`PBPReader.cpp`/`sceKernelModule.cpp`, psvimgtools, Dolphin `DolReader.cpp`, blawar/titledb (`LICENSE`, README, `US.en.json`, `ncas.json`), ghost-land/nlib-api README, Vita3K `interface.cpp`, RPCS3 `System.cpp`, Cemu `TitleInfo.cpp`, ES-DE USERGUIDE, DuckStation `LICENSE`, Sega Saturn programmer's guide, mc.pp.se IP0000.BIN, libretro-database `LICENSE`, ScreenScraper `webapi2.php`, RetroArch Assets `LICENSE`, GameTDB FAQ, the Yuzu consent judgement (1:24-cv-00082 D.R.I.).

## 17. Self-review

- **Placeholders:** none; every open question has a default (§13) and every new name has a home (§5). The nlib response shape is the one unverified technical fact; T3 pins it with a live capture before the parser is written (D9).
- **Contradictions checked:** D3 «one file» vs brief «US.en fallback» — reconciled as fallback when the locale file is unavailable, merging listed as Q1. D8 icons «local cache only» vs Fill — Fill never writes them. D15 folder extractions always huge vs batch 3 D23 aggregate — the aggregate stays only for plans with no huge item, so a plan is asked once per item or once overall, never both. D19 «acts» vs `is_confirmed() == false` — kept separate (`acts_unconfirmed`), so code that relies on byte proof (e.g. hard-link twins) is unaffected. D13 `.psvimg` left vs «classify» — classified (system, title id) and left, as research §7 recommends. D1 key test vs `is_key_material` name list — the list is the one allowed place.
- **Scope:** adjustment 3, launchers, §7.1 sources and user keys are excluded in the header and not referenced by any task.
- **Brief coverage:** (1) D1–D11, §6–7; (2) D12–D14, §8.2; (4) D15–D18, §9.3; (5) D19, §8.3; (6) D20–D24, §8.4; (7) D25–D32, §9.1; compliance gaps §6.1–6.5 → D29, D31, D32; required credits → D30 d.
- **Brand rules:** mono only for numbers (and the licence text viewer, where the text is code licences); no Manila outside its meaning; sentence case, no exclamation marks in the copy table.
