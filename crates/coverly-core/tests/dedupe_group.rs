//! Duplicates: groups per system, weakest link, report-only reasons
//! (spec §5.2–§5.5).
//!
//! Units here exist only on paper: grouping reads no file, it works on the
//! keys the earlier steps found.

use std::path::PathBuf;

use coverly_core::dedupe::group::{build_groups, GroupOpts, RawGroup, BIG_GROUP};
use coverly_core::dedupe::keys::{ExactKey, FormatKey, KeyedUnit, VersionKey};
use coverly_core::dedupe::source::{DatLookup, NoDats};
use coverly_core::dedupe::testing::{fingerprint, FakeDats};
use coverly_core::dedupe::units::Unit;
use coverly_core::dedupe::{GroupAction, Relation, ReportWhy, UnitKind};
use coverly_core::identify::names::parse_tags;
use coverly_core::systems::SystemDb;

// --- Builders ---

fn unit(id: usize, system: &str, path: &str, kind: UnitKind) -> Unit {
    let primary = PathBuf::from(path);
    Unit {
        id,
        system: system.to_string(),
        primary: primary.clone(),
        files: vec![primary.clone()],
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

fn keyed(unit: usize, exact: Option<ExactKey>) -> KeyedUnit {
    KeyedUnit {
        unit,
        confirmed: exact.as_ref().is_some_and(ExactKey::is_confirmed),
        exact,
        patch_set: Vec::new(),
        header: None,
        dat_game: None,
        chd_raw: None,
        dat_crcs: Vec::new(),
    }
}

fn payload(seed: &str) -> Option<ExactKey> {
    Some(ExactKey::Payload {
        sha1: format!("{seed:0>40}"),
        size: 1024,
    })
}

fn vkey(system: &str, game: &str) -> Option<VersionKey> {
    let tags = parse_tags(game);
    Some(VersionKey {
        system: system.to_string(),
        title_key: tags.title_key.clone(),
        disc: tags.disc,
        game: game.to_string(),
        tags,
    })
}

fn fkey(system: &str, serial: &str, seed: u8) -> Option<FormatKey> {
    Some(FormatKey {
        system: system.to_string(),
        serials: vec![serial.to_string()],
        disc_no: Some(1),
        volume: Some(fingerprint(seed)),
        modified: false,
        serial_conflict: false,
        catalogued: false,
    })
}

struct Case {
    units: Vec<Unit>,
    l1: Vec<KeyedUnit>,
    l2: Vec<Option<VersionKey>>,
    l3: Vec<Option<FormatKey>>,
}

impl Case {
    fn new() -> Case {
        Case {
            units: Vec::new(),
            l1: Vec::new(),
            l2: Vec::new(),
            l3: Vec::new(),
        }
    }

    /// Adds a unit with its keys; returns its index.
    fn add(
        &mut self,
        system: &str,
        path: &str,
        kind: UnitKind,
        exact: Option<ExactKey>,
        v: Option<VersionKey>,
        f: Option<FormatKey>,
    ) -> usize {
        let id = self.units.len();
        self.units.push(unit(id, system, path, kind));
        self.l1.push(keyed(id, exact));
        self.l2.push(v);
        self.l3.push(f);
        id
    }

    fn rom(&mut self, system: &str, path: &str, exact: Option<ExactKey>) -> usize {
        self.add(system, path, UnitKind::Single, exact, None, None)
    }

    fn groups(&self, opts: &GroupOpts) -> Vec<RawGroup> {
        build_groups(&self.units, &self.l1, &self.l2, &self.l3, opts)
    }
}

fn opts<'a>(db: &'a SystemDb, dats: &'a dyn DatLookup) -> GroupOpts<'a> {
    GroupOpts::new(db, dats)
}

fn units_of(g: &RawGroup) -> Vec<usize> {
    let mut u: Vec<usize> = g.members.iter().map(|(u, _)| *u).collect();
    u.sort_unstable();
    u
}

fn relation_of(rel: &[(usize, Relation)], unit: usize) -> Relation {
    rel.iter()
        .find(|(u, _)| *u == unit)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("unit {unit} is not a member"))
}

fn report_why(g: &RawGroup) -> Option<&ReportWhy> {
    match &g.action {
        GroupAction::ReportOnly { why } => Some(why),
        _ => None,
    }
}

/// xorshift64*: deterministic noise for the property test.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

// --- Systems ---

#[test]
fn no_group_spans_two_systems() {
    let db = SystemDb::builtin();
    let o = opts(&db, &NoDats);

    // GB vs GBC with identical bytes: two systems, so no group at all.
    let mut c = Case::new();
    c.rom("gb", "GB/Tetris.gb", payload("A"));
    c.rom("gbc", "GBC/Tetris.gb", payload("A"));
    assert!(c.groups(&o).is_empty(), "identical files in two systems");

    // FBNEO vs MAME identical zips.
    let mut c = Case::new();
    c.add(
        "fbneo",
        "FBNEO/ffight.zip",
        UnitKind::Arcade,
        payload("B"),
        None,
        None,
    );
    c.add(
        "mame",
        "MAME/ffight.zip",
        UnitKind::Arcade,
        payload("B"),
        None,
        None,
    );
    assert!(
        c.groups(&o).is_empty(),
        "identical arcade sets in two systems"
    );

    // Level 2 and level 3 keys never cross either, even when a key's
    // system field disagrees with its unit.
    let mut c = Case::new();
    c.add(
        "psx",
        "PS/a.pbp",
        UnitKind::Image,
        None,
        None,
        fkey("psx", "SLUS-00001", 1),
    );
    c.add(
        "psp",
        "PSP/a.iso",
        UnitKind::Image,
        None,
        None,
        fkey("psx", "SLUS-00001", 1),
    );
    c.add(
        "gb",
        "GB/t.gb",
        UnitKind::Single,
        None,
        vkey("gb", "Tetris (World)"),
        None,
    );
    c.add(
        "gbc",
        "GBC/t.gb",
        UnitKind::Single,
        None,
        vkey("gb", "Tetris (World)"),
        None,
    );
    let mut o2 = opts(&db, &NoDats);
    o2.versions = true;
    o2.one_game_one_rom = true;
    assert!(c.groups(&o2).is_empty(), "keys of other systems never join");

    // Property: 200 random system pairs, the same bytes in both and a copy
    // in each: one group per system, never mixed.
    let ids: Vec<String> = db.all().iter().map(|s| s.id.clone()).collect();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..200 {
        let a = &ids[(rng.next() % ids.len() as u64) as usize];
        let mut b = &ids[(rng.next() % ids.len() as u64) as usize];
        if a == b {
            b = &ids[(ids.iter().position(|x| x == a).unwrap() + 1) % ids.len()];
        }
        let mut c = Case::new();
        let seed = format!("{round:X}");
        for (s, n) in [(a, 1), (b, 1), (a, 2), (b, 2)] {
            c.rom(s, &format!("{s}/game {n}.bin"), payload(&seed));
        }
        let groups = c.groups(&o);
        for g in &groups {
            assert!(
                g.members
                    .iter()
                    .all(|(u, _)| c.units[*u].system == g.system),
                "round {round}: a group of {} holds a unit of another system",
                g.system
            );
            assert_eq!(g.members.len(), 2, "round {round}: {a} vs {b}");
        }
        let systems: Vec<&str> = groups.iter().map(|g| g.system.as_str()).collect();
        assert!(systems.len() <= 2 && systems.iter().all(|s| s == a || s == b));
    }
}

// --- Weakest link ---

#[test]
fn crash_bash_three_members_two_relations() {
    let db = SystemDb::builtin();
    let o = opts(&db, &NoDats);
    let mut c = Case::new();
    let bin = c.add(
        "psx",
        "PS/Crash Bash.bin",
        UnitKind::Single,
        payload("C"),
        None,
        fkey("psx", "SCUS-94570", 7),
    );
    let bin1 = c.add(
        "psx",
        "PS/Crash Bash 1.bin",
        UnitKind::Single,
        payload("C"),
        None,
        fkey("psx", "SCUS-94570", 7),
    );
    let pbp = c.add(
        "psx",
        "PS/Crash Bash.PBP",
        UnitKind::Image,
        None,
        None,
        fkey("psx", "SCUS-94570", 7),
    );

    let groups = c.groups(&o);
    assert_eq!(groups.len(), 1);
    let g = &groups[0];
    assert_eq!(g.system, "psx");
    assert_eq!(units_of(g), vec![bin, bin1, pbp]);
    assert_eq!(g.action, GroupAction::Act);

    // Keeping the PBP: both bins are another format.
    let r = g.relations_to(pbp);
    assert_eq!(relation_of(&r, pbp), Relation::Exact);
    assert_eq!(relation_of(&r, bin), Relation::Format);
    assert_eq!(relation_of(&r, bin1), Relation::Format);

    // Keeping a bin: the other bin is an exact copy, the PBP a format.
    let r = g.relations_to(bin);
    assert_eq!(relation_of(&r, bin), Relation::Exact);
    assert_eq!(relation_of(&r, bin1), Relation::Exact);
    assert_eq!(relation_of(&r, pbp), Relation::Format);

    // The default relations are to the first member.
    assert_eq!(g.members, g.relations_to(bin));
}

#[test]
fn level_two_joins_only_when_enabled() {
    let db = SystemDb::builtin();
    let mut c = Case::new();
    let v10 = c.add(
        "gb",
        "GB/Tetris (JUE) (V1.0) [!].zip",
        UnitKind::Zip,
        payload("10"),
        vkey("gb", "Tetris (Japan) (En)"),
        None,
    );
    let v10b = c.add(
        "gb",
        "GB/Tetris copy.zip",
        UnitKind::Zip,
        payload("10"),
        vkey("gb", "Tetris (Japan) (En)"),
        None,
    );
    let v11 = c.add(
        "gb",
        "GB/Tetris (World) (Rev 1).zip",
        UnitKind::Zip,
        payload("11"),
        vkey("gb", "Tetris (World) (Rev 1)"),
        None,
    );

    // Off: only the exact pair.
    let o = opts(&db, &NoDats);
    let groups = c.groups(&o);
    assert_eq!(groups.len(), 1);
    assert_eq!(units_of(&groups[0]), vec![v10, v10b]);
    assert_eq!(groups[0].action, GroupAction::Act);

    // Reported: one group, report-only, the other version linked as such.
    let mut o = opts(&db, &NoDats);
    o.versions = true;
    let groups = c.groups(&o);
    assert_eq!(groups.len(), 1);
    let g = &groups[0];
    assert_eq!(units_of(g), vec![v10, v10b, v11]);
    assert_eq!(report_why(g), Some(&ReportWhy::LevelTwoOff));
    let r = g.relations_to(v10);
    assert_eq!(relation_of(&r, v10b), Relation::Exact);
    assert_eq!(relation_of(&r, v11), Relation::Version);

    // 1G1R: the same group acts.
    o.one_game_one_rom = true;
    let groups = c.groups(&o);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].action, GroupAction::Act);
}

