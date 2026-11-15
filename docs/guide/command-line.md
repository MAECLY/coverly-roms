# Command line

[Coverly ROMs](../../README.md) › Command line · **English** · [Español](command-line.es.md)

`coverly` is the same engine as the app, for the terminal. Like the app, it
writes nothing unless you pass `--apply`.

| Command        | What it does                                                                                          |
| -------------- | ----------------------------------------------------------------------------------------------------- |
| `audit`        | Report which ROMs of a library lack cover, screenshot or title art (read-only)                        |
| `catalog`      | Manage the optional Switch name catalogue (`switch status`, `update`, `remove`)                       |
| `fill`         | Fill missing art from the enabled sources (dry run unless --apply)                                    |
| `integrations` | List, test, save or forget the accounts and keys of online services (`list`, `test`, `set`, `forget`) |
| `organize`     | Sort a pile of loose ROMs into one folder per system (dry run unless --apply)                         |
| `dedupe`       | Find the same game stored twice in a system and set the copies aside (dry run)                        |
| `profiles`     | List the built-in and custom layout profiles                                                          |
| `systems`      | List the known systems and their folder aliases                                                       |
| `undo`         | Undo the last job (fill, organise or duplicates), or the one given with --job                         |
| `update`       | Check for a newer coverly, or install it (verified with minisign)                                     |

```bash
coverly audit /Volumes/SD --missing-only --format csv --output report.csv
coverly fill /Volumes/SD                      # dry run: the plan, nothing written
coverly fill /Volumes/SD --apply --auto-only --system gba,snes --media cover
coverly fill /Volumes/SD --sources libretro,steamgriddb --strategy popularity
coverly organize ~/Downloads/roms             # dry run: a proposal by system
coverly organize ~/Downloads/roms --target /Volumes/SD --copy --apply
coverly dedupe /Volumes/SD                    # dry run: which copy stays, where the rest go
coverly dedupe /Volumes/SD --apply            # set them aside in /Volumes/SD/_Duplicados
coverly dedupe /Volumes/SD --apply --trash    # or send them to the Trash
coverly catalog switch update                 # download the optional Switch name catalogue
coverly integrations list                     # which services are on and which keys are set
coverly integrations set steamgriddb          # save a key, typed at a hidden prompt
coverly integrations test steamgriddb         # one request with the saved key
coverly undo --list
coverly undo                                  # undo the last job
```

### Useful flags

**`fill`:** `--profile ID|auto`, `--regions rom,world,usa,europe,japan`,
`--match hash-then-name|hash-only|name-only`, `--scope missing|all` (`all`
needs `--overwrite backup`), `--concurrency 1-6`, `--refresh` (ignore cached
candidates and indexes), `--report table|json`, `--output FILE`.

**`organize`:** `--layout PROFILE_ID`, `--target DIR`, `--flat`,
`--copy`/`--move`, `--names keep|catalogue`, `--assign FILE=SYSTEM_ID`
(repeatable), `--resume JOB_ID`, `--include-review`, `--offline`,
`--no-external-tools` (do not use unrar, 7-Zip or bsdtar),
`--duplicates set-aside|report|off` (copies already in the library),
`--yes-large` (confirm an apply that writes 10 GB or more), `--skip-large`
(leave those items where they are), `--game-folders`/`--no-game-folders`
(for this run, give each modern game its own folder with `Updates/` and
`DLC/`, or keep everything loose; default: the layout's setting),
`--emulator SYSTEM=EMULATOR|none` (repeatable; lay a system out the way that
emulator expects it, e.g. `ps4=shadps4`), `--emulator-data EMULATOR=DIR`
(repeatable, absolute; where that emulator keeps its cheats, patches and
mods).

**`catalog switch`:** `status`, `update`, `remove`, `--data-dir DIR`.

**`dedupe`:** `--system gb,psx`, `--level exact,format,version`,
`--one-game-one-rom`, `--keep PATH` (this copy stays; repeatable),
`--keep-rules has_save,verified,…`, `--reset-choices`, `--quarantine DIR`,
`--trash`, `--resume JOB_ID`, `--offline`, `--no-external-tools`,
`--report table|json`, `--output FILE`.

**`undo`:** `--list`, `--job ID`, `--include-later` (with `--job`: also undo
the newer jobs that touched the same folder, newest first).

**`integrations`:** `list [--json]`, `test ID [--json]`, `set ID [--field
user|password|api-key] [--from-env VAR]`, `forget ID [--yes]`. `set` never
takes the secret as an argument: it reads it from stdin (without echo when you
type it) or from the variable `--from-env` names. No output ever prints a
value, only `set` or `not set`. Exit codes: 0 ok, 1 the test failed (wrong
key, missing, service down) or the keychain refused, 2 bad usage, 5 offline or
rate limited.

**Environment:** `COVERLY_SGDB_KEY` (SteamGridDB key), `COVERLY_SS_USER` and
`COVERLY_SS_PASSWORD` (ScreenScraper account, used by `organize`). A variable
that is set wins; otherwise the CLI uses the key the app saved in the OS
keychain, and only towards the service's own address (never with
`--steamgriddb-url` or another custom endpoint). `COVERLY_NO_KEYCHAIN=1`
turns the keychain off.

`coverly <command> --help` lists every option.
