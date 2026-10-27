# Release File Names and Windows ARM64 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. There is no app UI in this plan; the only visual change is the landing's download cards (Tasks 8–9), which follow the spec's §6 brief and are checked with screenshots in Task 11.

**Goal:** Every release file says in plain words which system and processor it is for (`Coverly-ROMs_<V>_mac_arm.dmg`, `coverly_<V>_windows_arm.exe`, …), macOS ships two single-architecture builds instead of one universal DMG, Windows ARM64 gets native app (NSIS + MSI) and CLI builds, installed 0.1.0/0.2.0 copies keep updating (proven by manifest tests), and the landing offers the right file with both the old (live v0.2.0) and the new names.

**Architecture:** One table in `scripts/release/release_tools.py` (`RELEASE_PLATFORMS`: suffix → `(os, arch)`) drives everything: `build_artifacts` (what each of the 12 workflow artifacts must hold, with Tauri's/Cargo's names), `release_name` (the name a file is copied to by `stage-builds`), the full-name parser behind `updater-manifest`, and `add-cli`. The build jobs in `.github/workflows/release.yml` keep handing over Tauri's names; only `stage-builds` renames. `latest.json` keeps Tauri's platform keys (only urls change; four new `windows-aarch64` keys). The landing (`MAECLY/coverly-roms-landing`) resolves each download slot with a list of patterns (new first, legacy second), adds optional slots and `asset`/`noasset` HTML blocks so a v0.2.0 build renders today's cards and a new build renders the split macOS card and the ARM64 links.

**Tech Stack:** Python 3.9+ standard library (`release_tools.py`, landing `apply_release.py`/`check_site.py`), Rust integration tests that drive the Python tool (`src-tauri/tests/release_pipeline.rs`, `tempfile`, `coverly_core::update::{testing::TestKey, parse_manifest, cli_platform_key_for}`), GitHub Actions YAML (`actionlint` 1.7 at `/opt/homebrew/bin/actionlint`), PowerShell 7 on the `windows-11-arm` runner, Node 24 `node --test` for the landing's `release.test.mjs`. No new crate, no new npm package, no new third-party action.

**Spec:** [`docs/superpowers/specs/2026-10-26-release-names.md`](../specs/2026-10-26-release-names.md) (all sections; §4 is the name table every test compares against). Roadmap brief: `TODO.md` §0. Related: ADR-0011, `docs/04-architecture/updates.md`, `RELEASING.md`, `docs/07-build-and-release/ci-cd.md`.

## Owner decisions taken by default (recorded in the spec, §3)

- **D1** `_mac_arm` (not `_mac_silicon`): shorter, parallel with `_linux_arm` / `_windows_arm`.
- **D2** Two macOS builds (`aarch64-apple-darwin`, `x86_64-apple-darwin`, both on `macos-latest`), no universal DMG.
- **D6** Windows ARM64 (`aarch64-pc-windows-msvc`, runner `windows-11-arm`) for the app (NSIS + MSI on stable tags) and the CLI, with `windows-aarch64`, `windows-aarch64-nsis`, `windows-aarch64-msi` and `cli-windows-aarch64` in `latest.json`.
- **D3/D4** Rename only in `stage-builds`; manifest keys unchanged, so installed copies keep updating.
- **D9/D11** The landing accepts both schemes at build time, never translates in the browser, and its PR merges before the next app release.

## Global Constraints

