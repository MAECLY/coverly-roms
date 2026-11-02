//! Loose multi-disc games (adjustments spec D24): discs grouped into one
//! set by their headers or a DAT hash, a name-only group reviewed, placed
//! by the profile's `DiscLayout`, and a no-clobber `.m3u` that undo
//! removes. Every byte is synthetic; names may be real.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::cache::{Cache, DatRow};
use coverly_core::consoles::testing as ct;
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::disc::testing as dt;
use coverly_core::identify::dat::dat_sources;
use coverly_core::organize::discsets::SetEvidence;
use coverly_core::organize::{
    apply_organize, plan_from_survey, survey, ApplyEnv, Choice, Decision, ItemKind, ItemNote,
    MetaProbe, OrganizeAction, OrganizeEnv, OrganizeItem, OrganizeOptions, OrganizePlan,
    TransferMode,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::undo_job_with;
use tempfile::TempDir;

struct NoCloud;
impl MetaProbe for NoCloud {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
}

struct World {
    db: SystemDb,
    profiles: Vec<Profile>,
    cache: Cache,
}

impl World {
    fn new() -> World {
        World {
            db: SystemDb::builtin(),
            profiles: builtin_profiles(),
            cache: Cache::in_memory().unwrap(),
        }
    }

    fn env(&self) -> OrganizeEnv<'_> {
        OrganizeEnv {
            db: &self.db,
            profiles: &self.profiles,
            cache: &self.cache,
            dats: None,
            probe: &NoCloud,
            online: None,
            titles: None,
            now: SystemTime::now(),
        }
    }
}

/// A source pile and a separate library.
struct Dirs {
    _tmp: TempDir,
    src: PathBuf,
    target: PathBuf,
    data: PathBuf,
}

fn dirs() -> Dirs {
    let tmp = TempDir::new().unwrap();
    let (src, target, data) = (
        tmp.path().join("S"),
        tmp.path().join("T"),
        tmp.path().join("d"),
    );
    for d in [&src, &target, &data] {
        fs::create_dir_all(d).unwrap();
    }
    Dirs {
        _tmp: tmp,
        src,
        target,
        data,
    }
}

fn put(path: &Path, bytes: &[u8], n: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    pin_mtime(path, n);
}

/// A one-track cue sheet and its track.
fn cue_set(dir: &Path, stem: &str, track: &[u8], n: u64) {
    let cue = format!("FILE \"{stem}.bin\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n");
    put(&dir.join(format!("{stem}.cue")), cue.as_bytes(), n);
    put(&dir.join(format!("{stem}.bin")), track, n);
}

/// A GameCube disc head in an RVZ: `id6` and the disc byte `number`.
fn gc_rvz(id6: &str, number: u8) -> Vec<u8> {
    let mut h = ct::gc_header(id6, false);
    h[6] = number;
    ct::wia(&h, true, 1).head_bytes()
}

/// The first profile with `game_folder` discs (and a playlist).
fn layout(world: &World, game_folder: bool) -> String {
    world
        .profiles
        .iter()
        .find(|p| p.disc_layout.is_game_folder() == game_folder && p.disc_layout.m3u_for_multi())
        .unwrap()
        .id
        .clone()
}

fn options(layout: &str, target: &Path, mode: TransferMode) -> OrganizeOptions {
    OrganizeOptions {
        layout: layout.into(),
        target: Some(target.to_path_buf()),
        mode: Some(mode),
        ..OrganizeOptions::default()
    }
}

