# M0 + M1 — Foundation and Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A buildable, CI-checked Coverly ROMs repository (legal files, Rust workspace, Tauri + React shell with en/es i18n) whose `coverly-core` scans any ROM library and reports which ROMs lack cover/screenshot/title art, exposed through `coverly audit`.

**Architecture:** Cargo workspace at the repo root with three members: `crates/coverly-core` (pure library, no Tauri), `crates/coverly-cli` (binary `coverly`, clap) and `src-tauri` (Tauri 2 app, thin IPC over the core). React 19 + TypeScript + Tailwind 3 + zustand frontend in `src/`, mirroring `../pluryprint`.

**Tech Stack:** Rust 1.90+ (edition 2021), Tauri 2.12, serde/toml/serde_json, walkdir, thiserror, clap 4, assert_cmd + tempfile + insta (tests); React 19, Vite 8, Vitest 5, Tailwind 3.4, zustand 5, ESLint 10, Prettier 3, commitlint, husky.

**Spec:** `docs/superpowers/specs/2026-10-15-coverly-roms-design.md` (read §2, §5, §6, §9, §10, §12, §16 before starting).
**Reference repo (template):** `/Users/miguelangelesparzacalero/Documents/GitHub/pluryprint` — copy tooling/config files from it and rename; never copy its product code.

## Global Constraints

- Licence `AGPL-3.0-only` in every `Cargo.toml` and `package.json`; copyright holder "Miguel Angel Esparza Calero (MAECLY)".
- Product name **Coverly ROMs**; npm package `coverly-roms`; crates `coverly-core`, `coverly-cli` (binary `coverly`), app crate `coverly-roms`; bundle identifier `com.maecly.coverlyroms`.
- `rust-version = "1.90"`, `edition = "2021"`; `cargo clippy --workspace --all-targets -- -D warnings` must pass; `cargo fmt --all -- --check` must pass.
- The core never depends on Tauri. No network code in M0/M1.
- Never write to a scanned library in M1 (audit is read-only).
- Ignore macOS AppleDouble files (`._*`), `.DS_Store`, `Thumbs.db`, and hidden entries everywhere.
- Extension and folder-name comparisons are **case-insensitive**.
- Commits: conventional, granular, **no Co-Authored-By trailer**. Docs in English.
- UI strings only through i18n (`locales/en/ui.json`, `locales/es/ui.json`).

## Review Focus

1. **Case and dot-files on SD cards**: `IMGS`/`imgs` folders, `.PNG` art, `GAME.ZIP` ROMs, and `._Game.zip` AppleDouble twins must be classified correctly (AppleDouble never counted). Tests in Tasks 7 and 8.
2. **Multi-file games**: a `.cue` with its `.bin` tracks, a `.m3u` with its discs, and a `.gdi` with tracks must be ONE entry; tracks referenced by a cue/m3u are companions, not extra ROMs. Test in Task 7.
3. **Unknown system folders** (e.g. `APPS`, `PORTS/Shortcuts`, `bios`) must be reported as "unknown/skipped", not crash or be counted as ROMs. Test in Task 7.
4. **Art present with a different extension** (`.jpg` when profile expects `.png`) counts as present; art for a ROM inside a subfolder follows the profile's `mirror_subdirs` flag. Test in Task 8.
5. **Huge libraries / unreadable entries**: permission-denied dirs and broken symlinks are reported as scan warnings and never abort the scan. Test in Task 7.

---

### Task 1: Legal, community and repository hygiene files

