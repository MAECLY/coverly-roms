//! `coverly audit`, `coverly profiles` and `coverly systems` end to end.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// The realistic Anbernic-stock SD card of the core tests: Zelda has a
/// cover (in an upper-case `.PNG`), MARIO has none, FF7 is one m3u game
/// over two cue/bin discs, Sonic sits in a subfolder, `APPS` is unknown.
fn library() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("anbernic")).unwrap();
    write(root, "ROMS/SFC/Zelda (USA).sfc", "zelda-rom");
    write(root, "ROMS/SFC/._Zelda (USA).sfc", "appledouble");
    write(root, "ROMS/SFC/MARIO.SMC", "mario");
    write(root, "ROMS/SFC/notes.txt", "notes");
    write(root, "ROMS/SFC/Imgs/zelda (usa).PNG", "png");
    write(root, "ROMS/SFC/Imgs/._Zelda (USA).png", "appledouble");
    write(
        root,
        "ROMS/PS/FF7 (Disc 1).cue",
        "FILE \"FF7 (Disc 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n",
    );
    write(root, "ROMS/PS/FF7 (Disc 1).bin", "disc-one");
    write(
        root,
        "ROMS/PS/FF7 (Disc 2).cue",
        "FILE \"FF7 (Disc 2).bin\" BINARY\n  TRACK 01 MODE2/2352\n",
    );
    write(root, "ROMS/PS/FF7 (Disc 2).bin", "disc-two");
    write(
        root,
        "ROMS/PS/FF7.m3u",
        "FF7 (Disc 1).cue\nFF7 (Disc 2).cue\n",
    );
    write(root, "ROMS/MD/Hacks/Sonic Hack.md", "sonic");
    write(root, "ROMS/APPS/thing.sh", "#!/bin/sh\n");
    dir
}

fn coverly() -> Command {
    let mut cmd = Command::cargo_bin("coverly").unwrap();
    cmd.env("COVERLY_SETTINGS", "");
    cmd
}

fn stdout_of(cmd: &mut Command) -> String {
    let out = cmd.assert().success().get_output().stdout.clone();
    String::from_utf8(out).unwrap()
}

#[test]
fn table_auto_detects_anbernic_stock() {
    let lib = library();
    let text = stdout_of(coverly().arg("audit").arg(lib.path()));
    assert!(text.contains("anbernic-stock"), "{text}");
    let sfc = text
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .find(|f| f.first() == Some(&"SFC"))
        .unwrap_or_else(|| panic!("no SFC row:\n{text}"));
    assert_eq!(sfc[..4], ["SFC", "snes", "2", "1"], "{text}");
    assert!(
        text.contains("MARIO.SMC"),
        "missing entries listed:\n{text}"
    );
}

#[test]
fn table_header_names_each_media_and_marks_unsupported() {
    let lib = library();
    let text = stdout_of(coverly().arg("audit").arg(lib.path()));
    assert!(text.contains("MISSING-COVER"), "{text}");
    assert!(text.contains("MISSING-SCREENSHOT"), "{text}");
    let sfc = text
        .lines()
        .find(|l| l.starts_with("SFC"))
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    // Anbernic stock stores no screenshot: unsupported, not "0 missing".
    assert_eq!(sfc[4], "-", "{text}");
}

#[test]
fn json_parses_and_counts_roms() {
    let lib = library();
    let text = stdout_of(
        coverly()
            .args(["audit", "--format", "json"])
            .arg(lib.path()),
    );
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["totals"]["roms"], 4, "{json}");
    assert_eq!(json["profile_id"], "anbernic-stock");
}

#[test]
fn csv_missing_only_lists_only_roms_missing_art() {
    let lib = library();
    let text = stdout_of(
        coverly()
            .args(["audit", "--format", "csv", "--missing-only"])
            .arg(lib.path()),
    );
    assert!(text.starts_with("system,folder,rel_path"), "{text}");
    assert!(text.contains("MARIO"), "{text}");
    assert!(!text.contains("Zelda"), "{text}");
}

#[test]
fn media_option_restricts_columns() {
    let lib = library();
    let text = stdout_of(
        coverly()
            .args(["audit", "--format", "csv", "--media", "cover,title"])
            .arg(lib.path()),
    );
    let header = text.lines().next().unwrap();
    assert_eq!(
        header,
        "system,folder,rel_path,stem,size,modified,cover,title"
    );
}

#[test]
fn explicit_profile_skips_detection() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "ROMs/snes/Zelda (USA).sfc", "rom");
    write(
        dir.path(),
        "downloaded_media/snes/covers/Zelda (USA).jpg",
        "jpg",
    );
    let text = stdout_of(
        coverly()
            .args(["audit", "--profile", "esde", "--media", "cover"])
            .args(["--format", "json"])
            .arg(dir.path()),
    );
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["profile_id"], "esde");
    assert_eq!(json["totals"]["complete"], 1, "{json}");
}

#[test]
fn output_writes_to_a_file() {
    let lib = library();
    let out = TempDir::new().unwrap();
    let file = out.path().join("audit.json");
    coverly()
        .args(["audit", "--format", "json", "--output"])
        .arg(&file)
        .arg(lib.path())
        .assert()
        .success()
        .stdout("");
    let json: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(json["totals"]["roms"], 4);
}

#[test]
fn audit_never_writes_to_the_library() {
    let lib = library();
    let before = tree(lib.path());
    coverly().arg("audit").arg(lib.path()).assert().success();
    assert_eq!(tree(lib.path()), before);
}

fn tree(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        out.push(path.to_string_lossy().into_owned());
        if path.is_dir() {
            out.extend(tree(&path));
        }
    }
    out.sort();
    out
}

#[test]
fn missing_root_exits_3() {
    let dir = TempDir::new().unwrap();
    coverly()
        .arg("audit")
        .arg(dir.path().join("nope"))
        .assert()
        .code(3)
        .stderr(contains("not found"));
}

#[test]
fn root_that_is_a_file_exits_3() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "file.txt", "x");
    coverly()
        .arg("audit")
        .arg(dir.path().join("file.txt"))
        .assert()
        .code(3);
}

#[test]
fn undetectable_layout_without_profile_exits_4() {
    let dir = TempDir::new().unwrap();
    coverly()
        .arg("audit")
        .arg(dir.path())
        .assert()
        .code(4)
        .stderr(contains("--profile"))
        .stderr(contains("anbernic-stock"));
}

#[test]
fn unknown_profile_is_bad_usage() {
    let lib = library();
    coverly()
        .args(["audit", "--profile", "nope"])
        .arg(lib.path())
        .assert()
        .code(2)
        .stderr(contains("anbernic-stock"));
}

#[test]
fn bad_format_is_bad_usage() {
    let lib = library();
    coverly()
        .args(["audit", "--format", "xml"])
        .arg(lib.path())
        .assert()
        .code(2);
}

#[test]
fn profiles_lists_builtins() {
    let text = stdout_of(coverly().arg("profiles"));
    for id in ["anbernic-stock", "muos", "esde", "batocera", "retroarch"] {
        assert!(text.contains(id), "{id} in\n{text}");
    }
}

#[test]
fn systems_lists_systems_and_aliases() {
    let text = stdout_of(coverly().arg("systems"));
    let snes = text
        .lines()
        .find(|l| l.starts_with("snes "))
        .unwrap_or_else(|| panic!("no snes row:\n{text}"));
    assert!(snes.contains("SFC"), "{snes}");
}
