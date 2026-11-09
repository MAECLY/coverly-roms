//! Duplicados and game folders (game folders spec D9, D11, D15, D22, Q13):
//! an update or DLC in its game's folder is a unit with a role and never
//! pairs with its base; two updates of one base are versions; files in
//! `Mods/`, `Cheats/`, `Extras/` and `Saves/` are never units; a game
//! whose folder holds children is never removed (report only); a folder
//! game's unit leaves its fixed folders out; retro revisions keep the
//! user's rule order; and apply refuses a plan that would remove every
//! copy or a game with children. Every library is synthetic.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use coverly_core::archive::Limits;
use coverly_core::cache::{Cache, DatRow};
use coverly_core::consoles::testing as ct;
use coverly_core::dedupe::archive::ArchiveEntryListing;
use coverly_core::dedupe::keys::FACTS_VERSION;
use coverly_core::dedupe::testing::{pin_mtime, CountingBytes, FakeDiscProbe};
use coverly_core::dedupe::units::{units_from_scan, UnitOptions};
use coverly_core::dedupe::{
    apply_dedupe, plan_dedupe, DedupeApplyEnv, DedupeDecisions, DedupeEnv, DedupeError,
    DedupeOptions, DedupePlan, DupGroup, GroupAction, KeepReason, Relation, ReportWhy,
};
use coverly_core::game_folder::ChildRole;
use coverly_core::identify::dat::DatGame;
use coverly_core::organize::apply::ApplyError;
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::scan::{scan_library, LibraryScan, ScanOptions};
use coverly_core::settings::{DedupeLevel, Settings};
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::FakeTrash;
use tempfile::TempDir;

const BASE: u64 = 0x0004_0000_0005_5E00;
const UPDATE: u64 = 0x0004_000E_0005_5E00;
/// A 3DS title whose high word names no kind: the header says nothing.
const NO_KIND: u64 = 0x0004_0001_0005_5E00;

/// A library in a temp folder, its cache and its data folder.
struct Card {
    tmp: TempDir,
    root: PathBuf,
    data: PathBuf,
    cache: Cache,
    profile: Profile,
    settings: Settings,
    next: AtomicU64,
}

impl Card {
    fn new(profile: &str) -> Card {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("SD");
        fs::create_dir_all(&root).unwrap();
        Card {
            root,
            data: tmp.path().join("data"),
            tmp,
            cache: Cache::in_memory().unwrap(),
            profile: builtin_profiles()
                .into_iter()
                .find(|p| p.id == profile)
                .unwrap(),
            settings: Settings::default(),
            next: AtomicU64::new(0),
        }
    }

    /// `root/rel` (components split on `/`) with `bytes`, its mtime pinned
    /// one second after the previous file's.
    fn put(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = self.at(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, bytes).unwrap();
        pin_mtime(&p, self.next.fetch_add(1, Ordering::Relaxed));
        p
    }

    fn at(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.root.clone(), |p, c| p.join(c))
    }

    fn scan(&self) -> LibraryScan {
        scan_library(
            &self.root,
            &self.profile,
            &SystemDb::builtin(),
            &ScanOptions::default(),
        )
    }

    fn opts(&self) -> DedupeOptions {
        DedupeOptions::from_settings(&self.settings.dedupe)
    }

    fn versions(&self, one_game_one_rom: bool) -> DedupeOptions {
        let mut opts = self.opts();
        opts.levels = vec![
            DedupeLevel::Exact,
            DedupeLevel::Format,
            DedupeLevel::Version,
        ];
        opts.one_game_one_rom = one_game_one_rom;
        opts
    }

    fn plan(&self, opts: &DedupeOptions) -> DedupePlan {
        let db = SystemDb::builtin();
        let env = DedupeEnv {
            cache: &self.cache,
            db: &db,
            profile: &self.profile,
            settings: &self.settings,
            listing: &ArchiveEntryListing::new(Limits::default()),
            discs: &FakeDiscProbe::new(),
            dats: None,
            bytes: &CountingBytes::new(),
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
        };
        plan_dedupe(&self.scan(), opts, &env).unwrap()
    }

    fn apply(&self, plan: &DedupePlan) -> Result<(), ApplyError> {
        let trash = FakeTrash::new(self.tmp.path().join("trash"));
        let cancel = AtomicBool::new(false);
        let env = DedupeApplyEnv {
            data_dir: &self.data,
            trash: &trash,
            cache: &self.cache,
            progress: &|_, _| {},
            cancel: &cancel,
        };
        apply_dedupe(plan, &DedupeDecisions::default(), &env).map(|_| ())
    }
}

