//! External archive tools: discovery and process hygiene (plan Task 13,
//! spec §5.2 and S7 hygiene). The tools are copies of `coverly-fake-tool`
//! told by a script beside them what to print.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::{Duration, Instant};

use coverly_core::archive::external::{discover_with, pick_rar_tool, FoundTool, Search};
use coverly_core::archive::testing::{fake_tool, run_tool, FakeCase, FakeScript};
use coverly_core::archive::{ArchiveError, Tool, MAX_HEADER};

const SEVEN_ZIP_BANNER: &str =
    "7-Zip (z) 24.08 (arm64) : Copyright (c) 1999-2024 Igor Pavlov : 2024-08-11";
const UNRAR_BANNER: &str = "UNRAR 7.01 freeware      Copyright (c) 1993-2024 Alexander Roshal";
const UNRAR_OLD_BANNER: &str = "UNRAR 4.20 freeware      Copyright (c) 1993-2012 Alexander Roshal";

/// Sets, once and before any test spawns anything, the variables a tool
/// would read its default switches from, so `tool_env_is_cleared` proves
/// they never reach it.
fn setup() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Every test calls this first; `Once` blocks the others until done.
        std::env::set_var("RAR", "-y -o+");
        std::env::set_var("TAR_READER_OPTIONS", "hdrcharset=BINARY");
    });
}

fn search(path_entries: &[&OsStr], cwd: &Path) -> Search {
    Search {
        path_var: Some(std::env::join_paths(path_entries).expect("join PATH")),
        cwd: Some(cwd.to_path_buf()),
        fixed: Vec::new(),
    }
}

fn found(tool: Tool, path: PathBuf) -> FoundTool {
    FoundTool {
        tool,
        path,
        version: "24.08".into(),
        rar5: true,
    }
}

/// An archive path for the calls (the fake never opens it).
fn archive(dir: &Path) -> PathBuf {
    let path = dir.join("game.rar");
    std::fs::write(&path, b"Rar!\x1a\x07\x01\x00").expect("archive");
    path
}

fn report_value<'a>(stdout: &'a str, key: &str) -> &'a str {
    let prefix = format!("{key}=");
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("no {key} in report:\n{stdout}"))
}

fn report_case() -> FakeScript {
    FakeScript {
        cases: vec![FakeCase {
            report: true,
            ..FakeCase::default()
        }],
    }
}

#[test]
fn discover_ignores_relative_and_cwd_path_entries() {
    setup();
    let tmp = tempfile::tempdir().expect("tmp");
    let cwd = tmp.path().join("cwd");
    let rel = tmp.path().join("rel");
    let source = tmp.path().join("source");
    let good = tmp.path().join("good");
    for d in [&cwd, &rel, &source, &good] {
        std::fs::create_dir_all(d).expect("dir");
    }
    fake_tool(&cwd, "7zz", &FakeScript::banner(SEVEN_ZIP_BANNER));
    fake_tool(&rel, "7zz", &FakeScript::banner(SEVEN_ZIP_BANNER));
    fake_tool(
        &source,
        "bsdtar",
        &FakeScript::banner("bsdtar 3.7.4 - libarchive 3.7.4"),
    );
    fake_tool(&good, "unrar", &FakeScript::banner(UNRAR_BANNER));

    // `rel` relative to the cwd, the cwd itself, an empty entry, a folder
    // inside the source root, then the one safe folder.
    let s = search(
        &[
            OsStr::new("rel"),
            cwd.as_os_str(),
            OsStr::new(""),
            source.as_os_str(),
            good.as_os_str(),
        ],
        &cwd,
    );
    let tools = discover_with(&s, &[tmp.path().join("source").as_path()]);
    let paths: Vec<_> = tools.iter().map(|t| t.path.clone()).collect();
    assert_eq!(tools.len(), 1, "only the safe folder's tool: {paths:?}");
    assert_eq!(tools[0].tool, Tool::Unrar);
    assert!(tools[0].path.is_absolute());
    assert!(tools[0].path.starts_with(&good), "{paths:?}");
}

#[test]
fn discover_parses_version_banner() {
    setup();
    let tmp = tempfile::tempdir().expect("tmp");
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    fake_tool(&bin, "7zz", &FakeScript::banner(SEVEN_ZIP_BANNER));
    fake_tool(&bin, "unrar", &FakeScript::banner(UNRAR_BANNER));
    let bsdtar = FakeScript {
        cases: vec![FakeCase {
            args_prefix: vec!["--version".into()],
            stdout: "bsdtar 3.7.4 - libarchive 3.7.4 zlib/1.2.12 liblzma/5.4.3 bz2lib/1.0.8\n"
                .into(),
            ..FakeCase::default()
        }],
    };
    fake_tool(&bin, "bsdtar", &bsdtar);
    // A tool whose banner is not one of ours is skipped, not guessed.
    fake_tool(&bin, "7z", &FakeScript::banner("hello"));

    let tools = discover_with(&search(&[bin.as_os_str()], tmp.path()), &[]);
    let summary: Vec<_> = tools
        .iter()
        .map(|t| (t.tool, t.version.as_str(), t.rar5))
        .collect();
    assert_eq!(
        summary,
        vec![
            (Tool::Unrar, "7.01", true),
            (Tool::SevenZip, "24.08", true),
            (Tool::Bsdtar, "3.7.4", true),
        ],
        "preference order unrar, 7zz, 7z, bsdtar"
    );
}

