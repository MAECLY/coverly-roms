//! Duplicates: files that travel with a set-aside copy, and files that stay
//! because the keeper shares them (spec §9.1). Every file here is synthetic
//! and lives in a temporary folder.

use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::dedupe::companions::{
    art_slots, companions_for, ArtLeaves, CompanionCtx, GamelistEntry,
};
use coverly_core::dedupe::group::RawGroup;
use coverly_core::dedupe::units::Unit;
use coverly_core::dedupe::{GroupAction, Relation, SharedStay, Travel, TravelKind, UnitKind};
use coverly_core::organize::target::default_companion_exts;
use coverly_core::profiles::{builtin_profiles, Media, Profile};
use coverly_core::systems::SystemDb;

// --- Builders ---

fn write(path: &Path, bytes: &[u8]) -> PathBuf {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    path.to_path_buf()
}

fn touch(path: &Path) -> PathBuf {
    write(path, b"x")
}

fn profile(id: &str) -> Profile {
    builtin_profiles().into_iter().find(|p| p.id == id).unwrap()
}

/// A unit made of `files` (the first is the primary) in `system_dir`.
fn unit(id: usize, system: &str, system_dir: &Path, files: &[&str], kind: UnitKind) -> Unit {
    let files: Vec<PathBuf> = files.iter().map(|f| touch(&system_dir.join(f))).collect();
    let primary = files[0].clone();
    Unit {
        id,
        system: system.to_string(),
        primary: primary.clone(),
        files,
        kind,
        size: 1024,
        payload_size: Some(1024),
        mtime_ns: 0,
        file_id: None,
        patches: Vec::new(),
        rel_dir: PathBuf::new(),
        stem: primary.file_stem().unwrap().to_string_lossy().into_owned(),
        ext: primary
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default(),
        in_library: true,
    }
}

fn group(system: &str, members: &[usize]) -> RawGroup {
    RawGroup {
        system: system.to_string(),
        members: members.iter().map(|&u| (u, Relation::Exact)).collect(),
        action: GroupAction::Act,
        keeper_only: None,
        keys: Vec::new(),
        links: Vec::new(),
    }
}

struct Lib {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    db: SystemDb,
    exts: Vec<String>,
}

impl Lib {
    fn new() -> Lib {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        Lib {
            _tmp: tmp,
            root,
            db: SystemDb::builtin(),
            exts: default_companion_exts(),
        }
    }

    fn ctx<'a>(&'a self, profile: &'a Profile) -> CompanionCtx<'a> {
        CompanionCtx {
            root: &self.root,
            profile,
            db: &self.db,
            companion_exts: &self.exts,
        }
    }
}

fn travel(from: &Path, kind: TravelKind) -> Travel {
    Travel {
        from: from.to_path_buf(),
        kind,
    }
}

fn sorted(mut t: Vec<Travel>) -> Vec<Travel> {
    t.sort_by(|a, b| a.from.cmp(&b.from));
    t
}

/// Bytes that are not blank: a little game progress.
fn progress(seed: u8) -> Vec<u8> {
    (0..8192u32)
        .map(|i| (i as u8).wrapping_mul(seed) ^ 0x5A)
        .collect()
}

// --- Shared stems ---

#[test]
fn shared_stem_save_stays_with_the_keeper() {
    // Crash Bash shape: the PBP is kept, the cue/bin goes. `Crash Bash.srm`
    // matches both `Crash Bash.bin` and `Crash Bash.PBP`, so it stays.
    let lib = Lib::new();
    let ps = lib.root.join("ROMS/PS");
    let units = vec![
        unit(
            0,
            "psx",
            &ps,
            &["Crash Bash.cue", "Crash Bash.bin"],
            UnitKind::DiscSet,
        ),
        unit(1, "psx", &ps, &["Crash Bash.PBP"], UnitKind::Image),
    ];
    let srm = write(&ps.join("Crash Bash.srm"), &progress(3));
    let p = profile("anbernic-stock");
    let out = companions_for(&group("psx", &[0, 1]), 1, &units, &lib.ctx(&p));
    assert_eq!(out.len(), 2);

    let (travels, stays, gamelist, leaves) = &out[0];
    assert!(
        travels.iter().all(|t| t.from != srm),
        "the shared save must not leave: {travels:?}"
    );
    assert_eq!(
        stays,
        &vec![SharedStay {
            path: srm.clone(),
            with: 1
        }]
    );
    assert_eq!(gamelist, &None);
    assert!(leaves.is_empty());

    // The keeper itself carries nothing.
    let (travels, stays, gamelist, leaves) = &out[1];
    assert!(travels.is_empty());
    assert!(stays.is_empty());
    assert_eq!(gamelist, &None);
    assert!(leaves.is_empty());
}

