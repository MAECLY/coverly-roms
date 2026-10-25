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

/// Sends trashed archives to `<data>/trash` instead of the OS Trash
/// (honoured only by test builds of the core).
const FAKE_TRASH_ENV: &str = "COVERLY_TEST_FAKE_TRASH";

/// `coverly <args…>` with private data/cache dirs and no user settings file.
fn coverly(data: &Path, cache: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("coverly").unwrap();
    cmd.env(SETTINGS_ENV, data.join("no-settings.json"))
        .env(FAKE_TRASH_ENV, data.join("trash"));
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
    // Same bytes as Game A.gba, and newer: in its system it is the copy
    // set aside, so the assign changes nothing. (Another system is never
    // compared: an assign to gbc would be honoured.)
    let copy = p.path().join("Game A copy.gba");
    write_old(&copy, &fx::gba_tagged(1));
    filetime::set_file_mtime(&copy, filetime::FileTime::from_unix_time(1_600_000_100, 0)).unwrap();
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--assign", "Game A copy.gba=gba"],
    )
    .arg(p.path())
    .assert()
    .success()
    .stderr(contains("Game A copy.gba"))
    .stderr(contains(
        "ignored: it is set aside as a copy of ROMS/GBA/Game A.gba",
    ));
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

/// `coverly undo <args…>` with the same private data dir.
fn undo(data: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("coverly").unwrap();
    cmd.env(SETTINGS_ENV, data.join("no-settings.json"))
        .env(FAKE_TRASH_ENV, data.join("trash"));
    cmd.arg("undo").args(args).arg("--data-dir").arg(data);
    cmd
}

fn json_of(out: &[u8]) -> serde_json::Value {
    serde_json::from_slice(out).unwrap()
}

#[test]
fn apply_moves_and_undo_restores() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let before = tree_digest(p.path());
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--apply"],
    )
    .arg(p.path())
    .assert()
    .success()
    .stdout(contains("moved 2"))
    .stdout(contains("Open it with: coverly fill"))
    .stdout(contains("--profile anbernic-stock"))
    .stdout(contains("Dry run").not());
    assert!(p.path().join("ROMS/GBA/Game A.gba").is_file());
    assert!(p.path().join("ROMS/GB/Game B.gb").is_file());
    assert!(p.path().join("Odd.gb").is_file());
    assert!(!p.path().join("Game A.gba").exists());
    undo(data.path(), &[])
        .assert()
        .success()
        .stdout(contains("moved back 2"))
        .stdout(contains("folders removed"));
    assert_eq!(tree_digest(p.path()), before);
}

#[test]
fn apply_json_report_carries_the_job() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let out = coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--apply", "--report", "json"],
    )
    .arg(p.path())
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let v = json_of(&out);
    assert_eq!(v["mode"], "apply");
    assert_eq!(v["job"]["moved"], 2);
    assert!(v["job"]["job"].is_string());
}

#[test]
fn copy_to_another_target_keeps_the_pile() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let other = TempDir::new().unwrap();
    let before = tree_digest(p.path());
    coverly(
        data.path(),
        cache.path(),
        &[
            "organize",
            "--offline",
            "--copy",
            "--apply",
            "--target",
            other.path().to_str().unwrap(),
        ],
    )
    .arg(p.path())
    .assert()
    .success()
    .stdout(contains("copied 2"));
    assert_eq!(tree_digest(p.path()), before);
    assert!(other.path().join("ROMS/GBA/Game A.gba").is_file());
    undo(data.path(), &[])
        .assert()
        .success()
        .stdout(contains("removed 2 copies"));
    assert!(!other.path().join("ROMS/GBA/Game A.gba").exists());
    assert_eq!(tree_digest(p.path()), before);
}

#[test]
fn include_review_applies_the_best_candidate() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--apply", "--include-review"],
    )
    .arg(p.path())
    .assert()
    .success()
    .stdout(contains("moved 3"));
    assert!(!p.path().join("Odd.gb").exists());
    assert!(p.path().join("x.zip.crdownload").is_file());
}

#[test]
fn undo_list_shows_organise_jobs() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--apply"],
    )
    .arg(p.path())
    .assert()
    .success();
    undo(data.path(), &["--list"])
        .assert()
        .success()
        .stdout(contains("KIND"))
        .stdout(contains("organize"));
}

#[test]
fn resume_of_an_unknown_job_is_an_error() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--resume", "20990101T000000Z-1-1"],
    )
    .arg(p.path())
    .assert()
    .code(1)
    .stderr(contains("20990101T000000Z-1-1"));
}