fn cia(tid: u64) -> Vec<u8> {
    ct::cia(tid, true, false).head_bytes()
}

/// A CIA whose TMD names title version `version`.
fn cia_version(tid: u64, version: u16) -> Vec<u8> {
    let mut s = ct::cia(tid, true, false);
    s.put(ct::CIA_TMD + 0x1DC, &version.to_be_bytes());
    s.head_bytes()
}

fn group_with<'a>(plan: &'a DedupePlan, path: &Path) -> Option<&'a DupGroup> {
    plan.groups
        .iter()
        .find(|g| g.members.iter().any(|m| m.unit.primary == path))
}

fn member_index(g: &DupGroup, path: &Path) -> usize {
    g.members
        .iter()
        .position(|m| m.unit.primary == path)
        .unwrap_or_else(|| panic!("{} not in {g:#?}", path.display()))
}

#[test]
fn facts_version_is_four() {
    assert_eq!(FACTS_VERSION, 4);
}

#[test]
fn an_update_never_pairs_with_its_base_even_with_equal_bytes() {
    let card = Card::new("coverly-library");
    let g = "ROMS/3DS/Pokemon Y (World)";
    let bytes = cia(BASE);
    let base = card.put(&format!("{g}/Pokemon Y (World).cia"), &bytes);
    let update = card.put(&format!("{g}/Updates/Pokemon Y (World) (v1.5).cia"), &bytes);
    let dlc = card.put(&format!("{g}/DLC/Pokemon Y (World) - Pack.cia"), &bytes);

    let scan = card.scan();
    let units = units_from_scan(
        &scan,
        &SystemDb::builtin(),
        &UnitOptions::for_profile(&card.profile),
        &ArchiveEntryListing::new(Limits::default()),
    );
    let role = |p: &Path| units.units.iter().find(|u| u.primary == p).map(|u| u.role);
    assert_eq!(role(&base), Some(None));
    assert_eq!(role(&update), Some(Some(ChildRole::Update)));
    assert_eq!(role(&dlc), Some(Some(ChildRole::Dlc)));
    let base_unit = units.units.iter().find(|u| u.primary == base).unwrap();
    assert!(base_unit.holds_children, "{base_unit:?}");

    for opts in [card.opts(), card.versions(true)] {
        let plan = card.plan(&opts);
        assert!(plan.groups.is_empty(), "{:#?}", plan.groups);
    }
}

#[test]
fn a_child_without_a_header_kind_takes_its_folder_role() {
    let card = Card::new("coverly-library");
    let g = "ROMS/3DS/Pokemon Y (World)";
    card.put(&format!("{g}/Pokemon Y (World).cia"), &cia(BASE));
    // Two copies of one title with no kind in its id: as children of one
    // role they are copies; one at the top level is never paired with them.
    let a = card.put(&format!("{g}/DLC/a.cia"), &cia(NO_KIND));
    let b = card.put(&format!("{g}/DLC/b.cia"), &cia(NO_KIND));
    let loose = card.put("ROMS/3DS/c.cia", &cia(NO_KIND));
    let plan = card.plan(&card.opts());
    let grp = group_with(&plan, &a).expect("the two DLC copies group");
    assert_eq!(grp.members.len(), 2, "{grp:#?}");
    assert!(grp.members.iter().any(|m| m.unit.primary == b));
    assert!(group_with(&plan, &loose).is_none(), "{:#?}", plan.groups);
}

#[test]
fn two_updates_of_one_base_are_versions_and_the_higher_is_kept() {
    let card = Card::new("coverly-library");
    let g = "ROMS/3DS/Pokemon Y (World)";
    let base = card.put(&format!("{g}/Pokemon Y (World).cia"), &cia(BASE));
    let newer = card.put(
        &format!("{g}/Updates/Pokemon Y (World) (v2.0).cia"),
        &cia_version(UPDATE, 2080),
    );
    let older = card.put(
        &format!("{g}/Updates/Pokemon Y (World) (v1.5).cia"),
        &cia_version(UPDATE, 1040),
    );
    let plan = card.plan(&card.versions(false));
    assert_eq!(plan.groups.len(), 1, "{:#?}", plan.groups);
    let grp = &plan.groups[0];
    assert_eq!(grp.members.len(), 2);
    assert_eq!(grp.members[grp.keeper].unit.primary, newer);
    assert_eq!(
        grp.members[member_index(grp, &older)].relation,
        Relation::Version
    );
    assert!(
        matches!(grp.reason, KeepReason::Revision { .. }),
        "{:?}",
        grp.reason
    );
    assert!(grp.members.iter().all(|m| m.unit.primary != base));
}

