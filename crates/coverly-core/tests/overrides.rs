//! Manual identities (D32): data, not cache; tolerant of bad files.

use std::fs;
use std::path::Path;

use coverly_core::identify::overrides::*;

fn id(name: &str) -> ManualIdentity {
    ManualIdentity {
        name: name.into(),
        source: "libretro".into(),
        set_at: 1,
    }
}

#[test]
fn set_get_remove_and_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(OVERRIDES_FILE);
    let rom = dir.path().join("ROMS/GBA/Gldn Rsh.gba");
    let mut o = Overrides::default();
    assert!(o.set(&rom, id("Golden Rush (USA)")));
    assert_eq!(o.get(&rom).unwrap().name, "Golden Rush (USA)");
    o.save(&file).unwrap();
    let back = Overrides::read(&file);
    assert_eq!(back, o);
    let mut back = back;
    assert!(back.remove(&rom));
    assert!(!back.remove(&rom));
    assert!(back.is_empty());
}

#[test]
fn hostile_names_are_refused() {
    let mut o = Overrides::default();
    let rom = Path::new("/lib/a.gba");
    assert!(!o.set(rom, id("")));
    assert!(!o.set(rom, id("bad\u{0}name")));
    assert!(!o.set(rom, id(&"x".repeat(256))));
    assert!(o.is_empty());
}

#[test]
fn a_corrupt_file_reads_as_empty_and_is_set_aside_before_an_update() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(OVERRIDES_FILE);
    fs::write(&file, "{ nope").unwrap();
    assert!(Overrides::read(&file).is_empty());
    assert!(file.exists(), "read never touches the file");
    assert!(Overrides::load_for_update(&file).is_empty());
    assert!(!file.exists());
    let kept: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        kept.iter()
            .any(|n| n.starts_with("identity-overrides.json.bad-")),
        "{kept:?}"
    );
}

#[test]
fn remember_and_forget_update_the_data_dir() {
    let dir = tempfile::tempdir().unwrap();
    let rom = dir.path().join("a.gba");
    remember_manual(dir.path(), &rom, "A (USA)", "libretro").unwrap();
    assert_eq!(
        Overrides::read(&dir.path().join(OVERRIDES_FILE))
            .get(&rom)
            .unwrap()
            .name,
        "A (USA)"
    );
    assert!(forget_manual(dir.path(), &rom).unwrap());
    assert!(!forget_manual(dir.path(), &rom).unwrap());
}
