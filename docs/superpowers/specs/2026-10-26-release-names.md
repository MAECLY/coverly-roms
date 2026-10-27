# Coverly ROMs — Release File Names and Windows ARM64 (Design Spec)

> Status: **Draft** · Date: 2026-10-26 · Owner: Miguel Angel Esparza Calero (MAECLY)
> Roadmap item: TODO.md §0 «Nombres de los archivos de cada release y Windows ARM».
> Branches: `chore/release-names-windows-arm` (this repository) and `feat/release-names` (`MAECLY/coverly-roms-landing`).
> Related: [ADR-0011](../../adr/0011-release-pipeline-and-update-key.md) (release pipeline and update key), [`docs/04-architecture/updates.md`](../../04-architecture/updates.md), [`RELEASING.md`](../../../RELEASING.md), [`docs/07-build-and-release/ci-cd.md`](../../07-build-and-release/ci-cd.md).

## 1. Goals

1. Every downloadable file of a release says, in plain words, which system and processor it is for, with one naming scheme shared by the app and the CLI.
2. Add native Windows ARM64 builds of the app (NSIS and MSI) and of the CLI.
3. Ship separate macOS builds for Apple silicon and Intel instead of one universal DMG.
4. Copies already installed (0.1.0, 0.2.0) keep updating with no action from their users, proven by tests of the manifest.
5. The landing (`coverly-roms-landing`) offers the right file for the visitor's system with both the old names (the live v0.2.0 release) and the new ones (the next release).

Non-goals: Authenticode or Apple Developer ID signing (ADR-0008), changing the Tauri bundle names inside the build, changing the updater endpoint or the manifest format, any app UI.

## 2. The owner's asks (TODO.md §0, verbatim summary)

Today the files carry Tauri's names (`Coverly.ROMs_0.2.0_universal.dmg`, `_x64-setup.exe`, `_amd64.AppImage`, `_aarch64.AppImage`, `_arm64.deb`…) and the CLI is `coverly-0.2.0-darwin-aarch64`. They mix `amd64`/`x64` and `aarch64`/`arm64` and do not name the system in normal words. Proposed suffixes, the same for the app and the CLI:

| Platform | Suffix |
|---|---|
| macOS Apple silicon | `_mac_arm` (or `_mac_silicon`; decide one) |
| macOS Intel | `_mac_intel` |
| Linux x64 | `_linux_x64` |
| Linux ARM64 | `_linux_arm` |
| Windows x64 | `_windows_x64` |
| Windows ARM64 (new) | `_windows_arm` |

Examples given: `Coverly-ROMs_0.3.0_mac_arm.dmg`, `Coverly-ROMs_0.3.0_windows_x64-setup.exe`, `Coverly-ROMs_0.3.0_linux_arm.AppImage`, `coverly_0.3.0_linux_x64`.

Tasks:

