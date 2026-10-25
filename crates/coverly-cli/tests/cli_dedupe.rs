//! `coverly dedupe` end to end, as a dry run (duplicates spec §14). Every
//! library is synthetic and lives in a temporary folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use assert_cmd::Command;
use coverly_core::dedupe::{DedupePlan, GroupAction, Relation, PLAN_VERSION};
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

/// Synthetic ROM bytes: binary (NUL bytes), distinct per `seed`.
fn rom_bytes(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| if i % 3 == 0 { 0 } else { (i as u8) ^ seed })
        .collect()
}

fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    p
}

/// An Anbernic-stock card with:
/// - GB: `Tetris.gb` = `Tetris (1).gb`; the first has a real save, the
///   second a cover, so `has_save` and `cover` disagree.
/// - MD: `Sonic.md` = `Sonic (1).md`, each with its own real save: a
///   conflict.
/// - SFC: one game, no duplicate.
struct Library {
    dir: TempDir,
    cache: TempDir,
    data: TempDir,
}

impl Library {
    fn new() -> Library {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("anbernic")).unwrap();
        let tetris = rom_bytes(1, 32 * 1024);
        put(root, "ROMS/GB/Tetris.gb", &tetris);
        put(root, "ROMS/GB/Tetris (1).gb", &tetris);
        put(root, "ROMS/GB/Tetris.sav", &rom_bytes(20, 8192));
        put(root, "ROMS/GB/Imgs/Tetris (1).png", b"\x89PNG\r\n\x1a\nart");
        let sonic = rom_bytes(2, 64 * 1024);
        put(root, "ROMS/MD/Sonic.md", &sonic);
        put(root, "ROMS/MD/Sonic (1).md", &sonic);
        put(root, "ROMS/MD/Sonic.srm", &rom_bytes(10, 8192));
        put(root, "ROMS/MD/Sonic (1).srm", &rom_bytes(11, 8192));
        put(root, "ROMS/SFC/Mario.sfc", &rom_bytes(3, 128 * 1024));
        Library {
            dir,
            cache: TempDir::new().unwrap(),
            data: TempDir::new().unwrap(),
        }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// `coverly dedupe <root> --offline` with this library's folders.
    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("coverly").unwrap();
        cmd.env("COVERLY_SETTINGS", "")
            .arg("dedupe")
            .arg(self.root())
            .arg("--profile")
            .arg("anbernic-stock")
            .arg("--offline")
            .arg("--cache-dir")
            .arg(self.cache.path())
            .arg("--data-dir")
            .arg(self.data.path());
        cmd
    }
}

fn stdout_of(cmd: &mut Command) -> String {
    let out = cmd.assert().success().get_output().stdout.clone();
    String::from_utf8(out).unwrap()
}

fn json_plan(lib: &Library, extra: &[&str]) -> DedupePlan {
    let text = stdout_of(lib.cmd().args(["--report", "json"]).args(extra));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}:\n{text}"))
}

/// The file name of the copy that stays in the group holding `stem`.
fn keeper_of(plan: &DedupePlan, stem: &str) -> String {
    let g = plan
        .groups
        .iter()
        .find(|g| {
            g.members.iter().any(|m| {
                m.unit
                    .primary
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(stem))
            })
        })
        .unwrap_or_else(|| panic!("no group with {stem}"));
    g.members[g.keeper]
        .unit
        .primary
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

/// Sorted `(relative path, len, mtime)` of every entry under `root`.
fn digest(root: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let meta = fs::symlink_metadata(entry.path()).unwrap();
            if meta.is_dir() {
                stack.push(entry.path());
            }
            let rel = entry.path().strip_prefix(root).unwrap().to_path_buf();
            out.push((rel, meta.len(), meta.modified().ok()));
        }
    }
    out.sort();
    out
}

