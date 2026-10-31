//! Per-profile folder tables (systems batch 1, spec D5/D6): every `Named`
//! folder resolves back to its system under its profile, `Absent` ones are
//! marked as missing on the device, and names are Windows-safe. The tests
//! iterate whatever the tables hold, so they keep checking as systems land.

use std::collections::BTreeMap;

use coverly_core::profiles::naming::{system_folder, validate_folder_override};
use coverly_core::profiles::{builtin_profiles, ProfileFolder};
use coverly_core::systems::SystemDb;

const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn windows_safe(name: &str) -> bool {
    !name.is_empty()
        && !name.ends_with(['.', ' '])
        && !RESERVED.contains(&name.to_ascii_lowercase().as_str())
        && !name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*'])
}

#[test]
fn named_folders_resolve_back_and_absent_ones_are_marked() {
    let db = SystemDb::builtin();
    let none = BTreeMap::new();
    for p in builtin_profiles() {
        for (id, f) in &p.folders {
            let system = db
                .get(id)
                .unwrap_or_else(|| panic!("{}: unknown id {id}", p.id));
            let choice = system_folder(&p, system, &db, &none);
            match f {
                ProfileFolder::Named(n) => {
                    assert_eq!(&choice.folder, n, "{} {id}", p.id);
                    assert_eq!(
                        db.resolve_folder_in(Some(&p), n).map(|s| s.id.as_str()),
                        Some(id.as_str()),
                        "{} {n}",
                        p.id
                    );
                    assert!(validate_folder_override(&db, &p, id, n).is_ok());
                    assert!(windows_safe(n), "{n}");
                }
                ProfileFolder::Absent if choice.host.is_none() => {
                    assert_eq!(choice.on_device, Some(false), "{} {id}", p.id)
                }
                ProfileFolder::Absent => assert_ne!(choice.on_device, Some(false)),
            }
        }
    }
}

#[test]
fn first_aliases_are_windows_safe() {
    for s in SystemDb::builtin().all() {
        assert!(windows_safe(&s.aliases[0]), "{}", s.id);
    }
}

#[test]
fn retroarch_has_no_table_and_anbernic_is_all_absent() {
    let ps = builtin_profiles();
    let ra = ps.iter().find(|p| p.id == "retroarch").unwrap();
    assert!(ra.folders.is_empty());
    let anb = ps.iter().find(|p| p.id == "anbernic-stock").unwrap();
    assert!(anb.folders.values().all(|f| *f == ProfileFolder::Absent));
}

#[test]
fn existing_systems_keep_their_folders() {
    // No pre-batch system is in the tables (D5): same names as before.
    let db = SystemDb::builtin();
    let none = BTreeMap::new();
    for p in builtin_profiles() {
        for id in ["snes", "psx", "gba", "msx", "pcengine", "a800"] {
            assert!(!p.folders.contains_key(id), "{} {id}", p.id);
            let _ = system_folder(&p, db.get(id).unwrap(), &db, &none);
        }
    }
}

#[test]
fn a_named_folder_wins_over_global_aliases_under_its_profile_only() {
    // A synthetic profile: `GBA` named for snes resolves to snes under it,
    // to gba everywhere else.
    let db = SystemDb::builtin();
    let mut p = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "batocera")
        .unwrap();
    p.folders
        .insert("snes".into(), ProfileFolder::Named("GBA".into()));
    assert_eq!(p.folder_owner("gba"), Some("snes"));
    assert_eq!(
        db.resolve_folder_in(Some(&p), "gba").map(|s| s.id.as_str()),
        Some("snes")
    );
    assert_eq!(
        db.resolve_folder_in(None, "gba").map(|s| s.id.as_str()),
        Some("gba")
    );
    let choice = system_folder(&p, db.get("snes").unwrap(), &db, &BTreeMap::new());
    assert_eq!(choice.folder, "GBA");
    assert_eq!(choice.on_device, None);
}

#[test]
fn absent_folder_keeps_the_rule_name_and_is_not_on_device() {
    let db = SystemDb::builtin();
    let mut p = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "esde")
        .unwrap();
    p.folders.insert("snes".into(), ProfileFolder::Absent);
    let none = BTreeMap::new();
    let choice = system_folder(&p, db.get("snes").unwrap(), &db, &none);
    assert_eq!(choice.folder, "snes");
    assert_eq!(choice.on_device, Some(false));
    // A user override is the user's choice: no «no folder on device» note.
    let user = BTreeMap::from([("snes".to_string(), "SFC".to_string())]);
    let choice = system_folder(&p, db.get("snes").unwrap(), &db, &user);
    assert_eq!(choice.folder, "SFC");
    assert_eq!(choice.on_device, None);
}

#[test]
fn absent_system_with_a_host_keeps_the_host_folder_without_the_note() {
    // P-3: the host folder exists on the device by definition.
    let db = SystemDb::builtin();
    let mut p = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "onion")
        .unwrap();
    p.folders.insert("ngpc".into(), ProfileFolder::Absent);
    let choice = system_folder(&p, db.get("ngpc").unwrap(), &db, &BTreeMap::new());
    assert_eq!(choice.host.as_deref(), Some("ngp"));
    assert_ne!(choice.on_device, Some(false));
    assert_eq!(
        db.resolve_folder_in(Some(&p), &choice.folder)
            .map(|s| s.id.as_str()),
        Some("ngp")
    );
}

#[test]
fn scoped_aliases_resolve_only_under_a_profile() {
    let db = SystemDb::builtin()
        .with_overrides(
            r#"
            [[system]]
            id = "zzbox"
            name = "ZZ Box"
            aliases = ["zzbox", "dragon"]
            extensions = ["zzb"]
            scoped_aliases = ["dragon"]
            "#,
        )
        .unwrap();
    let p = builtin_profiles().into_iter().next().unwrap();
    assert!(db.resolve_folder_in(None, "Dragon").is_none());
    assert_eq!(
        db.resolve_folder_in(Some(&p), "Dragon")
            .map(|s| s.id.as_str()),
        Some("zzbox")
    );
    assert!(db.resolve_folder_in(Some(&p), "").is_none());
}
