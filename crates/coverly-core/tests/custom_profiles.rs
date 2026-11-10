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
        dlc_folder: BTreeMap::new(),
        game_folders: false,
        emulators: Default::default(),
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
    let mut thumbs = p.clone();
    thumbs.slots.get_mut("title").unwrap().template =
        "{root}/thumbs/{libretro}/{rel_dir}/{libretro_stem}.png".into();
    assert!(errors(&thumbs).is_empty(), "{:?}", errors(&thumbs));
    let amp = PreviewExample {
        file_name: "Mario & Luigi (USA).sfc".into(),
        ..ex.clone()
    };
    assert_eq!(
        preview_paths(&thumbs, &amp, &SystemDb::builtin())["title"],
        "thumbs/Nintendo - Super Nintendo Entertainment System/Hacks/Mario _ Luigi (USA).png"
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

#[test]
fn dlc_folders_load_round_trip_and_are_checked() {
    // Systems batch 3 D16: a file from before the field loads with none
    // and writes none.
    let old = sample().to_toml();
    assert!(!old.contains("dlc_folder"), "{old}");
    let back = ProfileFile::parse_toml(&old).unwrap();
    assert!(back.dlc_folder.is_empty());
    assert!(back.to_profile().dlc_folder.is_empty());

    let mut p = sample();
    p.dlc_folder.insert("ps4".into(), "DLC/{title_id}".into());
    assert!(errors(&p).is_empty(), "{:?}", errors(&p));
    let back = ProfileFile::parse_toml(&p.to_toml()).unwrap();
    assert_eq!(back.dlc_folder, p.dlc_folder);
    let profile = back.to_profile();
    assert_eq!(
        profile.dlc_folder.get("ps4").map(String::as_str),
        Some("DLC/{title_id}")
    );
    assert_eq!(ProfileFile::from_profile(&profile).dlc_folder, p.dlc_folder);

    for (system, value) in [
        ("ps4", "../x"),
        ("ps4", "/x"),
        ("ps4", "DLC/../x"),
        ("ps4", "DLC//{title_id}"),
        ("ps4", "DLC/"),
        ("ps4", ""),
        ("ps4", " DLC"),
        ("ps4", "C:\\x"),
        ("ps4", "DLC\\{title_id}"),
        ("ps4", "DLC/{stem}"),
        ("ps4", "DLC/{title_id"),
        ("ps4", "DLC/a\u{7}"),
        // Windows would drop the trailing dot or refuse the name, so the
        // folder would not be the one the frontend reads.
        ("ps4", "DLC./{title_id}"),
        ("ps4", "DLC/{title_id}."),
        ("ps4", "DLC?/{title_id}"),
        ("ps4", "DLC/<{title_id}>"),
        ("ps4", "D*C/{title_id}"),
        ("ps4", "DLC|x/{title_id}"),
        ("ps4", "\"DLC\"/{title_id}"),
        // A DOS device name (any case, any extension) is no folder on
        // Windows: creating it fails or opens the device.
        ("ps4", "CON/{title_id}"),
        ("ps4", "nul/{title_id}"),
        ("ps4", "DLC/Aux.{title_id}"),
        ("ps4", "com1/{title_id}"),
        ("ps4", "DLC/lpt9 .x/{title_id}"),
        ("nope", "DLC/{title_id}"),
    ] {
        let mut p = sample();
        p.dlc_folder.insert(system.into(), value.into());
        assert!(
            has(&p, &format!("dlc_folder.{system}"), FieldErrorCode::Invalid),
            "{system} {value:?}: {:?}",
            errors(&p)
        );
    }
}

#[test]
fn a_copy_of_batocera_keeps_its_ps4_suffix_and_dlc_folder() {
    let b = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "batocera")
        .unwrap();
    let mut f = ProfileFile::from_profile(&b);
    f.id = "my-batocera".into();
    assert_eq!(
        f.folder_game_suffix.get("ps4").map(String::as_str),
        Some(".ps4")
    );
    assert_eq!(
        f.dlc_folder.get("ps4").map(String::as_str),
        Some("DLC/{title_id}")
    );
    assert!(errors(&f).is_empty(), "{:?}", errors(&f));
    let back = ProfileFile::parse_toml(&f.to_toml()).unwrap();
    assert_eq!(back, f);
    assert_eq!(back.to_profile().dlc_folder, b.dlc_folder);
}

