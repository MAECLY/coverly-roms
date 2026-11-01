//! User-defined layout profiles (spec §5, D29).

use std::collections::BTreeMap;
use std::fs;

use coverly_core::archive::ContainerKind;
use coverly_core::profiles::custom::*;
use coverly_core::profiles::{builtin_profiles, DiscLayout, FolderNaming, ImageFormat, Media};
use coverly_core::settings::FieldErrorCode;
use coverly_core::systems::SystemDb;

fn builtin_ids() -> Vec<String> {
    builtin_profiles().into_iter().map(|p| p.id).collect()
}

fn sample() -> ProfileFile {
    ProfileFile {
        id: "my-sd".into(),
        name: "My SD".into(),
        roms_root: "Roms".into(),
        art_dirs: vec!["Imgs".into()],
        gamelist: false,
        slots: BTreeMap::from([(
            "cover".to_string(),
            SlotFile {
                template: "{system_dir}/{rel_dir}/Imgs/{stem}.png".into(),
                format: ImageFormat::Png,
                max_width: Some(250),
                max_height: Some(360),
            },
        )]),
        system_folder: FolderNaming::default(),
        folder_names: BTreeMap::new(),
        host_folders: BTreeMap::new(),
        disc_layout: DiscLayout::default(),
        accepts_containers: default_accepts_containers(),
        container_overrides: default_container_overrides(),
        folder_game_suffix: BTreeMap::new(),
        folder_stem_keeps_suffix: false,
    }
}

fn errors(p: &ProfileFile) -> Vec<(String, FieldErrorCode)> {
    validate_profile(p, &builtin_ids())
        .into_iter()
        .map(|e| (e.field, e.code))
        .collect()
}

fn has(p: &ProfileFile, field: &str, code: FieldErrorCode) -> bool {
    errors(p).contains(&(field.to_string(), code))
}

#[test]
fn a_valid_profile_round_trips_through_toml_and_into_a_profile() {
    let p = sample();
    assert!(errors(&p).is_empty(), "{:?}", errors(&p));
    let text = p.to_toml();
    assert!(text.contains("id = \"my-sd\""), "{text}");
    assert_eq!(ProfileFile::parse_toml(&text).unwrap(), p);
    let profile = p.to_profile();
    assert_eq!(profile.id, "my-sd");
    assert!(profile.mirror_subdirs);
    assert!(!profile.fixed_format && !profile.gamelist);
    let slot = profile.slot(Media::Cover).unwrap();
    assert_eq!(slot.max_size, Some((250, 360)));
    assert_eq!(ProfileFile::from_profile(&profile), p);
}

#[test]
fn every_builtin_profile_converts_and_validates_except_its_reserved_id() {
    for b in builtin_profiles() {
        let mut f = ProfileFile::from_profile(&b);
        assert!(has(&f, "id", FieldErrorCode::Duplicate), "{}", b.id);
        f.id = format!("copy-of-{}", b.id);
        assert!(errors(&f).is_empty(), "{}: {:?}", b.id, errors(&f));
        assert_eq!(f.to_profile().slots, b.slots, "{}", b.id);
    }
}

#[test]
fn ids_names_and_folders_are_checked() {
    let mut p = sample();
    p.id = "My SD!".into();
    assert!(has(&p, "id", FieldErrorCode::Invalid));
    p.id = "../evil".into();
    assert!(has(&p, "id", FieldErrorCode::Invalid));
    let mut p = sample();
    p.name = "  ".into();
    assert!(has(&p, "name", FieldErrorCode::Required));
    p.name = "bad\u{7}".into();
    assert!(has(&p, "name", FieldErrorCode::Invalid));
    for root in ["../up", "/abs", "a/../b", "C:\\x", "a\\b"] {
        let mut p = sample();
        p.roms_root = root.into();
        assert!(has(&p, "roms_root", FieldErrorCode::Invalid), "{root}");
    }
    let mut p = sample();
    p.art_dirs = vec!["a/b".into()];
    assert!(has(&p, "art_dirs", FieldErrorCode::Invalid));
}