**Files:**
- Create: `LICENSE` (copy verbatim from `../pluryprint/LICENSE`), `LICENSES/` (copy `../pluryprint/LICENSES/*` only as needed later; for now create `LICENSES/README.md` stating third-party licence texts live here)
- Create: `TRADEMARKS.md`, `CLA.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `THIRD-PARTY-NOTICES.md`, `README.md`, `README.es.md`, `.gitignore`, `.gitattributes`, `.editorconfig`, `docs/09-legal/trademark.md`, `docs/09-legal/license.md`

- [ ] **Step 1:** Read `../pluryprint/TRADEMARKS.md`, `CLA.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `.gitignore`, `.gitattributes`, `.editorconfig`.
- [ ] **Step 2:** Write `TRADEMARKS.md` adapted: **Coverly ROMs**, the wordmark `Coverly` + `ROMs`, and the Coverly mark belong to MAECLY (Miguel Angel Esparza Calero). Keep the AGPL §7(e) paragraph and "Forks must change the product name…" verbatim with the name replaced. The mark is not designed yet: say "The mark is being designed; until it ships the app uses the wordmark only." Include a "What a fork has to replace" list with the identifiers from Global Constraints (`src-tauri/tauri.conf.json` productName/identifier/updater pubkey, `crates/coverly-core/src/paths.rs` app-dir triple `com`/`Maecly`/`CoverlyROMs`, `package.json` name, `locales/*/ui.json` `app.name`, `src-tauri/icons/`).
- [ ] **Step 3:** Write `CLA.md` from Pluryprint's with the product name replaced, version 1.0, date 2026-10-15.
- [ ] **Step 4:** Write CONTRIBUTING (conventional commits, CLA, `make check`), CODE_OF_CONDUCT (copy), SECURITY (report via GitHub private advisory on `MAECLY/coverly-roms`), THIRD-PARTY-NOTICES (header only; "Art sources are fetched at the user's request and are not distributed with Coverly ROMs: libretro-thumbnails, …").
- [ ] **Step 5:** Write `README.md` (English) and `README.es.md` (Spanish): one-paragraph pitch from spec §1, feature list, status "pre-alpha", build instructions (`make install`, `make dev`, `cargo run -p coverly-cli -- audit <path>`), licence + trademark notice.
- [ ] **Step 6:** `.gitignore` must include `target/`, `node_modules/`, `dist/`, `coverage/`, `.DS_Store`, `src-tauri/gen/`, `*.log`.
- [ ] **Step 7:** Commit: `git add -A && git commit -m "chore: add licence, trademark policy, CLA and community files"`

### Task 2: Rust workspace with `coverly-core` and `coverly-cli` skeletons

**Files:**
- Create: `Cargo.toml` (workspace), `crates/coverly-core/{Cargo.toml,src/lib.rs,src/paths.rs}`, `crates/coverly-cli/{Cargo.toml,src/main.rs,tests/cli_version.rs}`, `rustfmt.toml`

**Interfaces:**
- Produces: `coverly_core::VERSION: &str` (= `env!("CARGO_PKG_VERSION")`); `coverly_core::paths::{ORG, QUALIFIER, APP}` = `("Maecly", "com", "CoverlyROMs")`; binary `coverly` supporting `--version` and `--help`.

- [ ] **Step 1:** Workspace `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/coverly-core", "crates/coverly-cli", "src-tauri"]

[workspace.package]
version = "0.0.1"
edition = "2021"
rust-version = "1.90"
license = "AGPL-3.0-only"
repository = "https://github.com/MAECLY/coverly-roms"
authors = ["Maecly maintainers"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "1"
thiserror = "2"
anyhow = "1"
walkdir = "2"
clap = { version = "4", features = ["derive"] }
tempfile = "3"
assert_cmd = "2"
predicates = "3"
insta = { version = "1", features = ["json"] }
```

(`src-tauri` is added as a member in Task 3; until then omit it from `members`.)
- [ ] **Step 2:** Write failing test `crates/coverly-cli/tests/cli_version.rs`:

```rust
use assert_cmd::Command;
#[test]
fn prints_version() {
    Command::cargo_bin("coverly").unwrap().arg("--version")
        .assert().success().stdout(predicates::str::contains("coverly 0.0.1"));
}
```
- [ ] **Step 3:** Run `cargo test -p coverly-cli` → FAIL (crate missing).
- [ ] **Step 4:** Implement `coverly-cli/src/main.rs` with clap `#[command(name = "coverly", version, about = "Audit and fill ROM cover art")]` and an empty `Commands` enum placeholder-free (only `--version/--help` for now: use `Cli { }` with no subcommand required). `coverly-core/src/lib.rs` exposes `pub const VERSION` and `pub mod paths;`.
- [ ] **Step 5:** `cargo test --workspace` PASS; `cargo clippy --workspace --all-targets -- -D warnings` PASS; `cargo fmt --all -- --check` PASS.
- [ ] **Step 6:** Commit `feat(core): scaffold Rust workspace with core library and coverly CLI`.

### Task 3: Tauri 2 app + React shell with en/es i18n and tooling

