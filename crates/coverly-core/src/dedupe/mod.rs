//! Duplicates (duplicates spec): the same game stored more than once inside
//! one system. Content decides, never the file name.
//!
//! This module holds the plan types shared by the planner, the CLI and the
//! app. Reasons are enums plus data; the UI and the CLI render them.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::settings::{DedupeLevel, DedupeSettings, RuleToggle};

pub mod companions;
pub mod group;
pub mod keeper;
pub mod keys;
mod plan;
pub mod saves;
pub mod source;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
pub mod units;

pub use companions::{ArtLeaves, GamelistEntry, SharedStay, Travel, TravelKind};
pub use keeper::{DumpQuality, KeepReason};
pub use saves::{SaveInfo, SaveKey};
pub use units::UnitKind;

/// Version of the [`DedupePlan`] format.
pub const PLAN_VERSION: u32 = 1;

/// The phases of a duplicates plan, in the order they run (spec §10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DedupePhase {
    /// Walking the library (metadata only).
    Listing,
    /// Size buckets and the cache.
    Sizes,
    /// Headers, prefix CRCs and full hashes of the files that collide.
    Fingerprints,
    /// Disc serials and volume fingerprints.
    Serials,
    /// Catalogue lookups and downloads.
    Catalogues,
}

/// How two members of a group are the same game. The derived order is the
/// strength order, strongest first: a group's relation is its weakest link,
/// the maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    /// Level 1: the same bytes, whatever the name, container or header.
    Exact,
    /// Level 3: the same disc in another format.
    Format,
    /// Level 2: the same game, another version.
    Version,
}

impl Relation {
    /// The weakest relation of a chain of links; `None` when empty.
    pub fn weakest(links: impl IntoIterator<Item = Relation>) -> Option<Relation> {
        links.into_iter().max()
    }
}

/// Why a group is only reported, with no file action (spec §7.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ReportWhy {
    /// Other versions, without "one game, one ROM".
    LevelTwoOff,
    /// Only a CRC matched; the full content was not confirmed.
    Unconfirmed,
    /// The payloads match but the headers differ (NES mappers).
    HeaderDiffers,
    /// The same serial, another build.
    BuildDiffers,
    /// A disc set with missing tracks or discs.
    PartialSet,
    /// Serials that disagree.
    SerialConflict,
    /// A modified program (hack, translation) sharing evidence.
    Modified,
    /// Arcade sets with no catalogue to tell clones apart.
    ArcadeNoDat,
    /// Too many members to act on safely.
    BigGroup { members: usize },
    /// You chose to skip this group.
    UserSkipped,
    /// CHDs with the same raw data but other metadata.
    ChdMetadataDiffers,
}

/// What happens to a group's extras.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum GroupAction {
    /// The extras are set aside.
    Act,
    /// Shown, never moved.
    ReportOnly { why: ReportWhy },
    /// Two or more members have real saves that differ (spec §8.3).
    Conflict { saves: Vec<SaveInfo> },
}

/// A unit as the plan names it: enough to show it and to check, before a
/// move, that it has not changed since the plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitRef {
    /// The file that stands for the unit (the cue, the zip, the ROM).
    pub primary: PathBuf,
    /// Every file of the unit, `primary` included.
    pub files: Vec<PathBuf>,
    pub kind: UnitKind,
    pub mtime_ns: i64,
    /// Already in a system folder (false for an Organise pile item).
    pub in_library: bool,
}

/// One copy in a group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    /// Index within the group.
    pub id: usize,
    pub unit: UnitRef,
    /// Relation to the keeper (`Exact` for the keeper itself).
    pub relation: Relation,
    pub size: u64,
    /// Files that leave with this copy when it is set aside.
    pub travels: Vec<Travel>,
    /// Shared files that stay with the keeper.
    pub stays: Vec<SharedStay>,
    /// Where the copy goes; `None` for the keeper and report-only groups.
    pub dest: Option<PathBuf>,
    pub catalogue_name: Option<String>,
    pub serials: Vec<String>,
    /// 0.0–1.0.
    pub confidence: f32,
}