#[test]
fn dry_run_writes_nothing() {
    let lib = Library::new();
    let before = digest(lib.root());
    stdout_of(&mut lib.cmd());
    assert_eq!(digest(lib.root()), before);

    // Dedupe writes only with --apply, whatever the dry-run setting says.
    let cfg = TempDir::new().unwrap();
    let settings = cfg.path().join("settings.json");
    fs::write(&settings, r#"{"safety":{"dry_run_default":false}}"#).unwrap();
    let text = stdout_of(lib.cmd().env("COVERLY_SETTINGS", &settings));
    assert!(text.contains("Dry run: nothing moved."), "{text}");
    assert_eq!(digest(lib.root()), before);
    assert!(!lib.root().join("_Duplicados").exists());
}

#[test]
fn table_lists_systems_groups_and_destinations() {
    let lib = Library::new();
    let text = stdout_of(&mut lib.cmd());
    let gb = text
        .lines()
        .find(|l| l.starts_with("GB "))
        .unwrap_or_else(|| panic!("no GB heading:\n{text}"));
    assert!(gb.contains("Game Boy"), "{text}");
    assert!(gb.contains("1 game"), "{text}");
    assert!(gb.contains("1 copy"), "{text}");
    assert!(text.contains("  Tetris"), "{text}");
    let stays = text
        .lines()
        .find(|l| l.trim_start().starts_with("stays"))
        .unwrap_or_else(|| panic!("no keeper row:\n{text}"));
    assert!(stays.contains("Tetris.gb"), "{text}");
    assert!(stays.contains("has a save"), "{text}");
    assert!(text.contains("exact copy"), "{text}");
    // Spelled with the platform's separator (`_Duplicados\GB\…` on Windows).
    let dest = Path::new("_Duplicados").join("GB").join("Tetris (1).gb");
    assert!(
        text.contains(&format!("→ {}", dest.display())),
        "destination relative to the quarantine's parent:\n{text}"
    );
    // The conflict is shown, never moved.
    assert!(text.contains("MD "), "{text}");
    assert!(text.contains("conflict"), "{text}");
    let md = Path::new("_Duplicados").join("MD");
    assert!(!text.contains(&md.display().to_string()), "{text}");
    // The one SFC game has no copy.
    assert!(!text.lines().any(|l| l.starts_with("SFC ")), "{text}");
    assert!(
        text.contains("1 extra copy across 1 game, 32,768 bytes. Dry run: nothing moved."),
        "{text}"
    );
}

#[test]
fn json_matches_the_plan_schema() {
    let lib = Library::new();
    let text = stdout_of(lib.cmd().args(["--report", "json"]));
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["version"], PLAN_VERSION);
    assert_eq!(v["version"], 1);
    assert_eq!(v["profile_id"], "anbernic-stock");
    assert!(v["groups"].is_array());
    assert_eq!(v["totals"]["extras"], 1);
    assert_eq!(v["totals"]["conflicts"], 1);
    let plan: DedupePlan = serde_json::from_value(v).unwrap();
    assert_eq!(plan.groups.len(), 2);
    assert!(plan
        .groups
        .iter()
        .any(|g| matches!(g.action, GroupAction::Conflict { .. })));

    // --output writes the same report to a file and nothing to stdout.
    let out = lib.data.path().join("report.json");
    let printed = stdout_of(lib.cmd().args(["--report", "json", "--output"]).arg(&out));
    assert!(printed.is_empty(), "{printed}");
    let written: DedupePlan = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(written.groups.len(), 2);
}

#[test]
fn keep_path_overrides_the_keeper() {
    let lib = Library::new();
    assert_eq!(keeper_of(&json_plan(&lib, &[]), "Tetris"), "Tetris.gb");
    let other = lib.root().join("ROMS/GB/Tetris (1).gb");
    let plan = json_plan(&lib, &["--keep", other.to_str().unwrap()]);
    assert_eq!(keeper_of(&plan, "Tetris"), "Tetris (1).gb");
}