**Files:**
- Create: `package.json`, `pnpm-workspace.yaml`, `index.html`, `vite.config.ts`, `vitest` setup `src/test/setup.ts`, `tsconfig.json`, `eslint.config.js`, `.prettierrc.json`, `.prettierignore`, `commitlint.config.cjs`, `.lintstagedrc.json`, `.husky/{pre-commit,commit-msg}`, `tailwind.config.ts`, `postcss.config.js`, `src/{main.tsx,App.tsx,App.test.tsx,styles/globals.css}`, `src/i18n/{I18nProvider.tsx,useT.ts,i18n.test.tsx}`, `locales/en/ui.json`, `locales/es/ui.json`, `src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,src/main.rs,src/lib.rs,icons/*}`, `Makefile`
- Modify: root `Cargo.toml` (add `src-tauri` member)

**Interfaces:**
- Consumes: `coverly_core::VERSION`.
- Produces: Tauri command `app_info() -> AppInfo { name: String, version: String, core_version: String }`; React `useT()` hook returning `t(key: string, vars?: Record<string,string|number>) => string`; locale chosen from `navigator.language` (`es*` → es, else en).

- [ ] **Step 1:** Copy and rename tooling from `../pluryprint` (package.json scripts/devDependencies — drop `@wdio/*`, `@types/mocha`, `sonner` for now; keep versions identical), `tsconfig.json`, `eslint.config.js` (remove pluryprint-specific ignores), `.prettierrc.json`, `commitlint.config.cjs`, `.lintstagedrc.json` (rust glob → `crates/**/*.rs` and `src-tauri/**/*.rs` with `cargo fmt --all --`), `.husky/*`, `tailwind.config.ts`, `postcss.config.js`, `vite.config.ts` (remove e2e stub plugin; port 1431/1432).
- [ ] **Step 2:** Write failing `src/i18n/i18n.test.tsx`: renders `<I18nProvider locale="es"><Probe/></I18nProvider>` where Probe shows `t('app.tagline')`; expects Spanish text "Encuentra y añade las portadas que faltan en tus ROMs"; English "Find and add the missing cover art in your ROMs". Also tests interpolation `t('library.count', {n: 3})` → "3 ROMs".
- [ ] **Step 3:** Implement provider + `useT` with JSON imports of `locales/{en,es}/ui.json`, fallback to `en`, then to the key itself.
- [ ] **Step 4:** `App.tsx`: a centered start screen with wordmark "Coverly ROMs", tagline, a disabled "Open library…" button (wired in M3), and footer showing versions from `app_info` (via `@tauri-apps/api/core` `invoke`, guarded so tests and plain browser render `—`). `App.test.tsx` asserts wordmark + tagline render.
- [ ] **Step 5:** `src-tauri`: Cargo package `coverly-roms` (lib `coverly_roms_lib`), deps `tauri = "2.12"`, `coverly-core = { path = "../crates/coverly-core" }`, serde. `tauri.conf.json`: productName "Coverly ROMs", identifier `com.maecly.coverlyroms`, window 1200x800 min 900x600 title "Coverly ROMs", devUrl `http://localhost:1431`, frontendDist `../dist`, beforeDevCommand `pnpm dev`, beforeBuildCommand `pnpm build`. Icons: generate a placeholder set with `pnpm tauri icon` from a simple 1024px PNG wordmark tile (`src-tauri/icons/source.png`, created with a small Node/Python script committed under `scripts/icons/`).
- [ ] **Step 6:** Rust unit test in `src-tauri/src/lib.rs` for `app_info()` returning name "Coverly ROMs" and `core_version == coverly_core::VERSION`.
- [ ] **Step 7:** Run `pnpm install`, `pnpm lint`, `pnpm typecheck`, `pnpm format:check`, `pnpm test`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm tauri build --debug --no-bundle` (must compile). All PASS.
- [ ] **Step 8:** `Makefile` with targets `help install doctor dev test test-rust test-web lint fmt check build audit` (copy Pluryprint's style; `audit` runs `cargo run -p coverly-cli -- audit $(ROOT)`).
- [ ] **Step 9:** Commits (granular): `build: add frontend tooling (vite, eslint, prettier, commitlint, husky)`, `feat(app): add Tauri shell with start screen and app_info command`, `feat(i18n): add English and Spanish locales`, `build: add Makefile developer tasks`.

### Task 4: CI on Linux, Windows and macOS

**Files:** Create `.github/workflows/ci.yml`, `docs/07-build-and-release/ci-cd.md`

- [ ] **Step 1:** Workflow modelled on `../pluryprint/.github/workflows/ci.yml` (pinned action SHAs copied from it): job `frontend` (ubuntu: pnpm install --frozen-lockfile, lint, typecheck, format:check, test); job `backend` matrix `[ubuntu-latest, windows-latest, macos-latest]`: on ubuntu install `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf build-essential libssl-dev libxdo-dev`; `cargo fmt --all -- --check` (ubuntu only), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. `src-tauri` needs `dist/` to exist for `tauri::generate_context!` → step `mkdir -p dist` before cargo (or build frontend first).
- [ ] **Step 2:** Push branch, open PR, confirm all jobs green (`gh pr checks --watch`). Fix until green.
- [ ] **Step 3:** Commit `ci: check frontend and Rust workspace on Linux, Windows and macOS`.

### Task 5: `systems` — system database with folder aliases

**Files:** Create `crates/coverly-core/src/systems/{mod.rs,systems.toml}`, tests inline (`#[cfg(test)]`).

