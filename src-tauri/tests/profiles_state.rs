mod common;

use std::collections::BTreeMap;

use coverly_core::profiles::custom::{PreviewExample, ProfileFile, SlotFile};
use coverly_core::profiles::ImageFormat;
use coverly_core::settings::FieldErrorCode;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::state::AppState;
use serde_json::json;
use tempfile::TempDir;

fn state(dir: &TempDir) -> AppState {
    AppState::new(
        dir.path().join("config"),
        FillConfig::new(dir.path().join("cache"), dir.path().join("data")),
    )
}

fn mine(id: &str, template: &str) -> ProfileFile {
    ProfileFile {
        id: id.into(),
        name: "Mine".into(),
        roms_root: "Games".into(),
        art_dirs: vec!["covers".into()],
        gamelist: false,
        slots: BTreeMap::from([(
            "cover".into(),
            SlotFile {
                template: template.into(),
                format: ImageFormat::Png,
                max_width: None,
                max_height: None,
            },
        )]),
    }
}

#[test]
fn builtins_are_listed_read_only_and_custom_ones_are_saved_renamed_and_deleted() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    let list = st.list_profiles();
    assert!(list.iter().all(|p| p.builtin));
    let out = st
        .save_profile(mine("mine", "{system_dir}/covers/{stem}.png"), None)
        .unwrap();
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    assert!(out
        .profiles
        .iter()
        .any(|p| p.profile.id == "mine" && !p.builtin));
    assert!(dir.path().join("config/profiles/mine.toml").exists());
    let set = st
        .update_settings(&json!({"library": {"default_profile": "mine"}, "art": {"max_size": {"mine": {"width": 300, "height": 400}}}}))
        .unwrap();
    assert!(set.errors.is_empty(), "{:?}", set.errors);
    let out = st
        .save_profile(
            mine("mine-2", "{system_dir}/covers/{stem}.png"),
            Some("mine".into()),
        )
        .unwrap();
    assert!(out.errors.is_empty());
    assert_eq!(
        st.settings().library.default_profile.as_deref(),
        Some("mine-2"),
        "the default follows a rename"
    );
    let sizes = st.settings().art.max_size;
    assert!(
        !sizes.contains_key("mine") && sizes.contains_key("mine-2"),
        "the max size follows a rename: {sizes:?}"
    );
    st.delete_profile("mine-2").unwrap();
    assert_eq!(
        st.settings().library.default_profile,
        None,
        "deleting the default falls back to detect"
    );
    assert!(
        st.settings().art.max_size.is_empty(),
        "and its max size goes with it"
    );
    assert!(
        matches!(st.delete_profile("onion"), Err(IpcError::BadRequest(_))),
        "built-ins are read-only"
    );
    assert!(matches!(
        st.delete_profile("ghost"),
        Err(IpcError::UnknownProfile(_))
    ));
}

#[test]
fn invalid_profiles_come_back_as_field_errors_and_are_not_saved() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    let out = st
        .save_profile(mine("mine", "{root}/../x/{stem}.png"), None)
        .unwrap();
    assert!(out
        .errors
        .iter()
        .any(|e| e.field == "slots.cover.template" && e.code == FieldErrorCode::Invalid));
    assert!(!dir.path().join("config/profiles/mine.toml").exists());
    st.save_profile(mine("taken", "{system_dir}/covers/{stem}.png"), None)
        .unwrap();
    let out = st
        .save_profile(mine("taken", "{system_dir}/covers/{stem}.png"), None)
        .unwrap();
    assert!(out
        .errors
        .iter()
        .any(|e| e.field == "id" && e.code == FieldErrorCode::Duplicate));
}

#[test]
fn preview_and_opening_with_a_custom_default_profile() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    let p = mine("mine", "{system_dir}/covers/{stem}.png");
    let preview = st.preview_profile(
        &p,
        &PreviewExample {
            system_folder: "GBA".into(),
            rel_dir: "".into(),
            file_name: "Golden Sun.gba".into(),
        },
    );
    assert_eq!(preview.paths["cover"], "Games/GBA/covers/Golden Sun.png");
    assert!(preview.errors.is_empty());
    st.save_profile(p, None).unwrap();
    st.update_settings(&json!({"library": {"default_profile": "mine"}}))
        .unwrap();
    let lib = TempDir::new().unwrap();
    std::fs::create_dir_all(lib.path().join("Games/GBA")).unwrap();
    std::fs::write(lib.path().join("Games/GBA/Golden Sun.gba"), b"x").unwrap();
    let view = st
        .open(lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    assert_eq!(view.profile.id, "mine");
    assert_eq!(view.entries.len(), 1);
}
