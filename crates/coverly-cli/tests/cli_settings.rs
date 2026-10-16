//! The CLI reads the app's settings file; flags override it.

use std::fs;

use assert_cmd::Command;
use coverly_core::net::testing::{png_bytes, MockServer};
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const GBA: &str = "Nintendo - Game Boy Advance";

struct Fx {
    lib: TempDir,
    cfg: TempDir,
    cache: TempDir,
    data: TempDir,
    server: MockServer,
}

fn fx() -> Fx {
    let lib = TempDir::new().unwrap();
    fs::create_dir_all(lib.path().join("anbernic")).unwrap();
    fs::create_dir_all(lib.path().join("ROMS/GBA/Junk")).unwrap();
    fs::write(lib.path().join("ROMS/GBA/Golden Sun.gba"), b"gs").unwrap();
    fs::write(lib.path().join("ROMS/GBA/Junk/Other.gba"), b"o").unwrap();
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA]);
    server.serve_libretro_index(GBA, "Named_Boxarts", &["Golden Sun (USA, Europe).png"]);
    server.serve_libretro_image(
        GBA,
        "Named_Boxarts",
        "Golden Sun (USA, Europe).png",
        png_bytes(1000, 1400),
    );
    Fx {
        lib,
        cfg: TempDir::new().unwrap(),
        cache: TempDir::new().unwrap(),
        data: TempDir::new().unwrap(),
        server,
    }
}

fn settings(f: &Fx, json: &str) {
    fs::write(f.cfg.path().join("settings.json"), json).unwrap();
}

fn coverly(f: &Fx) -> Command {
    let mut c = Command::cargo_bin("coverly").unwrap();
    c.env("COVERLY_SETTINGS", f.cfg.path().join("settings.json"));
    c
}

fn fill(f: &Fx) -> Command {
    let mut c = coverly(f);
    c.arg("fill").arg(f.lib.path());
    c.arg("--cache-dir")
        .arg(f.cache.path())
        .arg("--data-dir")
        .arg(f.data.path());
    c.arg("--libretro-url").arg(f.server.base());
    c.arg("--libretro-raw-url").arg(f.server.url("/raw"));
    c.arg("--database-url").arg(f.server.base());
    c
}

fn cover(f: &Fx) -> std::path::PathBuf {
    f.lib.path().join("ROMS/GBA/Imgs/Golden Sun.png")
}

#[test]
fn dry_run_default_off_applies_and_the_flag_still_wins() {
    let f = fx();
    settings(&f, r#"{"safety":{"dry_run_default":false}}"#);
    fill(&f)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("Dry run"));
    assert!(!cover(&f).exists());
    fill(&f).assert().success().stdout(contains("filled 1"));
    assert!(cover(&f).exists());
}

#[test]
fn flags_override_the_strategy_from_settings() {
    let f = fx();
    settings(&f, r#"{"matching":{"strategy":"priority"}}"#);
    fill(&f)
        .args(["--report", "json"])
        .assert()
        .success()
        .stdout(contains("\"strategy\": \"priority\""));
    fill(&f)
        .args(["--report", "json", "--strategy", "rom-region"])
        .assert()
        .success()
        .stdout(contains("\"strategy\": \"rom-region\""));
}

#[test]
fn ignored_folders_from_settings_apply_to_audit_and_fill() {
    let f = fx();
    coverly(&f)
        .arg("audit")
        .arg(f.lib.path())
        .args(["--format", "json"])
        .assert()
        .success()
        .stdout(contains("Other"));
    settings(&f, r#"{"library":{"ignored_dirs":["Junk"]}}"#);
    coverly(&f)
        .arg("audit")
        .arg(f.lib.path())
        .args(["--format", "json"])
        .assert()
        .success()
        .stdout(contains("Other").not());
}

#[test]
fn the_default_profile_can_be_a_custom_one() {
    let f = fx();
    let plain = TempDir::new().unwrap();
    fs::create_dir_all(plain.path().join("Games/GBA")).unwrap();
    fs::write(plain.path().join("Games/GBA/Golden Sun.gba"), b"gs").unwrap();
    fs::create_dir_all(f.cfg.path().join("profiles")).unwrap();
    fs::write(
        f.cfg.path().join("profiles/my-sd.toml"),
        "id = \"my-sd\"\nname = \"My SD\"\nroms_root = \"Games\"\n[slots.cover]\ntemplate = \"{system_dir}/covers/{stem}.png\"\n",
    )
    .unwrap();
    // Without a default: nothing detected (exit 4).
    coverly(&f).arg("audit").arg(plain.path()).assert().code(4);
    settings(&f, r#"{"library":{"default_profile":"my-sd"}}"#);
    coverly(&f)
        .arg("audit")
        .arg(plain.path())
        .assert()
        .success()
        .stdout(contains("my-sd"));
    coverly(&f)
        .arg("profiles")
        .assert()
        .success()
        .stdout(contains("my-sd"))
        .stdout(contains("custom"));
}

#[test]
fn a_corrupt_settings_file_is_read_as_defaults_and_left_alone() {
    let f = fx();
    settings(&f, "{ garbage");
    coverly(&f)
        .arg("audit")
        .arg(f.lib.path())
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(f.cfg.path().join("settings.json")).unwrap(),
        "{ garbage"
    );
    assert_eq!(
        fs::read_dir(f.cfg.path()).unwrap().count(),
        1,
        "no backup files from the CLI"
    );
}

#[test]
fn an_empty_variable_means_built_in_defaults() {
    let f = fx();
    settings(&f, r#"{"library":{"ignored_dirs":["Junk"]}}"#);
    let mut c = Command::cargo_bin("coverly").unwrap();
    c.env("COVERLY_SETTINGS", "")
        .arg("audit")
        .arg(f.lib.path())
        .args(["--format", "json"]);
    c.assert().success().stdout(contains("Other"));
}

#[test]
fn a_deleted_default_profile_falls_back_to_detection() {
    let f = fx();
    settings(&f, r#"{"library":{"default_profile":"gone"}}"#);
    coverly(&f)
        .arg("audit")
        .arg(f.lib.path())
        .assert()
        .success()
        .stdout(contains("auto-detected"));
}

#[test]
fn undo_reads_jobs_from_the_backup_location_setting() {
    let f = fx();
    let backups = TempDir::new().unwrap();
    let json = serde_json::json!({
        "safety": { "dry_run_default": false, "backup_dir": backups.path() }
    });
    settings(&f, &json.to_string());
    // No --data-dir: manual identities are read from the platform data
    // dir, so it is moved under a temp home.
    let home = TempDir::new().unwrap();
    let mut c = coverly(&f);
    c.env("HOME", home.path());
    c.arg("fill").arg(f.lib.path());
    c.arg("--cache-dir").arg(f.cache.path());
    c.arg("--libretro-url").arg(f.server.base());
    c.arg("--libretro-raw-url").arg(f.server.url("/raw"));
    c.arg("--database-url").arg(f.server.base());
    c.assert().success().stdout(contains("filled 1"));
    assert!(fs::read_dir(f.data.path()).unwrap().next().is_none());
    coverly(&f)
        .args(["undo", "--list"])
        .assert()
        .success()
        .stdout(contains("done"));
    coverly(&f)
        .arg("undo")
        .assert()
        .success()
        .stdout(contains("removed 1"));
    assert!(!cover(&f).exists());
}