#[test]
fn shared_stem_art_stays_with_the_keeper() {
    let lib = Lib::new();
    let ps = lib.root.join("ROMS/PS");
    let units = vec![
        unit(
            0,
            "psx",
            &ps,
            &["Crash Bash.cue", "Crash Bash.bin"],
            UnitKind::DiscSet,
        ),
        unit(1, "psx", &ps, &["Crash Bash.PBP"], UnitKind::Image),
    ];
    let art = touch(&ps.join("Imgs/Crash Bash.png"));
    let p = profile("anbernic-stock");
    let out = companions_for(&group("psx", &[0, 1]), 1, &units, &lib.ctx(&p));
    let (travels, stays, _, leaves) = &out[0];
    assert!(travels.iter().all(|t| t.from != art), "{travels:?}");
    assert!(stays.contains(&SharedStay { path: art, with: 1 }));
    assert!(leaves.is_empty());
}

// --- Own stems ---

#[test]
fn extra_with_its_own_stem_carries_save_and_art() {
    let lib = Lib::new();
    let gb = lib.root.join("ROMS/GB");
    let units = vec![
        unit(0, "gb", &gb, &["Tetris.gb"], UnitKind::Single),
        unit(
            1,
            "gb",
            &gb,
            &["Tetris (JUE) (V1.0) [!].gb"],
            UnitKind::Single,
        ),
    ];
    touch(&gb.join("Imgs/Tetris.png"));
    let sav = write(&gb.join("Tetris (JUE) (V1.0) [!].sav"), &progress(5));
    let art = touch(&gb.join("Imgs/Tetris (JUE) (V1.0) [!].png"));
    let p = profile("anbernic-stock");
    let out = companions_for(&group("gb", &[0, 1]), 0, &units, &lib.ctx(&p));

    let (travels, stays, gamelist, leaves) = &out[1];
    assert_eq!(
        sorted(travels.clone()),
        sorted(vec![
            travel(&sav, TravelKind::Save),
            travel(&art, TravelKind::Art(Media::Cover)),
        ])
    );
    assert!(stays.is_empty());
    assert_eq!(gamelist, &None);
    // The keeper has its own cover.
    assert!(leaves.is_empty());
    assert!(out[0].0.is_empty());
}

#[test]
fn art_leaving_with_the_extra_is_flagged_when_keeper_has_none() {
    let lib = Lib::new();
    let gb = lib.root.join("ROMS/GB");
    let units = vec![
        unit(0, "gb", &gb, &["Tetris.gb"], UnitKind::Single),
        unit(
            1,
            "gb",
            &gb,
            &["Tetris (JUE) (V1.0) [!].gb"],
            UnitKind::Single,
        ),
    ];
    // Case does not matter on FAT: `imgs`, `.PNG`.
    let art = touch(&gb.join("Imgs/Tetris (JUE) (V1.0) [!].PNG"));
    let p = profile("anbernic-stock");
    let out = companions_for(&group("gb", &[0, 1]), 0, &units, &lib.ctx(&p));
    let (travels, _, _, leaves) = &out[1];
    assert_eq!(travels, &vec![travel(&art, TravelKind::Art(Media::Cover))]);
    assert_eq!(
        leaves,
        &ArtLeaves {
            media: vec![Media::Cover]
        }
    );
    assert!(!leaves.is_empty());
    // The keeper never "loses" art.
    assert!(out[0].3.is_empty());
}