#[test]
fn ps5_archives_are_taken_as_is_by_default() {
    // Systems batch 3 D13: a PS5 app archive keeps its zip, 7z or rar;
    // PSP still always extracts.
    let o = default_container_overrides();
    assert_eq!(
        o.get("ps5"),
        Some(&vec![
            ContainerKind::Zip,
            ContainerKind::SevenZip,
            ContainerKind::Rar
        ])
    );
    assert_eq!(o.get("psp"), Some(&Vec::new()));
    let db = SystemDb::builtin();
    for b in builtin_profiles() {
        assert!(
            b.accepts_as_is(db.get("ps5").unwrap(), ContainerKind::Rar),
            "{}",
            b.id
        );
    }
    // A stored profile that never changed its overrides gets the entry.
    let p = ProfileFile::parse_toml(&sample().to_toml()).unwrap();
    assert!(p.to_profile().container_overrides.contains_key("ps5"));
    assert!(errors(&sample()).is_empty());
}

// Game folders (game folders spec D1, D2): the option, the built-in
// `coverly-library` profile and the migration guard for a custom file
// that already used its id.

#[test]
fn game_folders_round_trip_through_toml_and_are_written_only_when_on() {
    let off = sample();
    let text = off.to_toml();
    assert!(!text.contains("game_folders"), "{text}");
    let back = ProfileFile::parse_toml(&text).unwrap();
    assert!(!back.game_folders && !back.to_profile().game_folders);
    let mut on = sample();
    on.game_folders = true;
    let text = on.to_toml();
    assert!(text.contains("game_folders = true"), "{text}");
    let back = ProfileFile::parse_toml(&text).unwrap();
    assert!(back.game_folders);
    assert!(back.to_profile().game_folders);
    // «Duplicar perfil» copies it from a built-in.
    let library = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "coverly-library")
        .unwrap();
    assert!(ProfileFile::from_profile(&library).game_folders);
    let anbernic = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "anbernic-stock")
        .unwrap();
    assert!(!ProfileFile::from_profile(&anbernic).game_folders);
}

#[test]
fn the_settings_override_wins_over_the_profile_value() {
    use coverly_core::settings::OrganizeSettings;
    let find = |id: &str| builtin_profiles().into_iter().find(|p| p.id == id).unwrap();
    let library = find("coverly-library");
    let onion = find("onion");
    let mut o = OrganizeSettings::default();
    assert!(library.uses_game_folders(&o));
    assert!(!onion.uses_game_folders(&o));
    o.game_folders.insert("coverly-library".into(), false);
    o.game_folders.insert("onion".into(), true);
    assert!(!library.uses_game_folders(&o));
    assert!(onion.uses_game_folders(&o));
    // A custom profile: its own value, then the override.
    let mut mine = sample();
    mine.game_folders = true;
    let mine = mine.to_profile();
    assert!(mine.uses_game_folders(&OrganizeSettings::default()));
    o.game_folders.insert("my-sd".into(), false);
    assert!(!mine.uses_game_folders(&o));
}

fn legacy_library_file() -> ProfileFile {
    let mut p = sample();
    p.id = "coverly-library".into();
    p.name = "My library".into();
    p
}

