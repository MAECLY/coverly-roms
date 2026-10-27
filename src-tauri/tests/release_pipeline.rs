//! Release invariants (M7): one version source, the bump script, and (Task 10)
//! the release workflows and their Python helper.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn workspace_version(cargo_toml: &str) -> String {
    let v: toml::Value = toml::from_str(cargo_toml).unwrap();
    v["workspace"]["package"]["version"]
        .as_str()
        .unwrap()
        .to_string()
}

fn lock_version(lock: &str, name: &str) -> Option<String> {
    let v: toml::Value = toml::from_str(lock).unwrap();
    v["package"].as_array()?.iter().find_map(|p| {
        (p["name"].as_str() == Some(name)).then(|| p["version"].as_str().unwrap().to_string())
    })
}

#[test]
fn version_sources_agree() {
    let ws = workspace_version(&read("Cargo.toml"));
    assert_eq!(
        ws,
        env!("CARGO_PKG_VERSION"),
        "src-tauri inherits the workspace version"
    );
    let pkg: serde_json::Value = serde_json::from_str(&read("package.json")).unwrap();
    assert_eq!(
        pkg["version"],
        ws.as_str(),
        "package.json: run scripts/bump-version.sh"
    );
    let conf: serde_json::Value = serde_json::from_str(&read("src-tauri/tauri.conf.json")).unwrap();
    assert!(
        conf.get("version").is_none(),
        "tauri.conf.json must not pin a version: Cargo.toml is the only source"
    );
    for manifest in [
        "crates/coverly-core/Cargo.toml",
        "crates/coverly-cli/Cargo.toml",
        "src-tauri/Cargo.toml",
    ] {
        assert!(
            read(manifest).contains("version.workspace = true"),
            "{manifest}"
        );
    }
    let lock = read("Cargo.lock");
    for name in ["coverly-core", "coverly-cli", "coverly-roms"] {
        assert_eq!(
            lock_version(&lock, name).as_deref(),
            Some(ws.as_str()),
            "Cargo.lock {name}"
        );
    }
}

#[test]
fn changelog_follows_keep_a_changelog() {
    let log = read("CHANGELOG.md");
    assert!(log.starts_with("# Changelog"));
    assert!(log.contains("https://keepachangelog.com/en/1.1.0/"));
    assert!(log.contains("## [Unreleased]"));
}

/// The script tests run `bash` and `python3`: never on Windows (its `bash`
/// may be WSL's, which cannot see this checkout), and only where bash finds
/// a python3.
fn scripts_runnable() -> bool {
    !cfg!(windows)
        && Command::new("bash")
            .args(["-c", "command -v python3"])
            .output()
            .is_ok_and(|o| o.status.success())
}

fn bash() -> Option<&'static str> {
    scripts_runnable().then_some("bash")
}

/// A scratch checkout with just the files the script edits.
fn scratch_checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for rel in ["Cargo.toml", "package.json", "Cargo.lock"] {
        std::fs::copy(repo().join(rel), dir.path().join(rel)).unwrap();
    }
    dir
}

#[test]
fn bump_version_writes_every_source_and_check_agrees() {
    let Some(bash) = bash() else {
        eprintln!("no bash with python3 here (or Windows); bump-version.sh not exercised");
        return;
    };
    let dir = scratch_checkout();
    let script = repo().join("scripts/bump-version.sh");
    let ok = Command::new(bash)
        .arg(&script)
        .args(["--root"])
        .arg(dir.path())
        .arg("1.2.3-beta.4")
        .status()
        .unwrap();
    assert!(ok.success());
    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert_eq!(workspace_version(&cargo), "1.2.3-beta.4");
    let pkg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("package.json")).unwrap())
            .unwrap();
    assert_eq!(pkg["version"], "1.2.3-beta.4");
    let lock = std::fs::read_to_string(dir.path().join("Cargo.lock")).unwrap();
    for name in ["coverly-core", "coverly-cli", "coverly-roms"] {
        assert_eq!(
            lock_version(&lock, name).as_deref(),
            Some("1.2.3-beta.4"),
            "{name}"
        );
    }
    let check = Command::new(bash)
        .arg(&script)
        .args(["--root"])
        .arg(dir.path())
        .arg("--check")
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn bump_version_refuses_a_non_semver_and_check_catches_drift() {
    let Some(bash) = bash() else { return };
    let dir = scratch_checkout();
    let script = repo().join("scripts/bump-version.sh");
    for bad in ["v1.2.3", "1.2", "1.2.3+build"] {
        let st = Command::new(bash)
            .arg(&script)
            .arg("--root")
            .arg(dir.path())
            .arg(bad)
            .status()
            .unwrap();
        assert_eq!(st.code(), Some(2), "{bad}");
    }
    let pkg_path = dir.path().join("package.json");
    let text = std::fs::read_to_string(&pkg_path).unwrap();
    std::fs::write(
        &pkg_path,
        text.replacen("\"version\": \"", "\"version\": \"9", 1),
    )
    .unwrap();
    let check = Command::new(bash)
        .arg(&script)
        .arg("--root")
        .arg(dir.path())
        .arg("--check")
        .output()
        .unwrap();
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stderr).contains("disagree"));
}

fn python() -> Option<&'static str> {
    scripts_runnable().then_some("python3")
}

fn tool(args: &[&str]) -> std::process::Output {
    Command::new(python().unwrap())
        .arg("-I")
        .arg(repo().join("scripts/release/release_tools.py"))
        .args(args)
        .output()
        .unwrap()
}

/// A job key line: exactly two spaces of indent, then `name:`.
fn is_job_key(line: &str) -> bool {
    line.starts_with("  ")
        && !line.starts_with("   ")
        && line.trim_end().ends_with(':')
        && !line.trim_start().starts_with('#')
}

/// The text of job `name` in a workflow (its key line up to the next job key).
fn job(workflow: &str, name: &str) -> String {
    let key = format!("  {name}:");
    let mut lines = workflow.lines().skip_while(|l| l.trim_end() != key);
    let first = lines.next().unwrap_or_else(|| panic!("no job {name}"));
    let mut out = vec![first];
    out.extend(lines.take_while(|l| !is_job_key(l)));
    out.join("\n")
}

/// Every job key of a workflow, in order.
fn job_names(workflow: &str) -> Vec<String> {
    workflow
        .lines()
        .skip_while(|l| l.trim_end() != "jobs:")
        .skip(1)
        .filter(|l| is_job_key(l))
        .map(|l| l.trim().trim_end_matches(':').to_string())
        .collect()
}