#[test]
fn folder_game_suffixes_load_round_trip_and_are_checked() {
    // A file from before the field loads with none and writes none.
    let old = sample().to_toml();
    assert!(!old.contains("folder_game_suffix"));
    let back = ProfileFile::parse_toml(&old).unwrap();
    assert!(back.folder_game_suffix.is_empty());
    assert!(back.to_profile().folder_game_suffix.is_empty());

    let mut p = sample();
    p.folder_game_suffix.insert("psx".into(), ".psx".into());
    assert!(errors(&p).is_empty(), "{:?}", errors(&p));
    let back = ProfileFile::parse_toml(&p.to_toml()).unwrap();
    assert_eq!(back.folder_game_suffix, p.folder_game_suffix);
    let profile = back.to_profile();
    assert_eq!(
        profile.folder_game_suffix.get("psx").map(String::as_str),
        Some(".psx")
    );
    assert_eq!(
        ProfileFile::from_profile(&profile).folder_game_suffix,
        p.folder_game_suffix
    );

    for (system, suffix) in [
        ("nope", ".psx"),
        ("psx", "psx"),
        ("psx", "."),
        ("psx", ".p/x"),
        ("psx", ".p x"),
        ("psx", ".abcdefghijklmnopq"),
    ] {
        let mut p = sample();
        p.folder_game_suffix.insert(system.into(), suffix.into());
        assert!(
            has(
                &p,
                &format!("folder_game_suffix.{system}"),
                FieldErrorCode::Invalid
            ),
            "{system} {suffix}"
        );
    }
}

#[test]
fn templates_must_stay_inside_the_library() {
    let cases = [
        ("{root}/../outside/{stem}.png", FieldErrorCode::Invalid),
        ("/abs/{stem}.png", FieldErrorCode::Invalid),
        ("Imgs/{stem}.png", FieldErrorCode::Invalid),
        ("{system_dir}/./Imgs/{stem}.png", FieldErrorCode::Invalid),
        ("{system_dir}//{stem}.png", FieldErrorCode::Invalid),
        ("{system_dir}\\{stem}.png", FieldErrorCode::Invalid),
        ("{system_dir}/{nope}/{stem}.png", FieldErrorCode::Unknown),
        ("{system_dir}/Imgs/cover.png", FieldErrorCode::Invalid),
        ("{system_dir}/Imgs/{stem}.jpg", FieldErrorCode::Invalid),
        ("", FieldErrorCode::Required),
        // Path tokens only as whole segments: an absolute root pushed
        // inside a segment replaces the whole path.
        ("{root}/{root}{stem}.png", FieldErrorCode::Invalid),
        ("{system_dir}/{root}x/{stem}.png", FieldErrorCode::Invalid),
        ("{system_dir}/{root}/{stem}.png", FieldErrorCode::Invalid),
        ("{root}/x/{system_dir}{stem}.png", FieldErrorCode::Invalid),
        ("{root}/{system_dir}/{stem}.png", FieldErrorCode::Invalid),
        ("{root}/{rel_dir}{stem}.png", FieldErrorCode::Invalid),
        ("{root}/x{rel_dir}/{stem}.png", FieldErrorCode::Invalid),
        ("{root}{stem}.png", FieldErrorCode::Invalid),
        // What is validated is what is saved: no stray spaces, no
        // dot-like or control-character segments.
        ("{root}/{stem}.png ", FieldErrorCode::Invalid),
        (" {root}/{stem}.png", FieldErrorCode::Invalid),
        ("{root}/ ../{stem}.png", FieldErrorCode::Invalid),
        ("{root}/.. /{stem}.png", FieldErrorCode::Invalid),
        ("{root}/ /{stem}.png", FieldErrorCode::Invalid),
        ("{root}/a\u{0}b/{stem}.png", FieldErrorCode::Invalid),
        ("{root}/C:/{stem}.png", FieldErrorCode::Invalid),
        ("   ", FieldErrorCode::Required),
    ];
    for (template, code) in cases {
        let mut p = sample();
        p.slots.get_mut("cover").unwrap().template = template.into();
        assert!(
            has(&p, "slots.cover.template", code),
            "{template}: {:?}",
            errors(&p)
        );
    }
}