- Decide `_mac_arm` or `_mac_silicon`, and whether to keep the universal `.dmg` or split into two smaller builds (then the macOS `app` job becomes two: `aarch64-apple-darwin` and `x86_64-apple-darwin`).
- Rename in the staging step of `release.yml` (`release_tools.py stage-builds`), not in Tauri, so the names the updater expects inside are untouched.
- Update `latest.json` (each platform's url), `SHA256SUMS`, `.github/release-notes.md` (en/es download table), `RELEASING.md` §5, README en/es and the landing (`site/assets/release.js`).
- Compatibility: installed versions read `latest.json` by platform key (`darwin-aarch64`, `windows-x86_64`…), not by file name. Check it with a real update from 0.2.0 to the next version.
- Windows ARM64: add the `aarch64-pc-windows-msvc` build of the app (NSIS and MSI) and of the CLI on the `windows-11-arm` runner, its `windows-aarch64` entry in `latest.json` and in the notes, and check that WebView2 and the installer work on ARM.

## 3. Decisions

| # | Decision | Why |
|---|---|---|
| D1 | **`_mac_arm`**, not `_mac_silicon`. | Shorter, and parallel with `_linux_arm` / `_windows_arm`: one word per processor family across the table. "Apple silicon" is still the wording in the notes and on the landing, where people read it. |
| D2 | **Two macOS builds, no universal DMG.** The macOS `app` job becomes `aarch64-apple-darwin` and `x86_64-apple-darwin`, both on `macos-latest` (Apple silicon; Intel is cross-compiled with `--target x86_64-apple-darwin`). | Each DMG and update archive is about half the size; every other platform already ships one file per processor. Cross-compiling avoids the retired Intel macOS runners. |
| D3 | **Rename in `release_tools.py stage-builds` only.** `app` and `cli` jobs keep handing over Tauri's and Cargo's names (sanitised for GitHub, as today); `stage-builds` checks each artifact against its platform's Tauri patterns and copies every file under its release name. | One place owns the mapping and it is unit-tested; the build jobs (read only, third-party code) do not choose release names; Tauri's internal names (the `.app` inside the `.tar.gz`, NSIS/MSI product names, install folders) do not change, so updates replace the same installation. |
| D4 | **Manifest keys are unchanged**: `latest.json` keeps the Tauri platform keys (`darwin-aarch64`, `windows-x86_64`, `linux-aarch64`…), the bundle-specific keys (`darwin-aarch64-app`, `windows-x86_64-nsis`, `windows-x86_64-msi`, `linux-x86_64-appimage`, `linux-x86_64-deb`…) and the CLI keys `cli-<os>-<arch>`. Only the urls change. New keys: `windows-aarch64`, `windows-aarch64-nsis`, `windows-aarch64-msi`, `cli-windows-aarch64`. | The app's updater and `coverly update install` look entries up by key and download by url; neither checks the file name (the signature's trusted comment carries `file:<name>`, which `finalize` writes when it signs the renamed file and nobody verifies). |
| D5 | **App prefix `Coverly-ROMs_`, CLI prefix `coverly_`**, `_` between name, version and platform. Windows `.msi` drops Tauri's `_en-US` locale (`Coverly-ROMs_<V>_windows_x64.msi`): only one WiX language is built. CLI on Windows keeps `.exe`. | Matches the owner's examples. `-` stays only where it is part of the kind (`-setup.exe`) or of a prerelease version (`0.3.0-beta.1`), which never contains `_`, so names stay unambiguous. |
| D6 | **Windows ARM64 on the `windows-11-arm` runner**, natively (no `--target`), for `app` (NSIS, plus MSI on a stable tag, same WiX rule as x64) and `cli`. | Free for public repositories; a native build runs the same steps as x64. |
| D7 | **Installed x64 copies on Windows ARM stay x64.** The updater asks for the key of the build that is running (`windows-x86_64-*`), so an emulated x64 install keeps receiving x64 updates. The notes tell ARM users to install the ARM64 build once. | No automatic architecture migration: it would mean an updater installing a different product build, which Tauri does not do and we do not want to fake. |
| D8 | **Universal installs move to single-architecture updates.** An installed universal 0.2.0 on Apple silicon asks for `darwin-aarch64-app` and receives the `mac_arm` archive; on Intel, `darwin-x86_64-app` → `mac_intel`. | The updater replaces the `.app` with the archive's `.app`; same bundle identifier and name, smaller app. |
| D9 | **The landing accepts both schemes** until no live release carries the old names: it tries the new name first, then the old one. Windows ARM64 links appear only when the release has them. | v0.2.0 is live with old names until the next release; the next one has only new names. |
| D10 | **No real release, tag or `release.yml` run** is part of this work. Validation is `actionlint`, the Rust tests that drive `release_tools.py`, a local staging dry run with fake artifacts and the landing tests. The real 0.2.0 → next update and the ARM installer/WebView2 check stay owner steps (TODO.md, after the next release). | Releases are the owner's call; the pipeline only drafts. |

## 4. Name table