#[test]
fn only_draft_and_finalize_can_write_and_neither_builds_third_party_code() {
    let wf = read(".github/workflows/release.yml");
    let names = job_names(&wf);
    assert_eq!(names, ["guard", "draft", "app", "cli", "finalize"]);
    for name in &names {
        let j = job(&wf, name);
        let writes = j.contains("contents: write");
        assert_eq!(
            writes,
            name == "draft" || name == "finalize",
            "{name}: only draft and finalize hold contents: write"
        );
        if !writes {
            assert!(j.contains("contents: read"), "{name}");
        }
    }
    // The jobs that compile (crates, build scripts, npm packages) are
    // read-only and hand their files over as workflow artifacts.
    let app = job(&wf, "app");
    assert!(app.contains(
        "pnpm tauri build $ARGS $bundles --config \"$RUNNER_TEMP/updater-overlay.json\""
    ));
    assert!(app.contains("actions/upload-artifact@"));
    assert!(app.contains("name: app-${{ matrix.slug }}"));
    assert!(
        !wf.contains("tauri-action"),
        "no job uploads to the draft while it builds"
    );
    assert!(!wf.contains("releaseId") && !wf.contains("tagName"));
    assert!(job(&wf, "cli").contains("actions/upload-artifact@"));
    // draft only runs gh; finalize compiles nothing and uploads.
    let draft = job(&wf, "draft");
    for needle in ["pnpm", "cargo", "npm", "rustup", "setup-node", "tauri"] {
        assert!(!draft.contains(needle), "draft runs {needle}");
    }
    let finalize = job(&wf, "finalize");
    for needle in [
        "cargo",
        "tauri build",
        "rustup",
        "apt-get install -y --no-install-recommends",
    ] {
        assert!(!finalize.contains(needle), "finalize runs {needle}");
    }
    assert!(finalize.contains("actions/download-artifact@") && finalize.contains("stage-builds"));
    assert!(finalize.contains("updater-manifest") && finalize.contains("add-cli"));
    assert!(finalize.contains("gh release upload"));
    assert!(wf.contains("universal-apple-darwin"));
    assert!(wf.contains("ubuntu-22.04-arm"));
}

