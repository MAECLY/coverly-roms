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
/// D23: sub-systems the stock card has only the parent's folder for
/// (`PCE`, `SFC`, `MSX`, `A800` are in [`ANBERNIC_FOLDERS`]), so their
/// games keep the destination they had before the batch.
const ANBERNIC_STOCK_HOSTS: [(&str, &str); 4] = [
    ("supergrafx", "pcengine"),
    ("satellaview", "snes"),
    ("msx2", "msx"),
    ("atarixe", "a800"),
];
/// D23 on GarlicOS and Onion: Onion ships an `MSX` package and no `MSX2`
/// (OnionUI/Onion 07505ea5, `Emu/MSX/config.json`, blueMSX, extlist
/// `rom|mx1|mx2|dsk|cas|zip|7z|m3u`: every MSX2 extension).
const ONION_HOSTS: [(&str, &str); 1] = [("msx2", "msx")];
/// D23 on MinUI / NextUI: only a parent whose NextUI pak runs the
/// sub-system (`SFC.pak` is Snes9x, which loads BS-X carts; `PCE.pak` is
/// Beetle PCE Fast, which has no SuperGrafx, so SuperGrafx stays Absent;
/// `MSX.pak` is blueMSX, which runs MSX2). NextUI has no Atari 8-bit pak,
/// so Atari XE stays Absent.
const MINUI_HOSTS: [(&str, &str); 2] = [("satellaview", "snes"), ("msx2", "msx")];
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
    // Early consoles (Task 5).
    (
        "colecovision",
        [
            Some("colecovision"),
            Some("colecovision"),
            Some("colecovision"),
            Some("COLECO"),
            Some("COLECO"),
        ],
    ),
    (
        "intellivision",
        [
            Some("intellivision"),
            Some("intellivision"),
            Some("intellivision"),
            Some("INTELLIVISION"),
            None,
        ],
    ),
    (
        "vectrex",
        [
            Some("vectrex"),
            Some("vectrex"),
            Some("vectrex"),
            Some("VECTREX"),
            None,
        ],
    ),
    (
        "odyssey2",
        [
            Some("odyssey2"),
            Some("odyssey2"),
            Some("odyssey2"),
            Some("ODYSSEY"),
            None,
        ],
    ),
    (
        "channelf",
        [
            Some("channelf"),
            Some("channelf"),
            Some("channelf"),
            Some("FAIRCHILD"),
            None,
        ],
    ),
    (
        "arcadia",
        [Some("arcadia"), Some("arcadia"), None, None, None],
    ),
    ("scv", [Some("scv"), Some("scv"), Some("scv"), None, None]),
    (
        "jaguar",
        [
            Some("atarijaguar"),
            Some("jaguar"),
            Some("jaguar"),
            None,
            None,
        ],
    ),
    // Sega, NEC and Nintendo add-ons (Task 6).
    (
        "sg1000",
        [
            Some("sg-1000"),
            Some("sg1000"),
            Some("sg1000"),
            Some("SEGASGONE"),
            Some("SG1000"),
        ],
    ),
    ("pico", [None, Some("pico"), Some("segapico"), None, None]),
    (
        "supergrafx",
        [
            Some("supergrafx"),
            Some("supergrafx"),
            Some("supergrafx"),
            Some("SGFX"),
            None,
        ],
    ),
    (
        "n64dd",
        [Some("n64dd"), Some("n64dd"), Some("n64dd"), None, None],
    ),
    (
        "satellaview",
        [
            Some("satellaview"),
            Some("satellaview"),
            Some("satellaview"),
            Some("SATELLAVIEW"),
            None,
        ],
    ),
    (
        "sufami",
        [
            Some("sufami"),
            Some("sufami"),
            Some("sufami"),
            Some("SUFAMI"),
            None,
        ],
    ),
    // Handhelds (Task 7).
    (
        "supervision",
        [
            Some("supervision"),
            Some("supervision"),
            Some("supervision"),
            Some("SUPERVISION"),
            None,
        ],
    ),
    (
        "gamecom",
        [Some("gamecom"), Some("gamecom"), None, None, None],
    ),
    (
        "megaduck",
        [
            Some("megaduck"),
            Some("megaduck"),
            Some("megaduck"),
            Some("MEGADUCK"),
            None,
        ],
    ),
    // Computers I (Task 8).
    (
        "zxspectrum",
        [
            Some("zxspectrum"),
            Some("zxspectrum"),
            Some("spectrum"),
            Some("ZXS"),
            None,
        ],
    ),
    (
        "zx81",
        [Some("zx81"), Some("zx81"), Some("zx81"), None, None],
    ),
    (
        "amstradcpc",
        [
            Some("amstradcpc"),
            Some("amstradcpc"),
            Some("amstradcpc"),
            Some("CPC"),
            Some("CPC"),
        ],
    ),
    (
        "bbcmicro",
        [Some("bbcmicro"), Some("bbcmicro"), None, None, None],
    ),
    (
        "thomson",
        [Some("moto"), Some("thomson"), Some("thomson"), None, None],
    ),
    (
        "dragon",
        [
            Some("dragon32"),
            Some("dragon64"),
            Some("dragon"),
            None,
            None,
        ],
    ),
    (
        "coco",
        [Some("coco"), Some("coco"), Some("coco"), None, None],
    ),
    ("ti99", [Some("ti99"), Some("ti99"), None, None, None]),
    // Computers II (Task 9).
    (
        "x68000",
        [Some("x68000"), Some("x68000"), Some("x68000"), None, None],
    ),
    ("x1", [Some("x1"), Some("x1"), Some("x1"), None, None]),
    (
        "pc98",
        [Some("pc98"), Some("pc98"), Some("pc98"), None, None],
    ),
    (
        "pc88",
        [Some("pc88"), Some("pc88"), Some("pc8000"), None, None],
    ),
    // Computers III (Task 10). MinUI is NextUI's `PLUS4`, `PET`, `C128`
    // paks (LoveRetro/NextUI 5291344b, skeleton/EXTRAS/Emus).
    (
        "plus4",
        [
            Some("plus4"),
            Some("cplus4"),
            Some("plus4"),
            None,
            Some("PLUS4"),
        ],
    ),
    ("pet", [None, Some("pet"), Some("pet"), None, Some("PET")]),
    (
        "c128",
        [None, Some("c128"), Some("c128"), None, Some("C128")],
    ),
    ("atarixe", [Some("atarixe"), Some("xegs"), None, None, None]),
    (
        "msx2",
        [Some("msx2"), Some("msx2"), Some("msx2"), None, None],
    ),
    (
        "msxturbor",
        [Some("msxturbor"), Some("msxturbor"), None, None, None],
    ),
    ("apple2", [Some("apple2"), Some("apple2"), None, None, None]),
    // Fantasy consoles (Task 11). Onion's `TIC` package is TIC-80; its
    // `extlist` names Thomson extensions (spec §6.1 note), kept as found.
    (
        "tic80",
        [
            Some("tic80"),
            Some("tic80"),
            Some("tic80"),
            Some("TIC"),
            None,
        ],
    ),
    (
        "wasm4",
        [Some("wasm4"), Some("wasm4"), Some("wasm4"), None, None],
    ),
    (
        "arduboy",
        [
            Some("arduboy"),
            Some("arduboy"),
            Some("arduboy"),
            None,
            None,
        ],
    ),
    (
        "uzebox",
        [Some("uzebox"), Some("uzebox"), Some("uzebox"), None, None],
    ),
    (
        "lowresnx",
        [
            Some("lowresnx"),
            Some("lowresnx"),
            Some("lowresnx"),
            None,
            None,
        ],
    ),
    (
        "vircon32",
        [
            Some("vircon32"),
            Some("vircon32"),
            Some("vircon32"),
            None,
            None,
        ],
    ),
    // Ports and engines (Task 12). Onion and GarlicOS ship none of them;
    // NextUI has only the PrBoom pak.
    (
        "doom",
        [
            Some("doom"),
            Some("prboom"),
            Some("doom"),
            None,
            Some("PRBOOM"),
        ],
    ),
    (
        "quake",
        [Some("quake"), Some("quake"), Some("quake"), None, None],
    ),
    (
        "cavestory",
        [None, Some("cavestory"), Some("cavestory"), None, None],
    ),
    (
        "wolf3d",
        [None, Some("ecwolf"), Some("wolfenstein3d"), None, None],
    ),
    ("mrboom", [None, Some("mrboom"), None, None, None]),
    ("dinothawr", [None, None, None, None, None]),
    ("lutro", [Some("lutro"), Some("lutro"), None, None, None]),
    (
        "solarus",
        [Some("solarus"), Some("solarus"), None, None, None],
    ),
    ("flashback", [None, Some("reminiscence"), None, None, None]),
    ("xrick", [None, Some("xrick"), None, None, None]),
    // Extra arcade (Task 13): ES-DE and Batocera only (D16).
    ("model2", [Some("model2"), Some("model2"), None, None, None]),
    ("model3", [Some("model3"), Some("model3"), None, None, None]),
    ("naomi2", [Some("naomi2"), Some("naomi2"), None, None, None]),
    (
        "triforce",
        [Some("triforce"), Some("triforce"), None, None, None],
    ),
    // Systems batch 2 (spec §6).
    ("gamecube", [Some("gc"), Some("gamecube"), None, None, None]),
    ("wii", [Some("wii"), Some("wii"), None, None, None]),
    ("3ds", [Some("n3ds"), Some("3ds"), Some("3ds"), None, None]),
    ("wiiu", [Some("wiiu"), Some("wiiu"), None, None, None]),
    // Batocera has no `switch` key at `a3d49969`; Knulli and RetroBat do.
    // Named for the shared Batocera/Knulli profile (spec D19).
    ("switch", [Some("switch"), Some("switch"), None, None, None]),
    ("psvita", [Some("psvita"), Some("psvita"), None, None, None]),
    ("ps3", [Some("ps3"), Some("ps3"), None, None, None]),
    ("xbox", [Some("xbox"), Some("xbox"), None, None, None]),
    (
        "xbox360",
        [Some("xbox360"), Some("xbox360"), None, None, None],
    ),
    // Systems batch 3 (spec §6): ES-DE `es_systems.xml` at ef267ae6,
    // Batocera `es_systems.yml` at cc793b75 (Knulli shares the profile).
    // No frontend has a PS5 or Xbox Series system; Batocera has no Xbox One.
    ("ps4", [Some("ps4"), Some("ps4"), None, None, None]),
    ("ps5", [None, None, None, None, None]),
    ("xboxone", [Some("xboxone"), None, None, None, None]),
    ("xboxseries", [None, None, None, None, None]),
];