fn plan_with(
    world: &World,
    d: &Dirs,
    opts: &OrganizeOptions,
    decisions: &[Decision],
) -> OrganizePlan {
    let env = world.env();
    let s = survey(&d.src, opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
    plan_from_survey(&s, opts, decisions, &env).unwrap()
}

fn plan(world: &World, d: &Dirs) -> OrganizePlan {
    let opts = options(&layout(world, false), &d.target, TransferMode::Move);
    plan_with(world, d, &opts, &[])
}

fn sets(plan: &OrganizePlan) -> Vec<&OrganizeItem> {
    plan.items
        .iter()
        .filter(|i| i.kind == ItemKind::Set)
        .collect()
}

/// The single set of a plan.
fn the_set(plan: &OrganizePlan) -> &OrganizeItem {
    let s = sets(plan);
    assert_eq!(s.len(), 1, "{:#?}", plan.items);
    s[0]
}

/// File names of `paths`, by component.
fn names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

fn playlist_name(item: &OrganizeItem) -> Option<String> {
    item.playlist
        .as_ref()
        .map(|p| p.path.file_name().unwrap().to_string_lossy().into_owned())
}

#[test]
fn gamecube_discs_with_one_id_are_one_set() {
    let world = World::new();
    let d = dirs();
    put(
        &d.src.join("Resident Evil 4 (USA) (Disc 1).rvz"),
        &gc_rvz("G4BE08", 0),
        1,
    );
    put(
        &d.src.join("Resident Evil 4 (USA) (Disc 2).rvz"),
        &gc_rvz("G4BE08", 1),
        2,
    );
    let p = plan(&world, &d);
    assert_eq!(p.items.len(), 1);
    let set = the_set(&p);
    assert_eq!(set.system_id.as_deref(), Some("gamecube"));
    assert_eq!(set.action, OrganizeAction::Auto);
    assert_eq!(
        names(&set.sources),
        [
            "Resident Evil 4 (USA) (Disc 1).rvz",
            "Resident Evil 4 (USA) (Disc 2).rvz"
        ]
    );
    let playlist = set.playlist.as_ref().unwrap();
    assert_eq!(
        playlist_name(set).as_deref(),
        Some("Resident Evil 4 (USA).m3u")
    );
    // Flat: beside the discs in the system folder.
    assert_eq!(
        playlist.path.parent(),
        set.target.as_ref().unwrap().parent()
    );
    assert_eq!(names(&playlist.discs), names(&set.sources));
}

#[test]
fn saturn_discs_counted_by_their_device_field() {
    let world = World::new();
    let d = dirs();
    for n in 1..=3u64 {
        cue_set(
            &d.src,
            &format!("Panzer Dragoon Saga (USA) (Disc {n})"),
            &dt::saturn_track_disc("MK-81307  ", &format!("{n}/3")),
            n,
        );
    }
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert_eq!(p.items.len(), 1);
    assert_eq!(set.system_id.as_deref(), Some("saturn"));
    assert_eq!(set.action, OrganizeAction::Auto);
    assert_eq!(set.sources.len(), 6);
    let lines = &set.playlist.as_ref().unwrap().discs;
    assert_eq!(
        names(lines),
        [
            "Panzer Dragoon Saga (USA) (Disc 1).cue",
            "Panzer Dragoon Saga (USA) (Disc 2).cue",
            "Panzer Dragoon Saga (USA) (Disc 3).cue",
        ]
    );
    assert!(!set
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::MissingDisc { .. })));
}

#[test]
fn dreamcast_discs_counted_by_gd_rom() {
    let world = World::new();
    let d = dirs();
    for n in 1..=2u64 {
        cue_set(
            &d.src,
            &format!("Shenmue II (Europe) (Disc {n})"),
            &dt::dreamcast_track03("MK-5122350", &format!("{n}/2")),
            n,
        );
    }
    let set = the_set(&plan(&world, &d)).clone();
    assert_eq!(set.system_id.as_deref(), Some("dreamcast"));
    assert_eq!(set.action, OrganizeAction::Auto);
    assert_eq!(
        playlist_name(&set).as_deref(),
        Some("Shenmue II (Europe).m3u")
    );
}

#[test]
fn a_missing_disc_is_noted_and_the_playlist_lists_the_rest() {
    let world = World::new();
    let d = dirs();
    for n in [1u8, 3] {
        cue_set(
            &d.src,
            &format!("Grandia (Japan) (Disc {n})"),
            &dt::saturn_track_disc("T-4507G   ", &format!("{n}/3")),
            u64::from(n),
        );
    }
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert!(set.notes.contains(&ItemNote::MissingDisc { n: 2 }));
    assert_eq!(set.playlist.as_ref().unwrap().discs.len(), 2);
}

