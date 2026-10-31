//! Built-in layout profiles (spec §5 and §16.8, paths verified in
//! `docs/02-research/layouts.md`).

use std::collections::{BTreeMap, HashMap};

use super::{
    default_accepts_containers, default_container_overrides, DiscLayout, FolderNaming, ImageFormat,
    Media, MediaSlot, Profile, ProfileFolder,
};
use crate::archive::ContainerKind;

/// Largest cover Onion, GarlicOS and Anbernic stock display well (§16.8).
const HANDHELD_COVER: Option<(u32, u32)> = Some((250, 360));

/// Arcade systems preferred first when a set is in several DATs.
const DEFAULT_ARCADE: [&str; 2] = ["fbneo", "mame"];
/// Systems the Anbernic family keeps in another system's folder.
const ANBERNIC_HOSTS: [(&str, &str); 2] = [("ngpc", "ngp"), ("wonderswancolor", "wonderswan")];
/// Arcade order of the Anbernic family (its card has one folder each).
const ANBERNIC_ARCADE: [&str; 6] = ["fbneo", "mame", "cps1", "cps2", "cps3", "neogeo"];
/// Folder names of the stock Anbernic card (reference `~/Documents/ROMS`).
const ANBERNIC_FOLDERS: [&str; 55] = [
    "A2600",
    "A5200",
    "A7800",
    "A800",
    "AMIGA",
    "APPS",
    "ATARIST",
    "ATOMISWAVE",
    "C64",
    "CPS1",
    "CPS2",
    "CPS3",
    "DOS",
    "DREAMCAST",
    "EASYRPG",
    "FBNEO",
    "FC",
    "FDS",
    "GB",
    "GBA",
    "GBC",
    "GG",
    "GW",
    "HBMAME",
    "JAVA",
    "LYNX",
    "MAME",
    "MD",
    "MDCD",
    "MSX",
    "N64",
    "NAOMI",
    "NDS",
    "NEOCD",
    "NEOGEO",
    "NGP",
    "ONS",
    "OPENBOR",
    "PCE",
    "PCECD",
    "PGM2",
    "PICO",
    "POKE",
    "PORTS",
    "PS",
    "PSP",
    "SATURN",
    "SCUMMVM",
    "SEGA32X",
    "SFC",
    "SMS",
    "VARCADE",
    "VB",
    "VIC20",
    "WS",
];

/// Columns of [`SYSTEM_FOLDERS`].
const ESDE: usize = 0;
const BATOCERA: usize = 1;
const MUOS: usize = 2;
const ONION: usize = 3;
const MINUI: usize = 4;

/// Spec §6.1: each Tanda 1 system's folder per frontend (`None` = the
/// frontend has none: [`ProfileFolder::Absent`]), columns `[esde,
/// batocera, muos, onion, minui]`. GarlicOS reads the Onion column (D9);
/// Anbernic stock is Absent for every row (D11); RetroArch keeps `Id` and
/// has no entries. Pre-batch systems are not listed (no behaviour change).
const SYSTEM_FOLDERS: &[(&str, [Option<&str>; 5])] = &[
    // Disc family (Task 4).
    ("ps2", [Some("ps2"), Some("ps2"), None, None, None]),
    ("3do", [Some("3do"), Some("3do"), Some("3do"), None, None]),
    (
        "pcfx",
        [Some("pcfx"), Some("pcfx"), Some("pcfx"), None, None],
    ),
    (
        "cdi",
        [Some("cdimono1"), Some("cdi"), Some("cdi"), None, None],
    ),
    (
        "amigacd32",
        [Some("amigacd32"), Some("amigacd32"), None, None, None],
    ),
    ("cdtv", [Some("cdtv"), Some("amigacdtv"), None, None, None]),
    (
        "jaguarcd",
        [Some("atarijaguarcd"), Some("jaguarcd"), None, None, None],
    ),
    (
        "fmtowns",
        [Some("fmtowns"), Some("fmtowns"), None, None, None],
    ),
];

/// The folder table of one frontend column; `None` (Anbernic stock) marks
/// every listed system Absent.
fn folders(column: Option<usize>) -> BTreeMap<String, ProfileFolder> {
    SYSTEM_FOLDERS
        .iter()
        .map(|(id, cells)| {
            let folder = match column.and_then(|c| cells[c]) {
                Some(name) => ProfileFolder::Named(name.to_string()),
                None => ProfileFolder::Absent,
            };
            (id.to_string(), folder)
        })
        .collect()
}

