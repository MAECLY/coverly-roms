//! Built-in layout profiles (spec §5 and §16.8, paths verified in
//! `docs/02-research/layouts.md`).

use std::collections::HashMap;

use super::{ImageFormat, Media, MediaSlot, Profile};

/// Largest cover Onion, GarlicOS and Anbernic stock display well (§16.8).
const HANDHELD_COVER: Option<(u32, u32)> = Some((250, 360));

/// The built-in profiles, in detection tie-break order.
///
/// GarlicOS and Anbernic stock layouts are community-sourced (see
/// `docs/02-research/layouts.md`); their templates are editable defaults.
pub fn builtin_profiles() -> Vec<Profile> {
    vec![
        imgs_profile("anbernic-stock", "Anbernic stock", "ROMS"),
        imgs_profile("garlicos", "GarlicOS", "Roms"),
        imgs_profile("onion", "Onion OS", "Roms"),
        profile(
            "muos",
            "muOS",
            "ROMS",
            &[],
            false,
            &[
                (
                    Media::Cover,
                    "{root}/MUOS/info/catalogue/{system_id}/box/{stem}.png",
                ),
                (
                    Media::Screenshot,
                    "{root}/MUOS/info/catalogue/{system_id}/preview/{stem}.png",
                ),
            ],
            None,
        ),
        profile(
            "minui",
            "MinUI / NextUI",
            "Roms",
            &[".media", ".res"],
            true,
            &[(Media::Cover, "{system_dir}/{rel_dir}/.media/{stem}.png")],
            None,
        ),
        profile(
            "esde",
            "ES-DE",
            "ROMs",
            &[],
            true,
            &[
                (
                    Media::Cover,
                    "{root}/downloaded_media/{system_id}/covers/{rel_dir}/{stem}.png",
                ),
                (
                    Media::Screenshot,
                    "{root}/downloaded_media/{system_id}/screenshots/{rel_dir}/{stem}.png",
                ),
                (
                    Media::Title,
                    "{root}/downloaded_media/{system_id}/titlescreens/{rel_dir}/{stem}.png",
                ),
            ],
            None,
        ),
        profile(
            "batocera",
            "Batocera / Knulli / EmulationStation",
            "roms",
            &["images", "videos", "manuals", "magazines"],
            false,
            &[
                (Media::Cover, "{system_dir}/images/{stem}-image.png"),
                (
                    Media::Screenshot,
                    "{system_dir}/images/{stem}-screenshot.png",
                ),
                (Media::Title, "{system_dir}/images/{stem}-titlescreen.png"),
            ],
            None,
        ),
        profile(
            "retroarch",
            "RetroArch",
            "",
            &["thumbnails"],
            false,
            &[
                (
                    Media::Cover,
                    "{root}/thumbnails/{libretro}/Named_Boxarts/{stem}.png",
                ),
                (
                    Media::Screenshot,
                    "{root}/thumbnails/{libretro}/Named_Snaps/{stem}.png",
                ),
                (
                    Media::Title,
                    "{root}/thumbnails/{libretro}/Named_Titles/{stem}.png",
                ),
            ],
            None,
        ),
    ]
}

/// Single-slot handheld layout: `<SYS>/Imgs/<stem>.png`, mirroring
/// subfolders, capital `I` (Onion's folder is case sensitive).
fn imgs_profile(id: &str, name: &str, roms_root: &str) -> Profile {
    profile(
        id,
        name,
        roms_root,
        &["Imgs"],
        true,
        &[(Media::Cover, "{system_dir}/{rel_dir}/Imgs/{stem}.png")],
        HANDHELD_COVER,
    )
}