#[test]
fn slots_are_checked_as_a_set() {
    let mut p = sample();
    p.slots.clear();
    assert!(has(&p, "slots", FieldErrorCode::Required));
    let mut p = sample();
    p.slots.insert(
        "marquee".into(),
        SlotFile {
            template: "{system_dir}/{stem}.png".into(),
            format: ImageFormat::Png,
            max_width: None,
            max_height: None,
        },
    );
    assert!(has(&p, "slots.marquee", FieldErrorCode::Unknown));
    let mut p = sample();
    p.slots.insert(
        "title".into(),
        SlotFile {
            template: "{system_dir}/titles/{stem}.png".into(),
            format: ImageFormat::Png,
            max_width: None,
            max_height: None,
        },
    );
    assert!(
        has(&p, "slots", FieldErrorCode::Order),
        "{{rel_dir}} in one slot only"
    );
    let mut p = sample();
    p.slots.get_mut("cover").unwrap().max_width = Some(8);
    assert!(has(&p, "slots.cover.max_width", FieldErrorCode::Range));
    let mut p = sample();
    p.slots.get_mut("cover").unwrap().max_height = None;
    assert!(
        has(&p, "slots.cover.max_height", FieldErrorCode::Required),
        "a width without a height would be dropped silently: {:?}",
        errors(&p)
    );
    let mut p = sample();
    p.slots.get_mut("cover").unwrap().max_width = None;
    assert!(has(&p, "slots.cover.max_width", FieldErrorCode::Required));
    let mut p = sample();
    let cover = p.slots.get_mut("cover").unwrap();
    cover.max_width = None;
    cover.max_height = None;
    assert!(errors(&p).is_empty(), "no size at all is fine");
}

#[test]
fn preview_renders_library_relative_paths() {
    let mut p = sample();
    p.slots.insert(
        "title".into(),
        SlotFile {
            template: "{root}/thumbs/{libretro}/{rel_dir}/{stem}.png".into(),
            format: ImageFormat::Png,
            max_width: None,
            max_height: None,
        },
    );
    let ex = PreviewExample {
        system_folder: "SFC".into(),
        rel_dir: "Hacks".into(),
        file_name: "Zelda (USA).sfc".into(),
    };
    let paths = preview_paths(&p, &ex, &SystemDb::builtin());
    assert_eq!(paths["cover"], "Roms/SFC/Hacks/Imgs/Zelda (USA).png");
    assert_eq!(
        paths["title"],
        "thumbs/Nintendo - Super Nintendo Entertainment System/Hacks/Zelda (USA).png"
    );
    let mut broken = p.clone();
    broken.slots.get_mut("cover").unwrap().template = "{root}/../{stem}.png".into();
    let paths = preview_paths(&broken, &ex, &SystemDb::builtin());
    assert!(
        !paths.contains_key("cover"),
        "a bad template has no preview"
    );
}

#[test]
fn the_store_saves_renames_deletes_and_skips_hostile_files() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProfileStore::new(dir.path().join(PROFILES_DIR));
    let (none, warnings) = store.load();
    assert!(none.is_empty() && warnings.is_empty());
    let p = sample();
    store.save(&p, None).unwrap();
    let mut renamed = p.clone();
    renamed.id = "my-sd-2".into();
    store.save(&renamed, Some("my-sd")).unwrap();
    let pdir = dir.path().join(PROFILES_DIR);
    assert!(!pdir.join("my-sd.toml").exists() && pdir.join("my-sd-2.toml").exists());
    fs::write(pdir.join("broken.toml"), "id = [").unwrap();
    fs::write(pdir.join("liar.toml"), sample().to_toml()).unwrap(); // id my-sd ≠ liar
    fs::write(
        pdir.join("big.toml"),
        "#".repeat(MAX_PROFILE_BYTES as usize + 1),
    )
    .unwrap();
    let mut builtin = sample();
    builtin.id = "onion".into();
    fs::write(pdir.join("onion.toml"), builtin.to_toml()).unwrap();
    fs::write(pdir.join("notes.txt"), "ignored").unwrap();
    let (loaded, warnings) = store.load();
    assert_eq!(
        loaded.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["my-sd-2"]
    );
    assert_eq!(warnings.len(), 4, "{warnings:?}");
    assert!(store.delete("my-sd-2").unwrap());
    assert!(!store.delete("my-sd-2").unwrap());
    assert!(store.delete("../../etc/passwd").is_err());
}