/// Copies of one game inside one system.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DupGroup {
    /// Stable across runs: see [`group_id`].
    pub id: u64,
    pub system: String,
    /// Catalogue game name when known, else the keeper's stem.
    pub title: String,
    /// `Member::id` of the copy that stays.
    pub keeper: usize,
    pub reason: KeepReason,
    /// The member that came closest, and the rule it lost on.
    pub runner_up: Option<(usize, KeepReason)>,
    pub members: Vec<Member>,
    pub action: GroupAction,
    /// The keeper was chosen by the tie-breaker alone.
    pub tie: bool,
}

/// Counts for the summary. Conflicts and report-only groups are not in
/// `extras` or `bytes`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub groups: usize,
    /// Copies that would be set aside.
    pub extras: usize,
    /// Bytes that would leave the ROMs folder.
    pub bytes: u64,
    pub conflicts: usize,
    /// System → (groups, extras, bytes).
    pub by_system: BTreeMap<String, (usize, usize, u64)>,
}

/// A duplicates plan: what was found and what would move. Nothing moves
/// until it is applied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DedupePlan {
    /// [`PLAN_VERSION`].
    pub version: u32,
    /// Library root.
    pub root: PathBuf,
    pub roms_dir: PathBuf,
    pub quarantine: PathBuf,
    /// The quarantine shares the library's volume (moving frees nothing).
    pub same_volume: bool,
    /// The library is on a removable volume.
    pub removable: bool,
    pub profile_id: String,
    pub groups: Vec<DupGroup>,
    /// Ids of groups found before whose copies are all gone now.
    pub unchanged: Vec<u64>,
    pub not_roms: Vec<PathBuf>,
    pub unreadable: Vec<PathBuf>,
    /// Systems with no catalogue (no level 2).
    pub no_catalogue: Vec<String>,
    /// CHDs not read inside (no level 3, DD-2).
    pub chd_unread: usize,
    /// Systems whose saves may live off the card.
    pub off_card_saves: Vec<String>,
    pub totals: Totals,
}

impl Default for DedupePlan {
    fn default() -> Self {
        DedupePlan {
            version: PLAN_VERSION,
            root: PathBuf::new(),
            roms_dir: PathBuf::new(),
            quarantine: PathBuf::new(),
            same_volume: false,
            removable: false,
            profile_id: String::new(),
            groups: Vec::new(),
            unchanged: Vec::new(),
            not_roms: Vec::new(),
            unreadable: Vec::new(),
            no_catalogue: Vec::new(),
            chd_unread: 0,
            off_card_saves: Vec::new(),
            totals: Totals::default(),
        }
    }
}

/// Where extras go.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum QuarantineChoice {
    /// The quarantine folder next to the ROMs folder (spec §7.1).
    #[default]
    NextToRoms,
    /// A folder of your choice, anywhere.
    Folder { path: PathBuf },
    /// The OS Trash (opt-in, spec §7.2).
    Trash,
}

/// Options of one duplicates run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DedupeOptions {
    pub levels: Vec<DedupeLevel>,
    pub one_game_one_rom: bool,
    pub rules: Vec<RuleToggle>,
    /// As `settings.dedupe.container_order`.
    pub container_order: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// `None` = every system.
    pub systems: Option<Vec<String>>,
    /// No catalogue downloads.
    pub offline: bool,
    pub quarantine: QuarantineChoice,
    /// Copies that must stay, whatever the rules say.
    pub forced_keepers: Vec<PathBuf>,
    /// Forget remembered keeper choices.
    pub reset_choices: bool,
}

impl DedupeOptions {
    /// The options the settings ask for.
    pub fn from_settings(s: &DedupeSettings) -> Self {
        DedupeOptions {
            levels: s.levels.clone(),
            one_game_one_rom: s.one_game_one_rom,
            rules: s.keep_rules.clone(),
            container_order: s.container_order.clone(),
            systems: None,
            offline: false,
            quarantine: match (&s.action, &s.quarantine_dir) {
                (crate::settings::DedupeActionSetting::Trash, _) => QuarantineChoice::Trash,
                (_, Some(path)) => QuarantineChoice::Folder { path: path.clone() },
                (_, None) => QuarantineChoice::NextToRoms,
            },
            forced_keepers: Vec::new(),
            reset_choices: false,
        }
    }
}