/// Builds a profile whose slots are all PNG with the same size limit.
fn profile(
    id: &str,
    name: &str,
    roms_root: &str,
    art_dir_names: &[&str],
    mirror_subdirs: bool,
    slots: &[(Media, &str)],
    max_size: Option<(u32, u32)>,
) -> Profile {
    Profile {
        id: id.to_string(),
        name: name.to_string(),
        roms_root: roms_root.to_string(),
        art_dir_names: art_dir_names.iter().map(|s| s.to_string()).collect(),
        mirror_subdirs,
        slots: slots
            .iter()
            .map(|&(media, template)| {
                let slot = MediaSlot {
                    template: template.to_string(),
                    format: ImageFormat::Png,
                    max_size,
                };
                (media, slot)
            })
            .collect::<HashMap<_, _>>(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::super::{render, ImageFormat, Media, TemplateVars};
    use super::*;

    fn profile(id: &str) -> Profile {
        builtin_profiles()
            .into_iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("missing built-in profile {id}"))
    }

    fn cover_path(id: &str, rel_dir: &str) -> PathBuf {
        let p = profile(id);
        let slot = p.slot(Media::Cover).expect("cover slot");
        let system_dir = Path::new("/sd").join(&p.roms_root).join("SFC");
        render(
            &slot.template,
            &TemplateVars {
                root: Path::new("/sd"),
                system_dir: &system_dir,
                rel_dir: Path::new(rel_dir),
                stem: "Zelda (USA)",
                system_id: "snes",
                libretro: Some("Nintendo - Super Nintendo Entertainment System"),
            },
        )
    }

    #[test]
    fn has_the_expected_ids_in_order() {
        let ids: Vec<String> = builtin_profiles().into_iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            [
                "anbernic-stock",
                "garlicos",
                "onion",
                "muos",
                "minui",
                "esde",
                "batocera",
                "retroarch"
            ]
        );
    }

    #[test]
    fn single_slot_handhelds_store_250x360_png_covers_in_imgs() {
        for id in ["anbernic-stock", "garlicos", "onion"] {
            let p = profile(id);
            let cover = p.slot(Media::Cover).unwrap();
            assert_eq!(cover.format, ImageFormat::Png, "{id}");
            assert_eq!(cover.max_size, Some((250, 360)), "{id}");
            assert!(p.slot(Media::Screenshot).is_none(), "{id}");
            assert!(p.slot(Media::Title).is_none(), "{id}");
            assert!(p.mirror_subdirs, "{id}");
            assert_eq!(p.art_dir_names, ["Imgs"], "{id}");
        }
        assert_eq!(profile("anbernic-stock").roms_root, "ROMS");
        assert_eq!(
            cover_path("anbernic-stock", ""),
            PathBuf::from("/sd/ROMS/SFC/Imgs/Zelda (USA).png")
        );
        assert_eq!(
            cover_path("onion", "Hacks"),
            PathBuf::from("/sd/Roms/SFC/Hacks/Imgs/Zelda (USA).png")
        );
    }

    #[test]
    fn esde_mirrors_subfolders_under_downloaded_media() {
        let p = profile("esde");
        assert!(p.mirror_subdirs);
        assert_eq!(p.slots.len(), 3);
        assert_eq!(
            cover_path("esde", "Hacks"),
            PathBuf::from("/sd/downloaded_media/snes/covers/Hacks/Zelda (USA).png")
        );
        assert!(p
            .slot(Media::Screenshot)
            .unwrap()
            .template
            .contains("/screenshots/"));
        assert!(p
            .slot(Media::Title)
            .unwrap()
            .template
            .contains("/titlescreens/"));
    }

    #[test]
    fn batocera_is_flat_with_suffixes() {
        let p = profile("batocera");
        assert!(!p.mirror_subdirs);
        assert_eq!(
            cover_path("batocera", "Hacks"),
            PathBuf::from("/sd/roms/SFC/images/Zelda (USA)-image.png")
        );
        assert!(p
            .slot(Media::Screenshot)
            .unwrap()
            .template
            .ends_with("-screenshot.png"));
        assert!(p
            .slot(Media::Title)
            .unwrap()
            .template
            .ends_with("-titlescreen.png"));
    }

    #[test]
    fn retroarch_uses_libretro_playlist_folders() {
        let p = profile("retroarch");
        assert!(!p.mirror_subdirs);
        assert_eq!(
            cover_path("retroarch", "Hacks"),
            PathBuf::from(
                "/sd/thumbnails/Nintendo - Super Nintendo Entertainment System/Named_Boxarts/Zelda (USA).png"
            )
        );
        assert!(p
            .slot(Media::Screenshot)
            .unwrap()
            .template
            .contains("/Named_Snaps/"));
        assert!(p
            .slot(Media::Title)
            .unwrap()
            .template
            .contains("/Named_Titles/"));
    }

    #[test]
    fn muos_uses_flat_catalogue_box_and_preview() {
        let p = profile("muos");
        assert!(!p.mirror_subdirs);
        assert!(p.slot(Media::Title).is_none());
        assert_eq!(
            cover_path("muos", "Hacks"),
            PathBuf::from("/sd/MUOS/info/catalogue/snes/box/Zelda (USA).png")
        );
        assert!(p
            .slot(Media::Screenshot)
            .unwrap()
            .template
            .contains("/preview/"));
    }

    #[test]
    fn minui_uses_media_folder_next_to_the_rom() {
        let p = profile("minui");
        assert!(p.mirror_subdirs);
        assert!(p.art_dir_names.contains(&".media".to_string()));
        assert_eq!(
            cover_path("minui", "Hacks"),
            PathBuf::from("/sd/Roms/SFC/Hacks/.media/Zelda (USA).png")
        );
    }

    #[test]
    fn mirror_flag_matches_templates() {
        for p in builtin_profiles() {
            assert!(!p.slots.is_empty(), "{}", p.id);
            assert!(p.slot(Media::Cover).is_some(), "{}", p.id);
            for (media, slot) in &p.slots {
                assert_eq!(
                    slot.template.contains("{rel_dir}"),
                    p.mirror_subdirs,
                    "{} {media:?}",
                    p.id
                );
            }
        }
    }
}