#[test]
fn custom_profiles_are_listed_after_the_builtins_and_found_by_id() {
    let customs = vec![sample()];
    let all = all_profiles(&customs);
    assert_eq!(all.len(), builtin_profiles().len() + 1);
    assert!(all[0].1, "built-ins first");
    assert_eq!(all.last().unwrap().0.id, "my-sd");
    assert!(!all.last().unwrap().1);
    assert_eq!(find_profile(&customs, "MY-SD").unwrap().id, "my-sd");
    assert_eq!(find_profile(&customs, "onion").unwrap().id, "onion");
    assert!(find_profile(&customs, "nope").is_none());
}

#[test]
fn only_batocera_writes_a_gamelist() {
    for p in builtin_profiles() {
        assert_eq!(p.gamelist, p.id == "batocera", "{}", p.id);
    }
}

#[test]
fn a_hostile_preview_example_never_points_outside_the_library() {
    let p = sample();
    for (system_folder, rel_dir) in [("..", ""), ("SFC", "../../etc"), ("a/../..", "x")] {
        let ex = PreviewExample {
            system_folder: system_folder.into(),
            rel_dir: rel_dir.into(),
            file_name: "Zelda (USA).sfc".into(),
        };
        let paths = preview_paths(&p, &ex, &SystemDb::builtin());
        assert!(
            paths
                .values()
                .all(|v| !v.split('/').any(|s| s == ".." || s.is_empty())),
            "{system_folder} {rel_dir}: {paths:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn folders_and_links_named_like_profiles_are_skipped_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let pdir = dir.path().join(PROFILES_DIR);
    fs::create_dir_all(pdir.join("folder.toml")).unwrap();
    let outside = dir.path().join("outside.toml");
    fs::write(&outside, sample().to_toml()).unwrap();
    std::os::unix::fs::symlink(&outside, pdir.join("my-sd.toml")).unwrap();
    let (loaded, warnings) = ProfileStore::new(pdir).load();
    assert!(loaded.is_empty());
    assert_eq!(warnings.len(), 2, "{warnings:?}");
}

#[test]
fn folder_names_must_resolve_to_their_system() {
    let mut p = sample();
    p.folder_names.insert("gba".into(), "GBA".into());
    assert!(errors(&p).is_empty(), "{:?}", errors(&p));
    p.folder_names.insert("snes".into(), "GBA".into());
    p.folder_names.insert("nes".into(), "My Stuff".into());
    assert!(has(&p, "folder_names.snes", FieldErrorCode::Invalid));
    assert!(has(&p, "folder_names.nes", FieldErrorCode::Invalid));
    assert!(!has(&p, "folder_names.gba", FieldErrorCode::Invalid));
}

#[test]
fn folder_naming_round_trips_and_defaults_stay_out_of_the_file() {
    let plain = sample();
    let text = plain.to_toml();
    assert!(!text.contains("system_folder"), "{text}");
    assert!(!text.contains("folder_names"), "{text}");
    assert_eq!(ProfileFile::parse_toml(&text).unwrap(), plain);

    let mut named = sample();
    named.system_folder = FolderNaming::Id;
    named.folder_names.insert("gba".into(), "gba".into());
    let text = named.to_toml();
    assert!(text.contains("system_folder = \"id\""), "{text}");
    let back = ProfileFile::parse_toml(&text).unwrap();
    assert_eq!(back, named);
    assert_eq!(back.to_profile().system_folder, FolderNaming::Id);

    let esde = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "esde")
        .unwrap();
    assert_eq!(
        ProfileFile::from_profile(&esde).system_folder,
        FolderNaming::Id
    );
}

#[test]
fn a_copy_of_the_anbernic_layout_keeps_its_shared_folders() {
    use coverly_core::profiles::naming::system_folder;
    let stock = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "anbernic-stock")
        .unwrap();
    let mut file = ProfileFile::from_profile(&stock);
    file.id = "my-anbernic".into();
    assert!(errors(&file).is_empty(), "{:?}", errors(&file));
    let back = ProfileFile::parse_toml(&file.to_toml()).unwrap();
    assert_eq!(back, file);
    let copy = back.to_profile();
    assert_eq!(copy.host_folders, stock.host_folders);
    let db = SystemDb::builtin();
    let ngpc = system_folder(&copy, db.get("ngpc").unwrap(), &db, &BTreeMap::new());
    assert_eq!(ngpc.folder, "NGP");

    let plain = sample();
    assert!(!plain.to_toml().contains("host_folders"));
}