#[cfg(unix)]
#[test]
fn a_read_only_target_is_exit_5_and_resume_finishes() {
    use std::os::unix::fs::PermissionsExt;
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let other = TempDir::new().unwrap();
    let set_mode = |mode| {
        fs::set_permissions(other.path(), fs::Permissions::from_mode(mode)).unwrap();
    };
    set_mode(0o555);
    let out = coverly(
        data.path(),
        cache.path(),
        &[
            "organize",
            "--offline",
            "--apply",
            "--report",
            "json",
            "--target",
            other.path().to_str().unwrap(),
        ],
    )
    .arg(p.path())
    .assert()
    .code(5)
    .stderr(contains("paused"))
    .stderr(contains("--resume"))
    .get_output()
    .clone();
    set_mode(0o755);
    let v = json_of(&out.stdout);
    let job = v["job"]["job"]
        .as_str()
        .expect("a paused job has an id")
        .to_string();
    assert!(String::from_utf8_lossy(&out.stderr).contains(&job));
    coverly(data.path(), cache.path(), &["organize", "--resume", &job])
        .arg(p.path())
        .assert()
        .success()
        .stdout(contains("moved 2"));
    assert!(other.path().join("ROMS/GBA/Game A.gba").is_file());
    assert!(other.path().join("ROMS/GB/Game B.gb").is_file());
    assert!(!p.path().join("Game A.gba").exists());
}

#[test]
fn resume_refuses_options_that_shape_a_new_plan() {
    // The saved plan decides layout, target and mode: a `--copy` given
    // with `--resume` must not be silently ignored (the job would move).
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let before = tree_digest(p.path());
    for extra in [
        &["--copy"][..],
        &["--move"],
        &["--layout", "es-de"],
        &["--target", "/tmp"],
        &["--flat"],
        &["--names", "catalogue"],
        &["--assign", "Odd.gb=gb"],
        &["--include-review"],
    ] {
        let mut args = vec!["organize", "--resume", "20990101T000000Z-1-1"];
        args.extend_from_slice(extra);
        coverly(data.path(), cache.path(), &args)
            .arg(p.path())
            .assert()
            .code(2);
    }
    assert_eq!(tree_digest(p.path()), before);
}

/// The job id of an applied `organize … --report json` run.
fn applied_job(out: &[u8]) -> String {
    json_of(out)["job"]["job"].as_str().unwrap().to_string()
}

#[test]
fn undo_of_an_organise_job_with_later_jobs_refuses_unless_include_later() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let other = TempDir::new().unwrap();
    let sub = other.path().join("sub");
    fs::create_dir(&sub).unwrap();
    let copy_into = |target: &Path| {
        let out = coverly(
            data.path(),
            cache.path(),
            &[
                "organize",
                "--offline",
                "--copy",
                "--apply",
                "--report",
                "json",
                "--target",
                target.to_str().unwrap(),
            ],
        )
        .arg(p.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
        applied_job(&out)
    };
    let first = copy_into(other.path());
    // Job ids sort by their second: keep the second job in a later one.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let second = copy_into(&sub);
    undo(data.path(), &["--job", &first])
        .assert()
        .code(1)
        .stderr(contains(second.as_str()))
        .stderr(contains("--include-later"));
    assert!(other.path().join("ROMS/GBA/Game A.gba").is_file());
    assert!(sub.join("ROMS/GBA/Game A.gba").is_file());
    undo(data.path(), &["--job", &first, "--include-later"])
        .assert()
        .success()
        .stdout(contains(first.as_str()))
        .stdout(contains(second.as_str()));
    assert!(!other.path().join("ROMS").exists());
    assert!(!sub.join("ROMS").exists());
}

// --- Archives and disc games (archives spec §12) ---

mod archives {
    use super::*;
    use coverly_core::archive::testing::{sevenz, SevenZShape};
    use coverly_core::disc::testing as dt;

    const PS1_DIR: &str = "Next Tetris, The (USA)";
    const PS1_CUE: &str = "Next Tetris, The (USA).cue";
    const PS1_T1: &str = "Next Tetris, The (USA) (Track 01).bin";
    const PS1_T2: &str = "Next Tetris, The (USA) (Track 02).bin";
    /// The archive of the real case: a 7z named `.zip`.
    const ARCHIVE: &str = "Next Tetris, The.zip";

    fn cue(files: &[&str]) -> Vec<u8> {
        let mut s = String::new();
        for (i, f) in files.iter().enumerate() {
            let mode = if i == 0 { "MODE2/2352" } else { "AUDIO" };
            s.push_str(&format!(
                "FILE \"{f}\" BINARY\r\n  TRACK {:02} {mode}\r\n    INDEX 01 00:00:00\r\n",
                i + 1
            ));
        }
        s.into_bytes()
    }