#[test]
fn keep_rules_reorder_changes_the_winner() {
    let lib = Library::new();
    assert_eq!(keeper_of(&json_plan(&lib, &[]), "Tetris"), "Tetris.gb");
    let plan = json_plan(&lib, &["--keep-rules", "cover,has_save"]);
    assert_eq!(keeper_of(&plan, "Tetris"), "Tetris (1).gb");
}

#[test]
fn conflicts_exit_zero() {
    let lib = Library::new();
    let text = stdout_of(lib.cmd().args(["--system", "MD"]));
    assert!(text.contains("conflict"), "{text}");
    assert!(text.contains("Sonic.srm"), "{text}");
    assert!(
        text.contains("0 extra copies across 0 games, 0 bytes."),
        "{text}"
    );
}

#[test]
fn unknown_rule_is_exit_2() {
    let lib = Library::new();
    lib.cmd()
        .args(["--keep-rules", "has_save,shiny"])
        .assert()
        .code(2)
        .stderr(contains("shiny"));
    lib.cmd()
        .args(["--system", "nope"])
        .assert()
        .code(2)
        .stderr(contains("nope"));
}

#[test]
fn apply_waits_for_the_archive_work() {
    let lib = Library::new();
    let before = digest(lib.root());
    for flag in [&["--apply"][..], &["--trash"], &["--resume", "job-1"]] {
        lib.cmd()
            .args(flag)
            .assert()
            .code(2)
            .stderr(contains("available after the archive work lands"));
    }
    let q = lib.data.path().join("q");
    lib.cmd()
        .arg("--quarantine")
        .arg(&q)
        .assert()
        .code(2)
        .stderr(contains("available after the archive work lands"));
    assert!(!q.exists());
    assert_eq!(digest(lib.root()), before);
}

/// A root with no `ROMS` folder (the ROMs folder itself passed as the
/// card) must say so, not report a clean library.
#[test]
fn scan_warnings_reach_stderr() {
    let lib = Library::new();
    let mut cmd = Command::cargo_bin("coverly").unwrap();
    cmd.env("COVERLY_SETTINGS", "")
        .arg("dedupe")
        .arg(lib.root().join("ROMS"))
        .arg("--profile")
        .arg("anbernic-stock")
        .arg("--offline")
        .arg("--cache-dir")
        .arg(lib.cache.path())
        .arg("--data-dir")
        .arg(lib.data.path())
        .assert()
        .success()
        .stderr(contains("warning:").and(contains("ROMs folder `ROMS` not found")));
}

#[test]
fn missing_root_is_exit_3() {
    let lib = Library::new();
    Command::cargo_bin("coverly")
        .unwrap()
        .env("COVERLY_SETTINGS", "")
        .arg("dedupe")
        .arg(lib.root().join("nowhere"))
        .arg("--offline")
        .assert()
        .code(3)
        .stderr(contains("not found"));
}

/// Unix only: Windows refuses control characters in file names.
#[cfg(unix)]
#[test]
fn hostile_file_names_are_terminal_safe() {
    let lib = Library::new();
    let zelda = rom_bytes(4, 16 * 1024);
    put(lib.root(), "ROMS/GB/Zelda.gb", &zelda);
    put(lib.root(), "ROMS/GB/Zelda\u{1b}]0;pwned\u{7}.gb", &zelda);
    let text = stdout_of(&mut lib.cmd());
    assert!(text.contains("Zelda"), "{text}");
    assert!(!text.contains('\u{1b}'), "{text}");
    assert!(!text.contains('\u{7}'), "{text}");
    assert!(text.contains("Zelda\u{FFFD}]0;pwned\u{FFFD}.gb"), "{text}");
}

// --- Undo of duplicates jobs (duplicates plan Task 21) ---

use coverly_core::write::trash::TrashBackend as _;
use coverly_core::write::{Collision, JobKind, JobMeta, Writer};