#[test]
fn files_under_mods_cheats_extras_and_saves_are_never_units() {
    let card = Card::new("coverly-library");
    let g = "ROMS/3DS/Pokemon Y (World)";
    let bytes = cia(BASE);
    card.put(&format!("{g}/Pokemon Y (World).cia"), &bytes);
    for folder in ["Mods", "Cheats", "Extras", "Saves"] {
        card.put(&format!("{g}/{folder}/copy.cia"), &bytes);
    }
    let scan = card.scan();
    let units = units_from_scan(
        &scan,
        &SystemDb::builtin(),
        &UnitOptions::for_profile(&card.profile),
        &ArchiveEntryListing::new(Limits::default()),
    );
    let game = card.at(g);
    let inside: Vec<&Path> = units
        .units
        .iter()
        .flat_map(|u| &u.files)
        .map(PathBuf::as_path)
        .filter(|p| p.starts_with(&game) && p.parent() != Some(game.as_path()))
        .collect();
    assert!(inside.is_empty(), "{inside:?}");
    assert!(card.plan(&card.opts()).groups.is_empty());
}

/// A base in its game folder with an update, and a loose copy of the base.
fn base_with_copy(card: &Card, folder_first: bool) -> (PathBuf, PathBuf) {
    let bytes = cia(BASE);
    let g = "ROMS/3DS/Pokemon Y (World)";
    let put_folder = || card.put(&format!("{g}/Pokemon Y (World).cia"), &bytes);
    let put_loose = || card.put("ROMS/3DS/Pokemon Y (World) (copy).cia", &bytes);
    let (folder, loose) = if folder_first {
        let f = put_folder();
        (f, put_loose())
    } else {
        let l = put_loose();
        (put_folder(), l)
    };
    card.put(
        &format!("{g}/Updates/Pokemon Y (World) (v1.5).cia"),
        &cia(UPDATE),
    );
    (folder, loose)
}

#[test]
fn a_base_with_children_chosen_against_is_only_reported() {
    let card = Card::new("coverly-library");
    // The loose copy is older: the tie-breaker keeps it.
    let (folder, loose) = base_with_copy(&card, false);
    let plan = card.plan(&card.opts());
    let grp = group_with(&plan, &folder).expect("the two bases group");
    assert_eq!(grp.members[grp.keeper].unit.primary, loose);
    assert_eq!(
        grp.action,
        GroupAction::ReportOnly {
            why: ReportWhy::HasChildren
        }
    );
    assert!(grp.members.iter().all(|m| m.dest.is_none()), "{grp:#?}");
    assert!(grp.members[member_index(grp, &folder)].unit.holds_children);
    assert_eq!(plan.totals.extras, 0);
}

#[test]
fn a_base_with_children_may_be_the_keeper() {
    let card = Card::new("coverly-library");
    let (folder, loose) = base_with_copy(&card, true);
    let plan = card.plan(&card.opts());
    let grp = group_with(&plan, &folder).expect("the two bases group");
    assert_eq!(grp.members[grp.keeper].unit.primary, folder);
    assert_eq!(grp.action, GroupAction::Act);
    assert!(grp.members[member_index(grp, &loose)].dest.is_some());
}

#[test]
fn apply_refuses_a_plan_removing_a_base_with_children() {
    let card = Card::new("coverly-library");
    let (folder, loose) = base_with_copy(&card, true);
    let mut plan = card.plan(&card.opts());
    let pos = plan
        .groups
        .iter()
        .position(|g| g.members.iter().any(|m| m.unit.primary == folder))
        .unwrap();
    let id = plan.groups[pos].id;
    {
        // Crafted: the loose copy stays, the game folder's base goes.
        let g = &mut plan.groups[pos];
        let (f, l) = (member_index(g, &folder), member_index(g, &loose));
        let dest = g.members[l].dest.take();
        g.members[f].dest = dest;
        g.keeper = l;
    }
    let err = card.apply(&plan).unwrap_err();
    assert!(
        matches!(err, ApplyError::Dedupe(DedupeError::RemovesGameFolder(g)) if g == id),
        "{err:?}"
    );
    assert!(folder.is_file() && loose.is_file());
}