impl Default for DedupeOptions {
    fn default() -> Self {
        DedupeOptions::from_settings(&DedupeSettings::default())
    }
}

/// The stable id of a group: a hash of its system and its sorted content
/// keys, so the same copies give the same id in every run, whatever order
/// they were found in. The system is part of it because groups never cross
/// systems: identical bytes in two system folders are two groups, and a
/// decision sent by id must reach only one. Kept within 53 bits so it
/// survives a JavaScript number.
pub fn group_id<S: AsRef<str>>(system: &str, keys: &[S]) -> u64 {
    let mut sorted: Vec<&str> = keys.iter().map(AsRef::as_ref).collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut h = Sha1::new();
    h.update(system.as_bytes());
    h.update([0u8]);
    for k in sorted {
        h.update(k.as_bytes());
        h.update([0u8]);
    }
    let d = h.finalize();
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[..8]);
    u64::from_be_bytes(b) & ((1u64 << 53) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identify::names::Region;
    use crate::profiles::Media;

    fn member(id: usize, relation: Relation) -> Member {
        Member {
            id,
            unit: UnitRef {
                primary: PathBuf::from(format!("PS/Crash Bash {id}.cue")),
                files: vec![
                    PathBuf::from(format!("PS/Crash Bash {id}.cue")),
                    PathBuf::from(format!("PS/Crash Bash {id}.bin")),
                ],
                kind: UnitKind::DiscSet,
                mtime_ns: 1_700_000_000_000_000_000,
                in_library: true,
            },
            relation,
            size: 512 * 1024 * 1024,
            travels: vec![
                Travel {
                    from: PathBuf::from("PS/Imgs/Crash Bash 1.png"),
                    kind: TravelKind::Art(Media::Cover),
                },
                Travel {
                    from: PathBuf::from("PS/._Crash Bash 1.cue"),
                    kind: TravelKind::AppleDouble,
                },
            ],
            stays: vec![SharedStay {
                path: PathBuf::from("PS/Crash Bash.srm"),
                with: 0,
            }],
            dest: Some(PathBuf::from("/Volumes/SD/_Duplicados/PS/Crash Bash 1.cue")),
            catalogue_name: Some("Crash Bash (USA)".into()),
            serials: vec!["SCUS-94570".into()],
            confidence: 0.97,
        }
    }

    fn sample_plan() -> DedupePlan {
        let save = SaveInfo {
            path: PathBuf::from("GB/Tetris.sav"),
            size: 8192,
            mtime_ns: 1,
            sha1: "da39a3ee5e6b4b0d3255bfef95601890afd80709".into(),
            keyed_by: SaveKey::Stem,
        };
        let group = |id, action, reason| DupGroup {
            id,
            system: "PS".into(),
            title: "Crash Bash".into(),
            keeper: 0,
            reason,
            runner_up: Some((
                1,
                KeepReason::Container {
                    kept: "pbp".into(),
                    other: "cue".into(),
                    profile: "anbernic-stock".into(),
                },
            )),
            members: vec![member(0, Relation::Exact), member(1, Relation::Format)],
            action,
            tie: false,
        };
        DedupePlan {
            root: PathBuf::from("/Volumes/SD"),
            roms_dir: PathBuf::from("/Volumes/SD/ROMS"),
            quarantine: PathBuf::from("/Volumes/SD/_Duplicados"),
            same_volume: true,
            removable: true,
            profile_id: "anbernic-stock".into(),
            groups: vec![
                group(
                    group_id("PS", &["a", "b"]),
                    GroupAction::Act,
                    KeepReason::Region {
                        kept: Region::World,
                        other: Region::Japan,
                    },
                ),
                group(
                    2,
                    GroupAction::ReportOnly {
                        why: ReportWhy::BigGroup { members: 60 },
                    },
                    KeepReason::Verified {
                        kept: DumpQuality::Headerless,
                        other: DumpQuality::Headered,
                    },
                ),
                group(
                    3,
                    GroupAction::Conflict { saves: vec![save] },
                    KeepReason::HasSave {
                        path: PathBuf::from("GB/Tetris.sav"),
                    },
                ),
            ],
            unchanged: vec![7],
            not_roms: vec![PathBuf::from("GB/index.html.zip")],
            unreadable: vec![PathBuf::from("GB/broken.zip")],
            no_catalogue: vec!["PICO8".into()],
            chd_unread: 24,
            off_card_saves: vec!["GB".into()],
            totals: Totals {
                groups: 3,
                extras: 1,
                bytes: 512 * 1024 * 1024,
                conflicts: 1,
                by_system: BTreeMap::from([("PS".into(), (1, 1, 512 * 1024 * 1024))]),
            },
            ..DedupePlan::default()
        }
    }

    #[test]
    fn relation_weakest_link_order() {
        assert!(Relation::Exact < Relation::Format);
        assert!(Relation::Format < Relation::Version);
        assert_eq!(
            Relation::weakest([Relation::Exact, Relation::Version, Relation::Format]),
            Some(Relation::Version)
        );
        assert_eq!(
            Relation::weakest([Relation::Exact, Relation::Format]),
            Some(Relation::Format)
        );
        assert_eq!(Relation::weakest([]), None);
    }

    #[test]
    fn plan_json_round_trips() {
        let plan = sample_plan();
        assert_eq!(plan.version, PLAN_VERSION);
        let json = serde_json::to_string(&plan).unwrap();
        let back: DedupePlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back, plan);
    }

    #[test]
    fn wire_tags_are_kebab_case() {
        let v = serde_json::to_value(sample_plan()).unwrap();
        let g = &v["groups"];
        assert_eq!(g[0]["action"]["kind"], "act");
        assert_eq!(g[0]["reason"]["kind"], "region");
        assert_eq!(g[0]["reason"]["kept"], "world");
        assert_eq!(g[1]["action"]["why"]["kind"], "big-group");
        assert_eq!(g[1]["action"]["why"]["members"], 60);
        assert_eq!(g[2]["reason"]["kind"], "has-save");
        assert_eq!(g[0]["members"][1]["relation"], "format");
        assert_eq!(g[0]["members"][0]["travels"][0]["kind"]["kind"], "art");
        assert_eq!(g[0]["members"][0]["travels"][0]["kind"]["media"], "cover");
        assert_eq!(g[0]["members"][0]["unit"]["kind"], "disc-set");
    }

    #[test]
    fn options_round_trip_and_follow_settings() {
        let mut s = DedupeSettings::default();
        let o = DedupeOptions::from_settings(&s);
        assert_eq!(o, DedupeOptions::default());
        assert_eq!(o.quarantine, QuarantineChoice::NextToRoms);
        assert_eq!(o.rules, s.keep_rules);
        s.quarantine_dir = Some(PathBuf::from("/Users/x/Dups"));
        assert_eq!(
            DedupeOptions::from_settings(&s).quarantine,
            QuarantineChoice::Folder {
                path: PathBuf::from("/Users/x/Dups")
            }
        );
        s.action = crate::settings::DedupeActionSetting::Trash;
        let o = DedupeOptions::from_settings(&s);
        assert_eq!(o.quarantine, QuarantineChoice::Trash);
        let back: DedupeOptions =
            serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
        assert_eq!(back, o);
        // Missing fields take the defaults.
        let partial: DedupeOptions = serde_json::from_str(r#"{"offline":true}"#).unwrap();
        assert!(partial.offline);
        assert_eq!(partial.levels, DedupeSettings::default().levels);
    }

    #[test]
    fn group_id_is_stable_and_order_free() {
        let a = group_id("GB", &["sha1:aa", "sha1:bb"]);
        assert_eq!(a, group_id("GB", &["sha1:bb", "sha1:aa"]));
        assert_eq!(a, group_id("GB", &["sha1:bb", "sha1:aa", "sha1:aa"]));
        assert_ne!(a, group_id("GB", &["sha1:aa", "sha1:cc"]));
        assert!(a < (1u64 << 53));
    }

    #[test]
    fn group_id_never_crosses_systems() {
        // The same bytes grouped in GB and in GBC are two groups; a pick or
        // a skip on one must never reach the other.
        let keys = ["sha1:aa", "sha1:bb"];
        assert_ne!(group_id("GB", &keys), group_id("GBC", &keys));
        // The system cannot bleed into the first key.
        assert_ne!(group_id("GB", &["Csha1:aa"]), group_id("GBC", &["sha1:aa"]));
    }
}
