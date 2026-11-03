# Settings, Custom Profiles, Credentials and gamelist.xml

> **TL;DR.** Every spec §9 setting lives in one typed, versioned `settings.json` (format 2). The core loads it field by field: a broken, old or future file never crashes. Bad parts fall back to their defaults, a backup copy is kept and the UI shows a notice. The app and the CLI read the same file (CLI flags override it). Only the app writes it, through validated merge patches. Each fill job keeps a snapshot of the settings it started with. Custom layout profiles are TOML files next to the read-only built-ins. Source credentials live in the OS keychain and never reach the webview; Settings › Integrations shows one card per service from one registry, `integrations/integrations.json`, which also feeds the settings search, the notices on other screens and `coverly integrations`. A pick from "Search by title" is remembered as a manual identity. Batocera/Knulli fills merge the art paths into `gamelist.xml`, byte-preserving, journaled and undoable. Decisions D24–D32 ([tracker](../TRACKER.md)). Code: `crates/coverly-core/src/settings/`, `profiles/custom.rs`, `credentials/` (`mod.rs`, `keychain.rs`), `integrations.rs`, `identify/overrides.rs`, `gamelist.rs`; `src-tauri/src/{state,prefs,profiles,integrations,logging}.rs`; `src/stores/settingsStore.ts`, `src/components/settings/`, `src/lib/settingsRules.ts`, `src/lib/profileTemplate.ts`.

## The model (`settings::model`)

`Settings { version: 2, general, library, art, matching, sources, network, safety, advanced, organize, dedupe }`, with one struct per §9 group (plus `organize`, added with [organising loose ROMs](./organize.md), and `dedupe`, added with [duplicates](./dedupe.md); a file without them reads the defaults, so the format stays 2). Every struct has `#[serde(default)]`, so a missing group or field is its default.