#[test]
fn soft_patches_of_the_extra_travel() {
    let lib = Lib::new();
    let sfc = lib.root.join("ROMS/SFC");
    let mut units = vec![
        unit(0, "snes", &sfc, &["Chrono Trigger.sfc"], UnitKind::Single),
        unit(
            1,
            "snes",
            &sfc,
            &["Chrono Trigger (1).sfc"],
            UnitKind::Single,
        ),
    ];
    let ppf = touch(&sfc.join("Chrono Trigger (1).xdelta"));
    units[1].patches = vec![ppf.clone()];
    let p = profile("anbernic-stock");
    let out = companions_for(&group("snes", &[0, 1]), 0, &units, &lib.ctx(&p));
    assert_eq!(out[1].0, vec![travel(&ppf, TravelKind::Patch)]);
}

// --- AppleDouble ---

#[test]
fn apple_double_twins_travel() {
    let lib = Lib::new();
    let gb = lib.root.join("ROMS/GB");
    let units = vec![
        unit(0, "gb", &gb, &["Tetris.gb"], UnitKind::Single),
        unit(1, "gb", &gb, &["Tetris (1).gb"], UnitKind::Single),
    ];
    touch(&gb.join("._Tetris.gb"));
    let rom_twin = touch(&gb.join("._Tetris (1).gb"));
    let sav = write(&gb.join("Tetris (1).sav"), &progress(7));
    let sav_twin = touch(&gb.join("._Tetris (1).sav"));
    let art = touch(&gb.join("Imgs/Tetris (1).png"));
    let art_twin = touch(&gb.join("Imgs/._Tetris (1).png"));
    let p = profile("anbernic-stock");
    let out = companions_for(&group("gb", &[0, 1]), 0, &units, &lib.ctx(&p));

    assert_eq!(
        sorted(out[1].0.clone()),
        sorted(vec![
            travel(&rom_twin, TravelKind::AppleDouble),
            travel(&sav, TravelKind::Save),
            travel(&sav_twin, TravelKind::AppleDouble),
            travel(&art, TravelKind::Art(Media::Cover)),
            travel(&art_twin, TravelKind::AppleDouble),
        ])
    );
    // The keeper's twin is never touched.
    assert!(out
        .iter()
        .all(|(t, ..)| t.iter().all(|t| !t.from.ends_with("._Tetris.gb"))));
}

#[test]
fn apple_double_twin_of_a_shared_file_stays() {
    let lib = Lib::new();
    let ps = lib.root.join("ROMS/PS");
    let units = vec![
        unit(
            0,
            "psx",
            &ps,
            &["Crash Bash.cue", "Crash Bash.bin"],
            UnitKind::DiscSet,
        ),
        unit(1, "psx", &ps, &["Crash Bash.PBP"], UnitKind::Image),
    ];
    write(&ps.join("Crash Bash.srm"), &progress(3));
    let srm_twin = touch(&ps.join("._Crash Bash.srm"));
    let cue_twin = touch(&ps.join("._Crash Bash.cue"));
    let p = profile("anbernic-stock");
    let out = companions_for(&group("psx", &[0, 1]), 1, &units, &lib.ctx(&p));
    let travels = &out[0].0;
    assert!(travels.contains(&travel(&cue_twin, TravelKind::AppleDouble)));
    assert!(travels.iter().all(|t| t.from != srm_twin), "{travels:?}");
}

// --- gamelist.xml ---