#[test]
fn a_folder_game_unit_leaves_its_fixed_folders_out() {
    let card = Card::new("coverly-library");
    let a = card.at("ROMS/PS3/Game A");
    let b = card.at("ROMS/PS3/Game B");
    ct::ps3_jb(&a, "BLUS30001", "DG");
    ct::ps3_jb(&b, "BLUS30001", "DG");
    card.put("ROMS/PS3/Game A/Updates/notes.txt", &[7u8; 4096]);
    card.put("ROMS/PS3/Game A/Saves/save.dat", &[8u8; 4096]);
    let scan = card.scan();
    let units = units_from_scan(
        &scan,
        &SystemDb::builtin(),
        &UnitOptions::for_profile(&card.profile),
        &ArchiveEntryListing::new(Limits::default()),
    );
    let unit = |p: &Path| units.units.iter().find(|u| u.primary == p).unwrap();
    assert_eq!(
        unit(&a).size,
        unit(&b).size,
        "{:?} {:?}",
        unit(&a),
        unit(&b)
    );
    assert!(unit(&a).holds_children);
    assert!(!unit(&b).holds_children);
    // Same tree without the fixed folders: the two are copies; the one
    // holding a fixed folder is never the one set aside.
    let plan = card.plan(&card.opts());
    let grp = group_with(&plan, &a).expect("the two trees group");
    assert!(grp.members.iter().any(|m| m.unit.primary == b));
    assert!(
        grp.members[grp.keeper].unit.primary == a
            || grp.action
                == GroupAction::ReportOnly {
                    why: ReportWhy::HasChildren
                },
        "{grp:#?}"
    );
}

// --- Retro revisions (D15) ---

const GBA_DAT: &str = "metadat/no-intro/Nintendo - Game Boy Advance.dat";

fn rom_bytes(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| if i % 3 == 0 { 0 } else { (i as u8) ^ seed })
        .collect()
}

fn cache_dat(cache: &Cache, games: &[(&str, &[u8])]) {
    let games: Vec<DatGame> = games
        .iter()
        .map(|(name, bytes)| DatGame {
            name: name.to_string(),
            roms: vec![DatRow {
                game: name.to_string(),
                rom_name: format!("{name}.gba"),
                size: Some(bytes.len() as u64),
                crc32: Some(format!("{:08X}", crc32fast::hash(bytes))),
                md5: None,
                sha1: None,
                serial: None,
            }],
            cloneof: None,
            region: None,
            is_bios: false,
        })
        .collect();
    cache.replace_dat_games(GBA_DAT, &games).unwrap();
}

/// `Advance Wars (USA).gba` and `(Rev 1)`, both catalogued.
fn revisions(card: &Card) -> (PathBuf, PathBuf) {
    let first = rom_bytes(7, 4096);
    let rev = rom_bytes(8, 4100);
    let a = card.put("ROMS/GBA/Advance Wars (USA).gba", &first);
    let b = card.put("ROMS/GBA/Advance Wars (USA) (Rev 1).gba", &rev);
    cache_dat(
        &card.cache,
        &[
            ("Advance Wars (USA)", &first),
            ("Advance Wars (USA) (Rev 1)", &rev),
        ],
    );
    (a, b)
}

#[test]
fn the_newest_revision_is_the_keeper_with_default_rules() {
    let card = Card::new("anbernic-stock");
    let (first, rev) = revisions(&card);
    for one_game_one_rom in [false, true] {
        let plan = card.plan(&card.versions(one_game_one_rom));
        let grp = group_with(&plan, &rev).expect("revisions group");
        assert_eq!(grp.members[grp.keeper].unit.primary, rev);
        assert!(
            matches!(grp.reason, KeepReason::Revision { .. }),
            "{:?}",
            grp.reason
        );
        assert_eq!(
            grp.members[member_index(grp, &first)].relation,
            Relation::Version
        );
        if one_game_one_rom {
            assert_eq!(grp.action, GroupAction::Act);
        } else {
            assert!(matches!(grp.action, GroupAction::ReportOnly { .. }));
        }
    }
}

#[test]
fn a_save_keeps_the_first_release_by_the_rule_order() {
    let card = Card::new("anbernic-stock");
    let (first, _rev) = revisions(&card);
    card.put("ROMS/GBA/Advance Wars (USA).sav", &rom_bytes(20, 8192));
    let plan = card.plan(&card.versions(true));
    let grp = group_with(&plan, &first).expect("revisions group");
    assert_eq!(grp.members[grp.keeper].unit.primary, first);
    assert!(
        matches!(grp.reason, KeepReason::HasSave { .. }),
        "{:?}",
        grp.reason
    );
}

