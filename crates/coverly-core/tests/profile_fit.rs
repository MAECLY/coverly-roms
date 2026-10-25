mod common;

use common::{profile, write};
use coverly_core::art::{resolve_art, ArtState};
use coverly_core::profiles::{fit_to_root, Media};
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

#[test]
fn picking_the_roms_folder_itself_scans_it_directly() {
    // The user picked `…/ROMS`, not the SD root.
    let dir = TempDir::new().unwrap();
    write(dir.path(), "SFC/Zelda (USA).sfc", "z");
    write(dir.path(), "SFC/Imgs/Zelda (USA).png", "png");
    write(dir.path(), "APPS/tool.sh", "t");
    let db = SystemDb::builtin();

    let fitted = fit_to_root(&profile("anbernic-stock"), dir.path(), &db);
    assert_eq!(fitted.roms_root, "");

    let scan = scan_library(dir.path(), &fitted, &db, &ScanOptions::default());
    assert!(scan.warnings.is_empty(), "{:?}", scan.warnings);
    let sfc = scan
        .systems
        .iter()
        .find(|s| s.folder_name == "SFC")
        .unwrap();
    assert_eq!(sfc.roms.len(), 1);
    let art = resolve_art(&scan, &fitted, &db, &[Media::Cover]);
    let sfc_art = art.iter().find(|s| s.scan.folder_name == "SFC").unwrap();
    assert!(matches!(
        sfc_art.roms[0].art[&Media::Cover],
        ArtState::Present { .. }
    ));
}

#[test]
fn an_sd_root_keeps_its_roms_root() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "ROMS/SFC/Zelda (USA).sfc", "z");
    let fitted = fit_to_root(&profile("anbernic-stock"), dir.path(), &SystemDb::builtin());
    assert_eq!(fitted, profile("anbernic-stock"));
}

#[test]
fn a_folder_without_system_folders_keeps_the_profile() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "Photos/a.jpg", "j");
    let fitted = fit_to_root(&profile("anbernic-stock"), dir.path(), &SystemDb::builtin());
    assert_eq!(fitted, profile("anbernic-stock"));
}

#[test]
fn profiles_with_art_outside_system_folders_are_never_fitted() {
    // ES-DE keeps art in `{root}/downloaded_media`: scanning the ROMs
    // folder alone would look for art in the wrong place.
    let dir = TempDir::new().unwrap();
    write(dir.path(), "snes/Zelda (USA).sfc", "z");
    let fitted = fit_to_root(&profile("esde"), dir.path(), &SystemDb::builtin());
    assert_eq!(fitted, profile("esde"));
}

mod disc_layouts_and_containers {
    //! Spec §8.2/§8.3, defaults confirmed by `docs/02-research/layouts.md`
    //! "Disc layouts and containers (R1, 2026-10)".

    use super::common::profile;
    use coverly_core::archive::{Compressor, ContainerKind, Format};
    use coverly_core::profiles::{builtin_profiles, DiscLayout};
    use coverly_core::systems::SystemDb;

    #[test]
    fn builtin_disc_layouts_match_research() {
        let expected = [
            (
                "anbernic-stock",
                DiscLayout::Flat {
                    m3u_for_multi: false,
                },
            ),
            (
                "garlicos",
                DiscLayout::Flat {
                    m3u_for_multi: false,
                },
            ),
            (
                "onion",
                DiscLayout::Flat {
                    m3u_for_multi: false,
                },
            ),
            (
                "muos",
                DiscLayout::Flat {
                    m3u_for_multi: false,
                },
            ),
            (
                "minui",
                DiscLayout::GameFolder {
                    m3u_for_multi: true,
                },
            ),
            (
                "esde",
                DiscLayout::GameFolder {
                    m3u_for_multi: true,
                },
            ),
            (
                "batocera",
                DiscLayout::Flat {
                    m3u_for_multi: true,
                },
            ),
            (
                "retroarch",
                DiscLayout::Flat {
                    m3u_for_multi: true,
                },
            ),
        ];
        assert_eq!(expected.len(), builtin_profiles().len());
        for (id, layout) in expected {
            assert_eq!(profile(id).disc_layout, layout, "{id}");
        }
        assert!(profile("esde").disc_layout.is_game_folder());
        assert!(!profile("onion").disc_layout.is_game_folder());
        assert!(profile("batocera").disc_layout.m3u_for_multi());
        assert!(!profile("muos").disc_layout.m3u_for_multi());
    }

    #[test]
    fn builtin_accepts_containers() {
        let db = SystemDb::builtin();
        let gba = db.get("gba").unwrap();
        for p in builtin_profiles() {
            let zip = p.accepts_as_is(gba, ContainerKind::Zip);
            let seven = p.accepts_as_is(gba, ContainerKind::SevenZip);
            assert!(zip, "{} zip", p.id);
            assert_eq!(seven, p.id != "minui", "{} 7z", p.id);
            for kind in [
                ContainerKind::Rar,
                ContainerKind::Tar,
                ContainerKind::TarGz,
                ContainerKind::Gz,
                ContainerKind::Bz2,
                ContainerKind::Xz,
                ContainerKind::Zst,
                ContainerKind::Lz4,
                ContainerKind::Lzma,
            ] {
                assert!(!p.accepts_as_is(gba, kind), "{} {kind:?}", p.id);
            }
        }
        // `psp` takes no archive as-is (R1-9, R1-20); `nds`/`n64` keep zip/7z.
        let anbernic = profile("anbernic-stock");
        assert!(!anbernic.accepts_as_is(db.get("psp").unwrap(), ContainerKind::Zip));
        assert!(anbernic.accepts_as_is(db.get("nds").unwrap(), ContainerKind::SevenZip));
        assert!(anbernic.accepts_as_is(db.get("n64").unwrap(), ContainerKind::Zip));
    }

    #[test]
    fn container_kind_of_every_format() {
        assert_eq!(ContainerKind::of(Format::Zip), ContainerKind::Zip);
        assert_eq!(ContainerKind::of(Format::SevenZip), ContainerKind::SevenZip);
        assert_eq!(
            ContainerKind::of(Format::Rar { v5: true }),
            ContainerKind::Rar
        );
        assert_eq!(
            ContainerKind::of(Format::Tar { outer: None }),
            ContainerKind::Tar
        );
        assert_eq!(
            ContainerKind::of(Format::Tar {
                outer: Some(Compressor::Zstd)
            }),
            ContainerKind::TarZst
        );
        assert_eq!(
            ContainerKind::of(Format::Single(Compressor::Gzip)),
            ContainerKind::Gz
        );
        for kind in ContainerKind::ALL {
            assert_eq!(ContainerKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(ContainerKind::from_name("cab"), None);
    }

    #[test]
    fn disc_systems_never_accept_archives_as_is() {
        let db = SystemDb::builtin();
        let discs: Vec<_> = db.all().iter().filter(|s| s.multi_file).collect();
        assert!(discs.iter().any(|s| s.id == "psx"));
        for p in builtin_profiles() {
            for system in &discs {
                for kind in ContainerKind::ALL {
                    assert!(
                        !p.accepts_as_is(system, kind),
                        "{} {} {kind:?}",
                        p.id,
                        system.id
                    );
                }
            }
        }
        // Even an override naming a disc system cannot make it accept one.
        let mut p = profile("onion");
        p.container_overrides
            .insert("psx".into(), vec![ContainerKind::Zip]);
        assert!(!p.accepts_as_is(db.get("psx").unwrap(), ContainerKind::Zip));
    }
}