#[test]
fn gamelist_entry_found_for_the_extra() {
    let lib = Lib::new();
    let gb = lib.root.join("roms/gb");
    let units = vec![
        unit(0, "gb", &gb, &["Tetris.gb"], UnitKind::Single),
        unit(1, "gb", &gb, &["Tetris (1).gb"], UnitKind::Single),
        unit(2, "gb", &gb, &["Tetris (2).gb"], UnitKind::Single),
    ];
    let gamelist = write(
        &gb.join("gamelist.xml"),
        br#"<?xml version="1.0"?>
<gameList>
  <game><path>./Tetris.gb</path><name>Tetris</name></game>
  <game><path>./tetris (1).gb</path><name>Tetris</name><playcount>2</playcount></game>
</gameList>
"#,
    );
    let art = touch(&gb.join("images/Tetris (1)-image.png"));
    let p = profile("batocera");
    let out = companions_for(&group("gb", &[0, 1, 2]), 0, &units, &lib.ctx(&p));

    assert_eq!(out[0].2, None, "the keeper's entry stays");
    assert_eq!(
        out[1].2,
        Some(GamelistEntry {
            gamelist: gamelist.clone(),
            path: "./Tetris (1).gb".into(),
        })
    );
    assert_eq!(out[2].2, None, "no entry for this copy");
    assert!(out[1]
        .0
        .contains(&travel(&art, TravelKind::Art(Media::Cover))));

    // Profiles without a game list never look for one.
    let p = profile("anbernic-stock");
    let out = companions_for(&group("gb", &[0, 1, 2]), 0, &units, &lib.ctx(&p));
    assert!(out.iter().all(|(_, _, g, _)| g.is_none()));
}

// --- Matching ---

#[test]
fn companion_case_insensitive_every_dot() {
    let lib = Lib::new();
    let gb = lib.root.join("ROMS/GB");
    let units = vec![
        unit(0, "gb", &gb, &["Dr. Mario.gb"], UnitKind::Single),
        unit(1, "gb", &gb, &["Dr. Mario (1).gb"], UnitKind::Single),
    ];
    let auto = write(&gb.join("DR. MARIO (1).STATE.AUTO"), &progress(2));
    let srm = write(&gb.join("dr. mario (1).SRM"), &progress(3));
    let slot = write(&gb.join("Dr. Mario (1).state1"), &progress(4));
    let cht = touch(&gb.join("Dr. Mario (1).cht"));
    let ips = touch(&gb.join("Dr. Mario (1).ips"));
    let txt = touch(&gb.join("Dr. Mario (1).txt"));
    let nfo = touch(&gb.join("Dr. Mario (1).NFO"));
    let eep = write(&gb.join("Dr. Mario (1).eep"), &progress(9));
    // Not this copy's: another stem, an extension that is no companion.
    touch(&gb.join("Dr. Mario (1) extra.sav"));
    touch(&gb.join("Dr. Mario (1).exe"));
    touch(&gb.join("Dr.sav"));
    // The keeper's own save is never the extra's.
    write(&gb.join("Dr. Mario.sav"), &progress(6));
    let p = profile("anbernic-stock");
    let out = companions_for(&group("gb", &[0, 1]), 0, &units, &lib.ctx(&p));

    assert_eq!(
        sorted(out[1].0.clone()),
        sorted(vec![
            travel(&auto, TravelKind::State),
            travel(&srm, TravelKind::Save),
            travel(&slot, TravelKind::State),
            travel(&cht, TravelKind::Cheat),
            travel(&ips, TravelKind::Patch),
            travel(&txt, TravelKind::Text),
            travel(&nfo, TravelKind::Text),
            travel(&eep, TravelKind::Save),
        ])
    );
    assert!(out[1].1.is_empty());
}

#[test]
fn other_members_files_never_travel_as_companions() {
    // `Tetris.txt` is no companion when another member *is* `Tetris.txt`;
    // a file of a unit never travels with another unit.
    let lib = Lib::new();
    let gb = lib.root.join("ROMS/GB");
    let units = vec![
        unit(0, "gb", &gb, &["Tetris.gb"], UnitKind::Single),
        unit(1, "gb", &gb, &["Tetris.zip"], UnitKind::Zip),
    ];
    let p = profile("anbernic-stock");
    let out = companions_for(&group("gb", &[0, 1]), 0, &units, &lib.ctx(&p));
    assert!(out[1].0.is_empty(), "{:?}", out[1].0);
}