**Interfaces:**
- Produces:

```rust
pub struct System {
    pub id: String,                 // "snes"
    pub name: String,               // "Super Nintendo"
    pub aliases: Vec<String>,       // folder names, compared case-insensitively
    pub extensions: Vec<String>,    // lowercase, no dot: ["sfc","smc","zip","7z"]
    pub libretro: Option<String>,   // "Nintendo - Super Nintendo Entertainment System"
    pub arcade: bool,
    pub multi_file: bool,           // disc systems: cue/bin, m3u, gdi, chd
}
pub struct SystemDb { /* … */ }
impl SystemDb {
    pub fn builtin() -> SystemDb;                                   // parses embedded systems.toml
    pub fn with_overrides(self, toml_text: &str) -> Result<SystemDb, SystemsError>; // user file merges/overrides by id
    pub fn resolve_folder(&self, folder_name: &str) -> Option<&System>;            // case-insensitive alias match
    pub fn get(&self, id: &str) -> Option<&System>;
    pub fn all(&self) -> &[System];
}
```
- [ ] **Step 1:** Write tests: every folder name present on the real SD resolves — `A2600 A5200 A7800 A800 AMIGA ATARIST ATOMISWAVE C64 CPS1 CPS2 CPS3 DOS DREAMCAST EASYRPG FBNEO FC FDS GB GBA GBC GG GW HBMAME LYNX MAME MD MDCD MSX N64 NAOMI NDS NEOCD NEOGEO NGP ONS OPENBOR PCE PCECD PGM2 PICO POKE PS PSP SATURN SCUMMVM SEGA32X SFC SMS VARCADE VB VIC20 WS` — plus common aliases from other firmwares: `nes snes genesis megadrive psx ps1 gba mastersystem gamegear pcengine tg16 segacd neogeo arcade fbneo n64 nds`. `resolve_folder("sfc")`, `("SFC")`, `("Sfc")` all → id `snes`. `resolve_folder("APPS")`, `("PORTS")`, `("JAVA")`, `("bios")` → `None`. Overrides: a user TOML adding alias `SUPERFAMICOM` to `snes` resolves; a new id is appended.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Write `systems.toml` (one `[[system]]` per platform; include libretro playlist names exactly as on `thumbnails.libretro.com` — see `docs/02-research/libretro-thumbnails.md`; e.g. `FC`→`Nintendo - Nintendo Entertainment System`, `MD`→`Sega - Mega Drive - Genesis`, `PS`→`Sony - PlayStation`, `NDS`→`Nintendo - Nintendo DS`, `A2600`→`Atari - 2600`, `MAME`/`FBNEO`/`CPS1-3`/`NEOGEO`/`HBMAME`/`VARCADE`→ arcade with libretro `MAME` or `FBNeo - Arcade Games`). Implement loader with `include_str!` + `toml`.
- [ ] **Step 4:** PASS + clippy. **Step 5:** Commit `feat(core): add system database with case-insensitive folder aliases`.

### Task 6: `profiles` — layout profiles, path templates and auto-detection

**Files:** Create `crates/coverly-core/src/profiles/{mod.rs,builtin.rs,template.rs,detect.rs}`

**Interfaces:**

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Media { Cover, Screenshot, Title }