/// Flat disc folders without `.m3u` (Onion and spruceOS hide `.bin`, R1-1).
const FLAT: DiscLayout = DiscLayout::Flat {
    m3u_for_multi: false,
};
/// Flat disc folders plus an `.m3u` per multi-disc game (R1-18, R1-19).
const FLAT_M3U: DiscLayout = DiscLayout::Flat {
    m3u_for_multi: true,
};
/// One folder per game with an `.m3u` for multi-disc (R1-13, R1-14).
const GAME_FOLDER: DiscLayout = DiscLayout::GameFolder {
    m3u_for_multi: true,
};

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The built-in profiles, in detection tie-break order.
///
/// GarlicOS and Anbernic stock layouts are community-sourced (see
/// `docs/02-research/layouts.md`); their templates are editable defaults.
pub fn builtin_profiles() -> Vec<Profile> {
    vec![
        Profile {
            known_folders: strings(&ANBERNIC_FOLDERS),
            ..imgs_profile("anbernic-stock", "Anbernic stock", "ROMS", None)
        },
        imgs_profile("garlicos", "GarlicOS", "Roms", Some(ONION)),
        imgs_profile("onion", "Onion OS", "Roms", Some(ONION)),
        Profile {
            folders: folders(Some(MUOS)),
            ..profile(
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
            )
        },
        Profile {
            folders: folders(Some(MINUI)),
            system_folder: FolderNaming::FirstAlias,
            disc_layout: GAME_FOLDER,
            // MinUI's launcher unpacks `.zip` only (R1-12).
            accepts_containers: vec![ContainerKind::Zip],
            ..profile(
                "minui",
                "MinUI / NextUI",
                "Roms",
                &[".media", ".res"],
                true,
                &[(Media::Cover, "{system_dir}/{rel_dir}/.media/{stem}.png")],
                None,
            )
        },
        Profile {
            folders: folders(Some(ESDE)),
            disc_layout: GAME_FOLDER,
            ..profile(
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
            )
        },
        Profile {
            folders: folders(Some(BATOCERA)),
            gamelist: true,
            disc_layout: FLAT_M3U,
            ..profile(
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
                    (Media::Title, "{system_dir}/images/{stem}-titleshot.png"),
                ],
                None,
            )
        },
        Profile {
            fixed_format: true,
            disc_layout: FLAT_M3U,
            ..profile(
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
            )
        },
    ]
}

/// Single-slot handheld layout: `<SYS>/Imgs/<stem>.png`, mirroring
/// subfolders, capital `I` (Onion's folder is case sensitive). Folders are
/// named like the Anbernic card (`GBA`, `SFC`…), colour handhelds share
/// their host's folder.
/// `column` is the frontend's [`SYSTEM_FOLDERS`] column.
fn imgs_profile(id: &str, name: &str, roms_root: &str, column: Option<usize>) -> Profile {
    Profile {
        folders: folders(column),
        system_folder: FolderNaming::FirstAlias,
        host_folders: ANBERNIC_HOSTS
            .iter()
            .map(|&(s, h)| (s.to_string(), h.to_string()))
            .collect(),
        arcade_order: strings(&ANBERNIC_ARCADE),
        ..profile(
            id,
            name,
            roms_root,
            &["Imgs"],
            true,
            &[(Media::Cover, "{system_dir}/{rel_dir}/Imgs/{stem}.png")],
            HANDHELD_COVER,
        )
    }
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
        fixed_format: false,
        gamelist: false,
        system_folder: FolderNaming::Id,
        host_folders: BTreeMap::new(),
        arcade_order: strings(&DEFAULT_ARCADE),
        known_folders: Vec::new(),
        disc_layout: FLAT,
        accepts_containers: default_accepts_containers(),
        container_overrides: default_container_overrides(),
        folders: BTreeMap::new(),
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
            .ends_with("-titleshot.png"));
    }

    #[test]
    fn only_retroarch_has_a_fixed_format() {
        for p in builtin_profiles() {
            assert_eq!(p.fixed_format, p.id == "retroarch", "{}", p.id);
        }
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