#[test]
fn xbox_360_discs_group_without_a_playlist() {
    let world = World::new();
    let d = dirs();
    for n in 1..=2u8 {
        put(
            &d.src.join(format!("Lost Odyssey (USA) (Disc {n}).xex")),
            &ct::xex_loose(ct::XEX_MODULE_TITLE, 0x4D53_0832, n, 2),
            u64::from(n),
        );
    }
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert_eq!(set.system_id.as_deref(), Some("xbox360"));
    assert_eq!(set.playlist, None);
}

/// A PS1 set the DAT names by hash: two headerless tracks.
#[test]
fn dat_hash_names_confirm_a_playstation_set() {
    let world = World::new();
    let d = dirs();
    let mut rows = Vec::new();
    for n in 1..=2u8 {
        let bytes: Vec<u8> = (0..4096u32)
            .map(|i| (i as u8) ^ n.wrapping_mul(37))
            .collect();
        let name = format!("Final Fantasy VII (USA) (Disc {n})");
        put(&d.src.join(format!("{name}.bin")), &bytes, n.into());
        rows.push(DatRow {
            game: name.clone(),
            rom_name: format!("{name}.bin"),
            size: Some(bytes.len() as u64),
            crc32: Some(format!("{:08X}", crc32fast::hash(&bytes))),
            md5: None,
            sha1: None,
            serial: None,
        });
    }
    let dat = dat_sources(world.db.get("psx").unwrap()).remove(0);
    world.cache.replace_dat(&dat, &rows).unwrap();
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert_eq!(set.system_id.as_deref(), Some("psx"));
    assert_eq!(set.action, OrganizeAction::Auto, "a DAT hash confirms");
    assert_eq!(
        playlist_name(set).as_deref(),
        Some("Final Fantasy VII (USA).m3u")
    );
}

#[test]
fn one_folder_per_disc_is_one_set() {
    let world = World::new();
    let d = dirs();
    for n in 1..=2u64 {
        let stem = format!("Policenauts (Japan) (Disc {n})");
        cue_set(
            &d.src.join(&stem),
            &stem,
            &dt::saturn_track_disc("T-9527G   ", &format!("{n}/2")),
            n,
        );
    }
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert_eq!(set.action, OrganizeAction::Auto);
    let playlist = set.playlist.as_ref().unwrap();
    // The disc folders keep their names below the system folder; the
    // playlist sits beside them and names each disc's sheet in it.
    let system = playlist.path.parent().unwrap();
    let first = set.target.as_ref().unwrap();
    assert_eq!(first.parent().unwrap().parent().unwrap(), system);
    assert_eq!(
        first.parent().unwrap().file_name().unwrap(),
        "Policenauts (Japan) (Disc 1)"
    );
}

#[test]
fn a_game_folder_layout_puts_the_set_in_its_title_folder() {
    let world = World::new();
    let d = dirs();
    put(
        &d.src.join("Tales of Symphonia (USA) (Disc 1).rvz"),
        &gc_rvz("GQSE8P", 0),
        1,
    );
    put(
        &d.src.join("Tales of Symphonia (USA) (Disc 2).rvz"),
        &gc_rvz("GQSE8P", 1),
        2,
    );
    let opts = options(&layout(&world, true), &d.target, TransferMode::Move);
    let p = plan_with(&world, &d, &opts, &[]);
    let set = the_set(&p);
    let folder = set.target.as_ref().unwrap().parent().unwrap();
    assert_eq!(folder.file_name().unwrap(), "Tales of Symphonia (USA)");
    let playlist = set.playlist.as_ref().unwrap();
    assert_eq!(playlist.path.parent().unwrap(), folder);
    assert_eq!(
        playlist_name(set).as_deref(),
        Some("Tales of Symphonia (USA).m3u")
    );
}

