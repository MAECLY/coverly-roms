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

fn bash() -> Option<&'static str> {
    Command::new("bash")
        .arg("--version")
        .output()
        .ok()
        .map(|_| "bash")
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
        eprintln!("no bash on this machine; bump-version.sh not exercised");
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
    ["python3", "python"].into_iter().find(|p| {
        Command::new(p)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

fn tool(args: &[&str]) -> std::process::Output {
    Command::new(python().unwrap())
        .arg("-I")
        .arg(repo().join("scripts/release/release_tools.py"))
        .args(args)
        .output()
        .unwrap()
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
    let key_file = repo().join("crates/coverly-core/updater.pub");
    let out = tool(&["pubkey", "--file", key_file.to_str().unwrap()]);
    assert_eq!(
        out.status.success(),
        coverly_core::update::release_pubkey().is_some()
    );
}