/// Folder-game suffixes of ES-DE and Batocera (spec D13): both show a
/// PS3 game folder only when its name ends in `.ps3` (Batocera
/// `es_systems.yml`: «PS3 roms are directories must be renamed to end
/// with .ps3»; ES-DE `es_systems.xml` lists `.ps3` and `.ps3dir`).
fn ps3_folder_suffix() -> BTreeMap<String, String> {
    BTreeMap::from([("ps3".to_string(), ".ps3".to_string())])
}

/// Batocera `es_systems.yml` (`ps4`): «rename game folder to end with .ps4»
/// (systems batch 3 D16). ES-DE's PS4 entries are `.ps4` serial files,
/// never folders, so it keeps the PS3 suffix only.
fn batocera_folder_suffix() -> BTreeMap<String, String> {
    let mut m = ps3_folder_suffix();
    m.insert("ps4".into(), ".ps4".into());
    m
}

/// Batocera's shadPS4 launcher sets `addon_install_dir` to `roms/ps4/DLC`
/// (`batocera_launch/emulators/shadps4.py` at cc793b75) and the emulator
/// reads `<dir>/<TITLE_ID>/<content>/` (systems batch 3 D16).
fn batocera_dlc_folder() -> BTreeMap<String, String> {
    BTreeMap::from([("ps4".to_string(), "DLC/{title_id}".to_string())])
}

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

