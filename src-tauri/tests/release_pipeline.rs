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