#[test]
fn exact_copies_of_one_catalogue_game_still_act_with_level_two_on() {
    // Real sample: `Tetris Worlds (USA).zip` = `Tetris Worlds (USA) (1).zip`.
    // Both hit the same DAT game, so they share a level 2 key, but that is
    // not another version: the exact link alone decides.
    let db = SystemDb::builtin();
    let mut c = Case::new();
    let a = c.add(
        "gba",
        "GBA/Tetris Worlds (USA).zip",
        UnitKind::Zip,
        payload("tw"),
        vkey("gba", "Tetris Worlds (USA)"),
        None,
    );
    let b = c.add(
        "gba",
        "GBA/Tetris Worlds (USA) (1).zip",
        UnitKind::Zip,
        payload("tw"),
        vkey("gba", "Tetris Worlds (USA)"),
        None,
    );
    let mut o = opts(&db, &NoDats);
    o.versions = true;
    let groups = c.groups(&o);
    assert_eq!(groups.len(), 1);
    assert_eq!(units_of(&groups[0]), vec![a, b]);
    assert_eq!(groups[0].action, GroupAction::Act);
    assert!(groups[0]
        .links
        .iter()
        .all(|l| l.relation == Relation::Exact));
}

// --- Report-only guards ---

#[test]
fn unconfirmed_zip_group_is_report_only() {
    let db = SystemDb::builtin();
    let o = opts(&db, &NoDats);
    let mut c = Case::new();
    let crc = || {
        Some(ExactKey::ZipCrc {
            crc32: 0xDEAD_BEEF,
            size: 900 << 20,
        })
    };
    c.add("psp", "PSP/a.zip", UnitKind::Zip, crc(), None, None);
    c.add("psp", "PSP/b.zip", UnitKind::Zip, crc(), None, None);
    let groups = c.groups(&o);
    assert_eq!(groups.len(), 1);
    assert_eq!(report_why(&groups[0]), Some(&ReportWhy::Unconfirmed));

    // A confirmed pair in the same run still acts.
    c.rom("gb", "GB/x.gb", payload("X"));
    c.rom("gb", "GB/x (1).gb", payload("X"));
    let groups = c.groups(&o);
    assert_eq!(groups.len(), 2);
    let gb = groups.iter().find(|g| g.system == "gb").unwrap();
    assert_eq!(gb.action, GroupAction::Act);
}