#[test]
fn unrar_older_than_5_skipped_for_rar5() {
    setup();
    let tmp = tempfile::tempdir().expect("tmp");
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    fake_tool(&bin, "unrar", &FakeScript::banner(UNRAR_OLD_BANNER));

    let only_unrar = discover_with(&search(&[bin.as_os_str()], tmp.path()), &[]);
    assert_eq!(only_unrar.len(), 1);
    assert_eq!(only_unrar[0].version, "4.20");
    assert!(!only_unrar[0].rar5);
    assert!(
        pick_rar_tool(&only_unrar, true).is_none(),
        "unrar 4 cannot open RAR5"
    );
    assert_eq!(
        pick_rar_tool(&only_unrar, false).map(|t| t.tool),
        Some(Tool::Unrar),
        "RAR4 is fine"
    );

    fake_tool(&bin, "7zz", &FakeScript::banner(SEVEN_ZIP_BANNER));
    let both = discover_with(&search(&[bin.as_os_str()], tmp.path()), &[]);
    assert_eq!(
        pick_rar_tool(&both, true).map(|t| t.tool),
        Some(Tool::SevenZip),
        "RAR5 falls through to 7-Zip"
    );
    assert_eq!(
        pick_rar_tool(&both, false).map(|t| t.tool),
        Some(Tool::Unrar)
    );
}

#[test]
fn tool_env_is_cleared() {
    setup();
    let tmp = tempfile::tempdir().expect("tmp");
    let exe = fake_tool(tmp.path(), "unrar", &report_case());
    let run = run_tool(
        &found(Tool::Unrar, exe),
        &[OsStr::new("lt")],
        &archive(tmp.path()),
        MAX_HEADER,
        Duration::from_secs(20),
    )
    .expect("run");
    let out = String::from_utf8(run.stdout).expect("utf8");
    assert_eq!(report_value(&out, "env.RAR"), "<unset>");
    assert_eq!(report_value(&out, "env.TAR_READER_OPTIONS"), "<unset>");
    assert_eq!(
        report_value(&out, "env.HOME"),
        report_value(&out, "cwd"),
        "HOME is the empty working directory"
    );
    assert_ne!(report_value(&out, "env.PATH"), "<unset>");
    assert_eq!(run.status, Some(0));
}

#[test]
fn tool_cwd_is_empty_tempdir() {
    setup();
    let tmp = tempfile::tempdir().expect("tmp");
    let exe = fake_tool(tmp.path(), "7zz", &report_case());
    let archive = archive(tmp.path());
    let run = run_tool(
        &found(Tool::SevenZip, exe),
        &[OsStr::new("l"), OsStr::new("-slt")],
        &archive,
        MAX_HEADER,
        Duration::from_secs(20),
    )
    .expect("run");
    let out = String::from_utf8(run.stdout).expect("utf8");
    assert_eq!(report_value(&out, "cwd_entries"), "0");
    let cwd = PathBuf::from(report_value(&out, "cwd"));
    assert!(
        !cwd.starts_with(tmp.path()),
        "not the archive's folder: {cwd:?}"
    );
    assert!(!cwd.exists(), "the working directory is removed afterwards");

    let args: Vec<String> = serde_json::from_str(report_value(&out, "args")).expect("args");
    let archive_arg = archive.to_string_lossy().into_owned();
    assert_eq!(
        args,
        vec!["l".to_string(), "-slt".into(), "--".into(), archive_arg],
        "`--` right before the archive"
    );
}

#[test]
fn tool_timeout_killed() {
    setup();
    let tmp = tempfile::tempdir().expect("tmp");
    let marker = tmp.path().join("grandchild-survived");
    let script = FakeScript {
        cases: vec![FakeCase {
            stdout: "listing...\n".into(),
            grandchild_sleep_ms: 1_500,
            grandchild_marker: Some(marker.clone()),
            sleep_ms: 30_000,
            ..FakeCase::default()
        }],
    };
    let exe = fake_tool(tmp.path(), "7zz", &script);
    let start = Instant::now();
    let err = run_tool(
        &found(Tool::SevenZip, exe),
        &[OsStr::new("l")],
        &archive(tmp.path()),
        MAX_HEADER,
        Duration::from_millis(400),
    )
    .expect_err("timed out");
    assert!(
        matches!(
            err,
            ArchiveError::ToolFailed {
                tool: Tool::SevenZip,
                status: None
            }
        ),
        "{err:?}"
    );
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "returned after {:?}",
        start.elapsed()
    );
    // The grandchild would write its marker 1.5 s after it started.
    std::thread::sleep(Duration::from_millis(2_500));
    assert!(!marker.exists(), "the whole process tree was killed");
}