- **Worktree and branch.** App repository work happens only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names`, branch `chore/release-names-windows-arm` (already tracks `origin`). Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names &&` (or the landing path below), because agent shells reset their cwd.
- **Landing repository.** Tasks 8–10 work in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing` on a new branch `feat/release-names` created from an up-to-date `main` and pushed at once with `git push -u origin feat/release-names`. Same git rules as below.
- **Git rules (mandatory).** Conventional, granular commits in English with **no** `Co-Authored-By` line and no mention of Claude or AI. Run `git push` **immediately after every commit** (an earlier run lost a whole branch that was never pushed). Never force-push. To update with `main`: `git fetch origin && git merge origin/main`, resolve conflicts (other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel), run the gates, commit the merge, push. Every agent ends with `git status -sb` showing nothing ahead of `origin` in both repositories it touched.
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch (they are updated on `main` after the merge). Do not edit `src-tauri/tauri.conf.json`, `crates/coverly-core/**`, `locales/**` or any app UI file (spec §5.3: nothing there changes).
- **Never** push a git tag, create or publish a GitHub release, run `release.yml` or `release-channels.yml` for real (`gh workflow run` is forbidden for both), or upload an asset. Validation is `actionlint`, the Rust tests that drive `release_tools.py`, a local staging dry run with fake artifacts and the landing tests.
- **Secrets are never read or printed:** the minisign/Tauri key, `~/.coverly-release/`, any `.env*`, `.vercel/`. Tests sign with `coverly_core::update::testing::TestKey` only.
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, watch it pass, then run the gates for the task.
- **Gates (same as CI), before every commit that touches code or tests:** `pnpm check-all`, `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`. Docs-only commits (Tasks 6–7) run `pnpm check-all` (Prettier only covers `src/` and `locales/`, so align Markdown tables by hand) and `cargo test -p coverly-roms --test release_pipeline` (the release-notes test reads the docs). Workflow commits also run `actionlint .github/workflows/release.yml .github/workflows/release-channels.yml`.
- **Windows CI leg.** Fixture mtimes are pinned (helper `write_pinned`, Task 1) and paths are compared by component (file names from `read_dir`, `Path::join`, never slash text). Python builds destinations with `os.path.join`, never `"/".join`.
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`. This plan needs no real ROM data; any scratch tree lives under `target/` of the worktree or the scratchpad `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/release-names`, is created with `cp -RXp` when copying, holds no `._*` files and is deleted afterwards.
- **Servers.** The landing preview runs on port 1461 only (`make site SITE_PORT=1461 RELEASE=…`). Stop it only by port: `lsof -ti tcp:1461 -sTCP:LISTEN | xargs -r kill`.
- **Browser E2E.** Chrome DevTools MCP with `isolatedContext: 'release-names'`; fall back to Playwright MCP; if both profiles are locked, install `playwright-core` in the scratchpad folder above and drive Chrome for Testing.
- **Copy.** English and Spanish change in lockstep. Spanish: sentence case, no exclamation marks, exact counts, file names in code font. No `!` anywhere in `.github/release-notes.md` (a test checks it).

## Review Focus

1. **An installed copy stops updating.** A 0.1.0/0.2.0 app or CLI asks for a key that the new manifest lacks, or gets a file of another bundle kind or processor. Tests: Task 3 `installed_versions_find_their_update` (every key `tauri-plugin-updater` ~2.14 and `cli_platform_key()` build, with `parse_manifest` and signature checks), Task 2 `release_tools_map_the_update_artifacts_to_platform_keys` (exact key → file table, 20 keys + 6 CLI).
2. **A build smuggles or loses a file.** Another platform's file in an artifact, a missing `app-windows-aarch64`, a leftover `app-macos-universal`, a `.sig`, a symlink, two `.msi` locales, a name GitHub would rewrite, a partial copy. Tests: Task 1 `release_tools_stage_each_build_only_from_its_own_artifact` (extended tamper table) and `stage_builds_renames_every_platform`.
3. **The parser accepts something that is not a release name.** Old Tauri names, a suffix of another OS (`…_mac_arm-setup.exe`), a space, a CLI `.exe` off Windows. Tests: Task 2 `updater_manifest_refuses_names_that_are_not_release_names`, `add_cli_reads_release_names`.
4. **The workflow is wrong in a way only a real tag would show.** Six app and six CLI matrix entries, `tauri_arch` reaching the staging step, the Windows ARM toolchain step never seeing a secret, 12 required keys in `finalize` and `release-channels.yml`. Tests: Task 4 workflow assertions + the staging step run on a fake tree for both macOS architectures + `actionlint`; Task 11 dry run.
5. **The landing breaks on v0.2.0 or on the next release.** Legacy fixture must render today's cards exactly; new fixture the split macOS card and ARM64 links; new-without-ARM drops only the ARM64 blocks; both schemes in one release fails; `release.js` never crosses schemes. Tests: Tasks 8–9 (`check_site.py` variants, `release.test.mjs`), Task 11 E2E.

## File structure

App repository (`coverly-roms-release-names`):

| File | Change |
|---|---|
| `scripts/release/release_tools.py` | `RELEASE_PLATFORMS`, `TAURI_TOKENS`, `release_name`, new `build_artifacts`, renaming `stage_builds`, full-name `APP_RELEASE` parser in `updater_platforms`, new `CLI_FILE` in `add_cli`, docstring (Tasks 1–2) |
| `src-tauri/tests/release_pipeline.rs` | fixtures move to the new names, new tests (Tasks 1–5) |
| `.github/workflows/release.yml` | `app` matrix (6), `tauri_arch`, Windows ARM toolchain step in `app` and `cli`, `cli` matrix (6), `finalize` requires 12 keys, comments (Task 4) |
| `.github/workflows/release-channels.yml` | two more `--require` (Task 4) |
| `.github/release-notes.md` | en/es download tables and added sentences (Task 6) |
| `RELEASING.md`, `README.md`, `README.es.md`, `docs/07-build-and-release/ci-cd.md`, `docs/04-architecture/updates.md`, `docs/adr/0011-release-pipeline-and-update-key.md`, `CHANGELOG.md` | docs (Task 7) |

Landing repository (`coverly-roms-landing`, branch `feat/release-names`):

| File | Change |
|---|---|
| `scripts/site/apply_release.py` | pattern lists, optional slots, `asset`/`noasset` blocks, ambiguity across schemes (Task 8) |
| `scripts/site/fixtures/release-new.json`, `release-new-no-arm.json` | new fixtures (Task 8) |
| `scripts/site/check_site.py` | builds four variants, layout checks per variant, runs `release.test.mjs` once (Task 8) |
| `site/index.html`, `site/en/index.html`, `site/llms.txt` | slot rename `macos_universal` → `macos_arm`, new cards and CLI link (Task 9) |
| `site/assets/release.js` | `offer()` tries the new name, then the legacy one (Task 9) |
| `scripts/site/release.test.mjs` | fixture parameter, new-name and cross-scheme cases (Task 9) |
| `README.md` | both schemes and the merge order (Task 10) |

---

## Task 1: `stage-builds` copies every file under its release name (12 artifacts)

**Files:**
- Modify: `scripts/release/release_tools.py` (`build_artifacts`, `stage_builds`, new `RELEASE_PLATFORMS`, `TAURI_TOKENS`, `release_name`, `RELEASE_NAME`)
- Test: `src-tauri/tests/release_pipeline.rs` (`downloaded_builds`, `release_tools_stage_each_build_only_from_its_own_artifact`, new `stage_builds_renames_every_platform`, new helper `write_pinned`)

- [ ] **Step 0: Start from an up-to-date branch**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && git fetch origin && git merge origin/main && git push && git status -sb
```

Expected: `## chore/release-names-windows-arm...origin/chore/release-names-windows-arm` with nothing ahead. If the merge conflicts, resolve keeping both sides, run the gates, commit, push.

- [ ] **Step 1: Add the pinned-write helper and the new fixtures to the test file**

In `src-tauri/tests/release_pipeline.rs`, replace `downloaded_builds` (currently around line 1465) with the version below and add `write_pinned`, `PLATFORMS` and `release_files` next to it:

```rust
/// Fixture files get a fixed mtime so a run on the Windows CI leg and one on
/// macOS see the same tree.
fn write_pinned(path: &Path, body: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000))
        .unwrap();
}

/// (release suffix, os, arch): the six platforms of spec §4.
const PLATFORMS: [(&str, &str, &str); 6] = [
    ("mac_arm", "darwin", "aarch64"),
    ("mac_intel", "darwin", "x86_64"),
    ("windows_x64", "windows", "x86_64"),
    ("windows_arm", "windows", "aarch64"),
    ("linux_x64", "linux", "x86_64"),
    ("linux_arm", "linux", "aarch64"),
];

/// Lays out what `actions/download-artifact` writes (one folder per build
/// artifact, Tauri's and Cargo's names) for version `v`, with or without the
/// two Windows `.msi` files.
fn downloaded_builds(root: &Path, v: &str, msi: bool) {
    let mut files: Vec<(String, String)> = vec![
        ("app-darwin-aarch64".into(), format!("Coverly.ROMs_{v}_aarch64.app.tar.gz")),
        ("app-darwin-aarch64".into(), format!("Coverly.ROMs_{v}_aarch64.dmg")),
        ("app-darwin-x86_64".into(), format!("Coverly.ROMs_{v}_x64.app.tar.gz")),
        ("app-darwin-x86_64".into(), format!("Coverly.ROMs_{v}_x64.dmg")),
        ("app-windows-x86_64".into(), format!("Coverly.ROMs_{v}_x64-setup.exe")),
        ("app-windows-aarch64".into(), format!("Coverly.ROMs_{v}_arm64-setup.exe")),
        ("app-linux-x86_64".into(), format!("Coverly.ROMs_{v}_amd64.AppImage")),
        ("app-linux-x86_64".into(), format!("Coverly.ROMs_{v}_amd64.deb")),
        ("app-linux-aarch64".into(), format!("Coverly.ROMs_{v}_aarch64.AppImage")),
        ("app-linux-aarch64".into(), format!("Coverly.ROMs_{v}_arm64.deb")),
    ];
    if msi {
        files.push(("app-windows-x86_64".into(), format!("Coverly.ROMs_{v}_x64_en-US.msi")));
        files.push(("app-windows-aarch64".into(), format!("Coverly.ROMs_{v}_arm64_en-US.msi")));
    }
    for (_suffix, os, arch) in PLATFORMS {
        let ext = if os == "windows" { ".exe" } else { "" };
        files.push((format!("cli-{os}-{arch}"), format!("coverly-{v}-{os}-{arch}{ext}")));
    }
    for (artifact, name) in files {
        write_pinned(&root.join(&artifact).join(&name), name.as_bytes());
    }
}

/// Spec §4: the release names `stage-builds` must produce, sorted, as
/// (bundles, cli).
fn release_files(v: &str, msi: bool) -> (Vec<String>, Vec<String>) {
    let mut bundles = Vec::new();
    let mut cli = Vec::new();
    for (suffix, os, _arch) in PLATFORMS {
        let kinds: &[&str] = match os {
            "darwin" => &[".app.tar.gz", ".dmg"],
            "windows" if msi => &["-setup.exe", ".msi"],
            "windows" => &["-setup.exe"],
            _ => &[".AppImage", ".deb"],
        };
        for kind in kinds {
            bundles.push(format!("Coverly-ROMs_{v}_{suffix}{kind}"));
        }
        let ext = if os == "windows" { ".exe" } else { "" };
        cli.push(format!("coverly_{v}_{suffix}{ext}"));
    }
    bundles.sort();
    cli.sort();
    (bundles, cli)
}
```

- [ ] **Step 2: Write the failing rename test**

Add after `listing`:

```rust
#[test]
fn stage_builds_renames_every_platform() {
    if python().is_none() {
        return;
    }
    for (v, msi, n_bundles) in [("1.0.0", true, 12), ("1.0.0-beta.1", false, 10)] {
        let dir = tempfile::tempdir().unwrap();
        downloaded_builds(&dir.path().join("downloads"), v, msi);
        let out = stage_builds(dir.path(), v);
        assert!(out.status.success(), "{v}: {}", String::from_utf8_lossy(&out.stderr));
        let (bundles, cli) = release_files(v, msi);
        assert_eq!(bundles.len(), n_bundles, "{v}");
        assert_eq!(listing(&dir.path().join("bundles")), bundles, "{v}");
        assert_eq!(listing(&dir.path().join("cli")), cli, "{v}");
        // Content is copied byte for byte from the file it was renamed from.
        let staged = std::fs::read(
            dir.path().join("bundles").join(format!("Coverly-ROMs_{v}_windows_arm-setup.exe")),
        )
        .unwrap();
        assert_eq!(staged, format!("Coverly.ROMs_{v}_arm64-setup.exe").as_bytes());
        let staged = std::fs::read(dir.path().join("cli").join(format!("coverly_{v}_mac_intel"))).unwrap();
        assert_eq!(staged, format!("coverly-{v}-darwin-x86_64").as_bytes());
    }
}
```

- [ ] **Step 3: Update the existing staging test to the 12 artifacts**

In `release_tools_stage_each_build_only_from_its_own_artifact`:
- the stable listing asserts become `assert_eq!(listing(&bundles), release_files("1.0.0", true).0)` and `assert_eq!(listing(&cli), release_files("1.0.0", true).1)`; the beta one `release_files("1.0.0-beta.1", false).0` (10 files);
- in the tamper table, replace every old fixture name: `app-zzz/Coverly.ROMs_1.0.0_universal.dmg` stays (still an extra artifact); "another platform's file" writes `app-darwin-aarch64/Coverly.ROMs_1.0.0_x64.dmg` and expects `app-darwin-aarch64`; "a missing artifact" removes `app-windows-aarch64`; "a missing file" removes `app-linux-aarch64/Coverly.ROMs_1.0.0_arm64.deb` (unchanged); "a signature handed over" uses `app-windows-aarch64/Coverly.ROMs_1.0.0_arm64-setup.exe.sig`;
- add three rows (the array type becomes `[(&str, Tamper, &str); 10]`):

```rust
        (
            "the old universal artifact",
            |d| write_pinned(&d.join("app-macos-universal/Coverly.ROMs_1.0.0_universal.dmg"), b"old"),
            "app-macos-universal",
        ),
        (
            "two .msi locales",
            |d| write_pinned(&d.join("app-windows-x86_64/Coverly.ROMs_1.0.0_x64_es-ES.msi"), b"x"),
            "more than one .msi",
        ),
        (
            "a symlink",
            |d| {
                #[cfg(unix)]
                std::os::unix::fs::symlink(
                    "/etc/hosts",
                    d.join("app-linux-x86_64/Coverly.ROMs_1.0.0_amd64.x.AppImage"),
                )
                .unwrap();
                #[cfg(not(unix))]
                write_pinned(&d.join("app-linux-x86_64/Coverly.ROMs_1.0.0_amd64.x.AppImage"), b"x");
            },
            "app-linux-x86_64",
        ),
```

- the trailing "A stable release must ship its .msi" block stays; it now fails for both Windows artifacts.

- [ ] **Step 4: Run the tests and watch them fail**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && cargo test -p coverly-roms --test release_pipeline stage -- --nocapture 2>&1 | tail -30
```

Expected: `stage_builds_renames_every_platform` and `release_tools_stage_each_build_only_from_its_own_artifact` FAIL with `::error::unexpected build artifact app-darwin-aarch64` (the tool still expects `app-macos-universal`).

- [ ] **Step 5: Implement in `release_tools.py`**

Replace `build_artifacts` and `stage_builds` with the code below and add the tables above `UPDATER_KINDS` (they are used again in Task 2):

```python
# Release name suffix -> (os, arch) as the manifest keys spell them. The only
# place a platform is listed; every other table derives from it (spec §4).
RELEASE_PLATFORMS = {
    "mac_arm": ("darwin", "aarch64"),
    "mac_intel": ("darwin", "x86_64"),
    "windows_x64": ("windows", "x86_64"),
    "windows_arm": ("windows", "aarch64"),
    "linux_x64": ("linux", "x86_64"),
    "linux_arm": ("linux", "aarch64"),
}

# The architecture token Tauri writes into each bundle kind, per platform.
# The macOS .app.tar.gz carries the token the app job's staging step appends
# (matrix tauri_arch), the same one Tauri gives the .dmg.
TAURI_TOKENS = {
    ("darwin", "aarch64"): {".app.tar.gz": "aarch64", ".dmg": "aarch64"},
    ("darwin", "x86_64"): {".app.tar.gz": "x64", ".dmg": "x64"},
    ("windows", "x86_64"): {"-setup.exe": "x64", ".msi": "x64"},
    ("windows", "aarch64"): {"-setup.exe": "arm64", ".msi": "arm64"},
    ("linux", "x86_64"): {".AppImage": "amd64", ".deb": "amd64"},
    ("linux", "aarch64"): {".AppImage": "aarch64", ".deb": "arm64"},
}

APP_PREFIX = "Coverly-ROMs"
CLI_PREFIX = "coverly"
CLI_KIND = "CLI binary"
RELEASE_NAME = re.compile(r"[A-Za-z0-9._-]+")


def release_name(suffix: str, kind: str, version: str) -> str:
    """The name a staged file is published under (spec §4)."""
    v = version.lstrip("v")
    if kind == CLI_KIND:
        ext = ".exe" if RELEASE_PLATFORMS[suffix][0] == "windows" else ""
        return f"{CLI_PREFIX}_{v}_{suffix}{ext}"
    return f"{APP_PREFIX}_{v}_{suffix}{kind}"


def build_artifacts(version: str) -> dict[str, tuple[str, str, dict[str, str]]]:
    """artifact name -> (destination: "bundles" or "cli", release suffix,
    {kind: pattern} of the files it must hold under Tauri's or Cargo's name,
    each pattern matched by exactly one file)."""
    plain = version.lstrip("v")
    v = re.escape(plain)
    stable = "-" not in plain
    app = r"[A-Za-z0-9._-]+_" + v + "_"
    out: dict[str, tuple[str, str, dict[str, str]]] = {}
    for suffix, (os_name, arch) in RELEASE_PLATFORMS.items():
        patterns: dict[str, str] = {}
        for kind, token in TAURI_TOKENS[(os_name, arch)].items():
            if kind == ".msi":
                # WiX refuses a non-numeric prerelease: no .msi on a beta.
                if stable:
                    patterns[kind] = app + re.escape(token) + r"_[A-Za-z0-9-]+\.msi"
            else:
                patterns[kind] = app + re.escape(token) + re.escape(kind)
        out[f"app-{os_name}-{arch}"] = ("bundles", suffix, patterns)
        ext = ".exe" if os_name == "windows" else ""
        out[f"cli-{os_name}-{arch}"] = (
            "cli", suffix, {CLI_KIND: f"coverly-{v}-{os_name}-{arch}{re.escape(ext)}"})
    return out


def stage_builds(source: str, bundles: str, cli: str, version: str) -> list[str]:
    """Check every downloaded build artifact, then copy all their files (or
    none) into `bundles` and `cli`, each under its release name."""
    expected = build_artifacts(version)
    found = sorted(os.listdir(source))
    errors = [f"unexpected build artifact {name}" for name in found
              if name not in expected]
    errors += [f"build artifact {name} is missing" for name in expected
               if name not in found]
    copies: list[tuple[str, str]] = []
    targets: dict[str, str] = {}
    for artifact, (dest, suffix, patterns) in sorted(expected.items()):
        folder = os.path.join(source, artifact)
        if not os.path.isdir(folder):
            continue
        matched: dict[str, list[str]] = {kind: [] for kind in patterns}
        for name in sorted(os.listdir(folder)):
            path = os.path.join(folder, name)
            kind = next((k for k, p in patterns.items() if re.fullmatch(p, name)), None)
            if kind is None or os.path.islink(path) or not os.path.isfile(path):
                errors.append(f"{artifact}: unexpected file {name}")
                continue
            matched[kind].append(name)
            new = release_name(suffix, kind, version)
            try:
                check_file_name(new)
            except ManifestError as e:
                errors.append(f"{artifact}: {e}")
                continue
            if not RELEASE_NAME.fullmatch(new):
                errors.append(f"{artifact}: {new!r} has a character GitHub would rewrite")
                continue
            if new in targets:
                errors.append(f"{artifact}: {name} and {targets[new]} would both be {new}")
                continue
            targets[new] = name
            copies.append((path, os.path.join(bundles if dest == "bundles" else cli, new)))
        for kind, names in matched.items():
            if not names:
                errors.append(f"{artifact}: no {kind} file for {version.lstrip('v')}")
            elif len(names) > 1:
                errors.append(f"{artifact}: more than one {kind} file: {', '.join(names)}")
    if errors:
        return errors
    if any(os.path.exists(dst) for _src, dst in copies):
        return [f"{bundles} or {cli} is not empty"]
    os.makedirs(bundles, exist_ok=True)
    os.makedirs(cli, exist_ok=True)
    for src, dst in copies:
        shutil.copyfile(src, dst)
    return []
```

Update the module docstring's `stage-builds` paragraph: "…and copy their files into B (app bundles) and C (CLI binaries) under their release names: `Coverly-ROMs_<V>_<platform><kind>` and `coverly_<V>_<platform>[.exe]`, platform being one of mac_arm, mac_intel, windows_x64, windows_arm, linux_x64, linux_arm. …".

- [ ] **Step 6: Run the tests and watch them pass**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && cargo test -p coverly-roms --test release_pipeline stage 2>&1 | tail -15
```

Expected: both stage tests PASS. Other tests in the file may now fail (Task 2 and Task 4 fix them); note which ones and do not commit a red suite: run `cargo test -p coverly-roms --test release_pipeline 2>&1 | grep -E "^test .* FAILED"` and confirm only tests that read the manifest or the workflow (`release_tools_map_the_update_artifacts_to_platform_keys`, `the_app_job_stages_bundles_that_finalize_can_map`) fail. If so, mark those two `#[ignore = "release names: re-enabled in Task 2/Task 4"]` for this commit only.

- [ ] **Step 7: Gates, commit, push**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast 2>&1 | tail -5 && pnpm check-all && pnpm build
git add scripts/release/release_tools.py src-tauri/tests/release_pipeline.rs
git commit -m "feat(release): stage every build under its release name"
git push && git status -sb
```

---

## Task 2: The manifest reads release names (full-name parser, CLI names)

**Files:**
- Modify: `scripts/release/release_tools.py` (`updater_platforms`, `CLI_FILE`, `add_cli`, docstring; remove `ARCH_TOKENS`/`ARCH_RE`)
- Test: `src-tauri/tests/release_pipeline.rs` (`BUNDLES`, `release_tools_map_the_update_artifacts_to_platform_keys`, `release_tools_fall_back_to_the_msi_and_refuse_ambiguous_or_unsigned_bundles`, new `updater_manifest_refuses_names_that_are_not_release_names`, new `add_cli_reads_release_names`)

- [ ] **Step 1: Move the manifest fixtures to release names**

Replace `BUNDLES` with:

```rust
/// What `stage-builds` leaves in `bundles/` for 0.2.0 (stable), spec §4.
const BUNDLES: [&str; 12] = [
    "Coverly-ROMs_0.2.0_mac_arm.app.tar.gz",
    "Coverly-ROMs_0.2.0_mac_arm.dmg",
    "Coverly-ROMs_0.2.0_mac_intel.app.tar.gz",
    "Coverly-ROMs_0.2.0_mac_intel.dmg",
    "Coverly-ROMs_0.2.0_windows_x64-setup.exe",
    "Coverly-ROMs_0.2.0_windows_x64.msi",
    "Coverly-ROMs_0.2.0_windows_arm-setup.exe",
    "Coverly-ROMs_0.2.0_windows_arm.msi",
    "Coverly-ROMs_0.2.0_linux_x64.AppImage",
    "Coverly-ROMs_0.2.0_linux_x64.deb",
    "Coverly-ROMs_0.2.0_linux_arm.AppImage",
    "Coverly-ROMs_0.2.0_linux_arm.deb",
];
```

In `release_tools_map_the_update_artifacts_to_platform_keys`:
- `updater_artifacts` expectation: the 10 non-`.dmg` names, sorted (`Coverly-ROMs_0.2.0_linux_arm.AppImage`, `…_linux_arm.deb`, `…_linux_x64.AppImage`, `…_linux_x64.deb`, `…_mac_arm.app.tar.gz`, `…_mac_intel.app.tar.gz`, `…_windows_arm-setup.exe`, `…_windows_arm.msi`, `…_windows_x64-setup.exe`, `…_windows_x64.msi`);
- the key table becomes the full spec §4 manifest table:

```rust
    for (k, suffix) in [
        ("darwin-aarch64", "mac_arm.app.tar.gz"),
        ("darwin-aarch64-app", "mac_arm.app.tar.gz"),
        ("darwin-x86_64", "mac_intel.app.tar.gz"),
        ("darwin-x86_64-app", "mac_intel.app.tar.gz"),
        ("windows-x86_64", "windows_x64-setup.exe"),
        ("windows-x86_64-nsis", "windows_x64-setup.exe"),
        ("windows-x86_64-msi", "windows_x64.msi"),
        ("windows-aarch64", "windows_arm-setup.exe"),
        ("windows-aarch64-nsis", "windows_arm-setup.exe"),
        ("windows-aarch64-msi", "windows_arm.msi"),
        ("linux-x86_64", "linux_x64.AppImage"),
        ("linux-x86_64-appimage", "linux_x64.AppImage"),
        ("linux-x86_64-deb", "linux_x64.deb"),
        ("linux-aarch64", "linux_arm.AppImage"),
        ("linux-aarch64-appimage", "linux_arm.AppImage"),
        ("linux-aarch64-deb", "linux_arm.deb"),
    ] {
        assert_eq!(file(k), format!("Coverly-ROMs_0.2.0_{suffix}"), "{k}");
    }
    assert!(!m.platforms.values().any(|e| e.url.ends_with(".dmg")));
    assert_eq!(m.platforms.len(), 16, "6 generic + 10 bundle-specific keys");
```

- the CLI loop writes `coverly_0.2.0_{suffix}{ext}` for each `PLATFORMS` entry (six), signs each with `key.sign(..)`;
- `assert_eq!(args.len(), 7 + 24, "finalize requires the twelve keys")`. Task 4 adds the two `--require` lines to `finalize`, so this assertion stays red until then: for this one commit mark the test `#[ignore = "re-enabled in Task 4"]` after checking that everything above the assertion passes (temporarily comment the assertion, run, restore). Task 4 Step 4 removes every such `#[ignore]`.

In `release_tools_fall_back_to_the_msi_and_refuse_ambiguous_or_unsigned_bundles`: `msi_only` holds `Coverly-ROMs_0.2.0_windows_x64.msi`; the cases become `("unsigned", &["Coverly-ROMs_0.2.0_linux_x64.AppImage"], false)`, `("twice", &["Coverly-ROMs_0.2.0_linux_x64.AppImage", "Coverly-ROMs_0.2.1_linux_x64.AppImage"], true)`, `("no-arch", &["Coverly.ROMs.app.tar.gz"], true)`.

- [ ] **Step 2: Write the two new failing tests**

```rust
#[test]
fn updater_manifest_refuses_names_that_are_not_release_names() {
    if python().is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let latest = dir.path().join("latest.json");
    for (case, name) in [
        ("old tauri name", "Coverly.ROMs_0.2.0_x64-setup.exe"),
        ("old universal", "Coverly.ROMs_0.2.0_universal.app.tar.gz"),
        ("suffix of another os", "Coverly-ROMs_0.2.0_mac_arm-setup.exe"),
        ("linux suffix on a msi", "Coverly-ROMs_0.2.0_linux_x64.msi"),
        ("a space", "Coverly ROMs_0.2.0_linux_x64.AppImage"),
        ("unknown platform", "Coverly-ROMs_0.2.0_mac_silicon.app.tar.gz"),
    ] {
        let d = dir.path().join(case.replace(' ', "-"));
        std::fs::create_dir(&d).unwrap();
        write_pinned(&d.join(name), name.as_bytes());
        write_pinned(&d.join(format!("{name}.sig")), b"c2ln");
        let out = updater_manifest(&d, &latest, &[]);
        assert_clean_failure(&out, case);
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("not a release name"),
            "{case}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!latest.exists(), "{case}");
    }
}

#[test]
fn add_cli_reads_release_names() {
    if python().is_none() {
        return;
    }
    let add = |dir: &Path, manifest: &Path| {
        tool(&[
            "add-cli",
            "--manifest",
            manifest.to_str().unwrap(),
            "--dir",
            dir.to_str().unwrap(),
            "--version",
            "0.3.0",
            "--base-url",
            "https://github.com/MAECLY/coverly-roms/releases/download/v0.3.0",
        ])
    };
    let root = tempfile::tempdir().unwrap();
    let good = root.path().join("good");
    for (suffix, os, _arch) in PLATFORMS {
        let ext = if os == "windows" { ".exe" } else { "" };
        let name = format!("coverly_0.3.0_{suffix}{ext}");
        write_pinned(&good.join(&name), name.as_bytes());
        write_pinned(&good.join(format!("{name}.sig")), b"c2ln");
    }
    let manifest = root.path().join("latest.json");
    let out = add(&good, &manifest);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    for (suffix, os, arch) in PLATFORMS {
        let ext = if os == "windows" { ".exe" } else { "" };
        assert!(
            m["platforms"][format!("cli-{os}-{arch}")]["url"]
                .as_str()
                .unwrap()
                .ends_with(&format!("/coverly_0.3.0_{suffix}{ext}")),
            "{os}-{arch}"
        );
    }
    for bad in [
        "coverly_0.3.0_linux_x64.exe",
        "coverly_0.3.0_windows_x64",
        "coverly-0.3.0-linux-x86_64",
        "coverly_0.2.9_linux_x64",
    ] {
        let d = root.path().join(bad);
        write_pinned(&d.join(bad), b"x");
        write_pinned(&d.join(format!("{bad}.sig")), b"c2ln");
        let m = root.path().join(format!("{bad}.json"));
        assert_clean_failure(&add(&d, &m), bad);
    }
}
```

- [ ] **Step 3: Run and watch them fail**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && cargo test -p coverly-roms --test release_pipeline -- release_tools_map updater_manifest_refuses add_cli_reads release_tools_fall_back 2>&1 | tail -30
```

Expected: FAIL (`no architecture in the file name` for `Coverly-ROMs_…_mac_arm.app.tar.gz`; `unexpected file coverly_0.3.0_linux_x64`).

- [ ] **Step 4: Implement**

Delete `ARCH_TOKENS` and `ARCH_RE`. Add after `UPDATER_KINDS`:

```python
_SUFFIXES = "|".join(re.escape(s) for s in RELEASE_PLATFORMS)
APP_RELEASE = re.compile(
    re.escape(APP_PREFIX) + r"_(?P<v>[0-9A-Za-z.-]+)_(?P<suffix>" + _SUFFIXES + r")"
    r"(?P<kind>\.app\.tar\.gz|-setup\.exe|\.msi|\.AppImage|\.deb)"
)
```

In `updater_platforms`, replace the token lookup with:

```python
        os_name, bundle = updater_kind(name)
        m = APP_RELEASE.fullmatch(name)
        if not m or RELEASE_PLATFORMS[m.group("suffix")][0] != os_name:
            errors.append(f"{name}: not a release name")
            continue
        arch = RELEASE_PLATFORMS[m.group("suffix")][1]
        key = f"{os_name}-{arch}-{bundle}"
        if key in specific:
            errors.append(f"{key}: both {specific[key]} and {name}")
            continue
        specific[key] = name
```

(the version group is not compared here: `updater-manifest` writes `--version` and `check-manifest --signed-version` binds every signature to it; spec §5.1). Update the function docstring (no universal sentence).

Replace `CLI_FILE` and the loop body of `add_cli`:

```python
CLI_FILE = re.compile(
    re.escape(CLI_PREFIX) + r"_(?P<v>[0-9A-Za-z.-]+)_(?P<suffix>" + _SUFFIXES + r")(?P<exe>\.exe)?"
)
```

```python
        m = CLI_FILE.fullmatch(name)
        if not m or m.group("v") != version.lstrip("v"):
            errors.append(f"unexpected file {name}")
            continue
        os_name, arch = RELEASE_PLATFORMS[m.group("suffix")]
        if bool(m.group("exe")) != (os_name == "windows"):
            errors.append(f"unexpected file {name}")
            continue
```

and the key becomes `f"cli-{os_name}-{arch}"`. Update the docstring paragraphs of `updater-manifest` (keys per spec §4 table, "a name that is not a release name") and `add-cli` (`coverly_<V>_<platform>[.exe]`, `.exe` on Windows only).

- [ ] **Step 5: Run and watch them pass**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && cargo test -p coverly-roms --test release_pipeline 2>&1 | grep -E "^test |test result"
python3 -I scripts/release/release_tools.py --help >/dev/null && python3 -I scripts/release/release_tools.py updater-manifest --help >/dev/null && python3 -I scripts/release/release_tools.py add-cli --help >/dev/null && echo help-ok
```

Expected: all PASS except the workflow-reading assertion (`7 + 24`) that Task 4 completes; `help-ok`.

- [ ] **Step 6: Gates, commit, push**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast 2>&1 | tail -5 && pnpm check-all && pnpm build
git add scripts/release/release_tools.py src-tauri/tests/release_pipeline.rs
git commit -m "feat(release): map release names to the updater's platform keys"
git push && git status -sb
```

---

## Task 3: Prove that installed 0.1.0 / 0.2.0 copies keep updating

**Files:**
- Test: `src-tauri/tests/release_pipeline.rs` (new `full_manifest` helper, `installed_versions_find_their_update`, `recovery_tools_still_read_a_v0_2_0_manifest`)

These tests pin behaviour that Tasks 1–2 already deliver; they must pass on first run. To prove they can fail, Step 2 breaks one key on purpose and watches the test go red.

- [ ] **Step 1: Write the helper and the compatibility test**

```rust
/// A complete stable manifest for `v`, built the way finalize builds it:
/// stage-builds, real signatures from a test key, updater-manifest, add-cli.
fn full_manifest(root: &Path, v: &str, key: &coverly_core::update::testing::TestKey) -> PathBuf {
    downloaded_builds(&root.join("downloads"), v, true);
    let out = stage_builds(root, v);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let sign_dir = |d: &Path| {
        for name in listing(d) {
            if name.ends_with(".dmg") {
                continue;
            }
            let body = std::fs::read(d.join(&name)).unwrap();
            write_pinned(&d.join(format!("{name}.sig")), key.sign(&body, &name, Some(v)).as_bytes());
        }
    };
    sign_dir(&root.join("bundles"));
    sign_dir(&root.join("cli"));
    let base = format!("https://github.com/MAECLY/coverly-roms/releases/download/v{v}");
    let latest = root.join("latest.json");
    let out = tool(&[
        "updater-manifest", "--dir", root.join("bundles").to_str().unwrap(), "--version", v,
        "--base-url", &base, "--output", latest.to_str().unwrap(), "--pub-date", "2026-11-01T00:00:00Z",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = tool(&[
        "add-cli", "--manifest", latest.to_str().unwrap(), "--dir", root.join("cli").to_str().unwrap(),
        "--version", v, "--base-url", &base,
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    latest
}

/// Every key an installed copy asks for, with the file it must receive.
/// The app: tauri-plugin-updater ~2.14 (the version 0.1.0 and 0.2.0 ship)
/// looks up `{os}-{arch}-{installer}` first, then `{os}-{arch}`, arch being
/// the running binary's (a universal 0.2.0 build reports the chip it runs
/// on). The CLI: coverly_core::update::cli_platform_key(). The plugin cannot
/// be driven offline, so its keys are listed here.
const INSTALLED_KEYS: [(&str, &str); 22] = [
    ("darwin-aarch64", "Coverly-ROMs_{v}_mac_arm.app.tar.gz"),
    ("darwin-aarch64-app", "Coverly-ROMs_{v}_mac_arm.app.tar.gz"),
    ("darwin-x86_64", "Coverly-ROMs_{v}_mac_intel.app.tar.gz"),
    ("darwin-x86_64-app", "Coverly-ROMs_{v}_mac_intel.app.tar.gz"),
    ("windows-x86_64", "Coverly-ROMs_{v}_windows_x64-setup.exe"),
    ("windows-x86_64-nsis", "Coverly-ROMs_{v}_windows_x64-setup.exe"),
    ("windows-x86_64-msi", "Coverly-ROMs_{v}_windows_x64.msi"),
    ("linux-x86_64", "Coverly-ROMs_{v}_linux_x64.AppImage"),
    ("linux-x86_64-appimage", "Coverly-ROMs_{v}_linux_x64.AppImage"),
    ("linux-x86_64-deb", "Coverly-ROMs_{v}_linux_x64.deb"),
    ("linux-aarch64", "Coverly-ROMs_{v}_linux_arm.AppImage"),
    ("linux-aarch64-appimage", "Coverly-ROMs_{v}_linux_arm.AppImage"),
    ("linux-aarch64-deb", "Coverly-ROMs_{v}_linux_arm.deb"),
    ("cli-darwin-aarch64", "coverly_{v}_mac_arm"),
    ("cli-darwin-x86_64", "coverly_{v}_mac_intel"),
    ("cli-windows-x86_64", "coverly_{v}_windows_x64.exe"),
    ("cli-linux-x86_64", "coverly_{v}_linux_x64"),
    ("cli-linux-aarch64", "coverly_{v}_linux_arm"),
    // New in this release (no installed copy asks yet; the next ones will).
    ("windows-aarch64", "Coverly-ROMs_{v}_windows_arm-setup.exe"),
    ("windows-aarch64-nsis", "Coverly-ROMs_{v}_windows_arm-setup.exe"),
    ("windows-aarch64-msi", "Coverly-ROMs_{v}_windows_arm.msi"),
    ("cli-windows-aarch64", "coverly_{v}_windows_arm.exe"),
];

#[test]
fn installed_versions_find_their_update() {
    if python().is_none() {
        return;
    }
    use coverly_core::update::testing::TestKey;
    let key = TestKey::generate();
    let dir = tempfile::tempdir().unwrap();
    let v = "0.3.0";
    let latest = full_manifest(dir.path(), v, &key);
    let bytes = std::fs::read(&latest).unwrap();
    let m = coverly_core::update::parse_manifest(&bytes).expect("the CLI's parser reads it");
    assert_eq!(m.version, v);
    assert!(coverly_core::update::is_newer("0.2.0", &m.version));
    assert!(coverly_core::update::is_newer("0.1.0", &m.version));
    for (k, want) in INSTALLED_KEYS {
        let entry = m.platforms.get(k).unwrap_or_else(|| panic!("no {k}: an installed copy would stop updating"));
        let name = entry.url.rsplit('/').next().unwrap();
        assert_eq!(name, want.replace("{v}", v), "{k}");
        // The signature is the real one of that very file, bound to v.
        let body = std::fs::read(dir.path().join(if k.starts_with("cli-") { "cli" } else { "bundles" }).join(name)).unwrap();
        coverly_core::update::verify::verify_signed(&body, &entry.signature, &key.public_b64(), v)
            .unwrap_or_else(|e| panic!("{k}: {e}"));
    }
    // The CLI's own lookup, for every (os, arch) it can run on.
    for (os, arch) in [("macos", "aarch64"), ("macos", "x86_64"), ("windows", "x86_64"),
                       ("windows", "aarch64"), ("linux", "x86_64"), ("linux", "aarch64")] {
        let k = coverly_core::update::cli_platform_key_for(os, arch).unwrap();
        assert!(m.platforms.contains_key(&k), "{os}/{arch} -> {k}");
    }
    // Same manifest through the strict serde shape the app reads.
    let _: coverly_core::update::Manifest = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(m.platforms.len(), 16 + 6, "16 app keys + 6 CLI keys, nothing else");
}

#[test]
fn recovery_tools_still_read_a_v0_2_0_manifest() {
    if python().is_none() {
        return;
    }
    // The shape v0.2.0 published (old names), as list-artifacts and
    // resign-manifest must keep reading it.
    let old = [
        ("darwin-aarch64", "Coverly.ROMs_0.2.0_universal.app.tar.gz"),
        ("darwin-x86_64", "Coverly.ROMs_0.2.0_universal.app.tar.gz"),
        ("windows-x86_64", "Coverly.ROMs_0.2.0_x64-setup.exe"),
        ("windows-x86_64-msi", "Coverly.ROMs_0.2.0_x64_en-US.msi"),
        ("linux-x86_64", "Coverly.ROMs_0.2.0_amd64.AppImage"),
        ("cli-linux-aarch64", "coverly-0.2.0-linux-aarch64"),
    ];
    let dir = tempfile::tempdir().unwrap();
    let base = "https://github.com/MAECLY/coverly-roms/releases/download/v0.2.0";
    let platforms: serde_json::Map<String, serde_json::Value> = old
        .iter()
        .map(|(k, n)| ((*k).to_string(), serde_json::json!({"url": format!("{base}/{n}"), "signature": "b2xk"})))
        .collect();
    let manifest = dir.path().join("latest.json");
    std::fs::write(&manifest, serde_json::json!({"version": "0.2.0", "platforms": platforms}).to_string()).unwrap();
    let out = tool(&["list-artifacts", "--manifest", manifest.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let listed: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(String::from).collect();
    assert_eq!(listed.len(), 5, "each file once: {listed:?}");
    let sigs = dir.path().join("sigs");
    for name in &listed {
        write_pinned(&sigs.join(format!("{name}.sig")), b"bmV3");
    }
    let out = tool(&["resign-manifest", "--manifest", manifest.to_str().unwrap(), "--sigdir", sigs.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    for (k, _) in old {
        assert_eq!(m["platforms"][k]["signature"], "bmV3", "{k}");
    }
}
```

Check before writing: `coverly_core::update::Manifest`, `parse_manifest`, `is_newer`, `cli_platform_key_for` (takes `std::env::consts::OS` names: `macos`, not `darwin`), `verify::verify_signed(data, signature, pubkey_b64, version)` and `testing::TestKey::sign(data, file_name, version)` are the signatures in `crates/coverly-core/src/update/{mod,verify,testing}.rs` on this branch; if `parse_manifest` already returns the struct, drop the extra `serde_json` line. Do not change `coverly-core`. Run `cargo fmt` afterwards (the long lines above are reflowed by it).

- [ ] **Step 2: Run, then prove the test can fail**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && cargo test -p coverly-roms --test release_pipeline installed_versions recovery_tools 2>&1 | tail -8
```

Expected: both PASS. Then temporarily change `"windows-x86_64-msi"`'s `bundle` handling by editing `UPDATER_KINDS` `(".msi", "windows", "msi")` to `"wix"`, rerun, watch `installed_versions_find_their_update` FAIL with `no windows-x86_64-msi: an installed copy would stop updating`, and revert with `git checkout scripts/release/release_tools.py`.

- [ ] **Step 3: Gates, commit, push**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && cargo fmt --all && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast 2>&1 | tail -5 && pnpm check-all && pnpm build
git add src-tauri/tests/release_pipeline.rs
git commit -m "test(release): prove installed copies find their update under the new names"
git push && git status -sb
```

---

## Task 4: Workflows: split macOS, Windows ARM64, 12 required keys

**Files:**
- Modify: `.github/workflows/release.yml` (`app` matrix and staging step, new Windows ARM toolchain step in `app` and `cli`, `cli` matrix, `finalize` `--require`, comments)
- Modify: `.github/workflows/release-channels.yml` (two `--require`)
- Test: `src-tauri/tests/release_pipeline.rs` (`only_draft_and_finalize_can_write…`, `the_app_job_stages_bundles_that_finalize_can_map`, `finalize_requires_every_platform_and_writes_sha256sums`, `bundle_targets_and_macos_signing`, `finalize_checksums` fixtures, new `the_windows_arm_jobs_check_their_toolchain_without_secrets`, new `release_channels_require_the_same_keys_as_finalize`)

- [ ] **Step 1: Flip the workflow assertions (failing)**

In `only_draft_and_finalize_can_write_and_neither_builds_third_party_code`, replace the last two asserts with:

```rust
    assert!(!wf.contains("universal-apple-darwin"), "no universal build any more");
    for needle in [
        "--target aarch64-apple-darwin",
        "--target x86_64-apple-darwin",
        "platform: windows-11-arm",
        "ubuntu-22.04-arm",
    ] {
        assert!(wf.contains(needle), "{needle}");
    }
    let app = job(&wf, "app");
    assert_eq!(app.matches("slug: ").count(), 6, "six app builds");
    for slug in ["darwin-aarch64", "darwin-x86_64", "windows-x86_64", "windows-aarch64", "linux-x86_64", "linux-aarch64"] {
        assert!(app.contains(&format!("slug: {slug}\n")), "{slug}");
    }
    let cli = job(&wf, "cli");
    assert_eq!(cli.matches("target: ").count(), 6, "six CLI builds");
    assert!(cli.contains("target: aarch64-pc-windows-msvc"));
```

In `finalize_requires_every_platform_and_writes_sha256sums` add `"windows-aarch64"` and `"cli-windows-aarch64"` to the key list.

Add:

```rust
#[test]
fn release_channels_require_the_same_keys_as_finalize() {
    let requires = |text: &str| -> Vec<String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut keys: Vec<String> = words
            .windows(2)
            .filter(|w| w[0] == "--require")
            .map(|w| w[1].trim_end_matches('\\').to_string())
            .collect();
        keys.sort();
        keys
    };
    let finalize = requires(&job(&read(".github/workflows/release.yml"), "finalize"));
    assert_eq!(finalize.len(), 12);
    assert_eq!(requires(&read(".github/workflows/release-channels.yml")), finalize);
}

#[test]
fn the_windows_arm_jobs_check_their_toolchain_without_secrets() {
    let wf = read(".github/workflows/release.yml");
    for name in ["app", "cli"] {
        let j = job(&wf, name);
        let step: String = j
            .lines()
            .skip_while(|l| !l.contains("name: Windows ARM64 toolchain"))
            .take_while(|l| !l.contains("name: Rust toolchain"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!step.is_empty(), "{name}: no Windows ARM64 toolchain step before Rust toolchain");
        assert!(step.contains("if: matrix.platform == 'windows-11-arm'"), "{name}");
        assert!(step.contains("static.rust-lang.org/rustup/dist/aarch64-pc-windows-msvc/rustup-init.exe"), "{name}");
        assert!(step.contains(".sha256") && step.contains("Get-FileHash"), "{name}: rustup-init is checked");
        assert!(step.contains("::error::clang is not installed on this runner"), "{name}");
        assert!(!step.contains("secrets."), "{name}: the toolchain step sees no secret");
        assert!(!step.contains("uses:"), "{name}: no third-party action");
    }
    assert!(job(&wf, "app").contains("NDP\\v3.5"), "WiX needs .NET 3.5 for the .msi");
}
```

Rewrite `the_app_job_stages_bundles_that_finalize_can_map` to run the staging step once per macOS architecture plus once for Windows ARM64, then feed the result to `stage-builds`:

```rust
#[test]
fn the_app_job_stages_bundles_that_finalize_can_map() {
    if !scripts_runnable() {
        return;
    }
    let script = step_script(
        &job(&read(".github/workflows/release.yml"), "app"),
        "Stage the bundles for finalize",
    );
    // What `tauri build` leaves on each runner, the artifact it becomes and
    // the matrix tauri_arch.
    let runners: [(&str, &str, &[(&str, &str)]); 3] = [
        ("app-darwin-aarch64", "aarch64", &[
            ("target/aarch64-apple-darwin/release/bundle/macos/Coverly ROMs.app.tar.gz", "mac"),
            ("target/aarch64-apple-darwin/release/bundle/macos/Coverly ROMs.app.tar.gz.sig", "throwaway"),
            ("target/aarch64-apple-darwin/release/bundle/macos/Coverly ROMs.app/Contents/Info.plist", "x"),
            ("target/aarch64-apple-darwin/release/bundle/dmg/Coverly ROMs_0.3.0_aarch64.dmg", "dmg"),
        ]),
        ("app-darwin-x86_64", "x64", &[
            ("target/x86_64-apple-darwin/release/bundle/macos/Coverly ROMs.app.tar.gz", "mac"),
            ("target/x86_64-apple-darwin/release/bundle/dmg/Coverly ROMs_0.3.0_x64.dmg", "dmg"),
        ]),
        ("app-windows-aarch64", "", &[
            ("target/release/bundle/nsis/Coverly ROMs_0.3.0_arm64-setup.exe", "nsis"),
            ("target/release/bundle/nsis/Coverly ROMs_0.3.0_arm64-setup.exe.sig", "throwaway"),
            ("target/release/bundle/msi/Coverly ROMs_0.3.0_arm64_en-US.msi", "msi"),
        ]),
    ];
    let root = tempfile::tempdir().unwrap();
    // The rest of the release, as stage-builds expects it.
    downloaded_builds(&root.path().join("downloads"), "0.3.0", true);
    for (artifact, tauri_arch, files) in runners {
        let d = root.path().join(artifact.replace("app-", "runner-"));
        for (rel, body) in files {
            let mut p = d.clone();
            for part in rel.split('/') {
                p.push(part);
            }
            write_pinned(&p, body.as_bytes());
        }
        let out = Command::new("bash")
            .arg("-c")
            .arg(&script)
            .current_dir(&d)
            .env("GITHUB_REF_NAME", "v0.3.0")
            .env("TAURI_ARCH", tauri_arch)
            .output()
            .unwrap();
        assert!(out.status.success(), "{artifact}: {}", String::from_utf8_lossy(&out.stderr));
        // The staged folder replaces that artifact's fixture.
        let dl = root.path().join("downloads").join(artifact);
        std::fs::remove_dir_all(&dl).unwrap();
        std::fs::rename(d.join("staged"), &dl).unwrap();
    }
    assert_eq!(
        listing(&root.path().join("downloads/app-darwin-x86_64")),
        ["Coverly.ROMs_0.3.0_x64.app.tar.gz", "Coverly.ROMs_0.3.0_x64.dmg"],
        "no throwaway .sig, no spaces, the macOS archive versioned with its arch"
    );
    let out = stage_builds(root.path(), "0.3.0");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(listing(&root.path().join("bundles")), release_files("0.3.0", true).0);
}
```

(Paths are built component by component with `push`, never by joining slash text, so the test behaves the same on the Windows CI leg.) The staging step is renamed from "Stage the bundles under their release names" to "Stage the bundles for finalize" because it no longer chooses release names.

In `bundle_targets_and_macos_signing` change the message to `"ad hoc: an unsigned arm64 app does not launch on Apple silicon"`.

In `finalize_requires_the_msi_only_for_a_stable_release` and `finalize_checksums`, switch the fixture names to release names (`Coverly-ROMs_1.0.0_mac_arm.dmg`, `…_mac_arm.app.tar.gz`, `…_windows_x64-setup.exe`, `…_linux_x64.AppImage`, `…_linux_x64.deb`; `with_msi` pushes `Coverly-ROMs_1.0.0_windows_x64.msi`; the CLI file is `cli/coverly_1.0.0_linux_x64`).

- [ ] **Step 2: Run and watch them fail**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && cargo test -p coverly-roms --test release_pipeline 2>&1 | grep -E "FAILED|panicked" | head -20
```

Expected: the four workflow tests FAIL (`universal-apple-darwin` still present, five slugs, no toolchain step, ten keys).

- [ ] **Step 3: Edit `release.yml`**

`app` matrix (replace the four entries):

```yaml
        include:
          - name: macOS Apple silicon
            slug: darwin-aarch64
            platform: macos-latest
            targets: aarch64-apple-darwin
            args: '--target aarch64-apple-darwin'
            tauri_arch: aarch64
          - name: macOS Intel
            slug: darwin-x86_64
            platform: macos-latest
            targets: x86_64-apple-darwin
            args: '--target x86_64-apple-darwin'
            tauri_arch: x64
          - name: Windows x64
            slug: windows-x86_64
            platform: windows-latest
            targets: x86_64-pc-windows-msvc
            args: ''
            tauri_arch: ''
          - name: Windows ARM64
            slug: windows-aarch64
            platform: windows-11-arm
            targets: aarch64-pc-windows-msvc
            args: ''
            tauri_arch: ''
          - name: Linux x64
            slug: linux-x86_64
            platform: ubuntu-22.04
            targets: x86_64-unknown-linux-gnu
            args: ''
            tauri_arch: ''
          - name: Linux ARM64
            slug: linux-aarch64
            platform: ubuntu-22.04-arm
            targets: aarch64-unknown-linux-gnu
            args: ''
            tauri_arch: ''
```

New step in **both** `app` and `cli`, right before "Rust toolchain" (in `cli` set `NETFX: 'false'`; in `app` use the expression below):

```yaml
      - name: Windows ARM64 toolchain (rustup, clang, .NET 3.5)
        if: matrix.platform == 'windows-11-arm'
        shell: pwsh
        env:
          # WiX 3 (the .msi, stable tags only) needs .NET Framework 3.5.
          NETFX: ${{ !contains(github.ref_name, '-') }}
        run: |
          $ErrorActionPreference = 'Stop'
          if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
            $url = 'https://static.rust-lang.org/rustup/dist/aarch64-pc-windows-msvc/rustup-init.exe'
            $exe = Join-Path $env:RUNNER_TEMP 'rustup-init.exe'
            Invoke-WebRequest -Uri $url -OutFile $exe
            Invoke-WebRequest -Uri "$url.sha256" -OutFile "$exe.sha256"
            $want = ((Get-Content "$exe.sha256" -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
            $got = (Get-FileHash -Algorithm SHA256 $exe).Hash.ToLowerInvariant()
            if ($got -ne $want) { Write-Output "::error::rustup-init.exe does not match its .sha256"; exit 1 }
            & $exe -y --profile minimal --default-toolchain none
            if ($LASTEXITCODE -ne 0) { Write-Output "::error::rustup-init.exe failed"; exit 1 }
            Join-Path $env:USERPROFILE '.cargo\bin' | Out-File -FilePath $env:GITHUB_PATH -Append -Encoding utf8
          }
          if (-not (Get-Command clang -ErrorAction SilentlyContinue)) {
            # ring (pulled in by rustls) compiles C for aarch64-pc-windows-msvc.
            $llvm = 'C:\Program Files\LLVM\bin'
            if (Test-Path (Join-Path $llvm 'clang.exe')) {
              $llvm | Out-File -FilePath $env:GITHUB_PATH -Append -Encoding utf8
            } else {
              Write-Output '::error::clang is not installed on this runner'; exit 1
            }
          }
          if ($env:NETFX -eq 'true' -and -not (Test-Path 'HKLM:\SOFTWARE\Microsoft\NET Framework Setup\NDP\v3.5')) {
            Write-Output '::error::.NET Framework 3.5 (WiX, for the .msi) is not installed on this runner (NDP\v3.5)'; exit 1
          }
```

(In the `cli` copy, drop the `NETFX` env line and the last `if`, and keep the `NDP\v3.5` text out of it so the test's `app`-only assertion stays meaningful.)

Staging step: rename it to `Stage the bundles for finalize`, add `env: TAURI_ARCH: ${{ matrix.tauri_arch }}` and replace the `.app.tar.gz` case and its comment:

```bash
              # The macOS update archive carries no version or architecture;
              # give it Tauri's own token for this build (as the .dmg has),
              # so stage-builds can check it. Release names are chosen there.
              *.app.tar.gz)
                [ -n "$TAURI_ARCH" ] || { echo "::error::no tauri_arch for $base"; exit 1; }
                name="${base%.app.tar.gz}_${version}_${TAURI_ARCH}.app.tar.gz" ;;
```

`cli` matrix: add after the Windows x64 entry:

```yaml
          - {
              platform: windows-11-arm,
              target: aarch64-pc-windows-msvc,
              os: windows,
              arch: aarch64,
            }
```

`finalize` `check-manifest`: add `--require windows-aarch64` after `--require windows-x86_64` and `--require cli-windows-aarch64` after `--require cli-windows-x86_64`. Same two additions in `release-channels.yml`. Rewrite every comment in `release.yml` that says "universal".

- [ ] **Step 4: Lint and test**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && actionlint .github/workflows/release.yml .github/workflows/release-channels.yml && echo actionlint-ok
cargo test -p coverly-roms --test release_pipeline 2>&1 | grep -E "test result|FAILED"
```

Expected: `actionlint-ok` (shellcheck runs inside actionlint for the bash steps), `test result: ok`, no `FAILED`, and no `#[ignore]` left from Tasks 1–2 (`grep -n 'ignore = "re' src-tauri/tests/release_pipeline.rs` prints nothing).

- [ ] **Step 5: Gates, commit, push**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast 2>&1 | tail -5 && pnpm check-all && pnpm build
git add .github/workflows/release.yml .github/workflows/release-channels.yml src-tauri/tests/release_pipeline.rs
git commit -m "ci(release): build macOS per chip and add Windows ARM64"
git push && git status -sb
```

---

## Task 5: Local staging dry run script (evidence for the PR)

**Files:**
- Create (not committed): `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/release-names/dry_run.py`

This is throwaway tooling; nothing is committed in this task. It exercises `finalize`'s order end to end outside Rust, to catch anything the unit tests miss (for example a name that `sha256sum` would print differently).

- [ ] **Step 1: Write the script**

```python
#!/usr/bin/env python3
"""Dry run of finalize's staging and manifest steps with fake artifacts.

Usage: dry_run.py REPO OUT_DIR VERSION
Writes only under OUT_DIR, which must not exist yet. No real key, no network.
"""
import hashlib, json, os, subprocess, sys

repo, out, version = sys.argv[1], sys.argv[2], sys.argv[3]
stable = "-" not in version
tool = [sys.executable, "-I", os.path.join(repo, "scripts", "release", "release_tools.py")]
os.makedirs(out)
dl = os.path.join(out, "downloads")
files = {
    "app-darwin-aarch64": [f"Coverly.ROMs_{version}_aarch64.app.tar.gz", f"Coverly.ROMs_{version}_aarch64.dmg"],
    "app-darwin-x86_64": [f"Coverly.ROMs_{version}_x64.app.tar.gz", f"Coverly.ROMs_{version}_x64.dmg"],
    "app-windows-x86_64": [f"Coverly.ROMs_{version}_x64-setup.exe"] + ([f"Coverly.ROMs_{version}_x64_en-US.msi"] if stable else []),
    "app-windows-aarch64": [f"Coverly.ROMs_{version}_arm64-setup.exe"] + ([f"Coverly.ROMs_{version}_arm64_en-US.msi"] if stable else []),
    "app-linux-x86_64": [f"Coverly.ROMs_{version}_amd64.AppImage", f"Coverly.ROMs_{version}_amd64.deb"],
    "app-linux-aarch64": [f"Coverly.ROMs_{version}_aarch64.AppImage", f"Coverly.ROMs_{version}_arm64.deb"],
}
for os_name, arch in [("darwin", "aarch64"), ("darwin", "x86_64"), ("windows", "x86_64"),
                      ("windows", "aarch64"), ("linux", "x86_64"), ("linux", "aarch64")]:
    ext = ".exe" if os_name == "windows" else ""
    files[f"cli-{os_name}-{arch}"] = [f"coverly-{version}-{os_name}-{arch}{ext}"]
for artifact, names in files.items():
    os.makedirs(os.path.join(dl, artifact))
    for n in names:
        p = os.path.join(dl, artifact, n)
        with open(p, "w") as f:
            f.write(n)
        os.utime(p, (1_790_000_000, 1_790_000_000))
def run(*args):
    r = subprocess.run(tool + list(args), capture_output=True, text=True)
    if r.returncode:
        sys.exit(f"{args[0]} failed:\n{r.stderr}")
    return r.stdout
bundles, cli = os.path.join(out, "bundles"), os.path.join(out, "cli")
run("stage-builds", "--from", dl, "--bundles", bundles, "--cli", cli, "--version", version)
for n in run("updater-artifacts", "--dir", bundles).split():
    open(os.path.join(bundles, n + ".sig"), "w").write("ZmFrZQ==")
for n in os.listdir(cli):
    open(os.path.join(cli, n + ".sig"), "w").write("ZmFrZQ==")
base = f"https://github.com/MAECLY/coverly-roms/releases/download/v{version}"
latest = os.path.join(out, "latest.json")
run("updater-manifest", "--dir", bundles, "--version", version, "--base-url", base,
    "--output", latest, "--pub-date", "2026-11-01T00:00:00Z")
run("add-cli", "--manifest", latest, "--dir", cli, "--version", version, "--base-url", base)
keys = ["darwin-aarch64", "darwin-x86_64", "windows-x86_64", "windows-aarch64", "linux-x86_64",
        "linux-aarch64", "cli-darwin-aarch64", "cli-darwin-x86_64", "cli-windows-x86_64",
        "cli-windows-aarch64", "cli-linux-x86_64", "cli-linux-aarch64"]
run("check-manifest", "--manifest", latest, "--version", version, *sum((["--require", k] for k in keys), []))
sums = []
for d in (bundles, cli):
    for n in sorted(os.listdir(d)):
        if not n.endswith(".sig"):
            sums.append(f"{hashlib.sha256(open(os.path.join(d, n), 'rb').read()).hexdigest()}  {n}")
print("\n".join(sorted(sums, key=lambda s: s.split()[1])))
m = json.load(open(latest))
for k in sorted(m["platforms"]):
    print(f"{k:24} {m['platforms'][k]['url'].rsplit('/', 1)[1]}")
```

- [ ] **Step 2: Run it for a stable and a beta version, then delete the trees**

```bash
S=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/release-names
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && rm -rf "$S/dry-stable" "$S/dry-beta" && python3 -I "$S/dry_run.py" "$PWD" "$S/dry-stable" 0.3.0 | tee "$S/dry-stable.txt" && python3 -I "$S/dry_run.py" "$PWD" "$S/dry-beta" 0.3.0-beta.1 | tee "$S/dry-beta.txt" && rm -rf "$S/dry-stable" "$S/dry-beta"
```

Expected: stable prints 18 checksum lines (12 app + 6 CLI) and 22 manifest keys; beta prints 16 checksum lines and 20 keys (no `*-msi`). Compare by eye with spec §4; the two `.txt` files are the PR evidence used in Task 11. Nothing to commit (`git status -sb` clean).

---

## Task 6: Release notes (en/es download table)

**Files:**
- Modify: `.github/release-notes.md`
- Test: `src-tauri/tests/release_pipeline.rs` (`release_notes_explain_gatekeeper_and_smartscreen_in_en_and_es`)

- [ ] **Step 1: Flip the notes test (failing)**

Replace the two old-name needles with the new names and add the new sentences and a negative check:

```rust
        // The names stage-builds publishes (spec §4), in both languages.
        "Coverly-ROMs_<version>_mac_arm.dmg",
        "Coverly-ROMs_<version>_mac_intel.dmg",
        "Coverly-ROMs_<version>_windows_x64-setup.exe",
        "Coverly-ROMs_<version>_windows_arm-setup.exe",
        "Coverly-ROMs_<version>_linux_arm.AppImage",
        "Coverly-ROMs_<versión>_mac_arm.dmg",
        "Coverly-ROMs_<versión>_windows_x64-setup.exe",
        "Coverly-ROMs_<versión>_windows_arm-setup.exe",
        "coverly_<version>_<system>",
        "coverly_<versión>_<sistema>",
        "Windows 11 (ARM64)",
        "\"Chip\" means Apple silicon",
        "«Chip» es Apple silicon",
        "xattr -d com.apple.quarantine coverly_<version>_mac_arm",
```

and after the loop:

```rust
    for old in ["Coverly.ROMs_", "_universal", "_amd64", "_x64_en-US", "coverly-<"] {
        assert!(!notes.contains(old), "old release name left: {old}");
    }
```

Run `cargo test -p coverly-roms --test release_pipeline release_notes 2>&1 | tail -5`; expected FAIL on the first new needle.

- [ ] **Step 2: Rewrite the tables and add the sentences**

English table (keep the column alignment the formatter produces; run `pnpm format` if the repo formats Markdown, otherwise align by hand):

```markdown
| System                                       | File                                                                                                                      |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| macOS 11 or later, Apple silicon (M1 or newer) | `Coverly-ROMs_<version>_mac_arm.dmg`                                                                                    |
| macOS 11 or later, Intel                     | `Coverly-ROMs_<version>_mac_intel.dmg`                                                                                    |
| Windows 10/11 (x64)                          | `Coverly-ROMs_<version>_windows_x64-setup.exe` (recommended); stable releases also `Coverly-ROMs_<version>_windows_x64.msi` |
| Windows 11 (ARM64)                           | `Coverly-ROMs_<version>_windows_arm-setup.exe` (recommended); stable releases also `Coverly-ROMs_<version>_windows_arm.msi` |
| Linux x64                                    | `Coverly-ROMs_<version>_linux_x64.AppImage` or `Coverly-ROMs_<version>_linux_x64.deb`                                     |
| Linux ARM64                                  | `Coverly-ROMs_<version>_linux_arm.AppImage` or `Coverly-ROMs_<version>_linux_arm.deb`                                     |
| Command line                                 | `coverly_<version>_<system>` (`mac_arm`, `mac_intel`, `windows_x64.exe`, `windows_arm.exe`, `linux_x64`, `linux_arm`); once installed, `coverly update install` keeps it current |
```

Spanish table: same rows with «macOS 11 o posterior, Apple silicon (M1 o posterior)», «macOS 11 o posterior, Intel», «(recomendado); las versiones estables también …», «… o …», «Línea de comandos», `coverly_<versión>_<sistema>`, «después, `coverly update install` la mantiene al día», every `<version>` written `<versión>`.

Under each table add the spec §7 sentences verbatim:

- en: `Not sure which Mac you have? Apple menu → About This Mac: "Chip" means Apple silicon, "Processor" means Intel.` and `On a Windows ARM64 PC, install the ARM64 build. A copy of the x64 build installed earlier keeps working and updating as x64.`
- es: `¿No sabes qué Mac tienes? Menú Apple → Acerca de este Mac: «Chip» es Apple silicon, «Procesador» es Intel.` and `En un PC con Windows ARM64, instala la versión ARM64. Una copia de la versión x64 instalada antes sigue funcionando y actualizándose como x64.`

Change the CLI quarantine line in both languages to `xattr -d com.apple.quarantine coverly_<version>_mac_arm` (or `_mac_intel`) / `coverly_<versión>_mac_arm` (o `_mac_intel`). Any other old name in the file (Linux `.deb` example, etc.) moves to the new one.

- [ ] **Step 3: Check the draft job's substitution still covers the file**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && sed -e "s/<version>/0.3.0/g" -e "s/<versión>/0.3.0/g" .github/release-notes.md | grep -nE "<vers|Coverly\.ROMs|universal" ; echo "exit=$? (1 means nothing left)"
cargo test -p coverly-roms --test release_pipeline release_notes 2>&1 | tail -3
```

Expected: `exit=1`, test PASS.

- [ ] **Step 4: Gates, commit, push**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && pnpm check-all && cargo test -p coverly-roms --test release_pipeline 2>&1 | tail -3
git add .github/release-notes.md src-tauri/tests/release_pipeline.rs
git commit -m "docs(release): list the new download names and Windows ARM64 in the notes"
git push && git status -sb
```

---

## Task 7: Docs: runbook, READMEs, CI/CD, updates, ADR, changelog

**Files:**
- Modify: `RELEASING.md` (§1 job table, §5 checklist, §7 local builds, §8 `add-cli` names, §9 troubleshooting row), `README.md`, `README.es.md` (download table, Windows/Linux/quarantine notes), `docs/07-build-and-release/ci-cd.md` (`app`/`cli`/`finalize` rows), `docs/04-architecture/updates.md` (TL;DR six + six keys, example manifest, mapping paragraph), `docs/adr/0011-release-pipeline-and-update-key.md` ("Amended 2026-10-26" note), `CHANGELOG.md` (`[Unreleased]`)

- [ ] **Step 1: Find every old name**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && grep -nE 'universal|Coverly\.ROMs_|_amd64|_x64_en-US|coverly-<|coverly-0\.[0-9]|five `|the same 5 targets|ten keys' RELEASING.md README.md README.es.md docs/07-build-and-release/ci-cd.md docs/04-architecture/updates.md docs/adr/0011-release-pipeline-and-update-key.md
```

- [ ] **Step 2: Edit each hit**

- `RELEASING.md` §1: `app` row → "macOS Apple silicon and Intel (both on macos-latest, Intel cross-compiled), Windows x64, Windows ARM64 (`windows-11-arm`), Linux x64, Linux ARM64"; `cli` row → "the same 6 targets", staged as `coverly-<version>-<os>-<arch>[.exe]` and published as `coverly_<version>_<platform>[.exe]` by `stage-builds`. §5 checklist: list the 12 app files (10 on a prerelease) and 6 CLI files by their §4 names; `latest.json` maps `darwin-aarch64` → `_mac_arm.app.tar.gz`, `darwin-x86_64` → `_mac_intel.app.tar.gz`, `windows-aarch64` → `_windows_arm-setup.exe`; landing line: offers Apple silicon with an Intel link (no universal). Add a §5 note: "Merge the landing's `feat/release-names` PR before publishing the first release with these names (spec D11)." §8: `add-cli` reads `coverly_<V>_<platform>[.exe]`. §9 row: `updater-manifest` fails with `not a release name` → a bundle was not staged by `stage-builds`. Also add a §9 row for the Windows ARM toolchain errors (`clang is not installed`, `.NET Framework 3.5`) pointing at spec §9's fallback (`--bundles nsis` on that runner and drop the ARM64 `.msi` kind).
- `README.md` / `README.es.md`: download table with the six platforms + CLI exactly as the notes (Task 6), "Not sure which Mac" sentence, quarantine line `coverly_<version>_mac_arm`, Linux `sudo apt install ./Coverly-ROMs_<version>_linux_x64.deb` (or `_linux_arm.deb`). Windows requirement: "Windows 10/11 (x64) or Windows 11 (ARM64)".
- `docs/07-build-and-release/ci-cd.md`: `app` row lists the six runners and `--target` per macOS chip; `cli` "the same 6 targets"; `finalize` row: `stage-builds` renames to release names, 12 required keys.
- `docs/04-architecture/updates.md`: TL;DR "six `<os>-<arch>` keys … six `cli-<os>-<arch>` keys"; the example manifest uses `v0.3.0` and the new urls (add `windows-aarch64` and `cli-windows-aarch64` lines, `darwin-*` point at their own archive); the mapping paragraph says release names come from `stage-builds` and keys never change.
- ADR-0011: append a section `## Amended 2026-10-26` with three bullets: separate macOS builds (D2), Windows ARM64 (D6), release names chosen in `stage-builds` with manifest keys unchanged (D3/D4), link to the spec.
- `CHANGELOG.md` `[Unreleased]`: `### Changed` "Release files are named by system and processor (`Coverly-ROMs_<version>_mac_arm.dmg`, `coverly_<version>_linux_x64`, …); macOS ships one download per chip instead of a universal one. Installed copies keep updating." and `### Added` "Windows on ARM64 builds of the app and the command line."

- [ ] **Step 3: Check nothing old is left and the docs gates pass**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && grep -nE 'universal\.dmg|Coverly\.ROMs_|_amd64\.|_x64_en-US|coverly-<' RELEASING.md README.md README.es.md docs/07-build-and-release/ci-cd.md docs/04-architecture/updates.md .github/release-notes.md; echo "exit=$?"
pnpm check-all && cargo test -p coverly-roms --test release_pipeline 2>&1 | tail -3
```

Expected: only lines that deliberately describe the old v0.2.0 names (the ADR amendment, the RELEASING note about `list-artifacts` recovering old manifests); any other hit is fixed. `the_runbook_names_every_owner_step` and the other doc-reading tests PASS.

- [ ] **Step 4: Commit, push (one commit per area keeps review easy)**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && git add RELEASING.md docs/07-build-and-release/ci-cd.md && git commit -m "docs(release): describe per-chip macOS and Windows ARM64 jobs in the runbook" && git push
git add README.md README.es.md && git commit -m "docs(readme): list the new download names in en and es" && git push
git add docs/04-architecture/updates.md docs/adr/0011-release-pipeline-and-update-key.md && git commit -m "docs(updates): six platform keys and the amended release pipeline" && git push
git add CHANGELOG.md && git commit -m "docs(changelog): note the release names and Windows ARM64" && git push
git status -sb
```

---

## Task 8: Landing: slots with new and legacy patterns, optional slots, asset blocks

**Repository:** `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing`

**Files:**
- Modify: `scripts/site/apply_release.py` (`DOWNLOADS`, `values`, new `ASSET_BLOCK`/`asset_blocks`, `apply`, `main`, docstring)
- Create: `scripts/site/fixtures/release-new.json`, `scripts/site/fixtures/release-new-no-arm.json`
- Modify: `scripts/site/check_site.py` (variants, layout checks, run `release.test.mjs` once)

- [ ] **Step 1: Branch and push at once**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing && git status -sb && git switch main && git pull --ff-only && git switch -c feat/release-names && git push -u origin feat/release-names && git status -sb
```

Expected: `## feat/release-names...origin/feat/release-names`.

- [ ] **Step 2: Generate the two new fixtures from the legacy one**

Run once (a one-off generator, not committed):

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing && python3 -I - <<'PY'
import json
src = json.load(open("scripts/site/fixtures/release.json"))
v = "9.8.7"
base = f"https://github.com/MAECLY/coverly-roms/releases/download/v{v}/"
rename = {
    f"Coverly.ROMs_{v}_universal.dmg": f"Coverly-ROMs_{v}_mac_arm.dmg",
    f"Coverly.ROMs_{v}_universal.app.tar.gz": f"Coverly-ROMs_{v}_mac_arm.app.tar.gz",
    f"Coverly.ROMs_{v}_universal.app.tar.gz.sig": f"Coverly-ROMs_{v}_mac_arm.app.tar.gz.sig",
    f"Coverly.ROMs_{v}_x64-setup.exe": f"Coverly-ROMs_{v}_windows_x64-setup.exe",
    f"Coverly.ROMs_{v}_x64-setup.exe.sig": f"Coverly-ROMs_{v}_windows_x64-setup.exe.sig",
    f"Coverly.ROMs_{v}_x64_en-US.msi": f"Coverly-ROMs_{v}_windows_x64.msi",
    f"Coverly.ROMs_{v}_amd64.AppImage": f"Coverly-ROMs_{v}_linux_x64.AppImage",
    f"Coverly.ROMs_{v}_amd64.deb": f"Coverly-ROMs_{v}_linux_x64.deb",
    f"Coverly.ROMs_{v}_aarch64.AppImage": f"Coverly-ROMs_{v}_linux_arm.AppImage",
    f"Coverly.ROMs_{v}_arm64.deb": f"Coverly-ROMs_{v}_linux_arm.deb",
    f"coverly-{v}-darwin-aarch64": f"coverly_{v}_mac_arm",
    f"coverly-{v}-darwin-x86_64": f"coverly_{v}_mac_intel",
    f"coverly-{v}-windows-x86_64.exe": f"coverly_{v}_windows_x64.exe",
    f"coverly-{v}-linux-x86_64": f"coverly_{v}_linux_x64",
    f"coverly-{v}-linux-aarch64": f"coverly_{v}_linux_arm",
}
def asset(name, size):
    return {"name": name, "state": "uploaded", "size": size, "browser_download_url": base + name}
new = dict(src)
new["assets"] = [asset(rename.get(a["name"], a["name"]), a["size"]) for a in src["assets"]]
arm = [asset(f"Coverly-ROMs_{v}_windows_arm-setup.exe", 10_066_329),
       asset(f"Coverly-ROMs_{v}_windows_arm.msi", 11_324_620),
       asset(f"coverly_{v}_windows_arm.exe", 8_388_608)]
intel = [asset(f"Coverly-ROMs_{v}_mac_intel.dmg", 9_961_472),
         asset(f"Coverly-ROMs_{v}_mac_intel.app.tar.gz", 9_437_184)]
no_arm = dict(new, assets=new["assets"] + intel)
new = dict(new, assets=new["assets"] + intel + arm)
for path, r in (("scripts/site/fixtures/release-new.json", new),
                ("scripts/site/fixtures/release-new-no-arm.json", no_arm)):
    with open(path, "w") as f:
        json.dump(r, f, indent=2, ensure_ascii=False)
        f.write("\n")
PY
python3 -c "import json;[print(a['name']) for a in json.load(open('scripts/site/fixtures/release-new.json'))['assets']]"
```

Expected: 22 asset names, all `Coverly-ROMs_…` / `coverly_…` plus `SHA256SUMS`, `latest.json`. If `release.json` has a key order or indentation convention (check with `head -5`), match it.

- [ ] **Step 3: Write the failing gate checks in `check_site.py`**

- Add `FIXTURES = {"release": ROOT / "scripts/site/fixtures/release.json", "release-new": …/"release-new.json", "release-new-no-arm": …/"release-new-no-arm.json"}`; `build(site, out, fixture: Path | None)`; `main` loops over `[(name, path) for name, path in FIXTURES.items()] + [("no-release", None)]`, sets a module-level `VARIANT = name` before running the gates.
- In `gate_release`: run `release.test.mjs` only when `VARIANT == "release"` (once, as today). Add per-variant layout checks inside the `for rel in PAGES.values()` loop (`text = (site / rel).read_text(encoding="utf-8")`):

```python
        if "asset:" in text:
            r.fail("release", f"{rel}: an asset/noasset block comment is left over")
        intel, arm, universal = "_mac_intel.dmg" in text, "_windows_arm-setup.exe" in text, "_universal.dmg" in text
        expect = {"release": (False, False, True), "release-new": (True, True, False),
                  "release-new-no-arm": (True, False, False)}.get(VARIANT)
        if expect and (intel, arm, universal) != expect:
            r.fail("release", f"{rel}: layout for {VARIANT} is intel={intel} arm={arm} universal={universal}, expected {expect}")
```

- Keep "fewer than 13 data-asset links" as is (both layouts have at least 13).

Run `python3 -I scripts/site/check_site.py 2>&1 | tail -8`. Expected: FAIL on `release-new` (apply_release refuses: `has no asset for macos_universal`).

- [ ] **Step 4: Implement in `apply_release.py`**

Replace `APP`/`DOWNLOADS`:

```python
# Every download the landing offers: slot -> (required, patterns), {v} being
# the version. The first pattern is the release name the app repository's
# release_tools.py stage-builds publishes (Coverly-ROMs_<v>_<platform><kind>,
# coverly_<v>_<platform>[.exe]); the second, when there is one, is the name
# releases up to v0.2.0 used (Tauri's own, "Coverly ROMs" made "Coverly.ROMs").
# Optional slots exist only in the new scheme; a page shows them through
# <!-- asset:<slot> --> blocks.
NEW = r"Coverly-ROMs_{v}_"
OLD = r"[A-Za-z0-9._-]+_{v}_"
DOWNLOADS: dict[str, tuple[bool, tuple[str, ...]]] = {
    "macos_arm": (True, (NEW + r"mac_arm\.dmg", OLD + r"universal\.dmg")),
    "macos_intel": (False, (NEW + r"mac_intel\.dmg",)),
    "windows_x64": (True, (NEW + r"windows_x64-setup\.exe", OLD + r"x64-setup\.exe")),
    "windows_x64_msi": (True, (NEW + r"windows_x64\.msi", OLD + r"x64_[A-Za-z0-9-]+\.msi")),
    "windows_arm": (False, (NEW + r"windows_arm-setup\.exe",)),
    "windows_arm_msi": (False, (NEW + r"windows_arm\.msi",)),
    "linux_x64_appimage": (True, (NEW + r"linux_x64\.AppImage", OLD + r"amd64\.AppImage")),
    "linux_x64_deb": (True, (NEW + r"linux_x64\.deb", OLD + r"amd64\.deb")),
    "linux_arm64_appimage": (True, (NEW + r"linux_arm\.AppImage", OLD + r"aarch64\.AppImage")),
    "linux_arm64_deb": (True, (NEW + r"linux_arm\.deb", OLD + r"arm64\.deb")),
    "cli_macos_arm64": (True, (r"coverly_{v}_mac_arm", r"coverly-{v}-darwin-aarch64")),
    "cli_macos_x64": (True, (r"coverly_{v}_mac_intel", r"coverly-{v}-darwin-x86_64")),
    "cli_windows_x64": (True, (r"coverly_{v}_windows_x64\.exe", r"coverly-{v}-windows-x86_64\.exe")),
    "cli_windows_arm64": (False, (r"coverly_{v}_windows_arm\.exe",)),
    "cli_linux_x64": (True, (r"coverly_{v}_linux_x64", r"coverly-{v}-linux-x86_64")),
    "cli_linux_arm64": (True, (r"coverly_{v}_linux_arm", r"coverly-{v}-linux-aarch64")),
}
ASSET_BLOCK = re.compile(r"<!--\s*(no)?asset:([a-z0-9_]+):(begin|end)\s*-->")
```

In `values()`, change `asset()` to take the pattern tuple and an `optional` flag:

```python
    def asset(patterns: tuple[str, ...], label: str, required: bool = True):
        hits = []
        for p in patterns:
            rx = re.compile(p.replace("{v}", re.escape(version)))
            found = [a for a in assets if rx.fullmatch(str(a.get("name")))]
            if found:
                hits.append(found)
        if not hits:
            if not required:
                return None
            raise ReleaseError(f"{tag} has no asset for {label} ({' or '.join(patterns)})")
        if len(hits) > 1 or len(hits[0]) > 1:
            names = ", ".join(sorted(str(a.get("name")) for f in hits for a in f))
            raise ReleaseError(f"{tag} has several assets for {label}: {names}")
        a = hits[0][0]
        # … the existing name/state/size/url checks, unchanged …
        return name, url, size
```

`sha256sums_url` calls `asset((r"SHA256SUMS",), "SHA256SUMS")[1]`; the loop:

```python
    for label, (required, patterns) in DOWNLOADS.items():
        hit = asset(patterns, label, required)
        if hit is None:
            continue
        name, url, size = hit
        …
```

Add the block pass:

```python
def asset_blocks(text: str, vals: dict[str, str], where: str) -> str:
    """Keeps <!-- asset:<slot> --> blocks when that optional download is in
    the release and <!-- noasset:<slot> --> blocks when it is not (both
    without their comments). Before the first release no slot resolves."""
    out: list[str] = []
    pos = 0
    open_: tuple[bool, str] | None = None
    for m in ASSET_BLOCK.finditer(text):
        neg, slot, edge = bool(m.group(1)), m.group(2), m.group(3)
        if slot not in DOWNLOADS or DOWNLOADS[slot][0]:
            raise ReleaseError(f"{where}: {m.group(0)} names no optional download")
        chunk = text[pos:m.start()]
        if edge == "begin":
            if open_ is not None:
                raise ReleaseError(f"{where}: {m.group(0)} inside another asset block")
            out.append(chunk)
            open_ = (neg, slot)
        else:
            if open_ != (neg, slot):
                raise ReleaseError(f"{where}: {m.group(0)} without its begin")
            if (f"{slot}.url" in vals) != neg:
                out.append(chunk)
            open_ = None
        pos = m.end()
    if open_ is not None:
        raise ReleaseError(f"{where}: {'no' if open_[0] else ''}asset:{open_[1]}:begin is never closed")
    out.append(text[pos:])
    return "".join(out)
```

In `apply()`, before `blocks(...)`:

```python
        if ASSET_BLOCK.search(text) and path.suffix != ".html":
            raise ReleaseError(f"{rel}: asset blocks are only allowed in .html files")
        new = asset_blocks(text, vals, rel)
        new = blocks(new, release is not None, rel)
```

The unused check and `main()`'s print loop iterate only over resolved slots (`if f"{label}.url" in vals`). Update the module docstring: the new block kinds, the optional slots, "both schemes in one release fail (ambiguous)", and that `{{release.<slot>.*}}` for an optional slot must sit in an `asset:<slot>` block.

Also add one Python-level negative check to `check_site.py`'s `gate_release` (or a tiny `scripts/site/apply_release.test.mjs` if the repo prefers node; use whichever style the repo already has for apply_release negatives: `grep -n "ambiguous\|several assets" scripts/site/*.py scripts/site/*.mjs`): build with a fixture made in a temp dir from `release-new.json` plus the legacy `Coverly.ROMs_9.8.7_universal.dmg` asset and assert `apply_release.py` exits 1 with `several assets for macos_arm`.

- [ ] **Step 5: Run and watch it pass (site markup still old: expect the `release-new` layout check to fail until Task 9)**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing && python3 -I scripts/site/check_site.py 2>&1 | tail -10
```

Expected: `release` and `no-release` variants pass except where the HTML still says `macos_universal` (unknown marker). Since markup and script must change together for a green `make check`, do **not** commit yet: continue with Task 9 Step 1–3 and commit both in Task 9 Step 5. (If the task is split across agents, commit this step with `make check` red is not allowed; keep the work in the tree and hand Task 9 to the same agent.)

---

## Task 9: Landing: cards, CLI list, `llms.txt`, `release.js` `offer()`, tests

**Repository:** `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing`, branch `feat/release-names`

**Files:**
- Modify: `site/index.html`, `site/en/index.html` (JSON-LD `downloadUrl`, macOS and Windows cards, CLI list), `site/llms.txt`
- Modify: `site/assets/release.js` (`offer()`)
- Modify: `scripts/site/release.test.mjs`

- [ ] **Step 1: Markup (es and en in lockstep)**

JSON-LD (both pages): `{{release.macos_universal.url}}` → `{{release.macos_arm.url}}` (required slots only; spec §5.4).

Spanish macOS card (replace line 355):

```html
<div class="dl-card" data-os="mac"><span class="mine">Tu equipo</span><h3>macOS</h3><!-- noasset:macos_intel:begin --><p class="req">macOS 11 o posterior, Apple silicon e Intel</p><a class="btn btn-primary" data-asset href="{{release.macos_arm.url}}">Descargar .dmg ({{release.macos_arm.mib_es}}&nbsp;MiB)</a><!-- noasset:macos_intel:end --><!-- asset:macos_intel:begin --><p class="req">macOS 11 o posterior</p><a class="btn btn-primary" data-asset href="{{release.macos_arm.url}}">Descargar para Apple silicon (.dmg, {{release.macos_arm.mib_es}}&nbsp;MiB)</a><a class="alt" data-asset href="{{release.macos_intel.url}}">O para Mac con Intel (.dmg, {{release.macos_intel.mib_es}}&nbsp;MiB)</a><!-- asset:macos_intel:end --></div>
```

Spanish Windows card. The legacy block keeps today's text exactly (spec §6), `.msi` link included; the new block names the processor in every link. `windows_arm_msi` is its own optional slot with its own block, so the "download linked nowhere" check holds per slot:

```html
<div class="dl-card" data-os="win"><span class="mine">Tu equipo</span><h3>Windows</h3><!-- noasset:windows_arm:begin --><p class="req">Windows 10 y 11, x64</p><a class="btn btn-primary" data-asset href="{{release.windows_x64.url}}">Descargar instalador ({{release.windows_x64.mib_es}}&nbsp;MiB)</a><a class="alt" data-asset href="{{release.windows_x64_msi.url}}">O el .msi ({{release.windows_x64_msi.mib_es}}&nbsp;MiB)</a><!-- noasset:windows_arm:end --><!-- asset:windows_arm:begin --><p class="req">Windows 10 y 11 (x64), Windows 11 (ARM64)</p><a class="btn btn-primary" data-asset href="{{release.windows_x64.url}}">Descargar instalador (x64, {{release.windows_x64.mib_es}}&nbsp;MiB)</a><a class="alt" data-asset href="{{release.windows_x64_msi.url}}">O el .msi (x64, {{release.windows_x64_msi.mib_es}}&nbsp;MiB)</a><a class="alt" data-asset href="{{release.windows_arm.url}}">O el instalador para ARM64 ({{release.windows_arm.mib_es}}&nbsp;MiB)</a><!-- asset:windows_arm:end --><!-- asset:windows_arm_msi:begin --><a class="alt" data-asset href="{{release.windows_arm_msi.url}}">O el .msi para ARM64 ({{release.windows_arm_msi.mib_es}}&nbsp;MiB)</a><!-- asset:windows_arm_msi:end --></div>
```

(`windows_x64_msi` is linked in both blocks; only one of them survives a build, and the marker check counts it as used either way.)

CLI list: add after Windows x64:

```html
<!-- asset:cli_windows_arm64:begin --><li><a data-asset href="{{release.cli_windows_arm64.url}}">Windows ARM64</a></li><!-- asset:cli_windows_arm64:end -->
```

English: the same structure with spec §7 labels: "macOS 11 or later, Apple silicon and Intel" / "Download .dmg (…)" (legacy, unchanged); "macOS 11 or later" / "Download for Apple silicon (.dmg, N&nbsp;MiB)" / "Or for an Intel Mac (.dmg, N&nbsp;MiB)"; "Windows 10 and 11, x64" (legacy) / "Windows 10 and 11 (x64), Windows 11 (ARM64)"; "Download installer (x64, N&nbsp;MiB)"; "Or the .msi (x64, N&nbsp;MiB)"; "Or the ARM64 installer (N&nbsp;MiB)"; "Or the ARM64 .msi (N&nbsp;MiB)"; CLI "Windows ARM64". Copy the exact legacy English text from the current line 355–356 of `site/en/index.html` into the `noasset` blocks.

`site/llms.txt` (blocks are not allowed there): line 18 becomes `- macOS 11 or later, Apple silicon (newest release) or universal (releases up to 0.2.0): {{release.macos_arm.url}} (about {{release.macos_arm.mib_en}} MiB). Intel Mac and Windows ARM64 downloads, when the release has them, are listed on the release page: {{release.notes_url}}` and the macOS lines keep only required slots.

- [ ] **Step 2: `release.js` `offer()`**

Replace the `end`/`name` lines:

```js
    var ends = { mac: ["_mac_arm.dmg", "_universal.dmg"], win: ["_windows_x64-setup.exe", "_x64-setup.exe"],
      linux: arm ? ["_linux_arm.AppImage", "_aarch64.AppImage"] : ["_linux_x64.AppImage", "_amd64.AppImage"] }[os];
    var prefix = ["Coverly-ROMs_", "Coverly.ROMs_"], name, f, i;
    for (i = 0; ends && i < 2 && !f; i++) { name = prefix[i] + v + ends[i]; f = files[name]; }
    var url = BASE + "download/" + tag + "/" + name;
```

and drop the old `var end`/`name`/`f` declarations (keep the rest; the loop variable `i` is reused below, so rename the later `var btn = …, part, i;` to not redeclare: `var btn = all("a[data-rel-file]"), part;`). Update the comment above `offer()`: "by its exact name, the release name first and the name releases up to 0.2.0 used second". `apply()` does not change (D9). Check the size budget: `python3 -I scripts/site/check_site.py --only release -v | grep release.js`.

- [ ] **Step 3: `release.test.mjs`**

- `filled(release, lang)` takes a fixture name (`'release' | 'release-new' | 'release-new-no-arm' | null`); `FIXTURES` maps names to parsed JSON; `release(version, edit, fixture = 'release')`.
- Keep every existing test on the legacy fixture (they prove v0.2.0 pages still upgrade within their scheme).
- Add:

```js
for (const lang of ['es', 'en']) {
  test(`${lang}: a new-name page upgrades to a newer new-name release`, async () => {
    const html = filled('release-new', lang);
    const { links } = await run(html, { status: 200, body: release('9.9.0', (r) => r, 'release-new') });
    const hrefs = links.map((a) => a.attrs.href);
    assert.ok(hrefs.includes(`${BASE}download/v9.9.0/Coverly-ROMs_9.9.0_mac_arm.dmg`));
    assert.ok(hrefs.includes(`${BASE}download/v9.9.0/Coverly-ROMs_9.9.0_mac_intel.dmg`));
    assert.ok(hrefs.includes(`${BASE}download/v9.9.0/Coverly-ROMs_9.9.0_windows_arm-setup.exe`));
    assert.ok(hrefs.includes(`${BASE}download/v9.9.0/coverly_9.9.0_windows_arm.exe`));
    for (const h of hrefs) assert.ok(!h.includes('9.8.7'), h);
  });

  test(`${lang}: an old-name page is left alone by a new-name release`, async () => {
    const html = filled('release', lang);
    const before = dom(html).els.map((e) => [e.attrs.href, e.textContent]);
    const { page } = await run(html, { status: 200, body: release('9.9.0', (r) => r, 'release-new') });
    assert.deepEqual(page.els.map((e) => [e.attrs.href, e.textContent]), before);
  });

  test(`${lang}: the new layout drops only the ARM64 links when they are missing`, async () => {
    const html = filled('release-new-no-arm', lang);
    assert.ok(html.includes('_mac_intel.dmg'));
    assert.ok(!html.includes('_windows_arm'));
    assert.ok(!html.includes('asset:'));
  });

  test(`${lang}: the legacy layout renders today's macOS and Windows cards`, async () => {
    const html = filled('release', lang);
    assert.ok(html.includes('_universal.dmg'));
    assert.ok(!html.includes('_mac_intel') && !html.includes('_windows_arm') && !html.includes('asset:'));
    assert.ok(html.includes(lang === 'es' ? 'Apple silicon e Intel' : 'Apple silicon and Intel'));
  });
}
```

- In `before the first release, the installer for this system is offered`, run the case table twice: once against `release('0.1.0')` (legacy names, as today) and once against `release('0.1.0', (r) => r, 'release-new')` with the new file names (`Coverly-ROMs_0.1.0_mac_arm.dmg`, `…_windows_x64-setup.exe`, `…_linux_x64.AppImage`, `…_linux_arm.AppImage`; sizes from the new fixture + 1 MiB).
- In the "one file missing" test add an edit for the new fixture: drop `_windows_arm-setup.exe` from a new-name 9.9.0 release against a new-name page → nothing changes.

- [ ] **Step 4: Run every landing gate**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing && make check 2>&1 | tail -15
```

Expected: tokens, covers, `node --test` (all tests pass), `check_site.py` prints four variants each with `0 failure(s)`.

- [ ] **Step 5: Commit (granular) and push after each**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing && git add scripts/site/fixtures/release-new.json scripts/site/fixtures/release-new-no-arm.json && git commit -m "test(site): add release fixtures with the new file names" && git push
git add scripts/site/apply_release.py scripts/site/check_site.py site/index.html site/en/index.html site/llms.txt && git commit -m "feat(site): link per-chip macOS and Windows ARM64 downloads, old names still accepted" && git push
git add site/assets/release.js scripts/site/release.test.mjs && git commit -m "feat(site): offer the new installer names before the first release" && git push
git status -sb
```

(`apply_release.py` and the markup go in one commit because neither builds without the other.)

---

## Task 10: Landing: README, PR, CI, merge

**Repository:** `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing`

- [ ] **Step 1: README**

In the release section of `README.md`: the landing reads both the release names (`Coverly-ROMs_<v>_mac_arm.dmg`, …) and the names releases up to v0.2.0 used; Intel and Windows ARM64 links appear only when the release has them; `release.js` never mixes the two schemes (the deploy hook rebuilds the page); and "merge changes to the expected names here before the app publishes a release that uses them (otherwise the Vercel build fails and keeps serving the previous deployment)". Preview commands list the three fixtures (`make site RELEASE=scripts/site/fixtures/release-new.json`).

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing && make check 2>&1 | tail -4 && git add README.md && git commit -m "docs: explain both release name schemes and the merge order" && git push && git status -sb
```

- [ ] **Step 2: Open the PR in English (no AI attribution in the body)**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing && gh pr create --base main --head feat/release-names --title "feat(site): accept the new release file names and Windows ARM64" --body "$(cat <<'EOF'
## Summary
- Resolve every download with the new release names (`Coverly-ROMs_<v>_mac_arm.dmg`, `coverly_<v>_linux_x64`, ...) first and the names used up to v0.2.0 second, so the live v0.2.0 release and the next one both build.
- Split the macOS card (Apple silicon button, Intel link) and add Windows ARM64 links when the release has them, through `asset`/`noasset` blocks; a v0.2.0 build renders today's cards unchanged.
- `release.js` keeps upgrading only within one naming scheme; `offer()` tries the new name, then the old one.

## Merge order
This must merge before the app publishes its first release with the new names (MAECLY/coverly-roms spec 2026-10-26-release-names, D11).

## Tests
- `make check`: four builds (old names, new names, new names without ARM64, no release), `release.test.mjs` with cross-scheme cases.
EOF
)"
```

- [ ] **Step 3: Wait for green CI, then merge**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing && gh pr checks --watch --interval 30; gh pr view --json state,mergeStateStatus,statusCheckRollup
```

If the `Check` workflow passes: `gh pr merge --merge --delete-branch=false` (keep the remote branch until Task 11 is done), then `git switch main && git pull --ff-only && git status -sb`.

If Actions does not start or fails for billing (the owner's account has had Actions blocked by billing before), do not merge blind: run `make check` locally once more on the PR head, post its output as a PR comment (`gh pr comment --body-file …`), and then merge, as the owner's rule for that case says. Any real test failure is fixed on the branch first (commit + push), never bypassed.

- [ ] **Step 4: Confirm the production build still serves v0.2.0 correctly**

The merge triggers a Vercel deploy of `main` against the live v0.2.0 release (old names). Check it built and the page still shows the universal button:

```bash
curl -fsS https://coverly-roms.maecly.com/ | grep -oE 'Coverly[.-]ROMs_[0-9.]+_[a-z_0-9]+\.dmg' | sort -u
```

Expected: `Coverly.ROMs_0.2.0_universal.dmg` (the live release). If the deploy failed, read its logs with the Vercel MCP (`list_deployments` for project `coverly-roms-landing`, then the build log) and fix forward on a new branch; never touch the app release.

---

## Task 11: Real verification and E2E (app branch and landing)

**Files:** none new; evidence goes into the app PR description or a PR comment.

- [ ] **Step 1: Update with `main`**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && git fetch origin && git merge origin/main && git push && git status -sb
```

Resolve conflicts if any (expected only in files other workflows touch; this branch does not edit `locales/*`, `src/components/*` or `TODO.md`).

- [ ] **Step 2: All repository gates, from clean**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && pnpm install --frozen-lockfile && pnpm check-all && pnpm build && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast 2>&1 | grep -E "test result|FAILED" && actionlint .github/workflows/*.yml && python3 -I scripts/release/release_tools.py --help > /dev/null && echo ALL-GATES-OK
```

Expected: every `test result: ok`, no `FAILED`, `ALL-GATES-OK`.

- [ ] **Step 3: Staging dry run (Task 5) on the final tree**

Re-run Task 5 Step 2 and keep `dry-stable.txt` and `dry-beta.txt` as evidence. Compare each name with spec §4 (12 + 6 stable, 10 + 6 beta; 22 / 20 manifest keys; `windows-aarch64*` present; no `universal`).

- [ ] **Step 4: Landing E2E in a browser (port 1461)**

For each fixture (`release.json`, `release-new.json`, `release-new-no-arm.json`):

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing && lsof -ti tcp:1461 -sTCP:LISTEN | xargs -r kill; make site SITE_PORT=1461 RELEASE=scripts/site/fixtures/release-new.json
```

(run with `run_in_background`, then stop it by port before the next fixture). With Chrome DevTools MCP in `isolatedContext: 'release-names'` (fallbacks per Global Constraints):

1. `http://localhost:1461/#descargar` and `http://localhost:1461/en/#download`, viewport 1280×900 and 390×844: screenshot the download section.
2. Emulate user agents: macOS Safari (`Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15`), Windows Edge, Linux aarch64 Chrome; check the "Tu equipo"/"Your computer" highlight lands on the right card and that the macOS card (new fixture) shows the Apple silicon primary button with the Intel `.alt` link; the Windows card shows x64 primary, `.msi` and the ARM64 links.
3. `evaluate_script`: list `[...document.querySelectorAll('a[data-asset]')].map(a => a.href)` and check every href ends with a name present in that fixture (no `asset:` text in `document.documentElement.outerHTML`).
4. Legacy fixture: the macOS card reads «macOS 11 o posterior, Apple silicon e Intel» with one «Descargar .dmg» button, and there is no Intel or ARM64 link.
5. Once with `emulate` reduced motion: nothing hides or animates differently in the download section.
6. Keyboard: Tab through the macOS and Windows cards; every link has a visible focus ring and a self-describing name (check the accessibility tree via `take_snapshot`).
7. Console: no errors (`list_console_messages`).

Stop the server: `lsof -ti tcp:1461 -sTCP:LISTEN | xargs -r kill`. Delete any scratch files except the evidence `.txt` and screenshots in the scratchpad.

- [ ] **Step 5: Final state of both repositories**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-release-names && git status -sb && git log --oneline origin/main..HEAD
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing && git status -sb
```

Expected: both show nothing ahead of `origin`, no untracked files. Report: the app branch's commit list, the landing PR URL and merge commit, the dry-run listings, the screenshots, and the owner steps left for after the next real release (spec §10 "Owner": real 0.2.0 → next update on each OS, Windows 11 ARM installer/WebView2/x64 replacement, Intel Mac first launch). Do not edit `TODO.md` or `docs/TRACKER.md` here; they are updated on `main` after the merge.

## Self-review

- Spec coverage: §4 names → Tasks 1, 2, 3 (exact listings) and 5; §5.1 → Tasks 1–2; §5.2 → Task 4; §5.3 tests that flip → Tasks 1, 2, 4, 6; docs → Tasks 6–7; §5.4 landing → Tasks 8–10; §6 UX/a11y → Task 9 markup + Task 11 E2E; §7 copy → Tasks 6, 9; §9 edge cases: prerelease (Task 1 beta), another platform's file / two `.msi` / symlink / leftover universal artifact (Task 1), stray old name (Task 2), runner tools (Task 4), landing order (Task 10 Step 4, README); §10 test plan → Tasks 1–5, 8–9, 11.
- Owner decisions D1 (`_mac_arm`), D2 (two macOS builds), D6 (Windows ARM64 app + CLI, `windows-aarch64`) are in the header and enforced by tests (Task 4 slugs, Task 3 keys).
- No placeholders: every new test and function is written out; the two places that say "check before writing" (Task 3 signatures, Task 8 negative test style) name the exact file to read.
- Order: Tasks 1–4 keep the suite green per commit (an `#[ignore]` bridges at most one commit and Task 4 Step 4 checks none is left); Tasks 8–9 commit only with `make check` green.