    /// A pile holding the synthetic PS1 disc of the real case (a cue and
    /// two tracks in a 7z misnamed `.zip`, no copyrighted bytes).
    fn disc_pile() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let sheet = format!("{PS1_DIR}/{PS1_CUE}");
        let t1 = format!("{PS1_DIR}/{PS1_T1}");
        let t2 = format!("{PS1_DIR}/{PS1_T2}");
        let track1 = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
        let track2 = vec![0u8; 2352 * 4];
        let files: [(&str, &[u8]); 3] = [
            (&sheet, &cue(&[PS1_T1, PS1_T2])),
            (&t1, &track1),
            (&t2, &track2),
        ];
        write_old(
            &tmp.path().join(ARCHIVE),
            &sevenz(&files, SevenZShape::non_solid_lzma()),
        );
        tmp
    }

    fn dry_run_json(pile: &Path, extra: &[&str]) -> serde_json::Value {
        let (data, cache) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let mut args = vec!["organize", "--offline", "--report", "json"];
        args.extend_from_slice(extra);
        let out = coverly(data.path(), cache.path(), &args)
            .arg(pile)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        json_of(&out)
    }

    fn item_named<'a>(v: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
        v["plan"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["sources"][0].as_str().unwrap().ends_with(name))
            .unwrap_or_else(|| panic!("no item {name} in {v:#}"))
    }

    #[test]
    fn dry_run_shows_extract_with_sizes() {
        let (p, data, cache) = (
            disc_pile(),
            TempDir::new().unwrap(),
            TempDir::new().unwrap(),
        );
        let before = tree_digest(p.path());
        let out = coverly(data.path(), cache.path(), &["organize", "--offline"])
            .arg(p.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("extract 3 files 66.0 kB → ROMS/PS/"),
            "{text}"
        );
        assert!(text.contains("it is a 7z archive named .zip"), "{text}");
        assert!(text.contains("Extracting: 1 game (66.0 kB)"), "{text}");
        assert!(text.contains("Dry run: nothing was written"), "{text}");
        assert_eq!(tree_digest(p.path()), before);
    }

    #[test]
    fn report_json_has_transfer_and_disc() {
        let p = disc_pile();
        let v = dry_run_json(p.path(), &[]);
        let item = item_named(&v, ARCHIVE);
        assert_eq!(item["system_id"], "psx");
        assert_eq!(item["transfer"]["kind"], "extract");
        assert_eq!(item["transfer"]["entries"].as_array().unwrap().len(), 3);
        assert_eq!(item["transfer"]["archive_after"], "trash");
        let serials = item["disc"]["serials"].as_array().unwrap();
        assert!(serials.iter().any(|s| s == "SLUS-00862"), "{item:#}");
    }

    #[test]
    fn copy_flag_keeps_archive() {
        let (p, data, cache) = (
            disc_pile(),
            TempDir::new().unwrap(),
            TempDir::new().unwrap(),
        );
        let other = TempDir::new().unwrap();
        let before = tree_digest(p.path());
        coverly(
            data.path(),
            cache.path(),
            &[
                "organize",
                "--offline",
                "--copy",
                "--apply",
                "--target",
                other.path().to_str().unwrap(),
            ],
        )
        .arg(p.path())
        .assert()
        .success()
        .stdout(contains("Extracted: 1"));
        assert_eq!(tree_digest(p.path()), before);
        for name in [PS1_CUE, PS1_T1, PS1_T2] {
            assert!(other.path().join("ROMS/PS").join(name).is_file(), "{name}");
        }
        assert!(!data.path().join("trash").exists());
    }

    #[test]
    fn apply_then_undo_restores_archive() {
        let (p, data, cache) = (
            disc_pile(),
            TempDir::new().unwrap(),
            TempDir::new().unwrap(),
        );
        let before = tree_digest(p.path());
        coverly(
            data.path(),
            cache.path(),
            &["organize", "--offline", "--apply"],
        )
        .arg(p.path())
        .assert()
        .success()
        .stdout(contains("Extracted: 1"))
        .stdout(contains("Undo with: coverly undo --job"));
        assert!(p.path().join("ROMS/PS").join(PS1_CUE).is_file());
        assert!(!p.path().join(ARCHIVE).exists(), "moved to the Trash");
        assert!(data.path().join("trash").is_dir(), "the fake Trash got it");
        undo(data.path(), &[])
            .assert()
            .success()
            .stdout(contains("archive restored: "))
            .stdout(contains(ARCHIVE));
        assert_eq!(tree_digest(p.path()), before);
    }

    #[test]
    fn no_external_tools_rar_left_with_hint() {
        let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
        let mut rar = b"Rar!\x1a\x07\x01\x00".to_vec();
        rar.extend_from_slice(&[0u8; 64]);
        write_old(&p.path().join("Some Game (USA).rar"), &rar);
        coverly(
            data.path(),
            cache.path(),
            &["organize", "--offline", "--no-external-tools"],
        )
        .arg(p.path())
        .assert()
        .success()
        .stdout(contains("Some Game (USA).rar"))
        .stdout(contains(
            "to open RAR archives, install unrar (external programs are off)",
        ))
        // The rest of the plan is unaffected.
        .stdout(contains("ROMS/GBA/"));
    }

    #[test]
    fn hostile_entry_name_is_terminal_safe() {
        let p = TempDir::new().unwrap();
        let (data, cache) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let gba = fx::gba_tagged(3);
        let files: [(&str, &[u8]); 2] = [
            ("Game\u{1b}]0;pwned\u{7}\r.gba", &gba),
            ("Game\u{9b}2J.gba", &gba),
        ];
        write_old(
            &p.path().join("Hostile.7z"),
            &sevenz(&files, SevenZShape::non_solid_lzma()),
        );
        let out = coverly(data.path(), cache.path(), &["organize", "--offline"])
            .arg(p.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Hostile.7z"), "{text}");
        assert!(
            !text.chars().any(|c| c != '\n' && c.is_control()),
            "control characters reached the terminal: {text:?}"
        );
    }

    /// A name from inside the archive is printed when it is why the
    /// archive stays (the cue lists a track it lacks): no escape in it may
    /// reach the terminal.
    #[test]
    fn hostile_sheet_name_is_terminal_safe() {
        let p = TempDir::new().unwrap();
        let (data, cache) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let track = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
        let hostile = "Game\u{1b}]0;pwned\u{7}.bin";
        let sheet = cue(&[hostile]);
        let files: [(&str, &[u8]); 2] = [("Game.cue", &sheet), (hostile, &track)];
        write_old(
            &p.path().join("Evil.7z"),
            &sevenz(&files, SevenZShape::non_solid_lzma()),
        );
        let out = coverly(data.path(), cache.path(), &["organize", "--offline"])
            .arg(p.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("game files missing: Game"), "{text}");
        assert!(text.contains("]0;pwned"), "the name is shown: {text}");
        assert!(
            !text.chars().any(|c| c != '\n' && c.is_control()),
            "control characters reached the terminal: {text:?}"
        );
    }
}