#[test]
fn stderr_flood_no_deadlock() {
    setup();
    let tmp = tempfile::tempdir().expect("tmp");
    let script = FakeScript {
        cases: vec![FakeCase {
            stderr_fill: 10 * 1024 * 1024,
            stdout: "Path = a.bin\n".into(),
            stdout_fill: 1024 * 1024,
            ..FakeCase::default()
        }],
    };
    let exe = fake_tool(tmp.path(), "7zz", &script);
    let run = run_tool(
        &found(Tool::SevenZip, exe),
        &[OsStr::new("l")],
        &archive(tmp.path()),
        MAX_HEADER,
        Duration::from_secs(30),
    )
    .expect("run");
    assert_eq!(run.status, Some(0));
    assert_eq!(run.stdout.len(), "Path = a.bin\n".len() + 1024 * 1024);
    assert_eq!(run.stderr.len(), 64 * 1024, "stderr kept up to 64 KiB");
}

#[test]
fn listing_over_cap_rejected() {
    setup();
    let tmp = tempfile::tempdir().expect("tmp");
    let script = FakeScript {
        cases: vec![FakeCase {
            stdout_fill: MAX_HEADER + 1024 * 1024,
            ..FakeCase::default()
        }],
    };
    let exe = fake_tool(tmp.path(), "unrar", &script);
    let err = run_tool(
        &found(Tool::Unrar, exe),
        &[OsStr::new("lt")],
        &archive(tmp.path()),
        MAX_HEADER,
        Duration::from_secs(30),
    )
    .expect_err("over cap");
    assert!(matches!(err, ArchiveError::TooLarge { .. }), "{err:?}");
}

#[test]
fn nonzero_exit_is_reported_not_hidden() {
    setup();
    let tmp = tempfile::tempdir().expect("tmp");
    let script = FakeScript {
        cases: vec![FakeCase {
            stderr: "ERROR: Can not open the file as archive\n".into(),
            exit: 2,
            ..FakeCase::default()
        }],
    };
    let exe = fake_tool(tmp.path(), "7zz", &script);
    let run = run_tool(
        &found(Tool::SevenZip, exe),
        &[OsStr::new("l")],
        &archive(tmp.path()),
        MAX_HEADER,
        Duration::from_secs(20),
    )
    .expect("ran");
    assert_eq!(run.status, Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("Can not open"));
}

#[test]
fn relative_or_missing_tool_refused() {
    setup();
    let tmp = tempfile::tempdir().expect("tmp");
    let archive = archive(tmp.path());
    for path in [PathBuf::from("7zz"), tmp.path().join("missing-7zz")] {
        let err = run_tool(
            &found(Tool::SevenZip, path.clone()),
            &[OsStr::new("l")],
            &archive,
            MAX_HEADER,
            Duration::from_secs(5),
        )
        .expect_err("refused");
        assert!(
            matches!(
                err,
                ArchiveError::Unsupported {
                    need: Some(Tool::SevenZip),
                    ..
                }
            ),
            "{path:?}: {err:?}"
        );
    }
}

#[cfg(unix)]
mod unix_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn non_executable_tool_refused() {
        setup();
        let tmp = tempfile::tempdir().expect("tmp");
        let exe = fake_tool(tmp.path(), "7zz", &FakeScript::default());
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        let err = run_tool(
            &found(Tool::SevenZip, exe),
            &[OsStr::new("l")],
            &archive(tmp.path()),
            MAX_HEADER,
            Duration::from_secs(5),
        )
        .expect_err("refused");
        assert!(matches!(err, ArchiveError::Unsupported { .. }), "{err:?}");
    }

    #[test]
    fn timeout_returns_when_a_descendant_escapes_and_holds_the_pipes() {
        setup();
        let tmp = tempfile::tempdir().expect("tmp");
        let script = FakeScript {
            cases: vec![FakeCase {
                stdout: "listing...\n".into(),
                // Leaves the group, so `killpg` misses it, and keeps
                // stdout and stderr open for 8 s.
                grandchild_sleep_ms: 8_000,
                grandchild_detach: true,
                sleep_ms: 30_000,
                ..FakeCase::default()
            }],
        };
        let exe = fake_tool(tmp.path(), "7zz", &script);
        let start = Instant::now();
        let err = run_tool(
            &found(Tool::SevenZip, exe),
            &[OsStr::new("l")],
            &archive(tmp.path()),
            MAX_HEADER,
            Duration::from_millis(400),
        )
        .expect_err("timed out");
        assert!(
            matches!(err, ArchiveError::ToolFailed { status: None, .. }),
            "{err:?}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "the timeout bounds the run even with the pipes held: {:?}",
            start.elapsed()
        );
    }
}

#[cfg(windows)]
mod windows_tests {
    use super::*;

