//! `coverly update` against a local manifest server with a throwaway key.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::update::testing::{manifest_json, TestKey};
use coverly_core::update::{cli_platform_key, release_pubkey};
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const NEW: &[u8] = b"#!/bin/sh\necho coverly 9.9.9\n";

struct Fx {
    server: MockServer,
    key: TestKey,
    dir: TempDir,
    exe: PathBuf,
}

/// A copy of the built `coverly` in a temp dir (never the real one) and a
/// server whose manifest offers `version` for this platform.
fn fx(version: &str, body: &[u8], signed_for: Option<&str>) -> Fx {
    let key = TestKey::generate();
    let server = MockServer::start();
    let platform = cli_platform_key().expect("a supported test platform");
    let sig = key.sign(NEW, "coverly", signed_for);
    server.mock("/coverly-bin", MockResponse::ok(body.to_vec()));
    server.serve_json(
        "/latest.json",
        &manifest_json(version, &[(&platform, &server.url("/coverly-bin"), &sig)]),
    );
    let dir = TempDir::new().unwrap();
    let exe = dir.path().join(if cfg!(windows) {
        "coverly.exe"
    } else {
        "coverly"
    });
    fs::copy(assert_cmd::cargo::cargo_bin("coverly"), &exe).unwrap();
    Fx {
        server,
        key,
        dir,
        exe,
    }
}

fn run(f: &Fx) -> Command {
    run_at(f, &f.exe)
}

fn run_at(f: &Fx, exe: &Path) -> Command {
    let mut c = Command::new(exe);
    c.env("COVERLY_SETTINGS", "")
        .env("COVERLY_UPDATE_ENDPOINT", f.server.url("/latest.json"))
        .env("COVERLY_UPDATE_PUBKEY", f.key.public_b64());
    c
}

fn same_as_built(exe: &Path) -> bool {
    fs::read(exe).unwrap() == fs::read(assert_cmd::cargo::cargo_bin("coverly")).unwrap()
}

#[test]
fn check_reports_a_newer_version_without_downloading() {
    let f = fx("9.9.9", NEW, Some("9.9.9"));
    run(&f)
        .args(["update", "check"])
        .assert()
        .success()
        .stdout(contains("9.9.9 is available"));
    assert_eq!(f.server.hits("/coverly-bin"), 0);
}

#[test]
fn check_json_and_up_to_date() {
    let f = fx(
        env!("CARGO_PKG_VERSION"),
        NEW,
        Some(env!("CARGO_PKG_VERSION")),
    );
    let out = run(&f)
        .args(["update", "check", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["available"], false);
    assert_eq!(v["current"], env!("CARGO_PKG_VERSION"));
    run(&f)
        .args(["update", "check"])
        .assert()
        .success()
        .stdout(contains("up to date"));
}

#[test]
fn install_dry_run_verifies_and_changes_nothing() {
    let f = fx("9.9.9", NEW, Some("9.9.9"));
    run(&f)
        .args(["update", "install", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("verified").and(contains("not installed")));
    assert!(same_as_built(&f.exe));
}

#[test]
fn install_replaces_this_binary_with_the_verified_one() {
    let f = fx("9.9.9", NEW, Some("9.9.9"));
    run(&f)
        .args(["update", "install"])
        .assert()
        .success()
        .stdout(contains("Updated to 9.9.9"));
    assert_eq!(fs::read(&f.exe).unwrap(), NEW);
    let leftovers: Vec<_> = fs::read_dir(f.dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".new-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn install_refuses_a_tampered_download() {
    let f = fx("9.9.9", b"#!/bin/sh\necho evil\n", Some("9.9.9"));
    run(&f)
        .args(["update", "install"])
        .assert()
        .code(6)
        .stderr(contains("signature").and(contains("NOT installed")));
    assert!(same_as_built(&f.exe));
}

#[test]
fn install_refuses_a_replayed_signature() {
    let f = fx("9.9.9", NEW, Some("1.0.0"));
    run(&f)
        .args(["update", "install"])
        .assert()
        .code(6)
        .stderr(contains("signed for 1.0.0"));
    assert!(same_as_built(&f.exe));
}

#[test]
fn check_without_this_platform_says_so() {
    let f = fx("9.9.9", NEW, Some("9.9.9"));
    f.server
        .serve_json("/latest.json", &manifest_json("9.9.9", &[]));
    run(&f)
        .args(["update", "install"])
        .assert()
        .code(1)
        .stderr(contains("no update for this platform"));
}

#[test]
fn offline_is_a_clear_error() {
    let f = fx("9.9.9", NEW, Some("9.9.9"));
    run(&f)
        .env("COVERLY_UPDATE_ENDPOINT", "http://127.0.0.1:1/latest.json")
        .args(["update", "check"])
        .assert()
        .code(1)
        .stderr(contains("offline"));
}

#[test]
fn without_a_key_install_refuses() {
    if release_pubkey().is_some() {
        return; // the owner committed the real key: covered by the release workflow
    }
    let f = fx("9.9.9", NEW, Some("9.9.9"));
    run(&f)
        .env_remove("COVERLY_UPDATE_PUBKEY")
        .args(["update", "install"])
        .assert()
        .code(6)
        .stderr(contains("no update key"));
    assert!(same_as_built(&f.exe));
}

#[test]
fn a_bad_channel_is_a_usage_error() {
    let f = fx("9.9.9", NEW, Some("9.9.9"));
    run(&f)
        .args(["update", "check", "--channel", "nightly"])
        .assert()
        .code(2);
}

#[cfg(unix)]
#[test]
fn install_keeps_the_old_binary_when_the_folder_is_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let f = fx("9.9.9", NEW, Some("9.9.9"));
    fs::set_permissions(f.dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let out = run(&f).args(["update", "install"]).assert().code(1);
    fs::set_permissions(f.dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    out.stderr(contains("could not replace"));
    assert!(same_as_built(&f.exe));
    Command::new(&f.exe).arg("--version").assert().success();
}

#[cfg(unix)]
#[test]
fn install_through_a_symlink_replaces_the_real_file_and_keeps_the_link() {
    let f = fx("9.9.9", NEW, Some("9.9.9"));
    let links = TempDir::new().unwrap();
    let link = links.path().join("coverly");
    std::os::unix::fs::symlink(&f.exe, &link).unwrap();
    run_at(&f, &link)
        .args(["update", "install"])
        .assert()
        .success()
        .stdout(contains("Updated to 9.9.9"));
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link is still a link"
    );
    assert_eq!(fs::read(&f.exe).unwrap(), NEW, "the real file was replaced");
    assert_eq!(
        fs::read_dir(links.path()).unwrap().count(),
        1,
        "nothing left next to the link"
    );
}