#[test]
fn names_alone_are_reviewed() {
    let world = World::new();
    let d = dirs();
    for n in 1..=2u64 {
        cue_set(
            &d.src,
            &format!("Night Trap (USA) (Disc {n})"),
            &dt::segacd_track(),
            n,
        );
    }
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert_eq!(set.system_id.as_deref(), Some("segacd"));
    assert_eq!(set.action, OrganizeAction::Review);
    assert!(set.notes.contains(&ItemNote::DiscSet));
    // Accepted, the set moves with its playlist.
    let opts = options(&layout(&world, false), &d.target, TransferMode::Move);
    let accepted = plan_with(
        &world,
        &d,
        &opts,
        &[Decision {
            item: set.id,
            choice: Choice::Accept,
        }],
    );
    let set = the_set(&accepted);
    assert_eq!(set.action, OrganizeAction::Auto);
    assert!(set.playlist.is_some());
}

#[test]
fn a_ps2_set_gets_no_playlist() {
    let world = World::new();
    let d = dirs();
    for n in 1..=2u64 {
        put(
            &d.src
                .join(format!("Xenosaga Episode III (USA) (Disc {n}).iso")),
            &dt::ps2_iso_booting("cdrom0:\\SLUS_216.20;1"),
            n,
        );
    }
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert_eq!(set.system_id.as_deref(), Some("ps2"));
    let opts = options(&layout(&world, false), &d.target, TransferMode::Move);
    let accepted = plan_with(
        &world,
        &d,
        &opts,
        &[Decision {
            item: set.id,
            choice: Choice::Accept,
        }],
    );
    assert_eq!(the_set(&accepted).playlist, None);
}

#[test]
fn rejected_groups_stay_separate() {
    let world = World::new();
    // Two discs with one number: copies, for the duplicates step.
    let d = dirs();
    put(
        &d.src.join("Skies of Arcadia (USA) (Disc 1).rvz"),
        &gc_rvz("GEAE8P", 0),
        1,
    );
    put(
        &d.src.join("Skies of Arcadia (USA) (Disk 1).rvz"),
        &gc_rvz("GEAE8P", 0),
        2,
    );
    assert!(sets(&plan(&world, &d)).is_empty());
    // A header that says disc 1 under a «Disc 2» name.
    let d = dirs();
    put(
        &d.src.join("Baten Kaitos (USA) (Disc 1).rvz"),
        &gc_rvz("GKBEAF", 0),
        1,
    );
    put(
        &d.src.join("Baten Kaitos (USA) (Disc 2).rvz"),
        &gc_rvz("GKBEAF", 0),
        2,
    );
    assert!(sets(&plan(&world, &d)).is_empty());
    // Another region is another game.
    let d = dirs();
    put(
        &d.src.join("Metal Gear Solid (USA) (Disc 1).rvz"),
        &gc_rvz("GGSEA4", 0),
        1,
    );
    put(
        &d.src.join("Metal Gear Solid (Europe) (Disc 2).rvz"),
        &gc_rvz("GGSPA4", 1),
        2,
    );
    assert!(sets(&plan(&world, &d)).is_empty());
    // Saturn counts that disagree.
    let d = dirs();
    cue_set(
        &d.src,
        "Lunar (Japan) (Disc 1)",
        &dt::saturn_track_disc("T-1    ", "1/2"),
        1,
    );
    cue_set(
        &d.src,
        "Lunar (Japan) (Disc 2)",
        &dt::saturn_track_disc("T-1    ", "2/3"),
        2,
    );
    assert!(sets(&plan(&world, &d)).iter().all(|s| s.playlist.is_none()));
    assert_eq!(plan(&world, &d).items.len(), 2);
}

/// Floppy disks of a computer are no disc set: they stay as they are.
#[test]
fn computer_disks_are_not_grouped() {
    let world = World::new();
    let d = dirs();
    for n in 1..=2u64 {
        put(
            &d.src.join(format!("Monkey Island (Disk {n} of 2).adf")),
            &vec![0u8; 901_120],
            n,
        );
    }
    assert!(sets(&plan(&world, &d)).is_empty());
}