/// A duplicates job written with the core: `Tetris (1).gb` moved into
/// `_Duplicados/GB`, and `Tetris (2).gb` sent to a Trash place that no
/// longer holds it. Returns (library, data, trash, lost path).
fn dedupe_journal() -> (TempDir, TempDir, TempDir, PathBuf) {
    let (lib, data, trash) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let root = lib.path();
    let moved = put(root, "ROMS/GB/Tetris (1).gb", b"one");
    let lost = put(root, "ROMS/GB/Tetris (2).gb", b"two");
    let mut w = Writer::begin_with(
        root,
        data.path(),
        JobMeta {
            kind: JobKind::Dedupe,
            source_root: None,
        },
    )
    .unwrap();
    let dest = root.join("_Duplicados/GB");
    w.ensure_dir(&dest).unwrap();
    w.move_file(
        &moved,
        &dest.join("Tetris (1).gb"),
        Collision::Rename,
        Some(0),
    )
    .unwrap();
    w.archive_trash_intent(&lost, Some(1)).unwrap();
    fs::remove_file(&lost).unwrap();
    w.archive_trashed(
        &lost,
        Some(&trash.path().join("gone/Tetris (2).gb")),
        Some(1),
    )
    .unwrap();
    w.finish().unwrap();
    (lib, data, trash, lost)
}

/// `coverly undo <args…>` with a private data dir and a fake Trash.
fn undo_cmd(data: &Path, trash: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("coverly").unwrap();
    cmd.env("COVERLY_SETTINGS", "")
        .env("COVERLY_TEST_FAKE_TRASH", trash)
        .arg("undo")
        .args(args)
        .arg("--data-dir")
        .arg(data);
    cmd
}

#[test]
fn undo_list_names_dedupe_jobs() {
    let (_lib, data, trash, _) = dedupe_journal();
    let text = stdout_of(&mut undo_cmd(data.path(), trash.path(), &["--list"]));
    assert!(text.contains("dedupe"), "{text}");
}

#[test]
fn undo_text_of_a_dedupe_job_names_lost_trash_items() {
    let (lib, data, trash, lost) = dedupe_journal();
    let text = stdout_of(&mut undo_cmd(data.path(), trash.path(), &[]));
    assert!(text.contains("Undid dedupe job"), "{text}");
    assert!(text.contains("moved back 1"), "{text}");
    assert!(
        text.contains(&format!("not found in the Trash: {}", lost.display())),
        "{text}"
    );
    assert!(lib.path().join("ROMS/GB/Tetris (1).gb").exists());
}

#[test]
fn undo_of_a_dedupe_job_brings_a_trashed_copy_back_byte_identical() {
    let (lib, data, trash) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let root = lib.path();
    let bytes = rom_bytes(7, 4096);
    let copy = put(root, "ROMS/GB/Tetris (1).gb", &bytes);
    let mut w = Writer::begin_with(
        root,
        data.path(),
        JobMeta {
            kind: JobKind::Dedupe,
            source_root: None,
        },
    )
    .unwrap();
    w.archive_trash_intent(&copy, Some(0)).unwrap();
    let place = coverly_core::write::trash::FakeTrash::new(trash.path())
        .trash(&copy)
        .unwrap();
    w.archive_trashed(&copy, place.as_deref(), Some(0)).unwrap();
    w.finish().unwrap();
    assert!(!copy.exists());

    let text = stdout_of(&mut undo_cmd(data.path(), trash.path(), &[]));
    assert!(text.contains("Undid dedupe job"), "{text}");
    assert!(
        text.contains(&format!("back from the Trash: {}", copy.display())),
        "{text}"
    );
    assert!(!text.contains("not found in the Trash"), "{text}");
    assert_eq!(fs::read(&copy).unwrap(), bytes);
}