pub struct MediaSlot {
    pub template: String,     // e.g. "{system_dir}/Imgs/{stem}.png"
    pub format: ImageFormat,  // Png | Jpg
    pub max_size: Option<(u32, u32)>,
}
pub struct Profile {
    pub id: String,                       // "anbernic-stock"
    pub name: String,
    pub roms_root: String,                // relative to library root where system dirs live: "" or "ROMS" or "Roms"
    pub art_dir_names: Vec<String>,       // folder names never scanned as ROMs: ["Imgs"]
    pub mirror_subdirs: bool,
    pub slots: HashMap<Media, MediaSlot>, // absent media = profile cannot store it
}
pub fn builtin_profiles() -> Vec<Profile>; // ids: anbernic-stock, garlicos, onion, muos, minui, esde, batocera, retroarch
pub struct TemplateVars<'a> { pub root: &'a Path, pub system_dir: &'a Path, pub rel_dir: &'a Path, pub stem: &'a str, pub system_id: &'a str, pub libretro: Option<&'a str> }
pub fn render(template: &str, vars: &TemplateVars) -> PathBuf; // tokens {root} {system_dir} {rel_dir} {stem} {system_id} {libretro}
pub struct Detection { pub profile_id: String, pub score: u32, pub evidence: Vec<String> }
pub fn detect(library_root: &Path) -> Vec<Detection>; // sorted by score desc
```
- [ ] **Step 1:** Tests: `render("{system_dir}/Imgs/{stem}.png")` with system_dir `/sd/ROMS/SFC`, stem `Zelda (USA)` → `/sd/ROMS/SFC/Imgs/Zelda (USA).png`; with `mirror_subdirs` and rel_dir `Hacks` → `/sd/ROMS/SFC/Hacks/Imgs/…` for profiles whose template uses `{rel_dir}`. Detection on temp trees: tree with `anbernic/` + `ROMS/SFC/Imgs/` → top `anbernic-stock` with evidence `"anbernic/ folder"`, `"Imgs folders"`; `MUOS/info/catalogue` → `muos`; `.tmp_update` + `Roms/*/Imgs` → `onion`; `gamelist.xml` in a system dir + `images/` → `batocera`; `downloaded_media/` → `esde`; `thumbnails/*/Named_Boxarts` → `retroarch`; empty dir → empty Vec.
- [ ] **Step 2:** FAIL → **Step 3:** implement built-ins from spec §5 table + §16.8 (Anbernic/Garlic/Onion cover = `{system_dir}/{rel_dir}/Imgs/{stem}.png`, PNG, max 250×360; ES-DE uses `{root}/downloaded_media/{system_id}/covers|screenshots|titlescreens/{rel_dir}/{stem}.png`; Batocera `{system_dir}/images/{stem}-image.png` / `-screenshot` / `-titlescreen`, mirror_subdirs false; RetroArch `{root}/thumbnails/{libretro}/Named_Boxarts|Named_Snaps|Named_Titles/{stem}.png`; muOS `{root}/MUOS/info/catalogue/{system_id}/box|preview/{stem}.png`; MinUI `{system_dir}/.media/{stem}.png`). **Step 4:** PASS. **Step 5:** Commit `feat(core): add layout profiles, path templates and auto-detection`.

### Task 7: `scan` — library walker and ROM classification

**Files:** Create `crates/coverly-core/src/scan/{mod.rs,classify.rs,multifile.rs}`, `crates/coverly-core/tests/scan_fixtures.rs`, test helper `crates/coverly-core/tests/common/mod.rs` (builds fixture trees in a `tempfile::TempDir`).

**Interfaces:**

```rust
pub struct RomEntry {
    pub path: PathBuf,              // primary file (or .m3u/.cue/.gdi)
    pub rel_dir: PathBuf,           // relative to system dir ("" at top level)
    pub stem: String,
    pub ext: String,                // lowercase
    pub size: u64,                  // sum of primary + companions
    pub modified: SystemTime,
    pub companions: Vec<PathBuf>,
}
pub struct SystemScan { pub dir: PathBuf, pub folder_name: String, pub system_id: Option<String>, pub roms: Vec<RomEntry> }
pub struct ScanWarning { pub path: PathBuf, pub message: String }
pub struct LibraryScan { pub root: PathBuf, pub profile_id: String, pub systems: Vec<SystemScan>, pub warnings: Vec<ScanWarning> }
pub struct ScanOptions { pub include_hidden: bool, pub follow_symlinks: bool, pub ignored_dirs: Vec<String>, pub ignored_exts: Vec<String> }
impl Default for ScanOptions { /* include_hidden=false, follow_symlinks=false, ignored_dirs=["bios","saves","states","Shortcuts"], ignored_exts=["txt","xml","json","cfg","ini","db","png","jpg","jpeg","webp","bmp","srm","sav","state","pdf","md","nfo","log","dat"] */ }
pub fn scan_library(root: &Path, profile: &Profile, systems: &SystemDb, opts: &ScanOptions) -> LibraryScan;
```
Rules: system dirs = children of `root/profile.roms_root`; unknown folders produce `SystemScan{system_id: None, roms: vec![]}` (listed, not scanned); within a known system, accept files whose ext is in `system.extensions` (if the list is empty accept any non-ignored ext); skip art dirs and ignored dirs at any depth; recurse into other subdirs (rel_dir); AppleDouble `._*` and hidden always skipped; `.cue` → parse `FILE "x.bin"` lines → those become companions; `.m3u` → each line path → companion; `.gdi` → track filenames (3rd column) → companions; companions removed from the ROM list. Errors (permission denied, broken symlink) → `warnings`, never panic.
- [ ] **Step 1:** Fixture test (realistic): `ROMS/SFC/{Zelda (USA).sfc, ._Zelda (USA).sfc, MARIO.SMC, notes.txt, Imgs/Zelda (USA).png}`, `ROMS/PS/{FF7 (Disc 1).cue, FF7 (Disc 1).bin, FF7 (Disc 2).cue, FF7 (Disc 2).bin, FF7.m3u}` (m3u lists both cues), `ROMS/MD/Hacks/Sonic Hack.md`, `ROMS/APPS/thing.sh`, `ROMS/.hidden/x.sfc`. Expect: SFC 2 ROMs (`MARIO` with ext `smc`), PS 1 ROM (the m3u; 4 companions), MD 1 ROM with rel_dir `Hacks`, APPS listed with `system_id None`, no hidden dir, no `._` entry.
- [ ] **Step 2:** Permission test (unix only, `#[cfg(unix)]`): a chmod 000 subdir yields one warning and the scan still returns other ROMs.
- [ ] **Step 3:** FAIL → implement → PASS + clippy. **Step 4:** Commit `feat(core): scan libraries into systems and ROM entries with multi-file sets`.

