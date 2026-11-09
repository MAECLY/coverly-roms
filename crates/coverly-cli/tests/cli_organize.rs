//! `coverly organize` end to end on tempdir piles (never the real network:
//! `--offline` or a MockServer through `--database-url`).

mod common;

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
    let mut cmd = common::coverly();
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
    let mut cmd = common::coverly();
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
fn undo_names_a_created_folder_it_keeps() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--apply"],
    )
    .arg(p.path())
    .assert()
    .success();
    // The user adds a save to a folder the job created.
    write_old(&p.path().join("ROMS/GB/Game B.sav"), b"save");
    undo(data.path(), &[])
        .assert()
        .success()
        .stdout(contains("moved back 2"))
        .stdout(contains(format!(
            "kept folder {}; it holds files that are not the job's",
            p.path().join("ROMS").join("GB").display()
        )));
    assert!(p.path().join("ROMS/GB/Game B.sav").is_file());
    assert!(p.path().join("Game B.gb").is_file());
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

#[test]
fn cli_organize_names_the_library_quarantine_for_a_pile_inside_a_system_folder() {
    let (p, data, cache) = (
        pile_with_a_copy(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let rom = fx::gb_tagged(0x00, 41);
    fs::create_dir_all(p.path().join("ROMS/GB/incoming")).unwrap();
    write_old(&p.path().join("ROMS/GB/incoming/Game B again.gb"), &rom);
    let incoming = p.path().join("ROMS/GB/incoming");
    let target = format!("--target={}", p.path().display());
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", &target],
    )
    .arg(&incoming)
    .assert()
    .success()
    .stdout(contains(format!(
        "Duplicates set aside (1), to {}",
        p.path()
            .join("_Duplicados")
            .join("GB")
            .join("incoming")
            .display()
    )));
}

/// `coverly_core::organize::apply::LARGE_TRANSFER_ENV`: a test build's
/// large-transfer threshold in bytes, so no test writes 10 GB.
const LARGE_TRANSFER_ENV: &str = "COVERLY_TEST_LARGE_TRANSFER";

fn copy_args(target: &Path) -> Vec<String> {
    ["organize", "--offline", "--copy", "--apply", "--target"]
        .iter()
        .map(|s| s.to_string())
        .chain([target.to_str().unwrap().to_string()])
        .collect()
}

#[test]
fn a_large_apply_without_yes_large_writes_nothing_and_fails_like_no_space() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let other = TempDir::new().unwrap();
    let before = tree_digest(p.path());
    let args = copy_args(other.path());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let volume = other.path().file_name().unwrap().to_string_lossy();
    coverly(data.path(), cache.path(), &args)
        .env(LARGE_TRANSFER_ENV, "1")
        .arg(p.path())
        .assert()
        .code(1)
        .stderr(contains("bytes (≥ 10 GB) to "))
        .stderr(contains(volume.as_ref()))
        .stderr(contains(
            "re-run with --yes-large to confirm; nothing was written",
        ));
    assert_eq!(tree_digest(p.path()), before);
    assert!(tree_digest(other.path()).is_empty(), "nothing written");
    assert!(!data.path().join("jobs").exists(), "no job started");

    // Confirmed: it copies.
    let mut confirmed = args.clone();
    confirmed.push("--yes-large");
    coverly(data.path(), cache.path(), &confirmed)
        .env(LARGE_TRANSFER_ENV, "1")
        .arg(p.path())
        .assert()
        .success()
        .stdout(contains("copied 2"));
    assert!(other.path().join("ROMS/GBA/Game A.gba").is_file());
    assert_eq!(tree_digest(p.path()), before);
}

#[test]
fn yes_large_needs_apply() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    coverly(
        data.path(),
        cache.path(),
        &["organize", "--offline", "--yes-large"],
    )
    .arg(p.path())
    .assert()
    .code(2);
}

#[test]
fn a_small_apply_needs_no_yes_large() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let other = TempDir::new().unwrap();
    let args = copy_args(other.path());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    coverly(data.path(), cache.path(), &args)
        .arg(p.path())
        .assert()
        .success()
        .stdout(contains("copied 2"));
}