#[test]
fn a_custom_profile_with_the_library_id_is_kept_under_a_new_id() {
    let dir = tempfile::tempdir().unwrap();
    let pdir = dir.path().join(PROFILES_DIR);
    fs::create_dir_all(&pdir).unwrap();
    fs::write(
        pdir.join("coverly-library.toml"),
        legacy_library_file().to_toml(),
    )
    .unwrap();
    let store = ProfileStore::new(pdir.clone());
    let loaded = store.load_all();
    assert_eq!(
        loaded
            .files
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        ["coverly-library-custom"]
    );
    assert_eq!(loaded.files[0].name, "My library");
    assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
    assert_eq!(
        loaded.renamed,
        [ProfileRename {
            from: "coverly-library".into(),
            to: "coverly-library-custom".into(),
        }]
    );
    // `load` is the same list, and loading never writes.
    assert_eq!(store.load().0, loaded.files);
    assert!(!pdir.join("coverly-library-custom.toml").exists());
    // The app makes it stick: the file moves to its new id.
    store.persist_renames(&loaded).unwrap();
    assert!(!pdir.join("coverly-library.toml").exists());
    let again = store.load_all();
    assert_eq!(again.files, loaded.files);
    assert!(again.renamed.is_empty() && again.warnings.is_empty());
}

#[test]
fn a_legacy_library_file_beside_its_new_id_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let pdir = dir.path().join(PROFILES_DIR);
    fs::create_dir_all(&pdir).unwrap();
    fs::write(
        pdir.join("coverly-library.toml"),
        legacy_library_file().to_toml(),
    )
    .unwrap();
    let mut kept = sample();
    kept.id = "coverly-library-custom".into();
    fs::write(pdir.join("coverly-library-custom.toml"), kept.to_toml()).unwrap();
    let loaded = ProfileStore::new(pdir).load_all();
    assert_eq!(loaded.files, [kept]);
    assert!(loaded.renamed.is_empty());
    assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
}

#[test]
fn settings_references_follow_a_renamed_library_profile_only() {
    use coverly_core::settings::{read_settings, read_settings_with_renames, SETTINGS_FILE};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SETTINGS_FILE);
    fs::write(
        &path,
        r#"{"version":3,
            "library":{"default_profile":"coverly-library"},
            "organize":{"default_layout":"coverly-library",
                        "folder_names":{"coverly-library":{"gba":"GBA"}},
                        "arcade_order":{"coverly-library":["mame","fbneo"]},
                        "game_folders":{"coverly-library":false,"onion":true}}}"#,
    )
    .unwrap();
    let mut known = builtin_ids();
    known.push("coverly-library-custom".into());
    let renames = [ProfileRename {
        from: "coverly-library".into(),
        to: "coverly-library-custom".into(),
    }];
    let s = read_settings_with_renames(&path, &known, &renames);
    let to = "coverly-library-custom".to_string();
    assert_eq!(s.organize.default_layout, to);
    assert_eq!(s.library.default_profile.as_ref(), Some(&to));
    assert_eq!(s.organize.folder_names.keys().collect::<Vec<_>>(), [&to]);
    assert_eq!(s.organize.arcade_order.keys().collect::<Vec<_>>(), [&to]);
    assert_eq!(
        s.organize.game_folders,
        BTreeMap::from([(to.clone(), false), ("onion".to_string(), true)])
    );
    // No rename in this load: `coverly-library` is the built-in.
    let s = read_settings(&path, &known);
    assert_eq!(s.organize.default_layout, "coverly-library");
    assert_eq!(s.organize.game_folders.get("coverly-library"), Some(&false));
}

#[test]
fn unknown_profiles_leave_the_game_folders_map() {
    use coverly_core::settings::{read_settings, SETTINGS_FILE};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SETTINGS_FILE);
    fs::write(
        &path,
        r#"{"version":3,"organize":{"game_folders":{"nope":true,"onion":true}}}"#,
    )
    .unwrap();
    let s = read_settings(&path, &builtin_ids());
    assert_eq!(
        s.organize.game_folders,
        BTreeMap::from([("onion".to_string(), true)])
    );
    // An empty map is not written (older builds read the file unchanged).
    let json = serde_json::to_string(&coverly_core::settings::Settings::default()).unwrap();
    assert!(!json.contains("game_folders"), "{json}");
}