### Task 8: `art` — art status per media and orphans

**Files:** Create `crates/coverly-core/src/art/mod.rs`, `crates/coverly-core/tests/art_status.rs`

**Interfaces:**

```rust
#[derive(Serialize)] #[serde(tag = "state", rename_all = "lowercase")]
pub enum ArtState { Present { path: PathBuf }, Missing, Unsupported }
pub struct RomArt { pub rom: RomEntry, pub art: BTreeMap<Media, ArtState> }
pub struct SystemArt { pub scan: SystemScan /* without roms */, pub roms: Vec<RomArt>, pub orphans: Vec<PathBuf> }
pub fn resolve_art(scan: &LibraryScan, profile: &Profile, systems: &SystemDb, media: &[Media]) -> Vec<SystemArt>;
```
Rules: expected path = `render(slot.template, vars)`; present if a file exists whose name equals the expected stem with any extension in `png jpg jpeg webp`, case-insensitively for both stem-dir and extension (build a per-directory lowercase index once — never `exists()` per ROM per extension); media missing from profile → `Unsupported`; orphans = image files in the art directories whose stem matches no ROM stem (excluding `._*`).
- [ ] **Step 1:** Tests: with the Task 7 fixture + `Imgs/zelda (usa).PNG` (case differs) → Zelda cover Present; MARIO Missing; screenshot Unsupported for anbernic-stock; `Imgs/Old Game.png` → orphan; `Imgs/._Zelda (USA).png` ignored. ES-DE fixture: `downloaded_media/snes/covers/Zelda (USA).jpg` → Present.
- [ ] **Step 2:** FAIL → implement → PASS. **Step 3:** Commit `feat(core): resolve cover, screenshot and title art status per ROM`.

### Task 9: `audit` — report model, JSON and CSV

**Files:** Create `crates/coverly-core/src/audit/{mod.rs,csv.rs}`, snapshot tests with `insta`.

**Interfaces:**

