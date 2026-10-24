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
