mod common;

use common::{anbernic_tree, open, write};
use coverly_core::identify::names::Region;
use coverly_core::profiles::Media;
use coverly_core::scan::{ScanOptions, ScanProgress};
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::library::view::ArtMark;
use coverly_roms_lib::library::{open_session, OpenRequest};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn opens_an_anbernic_sd_with_art_status() {
    let tree = anbernic_tree(&["SFC", "GBA"], 4, 2);
    let (session, view) = open(tree.path(), None).unwrap();

    assert_eq!(view.generation, 7);
    assert_eq!(view.profile.id, "anbernic-stock");
    assert!(view.profile.detected);
    assert!(!view.profile.fitted);
    assert_eq!(view.profile.media, vec![Media::Cover]);
    assert_eq!(view.entries.len(), 8);
    assert_eq!(session.entries.len(), 8);
    assert_eq!(session.generation, 7);
    assert!(view
        .entries
        .iter()
        .enumerate()
        .all(|(i, e)| e.id as usize == i));
    let present = view
        .entries
        .iter()
        .filter(|e| e.art.cover == ArtMark::Present)
        .count();
    assert_eq!(present, 4);
    assert!(view
        .entries
        .iter()
        .all(|e| e.art.screenshot == ArtMark::Unsupported && e.art.title == ArtMark::Unsupported));

    // Folders are sorted by name: GBA before SFC.
    let first = &view.entries[0];
    let system = &view.systems[first.system as usize];
    assert_eq!(system.folder, "GBA");
    assert_eq!(system.system_id.as_deref(), Some("gba"));
    assert_eq!(system.name, "Game Boy Advance");
    assert_eq!(system.dir, "ROMS/GBA");
    assert_eq!(system.roms, 4);
    assert_eq!(first.rel_path, "ROMS/GBA/Game 00000 (USA).gba");
    assert_eq!(first.dir, "ROMS/GBA");
    assert_eq!(first.name, "Game 00000 (USA)");
    assert_eq!(first.ext, "gba");
    assert_eq!(first.size, 1);
    assert!(first.mtime > 1_600_000_000);
    assert_eq!(first.regions, vec![Region::Usa]);
    assert!(session.entries[0].art.contains_key(&Media::Cover));
    assert_eq!(
        view.name,
        tree.path().file_name().unwrap().to_string_lossy()
    );
}

#[test]
fn opens_the_roms_folder_itself() {
    let tree = anbernic_tree(&["SFC", "GBA"], 4, 2);
    let (_, view) = open(&tree.path().join("ROMS"), None).unwrap();
    assert!(view.profile.fitted);
    assert_eq!(view.entries.len(), 8);
    assert_eq!(view.entries[0].rel_path, "GBA/Game 00000 (USA).gba");
    assert_eq!(view.entries[0].dir, "GBA");
}

#[test]
fn an_explicit_profile_skips_detection() {
    let tree = anbernic_tree(&["GBA"], 1, 0);
    let (_, view) = open(tree.path(), Some("garlicos")).unwrap();
    assert_eq!(view.profile.id, "garlicos");
    assert!(!view.profile.detected);
}

#[test]
fn missing_folder_is_not_found() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("nope");
    assert_eq!(
        open(&missing, None).unwrap_err(),
        IpcError::NotFound(missing.display().to_string())
    );
}

#[test]
fn a_file_is_not_a_folder() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "file.txt", b"x");
    let file = dir.path().join("file.txt");
    assert_eq!(
        open(&file, None).unwrap_err(),
        IpcError::NotAFolder(file.display().to_string())
    );
}

#[test]
fn folder_without_layout_is_no_profile() {
    let dir = TempDir::new().unwrap();
    assert!(matches!(
        open(dir.path(), None).unwrap_err(),
        IpcError::NoProfile(_)
    ));
}

#[test]
fn unknown_profile_is_rejected() {
    let tree = anbernic_tree(&["GBA"], 1, 0);
    assert_eq!(
        open(tree.path(), Some("nope")).unwrap_err(),
        IpcError::UnknownProfile("nope".into())
    );
}

#[test]
fn progress_reaches_the_total() {
    let tree = anbernic_tree(&["SFC", "GBA", "MD"], 2, 0);
    let mut events: Vec<ScanProgress> = Vec::new();
    open_session(
        &OpenRequest {
            path: tree.path(),
            profile_id: None,
            scan: &ScanOptions::default(),
            generation: 1,
            custom: &[],
            default_profile: None,
            settings: None,
        },
        &SystemDb::builtin(),
        &mut |p| events.push(p),
    )
    .unwrap();
    assert_eq!(events.first().unwrap().done, 0);
    let last = events.last().unwrap();
    assert_eq!((last.done, last.total, last.roms), (3, 3, 6));
}

#[cfg(unix)]
#[test]
fn hostile_names_are_kept_verbatim() {
    let tree = anbernic_tree(&["GBA"], 0, 0);
    write(
        tree.path(),
        "ROMS/GBA/<img src=x onerror=alert(1)>.gba",
        b"x",
    );
    write(tree.path(), "ROMS/GBA/\u{202E}gnp.gba", b"x");
    let (_, view) = open(tree.path(), None).unwrap();
    let names: Vec<&str> = view.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"<img src=x onerror=alert(1)>"), "{names:?}");
    assert!(names.contains(&"\u{202E}gnp"), "{names:?}");
}

#[test]
fn wire_format_is_snake_case_with_kebab_errors() {
    let tree = anbernic_tree(&["GBA"], 1, 1);
    let (_, view) = open(tree.path(), None).unwrap();
    let json = serde_json::to_value(&view).unwrap();
    let entry = &json["entries"][0];
    for key in [
        "id", "system", "rel_path", "dir", "name", "ext", "size", "mtime", "regions", "art",
    ] {
        assert!(entry.get(key).is_some(), "entry lacks {key}: {entry}");
    }
    assert_eq!(entry["art"]["cover"], "present");
    assert_eq!(entry["art"]["title"], "unsupported");
    assert_eq!(entry["regions"][0], "usa");
    assert_eq!(json["profile"]["media"], json!(["cover"]));
    assert_eq!(
        serde_json::to_value(IpcError::NotFound("/x".into())).unwrap(),
        json!({ "code": "not-found", "detail": "/x" })
    );
    assert_eq!(
        serde_json::to_value(IpcError::NoLibrary).unwrap(),
        json!({ "code": "no-library" })
    );
    assert_eq!(
        serde_json::to_value(IpcError::UnknownEntry(5)).unwrap(),
        json!({ "code": "unknown-entry", "detail": 5 })
    );
}