#[test]
fn header_differs_group_is_report_only_unless_one_matches_the_header_db() {
    let db = SystemDb::builtin();
    let header = |mapper: u8| {
        let mut h = vec![0x4E, 0x45, 0x53, 0x1A, 1, 0, mapper << 4, 0];
        h.resize(16, 0);
        h
    };
    let build = || {
        let mut c = Case::new();
        let a = c.rom("nes", "NES/A.nes", payload("N"));
        let b = c.rom("nes", "NES/B.nes", payload("N"));
        let bare = c.rom("nes", "NES/C.nes", payload("N"));
        c.l1[a].header = Some(header(1));
        c.l1[a].dat_crcs = vec![(0x1111_1111, 1040), (0x3333_3333, 1024)];
        c.l1[b].header = Some(header(4));
        c.l1[b].dat_crcs = vec![(0x2222_2222, 1040), (0x3333_3333, 1024)];
        c.l1[bare].dat_crcs = vec![(0x3333_3333, 1024)];
        for u in [a, b] {
            c.units[u].payload_size = Some(1040);
        }
        (c, a, b, bare)
    };

    // No header database: the headers differ, report-only.
    let (c, ..) = build();
    let groups = c.groups(&opts(&db, &NoDats));
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members.len(), 3);
    assert_eq!(report_why(&groups[0]), Some(&ReportWhy::HeaderDiffers));
    assert_eq!(groups[0].keeper_only, None);

    // The headerless CRC hits for everyone: that says nothing about the
    // header.
    let dats = FakeDats::new().with_crc("nes", 0x3333_3333, "Game (USA)");
    let groups = c.groups(&opts(&db, &dats));
    assert_eq!(report_why(&groups[0]), Some(&ReportWhy::HeaderDiffers));

    // Exactly one header is the catalogue's: the group acts and that copy
    // is the only one that may stay.
    let (c, a, ..) = build();
    let dats = FakeDats::new().with_crc("nes", 0x1111_1111, "Game (USA)");
    let groups = c.groups(&opts(&db, &dats));
    assert_eq!(groups[0].action, GroupAction::Act);
    assert_eq!(groups[0].keeper_only, Some(vec![a]));

    // Both headers listed (two catalogue entries): still ambiguous.
    let dats = FakeDats::new()
        .with_crc("nes", 0x1111_1111, "Game (USA)")
        .with_crc("nes", 0x2222_2222, "Game (USA) (Alt)");
    let groups = c.groups(&opts(&db, &dats));
    assert_eq!(report_why(&groups[0]), Some(&ReportWhy::HeaderDiffers));

    // One header and a headerless copy do not differ: the group acts (the
    // keeper rules prefer the headered copy).
    let mut c = Case::new();
    let a = c.rom("nes", "NES/A.nes", payload("N"));
    c.rom("nes", "NES/C.nes", payload("N"));
    c.l1[a].header = Some(header(1));
    let groups = c.groups(&opts(&db, &NoDats));
    assert_eq!(groups[0].action, GroupAction::Act);
}

