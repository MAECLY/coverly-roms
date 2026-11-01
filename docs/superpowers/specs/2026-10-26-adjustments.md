# Coverly ROMs — Adjustments after systems batches 1–3 (adjustments 1, 2, 4, 5, 6 and 7) (Design Spec)

> Status: **Draft for implementation on `feat/adjustments`** (self-reviewed §17, adversarial review against the code §18) · Date: 2026-10-26 (research and spec run 2026-11-01) · Owner: Miguel Angel Esparza Calero (MAECLY)
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
| 3DS reader | `consoles/ctr.rs` | Already keyless: NCSD/NCCH headers, CIA header + TMD, CIA Meta SMDH, ExeFS icon on `NoCrypto` only, `encrypted: Some(true)` otherwise (plus `ItemNote::EncryptedContent { emulator }`, «Cifrado; algunos emuladores no pueden cargarlo»). 3DSX with SMDH. No change to what is read; only the «cifrado» label (D11). |
| Loose readers that already exist | `consoles/wiiu.rs` (`.tmd` by extension through `tmd_id`, **Wii U high values only**: a 3DS TMD returns `None`; `.rpx` ELF magic-only verdict «RPX header»), `consoles/xbox.rs` (`XBEH`/`XEX2` parsed under any extension, before this batch's reader would run), `systems.toml` (`nro`/`nso`/`nca` weak for `switch`, `xbe` weak for `xbox`, `pbp` shared by `psx` and `psp`) | §8.2 extends these readers in place; it does not add a second TMD/RPX/XBE/XEX parser that the earlier readers would shadow. |
| `identify_one` order | `consoles/mod.rs` | `nx` → `nintendo_disc` → `wad` → `ctr` → `wiiu` → `sony` → `orbis` → `xbox`, first `Some` wins. |
| Key guard | `consoles/mod.rs` `is_key_material`, `KEY_EXTS`; `LeftReason::KeyMaterial` | Key files are left by name, never opened. |
| ConsoleId | `consoles/mod.rs` | `kind: TitleKind { Base, Update, Dlc, Demo, Unknown }` — **no `Homebrew`**; `encrypted`, `title`, `id_source`, `content_id`, `region`, `product_id`, `parent_id`, `art`. |
| Folder probes | `consoles/mod.rs` `DirListing { dir, names }`, `DIR_PROBES` (PS3 JB, PS3 HDD, `sce_matches`, Wii U loadiine, NUS, XBE, XEX, GoD, GDK, ERA, Switch split folder), test-only `EXTRA_PROBES`, `identify_dir`; `sony::sce_matches` (PS4, PS5, Vita in one `sce_sys` listing; `psf_systems` already splits PS4 from Vita by `TITLE_ID`/`PSP2_SYSTEM_VER`/`APP_TYPE`, research C4 predates batch 3) | Every probe does its own disk I/O (`symlink_metadata`, `list_child`, `read_head`) from `l.dir`, and some read `l.dir.file_name()` (the `CUSA…` fallback, `FolderSuffix`): they cannot run on an archive. Art comes back as `ArtLocator::File { rel }`. `nx::probe_split` needs the FAT archive attribute, which no archive listing carries. |
| Archive inspection | `organize/inspect.rs` `vpk_console` (called only for ext `vpk` with zip magic), `vpk_entry`, `package_hits`; `archive::open_archive_with`, `file_entries`, `read_entry`, `Budget::plan_head` (64 KiB); `archive::MAX_ENTRIES` = 10 000 (`ArchiveTooBig` above it) | Reads `sce_sys/param.sfo` at the archive **root** only, case-insensitive. `from_vpk` already refuses a PS4 SFO. Inspection already lists every archive's entries once per plan. |
| Multi-disc | `organize/target.rs` (`multi_disc`, m3u for **extracted** archives, placed by the profile's `DiscLayout::{Flat, GameFolder}` and `m3u_for_multi`, skipped when the archive already has a playlist), `organize/extract.rs` `m3u_text`; `disc/mod.rs` `DiscIdentity { disc_no, … }` (the Sega `m` of `n/m` is dropped); `ConsoleId.disc_no` (GC/Wii byte `0x06` + 1 in `consoles/nintendo_disc.rs`); `dedupe/disc.rs` | Loose discs are never grouped and get no `.m3u`. |
| Large confirmation | `organize/apply.rs` `LARGE_TRANSFER` (10 GB, test override `COVERLY_TEST_LARGE_TRANSFER`), `large_transfer(plan) -> Option<LargeTransfer { bytes, items, copies, free, volume }>`; `PlanWarning::LargeTransfer`; `State::organize_apply(…, confirmed_bytes)`; `IpcError::LargeTransfer` (code `large-transfer`); `LargeTransferDialog.tsx`; CLI `--yes-large` | One aggregate question. `Choice::Leave` works on any item but always records `LeftReason::Unknown` (`plan.rs` decision loop); it carries no reason. A declined PS4 base also leaves its shadPS4 overlays (batch 3 D11 e). |
| Space check | `organize/apply.rs` `check_space` | Runs only when copying or crossing volumes and sums `item.size` (the **archive** size) plus 1 %; extraction bytes are **not** counted, and a same-volume extraction is not checked at all. |
| Duplicates | `dedupe/keys.rs` `ExactKey::Title { title_id, kind, version, disc }` — **`is_confirmed() == false`, report-only** (`ReportWhy::TitleId`, also used for other versions with the same title id, which stay reported whatever `one_game_one_rom` says); `dedupe/group.rs` `GroupAction::ReportOnly`; settings `dedupe.action` (`quarantine` / `trash`) and `dedupe.organize_step` (`OrganizeDedupeMode::{SetAside, Report, Off}`); `dedupe/units.rs` `UnitKind::Folder`, `units_from_folders` | Title and folder matches never move anything. |
| Network | `net/mod.rs` (`trait Http { fn get }` with four implementations: `UreqHttp`, `PoliteHttp`, test fakes; `HttpRequest { url, if_none_match, max_bytes = 32 MiB, bearer, forbidden_pauses }`; `HttpResponse { status, etag, last_modified, content_type, retry_after, body }`, body in memory; ureq 3); hosts in code: `thumbnails.libretro.com`, `raw.githubusercontent.com` (libretro-database), `www.steamgriddb.com`/`cdn2.steamgriddb.com`, `api.screenscraper.fr`, `github.com/MAECLY` (updater) | `last_modified` is already read but never sent back: no `If-Modified-Since`, no streaming body, no host registry. |
| Cache | `cache/mod.rs` (SQLite, `SCHEMA_VERSION` 3 in `user_version`; **a newer version makes `prepare` fail**, so an app rolled back after a schema bump cannot open its cache), `identify/dat.rs` `DatStore` (30-day TTL, conditional GET) | Pattern to copy for the catalogue, but not its file (D5). |
| Settings | `settings/model.rs` (`SETTINGS_VERSION` 3, tolerant load: an unknown group is ignored, a missing one takes defaults) | A new group is safe for installed users and for a rollback. |
| Settings UI | `src/components/settings/*Section.tsx`, `registry.ts` `SETTINGS_INDEX` (completeness test), `navStore.ts` `SettingsSection` (12 tabs), `SettingsSearch.tsx` | No credits tab. `AboutSection` links `LinkId::Notices` to the repo on GitHub. The tab labels live at `settings.section.<id>`; `settings.sections` is a **string** (the nav's label). |
| Links | `src-tauri/src/prefs.rs` `LinkId` + `link_url` (allowlist of repo URLs; a test pins the prefix); opened from the backend with `tauri_plugin_opener`; capabilities grant only `core:default` and `dialog:allow-open` | The front end never opens a URL it built itself. |
| Webview | `tauri.conf.json` CSP `img-src 'self' data: blob:`, `connect-src 'self' ipc: http://ipc.localhost` | No remote image or fetch from the webview: every icon reaches it through IPC. |
| Privacy copy | `about.noTelemetry` («Coverly ROMs solo contacta con las fuentes de arte que actives y […] con el servidor de actualizaciones») | Becomes untrue once the catalogue or nlib can be contacted (§10). |
| Licences | `LICENSES/` (`CC-BY-4.0.txt`, `ISC-lucide-react.txt`, `README.md`), `src/assets/fonts/OFL-*.txt`, `THIRD-PARTY-NOTICES.md`, `scripts/check-licenses.sh` (allowlist on `cargo metadata`); `tauri.conf.json` `bundle.resources` (map form) = `LICENSE`, `THIRD-PARTY-NOTICES.md` only | Gaps in `credits-inventory.md` §6 hold. `cargo-about 0.9.2` is installed on the owner's machine, not in the release jobs. |
| Release | `RELEASING.md`: bundles are built by the `app` job of the release workflow (`pnpm tauri build`), and `scripts/bump-version.sh` rewrites the workspace crates' versions in `Cargo.lock` | Anything that must exist in the bundle is committed, not generated by a tool the jobs lack. |
| Locales | `locales/{es,en}/ui.json`, nested; parity test; `…One` count keys; British «licence», «catalogue» in English | A key cannot be both a string and a parent. |

## 4. Decisions

Open questions got a default so nothing blocks; each owner-facing one is repeated in §13.

| # | Topic | Decision | Why |
|---|---|---|---|
| D1 | Keys | No code path reads, requests, bundles, downloads, links or logs console keys. A new test (`tests/no_keys.rs`) fails if any non-test source under `crates/`, `src-tauri/src` or `src/` contains `prod.keys`, `title.keys`, `keys.txt`, `aes_keys.txt`, `seeddb.bin` or `boot9` **outside** the `is_key_material` name list and its tests, or if a type in `catalog/` has a field named `key`, `rights_id`/`rightsId`, `title_key`/`titleKey`. The scan walks files by path component (Windows CI) and skips `#[cfg(test)]` modules by stripping from the `mod tests` line to the end of the file, the repo convention. The UI never names where keys come from. No log line, error message or status ever echoes catalogue content: a reducer error reports only the byte offset and the field name. | Owner rule; Yuzu judgement (`user-keys-legal.md` §3). |
| D2 | Catalogue opt-in | One setting `catalogs.switch.enabled` (default **off**; new top-level group `catalogs`, serde default, so installed users and a rollback are unaffected — §3 Settings). Off: nothing is downloaded, identification uses tickets/names only. On: the region file is downloaded on the first plan, library scan or the explicit «Descargar ahora» — never while the app's network is paused/offline, never under CLI `--offline`, and never for a plan with no Switch item. | Optional on-demand, never bundled. |
| D3 | Which file | UI locale `es` → `ES.es.json`, `en` → `US.en.json` (`https://raw.githubusercontent.com/blawar/titledb/master/<REGION>.<lang>.json`). If the locale's file answers 404 or fails to parse, `US.en.json` is used instead and the status says so. **Only one file is kept.** Ids missing from the locale file are not looked up in a second 90 MB file (owner decision §13 Q1). | Brief: one region/language, US.en fallback; 90 MB per file. |
| D4 | Refresh | At most once every **7 days** (setting `catalogs.switch.refresh_days`, 7–90). A conditional GET with the stored `ETag` (`If-None-Match`) **and** the stored `Last-Modified` (`If-Modified-Since`, new `HttpRequest.if_modified_since`; `HttpResponse.last_modified` already exists). 304 → only the check time moves. A failed refresh keeps the old index. Rate limit/offline pause like DATs. Turning icons on (D8) makes the next fetch unconditional once, because `iconUrl` was not kept before. | Region files have not changed since 2026-09-14; weekly is the cap. |
| D5 | Reduction | **No raw copy ever touches the disk.** New `Http::get_stream(req, &mut dyn FnMut(&mut dyn Read) -> Result<…>)` (a default trait method wraps `get()`'s body in a cursor, so `PoliteHttp` and the test fakes keep working; `UreqHttp` passes the live body, capped at **160 MB** of decoded bytes) feeds a streaming serde visitor over the top-level map that keeps, per entry with a valid 16-hex `id`, only `name`, `publisher`, `releaseDate` (int `YYYYMMDD` → ISO date), `region` and — only when icons are on (D8) — `iconUrl` if it is `https://img-eshop.cdn.nintendo.net/…`. Every other field (`key`, `rightsId`, `description`, `screenshots`, `nsuId`, ratings…) is visited as `IgnoredAny` and never held. If any entry carries a **non-null `key`** (all 37 473 are `null` today, `switch-titledb.md` row 48), the import is abandoned, the old rows stay and the status says «El catálogo trae datos que Coverly no acepta» (§13 Q11). Rows go into a **separate SQLite file** `catalog.sqlite` beside the cache (own `user_version`; not the shared cache DB, so `SCHEMA_VERSION` stays 3 — a rollback keeps its cache — and a minutes-long import never holds the cache's write lock that scans and the CLI share). Table `titledb(id INTEGER PRIMARY KEY, name, publisher, release, region, icon_url)`, filled in one transaction into a staging table and swapped on success. 29 572 ids ≈ 3.6 MB. Duplicate ids keep the first entry whose `name` is non-empty. Names are untrusted: control and bidi-override characters are stripped, length capped at 256 chars, NFC; when a name reaches a file or folder name (`NameMode::Catalogue`, m3u stems, `FolderSuffix`) it goes through the existing target-name sanitiser like any catalogue name. | Brief: id → name/publisher/releaseDate/region; never store key/rightsId/cnmts/cheats; never download keys. |
| D6 | NCA ids | `ncas.json` (87.6 MB) is fetched **lazily**, only while the catalogue is on and an item has an NCA id but no title id: a loose `<32 hex>.nca`/`.ncz` (name), an NSP/NSZ with neither ticket nor `cnmt.xml` whose PFS0 table (already read) lists `<32 hex>.nca`/`.ncz` names, or an XCI whose secure HFS0 lists `<32 hex>.nca` names (new: `nx::xci` reads the secure partition's plaintext HFS0 **header and names table only**, at the offset the root HFS0 gives, within D24 budgets, never the 0x190 gamecard info nor any NCA byte). Reduced the same whitelist way as D5 to `ncas(nca BLOB(16) PRIMARY KEY, title INTEGER, content_type INTEGER)` in `catalog.sqlite` (≈ 5.7 MB); same refresh rule (D4). The title id it gives counts as `IdSource::NcaCatalog`. `cnmts.json`, `versions.json`, `cheats.json` are never fetched. | Brief: NCA id cross-referenced with `ncas.json`; research C13. |
| D7 | Name priority | Switch title: plaintext `*.cnmt.xml`/NRO NACP title → **titledb** (by base id `& 0xFFFFFFFFFFFFE000` for updates/DLC, which show «Actualización de {name}»/«DLC de {name}») → nlib fallback (D9) → file-name title. `ConsoleId.title_source` (new, `Header`/`Titledb`/`Nlib`/`FileName`) feeds the «why» line. | Keyless order of `switch-titledb.md` §3 minus the parked NACP-by-keys step. |
| D8 | Switch icons | Setting `catalogs.switch.icons` (default **off**, enabled only while the catalogue is on). Icons are fetched on demand from the stored `iconUrl` (HTTPS, host allowlist `img-eshop.cdn.nintendo.net`, a redirect only to the same host, ≤ 1 MiB, JPEG/PNG decoded with the image module's existing dimension/memory caps and re-encoded to a 256 px PNG) into the art cache, and shown **only inside Coverly** (library tile, Organizar row thumbnail) through the existing thumbnail IPC — never as a remote URL in the webview (the CSP forbids it). Fill never writes them into the library (no new `KNOWN_SOURCES` id). Turning icons off deletes the cached icons and clears `icon_url`. | Brief: opt-in, on demand, local cache only. Official art is licensed to nobody (`TODO.md` §7.1 rules). |
| D9 | nlib fallback | Setting `catalogs.switch.fallback` (default **off**) with `catalogs.switch.fallback_url` (default `https://api.nlib.cc`, HTTPS only, validated like the proxy URL). When on, a title id the index does not resolve is asked as `GET {url}/nx/{TID}?lang={en|es}` (`TID` re-formatted from the parsed `u64` as 16 upper-case hex, never the raw file-name text; one request per id, the global limiter, `max_bytes` 64 KiB, results cached 30 days, misses 7 days in `catalog.sqlite`). The response is parsed defensively (`name`, `publisher`, `releaseDate` if present; anything else ignored; the exact shape is verified against the live API in task T3 and pinned in a fixture). Off by default because it sends title ids to a third party; usable only while the catalogue is on (the switch is disabled otherwise), so it never runs alone. | Brief: configurable fallback. |
| D10 | Offline | No catalogue, no network, or a failed first download: names come from the file name (D7 last step); the plan never waits on the catalogue (a 30 s cap on the first download inside a plan; past it the plan continues and the status shows «Descarga del catálogo pendiente»). | Brief: file-name fallback. |
| D11 | «cifrado» | An item of `switch` or `3ds` whose title stays unresolved after D7 **and** whose `encrypted == Some(true)` (or a Switch NSP/XCI/NCA, always encrypted) shows the label «Cifrado» next to its name and the note «No se puede leer el nombre sin descifrarlo». The label is text (a `Badge`, never colour alone), and the note is the badge's accessible description and a visible line in the item's detail (no hover-only tooltip). A resolved one keeps the existing `ItemNote::EncryptedContent { emulator }` note («Cifrado; algunos emuladores no pueden cargarlo») only where it already applies; an unresolved one shows the badge and that note, never two «Cifrado» texts on one row. `ItemNote::Encrypted` (an encrypted **archive entry**) and `LeftReason::EncryptedArchive` are unrelated and unchanged. | Owner: «Lo que no se resuelve queda marcado como "cifrado"». |
| D12 | Loose classifier | New `consoles/loose.rs`, called from `identify_one` **after** `sony` and **before** `orbis`/`xbox`, for the formats of §8.2 that no reader parses today (NPDM, NSO kind, `.nca` name, DOL, SCE/SELF, PBP, `~PSP` PRX, `.psvimg`). The formats an earlier reader already owns are extended **in that reader** so its verdict is not shadowed: `.tmd` in `wiiu.rs` (`tmd_id` learns the whole D14 table, 3DS/Wii/DSi included), `.rpx`/`.rpl` in `wiiu.rs`, `.nro`/`.nso` in `nx.rs`, `.xbe`/`.xex` in `xbox.rs`, `.3dsx` in `ctr.rs`. Every reader returns a `ConsoleId` with `kind` (new `TitleKind::Homebrew`) and, for pieces that cannot play alone, a new `ConsoleId.part: Option<PartKind>` (`ExeFs`, `Nca`, `Library`, `TitleMeta`, `Module`, `Backup`). | One module for the research table; existing readers keep priority. |
| D13 | Where they go | Homebrew, games, updates and DLC move like any ROM of their system (updates/DLC under their base, batch 2 D11). A `part` item is **left** with the new `LeftReason::GamePart { part, system, title_id }` («Parte de un juego; no se mueve suelta»); every loose `.nca`/`.ncz` is such a part, whatever the catalogue says of it. Homebrew goes to the system folder with `ItemNote::Homebrew` (no profile apps/ports mapping, §13 Q6). `.psvimg` is left `GamePart { Backup }` with the title id from its path. PSP `EBOOT.PBP` of a PS1 classic goes to `psx`. | Moving one ExeFS file breaks the dump; research §11. |
| D14 | `.tmd` kinds | Compare the **full** upper 32 bits of the title id: 3DS `00040000` app, `00040002` demo, `0004000E` update, `0004008C` DLC, `00040001` Download Play child (base), `00040010`/`0004001B`/`00040030`/`0004009B`/`000400DB`/`00040130`/`00040138` system → `NotARom::ConsoleSystem`; Wii `00010000`/`00010001`/`00010004` base, `00010005` DLC, `00010002`/`00010008`/`00000001` system; Wii U `00050000` base, `00050002` demo, `0005000E` update, `0005000C` DLC, `00050010`/`0005001B`/`0005004E`/`00070002`/`00070008`/`00000007` system; DSi `0003xxxx` → `nds` base unless the high word is a system one (`00030015`, `00030017`). Unknown high values → `TitleKind::Unknown`, no guess. | Research C4/C5. A bit test would file every 3DS update as DLC. |
| D15 | Huge items | An item is **huge** when it alone writes ≥ `LARGE_TRANSFER` (10 GB) to the target (the same counting as `large_transfer`: extraction bytes always, as-is bytes when copying or crossing volumes), **or** when it extracts a folder game from an archive (D20, any size). `large_transfer` gains `huge: Vec<HugeItem { item, name, system, bytes, extract, followers: Vec<usize> }>` (`bytes` includes the items that follow it — a PS4 base's shadPS4 overlays, batch 3 D11 e — and `followers` lists them) and `rest: u64` (bytes of every non-huge item). The per-item threshold is the same `LARGE_TRANSFER` constant and its test override (§13 Q12). | Brief: per-item prompt; folder extraction «con confirmación». |
| D16 | Per-item gate | IPC `organize_apply(plan_id, decisions, confirm: Option<LargeConfirm { bytes, items: Vec<usize> }>)` replaces `confirmed_bytes`. No compatibility shim: the webview and the backend ship in one binary, and `organize_resume` never takes a confirmation. The backend refuses (`IpcError::LargeTransfer`, now with `huge` and `rest`) unless every huge item of the **decided** plan is in `confirm.items` and `confirm.bytes ≥` the decided plan's `bytes`. «No» is sent as the new `Choice::Decline` (wire `decline`) for that item: like `Leave`, plus `LeftReason::NotConfirmed` instead of `LeftReason::Unknown`, never remembered by content, so the decided plan the gate checks already excludes it and its followers. The UI never subtracts sizes itself: it sends `rest + Σ bytes of the accepted huge items` from the numbers the error carried, and a mismatch (plan changed) is just refused again with fresh numbers. «Sí a todo» accepts the current and every later huge item at once. A plan that grew after the answers is refused again. `organize_resume` never asks again. | One source of truth (batch 3 D23), no second numbering; `Choice::Leave` has no reason to carry. |
| D16b | Sequence with the aggregate | A plan with huge items asks them one by one; **then**, only when `rest` alone reaches `LARGE_TRANSFER`, the existing aggregate dialog of batch 3 asks for the rest (its numbers are `rest`). «Sí a todo» also answers that last question. A plan with no huge item keeps batch 3 D23 exactly. So every byte over the threshold is confirmed once and only once. | Fixes the «per item or overall, never both» gap: three 40 GB PS4 folders plus 20 GB of PS2 ISOs would otherwise write the 20 GB unconfirmed. |
| D17 | Free space in the prompt | Each per-item dialog shows the item's size, the target volume and the free space **after `rest` and the items already accepted in this sequence**, with `check_space`'s 1 % margin («Quedarán {free} libres»). When the item does not fit, the «Sí» buttons are disabled and the body says so («No cabe: faltan {missing}»); «No» stays. Unknown free space (network share) → the line is omitted. `check_space` is fixed to count what `large_transfer` counts (extraction bytes in every mode, `item.size` only when copying or crossing volumes), so a same-volume extraction of a 40 GB PS4 zip is checked too. | The question is only useful if the number is current and apply refuses what the dialog said would not fit. |
| D18 | CLI prompt | `coverly organize … --apply` asks per huge item only when **stdin and stderr are both terminals** (`std::io::IsTerminal`; works on Windows consoles), writing the question to **stderr** so `--report json` on stdout stays clean: `Copy "<name>" (<size>; <free> free on <volume> after this)? [y]es / [a]ll / [n]o:`, then the aggregate question of D16b for `rest` when it applies. EOF or Ctrl-C at a prompt writes nothing and exits as today's refusal. `--yes-large` answers «all» without asking; new `--skip-large` answers «no» to all (they are left `NotConfirmed`, and the aggregate rest still needs `--yes-large` when it reaches the threshold); without a terminal and without either flag it refuses exactly as today (message and exit code unchanged). `--yes-large` and `--skip-large` together are a usage error. The prompt reads through an injected `Prompt` trait so tests run on every OS without a pty. | Brief: also in the CLI; scripts keep working. |
| D19 | Title and folder duplicates act | `ExactKey::Title` stops being report-only: `is_confirmed()` stays `false` (it is not a byte proof) but the new `ExactKey::acts_unconfirmed()` returns `true` for `Title` when **every** member has the same title id, kind, version (both `None` counts as equal; `None` vs `Some` does not group) and disc, **and every member's title id came from content** (`IdSource` ticket name, `cnmt.xml`, SFO, `param.json`, TMD, XBE/XEX, `meta.xml`/`app.xml`, NCA catalogue — never `FileName` alone, §13 Q14). Such a group acts as an **exact** relation (not a version relation, so `one_game_one_rom` does not gate it; keeper by the user's keep rules and container order; loser set aside to the quarantine or the Trash per **`dedupe.action`**, journaled, undoable) at `VERSION_CONFIDENCE`, with the reason «Mismo title ID y versión» visible on the group, and can be skipped per group as today. Other versions of one title id stay reported (`ReportWhy::TitleId`), unchanged. New `ExactKey::FolderTree { digest }` for folder games: SHA-1 of the sorted `(lower-case relative path, size)` list from the metadata-only tree summary (batch 2 D27), the same digest computed from the entry listing of an archive folder game (D20) so a zipped PS4 game is recognised as already in the library without extracting; equal digests in one system — and, when both have one, the same title id — act at `EXACT_CONFIDENCE` (no file is read). Folder games with the same title key but different trees act through `Title`. Both apply in the Duplicates screen, `coverly dedupe` and Organize when `dedupe.organize_step` is `set-aside`. `dedupe/keys.rs` `FACTS_VERSION` 2 → 3. Behaviour change for installed users: groups that were only reported now set a copy aside; the release notes say so. | Brief: through the existing system, not just reported. Undo makes the default safe; the tree digest is the «by folder» signal; a file-name id is too weak to move a file. |
| D20 | Folder games in archives | New trait `ListingFs` (`list(rel) -> Option<Vec<String>>`, `meta(rel) -> Option<EntryMeta { dir, file, link, len, fat_archive_bit: Option<bool> }>`, `read(rel, cap, budget) -> Option<Vec<u8>>`, `art(rel, format) -> ArtLocator`); `DirListing` gains `fs: &dyn ListingFs` and `name: &str` (the folder's own name: the disk folder's name, or the root folder's name inside the archive, or the archive stem for a root at the archive top), and `dir` becomes the root inside the `fs`. Probes read `l.name` instead of `l.dir.file_name()` and build art through `l.fs.art` (`DiskFs` → `ArtLocator::File { rel }`, `ArchiveFs` → `ArtLocator::ZipEntry { name: <root>/<rel> }`, which the embedded source already reads through the archive layer for any format, not only `.vpk`). `DiskFs` wraps today's calls unchanged; new `ArchiveFs` (in `organize/inspect.rs`) builds a virtual tree from the entries **inspection already listed** (no second open of the archive; names normalised `\` → `/`, entries with `unsafe_name` dropped, case kept, lookups case-insensitive as the probes already do; two entries equal after case folding → the archive goes to review, since extracting them collides on macOS and Windows) and reads entries through `read_entry` under `Budget::plan_head`. `fat_archive_bit` is `None` on archives, so `nx::probe_split` never matches there. Every probe and `EXTRA_PROBES` move to the trait; no probe changes its logic. | Brief: existing probes on a virtual listing, template `vpk_console`. |
| D21 | Roots | Markers per §8.4. Each entry whose path ends in a marker gives a candidate root (its prefix). 0 roots → not a folder game (today's behaviour). 1 root → `identify_dir` on the virtual listing at that root; a `DirConflict` → review. ≥ 2 roots → review «El comprimido contiene {count} juegos». A root deeper than 3 levels is ignored. Encrypted zip names / `he` 7z → no listing → today's behaviour. An archive over `archive::MAX_ENTRIES` (10 000; some PS3 discs exceed it) stays `ArchiveTooBig` as today — the cap is not raised in this batch. The marker scan is a name pass over the listing already in memory, so ordinary cartridge zips pay nothing measurable. | `zipped-folders-multidisc.md` §1.2 algorithm. |
| D22 | PS4 vs Vita in archives | The `sce_sys` probe already splits by `TITLE_ID`/`CATEGORY`; on `ArchiveFs` it works unchanged. When the `param.sfo` read exceeds the budget (solid 7z) the result is «structure only»: PS3 JB/HDD and Wii U loadiine/NUS still decide from names; an `sce_sys/param.sfo` + `eboot.bin` root goes to review with both `ps4` and `psvita` as alternatives («Hay que leer param.sfo para saber si es de PS4 o de Vita»). | Research C4: structure alone cannot tell them apart. |
| D23 | Actions | Vita (zip, vpk): **as is**, to `psvita/`; a Vita 7z gets the note «Vita3K solo instala .zip, .vpk o .vci» (Coverly does not recompress). PS3 disc (JB), PS4, Wii U loadiine/NUS: `Transfer::Extract` of the root subtree only, into the folder a **disk** folder game of that system would get (same naming, `FolderSuffix` and collision rules as batch 2 D13/batch 3; the root folder name, else the archive stem, is only the input), the archive then handled by `ArchiveAfter` as for any extraction (entries outside the root, such as an `.nfo`, are leftovers: `ArchiveKeptLeftovers`, archive kept); always a huge item (D15). Deep trees still obey `PathTooLong` (Windows) and the unsafe-name rules per entry. PS3 HDD/PSN (`<ID>/USRDIR/EBOOT.BIN`, `<ID>` matching `^[A-Z]{4}\d{5}$`, with `CATEGORY` `HG`/`DG` non-disc): review, default Leave, note `InstallFirst { emulator: "RPCS3" }`. Xbox/360 (`default.xbe`/`default.xex`) and PS5 roots: recognised (system and title shown) but moved **as is**, no extraction (the brief names no action; §13 Q16). Profile container overrides still win when the user set them. | `zipped-folders-multidisc.md` §1.3. |
| D24 | Loose multi-disc | New `organize/discsets.rs` after identification, on items not already in a set (a pile `.m3u` with its discs is a `Set` today and is left alone): candidates = same system, same source folder (or sibling folders whose names are equal once the disc tag is removed, the common «one folder per disc» download), same name key (title with the disc tag `(Disc|Disk|CD|GD)\s+(\d+|[A-Z])(\s+of\s+\d+)?` removed, every other tag kept). Confirmed when one of: GC/Wii same 6-char id and distinct `disc_no`; Saturn/Dreamcast same `m` and distinct `n` (`DiscIdentity` keeps `count: Option<u8>`); XBE `DiscNumber` / XEX `disc_number`+`disc_count` with the same title id; DAT hash matches whose names minus the disc tag are equal. Name-only candidates (Sega CD, CHD, no DAT) → review «Parece un juego de varios discos». Rejected when two members share a disc number (dedupe handles it), region/Rev differ, or a header contradicts the name. Confirmed sets keep their file names and are placed exactly as an extracted multi-disc archive is today (`target.rs`: the profile's `DiscLayout::Flat` → the system folder, `GameFolder` → `<system>/<title>/`), and get `<title>.m3u` (reusing `m3u_text`, stem through the target-name sanitiser) when `disc_layout.m3u_for_multi()`. The m3u is a new file: written no-clobber (an existing one at the target is kept and the set gets no new one), recorded in the journal and removed by undo, also in copy mode. Missing discs → the m3u lists only those present and the set gets the note «Falta el disco {n}». PS2, Xbox and Xbox 360 sets get no m3u. DuckStation `discsets.yaml` is not used or bundled (CC BY-NC-ND). | `zipped-folders-multidisc.md` §2.4. |
| D25 | Credits file | One file `credits/credits.json` (+ `credits/credits.schema.json`), read by: the app (`import` in the front end, `include_str!` in `coverly-core` for the tests and `open_credit`), `scripts/credits/readme.mjs` (writes the block between `<!-- credits:start -->`/`<!-- credits:end -->` in `README.md` and `README.es.md`; `--check` in `check-all`), and the landing by copy (like `design/tokens.json`; the copy and the Lucide line of `credits-inventory.md` §6.7 are done in the `coverly-roms-landing` repo after merge, not on this branch). Strings are in the file as `{ "es": …, "en": … }`; the tab's chrome stays in `locales/`. The front end imports it from the repo root (inside Vite's root; `tsconfig` gains the path), and `format:check`'s globs (today `src/**` and `locales/**` only) gain `"credits/**/*.json"`. | Single source of truth. |
| D26 | Entry shape | `{ id, group, name, url?, use: {es,en}, license: { spdx?, name, url? }, credit?: {es,en}, required: bool, endpoints?: [..], datasets?: [..], sources?: [..] }`. `group` ∈ `art-sources`, `databases`, `docs`, `conventions`, `libraries`, `fonts-icons`. `credit` is the exact required text when `required`. URLs are HTTPS official site or repo; a schema test rejects query strings and a denylist of ROM, key and emulator hosts **and repos** (the research cites emulator sources — Dolphin, PPSSPP, RPCS3, Vita3K, Cemu, Xenia, xemu, Azahar, DuckStation, shadPS4…: those entries carry their name, use and licence but **no `url`**, because `TODO.md` §7 forbids links to emulators). Frontends and firmwares (ES-DE, Batocera, muOS…) are allowed, as §7 lists them. | `TODO.md` §7. |
| D27 | Link opening | New IPC `open_credit(id, which: "site" \| "license")`: the backend looks the id up in its compiled copy of `credits.json` and opens that `url` or `license.url` with the opener (as `open_link` does; no new capability); an unknown id or a missing URL → `BadRequest`. The front end never passes a URL. Links carry a visible «se abre en el navegador» hint for screen readers (`aria-describedby`). `LinkId::Notices` stays in the allowlist (its test keeps pinning the repo prefix), but the About tab's «Avisos de terceros» now opens the in-app viewer (D29). | Keeps the `LinkId` allowlist model. |
| D28 | Notices generation | `pnpm notices` (`scripts/licenses/notices.mjs`, run by a developer): `cargo about generate --locked about.hbs` with `about.toml` (targets every release target — `aarch64`/`x86_64` Apple, `x86_64`/`aarch64` Windows MSVC, `x86_64`/`aarch64` Linux GNU —, `ignore-build-dependencies`, `ignore-dev-dependencies`, workspace crates excluded) → `LICENSES/generated/rust.txt`; `pnpm licenses list --prod --json` **plus** the build-time packages whose code ends up in the bundle although they are devDependencies (`tailwindcss` preflight, `tailwindcss-animate`, Vite's runtime helpers; a short explicit list in the script, checked against the built CSS/JS banner comments) + each package's own licence file from `node_modules` → `LICENSES/generated/js.txt`. Each file starts with `lock-sha256: <sha256>` of a **normalised** lock: `Cargo.lock` without the workspace's own packages (those without `source`, whose versions `bump-version.sh` rewrites every release) and `pnpm-lock.yaml`, both with CRLF → LF (Windows checkouts). The generated files are committed. `pnpm notices:check` (in `check-all`, pure Node, needs no cargo-about) fails with «run pnpm notices» when the hash no longer matches. The release `app` job runs `pnpm notices:check` before `tauri build` (it does not have cargo-about and must not regenerate). | Build-time notices without making every `pnpm check-all` or the release jobs need cargo-about, and without a version bump invalidating them. |
| D29 | Bundle | `tauri.conf.json` `bundle.resources` (map form, as today) adds `../LICENSES/` → `LICENSES/` (all `*.txt`, `README.md` and `generated/`) and each `../src/assets/fonts/OFL-*.txt` → `LICENSES/fonts/`; checked on the macOS `.app`, the NSIS/MSI install folder and the AppImage/deb by a test that reads the resolved config, not by hand. The tab's «Ver licencias de terceros» opens an in-app viewer that loads both generated files as a lazy chunk (`?raw` dynamic import, so it works in the browser mock too) and splits them per package. | Compliance gap §6.1. |
| D30 | Credit test | Every network endpoint lives in one registry, `net/hosts.rs` `ENDPOINTS: &[Endpoint { prefix, credit }]` (`https://thumbnails.libretro.com/`, `https://raw.githubusercontent.com/libretro/libretro-database/`, `https://raw.githubusercontent.com/blawar/titledb/`, `https://www.steamgriddb.com/`, `https://cdn2.steamgriddb.com/`, `https://api.screenscraper.fr/`, `https://img-eshop.cdn.nintendo.net/`, `https://api.nlib.cc/`, and the updater's `https://github.com/MAECLY/` with `credit: None`). `UreqHttp` refuses (a `NetError`, and a `debug_assert!`) any URL that does not start with a registered prefix, except the user's own nlib fallback URL and proxy, which the settings validation already pins to HTTPS. Rust `credits` tests: (a) every `ENDPOINTS` entry with a credit names an existing entry whose `endpoints` lists that prefix — **by path prefix, not host**, because `raw.githubusercontent.com` serves two datasets —, and every `https://` literal in non-test code under `crates/*/src` and `src-tauri/src` outside `net/hosts.rs`, `prefs.rs` (`LinkId`) and XML namespaces is a test failure naming the file; (b) every `KNOWN_SOURCES` id and every catalogue (`titledb`, `titledb-ncas`, `nlib`, `eshop-icons`) names a credit in `sources`/`datasets`; (c) every distinct DAT origin of the built-in system database, read through its API so derived defaults count (`metadat/<origin>/…` → `no-intro`, `redump`, `mame`, `fbneo-split`…, and `dat/…` → `dat`), has a dataset entry; (d) required credits named by the brief exist with their exact licence (ScreenScraper `CC-BY-NC-SA-4.0`, RetroArch Assets `CC-BY-4.0`, GameTDB with its link, libretro-database `CC-BY-SA-4.0`, titledb `MIT`). Vitest: es/en parity of every `use`/`credit`, and every group non-empty. | `TODO.md` §7 test; a literal scan alone misses runtime-built URLs. |
| D31 | Notices file fixes | `THIRD-PARTY-NOTICES.md`: drop TheGamesDB and the `docs/09-legal/` per-source pointer; add titledb (MIT, © 2019 Blake Warner), api.nlib.cc / nlib-api (GPL-3.0 service, contacted only when the fallback is on; nothing of it is shipped), Nintendo eShop icons (© Nintendo, fetched only on opt-in, cached locally, never redistributed), GameTDB link, libretro-database CC BY-SA, shadcn/ui MIT, the crates of `credits-inventory.md` §6.4 (wry, reqwest, brotli, option-ext MPL-2.0 with its source URL, webpki-roots CDLA-Permissive-2.0, ring Apache-2.0 AND ISC with BoringSSL, ICU4X Unicode-3.0) and a pointer to `LICENSES/generated/`. `docs/09-legal/license.md` loses its TheGamesDB line and names the Switch catalogue as an optional download. `credits-inventory.md` §6.6 (the «ScreenScraper lo conoce como» string) needs no change: the string is not in `src/`. | §6.2–6.6. |
| D32 | RetroArch notice in the UI | The tab shows the CC BY 4.0 notice verbatim in its «Tipografías e iconos» group (it is the entry's `credit`), and `SystemIcon`'s tooltip-free surfaces are unchanged. | Required in the UI; one place is enough. |
| D33 | Memoised facts | `organize/plan.rs` `FACTS_VERSION` 8 → 9 (`ConsoleId.part`, `title_source`, `TitleKind::Homebrew`, archive folder games, disc `count`). | Batch 2 D23. |

## 5. Architecture and files touched

### 5.1 Rust

| File | Change |
|---|---|
| `consoles/mod.rs` | `TitleKind::Homebrew`; `ConsoleId.part`, `ConsoleId.title_source`; `PartKind`; `IdSource::{NcaCatalog, Tmd, Npdm, Xbe, Xex, Self_, Pbp}`; `ListingFs`, `DiskFs`, `EntryMeta`; `DirListing.fs`; `identify_one` calls `loose::identify`. |
| `consoles/loose.rs` (new) | §8.2 readers no module has today: NPDM, `.nca` name, DOL structure, `.psvimg` path, SCE/SELF, PSP `~PSP`/PBP. |
| `consoles/wiiu.rs` | `tmd_id` learns the full D14 table (3DS, Wii, DSi, Wii U) and returns `part: TitleMeta`; `.rpx` loose → Homebrew, `.rpl` → part `Library` (today: magic-only). |
| `consoles/ctr.rs` | `.3dsx` kind Homebrew; regression test of §8.1. |
| `consoles/nx.rs` | NRO `ASET` NACP title and icon locator; `.nso` → part `ExeFs`; XCI secure-partition NCA names; NSP PFS0 NCA names when no ticket/cnmt; NCA ids on `ConsoleId` (`nca_ids: Vec<String>`, `#[serde(default, skip_serializing_if = "Vec::is_empty")]`). |
| `consoles/xbox.rs` | Loose `.xbe` (homebrew rule §8.2) and `.xex` module flags → kind; `DiscNumber`/`disc_count` on `ConsoleId.disc_no` + new `disc_count`. |
| `consoles/sony.rs`, `wiiu.rs`, `xbox.rs`, `nx.rs` probes, `EXTRA_PROBES` | Port to `ListingFs`, `l.name`, `l.fs.art` (no logic change). |
| `disc/mod.rs`, `disc/sega.rs` | `DiscIdentity.count`. |
| `catalog/mod.rs`, `catalog/titledb.rs`, `catalog/nlib.rs` (new) | `SwitchCatalog { ensure(), title(tid), by_nca(id), icon_url(tid), status(), remove() }` over its own `catalog.sqlite` (D5; `catalog_meta(name, etag, last_modified, checked_at, rows, source_file)`, `titledb`, `ncas`, `nlib(tid, name, publisher, release, fetched_at, miss)`); streaming reducer (D5, D6); nlib client (D9). Deleting the file is «Borrar catálogo». |
| `net/mod.rs`, `net/hosts.rs` (new) | `HttpRequest.if_modified_since` (sent from the stored `Last-Modified`; `HttpResponse.last_modified` already exists); `Http::get_stream` with a default implementation (D5; same retries, decoded-byte cap); `ENDPOINTS` registry and the prefix guard in `UreqHttp` (D30); every provider moves its URL literal there. |
| `cache/mod.rs` | **No change** (`SCHEMA_VERSION` stays 3; the catalogue has its own file, D5). |
| `settings/model.rs`, `validate.rs`, `patch.rs` | `CatalogSettings { switch: SwitchCatalogSettings { enabled, icons, refresh_days, fallback, fallback_url } }` (serde default, off). |
| `organize/inspect.rs` | `ArchiveFs`; `archive_folder_game(path, limits) -> Option<ArchiveFolderGame { root, matched, structure_only }>` generalising `vpk_console` (which becomes a call to it). |
| `organize/discsets.rs` (new) | D24. |
| `organize/plan.rs`, `target.rs`, `evidence.rs` | `Choice::Decline`; `LeftReason::{GamePart, NotConfirmed}`; `ItemNote::{Homebrew, Vita7z, MissingDisc { n }, EncryptedName}`; archive folder-game actions (D23); disc sets; `FACTS_VERSION` 9. Every new field is `#[serde(default)]` so plans and journals saved by 0.1.x still load for resume and undo. |
| `organize/apply.rs` | `HugeItem`, `large_transfer().huge`/`.rest`, `LargeConfirm`, `gate(plan, confirm) -> Result<(), LargeTransfer>` shared by IPC and CLI; `check_space` counts extraction bytes (D17). |
| `dedupe/keys.rs`, `group.rs`, `units.rs` | `acts_unconfirmed`, `FolderTree` digest, folder tree summary reuse, `FACTS_VERSION` 3. |
| `credits.rs` (new, `coverly-core`) | `include_str!("../../../credits/credits.json")`, `Credit`, `credit(id)`, tests D30. |
| `crates/coverly-cli/src/organize.rs` | D18 prompt (`std::io::IsTerminal` on stdin and stderr, question on stderr, injected `Prompt`), `--skip-large`; English text for the new reasons and notes. |
| `crates/coverly-cli/src/main.rs`, `catalog.rs` (new) | `coverly catalog switch status|update|remove` (respects the settings switch; `update` with the switch off is an error that says how to turn it on). |
| `src-tauri/src/ipc.rs`, `state.rs`, `error.rs`, `prefs.rs` | `organize_apply(…, confirm)`; `IpcError::LargeTransfer { …, huge, rest }`; `catalog_status`, `catalog_update`, `catalog_remove` (progress as an event, cancellable); `open_credit(id, which)`; every new command registered in `invoke_handler`. |
| `src-tauri/tauri.conf.json` | Resources (D29). |
| `about.toml`, `about.hbs` (new, repo root) | cargo-about config and template. |

### 5.2 Front end

| File | Change |
|---|---|
| `credits/credits.json`, `credits/credits.schema.json` (new) | D25, D26. |
| `src/lib/credits.ts` (new) | Typed import, grouping, search over name/use/licence in the active locale. |
| `src/components/settings/CreditsSection.tsx` (new), `NoticesDialog.tsx` (new) | §9.1. |
| `src/components/settings/SourcesSection.tsx` | «Catálogo de Switch» group (§9.2). |
| `src/stores/navStore.ts`, `src/screens/SettingsScreen.tsx`, `registry.ts`, `search.ts` | Section `credits` after `about` (13 tabs; the nav must still fit at 760 px); registry entries `setting-credits-search`, `setting-credits-notices`, family `credit-` **generated from `credits.json`** (one per entry, so the completeness test never needs a hand edit when an entry is added), `setting-sources-switch-catalog*`. |
| `src/components/organize/LargeTransferDialog.tsx` + `HugeItemDialog.tsx` (new) | Per-item sequence (§9.3), then the existing aggregate dialog for `rest` when D16b asks for it. |
| `src/stores/organizeStore.ts` | `pendingLarge` gains a cursor over `huge`, `answer(item, 'yes'|'all'|'no')`, builds `decline` decisions and `LargeConfirm` from the error's numbers (D16). |
| `src/ipc/types.ts`, `backend.ts`, `tauriBackend.ts`, `contract.sample.json`, `contract.test.ts` | New fields and calls; `TitleKind` `homebrew`; `LeftReason` `game-part`, `not-confirmed`; notes; `IdSource` values. |
| `src/ipc/mock/*` | §9.4. |
| `src/lib/organize.ts`, `src/components/organize/*Row*.tsx`, library tile | «Cifrado» label (D11), «Homebrew» note, disc sets, archive folder games. |
| `locales/{es,en}/ui.json` | §10, including the corrected `about.noTelemetry`. |
| `scripts/licenses/notices.mjs`, `scripts/credits/readme.mjs` (new), `package.json` | `notices`, `notices:check`, `credits:readme`, `credits:check`; `check-all` gains `notices:check` and `credits:check`. |
| `THIRD-PARTY-NOTICES.md`, `docs/09-legal/license.md`, `README.md`, `README.es.md` | D31, D25. |

## 6. Identification flow (Switch, adjustment 1)

1. `nx::identify` as today → title id from ticket name, `cnmt.xml`, file-name tokens; new NCA id list (loose name, NSP/NSZ PFS0 names, or XCI secure names).
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
| Removed | «Borrar catálogo» or toggle off | `catalog.sqlite` (tables `titledb`, `ncas`, `nlib`, `catalog_meta`) and the cached icons deleted; the toggle off asks nothing (the data is a cache). On Windows a file still open by a running scan is closed first (the catalogue handle lives in `AppState` and is dropped before the delete); a CLI process holding it makes the delete fail with the status «No se pudo borrar; cierra otras ventanas de Coverly». |
| Rejected | an entry with a non-null `key` (D5) | Old rows kept; status «El catálogo trae datos que Coverly no acepta»; no retry until the next refresh window. |

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
| PS3 HDD/PSN | `<ID>/PARAM.SFO` and `<ID>/USRDIR/EBOOT.BIN`, `<ID>` = `^[A-Z]{4}\d{5}$` (so a `PS3_GAME/` never matches twice) | prefix of `<ID>/` |
| PS4 / Vita / PS5 | `sce_sys/param.sfo` or `sce_sys/param.json` | prefix of `sce_sys/`; abort Vita on `sce_module/steroid.suprx` |
| Wii U loadiine | `code/*.rpx` with `content/` and `meta/` siblings | prefix of `code/` |
| Wii U NUS | `title.tmd` with `.app` siblings | its folder |
| Xbox / 360 | `default.xbe` / `default.xex` | its folder (recognised, moved as is, D23) |

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
- Motion: none on load. Filtering removes rows without animation; the notices dialog uses the existing dialog open/close, which already honours `prefers-reduced-motion` (`tokens.css`); no new animation is added anywhere in this batch.

**Review against generic defaults.** A first instinct was a grid of rounded cards with a licence chip in each (the SaaS-card kit, trait 4) and an all-caps «REQUIRED» eyebrow (trait 5): both rejected — cards chop a reading list into tiles and the badge adds a label above content. A second instinct, monospace licence identifiers (`CC-BY-NC-SA-4.0`), was replaced by the human licence name in body type, because mono is reserved for numbers in this app. Middle-dot meta strings («MIT · 2019 · Blake Warner») were replaced by a sentence in the use line. What stays distinctive is borrowed from the product's own world: the shelf rule as group header and the colophon as a reading page, not a gallery.

**Notices viewer.** A large dialog («Licencias de terceros»), two tabs «Rust» and «JavaScript» with the package counts in mono, a find field that highlights matches and jumps with Enter / Shift+Enter, and the text in Atkinson Hyperlegible Mono at caption size (it is code licence text, the one place where mono is the content). The generated files can reach a few MB: the text is split into one block per package and rendered through `@tanstack/react-virtual` (already a dependency), the search runs on the in-memory string (debounced) and scrolls to the block, and only visible blocks get `<mark>`s — never one giant `<pre>`. The match count («{index} de {count}») is an `aria-live="polite"` region. Focus starts on the find field; Escape closes; focus returns to the button. The dialog is usable at 760 px wide (tabs above, text wraps, no horizontal page scroll).

**Accessibility.** The search field has a visible label; the result count is an `aria-live="polite"` region; each entry is an `<article>` labelled by its name; links say where they go («Web oficial de ScreenScraper») and that they open the browser; an entry without `url` (D26) shows no dead link; keyboard order follows reading order; contrast from tokens (AA) in light and dark; the required-credit quotation is a `<blockquote>` so it is announced as a quote; Settings ⌘/Ctrl+F lands on the entry and moves focus to its `<article>`.

### 9.2 «Catálogo de Switch» (Sources tab)

A group after the source list, same `Field` primitives:
- Switch «Catálogo de nombres de Switch» + hint «Descarga la lista pública de blawar/titledb (unos 90 MB, se queda en tu equipo) para poner nombre a los juegos de Switch. No usa ni pide claves».
- Status line (§7) with counts and sizes in mono; buttons «Descargar ahora» / «Buscar actualización», «Borrar catálogo».
- Switch «Iconos de Switch» (disabled until the catalogue is on) + hint «Se descargan solo cuando se ven y solo se muestran en Coverly».
- In the Advanced fold: switch «Respaldo en línea» + field «Dirección del respaldo» (default `https://api.nlib.cc`) + hint «Si un juego no está en el catálogo, se pregunta su title ID a este servicio».

### 9.3 Per-item confirmation (D15–D17)

Built on `alert-dialog`, **one dialog instance for the whole sequence**: answering swaps its content in place (no close/reopen, so focus and screen-reader context are not lost and nothing flashes; with reduced motion there is no transition at all). Title «¿Copiar {name}?» / «¿Mover {name} a otro disco?» / «¿Extraer {name}?» (long names wrap, never truncate the question; size in mono in the body). Body: «Ocupa {bytes}. Quedarán {free} libres en {volume}.» A progress caption «Elemento {index} de {count}» (numbers in mono), part of the dialog's accessible description so each new item is announced. Buttons in this order: «No» (cancel style, initial focus on every item), «Sí a todo», «Sí». Escape = «No» for this item. When it does not fit: body «No cabe en {volume}: faltan {missing}.», «Sí» and «Sí a todo» disabled (`aria-disabled` with the reason as description, so they stay discoverable). After the huge items, the aggregate dialog for the rest when D16b applies. After the last answer the apply starts; if every item was declined and nothing else moves, the plan state returns with the toast «No se ha movido nada».

### 9.4 Mock

`?mockPick=` gains `huge` (three PS4 folders of 38, 52 and 61 GB on a 120 GB target, one zipped PS3 disc), `switch` (NSP/XCI with and without `[0100…]`, a loose `.nca`, an `.nro`), `odd` (one file per §8.2 row), `discs` (Saturn and GC sets, one name-only Sega CD set, one missing disc) and `dupes-title` (two NSPs with the same id and version, two `CUSA00900` folders with equal trees). `mockSettings` holds the catalogue status and simulates a download in 2 s. The notices viewer loads the real generated files.

## 10. Copy (es / en)

| Key | es | en |
|---|---|---|
| `settings.section.credits` | Agradecimientos | Credits |
| `about.noTelemetry` (changed) | Sin telemetría: Coverly ROMs solo contacta con las fuentes de arte y de datos que actives (incluido el catálogo de Switch y su respaldo en línea) y, salvo que lo desactives, con el servidor de actualizaciones. | No telemetry: Coverly ROMs only contacts the art and data sources you enable (including the Switch catalogue and its online fallback) and, unless you turn it off, the update server. |
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
| `settings.credits.opensBrowser` | Se abre en el navegador | Opens in your browser |
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
| `settings.sources.switchCatalog.rejected` | El catálogo trae datos que Coverly no acepta; se mantiene el anterior. | The catalogue holds data Coverly does not accept; the previous one is kept. |
| `settings.sources.switchCatalog.removeFailed` | No se pudo borrar; cierra otras ventanas de Coverly. | Could not delete it; close other Coverly windows. |
| `settings.sources.switchCatalog.download` | Descargar ahora | Download now |
| `settings.sources.switchCatalog.check` | Buscar actualización | Check for update |
| `settings.sources.switchCatalog.remove` | Borrar catálogo | Delete catalogue |
| `settings.sources.switchCatalog.icons` | Iconos de Switch | Switch icons |
| `settings.sources.switchCatalog.iconsHint` | Se descargan solo cuando se ven y solo se muestran en Coverly. | Fetched only when shown, and shown only in Coverly. |
| `settings.sources.switchCatalog.fallback` | Respaldo en línea | Online fallback |
| `settings.sources.switchCatalog.fallbackUrl` | Dirección del respaldo | Fallback address |
| `settings.sources.switchCatalog.fallbackHint` | Si un juego no está en el catálogo, se envía su title ID a este servicio para preguntar el nombre. | When a game is not in the catalogue, its title ID is sent to this service to ask for the name. |
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
| `dedupe.relation.titleId` | Mismo title ID y versión | Same title ID and version |
| `dedupe.relation.folderTree` | Misma carpeta | Same folder |

`dedupe.report.titleId` (other versions, still report-only) is unchanged. `settings.sections` stays the nav's string label; tab names live under `settings.section.*`. `organize.encrypted` (the existing `EncryptedContent` note) is unchanged; `organize.encryptedLabel` is a sibling string, not a child.

Numbers, sizes and dates render in mono through the existing rich-text helper (`useRichT`), never as plain string concatenation. Credit-entry strings live in `credits.json` (D25); their Spanish follows the same rules (sentence case, no exclamation marks, exact counts).

## 11. Data flow

1. **Catalogue:** Settings switch → `catalog_update` (IPC) or first plan → `SwitchCatalog::ensure` → `Http::get_stream` → streaming reducer → staging table in `catalog.sqlite` → swap → status event → Sources group. Identification asks `SwitchCatalog::title/by_nca` synchronously against SQLite (indexed, read-only connection).
2. **Organize plan:** walk → per file `identify_all` (with `loose`) / per folder `identify_dir(DiskFs)` / per archive `archive_folder_game(ArchiveFs)` → evidence → `discsets` → dedupe (`acts_unconfirmed`, `FolderTree`) → targets → `large_transfer` (with `huge`) → plan.
3. **Apply:** UI → `organize_apply(plan, decisions, None)` → `LargeTransfer { huge, rest }` error → per-item dialogs (→ aggregate dialog for `rest`, D16b) → `organize_apply(plan, decisions + Decline, Some(LargeConfirm))` → gate on the decided plan → `check_space` → apply job (journal, undo).
4. **Credits:** `credits.json` → front-end import → `CreditsSection`; link click → `open_credit(id)` → opener. Notices: button → dynamic import of `LICENSES/generated/*.txt` → viewer.

## 12. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| titledb answers 200 with HTML (GitHub outage page) | Streaming parse fails → staging table dropped, old rows kept, status failed. |
| Region file > 160 MB decoded | Refused by the cap; status failed; no partial import (staging table dropped). |
| Connection drops mid-download | Same as a parse failure; the next attempt starts from zero (no resume, nothing half-written). |
| Disk full while importing | SQLite error → rollback, error status; library untouched. |
| An entry with a non-null `key` | Import abandoned (D5), status «rejected». |
| A catalogue name with `/`, `..`, control or bidi characters | Stripped at import (D5); a target name still goes through the sanitiser. |
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
| Title-id duplicates with different versions | Different keys → no exact group; still reported as other versions (`ReportWhy::TitleId`), unchanged. |
| Two NSPs whose only title id is the `[0100…]` file-name token | Grouped but report-only (D19, Q14): a renamed file would otherwise set a different game aside. |
| Two folder games with equal trees but different system probes | Different systems → never grouped. |
| User undoes an organize that set aside a title duplicate | Existing undo restores it. |
| Zip with `PS3_GAME/` at two depths | Two roots → review «El comprimido contiene 2 juegos». |
| Zip with `PS3_GAME/` and `ps3_game/` | Case-fold collision → review (D20). |
| PS3 zip with more than 10 000 entries | `ArchiveTooBig`, as today. |
| Zipped PS4 game already extracted in the library | Same tree digest (D19) → set aside / reported as in the library, without extracting. |
| Zip of a Vita game with a `sce_module/steroid.suprx` | Vita3K aborts on it → review with note. |
| Solid 7z of a PS4 game, SFO beyond 64 KiB decode | Structure only → review ps4/psvita (D22). |
| Archive folder game extracted into a target that already has the folder | Existing `rename_dir_noclobber` / collision rules. |
| All huge items declined | Nothing huge moves; the rest of the plan applies; aggregate gate re-evaluated on the rest. |
| Free space changes between dialog and apply | Apply-time `check_space` still refuses (`ApplyError::NoSpace`), now counting extraction bytes (D17). |
| Same-volume move that extracts a 40 GB zip onto a nearly full disk | Today: not checked. Now: `check_space` refuses before anything is written. |
| User declines a PS4 base that has `-UPDATE` overlays | The overlays follow it (`followers`) and are left too; their bytes were in the base's `bytes`. |
| 300 × 4 GB ISOs, no single huge item | Only the aggregate dialog (batch 3), unchanged. |
| Three 40 GB PS4 folders plus 20 GB of small ROMs | Three per-item dialogs, then one aggregate dialog for the 20 GB rest. |
| CLI with `--report json` piped and a terminal stdin | Prompt on stderr; stdout stays pure JSON. |
| CLI at a prompt, Ctrl-C or EOF | Nothing written, the refusal message and exit code of today. |
| CLI `--skip-large` with no huge items | No effect. |
| Multi-disc set where disc 2 is a CHD and disc 1 a CUE | Allowed; m3u lists the `.cue` and the `.chd`. |
| `(Disc 0)` opening disc | Extra member; does not break 1..m. |
| Discs 1 and 2 in folders `Game (Disc 1)/`, `Game (Disc 2)/` | Candidates (D24 sibling folders); confirmed by headers or DAT like any set. |
| A pile `.m3u` already lists the discs | It is a `Set` today; `discsets` ignores it. |
| Target already has `<title>.m3u` | Not overwritten; no new m3u for that set. |
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
- **Q11** If titledb ever ships a non-null `key` (a title key), the import is abandoned and the old rows kept (D5). Alternative: import anyway, since the field is never read. Default: abandon — «never download keys» wins.
- **Q12** «Huge» per item uses the same 10 GB as batch 3's aggregate (D15), and a plan of many small items keeps the single aggregate question (D16b). Alternative: ask per item for every item of a plan over 10 GB.
- **Q13** The per-item dialog has only «Sí / Sí a todo / No» (the brief's three); there is no «Cancelar todo». Declining everything still applies the small items. Alternative: a fourth «Cancelar» that aborts the whole apply.
- **Q14** Title-id duplicates act only when every copy's id came from the content (ticket, cnmt, SFO, TMD, XBE/XEX…), not from the file name alone (D19). Alternative: also act on `[0100…]` file-name ids.
- **Q15** The landing's copy of `credits.json` and its Lucide credit (`credits-inventory.md` §6.7) are done in `coverly-roms-landing` after merge.
- **Q16** Xbox/360 and PS5 folder games found inside archives are recognised but moved as they are, not extracted (D23); the brief names actions only for Vita, PS3, PS4 and Wii U.
- **Q17** The titledb region file follows the UI language only (`es` → `ES.es`, `en` → `US.en`); a separate «región del catálogo» setting is not added. Changing the UI language re-downloads on the next refresh (§12).
- **Q18** Turning on title-id and folder duplicates (D19) changes what an existing Organize/Duplicates run does for installed users (copies that were only reported are now set aside, with undo). Release notes say so; no migration prompt is shown.

## 14. Test plan

### 14.1 Rust (`cargo test --workspace --no-fail-fast`)

- `catalog/titledb.rs`: reducer on a fixture of 6 entries (valid, no id, bad id, duplicate id, empty name, one with `key`/`rightsId`/`description`) → 3 rows, no stored `key`/`rightsId` column (schema test); `releaseDate` → ISO; icon URL only with icons on and the allowed host; 304 path keeps rows and moves `checked_at`; 404 on `ES.es` falls back to `US.en`; HTML body fails without touching rows; size cap; transaction rollback.
- `ncas` reducer: 32-hex keys, content type, bad rows skipped; lookup by NCA name.
- `net`: `If-Modified-Since`/`Last-Modified` round trip and `get_stream` (live body, decoded-byte cap, default implementation over `get` for fakes and `PoliteHttp`) with the loopback test server (`net/testing.rs`).
- `nlib`: fixture of the live response shape (captured in T3), misses cached.
- `nx.rs`: XCI secure names → NCA ids; NSP without ticket/cnmt → PFS0 NCA names; NRO NACP title and version; nothing reads a `.tik`/`.nca` body nor the 0x190 gamecard info (a `SparseAt` that panics on any read inside those ranges).
- `loose.rs`: one positive and one near-miss per §8.2 row (built in `consoles/testing.rs`); D14 table exhaustively, including `0004000E` → Update and `0004008C` → DLC; Vita authid homebrew; PBP PS1 vs PSP vs PSN.
- `no_keys.rs` (D1).
- `ListingFs`: every existing probe test runs twice, on `DiskFs` and on an `ArchiveFs` built from the same tree zipped (zip and 7z, fixture mtimes pinned, paths compared by component, zips written with `/` and with `\` separators); `l.name` gives the same `CUSA…` fallback on both; `probe_split` never matches on `ArchiveFs`; art locators become `ZipEntry` and the embedded source reads them from a `.zip` and a `.7z`.
- `archive_folder_game`: PS3 JB, PS3 HDD, PS4, Vita, Wii U loadiine, two roots, root too deep, solid 7z beyond budget → structure only, steroid abort.
- `discsets.rs`: GC 2-disc, Saturn 3-disc with `CD-1/3…`, DC `GD-ROM1/2`, XEX 2-disc, DAT-hash PS1 set, name-only Sega CD → review, duplicate disc number → rejected, region mismatch → rejected, missing disc → note and m3u of present ones; `m3u_text` output byte-exact.
- `dedupe`: title group acts (set aside, Trash mode, undo restores), version `None`/`None` acts, different versions do not group, tree digest equal → acts, different systems never; `FACTS_VERSION` bump invalidates.
- `organize/apply.rs`: `huge` list (≥ threshold single item, folder extraction of any size, followers' bytes included), `rest`, gate accepts full confirmation, refuses a missing id, refuses a lower `bytes`, accepts after declines (`Choice::Decline` → `NotConfirmed`, overlays follow), D16b rest question; `check_space` counts extraction bytes on a same-volume move; old saved plans and journals (fixtures from 0.1.x) still load.
- `cli_organize.rs`: the prompt through the injected `Prompt` (scripted `y`, `a`, `n`, EOF sequences; runs on Windows, macOS and Linux CI without a pty), the aggregate rest question after the huge items, prompt text on stderr with stdout JSON untouched, `--yes-large`, `--skip-large`, both flags → usage error, non-TTY refusal byte-for-byte unchanged; `coverly catalog switch status` with the switch off; `--offline` never contacts the catalogue.
- `credits.rs` (D30 a–d) and `net/hosts.rs`: a request to an unregistered prefix fails; the user's nlib URL passes; `raw.githubusercontent.com/<other>/` fails although the host is registered.
- `catalog`: `catalog.sqlite` is separate (the cache's `user_version` stays 3); import into staging then swap; a non-null `key` entry abandons the import; names stripped of control/bidi characters; deleting while a read connection is open (Windows semantics simulated by holding the handle).
- Real data (read-only, copies under `target/` with `cp -RXp`, deleted afterwards, no `._` files): the Wii U loadiine folder and `.wua`, the `.3ds`/`.zip` pair of «Hatsune Miku - Project Mirai DX (USA)» (expected: one duplicate group), the PS1 cue/bin set (no false multi-disc grouping).

### 14.2 Vitest (`pnpm check-all`)

- `credits.test.ts`: schema, es/en parity, groups non-empty, no denied hosts, required entries present.
- `CreditsSection.test.tsx`: groups in order, search filters and announces the count, empty state, link buttons call `openCredit(id)` (never a URL), RetroArch notice text exact.
- `NoticesDialog.test.tsx`: tabs, find, focus return.
- `registry.test.tsx`: new entries; search lands on a credit row.
- `HugeItemDialog.test.tsx` + `organizeStore.test.ts`: sequence order, «Sí a todo», «No» adds `Leave`, `bytes` sent = total − declined, no-room disables «Sí», Escape = «No», all declined → toast.
- `SourcesSection.test.tsx`: catalogue states (off, downloading, ok, fallback, failed old/none, rejected, remove failed), icons disabled while off, fallback URL validation.
- `AboutSection.test.tsx`: the corrected `noTelemetry` text; «Avisos de terceros» opens the viewer.
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
| T2 | `feat(core): conditional GET with Last-Modified, streaming bodies and an endpoint registry` |
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
| T17 | `build: generate third-party notices and bundle licence texts` (+ `notices:check` in the release `app` job) |
| T18 | `docs: fix third-party notices and README credits` |
| T19 | `test(e2e): adjustments flows` (evidence in the PR) |

Every merge from `main` is `git fetch origin && git merge origin/main`, never a rebase or a force-push.

## 16. Sources

All sources are those of the five research files listed in the header, fetched 2026-11-01: switchbrew (NRO, NSO, NPDM, NCA, CNMT, XCI), wiibrew (DOL, TMD, Homebrew Channel), wiiubrew (TMD, RPL, meta.xml, Title database), 3dbrew (TMD, Titles, NCCH, CIA, SMDH, 3DSX), xboxdevwiki and Cxbx-Reloaded `Xbe.h`, Xenia `xex2_info.h`, decaf-emu `cafe_loader_rpl.h`, scetool `sce.h`/`sce.cpp`, vita-toolchain `vita-make-fself.c`, PPSSPP `Loaders.cpp`/`PBPReader.cpp`/`sceKernelModule.cpp`, psvimgtools, Dolphin `DolReader.cpp`, blawar/titledb (`LICENSE`, README, `US.en.json`, `ncas.json`), ghost-land/nlib-api README, Vita3K `interface.cpp`, RPCS3 `System.cpp`, Cemu `TitleInfo.cpp`, ES-DE USERGUIDE, DuckStation `LICENSE`, Sega Saturn programmer's guide, mc.pp.se IP0000.BIN, libretro-database `LICENSE`, ScreenScraper `webapi2.php`, RetroArch Assets `LICENSE`, GameTDB FAQ, the Yuzu consent judgement (1:24-cv-00082 D.R.I.).

## 17. Self-review

- **Placeholders:** none; every open question has a default (§13) and every new name has a home (§5). The nlib response shape is the one unverified technical fact; T3 pins it with a live capture before the parser is written (D9).
- **Contradictions checked:** D3 «one file» vs brief «US.en fallback» — reconciled as fallback when the locale file is unavailable, merging listed as Q1. D8 icons «local cache only» vs Fill — Fill never writes them. D15 folder extractions always huge vs batch 3 D23 aggregate — per-item questions for huge items, then the aggregate only for the non-huge rest when it alone reaches the threshold (D16b), so every byte is confirmed exactly once. D19 «acts» vs `is_confirmed() == false` — kept separate (`acts_unconfirmed`), so code that relies on byte proof (e.g. hard-link twins) is unaffected. D13 `.psvimg` left vs «classify» — classified (system, title id) and left, as research §7 recommends. D1 key test vs `is_key_material` name list — the list is the one allowed place.
- **Scope:** adjustment 3, launchers, §7.1 sources and user keys are excluded in the header and not referenced by any task.
- **Brief coverage:** (1) D1–D11, §6–7; (2) D12–D14, §8.2; (4) D15–D18, §9.3; (5) D19, §8.3; (6) D20–D24, §8.4; (7) D25–D32, §9.1; compliance gaps §6.1–6.5 → D29, D31, D32; required credits → D30 d.
- **Brand rules:** mono only for numbers (and the licence text viewer, where the text is code licences); no Manila outside its meaning; sentence case, no exclamation marks in the copy table.

## 18. Adversarial review against the code (2026-11-01)

What the first draft assumed and the code says otherwise, and what changed here:

| # | Finding | Fixed in |
|---|---|---|
| R1 | `Choice::Leave` always records `LeftReason::Unknown`; it cannot carry `NotConfirmed`. | D16 `Choice::Decline` |
| R2 | The UI subtracting sizes misses shadPS4 overlays that follow their base, and a stale plan. | D15 `followers`, D16 numbers from the error |
| R3 | «Per item or overall, never both» left a plan with huge items plus a ≥ 10 GB rest unconfirmed. | D16b |
| R4 | `check_space` ignores extraction bytes and same-volume moves, so «apply still refuses» was false for archive folder games. | D17, §12 |
| R5 | `HttpResponse.last_modified` already exists; a temp file of the 90 MB catalogue would put a file with a `key` column on disk. | D4, D5 `get_stream`, no temp file |
| R6 | Adding tables to the shared cache bumps `SCHEMA_VERSION`, and an older app refuses a newer cache: a rollback would lose its cache. A long import would also hold the write lock scans share. | D5 `catalog.sqlite` |
| R7 | `.tmd`, `.rpx`, `.xbe`, `.xex` already have readers that run before the planned `loose.rs`; a second parser would be shadowed. | §3, D12, §5.1 |
| R8 | Probes read `l.dir.file_name()` and return `ArtLocator::File`; `probe_split` needs a FAT attribute. A virtual listing needs `name`, art mapping and no attribute. | D20 |
| R9 | Re-opening every archive for the folder-game check would double the plan's archive I/O; the entries are already listed. Case-fold collisions break extraction on macOS/Windows. `MAX_ENTRIES` caps PS3 trees. | D20, D21 |
| R10 | Title-id groups were planned to act «like a version group», which `one_game_one_rom` gates; the setting is `dedupe.action`, not `dedupe.quarantine`; file-name ids are too weak to move files. | D19, Q14 |
| R11 | `settings.sections` is a string: `settings.sections.credits` would break the locale tree. `about.noTelemetry` becomes false once the catalogue exists. | §10 |
| R12 | A host-only credit test passes any new `raw.githubusercontent.com` dataset; runtime-built URLs (nlib, icons) never appear as literals; DAT paths start with `metadat/`, not the origin. | D30 `ENDPOINTS` by prefix, origin from the system DB |
| R13 | Hashing raw `Cargo.lock` breaks `notices:check` on every version bump (workspace crates) and on CRLF checkouts; the release jobs have no cargo-about. Tailwind/Vite code lands in the bundle from devDependencies. | D28 |
| R14 | Research cites emulator sources; `TODO.md` §7 forbids linking emulators. | D26 `url?` + denylist |
| R15 | CLI prompt on stdout would corrupt `--report json`; a pty test does not run on Windows CI. | D18, §14.1 |
| R16 | Catalogue names are untrusted and can reach file names (`NameMode::Catalogue`, m3u stems). | D5, D24 |
| R17 | Loose discs are often one folder per disc; pile `.m3u` sets already exist; new m3u files must be undoable and no-clobber; placement must follow `DiscLayout`. | D24, §12 |
| R18 | Closing and reopening one dialog per item loses focus and flashes; disabled buttons need a reason; a multi-MB notices `<pre>` freezes the webview. | §9.1, §9.3 |
| R19 | A remote icon URL in the webview is blocked by the CSP; icon redirects and decode size need limits. | D8 |