/// The `run: |` script of the step named `step` in `job_text`, dedented.
fn step_script(job_text: &str, step: &str) -> String {
    let mut lines = job_text
        .lines()
        .skip_while(|l| !l.contains(&format!("name: {step}")))
        .skip_while(|l| l.trim() != "run: |")
        .skip(1)
        .peekable();
    let indent = lines
        .peek()
        .map(|l| l.len() - l.trim_start().len())
        .expect("a run block");
    lines
        .take_while(|l| l.trim().is_empty() || l.len() - l.trim_start().len() >= indent)
        .map(|l| l.get(indent..).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_app_job_stages_bundles_that_finalize_can_map() {
    if !scripts_runnable() {
        return;
    }
    let script = step_script(
        &job(&read(".github/workflows/release.yml"), "app"),
        "Stage the bundles under their release names",
    );
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    // What `tauri build` leaves on the four runners (merged here).
    for (rel, body) in [
        ("target/universal-apple-darwin/release/bundle/macos/Coverly ROMs.app.tar.gz", "mac"),
        ("target/universal-apple-darwin/release/bundle/macos/Coverly ROMs.app.tar.gz.sig", "throwaway"),
        ("target/universal-apple-darwin/release/bundle/macos/Coverly ROMs.app/Contents/Info.plist", "x"),
        ("target/universal-apple-darwin/release/bundle/dmg/Coverly ROMs_0.2.0_universal.dmg", "dmg"),
        ("target/release/bundle/nsis/Coverly ROMs_0.2.0_x64-setup.exe", "nsis"),
        ("target/release/bundle/nsis/Coverly ROMs_0.2.0_x64-setup.exe.sig", "throwaway"),
        ("target/release/bundle/msi/Coverly ROMs_0.2.0_x64_en-US.msi", "msi"),
        ("target/release/bundle/appimage/Coverly ROMs_0.2.0_amd64.AppImage", "appimage"),
        ("target/release/bundle/deb/Coverly ROMs_0.2.0_amd64.deb", "deb"),
    ] {
        let p = d.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    let out = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .current_dir(d)
        .env("GITHUB_REF_NAME", "v0.2.0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut staged: Vec<String> = std::fs::read_dir(d.join("staged"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    staged.sort();
    assert_eq!(
        staged,
        [
            "Coverly.ROMs_0.2.0_amd64.AppImage",
            "Coverly.ROMs_0.2.0_amd64.deb",
            "Coverly.ROMs_0.2.0_universal.app.tar.gz",
            "Coverly.ROMs_0.2.0_universal.dmg",
            "Coverly.ROMs_0.2.0_x64-setup.exe",
            "Coverly.ROMs_0.2.0_x64_en-US.msi",
        ],
        "no throwaway .sig, no spaces, the macOS archive named with version and arch"
    );
    assert_eq!(
        updater_artifacts(&d.join("staged")),
        [
            "Coverly.ROMs_0.2.0_amd64.AppImage",
            "Coverly.ROMs_0.2.0_amd64.deb",
            "Coverly.ROMs_0.2.0_universal.app.tar.gz",
            "Coverly.ROMs_0.2.0_x64-setup.exe",
            "Coverly.ROMs_0.2.0_x64_en-US.msi",
        ]
    );
}

#[test]
fn every_action_is_pinned_and_checkouts_keep_no_credentials() {
    for name in ["release.yml", "release-channels.yml", "ci.yml"] {
        let wf = read(&format!(".github/workflows/{name}"));
        for line in wf.lines().filter(|l| {
            l.trim_start().starts_with("- uses:") || l.trim_start().starts_with("uses:")
        }) {
            let r = line.split("uses:").nth(1).unwrap().trim();
            let sha = r
                .split('@')
                .nth(1)
                .unwrap_or("")
                .split_whitespace()
                .next()
                .unwrap_or("");
            assert!(
                sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
                "{name}: {line}"
            );
        }
        assert_eq!(
            wf.matches("actions/checkout@").count(),
            wf.matches("persist-credentials: false").count(),
            "{name}"
        );
    }
}

#[test]
fn the_real_signing_key_reaches_only_the_finalize_sign_step() {
    let wf = read(".github/workflows/release.yml");
    assert_eq!(
        wf.matches("secrets.TAURI_SIGNING_PRIVATE_KEY }}").count(),
        1,
        "only the finalize sign step (guard checks `!= ''`, not the value)"
    );
    let finalize = job(&wf, "finalize");
    assert!(finalize.contains("secrets.TAURI_SIGNING_PRIVATE_KEY }}"));
    assert!(finalize.contains("secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}"));
    assert!(!finalize.contains("tauri build") && !finalize.contains("cargo build"));
    for name in ["app", "cli"] {
        assert!(!job(&wf, name).contains("secrets.TAURI_SIGNING"), "{name}");
    }
    assert!(
        job(&wf, "app").contains("signer generate"),
        "the build signs with a throwaway key"
    );
    assert!(
        job(&wf, "app").contains("--config \"$RUNNER_TEMP/updater-overlay.json\""),
        "tauri build decodes plugins.updater.pubkey: overlay the throwaway public key"
    );
    assert!(wf.contains("permissions: {}"));
}

#[test]
fn guard_refuses_what_the_updater_would_reject() {
    let guard = job(&read(".github/workflows/release.yml"), "guard");
    for needle in [
        "TAURI_SIGNING_PRIVATE_KEY",
        "bump-version.sh",
        "CHANGELOG.md",
        "updater.pub",
        "private",
    ] {
        assert!(guard.contains(needle), "guard does not check {needle}");
    }
}

#[test]
fn finalize_requires_every_platform_and_writes_sha256sums() {
    let finalize = job(&read(".github/workflows/release.yml"), "finalize");
    for key in [
        "darwin-aarch64",
        "darwin-x86_64",
        "windows-x86_64",
        "linux-x86_64",
        "linux-aarch64",
        "cli-darwin-aarch64",
        "cli-darwin-x86_64",
        "cli-windows-x86_64",
        "cli-linux-x86_64",
        "cli-linux-aarch64",
    ] {
        assert!(finalize.contains(&format!("--require {key}")), "{key}");
    }
    assert!(finalize.contains("--app-version"));
    assert!(finalize.contains("--signed-version"));
    assert!(finalize.contains("SHA256SUMS"));
    assert!(
        finalize.contains("cp bundles/* cli/* all/"),
        "SHA256SUMS covers every asset, the .dmg, .msi and .deb included"
    );
    for kind in ["dmg", "msi", "deb", "AppImage"] {
        assert!(finalize.contains(&format!("'\\.{kind}'")), "{kind}");
    }
    assert!(finalize.contains("minisign -V"));
    assert!(finalize.contains("tampered"));
}

#[test]
fn the_beta_channel_tag_is_the_one_the_app_reads() {
    let wf = read(".github/workflows/release-channels.yml");
    assert!(wf.contains(&format!("BETA_TAG: {}", coverly_core::update::BETA_TAG)));
    assert!(wf.contains("semver-gt"));
}

#[test]
fn bundle_targets_and_macos_signing() {
    let conf: serde_json::Value = serde_json::from_str(&read("src-tauri/tauri.conf.json")).unwrap();
    let targets: Vec<&str> = conf["bundle"]["targets"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t.as_str())
        .collect();
    for t in ["app", "dmg", "nsis", "msi", "appimage", "deb"] {
        assert!(targets.contains(&t), "{t}");
    }
    assert_eq!(
        conf["bundle"]["macOS"]["signingIdentity"], "-",
        "ad hoc: universal apps must run on Apple silicon"
    );
}

#[test]
fn release_notes_explain_gatekeeper_and_smartscreen_in_en_and_es() {
    let notes = read(".github/release-notes.md");
    for needle in [
        "Open Anyway",
        "Abrir igualmente",
        "Run anyway",
        "Ejecutar de todas formas",
        "SHA256SUMS",
        "<version>",
        "<versión>",
        "## English",
        "## Español",
        // The app bundle's name has a space: the command only works quoted.
        "xattr -rd com.apple.quarantine \"/Applications/Coverly ROMs.app\"",
        "«Windows protegió su PC»",
        "libwebkit2gtk-4.1-0",
        // GitHub's names for the uploaded files (release.yml stages them so).
        "Coverly.ROMs_<version>_universal.dmg",
        "Coverly.ROMs_<versión>_x64-setup.exe",
    ] {
        assert!(notes.contains(needle), "{needle}");
    }
    assert!(!notes.contains("Coverly ROMs_<"), "use the uploaded names");
    assert!(!notes.contains('!'), "brand voice: no exclamation marks");
}

#[test]
fn release_tools_build_and_check_a_signed_manifest() {
    if python().is_none() {
        eprintln!("no python; release_tools.py not exercised");
        return;
    }
    use coverly_core::update::testing::TestKey;
    let key = TestKey::generate();
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    // tauri-action's manifest with throwaway signatures.
    let manifest = serde_json::json!({
        "version": "0.2.0", "notes": "n", "pub_date": "2026-10-16T00:00:00Z",
        "platforms": { "darwin-aarch64": { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.2.0/Coverly%20ROMs.app.tar.gz", "signature": "throwaway" } }
    });
    std::fs::write(d.join("latest.json"), manifest.to_string()).unwrap();
    let out = tool(&[
        "list-artifacts",
        "--manifest",
        d.join("latest.json").to_str().unwrap(),
    ]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "Coverly ROMs.app.tar.gz"
    );
    // Re-signed with the "real" key, bound to the version.
    let sigdir = d.join("sigs");
    std::fs::create_dir(&sigdir).unwrap();
    std::fs::write(
        sigdir.join("Coverly ROMs.app.tar.gz.sig"),
        key.sign(b"app", "app", Some("0.2.0")),
    )
    .unwrap();
    assert!(tool(&[
        "resign-manifest",
        "--manifest",
        d.join("latest.json").to_str().unwrap(),
        "--sigdir",
        sigdir.to_str().unwrap()
    ])
    .status
    .success());
    // CLI entries.
    let cli = d.join("cli");
    std::fs::create_dir(&cli).unwrap();
    for (name, body) in [
        ("coverly_0.2.0_mac_arm", "a"),
        ("coverly_0.2.0_windows_x64.exe", "w"),
    ] {
        std::fs::write(cli.join(name), body).unwrap();
        std::fs::write(
            cli.join(format!("{name}.sig")),
            key.sign(body.as_bytes(), name, Some("0.2.0")),
        )
        .unwrap();
    }
    let add = tool(&[
        "add-cli",
        "--manifest",
        d.join("latest.json").to_str().unwrap(),
        "--dir",
        cli.to_str().unwrap(),
        "--version",
        "0.2.0",
        "--base-url",
        "https://github.com/MAECLY/coverly-roms/releases/download/v0.2.0",
    ]);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let m: coverly_core::update::Manifest =
        serde_json::from_slice(&std::fs::read(d.join("latest.json")).unwrap()).unwrap();
    assert_eq!(m.platforms["cli-windows-x86_64"].url, "https://github.com/MAECLY/coverly-roms/releases/download/v0.2.0/coverly_0.2.0_windows_x64.exe");
    coverly_core::update::verify::verify_signed(
        b"a",
        &m.platforms["cli-darwin-aarch64"].signature,
        &key.public_b64(),
        "0.2.0",
    )
    .unwrap();
    let ok = tool(&[
        "check-manifest",
        "--manifest",
        d.join("latest.json").to_str().unwrap(),
        "--version",
        "0.2.0",
        "--signed-version",
        "0.2.0",
        "--require",
        "darwin-aarch64",
        "--require",
        "cli-darwin-aarch64",
    ]);
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let wrong = tool(&[
        "check-manifest",
        "--manifest",
        d.join("latest.json").to_str().unwrap(),
        "--version",
        "0.2.0",
        "--signed-version",
        "0.3.0",
        "--require",
        "cli-darwin-aarch64",
    ]);
    assert!(!wrong.status.success());
    let missing = tool(&[
        "check-manifest",
        "--manifest",
        d.join("latest.json").to_str().unwrap(),
        "--version",
        "0.2.0",
        "--require",
        "linux-x86_64",
    ]);
    assert!(!missing.status.success());
}

#[test]
fn release_tools_semver_and_pubkey() {
    if python().is_none() {
        return;
    }
    assert!(tool(&["semver-gt", "0.2.0", "0.2.0-beta.3"])
        .status
        .success());
    assert!(!tool(&["semver-gt", "0.1.5", "0.2.0-beta.3"])
        .status
        .success());
    assert_eq!(
        tool(&["semver-check", "v0.2.0-beta.1"]).status.code(),
        Some(0)
    );
    for bad in ["", "0.2", "latest"] {
        assert_eq!(tool(&["semver-check", bad]).status.code(), Some(2), "{bad}");
    }
    assert_eq!(
        tool(&["semver-gt", "0.2.0", "not-a-version"]).status.code(),
        Some(2),
        "release-channels.yml moves the pointer past a broken current manifest"
    );
    let key_file = repo().join("crates/coverly-core/updater.pub");
    let out = tool(&["pubkey", "--file", key_file.to_str().unwrap()]);
    assert_eq!(
        out.status.success(),
        coverly_core::update::release_pubkey().is_some()
    );
}

#[test]
fn the_runbook_names_every_owner_step() {
    let r = read("RELEASING.md");
    for needle in [
        "pnpm tauri signer generate",
        "TAURI_SIGNING_PRIVATE_KEY",
        "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
        "crates/coverly-core/updater.pub",
        "public",
        "scripts/bump-version.sh",
        "git tag v",
        "coverly update check",
        "channel-beta",
        "createUpdaterArtifacts",
    ] {
        assert!(r.contains(needle), "RELEASING.md lacks {needle}");
    }
    for readme in ["README.md", "README.es.md"] {
        let t = read(readme);
        assert!(t.contains("SHA256SUMS"), "{readme}");
        assert!(
            t.contains("Open Anyway") || t.contains("Abrir igualmente"),
            "{readme}"
        );
    }
    assert!(read("docs/adr/0011-release-pipeline-and-update-key.md").contains("updater.pub"));
}

/// A failure the workflow log shows as an annotation: exit 1, `::error::`, no
/// Python traceback.
fn assert_clean_failure(out: &std::process::Output, what: &str) {
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{what}: {err}");
    assert!(err.starts_with("::error::"), "{what}: {err}");
    assert!(!err.contains("Traceback"), "{what}: {err}");
}

#[test]
fn release_tools_refuse_asset_names_that_are_paths() {
    if python().is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let m = dir.path().join("latest.json");
    let base = "https://github.com/MAECLY/coverly-roms/releases/download/v0.2.0/";
    for url in [
        format!("{base}..%2Fevil.tar.gz"),
        format!("{base}a%2Fb.tar.gz"),
        format!("{base}a%5Cb.tar.gz"),
        format!("{base}%2E%2E"),
        format!("{base}.."),
        String::new(),
    ] {
        let manifest = serde_json::json!({
            "version": "0.2.0",
            "platforms": { "darwin-aarch64": { "url": url, "signature": "s" } }
        });
        std::fs::write(&m, manifest.to_string()).unwrap();
        let out = tool(&["list-artifacts", "--manifest", m.to_str().unwrap()]);
        assert_clean_failure(&out, &url);
        let out = tool(&[
            "resign-manifest",
            "--manifest",
            m.to_str().unwrap(),
            "--sigdir",
            dir.path().to_str().unwrap(),
        ]);
        assert_clean_failure(&out, &url);
    }
}

#[test]
fn release_tools_report_malformed_manifests_without_a_traceback() {
    if python().is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let m = dir.path().join("latest.json");
    for body in [
        "not json",
        "[]",
        r#"{"version": "0.2.0", "platforms": []}"#,
        r#"{"version": "0.2.0", "platforms": {"linux-x86_64": "x"}}"#,
        r#"{"version": "0.2.0", "platforms": {"linux-x86_64": {}}}"#,
        r#"{"version": "0.2.0", "platforms": {"linux-x86_64": {"url": 3}}}"#,
    ] {
        std::fs::write(&m, body).unwrap();
        for args in [
            vec!["list-artifacts", "--manifest", m.to_str().unwrap()],
            vec![
                "check-manifest",
                "--manifest",
                m.to_str().unwrap(),
                "--version",
                "0.2.0",
                "--require",
                "linux-x86_64",
            ],
            vec![
                "resign-manifest",
                "--manifest",
                m.to_str().unwrap(),
                "--sigdir",
                dir.path().to_str().unwrap(),
            ],
        ] {
            assert_clean_failure(&tool(&args), &format!("{} {body}", args[0]));
        }
    }
    let missing = dir.path().join("missing.json");
    assert_clean_failure(
        &tool(&[
            "check-manifest",
            "--manifest",
            missing.to_str().unwrap(),
            "--version",
            "0.2.0",
        ]),
        "missing file",
    );
}

#[test]
fn release_tools_read_a_key_file_with_a_bom() {
    if python().is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("updater.pub");
    std::fs::write(&f, "\u{feff}dW50cnVzdGVk\n").unwrap();
    let out = tool(&["pubkey", "--file", f.to_str().unwrap()]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "dW50cnVzdGVk"
    );
    std::fs::write(&f, "\u{feff}UNSET\n").unwrap();
    assert_clean_failure(&tool(&["pubkey", "--file", f.to_str().unwrap()]), "UNSET");
}

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

fn write_bundles(dir: &Path, names: &[&str]) {
    for name in names {
        std::fs::write(dir.join(name), name.as_bytes()).unwrap();
    }
}

fn updater_artifacts(dir: &Path) -> Vec<String> {
    let out = tool(&["updater-artifacts", "--dir", dir.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(String::from)
        .collect()
}

fn updater_manifest(dir: &Path, out_file: &Path, extra: &[&str]) -> std::process::Output {
    let mut args = vec![
        "updater-manifest",
        "--dir",
        dir.to_str().unwrap(),
        "--version",
        "0.2.0",
        "--base-url",
        "https://github.com/MAECLY/coverly-roms/releases/download/v0.2.0",
        "--output",
        out_file.to_str().unwrap(),
        "--pub-date",
        "2026-10-16T00:00:00Z",
    ];
    args.extend_from_slice(extra);
    tool(&args)
}

#[test]
#[ignore = "re-enabled in Task 4"]
fn release_tools_map_the_update_artifacts_to_platform_keys() {
    if python().is_none() {
        return;
    }
    use coverly_core::update::testing::TestKey;
    let key = TestKey::generate();
    let dir = tempfile::tempdir().unwrap();
    let bundles = dir.path().join("bundles");
    std::fs::create_dir(&bundles).unwrap();
    write_bundles(&bundles, &BUNDLES);
    // Only these are signed for latest.json; the .dmg is not.
    let artifacts = updater_artifacts(&bundles);
    assert_eq!(
        artifacts,
        [
            "Coverly-ROMs_0.2.0_linux_arm.AppImage",
            "Coverly-ROMs_0.2.0_linux_arm.deb",
            "Coverly-ROMs_0.2.0_linux_x64.AppImage",
            "Coverly-ROMs_0.2.0_linux_x64.deb",
            "Coverly-ROMs_0.2.0_mac_arm.app.tar.gz",
            "Coverly-ROMs_0.2.0_mac_intel.app.tar.gz",
            "Coverly-ROMs_0.2.0_windows_arm-setup.exe",
            "Coverly-ROMs_0.2.0_windows_arm.msi",
            "Coverly-ROMs_0.2.0_windows_x64-setup.exe",
            "Coverly-ROMs_0.2.0_windows_x64.msi",
        ]
    );
    // finalize signs each with the real key, bound to the version.
    for name in &artifacts {
        std::fs::write(
            bundles.join(format!("{name}.sig")),
            key.sign(name.as_bytes(), name, Some("0.2.0")),
        )
        .unwrap();
    }
    let changelog = dir.path().join("CHANGELOG.md");
    std::fs::write(
        &changelog,
        "# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-10-16\n\n### Added\n\n- Updates.\n\n## [0.1.0] - 2026-10-01\n\n- First.\n",
    )
    .unwrap();
    let latest = dir.path().join("latest.json");
    let out = updater_manifest(
        &bundles,
        &latest,
        &["--changelog", changelog.to_str().unwrap()],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let m: coverly_core::update::Manifest =
        serde_json::from_slice(&std::fs::read(&latest).unwrap()).unwrap();
    assert_eq!(m.version, "0.2.0");
    assert_eq!(m.notes.as_deref(), Some("### Added\n\n- Updates."));
    assert_eq!(m.pub_date.as_deref(), Some("2026-10-16T00:00:00Z"));
    let file = |k: &str| m.platforms[k].url.rsplit('/').next().unwrap().to_string();
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
    // Each entry carries the real signature of its own file.
    for (k, e) in &m.platforms {
        let name = file(k);
        coverly_core::update::verify::verify_signed(
            name.as_bytes(),
            &e.signature,
            &key.public_b64(),
            "0.2.0",
        )
        .unwrap_or_else(|err| panic!("{k}: {err}"));
    }
    // The CLI entries are added next to them and all twelve keys are checked.
    let cli = dir.path().join("cli");
    std::fs::create_dir(&cli).unwrap();
    for (suffix, os, _arch) in PLATFORMS {
        let ext = if os == "windows" { ".exe" } else { "" };
        let name = format!("coverly_0.2.0_{suffix}{ext}");
        std::fs::write(cli.join(&name), &name).unwrap();
        std::fs::write(
            cli.join(format!("{name}.sig")),
            key.sign(name.as_bytes(), &name, Some("0.2.0")),
        )
        .unwrap();
    }
    assert!(tool(&[
        "add-cli",
        "--manifest",
        latest.to_str().unwrap(),
        "--dir",
        cli.to_str().unwrap(),
        "--version",
        "0.2.0",
        "--base-url",
        "https://github.com/MAECLY/coverly-roms/releases/download/v0.2.0",
    ])
    .status
    .success());
    let wf = read(".github/workflows/release.yml");
    let mut args = vec![
        "check-manifest".to_string(),
        "--manifest".into(),
        latest.to_str().unwrap().into(),
        "--version".into(),
        "0.2.0".into(),
        "--signed-version".into(),
        "0.2.0".into(),
    ];
    // Exactly the keys finalize requires.
    for word in job(&wf, "finalize")
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
    {
        if word[0] == "--require" {
            args.push("--require".into());
            args.push(word[1].trim_end_matches('\\').to_string());
        }
    }
    assert_eq!(args.len(), 7 + 24, "finalize requires the twelve keys");
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let ok = tool(&args);
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

#[test]
fn release_tools_fall_back_to_the_msi_and_refuse_ambiguous_or_unsigned_bundles() {
    if python().is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let latest = dir.path().join("latest.json");
    let sign_all = |d: &Path| {
        for name in updater_artifacts(d) {
            std::fs::write(d.join(format!("{name}.sig")), "c2ln").unwrap();
        }
    };
    // No NSIS installer: windows-x86_64 is the MSI.
    let msi_only = dir.path().join("msi");
    std::fs::create_dir(&msi_only).unwrap();
    write_bundles(&msi_only, &["Coverly-ROMs_0.2.0_windows_x64.msi"]);
    sign_all(&msi_only);
    assert!(updater_manifest(&msi_only, &latest, &[]).status.success());
    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(&latest).unwrap()).unwrap();
    assert!(m["platforms"]["windows-x86_64"]["url"]
        .as_str()
        .unwrap()
        .ends_with(".msi"));
    assert!(m["notes"].is_null());
    std::fs::remove_file(&latest).unwrap();
    // A missing .sig, two AppImages for one platform, a name that is not a
    // release name, or a CHANGELOG without the version: nothing is written.
    let cases: [(&str, &[&str], bool); 3] = [
        (
            "unsigned",
            &["Coverly-ROMs_0.2.0_linux_x64.AppImage"],
            false,
        ),
        (
            "twice",
            &[
                "Coverly-ROMs_0.2.0_linux_x64.AppImage",
                "Coverly-ROMs_0.2.1_linux_x64.AppImage",
            ],
            true,
        ),
        ("no-arch", &["Coverly.ROMs.app.tar.gz"], true),
    ];
    for (name, files, signed) in cases {
        let d = dir.path().join(name);
        std::fs::create_dir(&d).unwrap();
        write_bundles(&d, files);
        if signed {
            for f in files {
                std::fs::write(d.join(format!("{f}.sig")), "c2ln").unwrap();
            }
        }
        assert_clean_failure(&updater_manifest(&d, &latest, &[]), name);
        assert!(!latest.exists(), "{name}");
    }
    let changelog = dir.path().join("CHANGELOG.md");
    std::fs::write(&changelog, "# Changelog\n\n## [0.1.0]\n- x\n").unwrap();
    assert_clean_failure(
        &updater_manifest(
            &msi_only,
            &latest,
            &["--changelog", changelog.to_str().unwrap()],
        ),
        "changelog",
    );
    assert!(!latest.exists());
    assert_clean_failure(
        &updater_manifest(&dir.path().join("empty-missing"), &latest, &[]),
        "missing dir",
    );
}

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
        (
            "suffix of another os",
            "Coverly-ROMs_0.2.0_mac_arm-setup.exe",
        ),
        ("linux suffix on a msi", "Coverly-ROMs_0.2.0_linux_x64.msi"),
        ("a space", "Coverly ROMs_0.2.0_linux_x64.AppImage"),
        (
            "unknown platform",
            "Coverly-ROMs_0.2.0_mac_silicon.app.tar.gz",
        ),
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
    // A prerelease name is a release name too: stage-builds gives a beta's
    // files `Coverly-ROMs_<V>-beta.N_<platform><kind>`.
    let beta = dir.path().join("beta");
    std::fs::create_dir(&beta).unwrap();
    let name = "Coverly-ROMs_0.3.0-beta.1_windows_arm-setup.exe";
    write_pinned(&beta.join(name), name.as_bytes());
    write_pinned(&beta.join(format!("{name}.sig")), b"c2ln");
    let out = updater_manifest(&beta, &latest, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(&latest).unwrap()).unwrap();
    let keys: Vec<&str> = m["platforms"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["windows-aarch64", "windows-aarch64-nsis"]);
}

#[test]
fn add_cli_reads_release_names() {
    if python().is_none() {
        return;
    }
    let add_version = |dir: &Path, manifest: &Path, version: &str| {
        tool(&[
            "add-cli",
            "--manifest",
            manifest.to_str().unwrap(),
            "--dir",
            dir.to_str().unwrap(),
            "--version",
            version,
            "--base-url",
            "https://github.com/MAECLY/coverly-roms/releases/download/v0.3.0",
        ])
    };
    let add = |dir: &Path, manifest: &Path| add_version(dir, manifest, "0.3.0");
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
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
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
        "coverly_0.3.0_mac_arm.exe",
        "coverly_0.3.0_windows_x64",
        "coverly_0.3.0_windows_arm",
        "coverly-0.3.0-linux-x86_64",
        "coverly-0.3.0-windows-aarch64.exe",
        "coverly_0.2.9_linux_x64",
        "coverly_0.3.0_mac_silicon",
        "Coverly-ROMs_0.3.0_linux_x64.AppImage",
    ] {
        let d = root.path().join(bad);
        write_pinned(&d.join(bad), b"x");
        write_pinned(&d.join(format!("{bad}.sig")), b"c2ln");
        let m = root.path().join(format!("{bad}.json"));
        let out = add(&d, &m);
        assert_clean_failure(&out, bad);
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(&format!("unexpected file {bad}")),
            "{bad}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!m.exists(), "{bad}");
    }
    // A beta's binary, with the tag's `v` on --version as the workflow may
    // pass it, gets its key; the version must still match exactly.
    let beta = root.path().join("beta");
    let name = "coverly_0.3.0-beta.1_windows_arm.exe";
    write_pinned(&beta.join(name), name.as_bytes());
    write_pinned(&beta.join(format!("{name}.sig")), b"c2ln");
    let m = root.path().join("beta.json");
    let out = add_version(&beta, &m, "v0.3.0-beta.1");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&m).unwrap()).unwrap();
    assert!(v["platforms"]["cli-windows-aarch64"]["url"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/{name}")));
    let m = root.path().join("beta-stable.json");
    assert_clean_failure(&add_version(&beta, &m, "0.3.0"), "beta as stable");
    assert!(!m.exists());
}

/// Runs release-channels.yml's script against a fake `gh` that serves
/// `<tag>.json` files from `releases` and logs what it is asked to do.
fn run_beta_pointer(current: Option<&str>) -> (std::process::Output, String) {
    use coverly_core::update::testing::TestKey;
    let script = step_script(
        &job(
            &read(".github/workflows/release-channels.yml"),
            "beta-pointer",
        ),
        "Copy latest.json to the channel-beta release",
    );
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::create_dir_all(d.join("scripts/release")).unwrap();
    std::fs::copy(
        repo().join("scripts/release/release_tools.py"),
        d.join("scripts/release/release_tools.py"),
    )
    .unwrap();
    let releases = d.join("releases");
    std::fs::create_dir(&releases).unwrap();
    let key = TestKey::generate();
    let mut platforms = serde_json::Map::new();
    for k in [
        "darwin-aarch64",
        "darwin-x86_64",
        "windows-x86_64",
        "linux-x86_64",
        "linux-aarch64",
        "cli-darwin-aarch64",
        "cli-darwin-x86_64",
        "cli-windows-x86_64",
        "cli-linux-x86_64",
        "cli-linux-aarch64",
    ] {
        platforms.insert(
            k.into(),
            serde_json::json!({ "url": format!("https://example.invalid/{k}"), "signature": key.sign(k.as_bytes(), k, Some("0.3.0")) }),
        );
    }
    let new = serde_json::json!({ "version": "0.3.0", "platforms": platforms });
    std::fs::write(releases.join("v0.3.0.json"), new.to_string()).unwrap();
    if let Some(body) = current {
        std::fs::write(releases.join("channel-beta.json"), body).unwrap();
    }
    let bin = d.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let gh = bin.join("gh");
    std::fs::write(
        &gh,
        r#"#!/usr/bin/env bash
echo "gh $*" >> "$FAKE_LOG"
case "$1 $2" in
  "release view") exit 0 ;;
  "release create") exit 0 ;;
  "release upload") exit 0 ;;
  "release download")
    tag="$3"; shift 3; dir=""
    while [ $# -gt 0 ]; do [ "$1" = --dir ] && dir="$2"; shift; done
    [ -f "$FAKE_RELEASES/$tag.json" ] || exit 1
    mkdir -p "$dir" && cp "$FAKE_RELEASES/$tag.json" "$dir/latest.json" ;;
  *) exit 3 ;;
esac
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let log = d.join("gh.log");
    std::fs::write(&log, "").unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("bash")
        .args(["-e", "-c", &script])
        .current_dir(d)
        .env("PATH", path)
        .env("FAKE_LOG", &log)
        .env("FAKE_RELEASES", &releases)
        .env("TAG", "v0.3.0")
        .env("REPO", "MAECLY/coverly-roms")
        .env("BETA_TAG", coverly_core::update::BETA_TAG)
        .env("GITHUB_SHA", "0000000")
        .env("GITHUB_STEP_SUMMARY", d.join("summary.md"))
        .output()
        .unwrap();
    let log = std::fs::read_to_string(&log).unwrap();
    (out, log)
}

#[test]
fn the_beta_pointer_moves_forward_only_and_past_a_broken_manifest() {
    if !scripts_runnable() {
        return;
    }
    let uploaded = |log: &str| log.contains("gh release upload channel-beta new/latest.json");
    // Newer: moves. Older or equal: stays, with a notice.
    let (out, log) = run_beta_pointer(Some(r#"{"version": "0.2.0", "platforms": {}}"#));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(uploaded(&log), "{log}");
    let (out, log) = run_beta_pointer(Some(r#"{"version": "0.4.0", "platforms": {}}"#));
    assert!(out.status.success());
    assert!(!uploaded(&log), "{log}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("::notice::beta stays on 0.4.0"));
    // No current manifest yet: moves.
    let (out, log) = run_beta_pointer(None);
    assert!(out.status.success());
    assert!(uploaded(&log), "{log}");
    // A current manifest whose version is missing or not SemVer (or that is
    // not JSON at all) cannot be compared: the pointer moves, with a warning.
    for broken in [
        r#"{"platforms": {}}"#,
        r#"{"version": "latest", "platforms": {}}"#,
        r#"{"version": 3}"#,
        "not json",
    ] {
        let (out, log) = run_beta_pointer(Some(broken));
        assert!(
            out.status.success(),
            "{broken}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(uploaded(&log), "{broken}: {log}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("::warning::"),
            "{broken}"
        );
    }
}

fn snapshot(dir: &Path) -> Vec<String> {
    ["Cargo.toml", "package.json", "Cargo.lock"]
        .iter()
        .map(|f| std::fs::read_to_string(dir.join(f)).unwrap())
        .collect()
}

fn bump(dir: &Path, arg: &str) -> std::process::Output {
    Command::new("bash")
        .arg(repo().join("scripts/bump-version.sh"))
        .arg("--root")
        .arg(dir)
        .arg(arg)
        .output()
        .unwrap()
}

#[test]
fn bump_version_check_fails_on_a_missing_source_and_reads_only_workspace_package() {
    if !scripts_runnable() {
        return;
    }
    // [workspace.package] without a version; a later table has one. Neither
    // --check nor a bump may take that other version for the workspace's.
    let dir = scratch_checkout();
    let cargo = dir.path().join("Cargo.toml");
    let text = std::fs::read_to_string(&cargo).unwrap();
    let ws = workspace_version(&text);
    let stripped = text.replacen(&format!("version = \"{ws}\"\n"), "", 1);
    assert_ne!(stripped, text);
    std::fs::write(
        &cargo,
        format!("{stripped}\n[package.metadata.x]\nversion = \"{ws}\"\n"),
    )
    .unwrap();
    let check = bump(dir.path(), "--check");
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stdout).contains("Cargo.toml <none>"));
    assert!(String::from_utf8_lossy(&check.stderr).contains("<none>"));
    let before = snapshot(dir.path());
    let out = bump(dir.path(), "1.2.3");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(snapshot(dir.path()), before, "nothing is written");

    // package.json without a version: the other four agreeing is not enough.
    let dir = scratch_checkout();
    let pkg = dir.path().join("package.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&pkg).unwrap()).unwrap();
    v.as_object_mut().unwrap().remove("version");
    std::fs::write(&pkg, v.to_string()).unwrap();
    assert_eq!(bump(dir.path(), "--check").status.code(), Some(1));
}

#[test]
fn bump_version_writes_nothing_unless_every_source_matched() {
    if !scripts_runnable() {
        return;
    }
    let dir = scratch_checkout();
    let lock = dir.path().join("Cargo.lock");
    let text = std::fs::read_to_string(&lock).unwrap();
    std::fs::write(
        &lock,
        text.replace("name = \"coverly-cli\"", "name = \"coverly-cli-renamed\""),
    )
    .unwrap();
    let before = snapshot(dir.path());
    let out = bump(dir.path(), "1.2.3");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("coverly-cli"));
    assert_eq!(
        snapshot(dir.path()),
        before,
        "Cargo.toml and package.json are untouched when Cargo.lock does not match"
    );
}

/// A folder of stand-in commands (`pnpm`, `gh`) that record their arguments
/// in `<name>.calls`, one per line, and succeed.
fn fake_bin(names: &[&str]) -> tempfile::TempDir {
    let bin = tempfile::tempdir().unwrap();
    for name in names {
        let p = bin.path().join(name);
        std::fs::write(
            &p,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"{}\"\n",
                bin.path().join(format!("{name}.calls")).display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    bin
}

fn with_fake_bin(bin: &Path) -> String {
    format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

/// The arguments `pnpm` receives from the app job's build step.
fn app_build_args(runner_os: &str, tag: &str, matrix_args: &str) -> Vec<String> {
    let script = step_script(
        &job(&read(".github/workflows/release.yml"), "app"),
        "Build the bundles",
    );
    let bin = fake_bin(&["pnpm"]);
    let out = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .env("PATH", with_fake_bin(bin.path()))
        .env("RUNNER_OS", runner_os)
        .env("RUNNER_TEMP", "/runner/temp")
        .env("GITHUB_REF_NAME", tag)
        .env("ARGS", matrix_args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(bin.path().join("pnpm.calls"))
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
}

#[test]
fn a_prerelease_builds_windows_without_the_msi() {
    if !scripts_runnable() {
        return;
    }
    let overlay = ["--config", "/runner/temp/updater-overlay.json"];
    // WiX refuses a non-numeric prerelease (1.0.0-beta.1), so a beta tag
    // builds only the NSIS installer on Windows.
    assert_eq!(
        app_build_args("Windows", "v1.0.0-beta.1", ""),
        [&["tauri", "build", "--bundles", "nsis"][..], &overlay[..]].concat()
    );
    // A stable tag builds every configured bundle, the .msi included.
    assert_eq!(
        app_build_args("Windows", "v1.0.0", ""),
        [&["tauri", "build"][..], &overlay[..]].concat()
    );
    // Other platforms never get --bundles.
    assert_eq!(
        app_build_args("macOS", "v1.0.0-beta.1", "--target universal-apple-darwin"),
        [
            &["tauri", "build", "--target", "universal-apple-darwin"][..],
            &overlay[..]
        ]
        .concat()
    );
    assert_eq!(
        app_build_args("Linux", "v1.0.0-beta.1", ""),
        [&["tauri", "build"][..], &overlay[..]].concat()
    );
}

/// Runs finalize's SHA256SUMS step over `bundles` for `tag`.
fn finalize_checksums(tag: &str, bundles: &[&str]) -> std::process::Output {
    let script = step_script(
        &job(&read(".github/workflows/release.yml"), "finalize"),
        "SHA256SUMS and upload to the draft",
    );
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::create_dir(d.join("bundles")).unwrap();
    std::fs::create_dir(d.join("cli")).unwrap();
    write_bundles(&d.join("bundles"), bundles);
    std::fs::write(d.join("cli/coverly-1.0.0-linux-x86_64"), "cli").unwrap();
    std::fs::write(d.join("latest.json"), "{}").unwrap();
    let bin = fake_bin(&["gh"]);
    Command::new("bash")
        .arg("-c")
        .arg(&script)
        .current_dir(d)
        .env("PATH", with_fake_bin(bin.path()))
        .env("GITHUB_REF_NAME", tag)
        .env("REPO", "MAECLY/coverly-roms")
        .env("GH_TOKEN", "x")
        .env("GITHUB_STEP_SUMMARY", d.join("summary.md"))
        .output()
        .unwrap()
}

#[test]
fn finalize_requires_the_msi_only_for_a_stable_release() {
    if !scripts_runnable() {
        return;
    }
    let no_msi = [
        "Coverly.ROMs_1.0.0_universal.dmg",
        "Coverly.ROMs_1.0.0_universal.app.tar.gz",
        "Coverly.ROMs_1.0.0_x64-setup.exe",
        "Coverly.ROMs_1.0.0_amd64.AppImage",
        "Coverly.ROMs_1.0.0_amd64.deb",
    ];
    let beta = finalize_checksums("v1.0.0-beta.1", &no_msi);
    assert!(
        beta.status.success(),
        "{}",
        String::from_utf8_lossy(&beta.stderr)
    );
    let stable = finalize_checksums("v1.0.0", &no_msi);
    assert!(!stable.status.success());
    assert!(String::from_utf8_lossy(&stable.stdout).contains("msi"));
    let mut with_msi = no_msi.to_vec();
    with_msi.push("Coverly.ROMs_1.0.0_x64_en-US.msi");
    let stable = finalize_checksums("v1.0.0", &with_msi);
    assert!(
        stable.status.success(),
        "{}",
        String::from_utf8_lossy(&stable.stderr)
    );
}

#[test]
fn the_guard_checks_the_signing_key_without_reading_it() {
    let wf = read(".github/workflows/release.yml");
    let guard = job(&wf, "guard");
    assert!(guard.contains("${{ secrets.TAURI_SIGNING_PRIVATE_KEY != '' }}"));
    assert!(!guard.contains("secrets.TAURI_SIGNING_PRIVATE_KEY }}"));
    if !scripts_runnable() {
        return;
    }
    let script = step_script(&guard, "Signing key must be configured");
    for (set, ok) in [("true", true), ("false", false)] {
        let out = Command::new("bash")
            .arg("-c")
            .arg(&script)
            .env("SIGNING_KEY_SET", set)
            .output()
            .unwrap();
        assert_eq!(out.status.success(), ok, "{set}");
    }
}

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
        (
            "app-darwin-aarch64".into(),
            format!("Coverly.ROMs_{v}_aarch64.app.tar.gz"),
        ),
        (
            "app-darwin-aarch64".into(),
            format!("Coverly.ROMs_{v}_aarch64.dmg"),
        ),
        (
            "app-darwin-x86_64".into(),
            format!("Coverly.ROMs_{v}_x64.app.tar.gz"),
        ),
        (
            "app-darwin-x86_64".into(),
            format!("Coverly.ROMs_{v}_x64.dmg"),
        ),
        (
            "app-windows-x86_64".into(),
            format!("Coverly.ROMs_{v}_x64-setup.exe"),
        ),
        (
            "app-windows-aarch64".into(),
            format!("Coverly.ROMs_{v}_arm64-setup.exe"),
        ),
        (
            "app-linux-x86_64".into(),
            format!("Coverly.ROMs_{v}_amd64.AppImage"),
        ),
        (
            "app-linux-x86_64".into(),
            format!("Coverly.ROMs_{v}_amd64.deb"),
        ),
        (
            "app-linux-aarch64".into(),
            format!("Coverly.ROMs_{v}_aarch64.AppImage"),
        ),
        (
            "app-linux-aarch64".into(),
            format!("Coverly.ROMs_{v}_arm64.deb"),
        ),
    ];
    if msi {
        files.push((
            "app-windows-x86_64".into(),
            format!("Coverly.ROMs_{v}_x64_en-US.msi"),
        ));
        files.push((
            "app-windows-aarch64".into(),
            format!("Coverly.ROMs_{v}_arm64_en-US.msi"),
        ));
    }
    for (_suffix, os, arch) in PLATFORMS {
        let ext = if os == "windows" { ".exe" } else { "" };
        files.push((
            format!("cli-{os}-{arch}"),
            format!("coverly-{v}-{os}-{arch}{ext}"),
        ));
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

fn stage_builds(root: &Path, version: &str) -> std::process::Output {
    tool(&[
        "stage-builds",
        "--from",
        root.join("downloads").to_str().unwrap(),
        "--bundles",
        root.join("bundles").to_str().unwrap(),
        "--cli",
        root.join("cli").to_str().unwrap(),
        "--version",
        version,
    ])
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|it| {
            it.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn stage_builds_renames_every_platform() {
    if python().is_none() {
        return;
    }
    for (v, msi, n_bundles) in [("1.0.0", true, 12), ("1.0.0-beta.1", false, 10)] {
        let dir = tempfile::tempdir().unwrap();
        downloaded_builds(&dir.path().join("downloads"), v, msi);
        let out = stage_builds(dir.path(), v);
        assert!(
            out.status.success(),
            "{v}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let (bundles, cli) = release_files(v, msi);
        assert_eq!(bundles.len(), n_bundles, "{v}");
        assert_eq!(listing(&dir.path().join("bundles")), bundles, "{v}");
        assert_eq!(listing(&dir.path().join("cli")), cli, "{v}");
        // Content is copied byte for byte from the file it was renamed from.
        let staged = std::fs::read(
            dir.path()
                .join("bundles")
                .join(format!("Coverly-ROMs_{v}_windows_arm-setup.exe")),
        )
        .unwrap();
        assert_eq!(
            staged,
            format!("Coverly.ROMs_{v}_arm64-setup.exe").as_bytes()
        );
        let staged = std::fs::read(
            dir.path()
                .join("cli")
                .join(format!("coverly_{v}_mac_intel")),
        )
        .unwrap();
        assert_eq!(staged, format!("coverly-{v}-darwin-x86_64").as_bytes());
    }
}

#[test]
fn release_tools_stage_each_build_only_from_its_own_artifact() {
    if python().is_none() {
        return;
    }
    // A stable release: every artifact holds exactly its platform's files.
    let dir = tempfile::tempdir().unwrap();
    downloaded_builds(&dir.path().join("downloads"), "1.0.0", true);
    let out = stage_builds(dir.path(), "1.0.0");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        listing(&dir.path().join("bundles")),
        release_files("1.0.0", true).0
    );
    assert_eq!(
        listing(&dir.path().join("cli")),
        release_files("1.0.0", true).1
    );

    // A beta has no .msi (WiX), and stages fine without it.
    let dir = tempfile::tempdir().unwrap();
    downloaded_builds(&dir.path().join("downloads"), "1.0.0-beta.1", false);
    let out = stage_builds(dir.path(), "1.0.0-beta.1");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        listing(&dir.path().join("bundles")),
        release_files("1.0.0-beta.1", false).0
    );

    // Each tampering fails and stages nothing.
    type Tamper = fn(&Path);
    let cases: [(&str, Tamper, &str); 10] = [
        (
            "an extra artifact",
            |d| write_pinned(&d.join("app-zzz/Coverly.ROMs_1.0.0_universal.dmg"), b"evil"),
            "app-zzz",
        ),
        (
            "another platform's file in an artifact",
            |d| {
                write_pinned(
                    &d.join("app-darwin-aarch64/Coverly.ROMs_1.0.0_x64.dmg"),
                    b"evil",
                )
            },
            "app-darwin-aarch64",
        ),
        (
            "a missing artifact",
            |d| std::fs::remove_dir_all(d.join("app-windows-aarch64")).unwrap(),
            "app-windows-aarch64",
        ),
        (
            "a missing file",
            |d| {
                std::fs::remove_file(d.join("app-linux-aarch64/Coverly.ROMs_1.0.0_arm64.deb"))
                    .unwrap()
            },
            "app-linux-aarch64",
        ),
        (
            "a second file of one kind",
            |d| {
                std::fs::write(
                    d.join("app-linux-x86_64/Coverly.ROMs_1.0.0_amd64.x.deb"),
                    "x",
                )
                .unwrap()
            },
            "app-linux-x86_64",
        ),
        (
            "a signature handed over",
            |d| {
                write_pinned(
                    &d.join("app-windows-aarch64/Coverly.ROMs_1.0.0_arm64-setup.exe.sig"),
                    b"x",
                )
            },
            "app-windows-aarch64",
        ),
        (
            "a CLI of another version",
            |d| {
                std::fs::rename(
                    d.join("cli-linux-x86_64/coverly-1.0.0-linux-x86_64"),
                    d.join("cli-linux-x86_64/coverly-0.9.0-linux-x86_64"),
                )
                .unwrap()
            },
            "cli-linux-x86_64",
        ),
        (
            "the old universal artifact",
            |d| {
                write_pinned(
                    &d.join("app-macos-universal/Coverly.ROMs_1.0.0_universal.dmg"),
                    b"old",
                )
            },
            "app-macos-universal",
        ),
        (
            "two .msi locales",
            |d| {
                write_pinned(
                    &d.join("app-windows-x86_64/Coverly.ROMs_1.0.0_x64_es-ES.msi"),
                    b"x",
                )
            },
            "more than one .msi",
        ),
        (
            "a symlink under a valid name",
            |d| {
                // The real AppImage is swapped for a link with the same
                // name, so only the symlink check can refuse it.
                let real = d.join("app-linux-x86_64/Coverly.ROMs_1.0.0_amd64.AppImage");
                // Outside the downloads folder, so the artifact list is
                // unchanged and the link points at a regular file.
                let outside = d.parent().unwrap().join("outside.AppImage");
                std::fs::rename(&real, &outside).unwrap();
                #[cfg(unix)]
                std::os::unix::fs::symlink(&outside, &real).unwrap();
                // Windows needs a privilege to create a symlink; a folder
                // under the valid name is refused by the same branch.
                #[cfg(not(unix))]
                std::fs::create_dir(&real).unwrap();
            },
            "app-linux-x86_64: unexpected file Coverly.ROMs_1.0.0_amd64.AppImage",
        ),
    ];
    for (what, tamper, names) in cases {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("downloads");
        downloaded_builds(&downloads, "1.0.0", true);
        tamper(&downloads);
        let out = stage_builds(dir.path(), "1.0.0");
        assert!(!out.status.success(), "{what}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("::error::") && err.contains(names),
            "{what}: {err}"
        );
        assert!(!err.contains("Traceback"), "{what}");
        assert!(listing(&dir.path().join("bundles")).is_empty(), "{what}");
        assert!(listing(&dir.path().join("cli")).is_empty(), "{what}");
    }

    // A version GitHub would rewrite in an asset name (`+` becomes `.`)
    // stages nothing: the url in latest.json would not match the asset.
    let dir = tempfile::tempdir().unwrap();
    downloaded_builds(&dir.path().join("downloads"), "1.0.0+build.1", true);
    let out = stage_builds(dir.path(), "1.0.0+build.1");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("GitHub would rewrite"), "{err}");
    assert!(!err.contains("Traceback"), "{err}");
    assert!(listing(&dir.path().join("bundles")).is_empty());
    assert!(listing(&dir.path().join("cli")).is_empty());

    // A stable release must ship its .msi.
    let dir = tempfile::tempdir().unwrap();
    downloaded_builds(&dir.path().join("downloads"), "1.0.0", false);
    let out = stage_builds(dir.path(), "1.0.0");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains(".msi"));
}

#[test]
fn finalize_downloads_every_build_separately_and_stages_them_by_name() {
    let finalize = job(&read(".github/workflows/release.yml"), "finalize");
    assert!(
        !finalize.contains("merge-multiple"),
        "one artifact must not overwrite another's files"
    );
    assert!(finalize.contains("stage-builds --from downloads"));
}

/// The landing (MAECLY/coverly-roms-landing) fills its download links from
/// the latest release, so publishing one here asks Vercel to rebuild it.
#[test]
fn landing_redeploys_when_a_release_is_published() {
    let wf = read(".github/workflows/landing-redeploy.yml");
    let on = wf
        .split_once("\non:\n")
        .unwrap()
        .1
        .split_once("\npermissions:")
        .unwrap()
        .0;
    assert!(on.contains("  release:\n    types: [published]\n"), "{on}");
    assert!(on.contains("  workflow_dispatch:"), "{on}");
    // Least privilege: the hook URL is the only credential.
    assert!(wf.contains("\npermissions: {}\n"));
    assert!(wf.contains("HOOK: ${{ secrets.VERCEL_DEPLOY_HOOK_URL }}"));
    assert!(wf.contains("if [ -z \"${HOOK:-}\" ]; then"));
    assert!(wf.contains("curl -fsS -X POST \"$HOOK\""));
    assert!(!wf.contains("uses:"), "no third-party action");
    assert!(!wf.contains("continue-on-error") && !wf.contains("|| true"));
}
