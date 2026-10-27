# Coverly ROMs — Release File Names and Windows ARM64 (Design Spec)

> Status: **Draft, reviewed** (adversarial review 2026-10-26, see §12) · Date: 2026-10-26 · Owner: Miguel Angel Esparza Calero (MAECLY)
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
| D2 | **Two macOS builds, no universal DMG.** The macOS `app` job becomes `aarch64-apple-darwin` and `x86_64-apple-darwin`, both on `macos-latest` (Apple silicon; Intel is cross-compiled with `--target x86_64-apple-darwin`). | Each DMG and update archive is about half the size; every other platform already ships one file per processor. Cross-compiling avoids the retired Intel macOS runners (the `cli` job already cross-compiles `x86_64-apple-darwin` there). Cost: CI never *runs* the Intel app, so the first real Intel launch is an owner check (§10). The Intel build also runs on Apple silicon under Rosetta, but the Apple silicon build does not run on an Intel Mac, so every place that offers the Apple silicon DMG also shows the Intel one next to it. |
| D3 | **Rename in `release_tools.py stage-builds` only.** `app` and `cli` jobs keep handing over Tauri's and Cargo's names (sanitised for GitHub, as today); `stage-builds` checks each artifact against its platform's Tauri patterns and copies every file under its release name. | One place owns the mapping and it is unit-tested; the build jobs (read only, third-party code) do not choose release names; Tauri's internal names (the `.app` inside the `.tar.gz`, NSIS/MSI product names, install folders) do not change, so updates replace the same installation. |
| D4 | **Manifest keys are unchanged**: `latest.json` keeps the Tauri platform keys (`darwin-aarch64`, `windows-x86_64`, `linux-aarch64`…), the bundle-specific keys (`darwin-aarch64-app`, `windows-x86_64-nsis`, `windows-x86_64-msi`, `linux-x86_64-appimage`, `linux-x86_64-deb`…) and the CLI keys `cli-<os>-<arch>`. Only the urls change. New keys: `windows-aarch64`, `windows-aarch64-nsis`, `windows-aarch64-msi`, `cli-windows-aarch64`. | The app's updater and `coverly update install` look entries up by key and download by url; neither checks the file name (the signature's trusted comment carries `file:<name>`, which `finalize` writes when it signs the renamed file and nobody verifies). |
| D5 | **App prefix `Coverly-ROMs_`, CLI prefix `coverly_`**, `_` between name, version and platform. Windows `.msi` drops Tauri's `_en-US` locale (`Coverly-ROMs_<V>_windows_x64.msi`): only one WiX language is built. CLI on Windows keeps `.exe`. | Matches the owner's examples. `-` stays only where it is part of the kind (`-setup.exe`) or of a prerelease version (`0.3.0-beta.1`), which never contains `_`, so names stay unambiguous. |
| D6 | **Windows ARM64 on the `windows-11-arm` runner**, natively (no `--target`), for `app` (NSIS, plus MSI on a stable tag, same WiX rule as x64) and `cli`. Supported system: **Windows 11 on ARM64** only (Windows 10 on ARM is out of support and untested); x64 keeps "Windows 10 and 11". | Free for public repositories (the `guard` job already refuses a private repository); a native build runs the same steps as x64. The lockfile already carries the `win32-arm64-msvc` packages the build needs (`@tauri-apps/cli`, `@rolldown/binding`, `lightningcss`, `@resvg/resvg-js`), so `pnpm install --frozen-lockfile` works there unchanged. That runner image differs from `windows-latest`, so the plan does not assume its tools (§5.2): rustup, `clang` (needed by `ring`, which `rustls` pulls into both the app and the CLI) and .NET Framework 3.5 (needed by WiX 3, which Tauri runs for the `.msi`) are checked by a step and the job fails with a `::error::` naming the tool. |
| D7 | **Installed x64 copies on Windows ARM stay x64.** The updater asks for the key of the build that is running (`windows-x86_64-*`), so an emulated x64 install keeps receiving x64 updates. The notes tell ARM users to install the ARM64 build once. Both NSIS builds install per user into the same folder under the same product name, so the ARM64 installer is expected to replace an x64 NSIS copy in place (and the ARM64 `.msi` an x64 `.msi`, same upgrade code); this is unverified and is an owner check on real hardware (§10). Until it is confirmed the notes only say "install the ARM64 build", not "it replaces the x64 copy". | No automatic architecture migration: it would mean an updater installing a different product build, which Tauri does not do and we do not want to fake. |
| D8 | **Universal installs move to single-architecture updates.** An installed universal 0.2.0 on Apple silicon asks for `darwin-aarch64-app` and receives the `mac_arm` archive; on Intel, `darwin-x86_64-app` → `mac_intel`. | The updater replaces the `.app` with the archive's `.app`; same bundle identifier and name, smaller app. |
| D9 | **The landing accepts both schemes at build time** until no live release carries the old names: `apply_release.py` tries the new name first, then the old one, and renders the matching layout (one universal macOS button for an old release; Apple silicon plus Intel for a new one). Windows ARM64 links appear only when the release has them. **In the browser, `release.js` does not translate between schemes**: a page built against an old-name release stays as built when the newer release uses new names (its all-or-nothing check already finds no file and leaves the page alone), and `landing-redeploy.yml` rebuilds it minutes after publishing. | v0.2.0 is live with old names until the next release; the next one has only new names. A client-side translation would either point the universal "Apple silicon and Intel" button at an Apple-silicon-only DMG (an Intel visitor gets a file that does not open) or have to recreate the Intel and ARM64 blocks the build left out; the built page still links working v0.2.0 files, which then update themselves. |
| D10 | **No real release, tag or `release.yml` run** is part of this work. Validation is `actionlint`, the Rust tests that drive `release_tools.py`, a local staging dry run with fake artifacts and the landing tests. The real 0.2.0 → next update and the ARM installer/WebView2 check stay owner steps (TODO.md, after the next release). | Releases are the owner's call; the pipeline only drafts. |
| D11 | **Order of delivery: the landing PR merges before the next app release.** | The Vercel build fails (and keeps serving the previous deployment) when a required download is missing; a release with new names published before the landing accepts them would leave the landing on v0.2.0 until a fix. With the landing first, both the current v0.2.0 and the next release build. |
| D12 | **No change to the installed apps' code, the updater plugin or the CLI updater.** | Compatibility comes entirely from keeping the manifest keys (D4); the tests prove it against the code that 0.1.0 and 0.2.0 already run (`tauri-plugin-updater` `~2.14`, `coverly_core::update`). |

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
- `release_name(suffix, kind, version)`: `Coverly-ROMs_<V>_<suffix><kind>` for the app, `coverly_<V>_<suffix>[.exe]` for the CLI. `stage_builds` copies each matched file to its release name instead of its own name. All existing refusals stay (extra or missing artifact, another platform's file, a `.sig`, a symlink, zero or two files of one kind, non-empty destination, all-or-nothing copy); new refusals: two sources mapping to one release name (defensive: the per-kind "more than one file" check already catches, for example, two `.msi` locales), and a release name outside `[A-Za-z0-9._-]` or failing `check_file_name`. The copy keeps using `shutil.copyfile` (content only, no metadata), and the destination is built with `os.path.join`, never by joining text with `/`.
- `updater_platforms`: replace `ARCH_TOKENS`/`ARCH_RE` (Tauri tokens, `universal`) with a **full-name** parser, `Coverly-ROMs_(?P<v>…)_(?P<suffix>mac_arm|mac_intel|windows_x64|windows_arm|linux_x64|linux_arm)(?P<kind>\.app\.tar\.gz|-setup\.exe|\.msi|\.AppImage|\.deb)` with `re.fullmatch`, instead of a search for a token anywhere in the name; the suffix must also agree with the kind's OS (`.deb` with `linux_*`, …). The error "no architecture in the file name" becomes "not a release name: <name>". The generic-key preference (`app`; `appimage`; `nsis` then `msi`) is unchanged and now also yields `windows-aarch64`. `updater-manifest` keeps taking no version for the names (the version it writes comes from `--version`; `check-manifest --signed-version` already binds every signature to it).
- `CLI_FILE` / `add_cli`: parse `coverly_<V>_<suffix>[.exe]` (`.exe` required for Windows, refused elsewhere) into `cli-<os>-<arch>`.
- Docstring and the `--help` text describe the new names. `list-artifacts` and `resign-manifest` are name-agnostic and do not change (they still recover old manifests).

### 5.2 `.github/workflows/release.yml`

- `app` matrix: `macOS Apple silicon` (`slug: darwin-aarch64`, `macos-latest`, `targets: aarch64-apple-darwin`, `args: --target aarch64-apple-darwin`, `tauri_arch: aarch64`), `macOS Intel` (`darwin-x86_64`, `macos-latest`, `x86_64-apple-darwin`, `--target x86_64-apple-darwin`, `x64`), `Windows x64`, **`Windows ARM64`** (`windows-aarch64`, `windows-11-arm`, `aarch64-pc-windows-msvc`, no args), `Linux x64`, `Linux ARM64`.
- The `app` staging step uses `TAURI_ARCH` instead of the literal `universal` for the `.app.tar.gz`; everything else in that step stays.
- `cli` matrix: add `{ platform: windows-11-arm, target: aarch64-pc-windows-msvc, os: windows, arch: aarch64 }`.
- Windows ARM toolchain step, in **both** the `app` and the `cli` job, only on `windows-11-arm` (`if: matrix.platform == 'windows-11-arm'`), before "Rust toolchain":
  - `rustup`: if `rustup --version` fails, download `rustup-init.exe` for `aarch64-pc-windows-msvc` from `static.rust-lang.org` over HTTPS, check it against the `.sha256` published next to it, run it with `-y --profile minimal` and add `~/.cargo/bin` to `GITHUB_PATH`. The existing "Rust toolchain" step then runs unchanged.
  - `clang`: `ring` (pulled in by `rustls`, used by the app and the CLI) needs it for `aarch64-pc-windows-msvc`. If `clang --version` fails, add `C:\Program Files\LLVM\bin` when it exists; otherwise fail with `::error::clang is not installed on this runner`.
  - .NET Framework 3.5 (`app` only, stable tags only, the ones that build the `.msi`): WiX 3 needs it. The step checks the `NDP\v3.5` registry key and fails with a `::error::` naming it instead of a WiX error late in the build. If the image lacks it, the fallback in §9 applies.
  - Everything is plain `pwsh` or `bash` with no new third-party action, and the step never sees a secret (the `app` job's permissions stay `contents: read`).
- `finalize`: `check-manifest` also requires `windows-aarch64` and `cli-windows-aarch64` (12 keys); the `SHA256SUMS` kind check is unchanged (every kind still exists).
- `release-channels.yml`: the same two `--require` additions.
- Comments mentioning "universal" or "the only macOS build is universal" are rewritten.

### 5.3 Tests and docs

- `src-tauri/tests/release_pipeline.rs`: fixtures and expectations move to the new names; new tests in §10. Existing assertions that flip: `only_draft_and_finalize_can_write…` asserts `universal-apple-darwin` is present (now: absent, and both macOS targets present); `the_app_job_stages_bundles_that_finalize_can_map` uses `target/universal-apple-darwin/…` fixtures (now one fixture tree per macOS target); `bundle_targets_and_macos_signing` keeps `signingIdentity: "-"` (still required: an unsigned arm64 binary does not launch) with its message reworded; `release_notes_explain_gatekeeper_and_smartscreen_in_en_and_es` names `Coverly.ROMs_<version>_universal.dmg` and `Coverly.ROMs_<versión>_x64-setup.exe` (now the new names in both placeholders).
- `.github/release-notes.md` (en/es table, §7), `RELEASING.md` (§1 job table, §5 checklist, §7 local builds, §8 `add-cli` names, §9 troubleshooting row), `README.md` / `README.es.md` (download table, Windows and Linux notes), `docs/07-build-and-release/ci-cd.md` (`app`/`cli`/`finalize` rows), `docs/04-architecture/updates.md` (TL;DR "five `<os>-<arch>` keys" becomes six and five CLI keys become six; example manifest and mapping), ADR-0011 (an "Amended 2026-10-26" note: separate macOS builds, Windows ARM64, release names chosen in `stage-builds`), `CHANGELOG.md` `[Unreleased]`.
- Not touched on this branch: `docs/TRACKER.md`, `TODO.md` (updated on `main` after the merge), `src-tauri/tauri.conf.json`, `crates/coverly-core` (the CLI key code already covers `windows` + `aarch64`), `locales/*`.

### 5.4 Landing (`coverly-roms-landing`, branch `feat/release-names`)

The landing today (checked on its `main`): `apply_release.py` fills `{{release.<slot>.url|name|mib_es|mib_en}}` markers inside `<!-- release:begin/end -->` blocks from a `DOWNLOADS` table of required slots (`macos_universal`, `windows_x64`, `windows_x64_msi`, `linux_{x64,arm64}_{appimage,deb}`, five `cli_*`) and fails the build on a missing, ambiguous or unlinked download; `release.js` `apply()` upgrades a built page to a newer release only when every `a[data-asset]` link's file exists under the same name with the version swapped (all or nothing), and `offer()` only runs on a page built with `--no-release` (before the first release); `landing-redeploy.yml` in this repository rebuilds the landing when a release is published.

- `scripts/site/apply_release.py`:
  - Each slot gets a list of patterns, new first, legacy second (`macos_arm`: `Coverly-ROMs_{v}_mac_arm\.dmg`, then the legacy `[A-Za-z0-9._-]+_{v}_universal\.dmg`; `windows_x64`, `windows_x64_msi`, `linux_*`, `cli_*` likewise). The first pattern that matches exactly one asset wins; matching both schemes in one release fails (ambiguous).
  - Optional slots with no legacy pattern: `macos_intel`, `windows_arm`, `windows_arm_msi` and `cli_windows_arm64`. Every other slot stays required (a missing file fails the build, as today).
  - Two new block kinds, mirroring `release`/`norelease`: `<!-- asset:<slot>:begin/end -->` is kept (comments removed) when the slot resolved and dropped otherwise; `<!-- noasset:<slot>:begin/end -->` is the opposite. The legacy macOS wording ("Apple silicon e Intel", one "Descargar .dmg" button) sits in `noasset:macos_intel`, the new wording (Apple silicon button plus the Intel `.alt`) in `asset:macos_intel`; the Windows requirement line likewise in `noasset`/`asset:windows_arm`. Blocks are only allowed in `.html` files: JSON-LD `downloadUrl` and `llms.txt` list required slots only (a comment inside a JSON string array would break the JSON, and in a `.txt` it would be literal text), so `llms.txt` names Windows ARM64 in prose with a link to the release page, not a direct file link.
  - The "download linked nowhere" check skips an optional slot that did not resolve.
- `site/assets/release.js`: `apply()` keeps its logic (version swap on the same name, all or nothing), which already works for new→new; it does **not** translate legacy names to new ones (D9). `offer()` (pre-first-release pages only) tries the new name for `data-os` (`_mac_arm.dmg`, `_windows_x64-setup.exe`, `_linux_arm.AppImage` or `_linux_x64.AppImage` by the existing UA hint), then the legacy one. No `navigator.userAgentData` high-entropy call: it would add an async step and a privacy-sensitive hint for a path that no longer runs in production, and Safari and Firefox do not have it.
- `site/index.html`, `site/en/index.html`, `site/llms.txt`, JSON-LD `downloadUrl`: cards and CLI list per §6 and §7, the two languages changed in lockstep (the landing's parity check); `scripts/site/fixtures/`: the existing fixture (old names, v0.2.0 shape) stays, plus one with new names and one with new names but no Windows ARM64 files; `release.test.mjs` and `check_site.py` cover all three.
- `README.md` of the landing: the release section names both schemes and D11 (merge before the next app release).

## 6. UX and UI (landing only; the app has no UI change)

The app does not show release file names anywhere (the update panel shows versions and notes), so there is no app UI, copy or locale change, and `locales/{es,en}/ui.json` and their parity test are untouched.

The landing change is small and lives inside its existing download cards, so the plan is to add nothing new to its visual system.

- **Tokens:** none new. Primary buttons keep `.btn-primary`; secondary downloads keep the existing `.alt` link; sizes keep the existing `MiB` format (comma decimal in Spanish).
- **Type:** the landing's current faces and scale; processor names are plain words in the label ("Apple silicon", "Intel", "ARM64"), not chips or mono tags.
- **Layout:** the four cards stay four. The macOS card's primary button is Apple silicon, with an `.alt` line for Intel under it, mirroring how the Windows card already offers the `.msi` as an `.alt`. The Windows card gains a second `.alt` line for ARM64. Linux cards are unchanged apart from the file names behind them. Built against an old-name release (v0.2.0), the macOS and Windows cards render exactly as today (one universal `.dmg` button, "Apple silicon e Intel"; "Windows 10 y 11, x64"), through the `noasset` blocks of §5.4.

```
┌ macOS ─────────────────────────┐  ┌ Windows ───────────────────────┐
│ macOS 11 o posterior           │  │ Windows 10 y 11 (x64),         │
│                                │  │ Windows 11 (ARM64)             │
│ [ Descargar para Apple silicon │  │ [ Descargar instalador (x64) ] │
│   (.dmg, 6,1 MiB) ]            │  │ O el .msi (x64)                │
│ O para Mac con Intel (.dmg)    │  │ O el instalador para ARM64     │
└────────────────────────────────┘  └────────────────────────────────┘
```

- **Motion:** none added, so `prefers-reduced-motion` is unaffected. The existing "Tu equipo" highlight keeps working by `data-os` (it marks the card, not a file: a Mac card still shows both files).
- **Accessibility:** every link's text names its processor and kind on its own ("O para Mac con Intel (.dmg, N MiB)", "O el instalador para ARM64 (N MiB)"), so a screen-reader link list is unambiguous without the card heading; no `title`-only or icon-only links; the `.alt` links keep their current contrast and focus ring (the landing's contrast check covers them); the size stays inside the link text.
- **Review against generic defaults:** the obvious default would be a platform picker with tabs or a grid of six identical tiles. That would replace a working, already-tested layout and add a decision step for most visitors. Keeping one primary action per card plus plain text alternatives follows the existing `.msi` pattern and keeps the choice where the visitor already looks.

## 7. Copy (en / es)

Spanish follows the house rules: sentence case, no exclamation marks, exact counts.

Release notes table (`.github/release-notes.md`). The Spanish section writes `<versión>` where the English one writes `<version>` (the `draft` job replaces both), and file names stay in code font in both languages:

| System (en) | Sistema (es) | File |
|---|---|---|
| macOS 11 or later, Apple silicon (M1 or newer) | macOS 11 o posterior, Apple silicon (M1 o posterior) | `Coverly-ROMs_<version>_mac_arm.dmg` |
| macOS 11 or later, Intel | macOS 11 o posterior, Intel | `Coverly-ROMs_<version>_mac_intel.dmg` |
| Windows 10/11 (x64) | Windows 10/11 (x64) | `Coverly-ROMs_<version>_windows_x64-setup.exe` (recommended / recomendado); stable releases also / las versiones estables también `…_windows_x64.msi` |
| Windows 11 (ARM64) | Windows 11 (ARM64) | `Coverly-ROMs_<version>_windows_arm-setup.exe` (recommended / recomendado); stable releases also / las versiones estables también `…_windows_arm.msi` |
| Linux x64 | Linux x64 | `Coverly-ROMs_<version>_linux_x64.AppImage` or/o `…_linux_x64.deb` |
| Linux ARM64 | Linux ARM64 | `Coverly-ROMs_<version>_linux_arm.AppImage` or/o `…_linux_arm.deb` |
| Command line | Línea de comandos | `coverly_<version>_<system>` (`mac_arm`, `mac_intel`, `windows_x64.exe`, `windows_arm.exe`, `linux_x64`, `linux_arm`) |

Added sentences:

- en: "Not sure which Mac you have? Apple menu → About This Mac: "Chip" means Apple silicon, "Processor" means Intel." / es: "¿No sabes qué Mac tienes? Menú Apple → Acerca de este Mac: «Chip» es Apple silicon, «Procesador» es Intel."
- en: "On a Windows ARM64 PC, install the ARM64 build. A copy of the x64 build installed earlier keeps working and updating as x64." / es: "En un PC con Windows ARM64, instala la versión ARM64. Una copia de la versión x64 instalada antes sigue funcionando y actualizándose como x64."
- The quarantine line for the CLI becomes `xattr -d com.apple.quarantine coverly_<version>_mac_arm` (or `_mac_intel`).

Landing labels (es / en): «Descargar para Apple silicon (.dmg, N MiB)» / "Download for Apple silicon (.dmg, N MiB)"; «O para Mac con Intel (.dmg, N MiB)» / "Or for an Intel Mac (.dmg, N MiB)"; «O el instalador para ARM64 (N MiB)» / "Or the ARM64 installer (N MiB)"; requirement line «Windows 10 y 11 (x64), Windows 11 (ARM64)» / "Windows 10 and 11 (x64), Windows 11 (ARM64)" (D6: Windows 10 on ARM is not supported); CLI list adds «Windows ARM64» / "Windows ARM64". The pre-first-release "download for your system" button (`offer()`) keeps its wording and names the system as today («macOS», «Windows», «Linux x64»/«Linux ARM64»), only the file behind it changes. The macOS primary button says «Descargar para Apple silicon» only in the new layout; the legacy layout keeps «Descargar .dmg» under «Apple silicon e Intel».

## 8. Data flow

1. Tag push → `guard`, `draft` (unchanged).
2. Six `app` jobs build with Tauri's names; the staging step only flattens, sanitises spaces and versions the `.app.tar.gz`; each uploads `app-<os>-<arch>`.
3. Six `cli` jobs upload `cli-<os>-<arch>` with `coverly-<V>-<os>-<arch>[.exe]`.
4. `finalize` downloads each artifact into its own folder → `stage-builds` validates every artifact against its platform's Tauri patterns and copies all files, renamed, into `bundles/` and `cli/` (or nothing) → `updater-artifacts` lists the update files → signing (trusted comment `file:<release name>`, `version:<V>`) → `updater-manifest` + `add-cli` → `check-manifest` (12 keys, version-bound signatures) → `minisign` verification and the tampered copy → `SHA256SUMS` over release names → upload to the draft.
5. Publishing → `release-channels.yml` checks the same 12 keys before moving the beta manifest (copied as is: its urls point at the versioned release, so the beta channel needs no other change); `landing-redeploy.yml` rebuilds the landing.
6. An installed app asks `releases/latest/download/latest.json`, looks up `<os>-<arch>-<bundle>` then `<os>-<arch>`, downloads the url (whatever its name), verifies the signature and version, installs. The CLI does the same with `cli-<os>-<arch>`.
7. The landing: at deploy, `apply_release.py` resolves each slot (new, then legacy pattern) and keeps the matching layout blocks; in the browser, `release.js` upgrades a built page to a newer release only within one scheme (D9).

## 9. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| Installed 0.1.0/0.2.0 universal Mac | Gets the single-architecture archive for its key (D8). Tested by the compatibility test. |
| Installed 0.2.0 x64 on a Windows ARM PC | Keeps x64 updates (D7); notes explain installing ARM64. |
| Prerelease tag (`v0.3.0-beta.1`) | No `.msi` on either Windows artifact; `stage-builds` refuses an `.msi` there, as today; manifest has no `windows-*-msi` keys. |
| A build hands over a file of another platform (e.g. `_x64.dmg` in `app-darwin-aarch64`) | `stage-builds` fails: patterns are per artifact. |
| Windows ARM runner lacks rustup, `clang` or .NET Framework 3.5, or WiX fails on ARM64 | rustup is installed by the checked step (§5.2); for the others the job fails before upload, so no draft gets partial files, and the error names the tool. Fallback if WiX cannot build ARM64 MSIs: drop the ARM64 `.msi` kind from `build_artifacts` and pass `--bundles nsis` on that runner (one-line change each), and the `windows-aarch64-msi` key disappears; NSIS remains. |
| Release name containing a character GitHub rewrites | Impossible by construction (`[A-Za-z0-9._-]` only); `stage-builds` asserts it with `check_file_name` plus a charset check. |
| A stray old-style name in `bundles/` (someone runs `updater-manifest` by hand on old files) | Fails with "not a release name"; recovery of old manifests uses `list-artifacts`/`resign-manifest`, which are name-agnostic. |
| Landing built against v0.2.0, newer release published with new names | `release.js` finds no `…_universal.dmg` in the newer release and leaves the page as built (still linking working v0.2.0 files); `landing-redeploy.yml` rebuilds it with the new layout within minutes (D9). |
| Next app release published before the landing PR is merged | The landing's Vercel build fails on the missing `macos_universal` file and Vercel keeps serving the v0.2.0 build; avoided by D11. Recovery: merge the landing PR, then run `landing-redeploy.yml` by hand. |
| Next release lacks the Windows ARM64 files (the WiX fallback above taken for the `.msi` only leaves NSIS; this row is the whole ARM64 job removed) | The landing drops the optional ARM64 blocks; `check-manifest` would fail first, though, so this needs the `--require windows-aarch64` lines removed in the same change. |
| Someone downloads the Intel DMG on Apple silicon | It runs under Rosetta and keeps updating as Intel (its updater asks `darwin-x86_64-*`); the notes' "which Mac" sentence is the prevention. The reverse (Apple silicon DMG on Intel) does not open; the Intel link sits next to it everywhere. |
| ARM64 installer run on an x64 Windows PC | Expected to refuse (Tauri's NSIS template checks the OS architecture); part of the owner check, not relied on. |
| Two `.msi` locales one day (a second WiX language) | Both would map to `_windows_x64.msi`; the existing "more than one .msi file" refusal fails first. Adding a language means adding the locale back to the release name. |
| Visitor on Safari on an Intel Mac | No architecture hint: the primary offer is Apple silicon; the Intel link is always visible in the macOS card, and the notes explain how to tell. |
| Windows ARM visitor | Browsers report x64 on Windows ARM, so nothing tries to detect it: the card's primary is x64 (runs under emulation) and the ARM64 link sits in the same card. |

## 10. Test plan

### Python via Rust (`src-tauri/tests/release_pipeline.rs`, `cargo test --workspace`)

- `stage_builds_renames_every_platform`: fake `downloads/` tree with the 12 artifacts and Tauri/Cargo names (stable), run `stage-builds`, compare the sorted `bundles/` and `cli/` listings with §4 exactly; contents copied byte for byte.
- `stage_builds_prerelease_has_no_msi`: same for `0.3.0-beta.1`; an `.msi` present → failure, nothing copied.
- Refusals kept and extended: a `_x64.dmg` in `app-darwin-aarch64`, a missing `app-windows-aarch64`, a leftover `app-macos-universal`, a `.sig`, a symlink, a non-empty destination.
- `updater_manifest_maps_release_names_to_tauri_keys`: from staged names + fake `.sig` files, the manifest has exactly the §4 keys and urls (percent-encoding unchanged), and `check-manifest` with the 12 required keys passes.
- `installed_versions_find_their_update` (compatibility proof): for every key a 0.1.0/0.2.0 app or CLI asks — `darwin-aarch64[-app]`, `darwin-x86_64[-app]`, `windows-x86_64[-nsis|-msi]`, `linux-x86_64[-appimage|-deb]`, `linux-aarch64[-appimage|-deb]`, `cli-darwin-aarch64`, `cli-darwin-x86_64`, `cli-windows-x86_64`, `cli-linux-x86_64`, `cli-linux-aarch64` — the new manifest has the key, and its file is of the same bundle kind and the same processor (e.g. `windows-x86_64-msi` → `_windows_x64.msi`, `darwin-x86_64-app` → `_mac_intel.app.tar.gz`). The manifest also parses with `coverly_core::update::parse_manifest`, and `cli_platform_key_for` for each (os, arch) pair, `windows`/`aarch64` included, resolves to an entry. The keys are the ones `tauri-plugin-updater` builds (`{os}-{arch}-{installer}`, then `{os}-{arch}`, arch from the running binary) and `cli_platform_key()`; the test lists them as constants with a comment naming that source, since the plugin itself cannot be driven offline.
- `add_cli_reads_release_names`: six binaries; `coverly_<V>_linux_x64.exe`, `coverly_<V>_windows_x64` (no `.exe`), an old `coverly-<V>-linux-x86_64` and a wrong version refused.
- `updater_manifest_refuses_names_that_are_not_release_names`: an old Tauri name, a suffix of another OS (`…_mac_arm-setup.exe`) and a name with a space each fail, and nothing is written.
- `recovery_tools_still_read_a_v0_2_0_manifest`: `list-artifacts` and `resign-manifest` on a manifest with the old names (the v0.2.0 shape) work unchanged.
- Workflow assertions: the `app` matrix has six entries including `windows-11-arm` and the two macOS targets, no `universal-apple-darwin`; the `cli` matrix has six; `finalize` and `release-channels.yml` require 12 keys; the `app` staging step script, run on a fake tree, produces versioned `.app.tar.gz` names for both macOS architectures.
- Release notes test: both tables name every new file pattern and none of the old ones.

### Static

`actionlint` on `release.yml` and `release-channels.yml`; `python3 -I scripts/release/release_tools.py --help` and each subcommand's `--help`.

### Local staging dry run

A throwaway script under the scratchpad builds a fake `downloads/` tree (both stable and beta), runs `stage-builds`, signs nothing (fake `.sig` text), runs `updater-manifest`, `add-cli`, `check-manifest` without `--signed-version`, then `sha256sum`, and prints the listing; output compared with §4 by hand and kept as PR evidence. Fixture paths are compared by component, never by slash text, and fixture mtimes are pinned (the same rule as the Rust tests, which also run on the Windows CI leg). It writes only under the scratchpad or `target/` of the worktree and deletes its tree afterwards.

### Repository gates

`pnpm check-all`, `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`. No Vitest changes are expected (no frontend code changes); the suite must stay green.

### Landing

`make check` (`check_site.py`, `release.test.mjs`, links, parity, budgets, contrast, motion): the three fixtures resolve every required slot; the legacy fixture renders today's macOS and Windows cards (universal button, no Intel or ARM64 links, no leftover `asset`/`noasset` comment); the new fixture renders the split macOS card and the ARM64 links; the new-without-ARM fixture drops only the ARM64 blocks; a release holding both schemes fails as ambiguous; `release.js` upgrades a new-name page to a newer new-name release, leaves an old-name page alone against a new-name release, and leaves any page alone when one file is missing; `offer()` picks the new name and falls back to the legacy one for each `data-os`. Browser E2E with Chrome DevTools MCP (`make site SITE_PORT=1461 RELEASE=<fixture>`, emulated user agents for macOS, Windows and Linux ARM, and once with reduced motion): the right primary button and alternatives are shown in both languages and links point at the fixture's names. PR in English, green CI, merge before the next app release (D11).

### Owner (after the next real release, kept in TODO.md)

Update a real 0.2.0 install to the new version on macOS (both chips if available; the Intel build is cross-compiled and never launched in CI), Windows x64 and Linux; install the ARM64 NSIS and MSI on a Windows 11 ARM machine and check that WebView2 starts, that the ARM64 installer replaces an x64 copy installed earlier (D7) and that the CLI `coverly_<V>_windows_arm.exe` runs and `coverly update check` finds `cli-windows-aarch64`.

## 11. Self-review

- No placeholders: every file name, key and matrix entry is spelled out (§4, §5).
- D2 (no universal) is consistent with §4 (no `universal` row), §5.2 (two macOS entries) and the workflow test ("no `universal-apple-darwin`").
- D4 (keys unchanged) is consistent with §4's key table and the compatibility test; the only new keys are the Windows ARM64 ones.
- D3 (rename only in `stage-builds`) holds: the `app` staging step still only versions the `.app.tar.gz` with Tauri's own architecture token, as it already did with `universal`.
- The `.msi` locale drop (D5) is reflected in the notes table and the compatibility test (`windows-x86_64-msi` → `_windows_x64.msi`).
- The landing keeps working for v0.2.0 (legacy patterns, optional ARM64 and Intel slots, `noasset` blocks keeping today's wording) and for the next release (new patterns first), provided D11's order is kept.

## 12. Adversarial review (2026-10-26)

Checked against TODO.md §0 and the code on this branch (`release.yml`, `release-channels.yml`, `landing-redeploy.yml`, `release_tools.py`, `release_pipeline.rs`, `coverly_core::update`, `pnpm-lock.yaml`) and the landing's `main` (`apply_release.py`, `release.js`, `site/index.html`). Changes made:

- **Landing, wrong assumption:** the first draft had `release.js` translate old names to new ones. That would point the built "Apple silicon e Intel" button at an Apple-silicon-only DMG and could not recreate the Intel and ARM64 links the build left out. Now the browser never crosses schemes (D9) and the redeploy does it; `offer()` was also described as a general picker, but it only runs on pages built before the first release, so the `userAgentData` high-entropy call was dropped (async, privacy-sensitive, Chromium-only).
- **Landing, legacy wording:** an old-name release now renders today's cards through `noasset` blocks, instead of an "Apple silicon" label on a universal file. The block mechanism is limited to `.html` (JSON-LD and `llms.txt` cannot hold it).
- **Release order (D11):** the landing build fails on a missing required file, so the landing PR must merge before the next app release.
- **Windows ARM runner:** rustup, `clang` (for `ring`, in the app **and** the CLI job) and .NET Framework 3.5 (WiX 3) are now checked; the lockfile was confirmed to carry the `win32-arm64-msvc` packages.
- **Supported Windows ARM:** Windows 11 only, the same wording in the notes and on the landing (the draft said "Windows 10 y 11, x64 o ARM64" on the landing and "Windows 11 (ARM64)" in the notes).
- **Parser:** release names are parsed with a full match bound to the kind's OS instead of a token search anywhere in the name.
- **Tests:** the existing assertions that flip are listed (§5.3); new tests for refused names and for the recovery tools on a v0.2.0 manifest; the dry run pins fixture mtimes.
- **Copy:** Spanish placeholder `<versión>`; the ARM64 x64-to-ARM replacement is stated as an owner check, not a promise.
- **Docs:** `updates.md`'s "five keys" becomes six.

Open owner questions (no default taken, nothing blocks this branch): whether to keep publishing a universal DMG as an extra download for people who move a disk between Macs (default: no, D2); and, after the owner check, whether the notes may say the ARM64 installer replaces an x64 copy.
