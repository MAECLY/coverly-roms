# Settings, Custom Profiles, Credentials and gamelist.xml

> **TL;DR.** Every spec §9 setting lives in one typed, versioned `settings.json` (format 2). The core loads it field by field: a broken, old or future file never crashes. Bad parts fall back to their defaults, a backup copy is kept and the UI shows a notice. The app and the CLI read the same file (CLI flags override it). Only the app writes it, through validated merge patches. Each fill job keeps a snapshot of the settings it started with. Custom layout profiles are TOML files next to the read-only built-ins. Source credentials live in the OS keychain and never reach the webview. A pick from "Search by title" is remembered as a manual identity. Batocera/Knulli fills merge the art paths into `gamelist.xml`, byte-preserving, journaled and undoable. Decisions D24–D32 ([tracker](../TRACKER.md)). Code: `crates/coverly-core/src/settings/`, `profiles/custom.rs`, `credentials.rs`, `identify/overrides.rs`, `gamelist.rs`; `src-tauri/src/{state,prefs,profiles,keychain,logging}.rs`; `src/stores/settingsStore.ts`, `src/components/settings/`, `src/lib/settingsRules.ts`, `src/lib/profileTemplate.ts`.

## The model (`settings::model`)

`Settings { version: 2, general, library, art, matching, sources, network, safety, advanced, organize }`, with one struct per §9 group (plus `organize`, added with [organising loose ROMs](./organize.md); a file without it reads the defaults, so the format stays 2). Every struct has `#[serde(default)]`, so a missing group or field is its default.

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
| `scan_options()` | `scan` (ignored folders/extensions, hidden, symlinks) |
| `fill_settings()` | the `fill` facade: media, strategy, regions, languages, thresholds, fuzzy, arcade, match method, max hash size |
| `art_profile(&base)` | the profile with the single-slot media, output format and max size applied |
| `fit()` | image fitting (keep aspect, JPEG quality) |
| `net_config()` | `net` (concurrency, timeout, retries, proxy) |
| `apply_options(root)` | the writer (`._*` cleanup on removable volumes, undo depth, organise journal retention) |
| `manual_options()` | drawer writes (same retention) |
| `OrganizeOptions::from_settings(settings, layout)` | organise plans (`organize/plan.rs`): layout, nesting, mode, names, thresholds, companions, the chosen layout's folder names and arcade order, catalogue downloads |
| `data_dir(default)` | the job data folder (backup location, D27) |
| `needs_reopen(next)` | the reopen notice |

**One snapshot per job.** `AppState::fill_config()` builds an `Arc<FillConfig>` from the current settings. A job keeps the config it **started** with for its dry run, its run and every resume: data folder, network and art output. Clear cache is refused while a job runs (`job-running`).

## The CLI

`coverly` reads the app's `settings.json` read-only: it never writes, migrates or renames it, and a broken file reads as defaults. `COVERLY_SETTINGS` overrides the path. Set it to the empty string for the built-in defaults (all CLI tests do this). Flags override settings: `--profile`, `--media`, `--strategy`, `--regions`, `--match`, `--overwrite`, `--concurrency`, `--data-dir`. Without `--apply`/`--dry-run`, `fill` follows "dry run by default". The CLI reads `overwrite: ask` as `never` unless `--overwrite backup` is given (D25). Custom profiles are found the same way as in the app.

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

- **Tokens:** `{root} {system_dir} {rel_dir} {stem} {system_id} {libretro}`, each only as a whole path segment (or `{stem}` plus an extension). A template must start with `{root}` or `{system_dir}` and its last segment must contain `{stem}`. `..`, `.`, absolute paths, unknown tokens and a `{rel_dir}` in only some slots are refused. Width and height are given together or not at all.
- **Validation** returns `FieldError`s per field (`slots.cover.template`, `id`, `name`, …). `preview(profile, example)` renders the library-relative path of an example ROM. A template that would leave the library has no preview.
- **Store:** files are untrusted. Files over 64 KiB, bad TOML, an id that differs from the file name, or a built-in id are skipped with a warning. Saving is atomic. Renaming or deleting a profile moves or clears `default_profile` and `max_size` entries that pointed to it. Built-ins are read-only and can be duplicated (the copy's name is translated: "Onion OS (copy)" / "(copia)").
- **Organise keys** (optional): `system_folder = "first-alias" | "id"` (how organise names the system folders; default `first-alias`), `[folder_names]` (system id → folder) and `[host_folders]` (system id → host system id, e.g. `ngpc = "ngp"`). A copy of a built-in keeps its naming and host folders. See [organize.md](./organize.md#target-layout).
- Custom systems keep M1's `systems.toml` override. Only the folder setting is new.

## Credentials (`credentials`, `src-tauri/src/keychain.rs`, D30)

- `trait CredentialStore { available, get, set, clear }` in the core, with `MemoryStore` (used by every test). `KeychainStore` (crate `keyring` 4, service `com.maecly.coverlyroms`) lives in `src-tauri` only. The CLI gets one in M6 when a source needs it.
- **Closed set of keys:** `steamgriddb-api-key`, `screenscraper-user`, `screenscraper-password`. Any other key is `bad-request`.
- **Values cross IPC once**, webview → Rust (`set_credential`), and never come back. `credential_status` returns `{ available, set: { key: bool } }`. Values are never written to `settings.json`, logs, journals, reports, events or the mock's storage. Errors carry no secret: `keychain-unavailable`, `keychain-locked`, `keychain-failed`. Keychain calls block (macOS may prompt), so they run on the blocking pool.
- The real-keychain round trip is an `#[ignore]` test, run by hand.

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

- `AppState::new(config_dir, base)` loads the settings once. Commands: `get_settings`, `update_settings` (emits `settings-changed`), `reset_settings`, `open_config_folder`, `open_logs_folder`, `open_link(LinkId)` (fixed ids: `repo`, `license`, `notices`, `trademarks`, `releases`, `issues`), `cache_usage`, `clear_cache`, profiles, credentials and search (see [desktop-app.md](./desktop-app.md#ipc-contract)). The webview never sends a folder or URL to open.
- **File logger** (`logging.rs`): `<data dir>/logs/coverly.log`, one line per message (newlines escaped), rotated once at 1 MiB, level from `advanced.log_level` and changed live. It never logs credential values. The CLI keeps writing to stderr.

## The mock (browser)

`src/ipc/mock/mockSettings.ts` mirrors the Rust rules (`settingsRules.ts`, `profileTemplate.ts`) and persists in `localStorage`. `coverly.mock.settings` holds the settings, `coverly.mock.profiles` the custom profiles, and `coverly.mock.credentials` **only `true`/`false` per key** (never a value). Corrupt storage starts from defaults with the `reset-corrupt` notice. Folder and link commands are recorded and nothing opens.