#[test]
fn a_file_two_extras_share_travels_once() {
    // Keeper in a subfolder; two extras side by side share `Tetris.sav`.
    let lib = Lib::new();
    let gb = lib.root.join("ROMS/GB");
    let mut keeper = unit(0, "gb", &gb.join("Best"), &["Tetris.gb"], UnitKind::Single);
    keeper.rel_dir = PathBuf::from("Best");
    let units = vec![
        keeper,
        unit(1, "gb", &gb, &["Tetris.gb"], UnitKind::Single),
        unit(2, "gb", &gb, &["Tetris.zip"], UnitKind::Zip),
    ];
    let sav = write(&gb.join("Tetris.sav"), &progress(8));
    let p = profile("anbernic-stock");
    let out = companions_for(&group("gb", &[0, 1, 2]), 0, &units, &lib.ctx(&p));
    let all: Vec<&Travel> = out.iter().flat_map(|(t, ..)| t).collect();
    assert_eq!(all.iter().filter(|t| t.from == sav).count(), 1, "{all:?}");
    assert!(out[1].0.contains(&travel(&sav, TravelKind::Save)));
}

#[test]
fn a_companion_a_unit_outside_the_group_uses_stays() {
    // `Tetris.zip` is another game (not in the group) beside the extra
    // `Tetris.gb`: its save and its cover share the stem and must stay.
    let lib = Lib::new();
    let gb = lib.root.join("ROMS/GB");
    let mut keeper = unit(0, "gb", &gb.join("Best"), &["Tetris.gb"], UnitKind::Single);
    keeper.rel_dir = PathBuf::from("Best");
    let units = vec![
        keeper,
        unit(1, "gb", &gb, &["Tetris.gb"], UnitKind::Single),
        unit(2, "gb", &gb, &["Tetris.zip"], UnitKind::Zip),
    ];
    let sav = write(&gb.join("Tetris.sav"), &progress(8));
    let art = touch(&gb.join("Imgs/Tetris.png"));
    let sav_twin = touch(&gb.join("._Tetris.sav"));
    let p = profile("anbernic-stock");
    let out = companions_for(&group("gb", &[0, 1]), 0, &units, &lib.ctx(&p));
    let travels = &out[1].0;
    for kept in [&sav, &art, &sav_twin] {
        assert!(travels.iter().all(|t| &t.from != kept), "{travels:?}");
    }
    assert!(out[1].3.is_empty(), "no art leaves: {:?}", out[1].3);
}

// --- Art slots ---

#[test]
fn art_slots_lists_present_slots_with_sizes() {
    let lib = Lib::new();
    let gb = lib.root.join("ROMS/GB");
    let u = unit(0, "gb", &gb, &["Tetris.gb"], UnitKind::Single);
    let art = write(&gb.join("Imgs/Tetris.png"), b"12345");
    let p = profile("anbernic-stock");
    let slots = art_slots(&u, &lib.ctx(&p));
    assert_eq!(slots, vec![(Media::Cover, art, 5)]);
}

#[test]
fn art_slots_skips_apple_double_and_missing() {
    let lib = Lib::new();
    let gb = lib.root.join("ROMS/GB");
    let u = unit(0, "gb", &gb, &["Tetris.gb"], UnitKind::Single);
    write(&gb.join("Imgs/._Tetris.png"), b"12345");
    let p = profile("anbernic-stock");
    assert!(art_slots(&u, &lib.ctx(&p)).is_empty());
}

#[cfg(unix)]
mod unix_tests {
    use super::*;

    #[test]
    fn art_slots_skips_symlinked_art() {
        let lib = Lib::new();
        let gb = lib.root.join("ROMS/GB");
        let u = unit(0, "gb", &gb, &["Tetris.gb"], UnitKind::Single);
        let outside = tempfile::tempdir().unwrap();
        let target = write(&outside.path().join("Tetris.png"), b"12345");
        fs::create_dir_all(gb.join("Imgs")).unwrap();
        std::os::unix::fs::symlink(&target, gb.join("Imgs/Tetris.png")).unwrap();
        let p = profile("anbernic-stock");
        assert!(art_slots(&u, &lib.ctx(&p)).is_empty());
    }
}