#[test]
fn host_folders_must_name_two_different_known_systems() {
    let mut p = sample();
    p.host_folders.insert("ngpc".into(), "ngp".into());
    assert!(errors(&p).is_empty(), "{:?}", errors(&p));
    p.host_folders.insert("nope".into(), "ngp".into());
    p.host_folders.insert("gba".into(), "gba".into());
    p.host_folders
        .insert("wonderswancolor".into(), "zzz".into());
    assert!(has(&p, "host_folders.nope", FieldErrorCode::Invalid));
    assert!(has(&p, "host_folders.gba", FieldErrorCode::Invalid));
    assert!(has(
        &p,
        "host_folders.wonderswancolor",
        FieldErrorCode::Invalid
    ));
    assert!(!has(&p, "host_folders.ngpc", FieldErrorCode::Invalid));
}

#[test]
fn custom_profile_toml_disc_layout_round_trip() {
    let mut p = sample();
    p.disc_layout = DiscLayout::GameFolder {
        m3u_for_multi: true,
    };
    p.accepts_containers = vec![ContainerKind::Zip];
    p.container_overrides.insert(
        "nds".to_string(),
        vec![ContainerKind::Zip, ContainerKind::SevenZip],
    );
    assert!(errors(&p).is_empty(), "{:?}", errors(&p));
    let text = p.to_toml();
    assert!(text.contains("accepts_containers = [\"zip\"]"), "{text}");
    assert!(text.contains("kind = \"game-folder\""), "{text}");
    assert!(text.contains("[container_overrides]"), "{text}");
    assert_eq!(ProfileFile::parse_toml(&text).unwrap(), p);

    let profile = p.to_profile();
    assert_eq!(profile.disc_layout, p.disc_layout);
    let db = SystemDb::builtin();
    assert!(!profile.accepts_as_is(db.get("gba").unwrap(), ContainerKind::SevenZip));
    assert!(profile.accepts_as_is(db.get("nds").unwrap(), ContainerKind::SevenZip));

    // A built-in copied into a file keeps its values.
    let minui = builtin_profiles()
        .into_iter()
        .find(|b| b.id == "minui")
        .unwrap();
    let copy = ProfileFile::from_profile(&minui);
    assert_eq!(copy.disc_layout, minui.disc_layout);
    assert_eq!(copy.accepts_containers, vec![ContainerKind::Zip]);
    let back = ProfileFile::parse_toml(&copy.to_toml())
        .unwrap()
        .to_profile();
    assert_eq!(back.disc_layout, minui.disc_layout);
    assert_eq!(back.accepts_containers, minui.accepts_containers);
    assert_eq!(back.container_overrides, minui.container_overrides);
}

#[test]
fn custom_profile_without_fields_defaults_flat() {
    let text = r#"
id = "old-sd"
name = "Old SD"
roms_root = "Roms"

[slots.cover]
template = "{system_dir}/Imgs/{stem}.png"
"#;
    let p = ProfileFile::parse_toml(text).unwrap();
    assert_eq!(
        p.disc_layout,
        DiscLayout::Flat {
            m3u_for_multi: false
        }
    );
    assert_eq!(
        p.accepts_containers,
        vec![ContainerKind::Zip, ContainerKind::SevenZip]
    );
    assert_eq!(p.container_overrides, default_container_overrides());
    // Defaults are not written back, so old files stay byte-stable.
    let out = p.to_toml();
    assert!(!out.contains("disc_layout"), "{out}");
    assert!(!out.contains("accepts_containers"), "{out}");
    assert!(!out.contains("container_overrides"), "{out}");
    let profile = p.to_profile();
    assert_eq!(profile.disc_layout, DiscLayout::default());
    let db = SystemDb::builtin();
    assert!(profile.accepts_as_is(db.get("gba").unwrap(), ContainerKind::SevenZip));
    assert!(!profile.accepts_as_is(db.get("psp").unwrap(), ContainerKind::Zip));
}