#[cfg(unix)]
#[test]
fn resume_of_a_confirmed_large_job_needs_no_flag() {
    use std::os::unix::fs::PermissionsExt;
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let other = TempDir::new().unwrap();
    let set_mode = |mode| {
        fs::set_permissions(other.path(), fs::Permissions::from_mode(mode)).unwrap();
    };
    set_mode(0o555);
    let mut args = copy_args(other.path());
    args.extend(["--yes-large", "--report", "json"].map(String::from));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = coverly(data.path(), cache.path(), &args)
        .env(LARGE_TRANSFER_ENV, "1")
        .arg(p.path())
        .assert()
        .code(5)
        .get_output()
        .clone();
    set_mode(0o755);
    let job = json_of(&out.stdout)["job"]["job"]
        .as_str()
        .expect("a paused job has an id")
        .to_string();
    coverly(data.path(), cache.path(), &["organize", "--resume", &job])
        .env(LARGE_TRANSFER_ENV, "1")
        .arg(p.path())
        .assert()
        .success()
        .stdout(contains("copied 2"));
    assert!(other.path().join("ROMS/GBA/Game A.gba").is_file());
}

/// `COVERLY_TEST_PROMPT` of the CLI: a test build answers the per-item
/// questions from this list (comma separated, then EOF), as if stdin and
/// stderr were terminals.
const TEST_PROMPT_ENV: &str = "COVERLY_TEST_PROMPT";

/// Between a GB (32 KiB) and a GBA (64 KiB) file: each GBA is huge on its
/// own, the GBs are not, and two GBs together reach it.
const HUGE_AT: &str = "40000";

/// Three GBA files (huge with [`HUGE_AT`]) and, with `rest`, two GB files.
fn big_pile(rest: bool) -> TempDir {
    let tmp = TempDir::new().unwrap();
    for (i, name) in ["Big 1.gba", "Big 2.gba", "Big 3.gba"].iter().enumerate() {
        write_old(&tmp.path().join(name), &fx::gba_tagged(10 + i as u8));
    }
    if rest {
        write_old(&tmp.path().join("Small 1.gb"), &fx::gb_tagged(0x00, 21));
        write_old(&tmp.path().join("Small 2.gb"), &fx::gb_tagged(0x00, 22));
    }
    tmp
}

/// The names asked about, in order (`Copy "<name>" (…`).
fn asked(stderr: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter_map(|l| l.strip_prefix("Copy \""))
        .filter_map(|l| l.split_once('"').map(|(n, _)| n.to_string()))
        .collect()
}

fn big_args(target: &Path, extra: &[&str]) -> Vec<String> {
    let mut args = copy_args(target);
    args.extend(["--report", "json"].map(String::from));
    args.extend(extra.iter().map(|s| s.to_string()));
    args
}

fn run_big(
    pile: &Path,
    target: &Path,
    data: &Path,
    cache: &Path,
    extra: &[&str],
    script: Option<&str>,
) -> assert_cmd::assert::Assert {
    let args = big_args(target, extra);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut cmd = coverly(data, cache, &args);
    cmd.env(LARGE_TRANSFER_ENV, HUGE_AT);
    if let Some(s) = script {
        cmd.env(TEST_PROMPT_ENV, s);
    }
    cmd.arg(pile).assert()
}

