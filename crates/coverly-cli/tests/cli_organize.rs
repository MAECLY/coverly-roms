//! `coverly organize` end to end on tempdir piles (never the real network:
//! `--offline` or a MockServer through `--database-url`).

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use coverly_core::identify::sniff::fixtures as fx;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

/// `crate::settings::SETTINGS_ENV` of the CLI.
const SETTINGS_ENV: &str = "COVERLY_SETTINGS";

/// `coverly <args…>` with private data/cache dirs and no user settings file.
fn coverly(data: &Path, cache: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("coverly").unwrap();
    cmd.env(SETTINGS_ENV, data.join("no-settings.json"));
    cmd.args(args)
        .arg("--data-dir")
        .arg(data)
        .arg("--cache-dir")
        .arg(cache);
    cmd
}

fn write_old(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    let old = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    filetime::set_file_mtime(path, old).unwrap();
}

fn pile() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_old(&tmp.path().join("Game A.gba"), &fx::gba_tagged(1));
    write_old(&tmp.path().join("Game B.gb"), &fx::gb_tagged(0x00, 2));
    write_old(&tmp.path().join("Odd.gb"), &fx::zeros(32 * 1024));
    fs::write(tmp.path().join("x.zip.crdownload"), b"x").unwrap();
    tmp
}

/// Sorted (relative path, length) of every entry under `root`.
fn tree_digest(root: &Path) -> Vec<(String, u64)> {
    fn visit(root: &Path, dir: &Path, out: &mut Vec<(String, u64)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if meta.is_dir() {
                out.push((format!("{rel}/"), 0));
                visit(root, &path, out);
            } else {
                out.push((rel, meta.len()));
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn dry_run_writes_nothing_and_lists_groups() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let before = tree_digest(p.path());
    coverly(data.path(), cache.path(), &["organize", "--offline"])
        .arg(p.path())
        .assert()
        .success()
        .stdout(contains("ROMS/GBA/"))
        .stdout(contains("Game A.gba"))
        .stdout(contains("To review"))
        .stdout(contains("Odd.gb"))
        .stdout(contains("Staying where they are"))
        .stdout(contains("download not finished"))
        .stdout(contains(
            "Dry run: nothing was written. Re-run with --apply",
        ));
    assert_eq!(tree_digest(p.path()), before);
}

#[test]
fn json_report_has_mode_and_plan() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let before = tree_digest(p.path());
    let out = coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--report", "json"],
    )
    .arg(p.path())
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["mode"], "dry-run");
    assert!(v["job"].is_null());
    assert_eq!(v["plan"]["layout"], "anbernic-stock");
    assert_eq!(v["plan"]["counts"]["auto"], 2);
    assert_eq!(v["plan"]["counts"]["review"], 1);
    assert_eq!(tree_digest(p.path()), before);
}

#[test]
fn output_writes_the_report_to_a_file() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let report = data.path().join("plan.json");
    coverly(
        data.path(),
        cache.path(),
        &[
            "organize",
            "--offline",
            "--report",
            "json",
            "--output",
            report.to_str().unwrap(),
        ],
    )
    .arg(p.path())
    .assert()
    .success()
    .stdout(predicate::str::is_empty());
    let v: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(v["mode"], "dry-run");
}

#[test]
fn unknown_layout_and_bad_assign_are_usage_errors() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--layout", "nope"],
    )
    .arg(p.path())
    .assert()
    .code(2)
    .stderr(contains("anbernic-stock"));
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--assign", "Odd.gb"],
    )
    .arg(p.path())
    .assert()
    .code(2);
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--assign", "Odd.gb=nope"],
    )
    .arg(p.path())
    .assert()
    .code(2)
    .stderr(contains("nope"));
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--assign", "Missing.gb=gb"],
    )
    .arg(p.path())
    .assert()
    .code(2)
    .stderr(contains("not in the plan"));
}

#[test]
fn a_missing_source_is_exit_3() {
    let (data, cache) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    coverly(data.path(), cache.path(), &["organize", "--offline"])
        .arg(data.path().join("does-not-exist"))
        .assert()
        .code(3)
        .stderr(contains("source"));
}

#[test]
fn assign_splits_on_the_last_equals() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    // Distinct from Odd.gb, or it would be its duplicate.
    let mut bytes = fx::zeros(32 * 1024);
    bytes[0x200] = 7;
    write_old(&p.path().join("a=b.gb"), &bytes);
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--assign", "a=b.gb=gbc"],
    )
    .arg(p.path())
    .assert()
    .success()
    .stdout(contains("ROMS/GBC/"))
    .stdout(contains("a=b.gb"))
    .stdout(contains("Chosen by you"));
}

#[test]
fn include_review_accepts_the_best_candidate() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let out = coverly(
        data.path(),
        cache.path(),
        &[
            "organize",
            "--offline",
            "--include-review",
            "--report",
            "json",
        ],
    )
    .arg(p.path())
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["plan"]["counts"]["review"], 0);
    assert_eq!(v["plan"]["counts"]["auto"], 3);
}

#[test]
fn flat_and_target_change_where_the_folders_go() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let target = TempDir::new().unwrap();
    let out = coverly(
        data.path(),
        cache.path(),
        &[
            "organize",
            "--offline",
            "--flat",
            "--copy",
            "--report",
            "json",
            "--target",
            target.path().to_str().unwrap(),
        ],
    )
    .arg(p.path())
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let roms_dir = v["plan"]["roms_dir"].as_str().unwrap();
    assert_eq!(
        Path::new(roms_dir),
        std::path::absolute(target.path()).unwrap()
    );
    assert_eq!(v["plan"]["mode"], "copy");
}

#[test]
fn assign_accepts_a_dot_slash_path() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--assign", "./Odd.gb=gbc"],
    )
    .arg(p.path())
    .assert()
    .success()
    .stdout(contains("ROMS/GBC/"))
    .stdout(contains("Chosen by you"));
}

#[test]
fn an_assign_the_plan_cannot_honour_is_reported() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    // Same bytes as Game A.gba: a duplicate that stays where it is.
    write_old(&p.path().join("Game A copy.gba"), &fx::gba_tagged(1));
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--assign", "Game A copy.gba=gbc"],
    )
    .arg(p.path())
    .assert()
    .success()
    .stderr(contains("Game A copy.gba"))
    .stderr(contains("ignored"));
}

#[test]
fn catalogues_come_from_the_database_url_when_online() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    // A zipped ROM: its CRC is read from the archive, so the catalogue
    // answers without hashing the file.
    let bytes = fx::gba_tagged(9);
    let crc = format!("{:08X}", crc32fast::hash(&bytes));
    let zip_path = p.path().join("Game Z.zip");
    {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("Game Z.gba", opts).unwrap();
        zip.write_all(&bytes).unwrap();
        zip.finish().unwrap();
    }
    let old = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    filetime::set_file_mtime(&zip_path, old).unwrap();
    let server = coverly_core::net::testing::MockServer::start();
    server.serve_dat(
        "metadat/no-intro/Nintendo - Game Boy Advance.dat",
        &format!(
            "game (\n name \"Game Z (USA)\"\n rom ( name \"Game Z (USA).gba\" size {} crc {crc} )\n)\n",
            bytes.len()
        ),
    );
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--database-url", &server.base()],
    )
    .arg(p.path())
    .assert()
    .success()
    .stdout(contains(format!(
        "CRC {crc} in the gba catalogue: Game Z (USA)"
    )));
}