#[test]
fn an_opening_disc_0_is_an_extra_member() {
    let world = World::new();
    let d = dirs();
    put(
        &d.src.join("Game (USA) (Disc 0).rvz"),
        &gc_rvz("GZZE01", 0),
        0,
    );
    put(
        &d.src.join("Game (USA) (Disc 1).rvz"),
        &gc_rvz("GZZE01", 0),
        1,
    );
    put(
        &d.src.join("Game (USA) (Disc 2).rvz"),
        &gc_rvz("GZZE01", 1),
        2,
    );
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert_eq!(set.sources.len(), 3);
    assert!(!set
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::MissingDisc { .. })));
    assert_eq!(names(&set.sources)[0], "Game (USA) (Disc 0).rvz");
}

#[test]
fn a_pile_playlist_set_is_left_as_it_is() {
    let world = World::new();
    let d = dirs();
    for n in 1..=2u64 {
        cue_set(
            &d.src,
            &format!("Riven (USA) (Disc {n})"),
            &dt::saturn_track_disc("T-2      ", &format!("{n}/2")),
            n,
        );
    }
    put(
        &d.src.join("Riven (USA).m3u"),
        b"Riven (USA) (Disc 1).cue\nRiven (USA) (Disc 2).cue\n",
        3,
    );
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert_eq!(names(&set.sources)[0], "Riven (USA).m3u");
    assert_eq!(set.playlist, None);
}

#[test]
fn a_playlist_already_at_the_target_is_kept() {
    let world = World::new();
    let d = dirs();
    put(
        &d.src.join("Resident Evil 4 (USA) (Disc 1).rvz"),
        &gc_rvz("G4BE08", 0),
        1,
    );
    put(
        &d.src.join("Resident Evil 4 (USA) (Disc 2).rvz"),
        &gc_rvz("G4BE08", 1),
        2,
    );
    let first = plan(&world, &d);
    let at = d
        .target
        .join(&the_set(&first).playlist.as_ref().unwrap().path);
    put(&at, b"mine\n", 9);
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert!(set.target.is_some(), "the set still moves");
    assert_eq!(set.playlist, None);
}

/// A synthetic copy of the real PS1 layout's names: one disc, six tracks.
#[test]
fn a_single_disc_with_tracks_is_not_grouped() {
    let world = World::new();
    let d = dirs();
    let mut cue = String::new();
    for t in 1..=6 {
        let name = format!("Next Tetris, The (USA) (Track {t:02}).bin");
        let bytes = if t == 1 {
            dt::psx_track(true, "cdrom:\\SLUS_007.20;1")
        } else {
            vec![0u8; 2352 * 4]
        };
        put(&d.src.join(&name), &bytes, t);
        cue.push_str(&format!(
            "FILE \"{name}\" BINARY\n  TRACK {t:02} MODE2/2352\n    INDEX 01 00:00:00\n"
        ));
    }
    put(&d.src.join("Next Tetris, The (USA).cue"), cue.as_bytes(), 7);
    let p = plan(&world, &d);
    let set = the_set(&p);
    assert_eq!(set.sources.len(), 7);
    assert_eq!(set.playlist, None);
}

fn apply(d: &Dirs, plan: &OrganizePlan, trash: &FakeTrash) -> String {
    let progress = |_| {};
    let env = ApplyEnv {
        data_dir: &d.data,
        progress: &progress,
        cancel: &AtomicBool::new(false),
        trash,
        tools: Some(&[]),
    };
    apply_organize(plan, &env).unwrap().job.unwrap()
}

fn saturn_pair(d: &Dirs) {
    for n in 1..=2u64 {
        cue_set(
            &d.src,
            &format!("Sakura Taisen (Japan) (Disc {n})"),
            &dt::saturn_track_disc("GS-9037   ", &format!("{n}/2")),
            n,
        );
    }
}