/// A pile that already holds a library (`ROMS/GB`) with one game, and a
/// loose copy of that game under another name.
fn pile_with_a_copy() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("ROMS/GB")).unwrap();
    let rom = fx::gb_tagged(0x00, 41);
    write_old(&tmp.path().join("ROMS/GB/Game B (USA).gb"), &rom);
    write_old(&tmp.path().join("Game B copy.gb"), &rom);
    write_old(&tmp.path().join("Game A.gba"), &fx::gba_tagged(42));
    tmp
}

#[test]
fn cli_organize_duplicates_flag() {
    let (p, data, cache) = (
        pile_with_a_copy(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    // Default: set aside in the pile's quarantine.
    coverly(data.path(), cache.path(), &["organize", "--offline"])
        .arg(p.path())
        .assert()
        .success()
        .stdout(contains("Duplicates set aside: 1 · Already in your library: 1"))
        .stdout(contains("Duplicates set aside (1), to _Duplicados/"))
        .stdout(contains(
            "Game B copy.gb  exact copy  → _Duplicados/Game B copy.gb  [stays: ROMS/GB/Game B (USA).gb",
        ));
    // Report only: it stays where it is, with the reason.
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--duplicates", "report"],
    )
    .arg(p.path())
    .assert()
    .success()
    .stdout(contains(
        "Game B copy.gb  already in your library as ROMS/GB/Game B (USA).gb (exact copy)",
    ))
    .stdout(contains("Duplicates set aside (").not());
    // Off: the old behaviour, the copy moves in.
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--duplicates", "off"],
    )
    .arg(p.path())
    .assert()
    .success()
    .stdout(contains("Game B copy.gb → ROMS/GB/Game B copy.gb"));
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--duplicates", "maybe"],
    )
    .arg(p.path())
    .assert()
    .code(2);
}

#[test]
fn cli_organize_sets_aside_and_undo_restores() {
    let (p, data, cache) = (
        pile_with_a_copy(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let before = tree_digest(p.path());
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--apply"],
    )
    .arg(p.path())
    .assert()
    .success()
    .stdout(contains("moved 1"))
    .stdout(contains("Duplicates set aside: 1 (in "));
    assert!(p.path().join("_Duplicados/Game B copy.gb").is_file());
    assert!(p.path().join("ROMS/GBA/Game A.gba").is_file());
    assert!(!p.path().join("ROMS/GB/Game B copy.gb").exists());
    undo(data.path(), &[]).assert().success();
    assert_eq!(tree_digest(p.path()), before);
}
