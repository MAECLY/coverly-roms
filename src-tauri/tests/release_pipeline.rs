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
    assert!(app.contains("pnpm tauri build $ARGS --config \"$RUNNER_TEMP/updater-overlay.json\""));
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
    assert!(finalize.contains("pattern: app-*") && finalize.contains("pattern: cli-*"));
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
        2,
        "guard check + finalize sign"
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
    ] {
        assert!(notes.contains(needle), "{needle}");
    }
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
        ("coverly-0.2.0-darwin-aarch64", "a"),
        ("coverly-0.2.0-windows-x86_64.exe", "w"),
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
    assert_eq!(m.platforms["cli-windows-x86_64"].url, "https://github.com/MAECLY/coverly-roms/releases/download/v0.2.0/coverly-0.2.0-windows-x86_64.exe");
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

/// The bundles the read-only `app` jobs hand to `finalize` (staged names).
const BUNDLES: [&str; 8] = [
    "Coverly.ROMs_0.2.0_universal.app.tar.gz",
    "Coverly.ROMs_0.2.0_universal.dmg",
    "Coverly.ROMs_0.2.0_x64-setup.exe",
    "Coverly.ROMs_0.2.0_x64_en-US.msi",
    "Coverly.ROMs_0.2.0_amd64.AppImage",
    "Coverly.ROMs_0.2.0_amd64.deb",
    "Coverly.ROMs_0.2.0_aarch64.AppImage",
    "Coverly.ROMs_0.2.0_arm64.deb",
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
    // Only these are signed for latest.json; the .dmg and .deb are not.
    let artifacts = updater_artifacts(&bundles);
    assert_eq!(
        artifacts,
        [
            "Coverly.ROMs_0.2.0_aarch64.AppImage",
            "Coverly.ROMs_0.2.0_amd64.AppImage",
            "Coverly.ROMs_0.2.0_universal.app.tar.gz",
            "Coverly.ROMs_0.2.0_x64-setup.exe",
            "Coverly.ROMs_0.2.0_x64_en-US.msi",
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
    for (k, name) in [
        ("darwin-aarch64", "Coverly.ROMs_0.2.0_universal.app.tar.gz"),
        ("darwin-x86_64", "Coverly.ROMs_0.2.0_universal.app.tar.gz"),
        (
            "darwin-aarch64-app",
            "Coverly.ROMs_0.2.0_universal.app.tar.gz",
        ),
        ("windows-x86_64", "Coverly.ROMs_0.2.0_x64-setup.exe"),
        ("windows-x86_64-nsis", "Coverly.ROMs_0.2.0_x64-setup.exe"),
        ("windows-x86_64-msi", "Coverly.ROMs_0.2.0_x64_en-US.msi"),
        ("linux-x86_64", "Coverly.ROMs_0.2.0_amd64.AppImage"),
        ("linux-aarch64", "Coverly.ROMs_0.2.0_aarch64.AppImage"),
        ("linux-x86_64-appimage", "Coverly.ROMs_0.2.0_amd64.AppImage"),
    ] {
        assert_eq!(file(k), name, "{k}");
    }
    assert!(!m
        .platforms
        .values()
        .any(|e| e.url.ends_with(".deb") || e.url.ends_with(".dmg")));
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
    // The CLI entries are added next to them and all ten keys are checked.
    let cli = dir.path().join("cli");
    std::fs::create_dir(&cli).unwrap();
    for (os, arch) in [
        ("darwin", "aarch64"),
        ("darwin", "x86_64"),
        ("windows", "x86_64"),
        ("linux", "x86_64"),
        ("linux", "aarch64"),
    ] {
        let ext = if os == "windows" { ".exe" } else { "" };
        let name = format!("coverly-0.2.0-{os}-{arch}{ext}");
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
    assert_eq!(args.len(), 7 + 20, "finalize requires the ten keys");
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
    write_bundles(&msi_only, &["Coverly.ROMs_0.2.0_x64_en-US.msi"]);
    sign_all(&msi_only);
    assert!(updater_manifest(&msi_only, &latest, &[]).status.success());
    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(&latest).unwrap()).unwrap();
    assert!(m["platforms"]["windows-x86_64"]["url"]
        .as_str()
        .unwrap()
        .ends_with(".msi"));
    assert!(m["notes"].is_null());
    std::fs::remove_file(&latest).unwrap();
    // A missing .sig, two AppImages for one architecture, a name without an
    // architecture, or a CHANGELOG without the version: nothing is written.
    let cases: [(&str, &[&str], bool); 3] = [
        ("unsigned", &["Coverly.ROMs_0.2.0_amd64.AppImage"], false),
        (
            "twice",
            &[
                "Coverly.ROMs_0.2.0_amd64.AppImage",
                "Other_0.2.0_x86_64.AppImage",
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