`<V>` is the version without `v` (`0.3.0`, `0.3.0-beta.1`).

| Artifact (workflow) | Tauri / Cargo name handed over | Release name |
|---|---|---|
| `app-darwin-aarch64` | `Coverly.ROMs_<V>_aarch64.dmg` | `Coverly-ROMs_<V>_mac_arm.dmg` |
| | `Coverly.ROMs_<V>_aarch64.app.tar.gz` ¹ | `Coverly-ROMs_<V>_mac_arm.app.tar.gz` |
| `app-darwin-x86_64` | `Coverly.ROMs_<V>_x64.dmg` | `Coverly-ROMs_<V>_mac_intel.dmg` |
| | `Coverly.ROMs_<V>_x64.app.tar.gz` ¹ | `Coverly-ROMs_<V>_mac_intel.app.tar.gz` |
| `app-windows-x86_64` | `Coverly.ROMs_<V>_x64-setup.exe` | `Coverly-ROMs_<V>_windows_x64-setup.exe` |
| | `Coverly.ROMs_<V>_x64_en-US.msi` (stable only) | `Coverly-ROMs_<V>_windows_x64.msi` |
| `app-windows-aarch64` | `Coverly.ROMs_<V>_arm64-setup.exe` | `Coverly-ROMs_<V>_windows_arm-setup.exe` |
| | `Coverly.ROMs_<V>_arm64_en-US.msi` (stable only) | `Coverly-ROMs_<V>_windows_arm.msi` |
| `app-linux-x86_64` | `Coverly.ROMs_<V>_amd64.AppImage` | `Coverly-ROMs_<V>_linux_x64.AppImage` |
| | `Coverly.ROMs_<V>_amd64.deb` | `Coverly-ROMs_<V>_linux_x64.deb` |
| `app-linux-aarch64` | `Coverly.ROMs_<V>_aarch64.AppImage` | `Coverly-ROMs_<V>_linux_arm.AppImage` |
| | `Coverly.ROMs_<V>_arm64.deb` | `Coverly-ROMs_<V>_linux_arm.deb` |
| `cli-darwin-aarch64` | `coverly-<V>-darwin-aarch64` | `coverly_<V>_mac_arm` |
| `cli-darwin-x86_64` | `coverly-<V>-darwin-x86_64` | `coverly_<V>_mac_intel` |
| `cli-windows-x86_64` | `coverly-<V>-windows-x86_64.exe` | `coverly_<V>_windows_x64.exe` |
| `cli-windows-aarch64` | `coverly-<V>-windows-aarch64.exe` | `coverly_<V>_windows_arm.exe` |
| `cli-linux-x86_64` | `coverly-<V>-linux-x86_64` | `coverly_<V>_linux_x64` |
| `cli-linux-aarch64` | `coverly-<V>-linux-aarch64` | `coverly_<V>_linux_arm` |

¹ Tauri's macOS update archive is `Coverly ROMs.app.tar.gz` with no version or architecture; the `app` job's existing staging step appends `_<V>_<tauri arch>` (`aarch64` or `x64`, a new matrix field) exactly as it appends `_universal` today, so `stage-builds` can check it like every other file.

A stable release has 12 app files + 6 CLI binaries + their `.sig` files + `latest.json` + `SHA256SUMS`; a prerelease has 10 app files (no `.msi`).

`latest.json` (stable) maps:

