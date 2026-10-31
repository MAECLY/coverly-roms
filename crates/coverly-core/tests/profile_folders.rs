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
fn every_alias_is_a_windows_safe_folder_name() {
    // Not only the first: any alias (scoped ones included) is accepted as
    // a user folder override or a custom layout's folder, and becomes one
    // path component under the ROMs root.
    for s in SystemDb::builtin().all() {
        for a in s.aliases.iter().chain(&s.scoped_aliases) {
            assert!(windows_safe(a), "{}: {a:?}", s.id);
        }
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

fn builtin(id: &str) -> coverly_core::profiles::Profile {
    builtin_profiles().into_iter().find(|p| p.id == id).unwrap()
}

#[test]
fn anbernic_hosts_keep_old_destinations() {
    // D23: the stock card has `PCE` and `SFC` only, so a SuperGrafx or
    // Satellaview game lands where it did before the batch.
    let db = SystemDb::builtin();
    let p = builtin("anbernic-stock");
    let none = BTreeMap::new();
    for (id, host, folder) in [
        ("supergrafx", "pcengine", "PCE"),
        ("satellaview", "snes", "SFC"),
    ] {
        let c = system_folder(&p, db.get(id).unwrap(), &db, &none);
        assert_eq!(
            (c.folder.as_str(), c.host.as_deref(), c.on_device),
            (folder, Some(host), Some(true)),
            "{id}"
        );
    }
    // Sufami Turbo never gets a host (no evidence the SFC core runs it).
    let c = system_folder(&p, db.get("sufami").unwrap(), &db, &none);
    assert_eq!((c.host, c.on_device), (None, Some(false)));
    // Onion has its own SGFX and SATELLAVIEW folders: no host there.
    let onion = builtin("onion");
    let c = system_folder(&onion, db.get("supergrafx").unwrap(), &db, &none);
    assert_eq!((c.folder.as_str(), c.host), ("SGFX", None));
}

#[test]
fn pico_is_sega_pico_only_on_batocera() {
    use coverly_core::profiles::custom::{validate_profile, ProfileFile};
    // D6: `pico` is Sega Pico on Batocera/Knulli, PICO-8 elsewhere and
    // with no profile.
    let db = SystemDb::builtin();
    let id = |s: Option<&coverly_core::systems::System>| s.map(|s| s.id.clone());
    let batocera = builtin("batocera");
    assert_eq!(
        id(db.resolve_folder_in(Some(&batocera), "pico")),
        Some("pico".into())
    );
    for other in ["onion", "anbernic-stock", "muos"] {
        assert_eq!(
            id(db.resolve_folder_in(Some(&builtin(other)), "PICO")),
            Some("pico8".into()),
            "{other}"
        );
    }
    assert_eq!(id(db.resolve_folder("pico")), Some("pico8".into()));
    assert_eq!(
        id(db.resolve_folder_in(Some(&builtin("muos")), "segapico")),
        Some("pico".into())
    );
    // A custom Batocera clone keeps `pico` as Sega Pico's folder.
    let mut file = ProfileFile::from_profile(&batocera);
    file.id = "my-batocera".into();
    assert_eq!(
        file.folder_names.get("pico").map(String::as_str),
        Some("pico")
    );
    let ids: Vec<String> = builtin_profiles().into_iter().map(|p| p.id).collect();
    assert!(validate_profile(&file, &ids).is_empty());
}

#[test]
fn scoped_aliases_need_a_profile() {
    // D22: everyday words (`Dragon`, `coco`, `moto`, `bbc`) resolve only
    // under a profile, so a pile folder named so is still organised and
    // profile detection never counts it. (`PET` joins with Task 10.)
    let db = SystemDb::builtin();
    for name in [
        "Dragon", "dragon32", "dragon64", "coco", "moto", "theodore", "bbc",
    ] {
        assert!(db.resolve_folder(name).is_none(), "{name}");
    }
    let id = |s: Option<&coverly_core::systems::System>| s.map(|s| s.id.clone());
    let batocera = builtin("batocera");
    for (name, want) in [
        ("dragon64", "dragon"),
        ("Dragon", "dragon"),
        ("coco", "coco"),
        ("moto", "thomson"),
        ("bbc", "bbcmicro"),
    ] {
        assert_eq!(
            id(db.resolve_folder_in(Some(&batocera), name)),
            Some(want.into()),
            "{name}"
        );
    }
    // Distinctive names stay global.
    for (name, want) in [
        ("zxspectrum", "zxspectrum"),
        ("ZXS", "zxspectrum"),
        ("amstradcpc", "amstradcpc"),
        ("BBCMICRO", "bbcmicro"),
        ("trs80coco", "coco"),
        ("thomson", "thomson"),
        ("to8", "thomson"),
        ("ti99", "ti99"),
        ("zx81", "zx81"),
    ] {
        assert_eq!(id(db.resolve_folder(name)), Some(want.into()), "{name}");
    }
    // Computers II (Task 9): `x1` is scoped, `X1` (its first alias) too.
    for name in ["x1", "X1"] {
        assert!(db.resolve_folder(name).is_none(), "{name}");
    }
    let esde = builtin("esde");
    assert_eq!(
        id(db.resolve_folder_in(Some(&esde), "x1")),
        Some("x1".into())
    );
    for (name, want) in [
        ("x68000", "x68000"),
        ("x68k", "x68000"),
        ("pc98", "pc98"),
        ("pc9800", "pc98"),
        ("pc88", "pc88"),
        ("pc8000", "pc88"),
    ] {
        assert_eq!(id(db.resolve_folder(name)), Some(want.into()), "{name}");
    }
    let tmp = tempfile::tempdir().unwrap();
    for dir in ["Dragon", "coco"] {
        let d = tmp.path().join(dir);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("game.cas"), b"x").unwrap();
    }
    let detect = |root: &std::path::Path| {
        coverly_core::profiles::detect_with_fallback(root, "batocera", &db)
    };
    assert!(detect(tmp.path()).is_empty());
    // The same layout with distinctive names is a library.
    let other = tempfile::tempdir().unwrap();
    for dir in ["zxspectrum", "amstradcpc"] {
        let d = other.path().join(dir);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("game.dsk"), b"x").unwrap();
    }
    assert_eq!(detect(other.path()).len(), 1);
}