#[test]
fn each_huge_item_is_asked_and_no_yes_all_follow_the_answers() {
    let (p, data, cache) = (
        big_pile(false),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let other = TempDir::new().unwrap();
    let out = run_big(
        p.path(),
        other.path(),
        data.path(),
        cache.path(),
        &[],
        Some("n,y,a"),
    )
    .success()
    .get_output()
    .clone();
    let names = asked(&out.stderr);
    assert_eq!(names.len(), 3, "{}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("free on "), "{stderr}");
    assert!(
        stderr.contains("after this)? [y]es / [a]ll / [n]o:"),
        "{stderr}"
    );
    assert!(!stderr.contains("Write "), "no rest question: {stderr}");
    // Prompts on stderr only: stdout is the JSON report.
    let report = json_of(&out.stdout);
    assert_eq!(report["mode"], "apply");
    let gba = other.path().join("ROMS/GBA");
    assert!(!gba.join(&names[0]).exists(), "declined");
    assert!(gba.join(&names[1]).is_file());
    assert!(gba.join(&names[2]).is_file());
    assert!(p.path().join(&names[0]).is_file());
    let left = report["plan"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["sources"][0].as_str().unwrap().ends_with(&names[0]))
        .unwrap()
        .clone();
    assert_eq!(left["reason"]["kind"], "not-confirmed", "{left}");
}

#[test]
fn yes_to_all_also_confirms_the_rest_and_the_rest_is_asked_otherwise() {
    let (p, data, cache) = (
        big_pile(true),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let other = TempDir::new().unwrap();
    // «All» at the first item: nothing else is asked.
    let out = run_big(
        p.path(),
        other.path(),
        data.path(),
        cache.path(),
        &[],
        Some("a"),
    )
    .success()
    .get_output()
    .clone();
    assert_eq!(asked(&out.stderr).len(), 1);
    assert!(other.path().join("ROMS/GB/Small 1.gb").is_file());

    // Yes to each: then the rest (two GB files, over the threshold) is
    // asked once; «no» there writes nothing.
    let other = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let out = run_big(
        p.path(),
        other.path(),
        data.path(),
        cache.path(),
        &[],
        Some("y,y,y,n"),
    )
    .code(1)
    .get_output()
    .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(asked(&out.stderr).len(), 3);
    assert!(stderr.contains("Write 65.5 kB more to "), "{stderr}");
    assert!(stderr.contains("nothing was written"), "{stderr}");
    assert!(tree_digest(other.path()).is_empty());

    let out = run_big(
        p.path(),
        other.path(),
        data.path(),
        cache.path(),
        &[],
        Some("y,y,y,y"),
    )
    .success()
    .get_output()
    .clone();
    assert!(String::from_utf8_lossy(&out.stderr).contains("Write 65.5 kB more to "));
    assert!(other.path().join("ROMS/GB/Small 2.gb").is_file());
    assert!(other.path().join("ROMS/GBA/Big 3.gba").is_file());
}

#[test]
fn eof_at_a_question_writes_nothing_and_refuses_like_today() {
    let (p, data, cache) = (
        big_pile(false),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let other = TempDir::new().unwrap();
    let before = tree_digest(p.path());
    run_big(
        p.path(),
        other.path(),
        data.path(),
        cache.path(),
        &[],
        Some(""),
    )
    .code(1)
    .stderr(contains(
        "re-run with --yes-large to confirm; nothing was written",
    ));
    assert_eq!(tree_digest(p.path()), before);
    assert!(tree_digest(other.path()).is_empty());
    assert!(!data.path().join("jobs").exists());
}

#[test]
fn yes_large_asks_nothing() {
    let (p, data, cache) = (
        big_pile(true),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let other = TempDir::new().unwrap();
    let out = run_big(
        p.path(),
        other.path(),
        data.path(),
        cache.path(),
        &["--yes-large"],
        Some("n,n,n,n"),
    )
    .success()
    .get_output()
    .clone();
    assert!(asked(&out.stderr).is_empty());
    assert!(other.path().join("ROMS/GBA/Big 1.gba").is_file());
}

#[test]
fn skip_large_leaves_every_huge_item_and_the_rest_still_needs_confirming() {
    let (p, data, cache) = (
        big_pile(false),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let other = TempDir::new().unwrap();
    let out = run_big(
        p.path(),
        other.path(),
        data.path(),
        cache.path(),
        &["--skip-large"],
        None,
    )
    .success()
    .get_output()
    .clone();
    assert!(asked(&out.stderr).is_empty());
    assert!(!other.path().join("ROMS/GBA").exists(), "every GBA left");
    let report = json_of(&out.stdout);
    let not_confirmed = report["plan"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["reason"]["kind"] == "not-confirmed")
        .count();
    assert_eq!(not_confirmed, 3);

    // With two GB files the rest reaches the threshold: no terminal, so
    // nothing confirms it.
    let (p, data) = (big_pile(true), TempDir::new().unwrap());
    let other = TempDir::new().unwrap();
    run_big(
        p.path(),
        other.path(),
        data.path(),
        cache.path(),
        &["--skip-large"],
        None,
    )
    .code(1)
    .stderr(contains("instead of --skip-large; nothing was written"));
    assert!(tree_digest(other.path()).is_empty());
}

/// Two copies of one huge GBA: the plan keeps one and sets the other
/// aside. Declining the keeper leaves it, so the other copy becomes the
/// keeper and is huge too: it is asked (or skipped) as well, not refused.
fn twin_pile() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_old(&tmp.path().join("Twin.gba"), &fx::gba_tagged(30));
    write_old(&tmp.path().join("Twin (1).gba"), &fx::gba_tagged(30));
    tmp
}

#[test]
fn an_item_that_becomes_huge_after_a_decline_is_asked_or_skipped_too() {
    let cache = TempDir::new().unwrap();
    // --skip-large: both copies stay where they are, nothing is refused.
    let (p, data, other) = (
        twin_pile(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let out = run_big(
        p.path(),
        other.path(),
        data.path(),
        cache.path(),
        &["--skip-large"],
        None,
    )
    .success()
    .get_output()
    .clone();
    assert!(asked(&out.stderr).is_empty());
    assert!(!other.path().join("ROMS/GBA").exists(), "both copies left");
    assert!(p.path().join("Twin.gba").is_file());
    assert!(p.path().join("Twin (1).gba").is_file());

    // Asked: «no» to the keeper, then the copy that takes its place is
    // asked on its own; «yes» copies it.
    let (data, other) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let out = run_big(
        p.path(),
        other.path(),
        data.path(),
        cache.path(),
        &[],
        Some("n,y"),
    )
    .success()
    .get_output()
    .clone();
    let names = asked(&out.stderr);
    assert_eq!(names.len(), 2, "{}", String::from_utf8_lossy(&out.stderr));
    assert_ne!(names[0], names[1]);
    assert!(other.path().join("ROMS/GBA").join(&names[1]).is_file());
    assert!(!other.path().join("ROMS/GBA").join(&names[0]).exists());
}

#[test]
fn skip_large_with_no_huge_item_changes_nothing() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let other = TempDir::new().unwrap();
    let mut args = copy_args(other.path());
    args.push("--skip-large".into());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    coverly(data.path(), cache.path(), &args)
        .arg(p.path())
        .assert()
        .success()
        .stdout(contains("copied 2"));
}

#[test]
fn yes_large_and_skip_large_together_are_a_usage_error() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    coverly(
        data.path(),
        cache.path(),
        &[
            "organize",
            "--offline",
            "--apply",
            "--yes-large",
            "--skip-large",
        ],
    )
    .arg(p.path())
    .assert()
    .code(2);
}

#[test]
fn without_a_terminal_the_refusal_is_unchanged() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let other = TempDir::new().unwrap();
    let args = copy_args(other.path());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = coverly(data.path(), cache.path(), &args)
        .env(LARGE_TRANSFER_ENV, "1")
        .arg(p.path())
        .assert()
        .code(1)
        .get_output()
        .clone();
    // The volume is the mount holding the target (the temp folder's own
    // name where none is listed): only it may vary by OS.
    let bytes = (64 + 32) * 1024;
    let stderr = String::from_utf8_lossy(&out.stderr);
    let volume = stderr
        .strip_prefix(&format!("error: would write {bytes} bytes (≥ 10 GB) to "))
        .and_then(|r| r.strip_suffix("; re-run with --yes-large to confirm; nothing was written\n"))
        .unwrap_or_else(|| panic!("unexpected refusal: {stderr:?}"));
    assert!(!volume.is_empty() && !volume.contains('\n'), "{stderr:?}");
}