// --- Arcade ---

fn arcade_dats() -> FakeDats {
    FakeDats::new()
        .with_set("fbneo", "ffight")
        .with_set("fbneo", "ffightu")
}

#[test]
fn two_valid_arcade_set_names_are_not_duplicates() {
    let db = SystemDb::builtin();
    let dats = arcade_dats();
    let mut c = Case::new();
    c.add(
        "fbneo",
        "FBNEO/ffight.zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    c.add(
        "fbneo",
        "FBNEO/ffightu.zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    assert!(c.groups(&opts(&db, &dats)).is_empty());
}

#[test]
fn arcade_copy_of_a_valid_set_groups_with_a_forced_keeper() {
    let db = SystemDb::builtin();
    let dats = arcade_dats();
    let mut c = Case::new();
    let set = c.add(
        "fbneo",
        "FBNEO/ffight.zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    let copy = c.add(
        "fbneo",
        "FBNEO/ffight (1).zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    let groups = c.groups(&opts(&db, &dats));
    assert_eq!(groups.len(), 1);
    assert_eq!(units_of(&groups[0]), vec![set, copy]);
    assert_eq!(groups[0].action, GroupAction::Act);
    assert_eq!(groups[0].keeper_only, Some(vec![set]));

    // Two copies with no valid name between them act with any keeper.
    let mut c = Case::new();
    c.add(
        "fbneo",
        "FBNEO/ffight (1).zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    c.add(
        "fbneo",
        "FBNEO/ffight copy.zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    let groups = c.groups(&opts(&db, &dats));
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].action, GroupAction::Act);
    assert_eq!(groups[0].keeper_only, None);

    // Two valid sets and a copy: the valid sets never join, and the copy
    // alone is no group.
    let mut c = Case::new();
    c.add(
        "fbneo",
        "FBNEO/ffight.zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    c.add(
        "fbneo",
        "FBNEO/ffightu.zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    c.add(
        "fbneo",
        "FBNEO/ffight (1).zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    assert!(c.groups(&opts(&db, &dats)).is_empty());
}

#[test]
fn arcade_without_dat_is_report_only() {
    let db = SystemDb::builtin();
    let mut c = Case::new();
    c.add(
        "fbneo",
        "FBNEO/ffight.zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    c.add(
        "fbneo",
        "FBNEO/ffight (1).zip",
        UnitKind::Arcade,
        payload("F"),
        None,
        None,
    );
    let groups = c.groups(&opts(&db, &NoDats));
    assert_eq!(groups.len(), 1);
    assert_eq!(report_why(&groups[0]), Some(&ReportWhy::ArcadeNoDat));
    assert_eq!(groups[0].keeper_only, None);
}

// --- CHD ---

#[test]
fn chd_same_raw_different_sha1_is_report_only() {
    let db = SystemDb::builtin();
    let o = opts(&db, &NoDats);
    let mut c = Case::new();
    let chd = |s: &str| {
        Some(ExactKey::Chd {
            sha1: s.to_string(),
        })
    };
    let a = c.add("psx", "PS/a.chd", UnitKind::Chd, chd("AAAA"), None, None);
    let b = c.add("psx", "PS/b.chd", UnitKind::Chd, chd("BBBB"), None, None);
    let d = c.add("psx", "PS/d.chd", UnitKind::Chd, chd("DDDD"), None, None);
    c.l1[a].chd_raw = Some("RAW1".into());
    c.l1[b].chd_raw = Some("RAW1".into());
    c.l1[d].chd_raw = Some("RAW2".into());
    let groups = c.groups(&o);
    assert_eq!(groups.len(), 1);
    assert_eq!(units_of(&groups[0]), vec![a, b]);
    assert_eq!(report_why(&groups[0]), Some(&ReportWhy::ChdMetadataDiffers));
    assert_eq!(relation_of(&groups[0].relations_to(a), b), Relation::Exact);

    // Equal combined SHA1: an exact copy that acts.
    let mut c = Case::new();
    c.add("psx", "PS/a.chd", UnitKind::Chd, chd("AAAA"), None, None);
    c.add(
        "psx",
        "PS/a (1).chd",
        UnitKind::Chd,
        chd("AAAA"),
        None,
        None,
    );
    let groups = c.groups(&o);
    assert_eq!(groups[0].action, GroupAction::Act);
}

// --- Level 3 guards carried into the group ---

#[test]
fn format_link_reasons_make_the_group_report_only() {
    let db = SystemDb::builtin();
    let mut c = Case::new();
    let a = c.add(
        "psx",
        "PS/Game.pbp",
        UnitKind::Image,
        None,
        None,
        fkey("psx", "SLUS-00002", 1),
    );
    let b = c.add(
        "psx",
        "PS/Game.cue",
        UnitKind::DiscSet,
        None,
        None,
        fkey("psx", "SLUS-00002", 2),
    );
    let groups = c.groups(&opts(&db, &NoDats));
    assert_eq!(groups.len(), 1);
    assert_eq!(units_of(&groups[0]), vec![a, b]);
    assert_eq!(report_why(&groups[0]), Some(&ReportWhy::BuildDiffers));

    // With 1G1R another build is another version, but a level 2 group
    // needs both builds known to the catalogue by hash (spec §5.3): a
    // serial alone (an untagged translation keeps it) never acts.
    let mut o = opts(&db, &NoDats);
    o.one_game_one_rom = true;
    let groups = c.groups(&o);
    assert_eq!(report_why(&groups[0]), Some(&ReportWhy::BuildDiffers));
    c.l3[a].as_mut().unwrap().catalogued = true;
    c.l3[b].as_mut().unwrap().catalogued = true;
    let groups = c.groups(&o);
    assert_eq!(
        report_why(&groups[0]),
        Some(&ReportWhy::BuildDiffers),
        "a catalogued serial is not a hash identity"
    );
    c.l2[a] = vkey("psx", "Game (USA)");
    c.l2[b] = vkey("psx", "Other Game (USA)");
    let groups = c.groups(&o);
    assert_eq!(
        report_why(&groups[0]),
        Some(&ReportWhy::BuildDiffers),
        "two catalogue games under one serial"
    );
    c.l2[b] = vkey("psx", "Game (USA) (Rev 1)");
    let groups = c.groups(&o);
    assert_eq!(groups[0].action, GroupAction::Act);
    assert_eq!(
        relation_of(&groups[0].relations_to(a), b),
        Relation::Version
    );

    // A modification tag vetoes, 1G1R or not.
    c.l3[b].as_mut().unwrap().volume = Some(fingerprint(1));
    c.l3[b].as_mut().unwrap().modified = true;
    let groups = c.groups(&o);
    assert_eq!(report_why(&groups[0]), Some(&ReportWhy::Modified));

    // Level 3 off: nothing joins by serial.
    let mut o = opts(&db, &NoDats);
    o.formats = false;
    assert!(c.groups(&o).is_empty());
}

#[test]
fn a_lone_disc_of_a_set_is_a_partial_set() {
    let db = SystemDb::builtin();
    let mut c = Case::new();
    let mut set = fkey("psx", "SLUS-01161", 3).unwrap();
    set.serials.push("SLUS-01318".into());
    set.disc_no = None;
    let m3u = c.add(
        "psx",
        "PS/Driver 2.m3u",
        UnitKind::MultiDisc,
        None,
        None,
        Some(set),
    );
    let d1 = c.add(
        "psx",
        "PS/Driver 2.img",
        UnitKind::Single,
        None,
        None,
        fkey("psx", "SLUS-01161", 3),
    );
    let groups = c.groups(&opts(&db, &NoDats));
    assert_eq!(groups.len(), 1);
    assert_eq!(units_of(&groups[0]), vec![m3u, d1]);
    assert_eq!(report_why(&groups[0]), Some(&ReportWhy::PartialSet));
}

// --- Size and shape ---

#[test]
fn huge_group_is_collapsed_and_report_only() {
    let db = SystemDb::builtin();
    let o = opts(&db, &NoDats);
    let mut c = Case::new();
    for i in 0..60 {
        c.rom("gb", &format!("GB/old/copy {i}.gb"), payload("H"));
    }
    let groups = c.groups(&o);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members.len(), 60);
    assert_eq!(
        report_why(&groups[0]),
        Some(&ReportWhy::BigGroup { members: 60 })
    );

    // At the limit it still acts.
    let mut c = Case::new();
    for i in 0..BIG_GROUP {
        c.rom("gb", &format!("GB/old/copy {i}.gb"), payload("H"));
    }
    assert_eq!(c.groups(&o)[0].action, GroupAction::Act);

    // A stricter reason is kept over the size.
    let mut c = Case::new();
    for i in 0..60 {
        c.add(
            "psp",
            &format!("PSP/{i}.zip"),
            UnitKind::Zip,
            Some(ExactKey::ZipCrc { crc32: 1, size: 1 }),
            None,
            None,
        );
    }
    assert_eq!(report_why(&c.groups(&o)[0]), Some(&ReportWhy::Unconfirmed));
}

#[test]
fn singletons_are_dropped() {
    let db = SystemDb::builtin();
    let o = opts(&db, &NoDats);
    let mut c = Case::new();
    c.rom("gb", "GB/a.gb", payload("1"));
    c.rom("gb", "GB/b.gb", payload("2"));
    c.rom("gb", "GB/c.gb", None);
    c.add(
        "psx",
        "PS/d.pbp",
        UnitKind::Image,
        None,
        None,
        fkey("psx", "SLUS-00003", 1),
    );
    assert!(c.groups(&o).is_empty());
}

#[test]
fn soft_patch_sets_split_a_copy_cluster() {
    let db = SystemDb::builtin();
    let o = opts(&db, &NoDats);
    let mut c = Case::new();
    let a = c.rom("snes", "SFC/Game.sfc", payload("P"));
    let b = c.rom("snes", "SFC/Game (1).sfc", payload("P"));
    c.l1[a].patch_set = vec!["IPS1".into()];
    assert!(c.groups(&o).is_empty(), "patched and clean are other games");
    c.l1[b].patch_set = vec!["IPS1".into()];
    assert_eq!(c.groups(&o).len(), 1, "equal patches, same game");
}

#[test]
fn groups_carry_their_content_keys_and_come_out_in_a_stable_order() {
    let db = SystemDb::builtin();
    let o = opts(&db, &NoDats);
    let mut c = Case::new();
    c.rom("nes", "NES/z.nes", payload("Z"));
    c.rom("gb", "GB/b.gb", payload("B"));
    c.rom("nes", "NES/z (1).nes", payload("Z"));
    c.rom("gb", "GB/b (1).gb", payload("B"));
    let groups = c.groups(&o);
    let systems: Vec<&str> = groups.iter().map(|g| g.system.as_str()).collect();
    assert_eq!(systems, vec!["gb", "nes"]);
    assert_eq!(groups[0].keys.len(), 1, "one content key for an exact pair");
    assert!(groups[0].keys[0].starts_with("l1:gb:"));
    assert_eq!(groups, c.groups(&o), "the same input gives the same groups");
}

#[test]
fn cache_dats_know_arcade_set_names_only_once_a_catalogue_is_cached() {
    use coverly_core::cache::{Cache, DatRow};
    use coverly_core::dedupe::source::CacheDats;
    use coverly_core::identify::dat::dat_sources;

    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let dats = CacheDats::new(&cache, &db);
    assert_eq!(dats.set_name("fbneo", "ffight"), None, "nothing cached");

    let dat = dat_sources(db.get("fbneo").unwrap()).remove(0);
    let row = |game: &str| DatRow {
        game: game.to_string(),
        rom_name: "ff.30".into(),
        size: Some(131_072),
        crc32: Some("1234ABCD".into()),
        md5: None,
        sha1: None,
        serial: None,
    };
    cache
        .replace_dat(&dat, &[row("ffight"), row("ffightu")])
        .unwrap();
    assert_eq!(dats.set_name("fbneo", "ffight"), Some(true));
    assert_eq!(dats.set_name("fbneo", "FFightU"), Some(true));
    assert_eq!(dats.set_name("fbneo", "ffight (1)"), Some(false));
    assert_eq!(
        dats.set_name("gb", "ffight"),
        None,
        "another system's catalogue"
    );
}