/// Unix only: Windows refuses control characters in file names. A name
/// holding a newline must not print a second, forged line, and the files
/// kept as they are print escaped too.
#[cfg(unix)]
#[test]
fn undo_text_of_a_dedupe_job_keeps_hostile_names_on_one_line() {
    let (lib, data, trash) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let root = lib.path();
    let moved = put(root, "ROMS/GB/Zelda\u{1b}]0;pwned\u{7}.gb", b"one");
    let lost = put(
        root,
        "ROMS/GB/Tetris\n  back from the Trash: forged.gb",
        b"two",
    );
    let mut w = Writer::begin_with(
        root,
        data.path(),
        JobMeta {
            kind: JobKind::Dedupe,
            source_root: None,
        },
    )
    .unwrap();
    let dest = root.join("_Duplicados/GB");
    w.ensure_dir(&dest).unwrap();
    let to = dest.join("Zelda.gb");
    w.move_file(&moved, &to, Collision::Rename, Some(0))
        .unwrap();
    w.archive_trash_intent(&lost, Some(1)).unwrap();
    fs::remove_file(&lost).unwrap();
    w.archive_trashed(&lost, Some(&trash.path().join("gone/x.gb")), Some(1))
        .unwrap();
    w.finish().unwrap();
    // Changed after the job: undo keeps it where it is and names it.
    fs::write(&to, b"changed since").unwrap();

    let text = stdout_of(&mut undo_cmd(data.path(), trash.path(), &[]));
    assert!(!text.contains('\u{1b}'), "{text}");
    assert!(!text.contains('\u{7}'), "{text}");
    assert!(text.contains("kept "), "{text}");
    assert!(
        !text
            .lines()
            .any(|l| l.trim_start().starts_with("back from the Trash")),
        "{text}"
    );
    assert!(text.contains("not found in the Trash: "), "{text}");
}

/// The names of every member of the group holding `name`, sorted, with
/// the weakest relation in it.
fn group_holding(plan: &DedupePlan, name: &str) -> Option<(Vec<String>, Relation)> {
    let file = |p: &Path| p.file_name().unwrap().to_string_lossy().into_owned();
    plan.groups
        .iter()
        .find(|g| g.members.iter().any(|m| file(&m.unit.primary) == name))
        .map(|g| {
            let mut names: Vec<String> = g.members.iter().map(|m| file(&m.unit.primary)).collect();
            names.sort();
            let rel = Relation::weakest(g.members.iter().map(|m| m.relation)).unwrap();
            (names, rel)
        })
}

#[test]
fn reads_inside_7z_and_pbp_without_external_tools() {
    use coverly_core::archive::testing::{sevenz, SevenZShape};
    use coverly_core::disc::testing as dt;

    let lib = Library::new();
    let root = lib.root();
    // A raw ROM and a 7z of it under another inner name: one exact copy
    // only when the CLI reads inside the 7z (whole-file bytes differ).
    let zelda = rom_bytes(4, 32 * 1024);
    put(root, "ROMS/GB/Zelda.gb", &zelda);
    put(
        root,
        "ROMS/GB/Zelda (1).7z",
        &sevenz(&[("Other name.gb", &zelda)], SevenZShape::non_solid_lzma()),
    );
    // The same PS1 disc as a bin and as a PBP: only the disc reader can
    // tell (level 3).
    let track = dt::psx_track(true, "cdrom:\\SCUS_945.70;1");
    put(root, "ROMS/PS/Crash Bash.bin", &track);
    put(root, "ROMS/PS/Crash Bash.PBP", &dt::pbp_of(&track));

    let plan = json_plan(&lib, &["--no-external-tools", "--level", "exact,format"]);
    assert_eq!(
        group_holding(&plan, "Zelda.gb"),
        Some((
            vec!["Zelda (1).7z".to_owned(), "Zelda.gb".to_owned()],
            Relation::Exact
        ))
    );
    assert_eq!(
        group_holding(&plan, "Crash Bash.bin"),
        Some((
            vec!["Crash Bash.PBP".to_owned(), "Crash Bash.bin".to_owned()],
            Relation::Format
        ))
    );
}