```rust
#[derive(Serialize)] pub struct AuditReport { pub root: PathBuf, pub profile_id: String, pub media: Vec<Media>, pub totals: Totals, pub systems: Vec<SystemReport>, pub warnings: Vec<ScanWarning> }
#[derive(Serialize)] pub struct Totals { pub systems_known: usize, pub systems_unknown: usize, pub roms: usize, pub missing: BTreeMap<Media, usize>, pub complete: usize, pub orphans: usize }
#[derive(Serialize)] pub struct SystemReport { pub folder: String, pub system_id: Option<String>, pub roms: usize, pub missing: BTreeMap<Media, usize>, pub entries: Vec<EntryReport> }
#[derive(Serialize)] pub struct EntryReport { pub rel_path: PathBuf, pub stem: String, pub size: u64, pub modified: String /* RFC3339 UTC */, pub art: BTreeMap<Media, ArtState> }
pub fn build_report(root: &Path, profile: &Profile, systems: &SystemDb, opts: &ScanOptions, media: &[Media]) -> AuditReport; // scan + resolve_art
pub fn write_csv<W: Write>(report: &AuditReport, w: W, missing_only: bool) -> io::Result<()>; // columns: system,folder,rel_path,stem,size,modified,<media...> (present|missing|unsupported)
```
- [ ] **Step 1:** Snapshot test of `build_report` JSON on the fixture (paths made relative to the temp root; `modified` fixed by setting mtimes with `filetime` dev-dependency). CSV snapshot with and without `missing_only`.
- [ ] **Step 2:** FAIL → implement → PASS. **Step 3:** Commit `feat(core): build audit reports with JSON and CSV output`.

### Task 10: `coverly audit` command

**Files:** Modify `crates/coverly-cli/src/main.rs`; create `crates/coverly-cli/src/{audit.rs,table.rs}`, `crates/coverly-cli/tests/cli_audit.rs`

**Interfaces:**
- `coverly audit <ROOT> [--profile <id>|auto] [--media cover,screenshot,title] [--format table|json|csv] [--missing-only] [--output <file>]`
- `coverly profiles` (list built-ins) and `coverly systems` (list systems + aliases).
- Exit codes: 0 ok; 2 bad usage (clap); 3 root not found / not a dir; 4 no profile detected and `--profile` not given (message lists candidates).
- Table format: per system `FOLDER  SYSTEM  ROMS  MISSING-COVER …`, then totals line, then up to 20 missing entries per system unless `--missing-only` (all) — plain text, no colours when not a TTY.
- [ ] **Step 1:** Tests: auto-detects anbernic-stock on the fixture and prints `SFC  snes  2  1`; `--format json` parses with serde_json and `totals.roms == 4`; `--format csv --missing-only` contains `MARIO` and not `Zelda`; missing root → exit 3; empty dir without `--profile` → exit 4.
- [ ] **Step 2:** FAIL → implement → PASS. **Step 3:** Commit `feat(cli): add audit, profiles and systems commands`.

### Task 11: Validate on the real SD (read-only) and document

**Files:** Modify `docs/TRACKER.md`; create `docs/README.md`, `docs/04-architecture/system-architecture.md`, `docs/adr/{README.md,0001-product-name.md,0002-stack-tauri-rust-react.md,0003-core-cli-app-workspace.md,0004-license-agpl.md,0005-libretro-primary-source.md,0006-hash-then-name-identification.md,0007-pluggable-strategies.md,0008-updater-public-releases.md}`

- [ ] **Step 1:** If `/Volumes/System/ROMS` is mounted: `cargo run -p coverly-cli --release -- audit /Volumes/System --format json --output <scratchpad>/sd-audit.json` and `--format table`. (Library root is `/Volumes/System`; profile auto → `anbernic-stock` with `roms_root = "ROMS"`.) Cross-check 3 systems manually with `ls` (count ROM files vs `Imgs/*.png`). Record totals + timings in the tracker as evidence. Never write to the SD.
- [ ] **Step 2:** Write ADRs (short: context, decision, consequences) from spec §2/§16, docs index like `../pluryprint/docs/README.md`, architecture doc from spec §6–§7.
- [ ] **Step 3:** Update TRACKER (M0-01..05, M1-01..04, M1-06, M1-07 ✅ with commit hashes; M1-05 SQLite cache moved to M2 — not needed for audit speed, record measured scan time as justification).
- [ ] **Step 4:** Commits `docs: add architecture overview and ADRs 0001-0008`, `docs(tracker): record M0-M1 completion and SD audit evidence`.