#[test]
fn apply_refuses_a_plan_removing_every_revision() {
    let card = Card::new("anbernic-stock");
    let (first, rev) = revisions(&card);
    let mut plan = card.plan(&card.versions(true));
    let pos = plan
        .groups
        .iter()
        .position(|g| g.members.iter().any(|m| m.unit.primary == rev))
        .unwrap();
    let id = plan.groups[pos].id;
    {
        let g = &mut plan.groups[pos];
        assert_eq!(g.action, GroupAction::Act);
        let k = g.keeper;
        let dest = g
            .members
            .iter()
            .find_map(|m| m.dest.clone())
            .expect("the other revision has a destination");
        g.members[k].dest = Some(dest.with_file_name("keeper too.gba"));
    }
    let err = card.apply(&plan).unwrap_err();
    assert!(
        matches!(err, ApplyError::Dedupe(DedupeError::EveryMemberMoves(g)) if g == id),
        "{err:?}"
    );
    assert!(first.is_file() && rev.is_file());
}

#[test]
fn another_base_version_never_makes_proven_copies_report_only() {
    // Only updates keep their title key beside a byte proof (D9): two
    // byte-identical copies of a base still act when another version of
    // that base, with other bytes, sits in the library.
    let card = Card::new("coverly-library");
    let first = card.put("ROMS/3DS/Pokemon Y (World).cia", &cia(BASE));
    let copy = card.put("ROMS/3DS/Pokemon Y (World) (copy).cia", &cia(BASE));
    let other = card.put(
        "ROMS/3DS/Pokemon Y (World) (Rev 1).cia",
        &cia_version(BASE, 2080),
    );
    let plan = card.plan(&card.opts());
    let grp = group_with(&plan, &first).expect("the two copies group");
    assert_eq!(grp.action, GroupAction::Act, "{grp:#?}");
    assert_eq!(grp.members.len(), 2, "{grp:#?}");
    assert!(grp.members.iter().any(|m| m.unit.primary == copy));
    assert!(grp.members.iter().all(|m| m.unit.primary != other));
}

#[test]
fn the_has_children_note_and_flag_keep_their_wire_names() {
    // `src/ipc/dedupeTypes.ts` reads `{ kind: 'has-children' }` and
    // `holds_children` (absent when false).
    assert_eq!(
        serde_json::to_value(ReportWhy::HasChildren).unwrap(),
        serde_json::json!({ "kind": "has-children" })
    );
    let card = Card::new("coverly-library");
    let (folder, _) = base_with_copy(&card, false);
    let plan = card.plan(&card.opts());
    let grp = group_with(&plan, &folder).unwrap();
    let wire = serde_json::to_value(grp).unwrap();
    let units: Vec<&serde_json::Value> = wire["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| &m["unit"])
        .collect();
    assert!(
        units.iter().any(|u| u["holds_children"] == true),
        "{units:?}"
    );
    assert!(
        units.iter().any(|u| u.get("holds_children").is_none()),
        "{units:?}"
    );
}

#[test]
fn a_folder_game_given_a_fixed_folder_after_the_plan_is_never_set_aside() {
    let card = Card::new("coverly-library");
    let a = card.at("ROMS/PS3/Game A");
    let b = card.at("ROMS/PS3/Game B");
    ct::ps3_jb(&a, "BLUS30001", "DG");
    ct::ps3_jb(&b, "BLUS30001", "DG");
    let plan = card.plan(&card.opts());
    let grp = group_with(&plan, &a).expect("the two trees group");
    assert_eq!(grp.action, GroupAction::Act, "{grp:#?}");
    let extra = grp
        .members
        .iter()
        .find(|m| m.dest.is_some())
        .expect("one copy is set aside")
        .unit
        .primary
        .clone();
    // Organizar puts an update into the copy's folder after the plan: the
    // fixed folders are left out of its measure, so only this check keeps
    // the update from leaving with the folder.
    let update = card.put(
        &format!(
            "ROMS/PS3/{}/Updates/Game (v1.01).pkg",
            extra.file_name().unwrap().to_string_lossy()
        ),
        &[9u8; 4096],
    );
    card.apply(&plan).unwrap();
    assert!(update.is_file(), "the update left with its game folder");
    assert!(extra.is_dir());
}