#[test]
fn the_playlist_is_written_byte_exact_and_undone() {
    for mode in [TransferMode::Move, TransferMode::Copy] {
        let world = World::new();
        let d = dirs();
        saturn_pair(&d);
        let opts = options(&layout(&world, false), &d.target, mode);
        let p = plan_with(&world, &d, &opts, &[]);
        let set = the_set(&p).clone();
        let m3u = d.target.join(&set.playlist.as_ref().unwrap().path);
        let trash = FakeTrash::new(d.data.join("trash"));
        let job = apply(&d, &p, &trash);
        assert_eq!(
            fs::read_to_string(&m3u).unwrap(),
            "Sakura Taisen (Japan) (Disc 1).cue\nSakura Taisen (Japan) (Disc 2).cue\n",
            "{mode:?}"
        );
        for target in std::iter::once(set.target.as_ref().unwrap()).chain(&set.set_targets) {
            assert!(d.target.join(target).is_file(), "{mode:?} {target:?}");
        }
        undo_job_with(&d.data, &job, &trash).unwrap();
        assert!(!m3u.exists(), "{mode:?}: undo removes the playlist");
        for source in &set.sources {
            assert!(d.src.join(source).is_file(), "{mode:?} {source:?}");
        }
    }
}

/// The playlist lists the discs in disc order, `Disc 10` after `Disc 9`.
#[test]
fn the_playlist_follows_the_disc_numbers() {
    let world = World::new();
    let d = dirs();
    for n in 1..=10u64 {
        cue_set(
            &d.src,
            &format!("Night Trap (USA) (Disc {n})"),
            &dt::segacd_track(),
            n,
        );
    }
    let opts = options(&layout(&world, false), &d.target, TransferMode::Move);
    let first = plan_with(&world, &d, &opts, &[]);
    let id = the_set(&first).id;
    let p = plan_with(
        &world,
        &d,
        &opts,
        &[Decision {
            item: id,
            choice: Choice::Accept,
        }],
    );
    let set = the_set(&p).clone();
    let m3u = d.target.join(&set.playlist.as_ref().unwrap().path);
    let trash = FakeTrash::new(d.data.join("trash"));
    apply(&d, &p, &trash);
    let want: String = (1..=10)
        .map(|n| format!("Night Trap (USA) (Disc {n}).cue\n"))
        .collect();
    assert_eq!(fs::read_to_string(&m3u).unwrap(), want);
}

#[test]
fn a_playlist_written_meanwhile_is_never_replaced() {
    let world = World::new();
    let d = dirs();
    saturn_pair(&d);
    let p = plan(&world, &d);
    let set = the_set(&p).clone();
    let m3u = d.target.join(&set.playlist.as_ref().unwrap().path);
    put(&m3u, b"mine\n", 9);
    let trash = FakeTrash::new(d.data.join("trash"));
    let job = apply(&d, &p, &trash);
    assert_eq!(fs::read(&m3u).unwrap(), b"mine\n");
    undo_job_with(&d.data, &job, &trash).unwrap();
    assert_eq!(
        fs::read(&m3u).unwrap(),
        b"mine\n",
        "undo keeps what it did not write"
    );
}

#[test]
fn set_evidence_is_kept_on_the_survey() {
    let world = World::new();
    let d = dirs();
    put(
        &d.src.join("Resident Evil 4 (USA) (Disc 1).rvz"),
        &gc_rvz("G4BE08", 0),
        1,
    );
    put(
        &d.src.join("Resident Evil 4 (USA) (Disc 2).rvz"),
        &gc_rvz("G4BE08", 1),
        2,
    );
    let opts = options(&layout(&world, false), &d.target, TransferMode::Move);
    let env = world.env();
    let s = survey(&d.src, &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
    let set = s.entries[0].disc_set.as_ref().unwrap();
    assert_eq!(set.evidence, SetEvidence::NintendoId);
    assert_eq!(set.title, "Resident Evil 4 (USA)");
    assert_eq!(set.discs.len(), 2);
}