fn hosts(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|&(s, h)| (s.to_string(), h.to_string()))
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The built-in profiles, in detection tie-break order.
///
/// GarlicOS and Anbernic stock layouts are community-sourced (see
/// `docs/02-research/layouts.md`); their templates are editable defaults.
pub fn builtin_profiles() -> Vec<Profile> {
    vec![
        {
            let mut p = Profile {
                known_folders: strings(&ANBERNIC_FOLDERS),
                ..imgs_profile("anbernic-stock", "Anbernic stock", "ROMS", None)
            };
            p.host_folders.extend(hosts(&ANBERNIC_STOCK_HOSTS));
            p
        },
        {
            let mut p = imgs_profile("garlicos", "GarlicOS", "Roms", Some(ONION));
            p.host_folders.extend(hosts(&ONION_HOSTS));
            p
        },
        {
            let mut p = imgs_profile("onion", "Onion OS", "Roms", Some(ONION));
            p.host_folders.extend(hosts(&ONION_HOSTS));
            p
        },
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
            host_folders: hosts(&MINUI_HOSTS),
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
            folder_game_suffix: ps3_folder_suffix(),
            disc_layout: GAME_FOLDER,
            // ES-DE `Utils::FileSystem::getStem` returns a directory's
            // whole name (es-core/src/utils/FileSystemUtil.cpp at
            // ef267ae6), and `getMediafilePath` and the scraper name
            // media after it (systems batch 2 D26).
            folder_stem_keeps_suffix: true,
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
            folder_game_suffix: batocera_folder_suffix(),
            dlc_folder: batocera_dlc_folder(),
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
        host_folders: hosts(&ANBERNIC_HOSTS),
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
        folder_game_suffix: BTreeMap::new(),
        folder_stem_keeps_suffix: false,
        dlc_folder: BTreeMap::new(),
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