#[test]
fn computers_iii_folders_and_hosts() {
    // §6.1 and D23: MSX2 and Atari XE keep the parent's folder where the
    // device has only that one (Anbernic `MSX`/`A800`, Onion's `MSX`
    // package, NextUI's `MSX.pak`); their own folder elsewhere.
    let db = SystemDb::builtin();
    let none = BTreeMap::new();
    let target = |profile: &str, id: &str| {
        let c = system_folder(&builtin(profile), db.get(id).unwrap(), &db, &none);
        (c.folder, c.host, c.on_device)
    };
    // Only Anbernic stock lists its card's folders, so only it can say
    // the host folder is on the device.
    for (profile, id, folder, host, on_device) in [
        ("anbernic-stock", "msx2", "MSX", "msx", Some(true)),
        ("anbernic-stock", "atarixe", "A800", "a800", Some(true)),
        ("onion", "msx2", "MSX", "msx", None),
        ("garlicos", "msx2", "MSX", "msx", None),
        ("minui", "msx2", "MSX", "msx", None),
    ] {
        assert_eq!(
            target(profile, id),
            (folder.to_string(), Some(host.to_string()), on_device),
            "{profile} {id}"
        );
    }
    for (profile, id, folder) in [
        ("batocera", "atarixe", "xegs"),
        ("esde", "atarixe", "atarixe"),
        ("batocera", "plus4", "cplus4"),
        ("minui", "plus4", "PLUS4"),
        ("minui", "pet", "PET"),
        ("minui", "c128", "C128"),
        ("esde", "msx2", "msx2"),
    ] {
        let (f, host, _) = target(profile, id);
        assert_eq!((f.as_str(), host), (folder, None), "{profile} {id}");
    }
    // NextUI has no Atari 8-bit pak, and MSX turbo R never gets a host.
    for (profile, id) in [
        ("minui", "atarixe"),
        ("anbernic-stock", "msxturbor"),
        ("onion", "msxturbor"),
    ] {
        let (_, host, on_device) = target(profile, id);
        assert_eq!((host, on_device), (None, Some(false)), "{profile} {id}");
    }
    // `pet` and its first alias are scoped (D22).
    for name in ["pet", "PET"] {
        assert!(db.resolve_folder(name).is_none(), "{name}");
    }
    assert_eq!(
        db.resolve_folder_in(Some(&builtin("batocera")), "pet")
            .map(|s| s.id.as_str()),
        Some("pet")
    );
}

#[test]
fn port_folders_are_scoped_but_prboom_is_global() {
    // D22: `doom`, `quake`, `flashback`, `solarus` are everyday words a
    // pile folder may carry, so they resolve only under a profile;
    // `prboom` is distinctive and global.
    let db = SystemDb::builtin();
    for name in [
        "doom",
        "quake",
        "QUAKE",
        "flashback",
        "Solarus",
        "lutro",
        "mrboom",
        "xrick",
    ] {
        assert!(db.resolve_folder(name).is_none(), "{name}");
    }
    let id = |s: Option<&coverly_core::systems::System>| s.map(|s| s.id.clone());
    for (name, want) in [
        ("prboom", "doom"),
        ("PRBOOM", "doom"),
        ("tyrquake", "quake"),
        ("cavestory", "cavestory"),
        ("ecwolf", "wolf3d"),
        ("wolfenstein3d", "wolf3d"),
        ("reminiscence", "flashback"),
        ("DINOTHAWR", "dinothawr"),
        ("rickdangerous", "xrick"),
    ] {
        assert_eq!(id(db.resolve_folder(name)), Some(want.into()), "{name}");
    }
    let batocera = builtin("batocera");
    for (name, want) in [
        ("doom", "doom"),
        ("quake", "quake"),
        ("flashback", "flashback"),
        ("solarus", "solarus"),
        ("mrboom", "mrboom"),
        ("xrick", "xrick"),
    ] {
        assert_eq!(
            id(db.resolve_folder_in(Some(&batocera), name)),
            Some(want.into()),
            "{name}"
        );
    }
}
