//! Folder names organise creates, per layout (spec §8.1).

use std::collections::BTreeMap;

use coverly_core::profiles::naming::{system_folder, validate_folder_override};
use coverly_core::profiles::{builtin_profiles, FolderNaming, Profile};
use coverly_core::systems::SystemDb;

fn profile(id: &str) -> Profile {
    builtin_profiles().into_iter().find(|p| p.id == id).unwrap()
}

fn folder(profile_id: &str, system_id: &str) -> String {
    let db = SystemDb::builtin();
    let p = profile(profile_id);
    system_folder(&p, db.get(system_id).unwrap(), &db, &BTreeMap::new()).folder
}

#[test]
fn anbernic_family_uses_the_sd_card_names() {
    for id in ["anbernic-stock", "garlicos", "onion", "minui"] {
        assert_eq!(profile(id).system_folder, FolderNaming::FirstAlias, "{id}");
    }
    assert_eq!(folder("anbernic-stock", "gba"), "GBA");
    assert_eq!(folder("anbernic-stock", "snes"), "SFC");
    assert_eq!(folder("anbernic-stock", "nes"), "FC");
    assert_eq!(folder("anbernic-stock", "megadrive"), "MD");
    assert_eq!(folder("anbernic-stock", "psx"), "PS");
}

#[test]
fn frontends_use_ids() {
    for id in ["esde", "batocera", "muos", "retroarch"] {
        assert_eq!(profile(id).system_folder, FolderNaming::Id, "{id}");
    }
    assert_eq!(folder("esde", "snes"), "snes");
    assert_eq!(folder("batocera", "megadrive"), "megadrive");
}

#[test]
fn colour_handhelds_share_their_host_folder_on_anbernic() {
    let db = SystemDb::builtin();
    let p = profile("anbernic-stock");
    let ngpc = system_folder(&p, db.get("ngpc").unwrap(), &db, &BTreeMap::new());
    assert_eq!(ngpc.folder, "NGP");
    assert_eq!(ngpc.host.as_deref(), Some("ngp"));
    assert_eq!(folder("anbernic-stock", "wonderswancolor"), "WS");
    assert_eq!(
        folder("esde", "ngpc"),
        "ngpc",
        "frontends keep their own folder"
    );
}

#[test]
fn every_profile_and_system_folder_resolves_back() {
    let db = SystemDb::builtin();
    for p in builtin_profiles() {
        for s in db.all() {
            let choice = system_folder(&p, s, &db, &BTreeMap::new());
            let back = db.resolve_folder(&choice.folder).map(|r| r.id.clone());
            let want = choice.host.clone().unwrap_or_else(|| s.id.clone());
            assert_eq!(back, Some(want), "{} × {} → {}", p.id, s.id, choice.folder);
        }
    }
}

#[test]
fn user_overrides_win_and_must_resolve() {
    let db = SystemDb::builtin();
    let p = profile("minui");
    let mut map = BTreeMap::new();
    map.insert("gba".to_string(), "gameboyadvance".to_string());
    assert_eq!(
        system_folder(&p, db.get("gba").unwrap(), &db, &map).folder,
        "gameboyadvance"
    );
    assert!(validate_folder_override(&db, &p, "gba", "gameboyadvance").is_ok());
    assert!(validate_folder_override(&db, &p, "gba", "Advance Stuff").is_err());
    assert!(
        validate_folder_override(&db, &p, "gba", "SFC").is_err(),
        "resolves to snes"
    );
}

#[test]
fn a_system_the_device_does_not_show_is_flagged() {
    let db = SystemDb::builtin();
    let p = profile("anbernic-stock");
    assert_eq!(
        system_folder(&p, db.get("gba").unwrap(), &db, &BTreeMap::new()).on_device,
        Some(true)
    );
    let mut map = BTreeMap::new();
    map.insert("gba".to_string(), "gameboyadvance".to_string());
    assert_eq!(
        system_folder(&p, db.get("gba").unwrap(), &db, &map).on_device,
        Some(false)
    );
    let esde = profile("esde");
    assert_eq!(
        system_folder(&esde, db.get("gba").unwrap(), &db, &BTreeMap::new()).on_device,
        None
    );
}

#[test]
fn arcade_order_defaults() {
    assert_eq!(
        profile("anbernic-stock").arcade_order[..2],
        ["fbneo".to_string(), "mame".to_string()]
    );
    assert_eq!(profile("esde").arcade_order, ["fbneo", "mame"]);
}