| Key | File |
|---|---|
| `darwin-aarch64`, `darwin-aarch64-app` | `Coverly-ROMs_<V>_mac_arm.app.tar.gz` |
| `darwin-x86_64`, `darwin-x86_64-app` | `Coverly-ROMs_<V>_mac_intel.app.tar.gz` |
| `windows-x86_64`, `windows-x86_64-nsis` | `Coverly-ROMs_<V>_windows_x64-setup.exe` |
| `windows-x86_64-msi` | `Coverly-ROMs_<V>_windows_x64.msi` |
| `windows-aarch64`, `windows-aarch64-nsis` | `Coverly-ROMs_<V>_windows_arm-setup.exe` |
| `windows-aarch64-msi` | `Coverly-ROMs_<V>_windows_arm.msi` |
| `linux-x86_64`, `linux-x86_64-appimage` | `Coverly-ROMs_<V>_linux_x64.AppImage` |
| `linux-x86_64-deb` | `Coverly-ROMs_<V>_linux_x64.deb` |
| `linux-aarch64`, `linux-aarch64-appimage` | `Coverly-ROMs_<V>_linux_arm.AppImage` |
| `linux-aarch64-deb` | `Coverly-ROMs_<V>_linux_arm.deb` |
| `cli-<os>-<arch>` (6 keys) | `coverly_<V>_<suffix>[.exe]` |

## 5. Architecture and files touched

### 5.1 `scripts/release/release_tools.py`