| Group | Field | Default | Notes |
|---|---|---|---|
| general | `language` | `auto` | `auto`/`en`/`es`, applied live |
| | `theme` | `system` | `system`/`light`/`dark`, applied live (`<html data-theme>`) |
| | `check_updates_on_launch`, `update_channel` | `true`, `stable` | stored only; the updater comes in M7 |
| library | `default_profile` | `null` (detect) | must be a built-in or custom id; a deleted one falls back to `null` |
| | `ignored_dirs` | `bios, saves, states, Shortcuts` | skipped at any depth |
| | `ignored_exts` | `txt, xml, json, cfg, ini, db, png, jpg, jpeg, webp, bmp, srm, sav, state, pdf, md, nfo, log, dat` | lowercase, without dots |
| | `include_hidden`, `follow_symlinks` | `false`, `false` | |
| | `recent` | `[]` | history (max 10), kept by Reset |
| art | `media` | `cover, screenshot, title` | what a fill fills by default |
| | `single_slot_media` | `cover` | what goes into the one slot of Anbernic/Garlic/Onion/MinUI |
| | `output_format` | `profile` | `profile`/`png`/`jpg`; RetroArch stays PNG (`Profile.fixed_format`) |
| | `max_size` | `{}` | per profile id `{width, height}`, 16–4096 |
| | `keep_aspect`, `jpeg_quality` | `true`, `90` | quality 1–100 |
| | `overwrite` | `never` | `never`/`ask`/`backup` (D25) |
| matching | `strategy`, `regions` | `rom-region`, `rom, world, usa, europe, japan` | |
| | `languages` | `en` | tie-break (D28) |
| | `auto_threshold`, `review_threshold` | `0.9`, `0.65` | 0.3–1.0, review ≤ auto |
| | `fuzzy`, `arcade_translation` | `true`, `true` | |
| | `match_method`, `max_hash_mb` | `hash-then-name`, `64` | `name-only` = hashing off (D26) |
| sources | `order` | libretro on; steamgriddb, screenscraper off | the last two stay off until M6 |
| network | `concurrency`, `timeout_secs`, `retries` | `4`, `30`, `3` | 1–6, 5–300, 0–10 |
| | `proxy`, `proxy_url` | `system`, `""` | `system` behaves like `env` (D26); `manual` needs an `http(s)://host:port` URL |
| | `cache_max_mb`, `index_refresh_hours` | `2048`, `48` | 64–102400, 1–720 |
| safety | `backup_before_overwrite` | `true` | locked on (Undo needs backups) |
| | `backup_dir` | `null` (app data dir) | absolute, not inside the open library (D27) |
| | `dry_run_default`, `clean_apple_double`, `undo_depth` | `true`, `true`, `10` | depth 1–100 |
| advanced | `log_level`, `custom_dir` | `info`, `null` | custom profiles folder |
| organize | `default_layout` | `anbernic-stock` | layout proposed for a loose pile; must be a built-in or custom id |
| | `nest_in_roms_root` | `true` | system folders inside the layout's ROMs folder (`<picked>/ROMS/<SYS>`); off = directly in the picked folder (`--flat`) |
| | `mode`, `names` | `auto`, `keep` | `auto` = move on the same volume, copy otherwise; `move`/`copy`; names `keep`/`catalogue` |
| | `download_dats` | `true` | download missing catalogues while planning (at most 4 per plan) |
| | `auto_threshold`, `review_threshold` | `0.85`, `0.50` | 0–1, review ≤ auto; separate from the matching thresholds |
| | `companion_exts` | `sav, srm, rtc, state*, cht, ips, bps, ups, txt, nfo, png, jpg` | files with a moved ROM's stem that follow it (`state*` = prefix) |
| | `folder_names`, `arcade_order` | `{}`, `{}` | per layout id: system id → folder name (must resolve back to that system), arcade systems in order |
| | `recent_seconds`, `keep_organize_journals_days` | `60`, `90` | 0–3600, 1–3650; organise journals are kept at least this long ([fill-pipeline.md](./fill-pipeline.md#writer-guarantees)) |
| | `external_tools` | `auto` | `auto` lets Organise run an external archive tool found on the system (unrar, 7-Zip, bsdtar) for RAR and unsupported 7z coders; `off` never runs one (those archives are left with an install/enable note) |
| dedupe | `levels` | `exact, format, version` | at least one, no repeat; `version` acts only with `one_game_one_rom`, otherwise it is reported |
| | `one_game_one_rom` | `false` | also set aside other versions of a game (1G1R) |
| | `action` | `quarantine` | `quarantine`/`trash` (the OS Trash, opt-in) |
| | `quarantine_dir`, `quarantine_name` | `null`, `null` | `null` dir = `<library>/<name>`; a custom dir must be absolute and not inside a ROMs or system folder (`invalid`). `null` name = `_Duplicados` in every language (choosing `_Duplicates` from the app language at first run is not built yet); no `/`, `\`, `.` or `..`. Scans skip both default names and this one |
| | `keep_rules` | all ten on, in the order `has_save, played, in_library, verified, retail, region, revision, container, cover, catalogue_name` | `{ rule, on }` each once (no `duplicate`); missing rules are appended, on. The user's pick and the tie-breaker are not settings |
| | `container_order` | handheld profiles `chd > pbp > cue > iso > img > bin`, frontends `chd > cue > pbp > iso > img > bin`, `*` arcade `zip` | profile id (or `*`) → system id, `disc`, `cart`, `arcade` (or `*`) → containers, preferred first; unknown profile → `unknown`, bad key or container → `invalid` |
| | `exclude_systems` | `ports, bios, apps` | folder names or system ids never looked at (lowercase) |
| | `max_dat_downloads` | `12` | 0–100, catalogues a library run may download (organise has its own 4) |
| | `organize_step` | `set-aside` | the Organise duplicates step: `set-aside`/`report`/`off` |

## Loading (`settings::load`, D24)

`load_settings(config_dir, known_profile_ids) -> (Settings, LoadReport)`:

- **Field-tolerant parse.** The file is read as JSON values and each field is taken on its own. A wrong type or an out-of-range value falls back to that field's default (both thresholds when they are misordered). Unknown groups and fields are ignored. List entries are cleaned one by one (bad entries dropped, never the whole list). The result is normalised and validated before it is used.
- **Corrupt.** A file that is not a JSON object, or is larger than 1 MiB (`MAX_SETTINGS_BYTES`), is renamed `settings.json.bad-<unix>`. Defaults are used and the notice is `reset-corrupt`.
- **Changed while loading.** When loading fixed something, the file on disk is first copied to `settings.json.bak-<unix>` (an identical earlier backup is reused). The notice is `fixed`, with the list of fields that fell back.
- **Future version.** A `version` above 2 is read the same way (what this version knows is used), backed up, and the notice is `future-version`. It is never refused.
- **Migration.** If there is no `settings.json` but M3's `library.json` (version 1) exists, its ignored folders and recent libraries move into `settings.json`. `library.json` stays as the backup and the notice is `migrated`. After a Reset, the migration never runs again.
- **Saving** is atomic: a unique temp file, then rename (`save_settings`). `backup_settings(dir, tag)` makes the copy before a Reset.

The app shows the notice once, together with the backup path (`SettingsView { settings, notice, backup, fixed }`).

## Validation and updates

- `validate(&Settings, known_ids) -> Vec<FieldError>`. A `FieldError` is `{ field: "group.field", code, min?, max? }` with codes `range`, `required`, `invalid`, `order`, `unknown`, `duplicate`, `not-absolute`. The UI shows them as `settings.field.<code>` next to the field. `RANGES` (16 numeric rules) is mirrored in `src/lib/settingsRules.ts`; the contract sample pins both.
- **Updates are JSON merge patches** (`Settings::with_patch(patch, known)`). A patch must be an object of groups (anything else is `invalid`). `null` resets a group or field to its default. The merged result is normalised and validated. Errors come back as **data** and nothing is saved, so the user can fix the field (`SettingsUpdateView { settings, errors, reopen }`).
- `needs_reopen(prev, next)` is true when the change affects the open library's scan or art paths: ignored folders/extensions, hidden files, symlinks, single-slot media, output format, max sizes, custom profiles folder. The UI then shows "Reopen the library to apply these changes." with a "Reopen now" button.
- **Reset** keeps the recent libraries, makes a backup copy first and returns the defaults.

## Adapters (`settings::apply`)

The core never reads the file itself. Callers turn `Settings` into the options the modules already take:

| Adapter | Feeds |
|---|---|
| `scan_options()` | `scan` (ignored folders/extensions, hidden, symlinks, the duplicates quarantine names) |
| `fill_settings()` | the `fill` facade: media, strategy, regions, languages, thresholds, fuzzy, arcade, match method, max hash size |
| `art_profile(&base)` | the profile with the single-slot media, output format and max size applied |
| `fit()` | image fitting (keep aspect, JPEG quality) |
| `net_config()` | `net` (concurrency, timeout, retries, proxy) |
| `apply_options(root)` | the writer (`._*` cleanup on removable volumes, undo depth, organise journal retention) |
| `manual_options()` | drawer writes (same retention) |
| `OrganizeOptions::from_settings(settings, layout)` | organise plans (`organize/plan.rs`): layout, nesting, mode, names, thresholds, companions, the chosen layout's folder names and arcade order, catalogue downloads, the duplicates step and quarantine name |
| `DedupeOptions::from_settings(&settings.dedupe)` | duplicates plans (`dedupe/mod.rs`): levels, 1G1R, keeper rules, container order, exclusions, DAT downloads; the CLI flags `--level`, `--one-game-one-rom`, `--keep-rules`, `--quarantine`, `--trash` override them |
| `data_dir(default)` | the job data folder (backup location, D27) |
| `needs_reopen(next)` | the reopen notice |

**One snapshot per job.** `AppState::fill_config()` builds an `Arc<FillConfig>` from the current settings. A job keeps the config it **started** with for its dry run, its run and every resume: data folder, network and art output. Clear cache is refused while a job runs (`job-running`).

## The CLI

`coverly` reads the app's `settings.json` read-only: it never writes, migrates or renames it, and a broken file reads as defaults. `COVERLY_SETTINGS` overrides the path. Set it to the empty string for the built-in defaults (all CLI tests do this, and set `COVERLY_NO_KEYCHAIN=1` too, through `tests/common::coverly()`). Flags override settings: `--profile`, `--media`, `--strategy`, `--regions`, `--match`, `--overwrite`, `--concurrency`, `--data-dir`. Without `--apply`/`--dry-run`, `fill` follows "dry run by default". The CLI reads `overwrite: ask` as `never` unless `--overwrite backup` is given (D25). Custom profiles are found the same way as in the app.

- **Keys.** The CLI reads `COVERLY_SGDB_KEY`, `COVERLY_SS_USER` and `COVERLY_SS_PASSWORD` first, then the entries the app saved in the OS keychain (`crates/coverly-cli/src/keys.rs`): only for an enabled source whose variable is empty (the two ScreenScraper variables as one pair), only when the run uses the default endpoints, and each call capped at 5 s; a keychain that fails or times out counts as empty with one note. `COVERLY_NO_KEYCHAIN=1` turns the keychain off.
- **`coverly integrations list|test|set|forget`** works on the same registry and keychain entries. `set` reads the value from stdin (no echo on a terminal) or from the variable `--from-env` names; a value typed after the id is bad usage and is not repeated. Outputs say `set` / `not set`, never a value. Exit codes: 0 ok; 1 the test said `invalid`, `missing` or `unavailable`, or a keychain error; 2 bad usage, an unknown or planned id, or `forget` without a terminal or `--yes`; 5 `offline` or `rate-limited`. Example: `printf %s "$KEY" | coverly integrations set steamgriddb`.

## Custom profiles (`profiles::custom`, D29)

One TOML file `<id>.toml` per profile, in `<config dir>/profiles` or the "custom profiles folder" setting:

```toml
id = "onion-covers"
name = "Onion OS (covers folder)"
roms_root = "Roms"
art_dirs = ["Imgs", "covers"]
gamelist = false

[slots.cover]
template = "{system_dir}/covers/{stem}.png"
format = "png"
max_width = 250
max_height = 360
```

- **Tokens:** `{root} {system_dir} {rel_dir} {stem} {libretro_stem} {system_id} {libretro}`, each only as a whole path segment (or `{stem}` / `{libretro_stem}` plus an extension). `{libretro_stem}` is the stem with ``&*/:`"<>?\|`` replaced by `_`, the name RetroArch looks a thumbnail up by; the built-in RetroArch profile uses it, every other built-in uses `{stem}`. A thumbnail at the `{stem}` name of a `{libretro_stem}` slot is not counted as art but belongs to the ROM (never an orphan; it travels with a duplicate set aside). A template must start with `{root}` or `{system_dir}` and its last segment must contain `{stem}` or `{libretro_stem}`. `..`, `.`, absolute paths, unknown tokens and a `{rel_dir}` in only some slots are refused. Width and height are given together or not at all.
- **Validation** returns `FieldError`s per field (`slots.cover.template`, `id`, `name`, …). `preview(profile, example)` renders the library-relative path of an example ROM. A template that would leave the library has no preview.
- **Store:** files are untrusted. Files over 64 KiB, bad TOML, an id that differs from the file name, or a built-in id are skipped with a warning. Saving is atomic. Renaming or deleting a profile moves or clears `default_profile` and `max_size` entries that pointed to it. Built-ins are read-only and can be duplicated (the copy's name is translated: "Onion OS (copy)" / "(copia)").
- **Organise keys** (optional): `system_folder = "first-alias" | "id"` (how organise names the system folders; default `first-alias`), `[folder_names]` (system id → folder) and `[host_folders]` (system id → host system id, e.g. `ngpc = "ngp"`). A copy of a built-in keeps its naming and host folders. See [organize.md](./organize.md#target-layout).
- Custom systems keep M1's `systems.toml` override. Only the folder setting is new.

## Credentials (`credentials`, `credentials/keychain.rs`, D30)

- `trait CredentialStore { available, get, set, clear }` in the core, with `MemoryStore` (used by every test). `KeychainStore` (crate `keyring` 4, service `com.maecly.coverlyroms`) lives in `coverly_core::credentials::keychain` behind the core feature `keychain`, which the app and the CLI turn on; `src-tauri` no longer has its own copy.
- **Closed set of keys:** `steamgriddb-api-key`, `screenscraper-user`, `screenscraper-password`. Any other key is `bad-request`. The service and account names are pinned by a test: keys saved before Integrations existed are found as they are, with nothing to migrate. Planned services get no key until they work.
- **Values cross IPC once**, webview → Rust (`set_credential`), and never come back. `credential_status` returns `{ available, set: { key: bool } }`. Values are never written to `settings.json`, logs, journals, reports, events or the mock's storage; log lines say `set` / `unset` or the `KeyCheck`. A test serialises a fully populated `Settings` and fails if an account name or a stored test secret appears in it, so a future export of the settings cannot carry them (there is no export today). The one exception it names is `network.proxy_url`, which may hold `user:pass@`; `coverly integrations list` and the logs never print it. Keychain calls block (macOS may prompt), so they run on the blocking pool.
- **Errors carry no secret and are classified by platform code**, never by message text (`keychain::classify`): `keychain-unavailable`, `keychain-locked`, `keychain-denied`, `keychain-failed`. macOS decides on the `OSStatus` (`-25308` locked; `-128`, `-25293`, `-61`, `-25292`, `-25244` denied; `-25291`, `-25294`, `-25295` unavailable), Windows on the Win32 error (`ERROR_NO_SUCH_LOGON_SESSION` unavailable, `ERROR_ACCESS_DENIED` denied), Linux on the Secret Service error (locked, a dismissed prompt is denied). The UI picks `errors.keychainOs.<kind>.<os>` from `AppInfo.platform` (`src/components/settings/keychainMessage.ts`, falling back to the generic text), and the CLI has the same advice in English (`integrations::keychain_message`).
- The real-keychain round trip is an `#[ignore]` test with its own service `com.maecly.coverlyroms.test`, run by hand.

## Integrations

- **Registry.** `integrations/integrations.json` lists every service Coverly talks to on the user's behalf. Each entry has `id`, `name`, `kind` (`api-key`, `account`, `dataset`, `link`), `group`, `status` (`working` or `planned`), `fields` (credential key + input), `test`, `help_url` (the service's official account or API page), `credits` (ids in `credits/credits.json`), `source` (its art source id), `setting` (where its on/off lives) and `powers` (`fill`, `organize-identify`, `organize-names`, `updates`). Planned entries take `needs` (display only) and `roadmap` (an anchor in `TODO.md`), never `fields` or `test`. Rust compiles it in (`coverly_core::integrations`: `get`, `working`, `for_source`, `problems`, the pure `state`); TypeScript imports the same file (`src/lib/integrations.ts`). It feeds the tab, the settings search, the notices on Fill and Organise and `coverly integrations`.
- **Checks.** `integrations::problems()` must be empty (`the_shipped_registry_has_no_problems`): ids unique and kebab-case, every credit and power known, a working entry with `fields` has a credit and a `help_url` on that credit's host that passes the credits denylist (no ROM, key or emulator sites), every `CredentialKey` belongs to exactly one working entry, and every network endpoint that needs credentials (`net::hosts::Endpoint.credentials`) is covered by a working entry with fields. `src/lib/integrations.test.ts` checks the copy of each entry in both catalogues.
- **States** (`integrations::state`, D6), strongest first: `off` (disabled in settings) > `unavailable` (this build cannot use it: ScreenScraper without developer credentials) > `not-configured` (a required field is not stored) > `quota` (the daily quota is used up) > `error` (the last test or job was refused, or the keychain could not be read; `reason` says which) > `connected` (the last test or job worked this session) > `saved`. Health and the last ScreenScraper quota (`SsQuota { requests_today, max_requests_per_day, max_threads }`) live in memory only (`HealthBook` in `src-tauri/src/integrations.rs`): a restart starts every card at `saved`. A job feeds it through `Gathered.troubles` / `answered` (`note_troubles`); a test begun before a key was saved or cleared is dropped. The Switch catalogue card maps its state from `CatalogStatus` in TypeScript (`catalogCardState`, plan IP-1).
- **No network on open.** Opening Settings reads presence from the keychain only; «Probar conexión» (`test_integration`) is the only request, one cheap authenticated call.
- **Tab.** `IntegrationsSection.tsx` renders one `IntegrationCard` per working entry (status tag with word and icon, what it is for, masked `SecretField`s that never refill and only say «guardado», test, «Cómo conseguir la clave» through `open_integration_help`, quota, enable/disable, «Olvidar credenciales» with an in-page confirmation, privacy note, links to its credit and its place in Sources), `SwitchCatalogGroup` (moved from Sources: on/off, icons, fallback URL; same settings paths) and the «Próximamente» list of planned entries linking to the roadmap (`LinkId::Roadmap`). Sources keeps only the order, with «Configurar en Integraciones» on sources that need an account. Other screens link to a card with `IntegrationLink` / `IntegrationNotice`.
- **Adding a service** (`integrations/README.md`): (1) one entry in `integrations.json` (kebab-case `id`, `powers` from `integrations::POWERS`, `help_url` on its credit's host when it has `fields`); (2) its credit in `credits/credits.json`, listed in `credits`; (3) its copy under `integrations.service.<id>.*` in `locales/es/ui.json` and `locales/en/ui.json`; (4) a planned service takes `needs` and `roadmap` only; once it works, add its `CredentialKey`, `fields` and `test`; (5) run `cargo test -p coverly-core integrations` and `pnpm test src/lib/integrations`.

## Manual identities (`identify::overrides`, D32)

A pick in the drawer's "Search by title" writes the image to the ROM's **own** target path (the ROM's file name, never the candidate's). It also stores `{ absolute ROM path → candidate name, source }` in `<app data dir>/identity-overrides.json`. This file lives in the app's own data dir: it is not the cache (Clear cache keeps it) and not the backup location (moving that folder never forgets manual matches). `FillSettings.overrides_dir` points the core at it. Later fills and the drawer's Identify use it first (`IdMethod::Manual`, confidence 1.0). "Forget manual match" (`clear_identity_override`) removes it. Hostile queries (empty, over 120 characters, control characters) are refused; results are shown as text.

## gamelist.xml merge (`gamelist`, D31, M2-10)

After the apply phase of a job on a profile with `gamelist = true` (Batocera/Knulli), each system folder's `gamelist.xml` is merged:

| Media | Element | Rule |
|---|---|---|
| cover | `<thumbnail>` | also `<image>` when the game has no usable `<image>` |
| screenshot | `<image>` | |
| title | `<titleshot>` | |
| everything else | `<marquee>`, `<rating>`, `<desc>`, attributes, comments, unknown elements | preserved, never written |

- A tag is set when it is missing, empty or points to a file that does not exist. It is replaced only in a "replace existing art" job. Games without an entry are appended with `<path>`, `<name>` and the art tags. A `<game>` is matched by its normalised `<path>` (`./` stripped, NFC, trimmed): the exact spelling first, then the first entry equal when case is ignored (as ES does). A ROM listed twice is merged once, into the first entry. A differently cased `GameList.xml` is merged in place, never shadowed.
- **Preserve everything.** `quick-xml` is used only to find byte offsets. The output is the input with spliced edits, so the declaration, BOM, comments, unknown content and line endings are kept. Values are XML-escaped. Entities are never expanded.
- **Refusals** (the file is left untouched, with a warning in the report): a DOCTYPE, not UTF-8, larger than 32 MiB, malformed, or not a regular file (folder, symlink).
- **Journal and undo.** The write goes through the job's `Writer`: the original is backed up and journaled, so Undo restores it byte for byte. When a paused and resumed job writes the file twice, the journal keeps the **first** backup and Undo goes back to the original bytes, not the intermediate ones.

## The app side

- `AppState::new(config_dir, base)` loads the settings once. Commands: `get_settings`, `update_settings` (emits `settings-changed`), `reset_settings`, `open_config_folder`, `open_logs_folder`, `open_link(LinkId)` (fixed ids: `repo`, `license`, `notices`, `trademarks`, `releases`, `issues`, `roadmap`), `cache_usage`, `clear_cache`, profiles, credentials and search (see [desktop-app.md](./desktop-app.md#ipc-contract)). The webview never sends a folder or URL to open.
- **Duplicates tab** (Settings › Duplicates, `DuplicatesSection.tsx` over `DedupeSettings.tsx`): action, quarantine folder and 1G1R on top; levels, reorderable keeper rules, container order and exclusions under *Advanced options*; see [desktop-app.md](./desktop-app.md#duplicates). The Organise step (`dedupe.organize_step`) is shown in the Organise tab.
- **Tabs are presentation only.** The screen groups its tabs (Library, Organise, Duplicates and Profiles under Collection, and so on), folds advanced options and offers a search, but `settings.json`, its groups, keys, defaults and validation are the same: a control keeps its settings path whichever tab shows it (see [desktop-app.md](./desktop-app.md#settings-screen)).
- **File logger** (`logging.rs`): `<data dir>/logs/coverly.log`, one line per message (newlines escaped), rotated once at 1 MiB, level from `advanced.log_level` and changed live. It never logs credential values. The CLI keeps writing to stderr.

## The mock (browser)

`src/ipc/mock/mockSettings.ts` mirrors the Rust rules (`settingsRules.ts`, `profileTemplate.ts`) and persists in `localStorage`. `coverly.mock.settings` holds the settings, `coverly.mock.profiles` the custom profiles, and `coverly.mock.credentials` **only `true`/`false` per key** (never a value). `?mockIntegrations=empty|saved|error|quota|dev|nodev`, `?mockKeychain=locked|denied|unavailable` and `?mockPlatform=macos|windows|linux` pick the integration scenario, a refusing keychain and the platform of the keychain copy. Corrupt storage starts from defaults with the `reset-corrupt` notice. Folder and link commands are recorded and nothing opens.