#[test]
fn a_settings_patch_sets_and_removes_one_override() {
    use coverly_core::settings::Settings;
    let known = builtin_ids();
    let s = Settings::default()
        .with_patch(
            &serde_json::json!({"organize":{"game_folders":{"onion":true}}}),
            &known,
        )
        .unwrap();
    assert_eq!(s.organize.game_folders.get("onion"), Some(&true));
    let s = s
        .with_patch(
            &serde_json::json!({"organize":{"game_folders":{"coverly-library":false}}}),
            &known,
        )
        .unwrap();
    assert_eq!(s.organize.game_folders.len(), 2);
    let s = s
        .with_patch(
            &serde_json::json!({"organize":{"game_folders":{"onion":null}}}),
            &known,
        )
        .unwrap();
    assert_eq!(
        s.organize.game_folders,
        BTreeMap::from([("coverly-library".to_string(), false)])
    );
    let err = Settings::default()
        .with_patch(
            &serde_json::json!({"organize":{"game_folders":{"nope":true}}}),
            &known,
        )
        .unwrap_err();
    assert_eq!(err[0].field, "organize.game_folders.nope");
}

#[test]
fn the_library_profile_is_never_detected() {
    use coverly_core::profiles::detect;
    let dir = tempfile::tempdir().unwrap();
    let sys = dir.path().join("ROMS").join("PS4");
    let game = sys.join("Bloodborne");
    for d in [
        game.join("Updates"),
        game.join("DLC"),
        sys.join(".media"),
        dir.path().join("anbernic"),
        dir.path().join(".system"),
        dir.path().join("downloaded_media"),
    ] {
        fs::create_dir_all(d).unwrap();
    }
    fs::write(game.join("Bloodborne.pkg"), b"x").unwrap();
    let found = detect(dir.path());
    assert!(!found.is_empty());
    assert!(
        found.iter().all(|d| d.profile_id != "coverly-library"),
        "{found:?}"
    );
}

// --- Default emulators (mods spec D16) ---

#[test]
fn custom_emulators_round_trip_and_are_checked_per_system() {
    use coverly_core::translate::EmulatorId;
    // A file from an older build has no key and loads with none.
    let old = sample().to_toml();
    assert!(!old.contains("emulators"), "{old}");
    let back = ProfileFile::parse_toml(&old).unwrap();
    assert!(back.emulators.is_empty() && back.to_profile().emulators.is_empty());

    let mut p = sample();
    p.emulators.insert("ps4".into(), "shadps4".into());
    p.emulators.insert("snes".into(), "retroarch".into());
    assert!(errors(&p).is_empty(), "{:?}", errors(&p));
    let text = p.to_toml();
    assert!(text.contains("[emulators]"), "{text}");
    let back = ProfileFile::parse_toml(&text).unwrap();
    assert_eq!(back.emulators, p.emulators);
    let profile = back.to_profile();
    assert_eq!(profile.emulators.get("ps4"), Some(&EmulatorId::Shadps4));
    assert_eq!(ProfileFile::from_profile(&profile).emulators, p.emulators);

    // An emulator that does not run the system, an unknown system, an
    // unknown emulator: each names its `emulators.<system>` field.
    let toml = |line: &str| {
        let mut text = sample().to_toml();
        text.push_str(&format!("\n[emulators]\n{line}\n"));
        ProfileFile::parse_toml(&text).unwrap()
    };
    let wrong = toml(r#"ps4 = "rpcs3""#);
    assert!(has(&wrong, "emulators.ps4", FieldErrorCode::Invalid));
    assert!(wrong.to_profile().emulators.is_empty());
    assert!(has(
        &toml(r#"zzbox = "retroarch""#),
        "emulators.zzbox",
        FieldErrorCode::Invalid
    ));
    assert!(has(
        &toml(r#"snes = "snes9x""#),
        "emulators.snes",
        FieldErrorCode::Unknown
    ));
}

#[test]
fn a_copy_of_es_de_keeps_its_default_emulators() {
    let esde = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "esde")
        .unwrap();
    let mut file = ProfileFile::from_profile(&esde);
    file.id = "my-esde".into();
    assert_eq!(
        file.emulators.get("ps4").map(String::as_str),
        Some("shadps4")
    );
    assert!(errors(&file).is_empty(), "{:?}", errors(&file));
    assert_eq!(file.to_profile().emulators, esde.emulators);
}
