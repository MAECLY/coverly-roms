<p align="center"><img src="design/icon/app.svg" width="96" height="96" alt="Coverly ROMs icon"></p>

# Coverly ROMs

**English** · [Español](README.es.md)

Coverly ROMs is a free, cross-platform desktop app (Windows, Linux, macOS) that
audits a ROM library in any folder layout — the SD card of a handheld,
EmulationStation/ES-DE, Batocera, RetroArch or a plain folder — shows which
games are missing a cover, a screenshot or a title screen, and fills the gaps
in bulk, by default from the free libretro-thumbnails collection, without
breaking anything on the card. The reference tools are Windows-only or need
heavy setup; Coverly ROMs aims to be the polished, safe, multi-platform option.

> **Status: pre-release** — v0.1.0 is being prepared; see
> [RELEASING.md](RELEASING.md).

## Features

Planned for v1 (see [the design spec](docs/superpowers/specs/2026-10-15-coverly-roms-design.md)):

- **Audit** any library: systems, ROMs (including multi-file games such as
  `.cue`/`.bin`, `.m3u` and `.gdi`), and the art status of every game per media
  type (cover, screenshot, title). Export the report as CSV or JSON.
- **Layout profiles** for Anbernic stock, GarlicOS, Onion, muOS, MinUI/NextUI,
  ES-DE/Batocera, RetroArch and custom layouts, auto-detected and overridable.
- **Fill missing art in bulk** from libretro-thumbnails (no account), with
  identification by hash and by name, arcade short-name translation, a choice
  of strategies (priority, ROM region, popularity), a dry-run preview, backups
  and undo. Doubtful matches go to a review queue.
- **Organise a folder of loose ROMs** — a downloads folder full of zips — into
  one folder per system in the layout you choose (by default Anbernic stock:
  `ROMS/GBA/`, `ROMS/GB/`, `ROMS/SFC/`…). Each file is identified by its
  contents (catalogue CRC, then header) before its extension or name, with a
  confidence and a reason; doubtful files go to a review list. Nothing moves
  until you confirm, nothing is overwritten, and it can be undone.
- **Never overwrites** existing art unless you ask; never writes outside the
  library root.
- **Browse** the library by folder, by system or by date, with filters
  (missing art, system, extension, region) and search.
- Optional secondary sources (SteamGridDB, later ScreenScraper).
- A **command-line tool**, `coverly`, for headless audits, fills and
  organising.
- English and Spanish interface, light and dark themes, automatic updates from
  GitHub Releases.

Coverly ROMs does not include ROMs, BIOS files or game art.

## Organise a folder of ROMs

Pick a folder that follows no known layout and Coverly offers to organise it
instead of stopping: **Organise this folder…** shows a proposal — ROMs per
system, the target folder of each file, how sure Coverly is and why, the files
to review and the ones that stay where they are (unfinished downloads, BIOS
files, web pages saved as `.zip`, duplicates). Choose the layout, the target
(this folder or another one, such as an SD card), move or copy and the names,
then **Organise {n} files**. When it ends, **Open library** opens the result like any
other library, ready to fill its covers; **Undo** puts every file back. A
library with stray ROMs outside its system folders offers **Organise loose
files…** from the library screen.

From the terminal:

```bash
coverly organize ~/Downloads/roms                 # dry run: proposal by system
coverly organize ~/Downloads/roms --apply         # move into ~/Downloads/roms/ROMS/<SYS>/
coverly organize ~/Downloads/roms --target /Volumes/SD --copy --apply
coverly undo                                      # put everything back
```

How it works: [docs/04-architecture/organize.md](docs/04-architecture/organize.md).

## Install

Download the file for your computer from the
[latest release](https://github.com/MAECLY/coverly-roms/releases/latest):

| System                                      | File                                                                                |
| ------------------------------------------- | ----------------------------------------------------------------------------------- |
| macOS 11 or newer (Apple silicon and Intel) | `Coverly ROMs_<version>_universal.dmg`                                              |
| Windows 10/11 (x64)                         | `Coverly ROMs_<version>_x64-setup.exe` (recommended) or `.msi`                      |
| Linux (x64, ARM64)                          | `.AppImage` or `.deb`                                                               |
| Command line (`coverly`)                    | `coverly-<version>-<os>-<arch>` (`darwin`, `windows`, `linux`; `x86_64`, `aarch64`) |

Check the file against `SHA256SUMS` from the same release, for example
`shasum -a 256 -c SHA256SUMS --ignore-missing` (macOS),
`sha256sum -c SHA256SUMS --ignore-missing` (Linux) or
`Get-FileHash <file>` (Windows PowerShell).

- **macOS:** the app is not signed with an Apple certificate, so the first
  time macOS says it cannot check it. Open **System Settings → Privacy &
  Security**, scroll down, click **Open Anyway** and confirm.
- **Windows:** the installer is not signed with a Windows certificate, so
  SmartScreen may warn. Click **More info → Run anyway**.
- **Linux:** make the AppImage executable (`chmod +x Coverly*.AppImage`) and
  run it, or install the `.deb` (`sudo apt install ./Coverly*.deb`).
- **CLI:** rename the binary to `coverly` (`coverly.exe` on Windows), make it
  executable (`chmod +x`) and put it on your `PATH`.

**Updates.** The app checks for updates on start (Settings → General; turn it
off and it requests nothing) and from Settings → About, and never restarts
while a fill job is running. The CLI: `coverly update check` and
`coverly update install`. Every update is verified with the project's minisign
key before it is installed; a download that fails the check is not installed.

## Build from source

Prerequisites: [Rust](https://rustup.rs/) 1.90 or newer,
[Node.js](https://nodejs.org/) 22 or newer with [pnpm](https://pnpm.io/), and
the [Tauri 2 system dependencies](https://v2.tauri.app/start/prerequisites/)
for your OS.

```sh
make install                                   # install JS and Rust dependencies
make dev                                       # run the desktop app in development
make web                                       # only the UI, in a browser, with a mock backend
make example                                   # build a small example library and audit it
make example-fill                              # plan filling its covers (dry run)
make audit ROOT=<path>                         # audit a library from the terminal
make check                                     # lint, format check and all tests
make site                                      # serve the landing page on http://localhost:8080
```

`make help` lists every target. The example library lives in `target/example`
(`make example-apply`, `make example-undo`, `make example-clean`).

The landing page at [coverly-roms.maecly.com](https://coverly-roms.maecly.com)
lives in `site/`. `make site` shows it as it looks before the first release;
`make site RELEASE=scripts/site/fixtures/release.json` shows it with a release,
and `make site-check` runs its checks.
[docs/07-build-and-release/landing.md](docs/07-build-and-release/landing.md)
explains how it is published.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Contributions are made under the
[Contributor License Agreement](CLA.md). Security reports go through
[SECURITY.md](SECURITY.md).

## Licence and trademarks

Coverly ROMs is free software, licensed under the
[GNU Affero General Public License v3.0 only](LICENSE) (`AGPL-3.0-only`).
Copyright © 2026 Miguel Angel Esparza Calero (MAECLY).

The name **Coverly ROMs**, its wordmark and its mark are trademarks of MAECLY
and are not licensed under the AGPL (section 7(e)); forks must use their own
name. See [TRADEMARKS.md](TRADEMARKS.md) and
[docs/09-legal/](docs/09-legal/license.md).

Game art is fetched from third-party sources at your request and belongs to
its owners; see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md). All other
product names mentioned are trademarks of their respective owners.