#[test]
fn custom_profile_bad_container_fields_are_errors() {
    let text = r#"
id = "bad-sd"
name = "Bad SD"
accepts_containers = ["cab"]

[slots.cover]
template = "{system_dir}/Imgs/{stem}.png"
"#;
    assert!(ProfileFile::parse_toml(text).is_err());
    let text = r#"
id = "bad-sd"
name = "Bad SD"

[disc_layout]
kind = "nested"

[slots.cover]
template = "{system_dir}/Imgs/{stem}.png"
"#;
    assert!(ProfileFile::parse_toml(text).is_err());
    let mut p = sample();
    p.container_overrides = BTreeMap::from([
        ("not-a-system".to_string(), vec![ContainerKind::Zip]),
        ("gba".to_string(), vec![ContainerKind::Zip]),
    ]);
    assert!(has(
        &p,
        "container_overrides.not-a-system",
        FieldErrorCode::Invalid
    ));
    assert!(!errors(&p)
        .iter()
        .any(|(f, _)| f == "container_overrides.gba"));
}

#[test]
fn custom_profile_cleared_overrides_survive_toml() {
    // A user who removes the default `psp = []` override keeps that choice.
    let mut p = sample();
    p.container_overrides.clear();
    let text = p.to_toml();
    assert_eq!(ProfileFile::parse_toml(&text).unwrap(), p, "{text}");
    let mut p = sample();
    p.accepts_containers.clear();
    let text = p.to_toml();
    assert_eq!(ProfileFile::parse_toml(&text).unwrap(), p, "{text}");
}

#[test]
fn custom_profile_disc_layout_unknown_key_is_error() {
    let text = r#"
id = "typo-sd"
name = "Typo SD"

[disc_layout]
kind = "flat"
m3u = true

[slots.cover]
template = "{system_dir}/Imgs/{stem}.png"
"#;
    assert!(ProfileFile::parse_toml(text).is_err());
}

#[test]
fn container_override_on_disc_system_is_invalid() {
    // Disc systems always extract (spec D4): an override naming one would
    // be ignored silently, so the editor flags it instead.
    let mut p = sample();
    p.container_overrides
        .insert("psx".to_string(), vec![ContainerKind::Zip]);
    assert!(has(&p, "container_overrides.psx", FieldErrorCode::Invalid));
    // `[]` (always extract) says what already happens: no error.
    p.container_overrides.insert("psx".to_string(), Vec::new());
    assert!(!errors(&p)
        .iter()
        .any(|(f, _)| f == "container_overrides.psx"));
}

#[test]
fn clone_copies_named_folders() {
    use coverly_core::profiles::ProfileFolder;
    for b in builtin_profiles() {
        let mut file = ProfileFile::from_profile(&b);
        let named: BTreeMap<String, String> = b
            .folders
            .iter()
            .filter_map(|(id, f)| match f {
                ProfileFolder::Named(n) => Some((id.clone(), n.clone())),
                ProfileFolder::Absent => None,
            })
            .collect();
        assert_eq!(file.folder_names, named, "{}", b.id);
        file.id = format!("copy-of-{}", b.id);
        assert!(errors(&file).is_empty(), "{}: {:?}", b.id, errors(&file));
        let copy = file.to_profile();
        assert!(
            copy.folders
                .values()
                .all(|f| matches!(f, ProfileFolder::Named(_))),
            "{}",
            b.id
        );
        let copied: BTreeMap<String, String> = copy
            .folders
            .iter()
            .filter_map(|(id, f)| match f {
                ProfileFolder::Named(n) => Some((id.clone(), n.clone())),
                ProfileFolder::Absent => None,
            })
            .collect();
        assert_eq!(copied, named, "{}", b.id);
        assert_eq!(ProfileFile::parse_toml(&file.to_toml()).unwrap(), file);
    }
}

#[test]
fn custom_folder_names_resolve_under_the_custom_profile() {
    use coverly_core::profiles::naming::system_folder;
    let mut p = sample();
    p.folder_names.insert("snes".into(), "supernintendo".into());
    assert!(errors(&p).is_empty(), "{:?}", errors(&p));
    let profile = p.to_profile();
    let db = SystemDb::builtin();
    assert_eq!(
        db.resolve_folder_in(Some(&profile), "SuperNintendo")
            .map(|s| s.id.as_str()),
        Some("snes")
    );
    let choice = system_folder(&profile, db.get("snes").unwrap(), &db, &BTreeMap::new());
    assert_eq!(choice.folder, "supernintendo");
    // Another system's folder is still refused: the file's own table does
    // not vouch for itself.
    p.folder_names.insert("snes".into(), "GBA".into());
    assert!(has(&p, "folder_names.snes", FieldErrorCode::Invalid));
}