    #[test]
    fn bat_tool_refused_on_windows() {
        setup();
        let tmp = tempfile::tempdir().expect("tmp");
        let archive = archive(tmp.path());
        for name in ["7z.bat", "7z.cmd", "7z.ps1", "7z.com"] {
            let script = tmp.path().join(name);
            std::fs::write(&script, b"@echo pwned\r\n").expect("script");
            let err = run_tool(
                &found(Tool::SevenZip, script),
                &[OsStr::new("l")],
                &archive,
                MAX_HEADER,
                Duration::from_secs(5),
            )
            .expect_err("refused");
            assert!(
                matches!(err, ArchiveError::Unsupported { .. }),
                "{name}: {err:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Strict 7-Zip and unrar listing parsers (plan Task 14, S7)
// ---------------------------------------------------------------------------

mod listing {
    use super::*;
    use coverly_core::archive::external::ToolReader;
    use coverly_core::archive::testing::{
        next_tetris_ps1_listing, seven_zip_l, seven_zip_l_with, seven_zip_listing_script,
        seven_zip_record, seven_zip_slt, seven_zip_summary_line, unrar_l, unrar_l_with,
        unrar_listing_script, unrar_lt, unrar_record, ToolEntry,
    };
    use coverly_core::archive::{ArchiveReader, EntryKind, Format, Limits, Method};

    const RAR5: Format = Format::Rar { v5: true };

    fn seven_zip(dir: &Path, script: &FakeScript) -> FoundTool {
        found(Tool::SevenZip, fake_tool(dir, "7zz", script))
    }

    fn unrar(dir: &Path, script: &FakeScript) -> FoundTool {
        FoundTool {
            tool: Tool::Unrar,
            path: fake_tool(dir, "unrar", script),
            version: "7.01".into(),
            rar5: true,
        }
    }

    fn open_7z(slt: &str, l: &str) -> Result<ToolReader, ArchiveError> {
        setup();
        let tmp = tempfile::tempdir().expect("tmp");
        let tool = seven_zip(tmp.path(), &seven_zip_listing_script(slt, l));
        ToolReader::open(
            &[tool],
            &archive(tmp.path()),
            Format::SevenZip,
            &Limits::default(),
        )
    }

    fn open_unrar(lt: &str, l: &str) -> Result<ToolReader, ArchiveError> {
        setup();
        let tmp = tempfile::tempdir().expect("tmp");
        let tool = unrar(tmp.path(), &unrar_listing_script(lt, l));
        ToolReader::open(&[tool], &archive(tmp.path()), RAR5, &Limits::default())
    }

    #[track_caller]
    fn assert_damaged(r: Result<ToolReader, ArchiveError>, what: &str) {
        match r {
            Err(ArchiveError::Damaged { .. }) => {}
            Err(e) => panic!("{what}: expected Damaged, got {e:?}"),
            Ok(r) => panic!("{what}: accepted {:?}", names(&r)),
        }
    }

    #[track_caller]
    fn assert_damaged_with(r: Result<ToolReader, ArchiveError>, what: &str, needle: &str) {
        match r {
            Err(ArchiveError::Damaged { detail }) => {
                assert!(
                    detail.contains(needle),
                    "{what}: {detail:?} lacks {needle:?}"
                );
            }
            Err(e) => panic!("{what}: expected Damaged, got {e:?}"),
            Ok(r) => panic!("{what}: accepted {:?}", names(&r)),
        }
    }

    #[track_caller]
    fn assert_unsafe_names(r: Result<ToolReader, ArchiveError>, what: &str) {
        match r {
            Err(ArchiveError::Unsupported { format, need: None }) if format == "unsafe names" => {}
            Err(e) => panic!("{what}: expected Unsupported(unsafe names), got {e:?}"),
            Ok(r) => panic!("{what}: accepted {:?}", names(&r)),
        }
    }

    fn names(r: &ToolReader) -> Vec<String> {
        r.entries()
            .iter()
            .map(|e| e.name.as_str().to_string())
            .collect()
    }

    fn two_files() -> Vec<ToolEntry> {
        vec![
            ToolEntry::file("a.bin", 3, 0x3524_41C2),
            ToolEntry::file("b.bin", 5, 0x0102_0304),
        ]
    }

    #[test]
    fn seven_zip_listing_round_trip() {
        let listing = next_tetris_ps1_listing();
        let r = open_7z(&seven_zip_slt(&listing), &seven_zip_l(&listing)).expect("listing");
        assert_eq!(r.format(), Format::SevenZip);
        assert_eq!(r.tool().tool, Tool::SevenZip);
        assert_eq!(r.tool().version, "24.08");
        let entries = r.entries();
        assert_eq!(entries.len(), 13);
        assert_eq!(entries[0].kind, EntryKind::Dir);
        assert_eq!(entries[0].name.as_str(), "Next Tetris, The (USA)");
        let track1 = &entries[1];
        assert_eq!(
            track1.name.as_str(),
            "Next Tetris, The (USA)/Next Tetris, The (USA) (Track 01).bin"
        );
        assert_eq!(track1.kind, EntryKind::File);
        assert_eq!(track1.size, 34_508_544);
        assert_eq!(track1.packed, Some(17_811_251));
        assert_eq!(track1.crc32, Some(0x8B6F_8183));
        assert_eq!(track1.method, Method::Lzma { dict: 1 << 26 });
        assert_eq!(track1.solid_block, Some(0));
        assert!(!track1.encrypted);
        let cue = &entries[12];
        assert_eq!(cue.name.file_name(), "Next Tetris, The (USA).cue");
        assert_eq!(
            (cue.size, cue.crc32, cue.solid_block),
            (1269, Some(0xC164_24E9), Some(11))
        );
        for (i, e) in entries.iter().enumerate() {
            assert_eq!(e.index, i, "archive order");
        }
        let total: u64 = entries.iter().map(|e| e.size).sum();
        assert_eq!(total, 405_749_493, "the real case's byte count");
    }

    #[test]
    fn unrar_listing_round_trip() {
        let listing = vec![
            ToolEntry::dir("Game"),
            ToolEntry::file("Game/Game (Track 1).bin", 2_352_000, 0xDEAD_BEEF),
            ToolEntry::file("Game/Game.cue", 80, 0x0BAD_F00D),
        ];
        let r = open_unrar(&unrar_lt(&listing), &unrar_l(&listing)).expect("listing");
        assert_eq!(r.format(), RAR5);
        assert_eq!(r.tool().tool, Tool::Unrar);
        assert_eq!(r.tool().version, "7.01");
        let got: Vec<_> = r
            .entries()
            .iter()
            .map(|e| (e.name.as_str(), e.kind, e.size, e.crc32))
            .collect();
        assert_eq!(
            got,
            vec![
                ("Game", EntryKind::Dir, 0, None),
                (
                    "Game/Game (Track 1).bin",
                    EntryKind::File,
                    2_352_000,
                    Some(0xDEAD_BEEF)
                ),
                ("Game/Game.cue", EntryKind::File, 80, Some(0x0BAD_F00D)),
            ]
        );
    }

    #[test]
    fn unrar_utf16_listing_decodes() {
        // `-scur` may make unrar print UTF-16LE with a byte-order mark.
        use coverly_core::archive::external::decode_tool_text;
        let listing = vec![ToolEntry::file("Pokémon.gba", 16, 0x1111_2222)];
        let lt = unrar_lt(&listing);
        let r = open_unrar(&lt, &unrar_l(&listing)).expect("utf-8 listing");
        assert_eq!(r.entries()[0].name.as_str(), "Pokémon.gba");
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(lt.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode_tool_text(&utf16).expect("utf-16 decodes"), lt);
        assert!(
            decode_tool_text(b"Name: \xff\xfe\xfd").is_err(),
            "invalid UTF-8 without a BOM is refused"
        );
        assert!(
            decode_tool_text(&[0xFF, 0xFE, 0x00, 0xD8]).is_err(),
            "an unpaired surrogate is refused"
        );
    }

    #[test]
    fn forged_newline_name_rejected() {
        // An old 7-Zip prints a stored newline as is: the name forges a
        // second `Path` field into the record.
        let mut listing = two_files();
        listing[0].name = "a.bin\nPath = evil".into();
        assert_damaged_with(
            open_7z(&seven_zip_slt(&listing), &seven_zip_l(&listing)),
            "7-Zip duplicate Path",
            "\"Path\" repeated or out of order",
        );
        // A continuation line that is no `Key = value`.
        listing[0].name = "a.bin\nevil".into();
        assert_damaged(
            open_7z(&seven_zip_slt(&listing), &seven_zip_l(&listing)),
            "7-Zip continuation line",
        );
        // unrar: the name forges a `Type` field.
        let mut listing = two_files();
        listing[0].name = "a.bin\n        Type: Directory".into();
        assert_damaged_with(
            open_unrar(&unrar_lt(&listing), &unrar_l(&listing)),
            "unrar duplicate Type",
            "\"Type\" repeated or out of order",
        );
    }

    #[test]
    fn forged_record_with_full_key_set_caught_by_count() {
        // The stored name is `a.bin` + the rest of a complete record + a
        // blank line + a second complete record: every record parses, so
        // only the tool's own count can catch it.
        let real = ToolEntry::file("a.bin", 3, 0x3524_41C2);
        let tail = seven_zip_record(&real)["Path = a.bin\n".len()..].to_string();
        let mut forged = real.clone();
        forged.name = format!("a.bin\n{tail}\nPath = evil.bin");
        let summary = seven_zip_summary_line(1, 0, 3, 3);
        assert_damaged_with(
            open_7z(
                &seven_zip_slt(std::slice::from_ref(&forged)),
                &seven_zip_l_with(std::slice::from_ref(&real), &summary),
            ),
            "7-Zip forged record",
            "2 records but the tool counts 1 files",
        );

        let tail = unrar_record(&real)["        Name: a.bin\n".len()..].to_string();
        let mut forged = real.clone();
        forged.name = format!("a.bin\n{tail}\n        Name: evil.bin");
        assert_damaged_with(
            open_unrar(
                &unrar_lt(std::slice::from_ref(&forged)),
                &unrar_l_with(&[real], 3, 1),
            ),
            "unrar forged record",
            "2 records but the tool counts 1 files",
        );
    }

    #[test]
    fn record_key_set_mismatch_rejected() {
        let listing = two_files();
        let good = seven_zip_slt(&listing);
        let l = seven_zip_l(&listing);
        let second = good.rfind("Path = b.bin").expect("second record");
        let (head, tail) = good.split_at(second);
        let cases = [
            (
                "a key missing from one record",
                format!("{head}{}", tail.replacen("Encrypted = -\n", "", 1)),
            ),
            (
                "an unknown key",
                format!(
                    "{head}{}",
                    tail.replacen("Block = 0\n", "Block = 0\nComment = x\n", 1)
                ),
            ),
            (
                "keys out of order",
                format!(
                    "{head}{}",
                    tail.replacen(
                        "Size = 5\nPacked Size = 5\n",
                        "Packed Size = 5\nSize = 5\n",
                        1
                    )
                ),
            ),
            (
                "a key twice",
                format!(
                    "{head}{}",
                    tail.replacen("CRC = ", "Method = LZMA:26\nCRC = ", 1)
                ),
            ),
            (
                "a required key missing everywhere",
                good.replace("Size = 3\n", "").replace("Size = 5\n", ""),
            ),
        ];
        for (what, slt) in cases {
            assert_damaged(open_7z(&slt, &l), what);
        }

        let lt = unrar_lt(&listing);
        let l = unrar_l(&listing);
        let cases = [
            (
                "unrar unknown key",
                lt.replacen("     Host OS: Unix\n", "      Secret: 1\n", 1),
            ),
            (
                "unrar file without a size",
                lt.replacen("        Size: 3\n", "", 1),
            ),
            (
                "unrar misaligned key",
                lt.replacen("        Name: b.bin", "Name: b.bin", 1),
            ),
            (
                "unrar keys out of order",
                lt.replacen(
                    "        Name: a.bin\n        Type: File\n",
                    "        Type: File\n        Name: a.bin\n",
                    1,
                ),
            ),
            (
                "unrar stray preamble line",
                lt.replacen("Details: RAR 5\n", "Details: RAR 5\nComment: hi\n", 1),
            ),
        ];
        for (what, lt) in cases {
            assert_damaged(open_unrar(&lt, &l), what);
        }
    }

    #[test]
    fn record_count_vs_summary_mismatch_rejected() {
        let listing = two_files();
        let slt = seven_zip_slt(&listing);
        for (what, files, folders) in [("files", 3, 0), ("folders", 2, 1), ("fewer", 1, 0)] {
            let l = seven_zip_l_with(&listing, &seven_zip_summary_line(files, folders, 8, 8));
            assert_damaged_with(open_7z(&slt, &l), what, "records but the tool counts");
        }
        let no_summary = seven_zip_l_with(&listing, "");
        assert_damaged(open_7z(&slt, &no_summary), "no summary line");
        let errors = format!("{}\nErrors: 1\n", seven_zip_l(&listing));
        assert_damaged(open_7z(&slt, &errors), "7-Zip reported errors");

        let lt = unrar_lt(&listing);
        assert_damaged_with(
            open_unrar(&lt, &unrar_l_with(&listing, 8, 3)),
            "unrar count",
            "records but the tool counts 3",
        );
        let no_totals = unrar_lt(&listing);
        assert_damaged(open_unrar(&lt, &no_totals), "unrar without totals");
    }

    #[test]
    fn size_total_mismatch_rejected() {
        let listing = two_files();
        let l = seven_zip_l_with(&listing, &seven_zip_summary_line(2, 0, 9, 8));
        assert_damaged_with(
            open_7z(&seven_zip_slt(&listing), &l),
            "7-Zip size total",
            "sizes add up to 8",
        );
        assert_damaged_with(
            open_unrar(&unrar_lt(&listing), &unrar_l_with(&listing, 7, 2)),
            "unrar size total",
            "sizes add up to 8",
        );
    }

    #[test]
    fn archive_changed_between_two_list_runs_rejected() {
        setup();
        let listing = two_files();
        let mut script = seven_zip_listing_script(&seven_zip_slt(&listing), &seven_zip_l(&listing));
        // The summary run (second case) grows the archive after printing.
        script.cases[1].append_to_archive = "more".into();
        let tmp = tempfile::tempdir().expect("tmp");
        let tool = seven_zip(tmp.path(), &script);
        let r = ToolReader::open(
            &[tool],
            &archive(tmp.path()),
            Format::SevenZip,
            &Limits::default(),
        );
        assert_damaged_with(r, "archive changed", "changed while it was listed");
    }

    #[test]
    fn unsafe_name_rejects_whole_listing() {
        for bad in [
            "a\u{1b}[31m.bin",
            "a\u{85}.bin",
            "../evil.bin",
            "/etc/passwd",
            "CON.bin",
        ] {
            let mut listing = two_files();
            listing[1].name = bad.into();
            assert_unsafe_names(
                open_7z(&seven_zip_slt(&listing), &seven_zip_l(&listing)),
                bad,
            );
            assert_unsafe_names(open_unrar(&unrar_lt(&listing), &unrar_l(&listing)), bad);
        }
    }

    #[test]
    fn rar_without_tool_is_unsupported_need_unrar() {
        setup();
        let tmp = tempfile::tempdir().expect("tmp");
        let archive = archive(tmp.path());
        let need_unrar = |r: Result<ToolReader, ArchiveError>, what: &str| match r {
            Err(ArchiveError::Unsupported {
                need: Some(Tool::Unrar),
                ..
            }) => {}
            Err(e) => panic!("{what}: {e:?}"),
            Ok(_) => panic!("{what}: opened"),
        };
        need_unrar(
            ToolReader::open(&[], &archive, RAR5, &Limits::default()),
            "no tools",
        );
        // bsdtar is never text-parsed: it is no listing tool.
        let bsdtar = found(
            Tool::Bsdtar,
            fake_tool(tmp.path(), "bsdtar", &FakeScript::default()),
        );
        need_unrar(
            ToolReader::open(&[bsdtar], &archive, RAR5, &Limits::default()),
            "only bsdtar",
        );
        // unrar 4 cannot open RAR5.
        let mut old = unrar(tmp.path(), &FakeScript::default());
        old.version = "4.20".into();
        old.rar5 = false;
        need_unrar(
            ToolReader::open(&[old], &archive, RAR5, &Limits::default()),
            "unrar 4 for RAR5",
        );
        // Setting `organize.external_tools: off`.
        let listing = two_files();
        let other = tempfile::tempdir().expect("tmp");
        let tool = unrar(
            other.path(),
            &unrar_listing_script(&unrar_lt(&listing), &unrar_l(&listing)),
        );
        let off = Limits {
            external_tools: false,
            ..Limits::default()
        };
        need_unrar(ToolReader::open(&[tool], &archive, RAR5, &off), "tools off");
        // A 7z the crate cannot decode asks for 7-Zip.
        match ToolReader::open(&[], &archive, Format::SevenZip, &Limits::default()) {
            Err(ArchiveError::Unsupported {
                need: Some(Tool::SevenZip),
                ..
            }) => {}
            other => panic!("7z without 7-Zip: {:?}", other.map(|r| names(&r))),
        }
    }

    #[test]
    fn tool_below_pinned_version_not_used() {
        setup();
        let tmp = tempfile::tempdir().expect("tmp");
        let listing = two_files();
        let mut tool = seven_zip(
            tmp.path(),
            &seven_zip_listing_script(&seven_zip_slt(&listing), &seven_zip_l(&listing)),
        );
        tool.version = "9.20".into();
        let r = ToolReader::open(
            &[tool],
            &archive(tmp.path()),
            Format::SevenZip,
            &Limits::default(),
        );
        assert!(
            matches!(
                r,
                Err(ArchiveError::Unsupported {
                    need: Some(Tool::SevenZip),
                    ..
                })
            ),
            "7-Zip 9.20 predates the pinned switches"
        );
    }

    #[test]
    fn listing_failure_is_damaged_or_encrypted() {
        setup();
        let tmp = tempfile::tempdir().expect("tmp");
        let failing = FakeScript {
            cases: vec![FakeCase {
                stderr: "ERROR: game.7z : Can not open the file as archive\n".into(),
                exit: 2,
                ..FakeCase::default()
            }],
        };
        let tool = seven_zip(tmp.path(), &failing);
        assert_damaged(
            ToolReader::open(
                &[tool],
                &archive(tmp.path()),
                Format::SevenZip,
                &Limits::default(),
            ),
            "exit 2",
        );
        let locked = FakeScript {
            cases: vec![FakeCase {
                stderr: "ERROR: game.7z : Can not open encrypted archive. Wrong password?\n".into(),
                exit: 2,
                ..FakeCase::default()
            }],
        };
        // A fresh folder: on macOS, overwriting an executable that has run
        // invalidates its code signature and the next run is killed.
        let tmp = tempfile::tempdir().expect("tmp");
        let tool = seven_zip(tmp.path(), &locked);
        let r = ToolReader::open(
            &[tool],
            &archive(tmp.path()),
            Format::SevenZip,
            &Limits::default(),
        );
        assert!(matches!(r, Err(ArchiveError::Encrypted)), "{:?}", r.err());
    }

    #[test]
    fn listing_limits_apply() {
        let listing = two_files();
        setup();
        let tmp = tempfile::tempdir().expect("tmp");
        let tool = seven_zip(
            tmp.path(),
            &seven_zip_listing_script(&seven_zip_slt(&listing), &seven_zip_l(&listing)),
        );
        let few = Limits {
            max_entries: 1,
            ..Limits::default()
        };
        let r = ToolReader::open(
            std::slice::from_ref(&tool),
            &archive(tmp.path()),
            Format::SevenZip,
            &few,
        );
        assert!(
            matches!(r, Err(ArchiveError::TooManyEntries { .. })),
            "{:?}",
            r.err()
        );
        let small = Limits {
            max_entry_size: 4,
            ..Limits::default()
        };
        let r = ToolReader::open(&[tool], &archive(tmp.path()), Format::SevenZip, &small);
        assert!(
            matches!(r, Err(ArchiveError::TooLarge { .. })),
            "{:?}",
            r.err()
        );
    }

    #[test]
    fn unrar_optional_and_windows_keys_accepted() {
        // unrar.exe prints `Modified`/`Created`/`Accessed` where Unix builds
        // print `mtime`/`ctime`/`atime`; `RR%`, `File version` and `Unix
        // owner` appear only for some headers (unrar list.cpp).
        let listing = vec![
            ToolEntry::file("a.bin", 3, 0x3524_41C2),
            ToolEntry::file("b.bin", 5, 0x0102_0304),
            ToolEntry::file("link", 0, 0),
        ];
        let lt = unrar_lt(&listing)
            .replacen(
                "       Ratio: 100%\n       mtime: 2024-01-01 10:00:00,000000000\n",
                "       Ratio: 100%\n         RR%: 3%\n    Modified: 2024-01-01 10:00:00,000000000\n     Created: 2024-01-01 10:00:00,000000000\n    Accessed: 2024-01-01 10:00:00,000000000\n",
                1,
            )
            .replacen(
                " Compression: RAR 5.0(v50) -m3 -md=128K\n\n        Name: b.bin",
                " Compression: RAR 5.0(v50) -m3 -md=128K\n       Flags: solid \nFile version: 2\n  Unix owner: me:staff\n\n        Name: b.bin",
                1,
            )
            .replacen(
                "        Name: link\n        Type: File\n",
                "        Name: link\n        Type: NTFS junction point\n      Target: C:\\x\n",
                1,
            );
        let r = open_unrar(&lt, &unrar_l(&listing)).expect("listing");
        let kinds: Vec<_> = r.entries().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![EntryKind::File, EntryKind::File, EntryKind::Symlink]
        );
        assert_eq!(r.entries()[0].solid_block, Some(0), "Flags: solid");
        assert_eq!(r.entries()[1].solid_block, None);
    }

    #[test]
    fn unrar_totals_separator_of_every_version_accepted() {
        // unrar 5/6 print a 9-dash size column, unrar 7 a 10-dash one.
        let listing = two_files();
        let lt = unrar_lt(&listing);
        let l = unrar_l(&listing);
        let sep = l
            .lines()
            .find(|line| line.starts_with("-----------"))
            .expect("separator")
            .to_string();
        for width in [9usize, 10] {
            let other = format!("----------- {}  ---------- -----  ----", "-".repeat(width));
            let l = l.replace(&sep, &other);
            open_unrar(&lt, &l).unwrap_or_else(|e| panic!("{width} dashes: {e:?}"));
        }
        // A separator-like line that is no separator is still no summary.
        let l = l.replace(&sep, "----------- ---------- x ---------- -----  ----");
        assert_damaged(open_unrar(&lt, &l), "not a separator");
    }

    #[test]
    fn links_and_encrypted_entries_are_marked() {
        let listing = two_files();
        let slt =
            seven_zip_slt(&listing).replacen("Attributes = A\n", "Attributes = A lrwxr-xr-x\n", 1);
        let slt = {
            let at = slt.rfind("Encrypted = -").expect("second");
            format!(
                "{}Encrypted = +{}",
                &slt[..at],
                &slt[at + "Encrypted = -".len()..]
            )
        };
        let r = open_7z(&slt, &seven_zip_l(&listing)).expect("listing");
        assert_eq!(r.entries()[0].kind, EntryKind::Symlink);
        assert!(r.entries()[1].encrypted);
        assert_eq!(r.entries()[1].kind, EntryKind::File);
    }
}

/// The real case of spec §1, listed read-only with the 7-Zip on this
/// machine (`7zz l` only). Skips when the volume is not mounted or no
/// 7-Zip is installed.
#[test]
#[ignore = "needs the real download pile and 7-Zip"]
fn real_sample_lists_through_seven_zip() {
    use coverly_core::archive::external::{discover, ToolReader};
    use coverly_core::archive::{ArchiveReader, EntryKind, Format, Limits};
    let dir = std::env::var_os("COVERLY_SAMPLE_DIR").map_or_else(
        || PathBuf::from("/Volumes/TinyExt/ROMs-Downloaded"),
        PathBuf::from,
    );
    if !dir.is_dir() {
        eprintln!("skipped: {} is not mounted", dir.display());
        return;
    }
    let tools = discover(&[dir.as_path()]);
    if !tools.iter().any(|t| t.tool == Tool::SevenZip) {
        eprintln!("skipped: no 7-Zip found");
        return;
    }
    let cases = [
        ("Next Tetris, The.zip", 13, 405_749_493),
        ("Tetris Plus.zip", 0, 0),
        ("Next Tetris, The_ On-Line Edition.zip", 0, 0),
    ];
    for (name, want_entries, want_bytes) in cases {
        let r = ToolReader::open(
            &tools,
            &dir.join(name),
            Format::SevenZip,
            &Limits::default(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let files = r
            .entries()
            .iter()
            .filter(|e| e.kind == EntryKind::File)
            .count();
        let bytes: u64 = r.entries().iter().map(|e| e.size).sum();
        eprintln!(
            "{name}: {} entries, {files} files, {bytes} bytes",
            r.entries().len()
        );
        if want_entries > 0 {
            assert_eq!(
                (r.entries().len(), bytes),
                (want_entries, want_bytes),
                "{name}"
            );
        }
    }
}
