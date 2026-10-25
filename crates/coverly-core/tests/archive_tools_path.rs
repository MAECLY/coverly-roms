//! A caller that passes no external tools runs none (spec §5.2): the
//! archive reader never searches `PATH` on its own, so an `unrar` planted
//! beside the ROMs and put on `PATH` is not run. Its own test binary: the
//! one test here changes `PATH` for the whole process.

use coverly_core::archive::cache::Cache;
use coverly_core::archive::testing::{fake_tool, FakeScript};
use coverly_core::archive::{open_archive, open_archive_with, ArchiveError, Limits, Tool};
use coverly_core::organize::inspect::{inspect_file_with, ArchiveIssue};
use coverly_core::systems::SystemDb;

const UNRAR_BANNER: &str = "UNRAR 7.01 freeware      Copyright (c) 1993-2024 Alexander Roshal";

fn needs_unrar(e: &ArchiveError) -> bool {
    matches!(e, ArchiveError::Unsupported { format, need: Some(Tool::Unrar) } if format == "RAR")
}

#[test]
fn no_tools_passed_never_searches_path() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("ROMs");
    let bin = source.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    fake_tool(&bin, "unrar", &FakeScript::banner(UNRAR_BANNER));
    let mut path = vec![bin.clone()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::set_var("PATH", std::env::join_paths(path).unwrap());

    let rar = source.join("Game.rar");
    let mut bytes = b"Rar!\x1a\x07\x00".to_vec();
    bytes.extend_from_slice(&[0u8; 64]);
    std::fs::write(&rar, bytes).unwrap();
    let limits = Limits::default();
    assert!(limits.external_tools);

    let err = open_archive(&rar, &limits).err().expect("no tool");
    assert!(needs_unrar(&err), "{err:?}");
    let err = open_archive_with(&rar, &limits, None)
        .err()
        .expect("no tool");
    assert!(needs_unrar(&err), "{err:?}");
    let err = Cache::new().listing(&rar, &limits).err().expect("no tool");
    assert!(needs_unrar(&err), "{err:?}");

    let len = std::fs::metadata(&rar).unwrap().len();
    let i = inspect_file_with(&rar, len, &SystemDb::builtin(), &limits, None).unwrap();
    assert_eq!(
        i.issue,
        Some(ArchiveIssue::Unsupported {
            format: Some("RAR".into()),
            need: Some(Tool::Unrar)
        })
    );
}