- One table, `RELEASE_PLATFORMS`: suffix → (`os`, `arch`) — `mac_arm` → (`darwin`, `aarch64`), `mac_intel` → (`darwin`, `x86_64`), `windows_x64`, `windows_arm`, `linux_x64`, `linux_arm`. Every other mapping derives from it.
- `build_artifacts(version)` returns, per expected workflow artifact, its destination (`bundles`/`cli`), its release suffix and `{kind: Tauri pattern}`; kinds stay the labels already used (`.dmg`, `.app.tar.gz`, `-setup.exe`, `.msi`, `.AppImage`, `.deb`, `CLI binary`). Expected artifacts: six `app-<os>-<arch>` and six `cli-<os>-<arch>`.
- `release_name(suffix, kind, version)`: `Coverly-ROMs_<V>_<suffix><kind>` for the app, `coverly_<V>_<suffix>[.exe]` for the CLI. `stage_builds` copies each matched file to its release name instead of its own name. All existing refusals stay (extra or missing artifact, another platform's file, a `.sig`, a symlink, zero or two files of one kind, non-empty destination, all-or-nothing copy); new refusal: two sources mapping to one release name (defensive; impossible with the table).
- `updater_platforms`: replace `ARCH_TOKENS`/`ARCH_RE` (Tauri tokens, `universal`) with a parser of the release suffix, `_(mac_arm|mac_intel|windows_x64|windows_arm|linux_x64|linux_arm)(?=[-.])`. The error "no architecture in the file name" becomes "no platform in the file name". The generic-key preference (`app`; `appimage`; `nsis` then `msi`) is unchanged and now also yields `windows-aarch64`.
- `CLI_FILE` / `add_cli`: parse `coverly_<V>_<suffix>[.exe]` (`.exe` required for Windows, refused elsewhere) into `cli-<os>-<arch>`.
- Docstring and the `--help` text describe the new names. `list-artifacts` and `resign-manifest` are name-agnostic and do not change (they still recover old manifests).

### 5.2 `.github/workflows/release.yml`

- `app` matrix: `macOS Apple silicon` (`slug: darwin-aarch64`, `macos-latest`, `targets: aarch64-apple-darwin`, `args: --target aarch64-apple-darwin`, `tauri_arch: aarch64`), `macOS Intel` (`darwin-x86_64`, `macos-latest`, `x86_64-apple-darwin`, `--target x86_64-apple-darwin`, `x64`), `Windows x64`, **`Windows ARM64`** (`windows-aarch64`, `windows-11-arm`, `aarch64-pc-windows-msvc`, no args), `Linux x64`, `Linux ARM64`.
- The `app` staging step uses `TAURI_ARCH` instead of the literal `universal` for the `.app.tar.gz`; everything else in that step stays.
- `cli` matrix: add `{ platform: windows-11-arm, target: aarch64-pc-windows-msvc, os: windows, arch: aarch64 }`.
- Windows ARM toolchain note: `ring` (TLS) needs `clang` for `aarch64-pc-windows-msvc`. A step on that runner checks `clang --version` and, if absent, adds the image's LLVM `bin` folder to `PATH`; it fails with a `::error::` naming the missing tool instead of a deep `cc` error.
- `finalize`: `check-manifest` also requires `windows-aarch64` and `cli-windows-aarch64` (12 keys); the `SHA256SUMS` kind check is unchanged (every kind still exists).
- `release-channels.yml`: the same two `--require` additions.
- Comments mentioning "universal" or "the only macOS build is universal" are rewritten.

### 5.3 Tests and docs

- `src-tauri/tests/release_pipeline.rs`: fixtures and expectations move to the new names; new tests in §9.
- `.github/release-notes.md` (en/es table, §7), `RELEASING.md` (§1 job table, §5 checklist, §7 local builds, §8 `add-cli` names, §9 troubleshooting row), `README.md` / `README.es.md` (download table, Windows and Linux notes), `docs/07-build-and-release/ci-cd.md` (`app`/`cli`/`finalize` rows), `docs/04-architecture/updates.md` (example manifest and mapping), ADR-0011 (an "Amended 2026-10-26" note: separate macOS builds, Windows ARM64, release names chosen in `stage-builds`), `CHANGELOG.md` `[Unreleased]`.
- Not touched on this branch: `docs/TRACKER.md`, `TODO.md` (updated on `main` after the merge), `src-tauri/tauri.conf.json`, `crates/coverly-core` (the CLI key code already covers `windows` + `aarch64`), `locales/*`.

### 5.4 Landing (`coverly-roms-landing`, branch `feat/release-names`)

- `scripts/site/apply_release.py`: each download slot gets a list of patterns, new first, legacy second (`macos_arm`: `Coverly-ROMs_{v}_mac_arm\.dmg` then the legacy `…_universal\.dmg`; `macos_intel`: `…_mac_intel\.dmg` then the same universal DMG; `windows_x64`, `windows_x64_msi`, `linux_*`, `cli_*` likewise). `windows_arm`, `windows_arm_msi` and `cli_windows_arm64` are **optional** slots with no legacy pattern: when absent, the HTML between `<!-- asset:<slot>:begin -->` and `<!-- asset:<slot>:end -->` is dropped; every other slot stays required (a missing file fails the build, as today). When both macOS slots resolve to the same universal file, the Intel link is dropped too (one button, as today).
- `site/assets/release.js`: `apply()` maps each built link to the newer release by trying the version swap on the same name and then the translated name (legacy → new table); any missing file still leaves the built page alone. `offer()` picks by `data-os` and architecture: `navigator.userAgentData.getHighEntropyValues(["architecture"])` when available (Chromium), else Apple silicon on macOS and x64 on Windows/Linux (the UA's `aarch64|arm64` hint still selects Linux ARM). It tries the new name, then the legacy one.
- `site/index.html`, `site/en/index.html`, `site/llms.txt`, JSON-LD `downloadUrl`: cards and CLI list per §6; `scripts/site/fixtures/`: one release fixture with old names (v0.2.0 shape) and one with new names; `release.test.mjs` and the Python tests cover both.

## 6. UX and UI (landing only; the app has no UI change)

The app does not show release file names anywhere (the update panel shows versions and notes), so there is no app UI, copy or locale change, and `locales/{es,en}/ui.json` and their parity test are untouched.

The landing change is small and lives inside its existing download cards, so the plan is to add nothing new to its visual system.

- **Tokens:** none new. Primary buttons keep `.btn-primary`; secondary downloads keep the existing `.alt` link; sizes keep the existing `MiB` format (comma decimal in Spanish).
- **Type:** the landing's current faces and scale; processor names are plain words in the label ("Apple silicon", "Intel", "ARM64"), not chips or mono tags.
- **Layout:** the four cards stay four. The macOS card's primary button is Apple silicon, with an `.alt` line for Intel under it, mirroring how the Windows card already offers the `.msi` as an `.alt`. The Windows card gains a second `.alt` line for ARM64. Linux cards are unchanged apart from the file names behind them.

```
┌ macOS ─────────────────────────┐  ┌ Windows ───────────────────────┐
│ macOS 11 o posterior           │  │ Windows 10 y 11, x64 o ARM64   │
│ [ Descargar para Apple silicon │  │ [ Descargar instalador (x64) ] │
│   (.dmg, 6,1 MiB) ]            │  │ O el .msi (x64)                │
│ O para Mac con Intel (.dmg)    │  │ O el instalador para ARM64     │
└────────────────────────────────┘  └────────────────────────────────┘
```

- **Motion:** none added. The existing "Tu equipo" highlight keeps working by `data-os`.
- **Review against generic defaults:** the obvious default would be a platform picker with tabs or a grid of six identical tiles. That would replace a working, already-tested layout and add a decision step for most visitors. Keeping one primary action per card plus plain text alternatives follows the existing `.msi` pattern and keeps the choice where the visitor already looks.

## 7. Copy (en / es)

Spanish follows the house rules: sentence case, no exclamation marks, exact counts.

Release notes table (`.github/release-notes.md`):

| System (en) | Sistema (es) | File |
|---|---|---|
| macOS 11 or later, Apple silicon (M1 or newer) | macOS 11 o posterior, Apple silicon (M1 o posterior) | `Coverly-ROMs_<version>_mac_arm.dmg` |
| macOS 11 or later, Intel | macOS 11 o posterior, Intel | `Coverly-ROMs_<version>_mac_intel.dmg` |
| Windows 10/11 (x64) | Windows 10/11 (x64) | `Coverly-ROMs_<version>_windows_x64-setup.exe` (recommended / recomendado); stable releases also / las versiones estables también `…_windows_x64.msi` |
| Windows 11 (ARM64) | Windows 11 (ARM64) | `Coverly-ROMs_<version>_windows_arm-setup.exe`; stable releases also `…_windows_arm.msi` |
| Linux x64 | Linux x64 | `Coverly-ROMs_<version>_linux_x64.AppImage` or/o `…_linux_x64.deb` |
| Linux ARM64 | Linux ARM64 | `Coverly-ROMs_<version>_linux_arm.AppImage` or/o `…_linux_arm.deb` |
| Command line | Línea de comandos | `coverly_<version>_<system>` (`mac_arm`, `mac_intel`, `windows_x64.exe`, `windows_arm.exe`, `linux_x64`, `linux_arm`) |

Added sentences:

- en: "Not sure which Mac you have? Apple menu → About This Mac: "Chip" means Apple silicon, "Processor" means Intel." / es: "¿No sabes qué Mac tienes? Menú Apple → Acerca de este Mac: «Chip» es Apple silicon, «Procesador» es Intel."
- en: "On a Windows ARM64 PC, install the ARM64 build. A copy of the x64 build installed earlier keeps working and updating as x64." / es: "En un PC con Windows ARM64, instala la versión ARM64. Una copia de la versión x64 instalada antes sigue funcionando y actualizándose como x64."
- The quarantine line for the CLI becomes `xattr -d com.apple.quarantine coverly_<version>_mac_arm` (or `_mac_intel`).

Landing labels (es / en): «Descargar para Apple silicon (.dmg, N MiB)» / "Download for Apple silicon (.dmg, N MiB)"; «O para Mac con Intel (.dmg, N MiB)» / "Or for an Intel Mac (.dmg, N MiB)"; «O el instalador para ARM64 (N MiB)» / "Or the ARM64 installer (N MiB)"; requirement line «Windows 10 y 11, x64 o ARM64» / "Windows 10 and 11, x64 or ARM64"; CLI list adds «Windows ARM64» / "Windows ARM64". The "download for your system" button names the platform: «Descargar para macOS Apple silicon», «… Windows ARM64», etc.

## 8. Data flow

1. Tag push → `guard`, `draft` (unchanged).
2. Six `app` jobs build with Tauri's names; the staging step only flattens, sanitises spaces and versions the `.app.tar.gz`; each uploads `app-<os>-<arch>`.
3. Six `cli` jobs upload `cli-<os>-<arch>` with `coverly-<V>-<os>-<arch>[.exe]`.
4. `finalize` downloads each artifact into its own folder → `stage-builds` validates every artifact against its platform's Tauri patterns and copies all files, renamed, into `bundles/` and `cli/` (or nothing) → `updater-artifacts` lists the update files → signing (trusted comment `file:<release name>`, `version:<V>`) → `updater-manifest` + `add-cli` → `check-manifest` (12 keys, version-bound signatures) → `minisign` verification and the tampered copy → `SHA256SUMS` over release names → upload to the draft.
5. Publishing → `release-channels.yml` checks the same 12 keys before moving the beta manifest.
6. An installed app asks `releases/latest/download/latest.json`, looks up `<os>-<arch>-<bundle>` then `<os>-<arch>`, downloads the url (whatever its name), verifies the signature and version, installs. The CLI does the same with `cli-<os>-<arch>`.
7. The landing: at deploy, `apply_release.py` resolves each slot (new, then legacy pattern); in the browser, `release.js` upgrades the built page to a newer release under either scheme and offers the visitor's file.

## 9. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| Installed 0.1.0/0.2.0 universal Mac | Gets the single-architecture archive for its key (D8). Tested by the compatibility test. |
| Installed 0.2.0 x64 on a Windows ARM PC | Keeps x64 updates (D7); notes explain installing ARM64. |
| Prerelease tag (`v0.3.0-beta.1`) | No `.msi` on either Windows artifact; `stage-builds` refuses an `.msi` there, as today; manifest has no `windows-*-msi` keys. |
| A build hands over a file of another platform (e.g. `_x64.dmg` in `app-darwin-aarch64`) | `stage-builds` fails: patterns are per artifact. |
| Windows ARM runner lacks `clang` or WiX fails on ARM64 | The job fails before upload, so no draft gets partial files; the error names the tool. Fallback if WiX cannot build ARM64 MSIs: drop the ARM64 `.msi` kind from `build_artifacts` and pass `--bundles nsis` on that runner (one-line change each), and the `windows-aarch64-msi` key disappears; NSIS remains. |
| Release name containing a character GitHub rewrites | Impossible by construction (`[A-Za-z0-9._-]` only); `stage-builds` asserts it with `check_file_name` plus a charset check. |
| A stray old-style name in `bundles/` (someone runs `updater-manifest` by hand on old files) | Fails with "no platform in the file name"; recovery of old manifests uses `list-artifacts`/`resign-manifest`, which are name-agnostic. |
| Landing built against v0.2.0, newer release published with new names | `release.js` translates each link and upgrades the page; the ARM64 Windows link appears after the Vercel redeploy that the release triggers. |
| Visitor on Safari on an Intel Mac | No architecture hint: the primary offer is Apple silicon; the Intel link is always visible in the macOS card, and the notes explain how to tell. |
| Windows ARM visitor on a browser without `userAgentData` | Offered x64 (runs under emulation); the ARM64 link is visible in the card. |

## 10. Test plan

### Python via Rust (`src-tauri/tests/release_pipeline.rs`, `cargo test --workspace`)

- `stage_builds_renames_every_platform`: fake `downloads/` tree with the 12 artifacts and Tauri/Cargo names (stable), run `stage-builds`, compare the sorted `bundles/` and `cli/` listings with §4 exactly; contents copied byte for byte.
- `stage_builds_prerelease_has_no_msi`: same for `0.3.0-beta.1`; an `.msi` present → failure, nothing copied.
- Refusals kept and extended: a `_x64.dmg` in `app-darwin-aarch64`, a missing `app-windows-aarch64`, a leftover `app-macos-universal`, a `.sig`, a symlink, a non-empty destination.
- `updater_manifest_maps_release_names_to_tauri_keys`: from staged names + fake `.sig` files, the manifest has exactly the §4 keys and urls (percent-encoding unchanged), and `check-manifest` with the 12 required keys passes.
- `installed_versions_find_their_update` (compatibility proof): for every key a 0.1.0/0.2.0 app or CLI asks — `darwin-aarch64[-app]`, `darwin-x86_64[-app]`, `windows-x86_64[-nsis|-msi]`, `linux-x86_64[-appimage|-deb]`, `linux-aarch64[-appimage|-deb]`, `cli-darwin-aarch64`, `cli-darwin-x86_64`, `cli-windows-x86_64`, `cli-linux-x86_64`, `cli-linux-aarch64` — the new manifest has the key, and its file is of the same bundle kind and the same processor (e.g. `windows-x86_64-msi` → `_windows_x64.msi`, `darwin-x86_64-app` → `_mac_intel.app.tar.gz`). The manifest also parses with `coverly_core::update::parse_manifest`, and `cli_platform_key_for` for each (os, arch) pair, `windows`/`aarch64` included, resolves to an entry.
- `add_cli_reads_release_names`: six binaries; `coverly_<V>_linux_x64.exe` and `coverly_<V>_windows_x64` (no `.exe`) refused.
- Workflow assertions: the `app` matrix has six entries including `windows-11-arm` and the two macOS targets, no `universal-apple-darwin`; the `cli` matrix has six; `finalize` and `release-channels.yml` require 12 keys; the `app` staging step script, run on a fake tree, produces versioned `.app.tar.gz` names for both macOS architectures.
- Release notes test: both tables name every new file pattern and none of the old ones.

### Static

`actionlint` on `release.yml` and `release-channels.yml`; `python3 -I scripts/release/release_tools.py --help` and each subcommand's `--help`.

### Local staging dry run

A throwaway script under the scratchpad builds a fake `downloads/` tree (both stable and beta), runs `stage-builds`, signs nothing (fake `.sig` text), runs `updater-manifest`, `add-cli`, `check-manifest` without `--signed-version`, then `sha256sum`, and prints the listing; output compared with §4 by hand and kept as PR evidence. Fixture paths are compared by component, never by slash text, and mtimes are not relied on.

### Repository gates

`pnpm check-all`, `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`. No Vitest changes are expected (no frontend code changes); the suite must stay green.

### Landing

`make check` (Python tests of `apply_release.py`, `release.test.mjs`, links, parity, budgets): both fixtures resolve every required slot; the legacy fixture drops the ARM64 Windows and Intel-only blocks; `release.js` upgrades a page built with old names to a release with new names and leaves it alone when one file is missing; `offer()` picks each of the six files from emulated `data-os` + architecture hints and falls back to the legacy name. Browser E2E with Chrome DevTools MCP (`make site RELEASE=<fixture>` on port 1461, emulated user agents for macOS, Windows ARM and Linux ARM): the right primary button and the alternatives are shown, links point at the fixture's names. PR in English, green CI, merge.

### Owner (after the next real release, kept in TODO.md)

Update a real 0.2.0 install to the new version on macOS (both chips if available), Windows x64 and Linux; install the ARM64 NSIS and MSI on a Windows 11 ARM machine and check that WebView2 starts.

## 11. Self-review

- No placeholders: every file name, key and matrix entry is spelled out (§4, §5).
- D2 (no universal) is consistent with §4 (no `universal` row), §5.2 (two macOS entries) and the workflow test ("no `universal-apple-darwin`").
- D4 (keys unchanged) is consistent with §4's key table and the compatibility test; the only new keys are the Windows ARM64 ones.
- D3 (rename only in `stage-builds`) holds: the `app` staging step still only versions the `.app.tar.gz` with Tauri's own architecture token, as it already did with `universal`.
- The `.msi` locale drop (D5) is reflected in the notes table and the compatibility test (`windows-x86_64-msi` → `_windows_x64.msi`).
- The landing keeps working for v0.2.0 (legacy patterns, optional ARM64 slots) and for the next release (new patterns first).
