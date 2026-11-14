//! Organise plan types (organise spec §11) and the planner: the survey of
//! a pile (walk, disc sets, identification on a few worker threads,
//! memoised facts, the duplicate bucket), the plan built from a survey
//! with the user's decisions (including the duplicates step against the
//! target's system folders), and the cheap name-only folder probe.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::addons::{AddonFormat, AddonKind, AddonLink, HeaderFit, UnlinkedWhy};
use crate::archive::names::UnsafeWhy;
use crate::archive::{ContainerKind, Limits, Tool, ToolsOnDemand};
use crate::cache::Cache;
use crate::consoles::{base_title_id, link_kind, PartKind, TitleKind};
use crate::dedupe::archive::ArchiveEntryListing;
use crate::dedupe::disc::ReaderDiscProbe;
use crate::dedupe::source::DiscId;
use crate::dedupe::source::{CacheDats, DiscProbe, EntryListing, ListedEntry};
use crate::dedupe::units::{
    units_from_facts, units_from_folders, units_from_scan, units_from_sets, UnitOptions,
};
use crate::dedupe::{
    group_units, DedupeOptions, DupGroup, GroupAction, GroupEnv, KeepReason, QuarantineChoice,
    Relation, ReportWhy, DEFAULT_QUARANTINE_NAME,
};
use crate::game_folder::{ChildRole, FixedFolder};
use crate::identify::dat::DatStore;
use crate::identify::evidence::{Evidence, IdLevel};
use crate::identify::hash::HeaderRule;
use crate::identify::names::{bare_region, parse_tags, Region};
use crate::identify::sniff::{NotARom, Sniff};
use crate::organize::apply::{cluster_size, large_transfer, user_available};
use crate::organize::discsets::{self, HeaderDisc, SetEvidence};
use crate::organize::evidence::Combined;
use crate::organize::evidence::{
    identify_candidate, DatCatalogue, Facts, Identification, LadderEnv, OnlineLookup, Thresholds,
    Verdict, MAX_DAT_DOWNLOADS,
};
use crate::organize::inspect::{
    inspect_file_with, inspect_set_primary, is_metadata_entry, ArchiveGame, ArchiveIssue,
    Container, Inspection, Payload, ZipEntryInfo, NOTE_TOOL_FAILED,
};
use crate::organize::library_bases::{
    demo_free_key, has_demo_tag, library_bases, pick_by_region, single_name_region, LibraryIndex,
    LookupBudget,
};
use crate::organize::target::{
    assign_targets_in, attach_set_aside_companions, child_art_companions, default_companion_exts,
    dlc_target, file_system_is_fat32, folder_dir, folder_gate, game_children_follow,
    game_children_targets, game_folder_name, is_game_folder_system, licence_companions,
    limit_fat_folders, overlay_pairs, sibling_targets, siblings_follow, windows_paths, DirIndex,
    ExtractShape, Subdir, TargetLayout,
};
use crate::organize::walk::{
    is_broad_root, walk, MetaProbe, TreeSummary, WalkCandidate, WalkOptions, Walked, MAX_DEPTH,
};
use crate::profiles::{detect, Profile};
use crate::scan::classify::{
    extension, is_appledouble, is_hidden, is_junk_file, is_partial_download, is_zone_identifier,
};
use crate::scan::multifile::{group_sets, is_list_extension};
use crate::scan::{find_dir_ignoring_case, scan_systems, ScanOptions, ScanWarning};
use crate::settings::{ExternalTools, OrganizeDedupeMode, OrganizeModeSetting, Settings};
use crate::systems::{System, SystemDb};
use crate::translate::{EmulatorChoice, EmulatorId, NotSupportedWhy, TranslatedGameView};

/// How files reach their system folder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferMode {
    /// Rename in place (same volume) or copy, verify, then delete.
    #[default]
    Move,
    /// Leave the source untouched.
    Copy,
}

/// Whether organised files keep their name or take the catalogue name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NameMode {
    #[default]
    Keep,
    Catalogue,
}

/// What one plan item moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKind {
    /// A raw ROM file.
    File,
    /// A zip/7z holding one game.
    Archive,
    /// Several files that travel together (`.cue` + `.bin`, `.m3u`, `.gdi`).
    Set,
    /// A game that is a folder (systems batch 2 D12): moved whole, its
    /// `size` the metadata sum of its files and its one fingerprint the
    /// tree's total bytes and newest mtime (D25, D27).
    Folder,
}

/// Why a file stays where it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LeftReason {
    /// Still being downloaded (`.crdownload`, `.part` sibling, zero bytes).
    PartialDownload,
    /// Modified within the last `recent` seconds: may still be written.
    RecentlyModified,
    /// A cloud placeholder whose bytes are not on this disk.
    CloudOnly,
    /// A web page or text file with a ROM name.
    NotARom {
        what: NotARom,
    },
    /// BIOS or firmware: moving it into a game folder makes it a fake game.
    Bios,
    DamagedArchive,
    /// A container this build cannot open; `need` names the external
    /// tool that would (fields absent in plans from before archives).
    UnsupportedArchive {
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        need: Option<Tool>,
    },
    /// The archive's header or every payload is password protected.
    EncryptedArchive,
    /// Decoding needs more memory than the cap (S2, S3).
    ArchiveNeedsMemory {
        mib: u64,
    },
    /// Too many entries or too many declared bytes to open safely; never
    /// damage (D1).
    ArchiveTooBig {
        entries: Option<u64>,
        bytes: Option<u64>,
    },
    /// One part of a multi-volume archive.
    MultiVolumeArchive {
        parts: usize,
    },
    /// A needed entry has an unsafe path (zip-slip, device name, link…).
    UnsafeArchiveEntry {
        name: String,
        why: UnsafeWhy,
    },
    /// A set member's name the target cannot store unchanged (sheets
    /// reference it, so it is never renamed).
    SetUnsafeName {
        name: String,
    },
    /// A target or temp path over the Windows limit.
    PathTooLong,
    /// The only payload is another archive.
    NestedArchive,
    /// An arcade set in a container other than zip.
    RepackArcade,
    /// The FAT32 folder has no directory slots left.
    FolderFull {
        folder: String,
    },
    /// The target folder is a symlink or junction.
    UnsafeTarget,
    /// An archive holding several games.
    MultiGameArchive {
        count: usize,
    },
    /// A system Coverly recognises but does not organise.
    UnsupportedSystem {
        name: String,
    },
    /// ROM-like, but no level could place it.
    Unknown,
    /// Same content as another item of the plan. Plans from before the
    /// duplicates step only: no longer produced (pile copies are set aside
    /// or reported as [`LeftReason::InLibrary`]).
    Duplicate {
        of: PathBuf,
    },
    /// The game is already in the library (or will be, as another pile
    /// copy): `path` is that copy, relative to the target root (a pile
    /// copy that stays on the pile: relative to the source root), and
    /// `relation` how the two are the same game. Reported, not set aside
    /// (the duplicates step only reports, or the group may not act).
    InLibrary {
        path: PathBuf,
        relation: Relation,
    },
    /// The same content already sits in the target folder.
    AlreadyPresent {
        at: PathBuf,
    },
    /// Already inside a folder that resolves to a system.
    AlreadySorted {
        folder: String,
    },
    /// A game that is a folder (Doom WAD, Solarus, Lutro, ScummVM): left
    /// as one item; move the whole folder to `folder`, the system's folder
    /// under the target layout (spec D15).
    FolderGame {
        system: String,
        folder: String,
    },
    /// Symlinks, junctions and other reparse points are never followed.
    Symlink,
    /// A disc set whose listed files are missing.
    SetIncomplete {
        missing: Vec<String>,
    },
    /// A disc set that lists files outside its folder.
    SetOutsideFolder,
    /// A track referenced by more than one disc set.
    SharedTrack,
    /// A companion whose stem matches several ROMs.
    AmbiguousCompanion,
    /// Larger than 4 GiB on a FAT32 target.
    TooBigForFat,
    Unreadable,
    /// One file of a set collides at the target: the whole set stays.
    CollisionSet,
    /// A console key or licence file (systems batch 2 D4): never read,
    /// never moved on its own.
    KeyMaterial,
    /// A licence that goes with its game as a companion (game folders
    /// spec D17): never opened, moved only by a same-volume rename when
    /// `of` (relative to the source root) is applied; not moved on its
    /// own.
    LicenceOf {
        of: PathBuf,
    },
    /// Inside an emulator's own data tree (`ux0`, `dev_hdd0`, `mlc01`,
    /// `sdmc/Nintendo 3DS`, `nand`): moving it breaks the install.
    /// `folder` is the root's name as on disk.
    EmulatorData {
        folder: String,
    },
    /// A shadPS4 `-mods`/`-UPDATE`/`-patch` folder beside its base that
    /// cannot move alone (systems batch 3 D11 d, e): `of` is the base's
    /// title, or its folder name when the base is not an item (plan P-3).
    GameOverlay {
        of: String,
    },
    /// A piece of a game that cannot play alone (spec D13): an ExeFS
    /// file, a loose NCA, a library, title metadata, a module or a
    /// backup. `title_id` is the game's, when the piece or its path says.
    GamePart {
        part: PartKind,
        system: String,
        #[serde(default)]
        title_id: Option<String>,
    },
    /// A huge item the user said «No» to (spec D16): it stays where it is.
    NotConfirmed,
    /// A file inside a fixed folder of a game folder (`Updates/`, `DLC/`,
    /// `Mods/`…; game folders spec D3, §8.1): it belongs to its game and
    /// stays for now. `of` is the game folder's name.
    GameFolderContent {
        folder: FixedFolder,
        of: String,
    },
    /// An addon whose game is not known (mods spec D7): never moved,
    /// never deleted. On the wire the addon kind is `addon` (`kind` is
    /// the tag).
    AddonUnlinked {
        #[serde(rename = "addon")]
        kind: AddonKind,
        format: AddonFormat,
        why: UnlinkedWhy,
    },
    /// A file of that name, with other content, is already in the
    /// addon's fixed folder (D11): `at` is that folder, relative to the
    /// target root. Never renamed to ` (2)`.
    AddonNameTaken {
        at: PathBuf,
    },
    /// A linked addon the layout has no place for (D12).
    AddonNoPlace {
        #[serde(rename = "addon")]
        kind: AddonKind,
    },
    /// The addon goes in an emulator's own data folder, which the run did
    /// not name (D16): `path` is where, relative to that folder.
    /// `emulator` is the product name.
    AddonForEmulator {
        emulator: String,
        path: PathBuf,
    },
    /// The chosen emulator does not use the item like this (D15).
    NotSupported {
        emulator: String,
        why: NotSupportedWhy,
    },
    /// A patch the emulator would apply when playing, only renamed for it
    /// when the user chooses that emulator (D16, Q11).
    PatchNotApplied {
        emulator: String,
    },
}

/// The reason an inspection issue leaves an item in place (archives spec
/// §4.2): size caps are never damage, and a spent budget is no issue at all
/// (the item goes to Review with `ItemNote::ArchiveNotFullyRead`).
pub fn left_reason_for(issue: &ArchiveIssue) -> LeftReason {
    match issue {
        ArchiveIssue::Damaged => LeftReason::DamagedArchive,
        ArchiveIssue::Unsupported { format, need } => LeftReason::UnsupportedArchive {
            format: format.clone(),
            need: *need,
        },
        ArchiveIssue::Encrypted => LeftReason::EncryptedArchive,
        ArchiveIssue::NeedsMemory { mib } => LeftReason::ArchiveNeedsMemory { mib: *mib },
        ArchiveIssue::TooBig { entries, bytes } => LeftReason::ArchiveTooBig {
            entries: *entries,
            bytes: *bytes,
        },
        ArchiveIssue::MultiVolume { parts } => LeftReason::MultiVolumeArchive { parts: *parts },
        ArchiveIssue::Nested => LeftReason::NestedArchive,
        ArchiveIssue::NotARom { what } => LeftReason::NotARom { what: *what },
        ArchiveIssue::TooManyRoms { count } | ArchiveIssue::SeveralGames { count } => {
            LeftReason::MultiGameArchive { count: *count }
        }
        ArchiveIssue::UnsafeEntry { name, why } => LeftReason::UnsafeArchiveEntry {
            name: name.clone(),
            why: *why,
        },
        ArchiveIssue::SetIncomplete { missing } => LeftReason::SetIncomplete {
            missing: missing.clone(),
        },
        ArchiveIssue::SetOutsideFolder => LeftReason::SetOutsideFolder,
        ArchiveIssue::SharedTrack => LeftReason::SharedTrack,
    }
}

/// Something worth telling about an item that still moves (data for the
/// UI, never prose).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ItemNote {
    /// A zip with several ROMs of one system: most cores load the first.
    MultiRomZip { count: usize },
    /// The target folder does not exist on the chosen device layout.
    NoFolderOnDevice { folder: String },
    /// The extension says one system, the contents do not look like it.
    /// `system` is the extension's owner: the item's own system when its
    /// header is missing, another one when a weak owner's header won
    /// (spec D24). Absent in plans written before it.
    ExtensionContradicted {
        ext: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system: Option<String>,
    },
    /// The system's DAT could not be consulted (offline, cap, none).
    CatalogueMissing { system: String },
    /// An archive entry is encrypted: its header was not read.
    Encrypted,
    /// The magic says another container than the extension (`actual` is
    /// the format's short name, `7z`).
    ContainerMismatch { ext: String, actual: String },
    /// A `.gdi` and a `.cue` for the same tracks: both are extracted.
    TwinSheets { kept: Vec<String> },
    /// The format stores no CRC: only the size is checked.
    NoStoredChecksum,
    /// The decode budget ran out during the dry run.
    ArchiveNotFullyRead,
    /// Move mode keeps the archive: it holds files that are not extracted.
    ArchiveKeptLeftovers { count: usize, examples: Vec<String> },
    /// The sheet sits beyond the plan-time budget: checked on apply.
    SheetNotReadYet,
    /// Several ROMs of one system are extracted from one archive.
    MultiRomExtract { count: usize },
    /// An update of `of` (its title, or its title id when the base is not
    /// in the plan).
    Update { of: String },
    /// Downloadable content of `of` (as for [`ItemNote::Update`]).
    Dlc { of: String },
    /// The header says the content is encrypted; `emulator` only opens
    /// decrypted copies (a fact about the emulator, systems batch 2 D4).
    EncryptedContent { emulator: String },
    /// A package the emulator installs (PS3/Vita PKG).
    InstallFirst { emulator: String },
    /// The folder is renamed to `name` so `layout` shows it (systems
    /// batch 2 D13).
    FolderSuffix { name: String, layout: String },
    /// A PS4 PKG: catalogued, has to be extracted to play (systems
    /// batch 3 D9).
    ExtractFirst,
    /// A PS5 store PKG, `.exfat`, `.ffpfs`, `.ffpfsc`, or an XVC (systems
    /// batch 3 D10, D19): no emulator opens it in this form.
    NoEmulator,
    /// PS5 `attribute3` bit 22 (KytyPS5 `title_supports_trinity`).
    ProEnhanced,
    /// The item goes to the DLC folder of `layout` (the profile's display
    /// name), where it looks for downloadable content (systems batch 3
    /// D16).
    DlcFolder { layout: String },
    /// Homebrew: goes to the system folder like any ROM (spec D13, §13 Q6).
    Homebrew,
    /// A Vita game kept in a 7z: Vita3K only installs `.zip`, `.vpk` or
    /// `.vci` (spec D23).
    Vita7z,
    /// Disc `n` of a multi-disc set is not in the pile (spec D24).
    MissingDisc { n: u8 },
    /// Loose discs whose names alone say they are one game (spec D24):
    /// grouped for review, never moved together automatically.
    DiscSet,
    /// The title could not be read without keys and the catalogue did not
    /// name it: shown as «Cifrado» (spec D11).
    EncryptedName,
    /// The archive holds the folders of `count` games (spec D21): review.
    ArchiveGames { count: usize },
    /// An `sce_sys/param.sfo` game whose SFO was not read within budget:
    /// PS4 and Vita share the structure (spec D22), so review.
    Ps4OrVita,
    /// Two names in the archive differ only in case: extracting them
    /// collides on macOS and Windows (spec D20), so review.
    ArchiveCaseCollision,
    /// A demo of `of`, linked by name only (game folders spec D12): it
    /// becomes its own game.
    DemoOf { of: String },
    /// An update or DLC whose game is neither in the plan nor in the
    /// library (game folders spec D13): review.
    BaseMissing,
    /// A mod of `of` (its title), linked by its key (mods spec D10).
    ModOf { of: String },
    /// A translation patch or hack of `of`.
    PatchOf { of: String },
    /// Cheats or emulator patches of `of`.
    CheatOf { of: String },
    /// The patch expects the ROM with or without its header (D8).
    HeaderMismatch { needs: HeaderFit },
    /// The emulator (product name) has to be pointed at `folder` (D15).
    PointEmulatorHere { emulator: String, folder: String },
    /// A 3DS product code ends in a region letter the verified table does
    /// not know: no region is added to the name (part C §5, P-1).
    RegionLetterUnknown { letter: char },
    /// `count` art files of this update or DLC (its slots beside it and
    /// their origin file) stay where they are: the layout shows no child
    /// art, or their name is taken or too long at the target (part C
    /// D10 d, D16).
    ChildArtStays { count: usize },
}

/// How an item reaches its target (archives spec §8.1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Transfer {
    /// The file moves or is copied as it is.
    #[default]
    AsIs,
    /// The listed entries are extracted; `m3u` is a generated playlist
    /// (relative to the target root).
    Extract {
        entries: Vec<ExtractEntry>,
        bytes: u64,
        archive_after: ArchiveAfter,
        #[serde(default)]
        source: ExtractSource,
        #[serde(default)]
        sheet_check: SheetCheck,
        #[serde(default)]
        m3u: Option<PathBuf>,
        /// A folder game extracted out of an archive (adjustments spec
        /// D23): always asked one by one before apply (D15). Plans from
        /// before the adjustments have none.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        folder_game: bool,
    },
}

/// One archive entry an `Extract` item writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractEntry {
    /// Position in the archive's own order.
    pub index: usize,
    /// The validated entry path (`RelPath`), `/`-separated.
    pub name: String,
    pub size: u64,
    /// Stored CRC32 when the format has one.
    pub crc32: Option<u32>,
    /// Relative to the target root.
    pub target: PathBuf,
}

/// What happens to the archive once every entry is verified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArchiveAfter {
    /// Copy mode, or move mode with leftovers.
    Keep,
    /// Move mode with nothing left behind: sent to the Trash.
    Trash,
}

/// Who decodes the archive on apply: the same as at plan time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExtractSource {
    #[default]
    Builtin,
    /// An external tool, by kind and the version it reported.
    Tool { tool: Tool, version: String },
}

/// Whether a disc set's sheet was parsed at plan time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SheetCheck {
    #[default]
    Read,
    /// Inferred from the listing; parsed and checked on apply.
    Deferred,
}

/// Why an item's target name differs from its source name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RenameReason {
    /// `name` was taken in the target folder (on disk or by another
    /// item): the item got the next free ` (n)` name.
    Collision { name: String },
    /// Characters FAT/exFAT cannot store were replaced (`from` is the
    /// name before).
    SafeName { from: String },
    /// The stem was shortened so the full path fits Windows' 259 units.
    PathLength,
    /// Renamed to the DAT game name (*Names: catalogue*).
    Catalogue { from: String },
    /// Renamed to its game folder's name (`<Game>.<ext>`, `<Game>
    /// (vX).<ext>`, `<Game> - <DLC name>.<ext>`; game folders spec §7).
    GameFolder { from: String },
}

/// A save, patch or note that follows its ROM.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Companion {
    /// Relative to the source root.
    pub source: PathBuf,
    /// Relative to the target root; for a set-aside item, where it goes
    /// below the quarantine, as the item's `dest`.
    pub target: PathBuf,
    /// A licence recognised by its name only (game folders spec D17):
    /// never opened, it follows by a same-volume rename or stays.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub licence: bool,
    /// An art slot or origin file of an update or DLC (part C D10 d): it
    /// keeps the child's new stem, and a name taken at its target leaves
    /// it where it is (never a numbered name, never replaced).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub child_art: bool,
}

/// What an item is to its game (game folders spec D16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GameRole {
    Base,
    Update,
    Dlc,
    Demo,
    /// A translation patch, hack, mod or texture pack (mods spec D10).
    Mod,
    /// A cheat or an emulator patch.
    Cheat,
}

/// How an item belongs to a game folder (game folders spec D16): data
/// for the UI, never prose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameLink {
    pub role: GameRole,
    /// Display form (D11), never the raw header value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<crate::identify::names::Region>,
    /// The base item's id in this plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<usize>,
    /// The game folder, relative to the target root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// The game folder was already on disk (plan P-5).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folder_exists: bool,
    /// A demo's base title, linked by name only (D12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub demo_of: Option<String>,
    /// Highest version present for this base (D24).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub newest_here: bool,
    /// What a `Mod` or `Cheat` row is and how it was linked (mods spec
    /// D10); absent in plans made before addons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub addon: Option<AddonLink>,
}

impl GameLink {
    /// A link of `role` with nothing else known yet.
    pub fn new(role: GameRole) -> GameLink {
        GameLink {
            role,
            version: None,
            region: None,
            of: None,
            folder: None,
            folder_exists: false,
            demo_of: None,
            newest_here: false,
            addon: None,
        }
    }
}

/// Size and mtime of a source when it was planned: apply skips items
/// whose source changed since.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFp {
    pub size: u64,
    pub mtime_ns: i64,
}

/// What the plan proposes for an item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum OrganizeAction {
    /// Moved without asking.
    Auto,
    /// Moved once the user confirms the system.
    Review,
    /// Stays where it is.
    Left { reason: LeftReason },
    /// Another copy of a game the library holds, or of a pile copy that
    /// stays (duplicates spec §4, §7.1): moved to the pile's own
    /// quarantine (or the Trash), never copied into the library.
    SetAside {
        /// The copy that stays, relative to the target root (the library
        /// copy, or where the pile copy that stays is organised to); a
        /// pile copy that stays on the pile: relative to the source root.
        keeper: PathBuf,
        /// How this copy is the same game as the keeper.
        relation: Relation,
        /// Why the keeper stays.
        reason: KeepReason,
        /// Where the primary file goes, relative to the source root
        /// (`_Duplicados/<its path>`), or absolute in the library's
        /// quarantine for a pile inside a system folder
        /// (`<target>/_Duplicados/GB/<pile>/<its path>`); the other files
        /// of a set keep their own paths below the same folder.
        dest: PathBuf,
    },
}

/// A `.m3u` written for a loose multi-disc set (adjustments spec D24):
/// a new file, never over an existing one, journaled and undone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    /// Relative to the target root.
    pub path: PathBuf,
    /// Each disc's sheet or image, relative to the target root, in disc
    /// order: the playlist's lines.
    pub discs: Vec<PathBuf>,
}

/// One game of the plan: a file, an archive or a disc set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrganizeItem {
    pub id: usize,
    /// Relative to the source root; the primary file first.
    pub sources: Vec<PathBuf>,
    pub kind: ItemKind,
    pub size: u64,
    /// One per source, for re-validation at apply.
    pub fps: Vec<SourceFp>,
    /// Relative to the target root (the primary file).
    pub target: Option<PathBuf>,
    /// The other members of a set, in the order of `sources[1..]`.
    pub set_targets: Vec<PathBuf>,
    pub system_id: Option<String>,
    /// The system folder name actually used (an existing folder keeps
    /// its casing).
    pub folder: Option<String>,
    pub confidence: f32,
    pub method: Option<IdLevel>,
    pub why: Vec<Evidence>,
    pub alternatives: Vec<(String, f32)>,
    pub rename: Option<RenameReason>,
    pub notes: Vec<ItemNote>,
    pub companions: Vec<Companion>,
    pub catalogue_name: Option<String>,
    /// As-is (default, plans from before archives) or extract.
    #[serde(default)]
    pub transfer: Transfer,
    /// The disc's serials and fingerprint, for the UI and dedupe level 3.
    #[serde(default)]
    pub disc: Option<DiscId>,
    /// A console title id and where it was read (systems batch 2 §10),
    /// for the row's «why» line; left out of the JSON when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_id: Option<crate::consoles::TitleIdRef>,
    /// The console content id (PKG header, `param.sfo`, `param.json`,
    /// XVC), for the row's «why» line (systems batch 3 §10); left out of
    /// the JSON when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
    /// The playlist a loose multi-disc set gets (spec D24); left out of
    /// the JSON when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playlist: Option<Playlist>,
    /// Set only when the optional Switch catalogue named the title
    /// (`titledb` or `nlib`, spec D7), for the row's «why» line; a
    /// header or file-name title says so through its own lines, so it is
    /// left out of the JSON then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_source: Option<crate::consoles::TitleSource>,
    /// What the item is to its game folder (game folders spec D16); left
    /// out of the JSON when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<GameLink>,
    #[serde(flatten)]
    pub action: OrganizeAction,
}

impl OrganizeItem {
    /// The item stays in place.
    pub fn is_left(&self) -> bool {
        matches!(self.action, OrganizeAction::Left { .. })
    }

    /// The item goes into the library (automatically or once reviewed).
    pub fn moves_in(&self) -> bool {
        matches!(self.action, OrganizeAction::Auto | OrganizeAction::Review)
    }

    /// The item is set aside as a duplicate.
    pub fn is_set_aside(&self) -> bool {
        matches!(self.action, OrganizeAction::SetAside { .. })
    }

    /// Leaves the item in place with `reason` and drops its targets.
    pub fn leave(&mut self, reason: LeftReason) {
        self.action = OrganizeAction::Left { reason };
        self.target = None;
        self.set_targets.clear();
        self.companions.clear();
        self.rename = None;
        self.transfer = Transfer::AsIs;
        self.playlist = None;
    }
}

// --- The planner (organise spec §6.7, §7, §11) ---

/// Layout used when none is chosen: the owner's Anbernic stock card.
pub const DEFAULT_LAYOUT: &str = "anbernic-stock";

/// Entries [`probe_folder`] looks at before it stops (`truncated`).
pub const PROBE_LIMIT: usize = 5_000;

/// Full-file hashes the ladder ran (test builds only): the perf gate
/// checks that headers alone decide a big clean pile.
#[cfg(any(test, feature = "test-util"))]
pub static HASH_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Worker threads identifying candidates on internal disks; removable and
/// network volumes get [`SLOW_VOLUME_WORKERS`] (plan O-D2).
const WORKERS: usize = 4;
const SLOW_VOLUME_WORKERS: usize = 2;
/// Identify progress is reported at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Knobs of one plan.
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizeOptions {
    /// Profile id of the target layout.
    pub layout: String,
    /// Where the system folders go; `None` = the picked folder.
    pub target: Option<PathBuf>,
    /// *Directly here*: system folders straight in the target, without
    /// the layout's ROMs folder.
    pub flat: bool,
    /// `None` = move on the same volume, copy across volumes.
    pub mode: Option<TransferMode>,
    pub names: NameMode,
    /// Cached DATs only; no download, no online lookup.
    pub offline: bool,
    pub thresholds: Thresholds,
    /// Lower-case extensions that follow their ROM (`state*` = prefix).
    pub companion_exts: Vec<String>,
    /// System id → folder name overrides for the chosen layout.
    pub folder_names: BTreeMap<String, String>,
    /// Files modified more recently than this may still be written.
    pub recent_secs: u64,
    /// Overrides the layout's arcade preference order.
    pub arcade_order: Option<Vec<String>>,
    /// Download missing DATs (up to [`MAX_DAT_DOWNLOADS`] per plan).
    pub download_dats: bool,
    /// Bounds for every archive the plan opens (`external_tools` follows
    /// the setting `organize.external_tools`).
    pub limits: Limits,
    /// What the duplicates step does (`dedupe.organize_step`).
    pub duplicates: OrganizeDedupeMode,
    /// Keeper rules, levels and container order of the duplicates step;
    /// `quarantine: Trash` sends set-aside copies to the Trash.
    pub dedupe: DedupeOptions,
    /// The quarantine folder's name (`dedupe.quarantine_name`): set-aside
    /// pile copies go to `<source>/<name>`, and walks skip it.
    pub quarantine_name: String,
    /// `settings.matching.regions`: the keeper rules' region order.
    pub regions: Vec<String>,
    /// Game folders for this run (game folders spec D1): `None` = the
    /// layout profile's own value; the app passes the user's override
    /// (`organize.game_folders`) resolved, the CLI its flag.
    pub game_folders: Option<bool>,
    /// The emulator chosen per system for this run (mods spec D16): a
    /// system absent here uses the layout's default.
    pub emulators: BTreeMap<String, EmulatorChoice>,
    /// Emulator data folders the run may write into (CLI only, D16):
    /// absolute.
    pub emulator_data: BTreeMap<EmulatorId, PathBuf>,
}

impl Default for OrganizeOptions {
    fn default() -> Self {
        OrganizeOptions {
            layout: DEFAULT_LAYOUT.to_string(),
            target: None,
            flat: false,
            mode: None,
            names: NameMode::Keep,
            offline: false,
            thresholds: Thresholds::default(),
            companion_exts: default_companion_exts(),
            folder_names: BTreeMap::new(),
            recent_secs: 60,
            arcade_order: None,
            download_dats: true,
            limits: Limits::default(),
            duplicates: OrganizeDedupeMode::SetAside,
            dedupe: DedupeOptions::default(),
            quarantine_name: DEFAULT_QUARANTINE_NAME.to_string(),
            regions: Vec::new(),
            game_folders: None,
            emulators: BTreeMap::new(),
            emulator_data: BTreeMap::new(),
        }
    }
}

impl OrganizeOptions {
    /// The options `settings.organize` asks for; `layout` (a per-run
    /// choice) replaces `default_layout`, and the folder names and arcade
    /// order are the ones stored for the chosen layout. `target` and
    /// `offline` are per run and stay at their defaults.
    pub fn from_settings(settings: &Settings, layout: Option<&str>) -> OrganizeOptions {
        let o = &settings.organize;
        let layout = layout
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .or_else(|| Some(o.default_layout.trim()).filter(|l| !l.is_empty()))
            .unwrap_or(DEFAULT_LAYOUT)
            .to_string();
        OrganizeOptions {
            flat: !o.nest_in_roms_root,
            mode: match o.mode {
                OrganizeModeSetting::Auto => None,
                OrganizeModeSetting::Move => Some(TransferMode::Move),
                OrganizeModeSetting::Copy => Some(TransferMode::Copy),
            },
            names: o.names,
            thresholds: Thresholds {
                auto: o.auto_threshold,
                review: o.review_threshold,
            },
            companion_exts: o.companion_exts.clone(),
            folder_names: o.folder_names.get(&layout).cloned().unwrap_or_default(),
            recent_secs: u64::from(o.recent_seconds),
            arcade_order: o
                .arcade_order
                .get(&layout)
                .filter(|order| !order.is_empty())
                .cloned(),
            download_dats: o.download_dats,
            limits: Limits {
                external_tools: o.external_tools == ExternalTools::Auto,
                ..Limits::default()
            },
            duplicates: settings.dedupe.organize_step,
            regions: settings.matching.regions.clone(),
            dedupe: DedupeOptions::from_settings(&settings.dedupe),
            quarantine_name: settings
                .dedupe
                .quarantine_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .unwrap_or(DEFAULT_QUARANTINE_NAME)
                .to_string(),
            layout,
            ..OrganizeOptions::default()
        }
    }
}

/// Everything the planner consults besides the pile and the options.
#[derive(Clone, Copy)]
pub struct OrganizeEnv<'a> {
    pub db: &'a SystemDb,
    /// Built-in and custom profiles the layout is chosen from.
    pub profiles: &'a [Profile],
    pub cache: &'a Cache,
    /// Downloads missing DATs; `None` = cached DATs only.
    pub dats: Option<&'a DatStore<'a>>,
    pub probe: &'a dyn MetaProbe,
    pub online: Option<&'a dyn OnlineLookup>,
    /// The optional Switch name catalogue (adjustments plan P-1): local,
    /// synchronous lookups only. `None` = no catalogue (tests, `--offline`,
    /// setting off); callers download it beforehand (P-2).
    pub titles: Option<&'a dyn crate::catalog::TitleNames>,
    pub now: SystemTime,
}

/// Which part of planning is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrganizePhase {
    Walk,
    Identify,
    Dats,
    Hash,
    /// Moving or copying the items (`organize::apply`).
    Apply,
    /// Extracting archive item `item` (its [`OrganizeItem::id`]):
    /// `bytes_done` of the `bytes_total` its decode pass goes through (a
    /// solid block's other entries included). The first event of an item
    /// has `bytes_done == 0` and its last `bytes_done == bytes_total`.
    Extract {
        item: usize,
        bytes_done: u64,
        bytes_total: u64,
        /// Entries of the item staged so far (written, re-read, CRC equal),
        /// in ascending archive index; adopted entries count.
        entries_done: usize,
        /// The item's entries (`Transfer::Extract::entries.len()`; a
        /// generated playlist is not one).
        entries_total: usize,
    },
    /// Copying the files of folder item `item` (a game that is a folder,
    /// spec D25): `bytes_done` of its `bytes_total` (the item's `size`).
    /// The first event of an item has `bytes_done == 0` and the last
    /// `bytes_done == bytes_total`; a move by one rename sends none.
    Folder {
        item: usize,
        bytes_done: u64,
        bytes_total: u64,
    },
}

/// Planning progress: `done` of `total` in `phase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizeProgress {
    pub phase: OrganizePhase,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum OrganizeError {
    #[error("source not found: {0}")]
    SourceMissing(PathBuf),
    #[error("cannot read {0}: {1}")]
    Unreadable(PathBuf, String),
    #[error("unknown layout `{0}`")]
    UnknownLayout(String),
    #[error("unknown system `{0}`")]
    UnknownSystem(String),
    #[error("cancelled")]
    Cancelled,
    #[error("item {0} is not in the plan")]
    UnknownItem(usize),
    /// The run's emulator data folder for `emulator` cannot be written
    /// into (mods spec D16).
    #[error("emulator data folder for {emulator} refused ({why}): {}", path.display())]
    EmulatorData {
        emulator: EmulatorId,
        path: PathBuf,
        why: super::translate_plan::EmulatorDataWhy,
    },
}

/// The user's answer for one item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub item: usize,
    #[serde(flatten)]
    pub choice: Choice,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "choice", rename_all = "kebab-case")]
pub enum Choice {
    /// Move to the proposed system.
    Accept,
    /// Move to this system (remembered by content).
    Assign { system: String },
    /// Keep it where it is.
    Leave,
    /// «No» to a huge item (spec D16): kept where it is with
    /// [`LeftReason::NotConfirmed`]; never remembered by content.
    Decline,
}

/// Totals of a plan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    /// Items that move (automatically or once reviewed).
    pub roms: usize,
    /// Systems those items go to.
    pub systems: usize,
    pub auto: usize,
    pub review: usize,
    pub left: usize,
    pub junk: usize,
    pub other_files: usize,
    pub deep_folders: usize,
    /// Bytes of the items that move.
    pub bytes: u64,
    /// Pile copies set aside as duplicates (left out of the JSON when 0).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub set_aside: usize,
    /// Pile copies of games the library already holds, set aside or
    /// reported (left out of the JSON when 0).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub in_library: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Something about the whole plan (data, never prose).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlanWarning {
    /// The system's DAT could not be consulted.
    CatalogueMissing { system: String },
    /// The target already looks like another layout.
    TargetLayoutMismatch { profile: String },
    /// The source cannot be written: files are copied, not moved.
    MoveUnavailable,
    /// Nothing loose: every ROM already sits in a system folder.
    SortedOnly,
    /// The target volume lacks room for what the plan writes: `needed`
    /// (cluster-rounded bytes, 1 % and the reserve) against the bytes
    /// `free` for this user (archives spec §8.4).
    NoSpace { needed: u64, free: u64 },
    /// The items that move by default write at least the large-transfer
    /// threshold (`bytes` in `items` items) to the target (systems
    /// batch 3 D23).
    LargeTransfer { bytes: u64, items: usize },
}

/// A dry run: what would move where, and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrganizePlan {
    /// Absolute.
    pub source_root: PathBuf,
    /// Absolute.
    pub target_root: PathBuf,
    /// Absolute: the folder holding the system folders.
    pub roms_dir: PathBuf,
    pub layout: String,
    pub mode: TransferMode,
    pub names: NameMode,
    pub items: Vec<OrganizeItem>,
    pub counts: Counts,
    /// Relative to the source root, with the system they resolve to.
    pub sorted_folders: Vec<(PathBuf, String)>,
    /// Relative to the source root.
    pub deep_folders: Vec<PathBuf>,
    /// Relative to the source root.
    pub other_files: Vec<PathBuf>,
    pub warnings: Vec<PlanWarning>,
    pub broad_root: bool,
    pub fat32: bool,
    pub move_allowed: bool,
    /// The layout the target already looks like, when it is not the
    /// chosen one.
    pub target_detected: Option<String>,
    pub created_at: String,
    /// Duplicate groups with at least one pile copy (duplicates spec
    /// §11.2); left out of the JSON when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub duplicates: Vec<DupGroup>,
    /// Set-aside items go to the OS Trash instead of the quarantine
    /// (left out of the JSON when false).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub set_aside_trash: bool,
    /// Rows that are an update or DLC of a game (systems batch 2 D11),
    /// for the summary sentence; left out of the JSON when 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub updates: usize,
    /// Services whose absence or used-up quota changes this plan
    /// (integrations spec D28): the app fills it from its settings and
    /// health, since the plan itself never reads the keychain. Left out of
    /// the JSON when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub integration_notices: Vec<crate::integrations::Notice>,
    /// Set when the target did not exist at plan time (mods/cheats spec
    /// D19): its nearest existing ancestor, so apply can create the target
    /// and still tell it from a vanished volume. Left out of the JSON when
    /// the target existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_anchor: Option<super::roots::TargetAnchor>,
    /// Games laid out for the emulator chosen for their system (mods spec
    /// D16), for the preview. Left out of the JSON when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub translated: Vec<TranslatedGameView>,
    /// Emulator data folders the plan writes into (D16, D17), with their
    /// anchor when apply has to create them. Left out of the JSON when
    /// empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emulator_roots: Vec<super::translate_plan::EmulatorRoot>,
}

/// One future plan item as the survey found it.
#[derive(Clone, Debug)]
pub struct SurveyEntry {
    /// Absolute, primary file first.
    pub sources: Vec<PathBuf>,
    pub kind: ItemKind,
    pub size: u64,
    pub fps: Vec<SourceFp>,
    /// Left before identification (walk, broken disc set, unreadable).
    pub left: Option<LeftReason>,
    pub identification: Option<Identification>,
    /// What opening an archive found (archives only): the listing, the
    /// disc set and the tool an extraction plan needs.
    pub inspection: Option<Inspection>,
    /// What the ladder read (identified entries): the duplicates step
    /// builds its units from it without reading the file again.
    pub facts: Option<Facts>,
    /// A folder game's tree (systems batch 2 D27): the target gates
    /// (FAT32 member size, Windows path length) read it.
    pub tree: Option<TreeSummary>,
    /// What a console header reader found (systems batch 2 §8): title id
    /// and kind group updates and DLC with their base (D11).
    pub console: Option<crate::consoles::ConsoleId>,
    /// Loose discs grouped into one game (adjustments D24): the entry is
    /// their `Set`, sources in disc order.
    pub disc_set: Option<LooseDiscSet>,
    /// The role the entry's `Updates/` or `DLC/` folder gives it (game
    /// folders D7); a header kind wins over it ([`effective_kind`]).
    pub role_hint: Option<ChildRole>,
    /// The game folder holding that `Updates/` or `DLC/` folder when it has
    /// exactly one base at its root (D3): the entry's link candidate.
    pub game_dir: Option<PathBuf>,
}

/// What an entry is to its game (game folders D7, D14): the header's kind
/// when it says anything, else the role its `Updates/` or `DLC/` folder
/// gives it, else `Unknown`. A header kind always wins over the folder.
pub fn effective_kind(entry: &SurveyEntry) -> TitleKind {
    let header = entry
        .console
        .as_ref()
        .map_or(TitleKind::Unknown, |c| c.kind);
    match (header, entry.role_hint) {
        (TitleKind::Unknown, Some(ChildRole::Update)) => TitleKind::Update,
        (TitleKind::Unknown, Some(ChildRole::Dlc)) => TitleKind::Dlc,
        (kind, _) => kind,
    }
}

/// Gives every entry found below an `Updates/` or `DLC/` folder of the
/// pile its role hint and game folder (D7); the deepest folder wins.
fn hint_child_roles(entries: &mut [SurveyEntry], child_dirs: &[super::walk::ChildDir]) {
    if child_dirs.is_empty() {
        return;
    }
    for entry in entries {
        let Some(primary) = entry.sources.first() else {
            continue;
        };
        let found = child_dirs
            .iter()
            .filter(|c| primary.starts_with(&c.dir) && primary != &c.dir)
            .max_by_key(|c| c.dir.components().count());
        if let Some(child) = found {
            entry.role_hint = Some(child.role);
            entry.game_dir = child.game_dir.clone();
        }
    }
}

/// A loose multi-disc game the survey grouped (adjustments spec D24).
#[derive(Clone, Debug, PartialEq)]
pub struct LooseDiscSet {
    /// The game's name without its disc tag: folder and playlist name.
    pub title: String,
    pub evidence: SetEvidence,
    /// Each disc's primary file (sheet or image), absolute, in disc order.
    pub discs: Vec<PathBuf>,
    /// Disc numbers that are not in the pile.
    pub missing: Vec<u8>,
}

/// What a pile holds and what each candidate is: everything a plan needs
/// except the user's options for the target and decisions, so changing
/// those re-plans without reading files.
#[derive(Clone, Debug)]
pub struct Survey {
    /// Absolute.
    pub source_root: PathBuf,
    /// Absolute.
    pub target_root: PathBuf,
    pub broad_root: bool,
    pub entries: Vec<SurveyEntry>,
    /// The walk's lists (candidates were turned into `entries`).
    pub walked: Walked,
}

/// Extensions of Switch files (the catalogue is only worth fetching for a
/// pile that has one; plan P-2).
const SWITCH_EXTS: [&str; 8] = ["xci", "xcz", "nsp", "nsz", "nro", "nso", "nca", "ncz"];

impl Survey {
    /// Whether an entry is a Switch title, by its header or its
    /// extension: callers fetch the optional catalogue only then (plan
    /// P-2, spec D10).
    pub fn has_switch(&self) -> bool {
        self.entries.iter().any(|e| {
            e.console.as_ref().is_some_and(|c| c.system == "switch")
                || e.sources
                    .first()
                    .and_then(|p| p.extension())
                    .is_some_and(|x| {
                        SWITCH_EXTS
                            .iter()
                            .any(|s| x.to_string_lossy().eq_ignore_ascii_case(s))
                    })
        })
    }

    /// Whether a Switch entry has NCA ids but no title id: only then is
    /// the catalogue's `ncas.json` fetched (spec D6).
    pub fn wants_ncas(&self) -> bool {
        self.entries.iter().any(|e| {
            e.console.as_ref().is_some_and(|c| {
                c.system == "switch" && c.title_id.is_none() && !c.nca_ids.is_empty()
            })
        })
    }

    /// The Switch title ids the readers found, sorted and without
    /// repeats: what the optional nlib fallback may be asked about (D9).
    pub fn switch_title_ids(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self
            .entries
            .iter()
            .filter_map(|e| e.console.as_ref())
            .filter(|c| c.system == "switch")
            .filter_map(|c| u64::from_str_radix(c.title_id.as_deref()?, 16).ok())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

/// What a quick, name-only look at a folder found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderProbe {
    pub rom_like: usize,
    pub partial_downloads: usize,
    pub cloud_only: usize,
    /// Names of folders that resolve to a system.
    pub sorted_folders: Vec<String>,
    pub writable: bool,
    pub broad_root: bool,
    /// Stopped after [`PROBE_LIMIT`] entries.
    pub truncated: bool,
}

/// The layout profile `opts.layout`, the target root and the ROMs folder.
fn resolve_layout(
    source: &Path,
    opts: &OrganizeOptions,
    env: &OrganizeEnv,
) -> Result<(Profile, PathBuf, PathBuf), OrganizeError> {
    let profile = env
        .profiles
        .iter()
        .find(|p| p.id == opts.layout)
        .cloned()
        .ok_or_else(|| OrganizeError::UnknownLayout(opts.layout.clone()))?;
    let target = match &opts.target {
        Some(t) => std::path::absolute(t).unwrap_or_else(|_| t.clone()),
        None => source.to_path_buf(),
    };
    let roms_dir = roms_dir_of(&target, &profile, opts.flat);
    Ok((profile, target, roms_dir))
}

fn roms_dir_of(target: &Path, profile: &Profile, flat: bool) -> PathBuf {
    if flat || profile.roms_root.is_empty() {
        target.to_path_buf()
    } else {
        find_dir_ignoring_case(target, &profile.roms_root)
            .unwrap_or_else(|| target.join(&profile.roms_root))
    }
}

/// The quarantine folders a walk of `source` organised into `target`
/// skips: `<source>/<name>` and `<target>/<name>` (the library's default
/// one), for `name` and both default names, in any letter case (a
/// case-sensitive file system can hold `_duplicates` beside `_Duplicados`).
pub fn quarantine_dirs(source: &Path, target: &Path, name: &str) -> Vec<PathBuf> {
    let names = [name, DEFAULT_QUARANTINE_NAME, "_Duplicates"];
    let mut out = Vec::new();
    for root in [source, target] {
        let mut push = |dir: PathBuf| {
            if !out.contains(&dir) {
                out.push(dir);
            }
        };
        for n in &names {
            push(root.join(n));
        }
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let found = entry.file_name();
            let found = found.to_string_lossy();
            if names.iter().any(|n| n.eq_ignore_ascii_case(&found)) {
                push(root.join(&*found));
            }
        }
    }
    out
}

/// `inner` is `outer` or below it (lexically).
fn is_within(inner: &Path, outer: &Path) -> bool {
    inner.starts_with(outer)
}

/// `member` is a symlink or reparse point itself, or reaches its file
/// through one: a folder between `source` and `member` that is a link.
fn reached_through_link(member: &Path, source: &Path) -> bool {
    let is_link = |p: &Path| {
        fs::symlink_metadata(p)
            .is_ok_and(|m| m.file_type().is_symlink() || super::walk::is_reparse_point(&m))
    };
    member
        .ancestors()
        .take_while(|p| *p != source && p.starts_with(source))
        .any(is_link)
}

/// `member` really lives under `source` once every link and `..` is
/// resolved. Anything that cannot be resolved counts as outside.
fn really_within(member: &Path, source: &Path) -> bool {
    match (fs::canonicalize(member), fs::canonicalize(source)) {
        (Ok(m), Ok(s)) => m.starts_with(s),
        _ => false,
    }
}

fn mtime_ns_of(meta: &fs::Metadata) -> i64 {
    match meta
        .modified()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
    {
        Ok(d) => i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
        Err(e) => i64::try_from(e.duration().as_nanos()).map_or(i64::MIN, |n| -n),
    }
}

fn fp_of(path: &Path) -> SourceFp {
    fs::symlink_metadata(path).map_or(
        SourceFp {
            size: 0,
            mtime_ns: 0,
        },
        |m| SourceFp {
            size: m.len(),
            mtime_ns: mtime_ns_of(&m),
        },
    )
}

/// [`fp_of`] for a left-in-place path. A folder game (`Freedoom Phase 1/`,
/// a ScummVM game) counts the bytes of the files below it: a directory's
/// own length is a filesystem detail (128 on APFS, 4096 on ext4).
fn left_fp_of(path: &Path) -> SourceFp {
    let mut fp = fp_of(path);
    if fs::symlink_metadata(path).is_ok_and(|m| m.is_dir()) {
        fp.size = WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();
    }
    fp
}

/// Walks, groups disc sets and identifies every candidate of `source`.
pub fn survey(
    source: &Path,
    opts: &OrganizeOptions,
    env: &OrganizeEnv,
    progress: &(dyn Fn(OrganizeProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<Survey, OrganizeError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(OrganizeError::Cancelled);
    }
    let source_abs = std::path::absolute(source).unwrap_or_else(|_| source.to_path_buf());
    match fs::metadata(&source_abs) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(OrganizeError::SourceMissing(source_abs)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(OrganizeError::SourceMissing(source_abs))
        }
        Err(e) => return Err(OrganizeError::Unreadable(source_abs, e.to_string())),
    }
    let (profile, target_root, roms_dir) = resolve_layout(&source_abs, opts, env)?;
    let broad_root = is_broad_root(source) || is_broad_root(&source_abs);

    // 1. The read-only walk.
    progress(OrganizeProgress {
        phase: OrganizePhase::Walk,
        done: 0,
        total: 0,
    });
    let mut skip_dirs = if target_root != source_abs && is_within(&target_root, &source_abs) {
        vec![target_root.clone()]
    } else {
        Vec::new()
    };
    // The pile's own quarantine and the library's are never walked
    // (duplicates spec §7.1): their copies are set aside on purpose.
    skip_dirs.extend(quarantine_dirs(
        &source_abs,
        &target_root,
        &opts.quarantine_name,
    ));
    let roms_root =
        (roms_dir != source_abs && is_within(&roms_dir, &source_abs)).then(|| roms_dir.clone());
    let ignored = ScanOptions::default().ignored_dirs;
    let mut walked = walk(
        &source_abs,
        env.db,
        &WalkOptions {
            skip_dirs,
            roms_root,
            art_dirs: &profile.art_dir_names,
            ignored_dirs: &ignored,
            now: env.now,
            recent: Duration::from_secs(opts.recent_secs),
            probe: env.probe,
            max_depth: MAX_DEPTH,
            profile: Some(&profile),
            cache: Some(env.cache),
        },
    )
    .map_err(|e| OrganizeError::Unreadable(source_abs.clone(), e.to_string()))?;
    // The walk names a folder game's target without the user's folder
    // names; the survey knows them.
    for (_, reason) in &mut walked.left {
        if let LeftReason::FolderGame { system, folder } = reason {
            if let Some(s) = env.db.get(system) {
                *folder =
                    crate::profiles::naming::system_folder(&profile, s, env.db, &opts.folder_names)
                        .folder;
            }
        }
    }
    walked.left.sort_by(|a, b| a.0.cmp(&b.0));
    progress(OrganizeProgress {
        phase: OrganizePhase::Walk,
        done: walked.candidates.len(),
        total: walked.candidates.len(),
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(OrganizeError::Cancelled);
    }

    // 2. Disc sets, then one entry per candidate or set.
    let candidates = std::mem::take(&mut walked.candidates);
    let mut entries = build_entries(&source_abs, candidates, &mut walked);
    for (path, reason) in std::mem::take(&mut walked.left) {
        let fp = left_fp_of(&path);
        entries.push(Pending {
            entry: SurveyEntry {
                fps: vec![fp],
                size: fp.size,
                sources: vec![path],
                kind: ItemKind::File,
                left: Some(reason),
                identification: None,
                inspection: None,
                facts: None,
                tree: None,
                console: None,
                disc_set: None,
                role_hint: None,
                game_dir: None,
            },
            candidate: None,
            set_sniff: None,
        });
    }
    // Every addon stays where it is, with the reason it would have if no
    // game took it, until the plan's link pass places it (mods spec D7):
    // a mod folder as one folder, an MSU-1 set as one set (D10).
    let tree_opts = WalkOptions {
        skip_dirs: Vec::new(),
        roms_root: None,
        art_dirs: &[],
        ignored_dirs: &[],
        now: env.now,
        recent: Duration::from_secs(opts.recent_secs),
        probe: env.probe,
        max_depth: MAX_DEPTH,
        profile: None,
        cache: None,
    };
    for (sources, kind, reason) in super::addons::unlinked_entries(&walked.addons) {
        let (fps, tree, reason) = if kind == ItemKind::Folder {
            let tree = super::walk::tree_summary(&sources[0], &tree_opts);
            let fp = SourceFp {
                size: tree.bytes,
                mtime_ns: tree.newest_mtime_ns,
            };
            // A mod folder passes a folder game's gates before the link
            // pass may move it (a link, a partial download, a file still
            // being written, a cloud placeholder inside stay).
            let reason = if tree.symlink {
                LeftReason::Symlink
            } else if tree.partial {
                LeftReason::PartialDownload
            } else if tree.recent {
                LeftReason::RecentlyModified
            } else if tree.cloud_only {
                LeftReason::CloudOnly
            } else {
                reason
            };
            (vec![fp], Some(tree), reason)
        } else {
            (
                sources.iter().map(|p| fp_of(p)).collect::<Vec<_>>(),
                None,
                reason,
            )
        };
        entries.push(Pending {
            entry: SurveyEntry {
                size: fps.iter().map(|f| f.size).sum(),
                fps,
                sources,
                kind,
                left: Some(reason),
                identification: None,
                inspection: None,
                facts: None,
                tree,
                console: None,
                disc_set: None,
                role_hint: None,
                game_dir: None,
            },
            candidate: None,
            set_sniff: None,
        });
    }
    entries.extend(
        folder_entries(&mut walked, opts.thresholds)
            .into_iter()
            .map(|entry| Pending {
                entry,
                candidate: None,
                set_sniff: None,
            }),
    );
    entries.sort_by(|a, b| a.entry.sources[0].cmp(&b.entry.sources[0]));

    // 3. The identification ladder on a few workers.
    let store = if opts.offline || !opts.download_dats {
        None
    } else {
        env.dats
    };
    let dat_progress = |done: usize, total: usize| {
        progress(OrganizeProgress {
            phase: OrganizePhase::Dats,
            done,
            total,
        });
    };
    let catalogue = DatCatalogue::new(env.db, env.cache, store, MAX_DAT_DOWNLOADS)
        .with_download_progress(&dat_progress);
    let arcade_order = opts
        .arcade_order
        .clone()
        .unwrap_or_else(|| profile.arcade_order.clone());
    let ladder = LadderEnv {
        db: env.db,
        catalogue: &catalogue,
        cache: env.cache,
        thresholds: opts.thresholds,
        broad_root,
        need_hash: false,
        arcade_order: &arcade_order,
        online: if opts.offline { None } else { env.online },
    };
    // Folder games come identified by their probe (plan P-3).
    let todo: Vec<usize> = (0..entries.len())
        .filter(|&i| entries[i].entry.left.is_none() && entries[i].entry.kind != ItemKind::Folder)
        .collect();
    let workers = if crate::write::on_removable_volume(&source_abs) {
        SLOW_VOLUME_WORKERS
    } else {
        WORKERS
    };
    // External tools are looked for only when an archive needs one, and
    // never in a `PATH` entry inside the source or target root (spec §5.2).
    let tools = ToolsOnDemand::new(&[&source_abs, &target_root]);
    let results = identify_all(
        &entries,
        &todo,
        workers,
        (env, &ladder, &opts.limits, &tools),
        progress,
        cancel,
    )?;
    let mut facts_of: HashMap<usize, Facts> = HashMap::new();
    let mut volume_sets: Vec<Vec<PathBuf>> = Vec::new();
    for (i, outcome) in results {
        match outcome {
            Ok((facts, id)) => {
                if !facts.inspection.volume_parts.is_empty() {
                    volume_sets.push(facts.inspection.volume_parts.clone());
                }
                let entry = &mut entries[i].entry;
                entry.identification = Some(id);
                // Any container the reader opened is an archive item (a
                // `.tar.gz` or a `game.gba.gz` too), extracted or moved.
                if facts.inspection.container.is_archive() && entry.kind == ItemKind::File {
                    entry.kind = ItemKind::Archive;
                }
                if entry.kind == ItemKind::Archive {
                    entry.inspection = Some(facts.inspection.clone());
                }
                facts_of.insert(i, facts);
            }
            Err(left) => entries[i].entry.left = Some(left),
        }
    }

    // 4. Duplicate bucket: raw files sharing size and prefix are hashed so
    //    duplicates can be told apart.
    //    A raw file as large as an archived ROM is hashed too, so a loose
    //    copy of a zipped game is seen as the same content.
    let archived_sizes: BTreeSet<u64> = entries
        .iter()
        .filter(|p| p.entry.kind == ItemKind::Archive)
        .filter_map(|p| p.entry.identification.as_ref()?.content_key.as_ref())
        .map(|(_, size)| *size)
        .collect();
    let mut buckets: HashMap<(u64, String), Vec<usize>> = HashMap::new();
    let mut lone: Vec<usize> = Vec::new();
    for (&i, facts) in &facts_of {
        let entry = &entries[i].entry;
        let Some(id) = &entry.identification else {
            continue;
        };
        if entry.kind == ItemKind::Set
            || facts.inspection.container != Container::Raw
            || id.left.is_some()
            || id.content_key.is_some()
        {
            continue;
        }
        if archived_sizes.contains(&entry.size) {
            lone.push(i);
        } else if let Some(head) = &facts.inspection.head_crc {
            buckets
                .entry((entry.size, head.clone()))
                .or_default()
                .push(i);
        }
    }
    let mut to_hash: Vec<usize> = buckets
        .into_values()
        .filter(|g| g.len() > 1)
        .flatten()
        .chain(lone)
        .collect();
    to_hash.sort_unstable();
    if !to_hash.is_empty() {
        let hashing = LadderEnv {
            need_hash: true,
            ..ladder
        };
        for (done, &i) in to_hash.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(OrganizeError::Cancelled);
            }
            let Some(mut facts) = facts_of.remove(&i) else {
                continue;
            };
            let candidate = entries[i]
                .candidate
                .clone()
                .expect("raw entries come from candidates");
            let id = identify_one(&candidate, &mut facts, &hashing);
            remember_facts(env.cache, &candidate, &facts);
            entries[i].entry.identification = Some(id);
            entries[i].entry.facts = Some(facts);
            progress(OrganizeProgress {
                phase: OrganizePhase::Hash,
                done: done + 1,
                total: to_hash.len(),
            });
        }
    }

    for (i, facts) in facts_of {
        entries[i].entry.facts = Some(facts);
    }
    let mut entries: Vec<SurveyEntry> = entries.into_iter().map(|p| p.entry).collect();
    gather_volume_sets(&mut entries, &volume_sets, &mut walked);
    for entry in &mut entries {
        if entry.console.is_none() {
            entry.console = entry.facts.as_ref().and_then(|f| {
                f.inspection
                    .payloads
                    .iter()
                    .find_map(|p| p.sniff.console.clone())
            });
        }
        dol_of_an_extracted_disc(entry);
        leave_game_part(entry);
        review_xbe_with_siblings(entry);
    }
    // Loose discs of one game become one set (adjustments D24), before
    // the duplicates step sees them.
    let entries = group_loose_discs(entries, env.db);
    let mut entries = updates_after_bases(entries);
    hint_child_roles(&mut entries, &walked.child_dirs);

    Ok(Survey {
        source_root: source_abs,
        target_root,
        broad_root,
        entries,
        walked,
    })
}

/// What a disc's header says about its place in a set (spec D24): the
/// GameCube/Wii ID6 and disc byte, an Xbox title id and disc number, or a
/// Sega `n/m` device field.
fn header_disc(entry: &SurveyEntry) -> Option<HeaderDisc> {
    if let Some(c) = &entry.console {
        match c.system.as_str() {
            "gamecube" | "wii" => {
                return Some(HeaderDisc::Nintendo {
                    id: c.title_id.clone()?,
                    n: c.disc_no?,
                })
            }
            "xbox" | "xbox360" => {
                return Some(HeaderDisc::Xbox {
                    id: c.title_id.clone()?,
                    n: c.disc_no?,
                    m: c.disc_count,
                })
            }
            _ => {}
        }
    }
    let sniff = &entry.facts.as_ref()?.inspection.payloads.first()?.sniff;
    sniff.disc.map(|(n, m)| HeaderDisc::Sega { n, m })
}

/// Whether an entry may be one disc of a loose set: an identified file or
/// sheet set (never a pile `.m3u`, an archive or a folder game) of a disc
/// system (`multi_file`; floppy disks of computers are left as they are)
/// that is a game, not an update, DLC or part.
fn disc_candidate<'e>(entry: &'e SurveyEntry, db: &SystemDb) -> Option<&'e str> {
    if entry.left.is_some() || entry.disc_set.is_some() {
        return None;
    }
    let primary = entry.sources.first()?;
    match entry.kind {
        ItemKind::File => {}
        ItemKind::Set if extension(primary) != "m3u" => {}
        _ => return None,
    }
    if entry
        .console
        .as_ref()
        .is_some_and(|c| c.part.is_some() || matches!(c.kind, TitleKind::Update | TitleKind::Dlc))
    {
        return None;
    }
    let id = entry.identification.as_ref()?;
    if id.left.is_some() || id.combined.verdict == Verdict::Unknown {
        return None;
    }
    id.combined
        .system
        .as_deref()
        .filter(|s| db.get(s).is_some_and(|s| s.multi_file))
}

/// Merges loose discs of one game into one `Set` entry (spec D24): the
/// first disc's identification stands for the set, its sources are every
/// disc's files in disc order, missing discs are noted and a set the names
/// alone group is reviewed. Each merged entry takes the place of its first
/// disc.
fn group_loose_discs(entries: Vec<SurveyEntry>, db: &SystemDb) -> Vec<SurveyEntry> {
    let (sets, at): (Vec<discsets::DiscSet>, Vec<usize>) = {
        let mut at = Vec::new();
        let mut candidates = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            let Some(system) = disc_candidate(entry, db) else {
                continue;
            };
            let id = entry.identification.as_ref();
            let dat_name = id
                .filter(|id| id.combined.method == Some(IdLevel::Hash))
                .and_then(|id| id.catalogue_name.as_deref());
            candidates.push(discsets::Candidate {
                path: &entry.sources[0],
                system,
                header: header_disc(entry),
                dat_name,
                region: entry.console.as_ref().and_then(|c| c.region),
            });
            at.push(i);
        }
        (discsets::group(&candidates), at)
    };
    if sets.is_empty() {
        return entries;
    }
    let mut slots: Vec<Option<SurveyEntry>> = entries.into_iter().map(Some).collect();
    for set in sets {
        let members: Vec<usize> = set.members.iter().map(|&m| at[m]).collect();
        let Some(first) = members.iter().copied().min() else {
            continue;
        };
        if members.iter().any(|&i| slots[i].is_none()) {
            continue;
        }
        let mut discs: Vec<SurveyEntry> = members.iter().filter_map(|&i| slots[i].take()).collect();
        let mut merged = discs.remove(0);
        let mut primaries = vec![merged.sources[0].clone()];
        let mut review = matches!(
            merged.identification.as_ref().map(|id| id.combined.verdict),
            Some(Verdict::Review)
        );
        for disc in discs {
            primaries.push(disc.sources[0].clone());
            review |= matches!(
                disc.identification.as_ref().map(|id| id.combined.verdict),
                Some(Verdict::Review)
            );
            merged.size = merged.size.saturating_add(disc.size);
            merged.sources.extend(disc.sources);
            merged.fps.extend(disc.fps);
        }
        merged.kind = ItemKind::Set;
        merged.inspection = None;
        if let Some(id) = merged.identification.as_mut() {
            if set.evidence == SetEvidence::NameOnly {
                review = true;
                id.notes.push(ItemNote::DiscSet);
            }
            if review && id.combined.verdict == Verdict::Auto {
                id.combined.verdict = Verdict::Review;
            }
            id.notes
                .extend(set.missing.iter().map(|&n| ItemNote::MissingDisc { n }));
        }
        merged.disc_set = Some(LooseDiscSet {
            title: set.title,
            evidence: set.evidence,
            discs: primaries,
            missing: set.missing,
        });
        slots[first] = Some(merged);
    }
    slots.into_iter().flatten().collect()
}

/// Systems whose loose disc sets get no `.m3u` (adjustments D24): their
/// emulators swap discs another way.
const NO_PLAYLIST: [&str; 3] = ["ps2", "xbox", "xbox360"];

/// The base title id and kind of an entry's console header (D11).
pub(crate) fn title_group(entry: &SurveyEntry) -> Option<((String, String), TitleKind)> {
    let c = entry.console.as_ref()?;
    Some(((c.system.clone(), base_title_id(c)?), link_kind(c)))
}

/// Puts every update and DLC right after its base (systems batch 2
/// D11): updates first, then DLC, each in path order. One with no base in
/// the pile keeps its place. Item ids are survey positions, so the plan
/// lists them in this order too.
fn updates_after_bases(entries: Vec<SurveyEntry>) -> Vec<SurveyEntry> {
    let bases: BTreeSet<(String, String)> = entries
        .iter()
        .filter_map(title_group)
        .filter(|(_, kind)| *kind == TitleKind::Base)
        .map(|(group, _)| group)
        .collect();
    if bases.is_empty() {
        return entries;
    }
    let mut rest: Vec<SurveyEntry> = Vec::with_capacity(entries.len());
    let mut children: HashMap<(String, String), Vec<SurveyEntry>> = HashMap::new();
    for entry in entries {
        match title_group(&entry) {
            Some((group, TitleKind::Update | TitleKind::Dlc)) if bases.contains(&group) => {
                children.entry(group).or_default().push(entry);
            }
            _ => rest.push(entry),
        }
    }
    let mut out = Vec::with_capacity(rest.len());
    for entry in rest {
        let group = title_group(&entry)
            .filter(|(_, kind)| *kind == TitleKind::Base)
            .map(|(group, _)| group);
        out.push(entry);
        if let Some(mut kids) = group.and_then(|g| children.remove(&g)) {
            kids.sort_by_key(|e| title_group(e).map(|(_, k)| k == TitleKind::Dlc));
            out.extend(kids);
        }
    }
    out
}

/// A title the optional Switch catalogue gave (titledb or nlib, D7).
fn catalogue_title(c: &crate::consoles::ConsoleId) -> Option<String> {
    use crate::consoles::TitleSource;
    matches!(
        c.title_source,
        Some(TitleSource::Titledb | TitleSource::Nlib)
    )
    .then(|| c.title.clone())
    .flatten()
}

/// A piece of a game that cannot play alone (adjustments D13: an ExeFS
/// file, a loose NCA, a library, title metadata, a module, a Vita
/// backup) is left with [`LeftReason::GamePart`], whatever else named
/// it. The title id is the piece's own, or for a backup the nearest
/// folder of its path named like a Sony title id (`PCSE00123`). Folder
/// games and sets are never parts.
fn leave_game_part(entry: &mut SurveyEntry) {
    let (Some(c), Some(id)) = (entry.console.as_ref(), entry.identification.as_mut()) else {
        return;
    };
    let Some(part) = c.part else {
        return;
    };
    if matches!(entry.kind, ItemKind::Folder | ItemKind::Set) || id.left.is_some() {
        return;
    }
    let title_id = if part == PartKind::Backup {
        entry.sources.first().and_then(|p| backup_title_id(p))
    } else {
        c.title_id.clone()
    };
    id.left = Some(LeftReason::GamePart {
        part,
        system: c.system.clone(),
        title_id,
    });
}

/// The nearest folder above `path` named like a Sony title id (four
/// letters, five digits): a Vita backup's only title id (psvimgtools,
/// `PS Vita/PGAME/<account>/NPJH00053/game/game.psvimg`).
fn backup_title_id(path: &Path) -> Option<String> {
    path.ancestors().skip(1).find_map(|dir| {
        let name = dir.file_name()?.to_str()?;
        let b = name.as_bytes();
        (b.len() == 9
            && b[..4].iter().all(u8::is_ascii_uppercase)
            && b[4..].iter().all(u8::is_ascii_digit))
        .then(|| name.to_owned())
    })
}

/// Files that sit beside `main.dol` in the `sys/` folder of a disc a
/// tool extracted (Dolphin «Extract Entire Disc», wiibrew «Wii disc»):
/// the apploader, or the disc header pair.
const EXTRACTED_DISC_SYS: [&str; 3] = ["apploader.img", "boot.bin", "bi2.bin"];

/// A DOL beside an extracted disc's system files (adjustments D13,
/// research §11) is the game's executable, not homebrew: it becomes part
/// `ExeFs` so [`leave_game_part`] leaves it where it is. Moving it alone
/// would break the extracted game.
fn dol_of_an_extracted_disc(entry: &mut SurveyEntry) {
    let Some(c) = entry.console.as_mut() else {
        return;
    };
    if entry.kind != ItemKind::File
        || !matches!(c.system.as_str(), "gamecube" | "wii")
        || c.kind != TitleKind::Homebrew
        || c.part.is_some()
    {
        return;
    }
    let Some(path) = entry.sources.first() else {
        return;
    };
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("dol"))
    {
        return;
    }
    let disc = path
        .parent()
        .and_then(super::walk::list_dir)
        .is_some_and(|listed| {
            let has = |want: &str| {
                listed
                    .iter()
                    .any(|(name, _, _)| name.eq_ignore_ascii_case(want))
            };
            has(EXTRACTED_DISC_SYS[0]) || (has(EXTRACTED_DISC_SYS[1]) && has(EXTRACTED_DISC_SYS[2]))
        });
    if disc {
        c.part = Some(PartKind::ExeFs);
        c.kind = TitleKind::Unknown;
    }
}

/// A loose retail XBE beside other files (adjustments §8.2) is the
/// executable of a game folder no probe matched (no `default.xbe`): it
/// becomes part `ExeFs` and is asked about, never moved alone. A lone
/// one, homebrew and a user's choice are left as they are.
fn review_xbe_with_siblings(entry: &mut SurveyEntry) {
    let Some(c) = entry.console.as_mut() else {
        return;
    };
    let user_chose = entry
        .identification
        .as_ref()
        .is_some_and(|id| id.combined.method == Some(IdLevel::User));
    if user_chose
        || entry.kind != ItemKind::File
        || c.system != "xbox"
        || c.id_source != Some(crate::consoles::IdSource::Xbe)
        || c.kind != TitleKind::Base
        || c.part.is_some()
    {
        return;
    }
    let Some(path) = entry.sources.first() else {
        return;
    };
    let own = path.file_name();
    let siblings = path
        .parent()
        .and_then(super::walk::list_dir)
        .is_some_and(|listed| {
            listed
                .iter()
                .any(|(name, _, _)| Some(std::ffi::OsStr::new(name)) != own)
        });
    if !siblings {
        return;
    }
    c.part = Some(PartKind::ExeFs);
    if let Some(id) = entry.identification.as_mut() {
        if id.combined.verdict == Verdict::Auto {
            id.combined.verdict = Verdict::Review;
        }
    }
}

/// The survey with every Switch header verdict completed from the
/// catalogue, or from the file name without one (adjustments D7); `None`
/// when no entry is a Switch title, so other piles are never copied.
fn with_switch_names(
    survey: &Survey,
    titles: Option<&dyn crate::catalog::TitleNames>,
) -> Option<Survey> {
    let is_switch = |e: &SurveyEntry| e.console.as_ref().is_some_and(|c| c.system == "switch");
    if !survey.entries.iter().any(is_switch) {
        return None;
    }
    let mut named = survey.clone();
    for entry in named.entries.iter_mut() {
        let name = entry
            .sources
            .first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(c) = entry.console.as_mut() {
            super::evidence::resolve_switch_title(c, titles, &name);
        }
    }
    Some(named)
}

/// The title of the base game an update or DLC belongs to, for its note:
/// the header's title, else the base's file (or folder) name without its
/// tags, else the base title id.
fn base_title(base: Option<(&SurveyEntry, &OrganizeItem)>, id: &str) -> String {
    let Some((entry, item)) = base else {
        return id.to_string();
    };
    if let Some(title) = entry.console.as_ref().and_then(|c| c.title.clone()) {
        return title;
    }
    let name = item
        .sources
        .first()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = match item.kind {
        ItemKind::Folder => name.as_str(),
        _ => name
            .rsplit_once('.')
            .map_or(name.as_str(), |(stem, _)| stem),
    };
    let title = crate::identify::names::parse_name(stem, false).title;
    if title.trim().is_empty() {
        id.to_string()
    } else {
        title.trim().to_string()
    }
}

/// Names the base's title in every item the walk left
/// `GameOverlay { of: <base folder name> }` (plan P-3): when the folder
/// `of` beside it is a folder item whose header has a title.
fn overlay_titles(items: &mut [OrganizeItem], survey: &Survey) {
    for (i, entry) in survey.entries.iter().enumerate() {
        let Some(LeftReason::GameOverlay { of }) = &entry.left else {
            continue;
        };
        let Some(parent) = entry.sources.first().and_then(|p| p.parent()) else {
            continue;
        };
        let base_path = parent.join(of);
        let titled = |e: &SurveyEntry| {
            e.kind == ItemKind::Folder && e.console.as_ref().is_some_and(|c| c.title.is_some())
        };
        // The exact name first; else, as on a case-insensitive volume
        // where the walk found the base under the overlay's spelling,
        // the folder beside it whose name differs only in ASCII case.
        let exact = survey
            .entries
            .iter()
            .position(|e| titled(e) && e.sources.first() == Some(&base_path));
        let Some(b) = exact.or_else(|| {
            survey.entries.iter().position(|e| {
                titled(e)
                    && e.sources.first().is_some_and(|p| {
                        p.parent() == Some(parent)
                            && p.file_name()
                                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(of))
                    })
            })
        }) else {
            continue;
        };
        let (Some(base_entry), Some(base_item)) = (survey.entries.get(b), items.get(b)) else {
            continue;
        };
        let title = base_title(Some((base_entry, base_item)), of);
        if let Some(OrganizeAction::Left {
            reason: LeftReason::GameOverlay { of },
        }) = items.get_mut(i).map(|item| &mut item.action)
        {
            *of = title;
        }
    }
}

/// The stem a file name is split from (`Tetris` for `Tetris.cia`); a
/// leading dot is not an extension.
fn stem_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => &name[..i],
        _ => name,
    }
}

/// `stem` without every `(…)`/`[…]` group and every case-insensitive
/// occurrence of `title`, trimmed of separators: what names a DLC when
/// its header gives no title of its own (game folders spec D6).
fn dlc_rest(stem: &str, title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in stem.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let title = title.trim();
    if !title.is_empty() {
        let lower = out.to_lowercase();
        let wanted = title.to_lowercase();
        // Offsets found in the lower-cased text hold in `out` only when
        // every char keeps its byte length (`İ` grows, `ẞ` shrinks, and
        // two such can even out the total); otherwise the title is left
        // in place rather than cut inside a char.
        let keeps_len = |s: &str| {
            s.chars()
                .all(|c| c.to_lowercase().map(char::len_utf8).sum::<usize>() == c.len_utf8())
        };
        if keeps_len(&out) && keeps_len(title) {
            let mut kept = String::new();
            let mut at = 0;
            while let Some(i) = lower[at..].find(&wanted) {
                kept.push_str(&out[at..at + i]);
                kept.push(' ');
                at += i + wanted.len();
            }
            kept.push_str(&out[at..]);
            out = kept;
        }
    }
    let words: Vec<&str> = out.split_whitespace().collect();
    words
        .join(" ")
        .trim_matches(|c: char| c == '-' || c == '_' || c == '.' || c == ',' || c == ' ')
        .to_string()
}

/// The `<DLC name>` of a DLC entry (game folders spec D6): its own header
/// title when it differs from its base's title, else its file stem
/// without the base title and its groups when anything is left (a bare
/// title id does not count), else `DLC <title id>`. Untrusted text: the
/// target pass makes it a safe path component.
fn dlc_name(
    entry: &SurveyEntry,
    item: &OrganizeItem,
    base: (&SurveyEntry, &OrganizeItem),
) -> Option<String> {
    Some(dlc_name_with(entry, item, &base_title(Some(base), "")))
}

/// [`dlc_name`] against the base title `base_title`.
fn dlc_name_with(entry: &SurveyEntry, item: &OrganizeItem, base_title: &str) -> String {
    let console = entry.console.as_ref();
    let title_id = console.and_then(|c| c.title_id.clone());
    let own = console
        .and_then(|c| c.title.as_deref())
        .and_then(crate::consoles::clean_title);
    if let Some(own) = own {
        if crate::write::name_key(&own) != crate::write::name_key(base_title) {
            return own;
        }
    }
    let name = item
        .sources
        .first()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = match item.kind {
        ItemKind::Folder => name.as_str(),
        _ => stem_of(&name),
    };
    let rest = dlc_rest(stem, base_title);
    let bare_id = title_id
        .as_deref()
        .is_some_and(|id| rest.eq_ignore_ascii_case(id.trim()));
    if rest.chars().any(char::is_alphanumeric) && !bare_id {
        return rest;
    }
    match title_id {
        Some(id) => format!("DLC {}", id.trim()),
        None => "DLC".to_string(),
    }
}

/// A base or child placeable in a game folder as it is: a folder game, a
/// file or archive moved whole, or a loose set without a layout of its
/// own (extractions and grouped discs keep today's placement).
fn game_folder_shape(item: &OrganizeItem, shape: Option<&ExtractShape>) -> bool {
    match item.kind {
        ItemKind::Folder | ItemKind::File | ItemKind::Archive => {
            matches!(item.transfer, Transfer::AsIs) && shape.is_none()
        }
        ItemKind::Set => shape.is_none(),
    }
}

/// The game folders link pass (game folders spec D5, D16, D24, plan
/// P-3), before targets, under a layout with the option on:
///
/// 1. every base of a [`GAME_FOLDER_SYSTEMS`] system that moves in gets
///    `game` (role, region) and, unless it is a folder game (its own
///    folder is the game folder), its `<Game>` ([`game_folder_name`]) as
///    [`Subdir::Game`] with `game.folder`;
/// 2. every update or DLC (header kind, else its `Updates/`/`DLC/`
///    folder) of such a base, by base title id or by the pile game folder
///    it sits in, gets `game.of` and [`Subdir::Child`]; a child of a base
///    left before targets is left with the same reason (§12);
/// 3. the highest update version of each base is `newest_here`.
///
/// Items a frontend's own DLC rule places (`subdirs` already set) and
/// shadPS4 overlay siblings keep their rule. Returns each DLC child's
/// `<DLC name>` source text.
#[allow(clippy::too_many_arguments)]
fn link_game_folders(
    items: &mut [OrganizeItem],
    entries: &[SurveyEntry],
    shapes: &[Option<ExtractShape>],
    subdirs: &mut [Option<Subdir>],
    siblings: &BTreeSet<usize>,
    layout: &TargetLayout,
    db: &SystemDb,
    index: &mut DirIndex,
) -> HashMap<usize, String> {
    let shape = |i: usize| shapes.get(i).and_then(Option::as_ref);
    let base_kind = |i: usize, item: &OrganizeItem| -> bool {
        let Some(c) = entries.get(i).and_then(|e| e.console.as_ref()) else {
            return false;
        };
        let system = item.system_id.as_deref();
        system == Some(c.system.as_str())
            && is_game_folder_system(&c.system)
            && link_kind(c) == TitleKind::Base
            && !is_demo(&entries[i], item)
            && !siblings.contains(&i)
            && subdirs.get(i).is_some_and(Option::is_none)
            && game_folder_shape(item, shape(i))
    };
    // 1. Bases: moving ones get their folder; left ones still lead their
    // children (§12).
    let bases: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(i, item)| {
            base_kind(*i, item)
                && (item.moves_in() || item.is_left())
                && library_keeper(item).is_none()
        })
        .map(|(i, _)| i)
        .collect();
    let kept_bases: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(i, item)| base_kind(*i, item) && library_keeper(item).is_some())
        .map(|(i, _)| i)
        .collect();
    let mut by_id: HashMap<(String, String), usize> = HashMap::new();
    let mut by_dir: HashMap<PathBuf, usize> = HashMap::new();
    for i in bases {
        let entry = &entries[i];
        let Some(c) = entry.console.as_ref() else {
            continue;
        };
        let item = &items[i];
        let kind = item.kind;
        if item.moves_in() {
            let mut link = GameLink::new(GameRole::Base);
            link.region = c.region;
            if item.kind != ItemKind::Folder {
                let original = item
                    .sources
                    .first()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let stem = match (&layout.names, &item.catalogue_name) {
                    (NameMode::Catalogue, Some(game)) if !game.trim().is_empty() => {
                        game.trim().to_string()
                    }
                    _ => stem_of(&original).to_string(),
                };
                let (system_dir, _) = folder_dir(layout, db, &c.system);
                let Some((name, exists)) = game_folder_name(&stem, c.region, &system_dir, index)
                else {
                    continue;
                };
                link.folder = Some(
                    system_dir
                        .join(&name)
                        .strip_prefix(&layout.target_root)
                        .map_or_else(|_| system_dir.join(&name), Path::to_path_buf),
                );
                link.folder_exists = exists;
                subdirs[i] = Some(Subdir::Game(name));
            }
            items[i].game = Some(link);
        }
        if let Some(id) = base_title_id(c) {
            by_id.entry((c.system.clone(), id)).or_insert(i);
        }
        if let Some(primary) = entry.sources.first() {
            let dir = match kind {
                ItemKind::Folder => primary.clone(),
                _ => primary.parent().map(Path::to_path_buf).unwrap_or_default(),
            };
            by_dir.entry(dir).or_insert(i);
        }
    }
    // A base whose copy in the library stays (reported in the library,
    // or set aside for it): its children go into that copy's game folder
    // when it has one, else they are placed on their own (flat beside it,
    // D19), never left as if they were in the library themselves.
    let mut in_library: HashMap<usize, Option<PathBuf>> = HashMap::new();
    for i in kept_bases {
        let Some(keeper) = library_keeper(&items[i]) else {
            continue;
        };
        let entry = &entries[i];
        let Some(c) = entry.console.as_ref() else {
            continue;
        };
        let (system_dir, _) = folder_dir(layout, db, &c.system);
        let abs = layout.target_root.join(keeper);
        let folder = abs
            .parent()
            .filter(|dir| {
                abs.symlink_metadata().is_ok_and(|m| !m.is_dir())
                    && *dir != system_dir
                    && dir.parent() == Some(system_dir.as_path())
            })
            .map(|dir| rel(&layout.target_root, dir));
        in_library.insert(i, folder);
        if let Some(id) = base_title_id(c) {
            by_id.entry((c.system.clone(), id)).or_insert(i);
        }
        if let Some(primary) = entry.sources.first() {
            let dir = match items[i].kind {
                ItemKind::Folder => primary.clone(),
                _ => primary.parent().map(Path::to_path_buf).unwrap_or_default(),
            };
            by_dir.entry(dir).or_insert(i);
        }
    }
    if by_id.is_empty() && by_dir.is_empty() {
        return HashMap::new();
    }
    // 2. Children.
    let mut names: HashMap<usize, String> = HashMap::new();
    for j in 0..items.len() {
        let entry = &entries[j];
        let role = match effective_kind(entry) {
            TitleKind::Update => GameRole::Update,
            TitleKind::Dlc => GameRole::Dlc,
            _ => continue,
        };
        let item = &items[j];
        if !item.moves_in()
            || siblings.contains(&j)
            || subdirs.get(j).is_some_and(Option::is_some)
            || !matches!(
                item.kind,
                ItemKind::Folder | ItemKind::File | ItemKind::Archive
            )
            || !game_folder_shape(item, shape(j))
        {
            continue;
        }
        let console = entry.console.as_ref();
        let by_title = console.and_then(|c| {
            let id = base_title_id(c)?;
            by_id.get(&(c.system.clone(), id)).copied()
        });
        let by_folder = || entry.game_dir.as_ref().and_then(|d| by_dir.get(d).copied());
        let Some(b) = by_title.or_else(by_folder).filter(|&b| b != j) else {
            continue;
        };
        if items[b].system_id != item.system_id {
            continue;
        }
        let mut link = GameLink::new(role);
        match in_library.get(&b) {
            None => link.of = Some(b),
            // Flat in the library, or no copy found there: on its own.
            Some(None) => continue,
            Some(Some(folder)) => {
                link.folder = Some(folder.clone());
                link.folder_exists = true;
            }
        }
        link.region = match in_library.get(&b) {
            None => items[b].game.as_ref().and_then(|g| g.region),
            Some(_) => entries[b].console.as_ref().and_then(|c| c.region),
        };
        if role == GameRole::Update {
            link.version = console.and_then(crate::consoles::display_version_of);
        }
        if role == GameRole::Dlc {
            if let Some(name) = dlc_name(entry, item, (&entries[b], &items[b])) {
                names.insert(j, name);
            }
        }
        let base_left = match &items[b].action {
            OrganizeAction::Left { reason } if !in_library.contains_key(&b) => Some(reason.clone()),
            _ => None,
        };
        let child = &mut items[j];
        child.game = Some(link);
        match base_left {
            Some(reason) => child.leave(reason),
            None => subdirs[j] = Some(Subdir::Child),
        }
    }
    // 3. The newest update of each base (D24), by the raw versions the
    // keeper rules compare.
    let mut newest: HashMap<usize, (Vec<u64>, usize)> = HashMap::new();
    for (j, item) in items.iter().enumerate() {
        let Some(b) = item
            .game
            .as_ref()
            .filter(|g| g.role == GameRole::Update)
            .and_then(|g| g.of)
        else {
            continue;
        };
        let Some(order) = entries[j]
            .console
            .as_ref()
            .and_then(|c| crate::dedupe::keeper::version_order(&c.system, c.version.as_deref()?))
        else {
            continue;
        };
        match newest.get(&b) {
            Some((best, _)) if *best >= order => {}
            _ => {
                newest.insert(b, (order, j));
            }
        }
    }
    for (_, j) in newest.into_values() {
        if let Some(link) = items[j].game.as_mut() {
            link.newest_here = true;
        }
    }
    names
}

/// The name a demo and its base are matched by (D12): a folder's own
/// name, a file's stem.
/// The library copy that stays instead of `item` (relative to the target
/// root): a pile copy reported as already in the library, or set aside
/// for a keeper there.
fn library_keeper(item: &OrganizeItem) -> Option<&Path> {
    match &item.action {
        OrganizeAction::Left {
            reason: LeftReason::InLibrary { path, .. },
        } => Some(path),
        OrganizeAction::SetAside { keeper, .. } => Some(keeper),
        _ => None,
    }
}

fn item_stem(item: &OrganizeItem) -> String {
    let name = item
        .sources
        .first()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match item.kind {
        ItemKind::Folder => name,
        _ => stem_of(&name).to_string(),
    }
}

/// A demo of a [`GAME_FOLDER_SYSTEMS`] system (game folders spec D12):
/// its header says so (3DS `00040002`, Wii U `00050002`, PS4 `APP_TYPE
/// 3`, Xbox 360 `0x00080000`), or its name has a `(Demo)`, `(Trial)` or
/// `(Kiosk Demo)` tag and its header names no other kind (PS3, Vita and
/// PSP headers have no demo flag).
fn is_demo(entry: &SurveyEntry, item: &OrganizeItem) -> bool {
    if !item.system_id.as_deref().is_some_and(is_game_folder_system) {
        return false;
    }
    match effective_kind(entry) {
        TitleKind::Demo => true,
        TitleKind::Base | TitleKind::Unknown => has_demo_tag(&item_stem(item)),
        _ => false,
    }
}

/// The stem of a demo's own game folder (D12) and the region still to
/// add: ` (Demo)` is appended, after the region, only when the name has
/// no demo tag already.
fn demo_folder_stem(stem: &str, region: Option<Region>) -> (String, Option<Region>) {
    if has_demo_tag(stem) {
        return (stem.to_string(), region);
    }
    let stem = stem.trim();
    match region {
        Some(r) if parse_tags(stem).regions.is_empty() && bare_region(stem) != Some(r) => {
            (format!("{stem} ({}) (Demo)", r.no_intro_word()), None)
        }
        _ => (format!("{stem} (Demo)"), None),
    }
}

/// The header of a library file for [`library_bases`]: from the memoised
/// facts (size and mtime keyed) when they are there, else read once
/// (and memoised) when `budget` allows a fresh read; `None` once it does
/// not. A cloud placeholder is never read. Key files never get here
/// (skipped by name).
fn library_header(
    path: &Path,
    meta: &fs::Metadata,
    env: &OrganizeEnv,
    limits: &Limits,
    budget: &mut LookupBudget,
) -> Option<Option<crate::consoles::ConsoleId>> {
    if env.probe.cloud_only(path, meta) {
        return Some(None);
    }
    let candidate = WalkCandidate {
        path: path.to_path_buf(),
        ext: extension(path),
        size: meta.len(),
        mtime_ns: mtime_ns_of(meta),
        cloud_only: false,
    };
    let facts = match memoised_facts(env.cache, &candidate) {
        Some(facts) => facts,
        None => {
            if !budget.spend_read() {
                return None;
            }
            let Ok(inspection) = inspect_file_with(path, candidate.size, env.db, limits, None)
            else {
                return Some(None);
            };
            let facts = Facts {
                inspection,
                raw_crcs: Vec::new(),
                hashed: false,
            };
            if memoisable(&facts) {
                remember_facts(env.cache, &candidate, &facts);
            }
            facts
        }
    };
    Some(
        facts
            .inspection
            .payloads
            .iter()
            .find_map(|p| p.sniff.console.clone()),
    )
}

/// Where an orphan update or DLC goes (game folders spec D13).
enum OrphanPlace {
    /// Into this game folder (absolute), on disk or not, its base found
    /// (by title id, or a folder of its title's name already there).
    Join(PathBuf, bool),
    /// Its base is a flat file of the library: flat beside it (D19).
    Flat,
    /// No base anywhere: into a new folder named by its title, or flat
    /// with none; review either way.
    Missing(Option<PathBuf>),
}

/// Demos and orphans (game folders spec D12, D13, plan P-4), after
/// [`link_game_folders`] and before targets:
///
/// - every demo of a [`GAME_FOLDER_SYSTEMS`] system that moves in is
///   linked by name only to a base with its title key, in the plan or at
///   the top of the target system folder ([`ItemNote::DemoOf`], also
///   with the option off); with the option on it is its own game,
///   `<Name> (Demo)/<Name> (Demo).<ext>` ([`GameRole::Demo`]), never in
///   its base's folder;
/// - with the option on, every update or DLC whose base is not in the
///   plan is looked up in the target ([`library_bases`]): a base in a game
///   folder takes it in (Auto), a flat base leaves it flat beside it
///   (Auto, no folder made, D19); no base → `<System>/<Title>/` when a
///   trusted title is known, else flat, Review and
///   [`ItemNote::BaseMissing`] either way. A spent budget is no base.
///
/// Each orphan DLC's `<DLC name>` source text goes into `dlc_names`.
#[allow(clippy::too_many_arguments)]
fn link_demos_and_orphans(
    items: &mut [OrganizeItem],
    entries: &[SurveyEntry],
    shapes: &[Option<ExtractShape>],
    subdirs: &mut [Option<Subdir>],
    siblings: &BTreeSet<usize>,
    layout: &TargetLayout,
    (env, limits): (&OrganizeEnv, &Limits),
    index: &mut DirIndex,
    dlc_names: &mut HashMap<usize, String>,
) {
    let placeable = |i: usize, item: &OrganizeItem, subdirs: &[Option<Subdir>]| {
        item.moves_in()
            && !siblings.contains(&i)
            && subdirs.get(i).is_some_and(Option::is_none)
            && game_folder_shape(item, shapes.get(i).and_then(Option::as_ref))
    };
    let demos: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(i, item)| item.moves_in() && entries.get(*i).is_some_and(|e| is_demo(e, item)))
        .map(|(i, _)| i)
        .collect();
    // Every base of the plan, placeable in a game folder or not (a zip to
    // extract, a frontend's own rule): a child of one is no orphan (D13),
    // it stays where the link pass left it.
    let mut plan_ids: BTreeSet<(String, String)> = BTreeSet::new();
    let mut plan_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for (i, item) in items.iter().enumerate() {
        let Some(entry) = entries.get(i) else {
            continue;
        };
        let Some(c) = entry.console.as_ref() else {
            continue;
        };
        if item.system_id.as_deref() != Some(c.system.as_str())
            || link_kind(c) != TitleKind::Base
            || is_demo(entry, item)
        {
            continue;
        }
        if let Some(id) = base_title_id(c) {
            plan_ids.insert((c.system.clone(), id));
        }
        if let Some(primary) = entry.sources.first() {
            plan_dirs.insert(match item.kind {
                ItemKind::Folder => primary.clone(),
                _ => primary.parent().map(Path::to_path_buf).unwrap_or_default(),
            });
        }
    }
    let base_in_plan = |entry: &SurveyEntry| {
        let by_id = entry.console.as_ref().is_some_and(|c| {
            base_title_id(c).is_some_and(|id| plan_ids.contains(&(c.system.clone(), id)))
        });
        by_id
            || entry
                .game_dir
                .as_ref()
                .is_some_and(|d| plan_dirs.contains(d))
    };
    let orphans: Vec<usize> = if layout.game_folders {
        items
            .iter()
            .enumerate()
            .filter(|(j, item)| {
                matches!(
                    entries.get(*j).map(effective_kind),
                    Some(TitleKind::Update | TitleKind::Dlc)
                ) && item.game.is_none()
                    && !entries.get(*j).is_some_and(base_in_plan)
                    && item.system_id.as_deref().is_some_and(is_game_folder_system)
                    && matches!(
                        item.kind,
                        ItemKind::Folder | ItemKind::File | ItemKind::Archive
                    )
                    && placeable(*j, item, subdirs)
            })
            .map(|(j, _)| j)
            .collect()
    } else {
        Vec::new()
    };
    if demos.is_empty() && orphans.is_empty() {
        return;
    }

    // One listing per system with a demo or an orphan; headers only for
    // systems with an orphan.
    let mut wanted: BTreeMap<String, bool> = BTreeMap::new();
    for &i in &demos {
        if let Some(system) = items[i].system_id.clone() {
            wanted.entry(system).or_insert(false);
        }
    }
    for &j in &orphans {
        if let Some(system) = items[j].system_id.clone() {
            wanted.insert(system, true);
        }
    }
    let mut budget = LookupBudget::new(limits.plan_head_budget, limits.plan_wall_time);
    let mut libraries: HashMap<String, LibraryIndex> = HashMap::new();
    for (system, read_ids) in wanted {
        let (system_dir, _) = folder_dir(layout, env.db, &system);
        let mut read =
            |path: &Path, meta: &fs::Metadata| library_header(path, meta, env, limits, &mut budget);
        let found = library_bases(&system_dir, &system, env.db, read_ids, &mut read);
        libraries.insert(system, found);
    }

    // Demos (D12): every base of a title key with its region (header of
    // the same system, else the single name-tag region), so a demo links
    // to the base of its region (mods spec D20).
    let header_region = |i: usize, system: &str| {
        entries
            .get(i)
            .and_then(|e| e.console.as_ref())
            .filter(|c| c.system == system)
            .and_then(|c| c.region)
    };
    type Bases = HashMap<(String, String), Vec<(String, Option<Region>)>>;
    let mut plan_bases: Bases = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let (Some(system), Some(entry)) = (item.system_id.clone(), entries.get(i)) else {
            continue;
        };
        if !is_game_folder_system(&system)
            || is_demo(entry, item)
            || matches!(
                effective_kind(entry),
                TitleKind::Update | TitleKind::Dlc | TitleKind::System
            )
        {
            continue;
        }
        let stem = item_stem(item);
        let key = parse_tags(&stem).title_key;
        if !key.is_empty() {
            let region = header_region(i, &system).or_else(|| single_name_region(&stem));
            plan_bases
                .entry((system, key))
                .or_default()
                .push((stem, region));
        }
    }
    for i in demos {
        let Some(system) = items[i].system_id.clone() else {
            continue;
        };
        let stem = item_stem(&items[i]);
        let key = demo_free_key(&stem);
        let demo_region = header_region(i, &system).or_else(|| single_name_region(&stem));
        let in_plan = plan_bases
            .get(&(system.clone(), key.clone()))
            .and_then(|cands| pick_by_region(cands, demo_region, |c| c.1, |c| &c.0))
            .map(|c| (c.0.clone(), c.1));
        let in_library = libraries.get(&system).and_then(|l| {
            pick_by_region(
                l.by_title_key_all(&key),
                demo_region,
                |b| b.region,
                |b| b.title.as_deref().unwrap_or(""),
            )
            .and_then(|b| b.title.clone().map(|t| (t, b.region)))
        });
        // The demo's own region wins wherever it is (plan first); else the
        // plan's pick by preference, else the library's (D20).
        let exact = |c: &Option<(String, Option<Region>)>| {
            demo_region.is_some() && c.as_ref().is_some_and(|c| c.1 == demo_region)
        };
        let of = if !exact(&in_plan) && exact(&in_library) {
            in_library
        } else {
            in_plan.or(in_library)
        }
        .map(|c| c.0);
        if let Some(of) = &of {
            let note = ItemNote::DemoOf { of: of.clone() };
            if !items[i].notes.contains(&note) {
                items[i].notes.push(note);
            }
        }
        if !layout.game_folders {
            continue;
        }
        let region = entries[i]
            .console
            .as_ref()
            .filter(|c| c.system == system)
            .and_then(|c| c.region);
        let mut link = GameLink::new(GameRole::Demo);
        link.region = region;
        link.demo_of = of;
        // A folder game is its own folder; anything else gets one.
        if items[i].kind != ItemKind::Folder && placeable(i, &items[i], subdirs) {
            let named = match (&layout.names, &items[i].catalogue_name) {
                (NameMode::Catalogue, Some(game)) if !game.trim().is_empty() => {
                    game.trim().to_string()
                }
                _ => stem.clone(),
            };
            let (wanted, region) = demo_folder_stem(&named, region);
            let (system_dir, _) = folder_dir(layout, env.db, &system);
            if let Some((name, exists)) = game_folder_name(&wanted, region, &system_dir, index) {
                link.folder = Some(rel(&layout.target_root, &system_dir.join(&name)));
                link.folder_exists = exists;
                subdirs[i] = Some(Subdir::Game(name));
            }
        }
        items[i].game = Some(link);
    }

    // Orphans (D13).
    let mut titled: HashMap<(String, String), Option<(PathBuf, bool)>> = HashMap::new();
    for j in orphans {
        let Some(system) = items[j].system_id.clone() else {
            continue;
        };
        let entry = &entries[j];
        let role = match effective_kind(entry) {
            TitleKind::Update => GameRole::Update,
            _ => GameRole::Dlc,
        };
        let console = entry.console.as_ref().filter(|c| c.system == system);
        let mut link = GameLink::new(role);
        link.region = console.and_then(|c| c.region);
        if role == GameRole::Update {
            link.version = console.and_then(crate::consoles::display_version_of);
        }
        let (system_dir, choice) = folder_dir(layout, env.db, &system);
        let found = console
            .and_then(base_title_id)
            .and_then(|id| libraries.get(&system)?.by_title_id(&id));
        let place = match found {
            Some(base) if base.in_game_folder => OrphanPlace::Join(base.path.clone(), true),
            Some(_) => OrphanPlace::Flat,
            None => {
                // A trusted title: the catalogue's name for its base, else
                // an update's own header title (a DLC's names the DLC).
                let title = console
                    .and_then(catalogue_title)
                    .or_else(|| {
                        (role == GameRole::Update)
                            .then(|| console.and_then(|c| c.title.clone()))
                            .flatten()
                    })
                    .and_then(|t| crate::consoles::clean_title(&t));
                match title {
                    None => OrphanPlace::Missing(None),
                    Some(title) => {
                        let folder = titled
                            .entry((system.clone(), crate::write::name_key(&title)))
                            .or_insert_with(|| {
                                game_folder_name(&title, link.region, &system_dir, index)
                                    .map(|(name, exists)| (system_dir.join(name), exists))
                            })
                            .clone();
                        match folder {
                            Some((dir, true)) => OrphanPlace::Join(dir, true),
                            Some((dir, false)) => OrphanPlace::Missing(Some(dir)),
                            None => OrphanPlace::Missing(None),
                        }
                    }
                }
            }
        };
        let folder = match place {
            OrphanPlace::Join(dir, exists) => Some((dir, exists)),
            OrphanPlace::Flat => None,
            OrphanPlace::Missing(dir) => {
                let item = &mut items[j];
                if !item.notes.contains(&ItemNote::BaseMissing) {
                    item.notes.push(ItemNote::BaseMissing);
                }
                if item.action == OrganizeAction::Auto {
                    item.action = OrganizeAction::Review;
                }
                dir.map(|d| (d, false))
            }
        };
        if let Some((dir, exists)) = folder {
            let game = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if role == GameRole::Dlc {
                let title = crate::identify::names::parse_name(&game, false).title;
                dlc_names.insert(j, dlc_name_with(entry, &items[j], title.trim()));
            }
            link.folder = Some(rel(&layout.target_root, &dir));
            link.folder_exists = exists;
            subdirs[j] = Some(Subdir::Child);
            items[j].folder = system_dir
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .or(Some(choice.folder));
        }
        items[j].game = Some(link);
    }
}

/// Notes every update and DLC with the base it belongs to (D11), when
/// its header names the system the item goes to; returns how many rows
/// carry such a note.
fn note_updates(items: &mut [OrganizeItem], entries: &[SurveyEntry]) -> usize {
    let mut bases: HashMap<(String, String), usize> = HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        if let Some((group, TitleKind::Base)) = title_group(entry) {
            bases.entry(group).or_insert(i);
        }
    }
    let mut notes: Vec<(usize, ItemNote)> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let Some(((system, base_id), kind)) = title_group(entry) else {
            continue;
        };
        let Some(item) = items.get(i) else { continue };
        if item.system_id.as_deref() != Some(system.as_str()) {
            continue;
        }
        let base = bases
            .get(&(system.clone(), base_id.clone()))
            .and_then(|&b| Some((entries.get(b)?, items.get(b)?)));
        // With no base in the pile, the catalogue's base name (D7) before
        // the bare title id.
        let of = match base {
            Some(_) => base_title(base, &base_id),
            None => entry
                .console
                .as_ref()
                .and_then(catalogue_title)
                .unwrap_or_else(|| base_id.clone()),
        };
        let note = match kind {
            TitleKind::Update => ItemNote::Update { of },
            TitleKind::Dlc => ItemNote::Dlc { of },
            _ => continue,
        };
        notes.push((i, note));
    }
    let count = notes.len();
    for (i, note) in notes {
        items[i].notes.push(note);
    }
    count
}

/// One entry per folder game of the walk (systems batch 2 D12, D27),
/// identified already by its probe: Auto or Review by its confidence, or
/// left by the first gate its tree fails (a symlink, a partial download,
/// a recent file, a cloud placeholder, in that order). A tree past the
/// caps stays in review. Folders two probes claimed for different
/// systems go to review with both systems as alternatives. A zero-byte
/// file is a partial download except in a folder game of
/// [`ZERO_BYTE_OK`] (systems batch 3 D24).
fn folder_entries(walked: &mut Walked, thresholds: Thresholds) -> Vec<SurveyEntry> {
    let entry = |path: PathBuf,
                 tree: TreeSummary,
                 combined: Combined,
                 console: Option<crate::consoles::ConsoleId>,
                 zero_ok: bool| {
        // A PS5 app folder enhanced for the PS5 Pro (systems batch 3 D6):
        // the flag rides on the probe's fixed `detail`.
        let notes = console
            .as_ref()
            .filter(|c| {
                c.system == "ps5" && c.detail == crate::consoles::orbis::PARAM_JSON_PRO_DETAIL
            })
            .map(|_| vec![ItemNote::ProEnhanced])
            .unwrap_or_default();
        let left = if tree.symlink {
            Some(LeftReason::Symlink)
        } else if tree.partial || (tree.zero_bytes && !zero_ok) {
            Some(LeftReason::PartialDownload)
        } else if tree.recent {
            Some(LeftReason::RecentlyModified)
        } else if tree.cloud_only {
            Some(LeftReason::CloudOnly)
        } else {
            None
        };
        let fp = SourceFp {
            size: tree.bytes,
            mtime_ns: tree.newest_mtime_ns,
        };
        SurveyEntry {
            sources: vec![path],
            kind: ItemKind::Folder,
            size: tree.bytes,
            fps: vec![fp],
            left,
            identification: Some(Identification {
                combined,
                left: None,
                catalogue_name: None,
                content_key: None,
                rom_count: 1,
                notes,
                disc: None,
            }),
            inspection: None,
            facts: None,
            tree: Some(tree),
            console,
            disc_set: None,
            role_hint: None,
            game_dir: None,
        }
    };
    let mut out = Vec::new();
    for game in std::mem::take(&mut walked.folder_games) {
        let id = &game.id;
        let verdict = if !game.tree.truncated && id.confidence >= thresholds.auto {
            Verdict::Auto
        } else {
            Verdict::Review
        };
        let combined = Combined {
            verdict,
            system: Some(id.system.clone()),
            confidence: id.confidence,
            method: Some(IdLevel::Header),
            why: vec![Evidence::new(
                IdLevel::Header,
                &id.system,
                id.confidence,
                id.detail.clone(),
            )],
            alternatives: Vec::new(),
        };
        let zero_ok = ZERO_BYTE_OK.contains(&game.id.system.as_str());
        out.push(entry(
            game.path,
            game.tree,
            combined,
            Some(game.id),
            zero_ok,
        ));
    }
    for (path, systems, tree) in std::mem::take(&mut walked.folder_conflicts) {
        // Every system it may go to keeps empty files in its dumps.
        let zero_ok = systems.iter().all(|s| ZERO_BYTE_OK.contains(&s.as_str()));
        let combined = Combined {
            verdict: Verdict::Review,
            system: None,
            confidence: 0.0,
            method: Some(IdLevel::Header),
            why: Vec::new(),
            alternatives: systems
                .into_iter()
                .map(|s| (s, thresholds.review))
                .collect(),
        };
        out.push(entry(path, tree, combined, None, zero_ok));
    }
    out
}

/// Systems whose folder games may hold zero-byte files (systems batch 3
/// D24): console dumps legitimately do, and the `.ps4` marker is one.
const ZERO_BYTE_OK: [&str; 4] = ["ps4", "ps5", "xboxone", "xboxseries"];

/// Turns every multi-volume set into one Left entry holding all its parts
/// (archives spec §3.2 rule 6): the entry of the set's first part found
/// (in volume order) takes the others' files, which never become items,
/// companions or other files of their own, so no part moves apart.
fn gather_volume_sets(entries: &mut Vec<SurveyEntry>, sets: &[Vec<PathBuf>], walked: &mut Walked) {
    let mut gone: BTreeSet<PathBuf> = BTreeSet::new();
    for parts in sets {
        if parts.iter().any(|p| gone.contains(p)) {
            continue;
        }
        let Some(owner) = parts
            .iter()
            .find_map(|p| entries.iter().position(|e| e.sources[0] == *p))
        else {
            continue;
        };
        let present: Vec<PathBuf> = parts.iter().filter(|p| p.is_file()).cloned().collect();
        let entry = &mut entries[owner];
        entry.fps = present.iter().map(|p| fp_of(p)).collect();
        entry.size = entry.fps.iter().map(|fp| fp.size).sum();
        entry.sources = present;
        entry.kind = ItemKind::Archive;
        entry.left = Some(LeftReason::MultiVolumeArchive { parts: parts.len() });
        entry.identification = None;
        entry.inspection = None;
        gone.extend(entry.sources.iter().skip(1).cloned());
    }
    if gone.is_empty() {
        return;
    }
    entries.retain(|e| e.sources.len() > 1 || !gone.contains(&e.sources[0]));
    walked.other_files.retain(|p| !gone.contains(p));
    walked.companions.retain(|p| !gone.contains(p));
    walked.left.retain(|(p, _)| !gone.contains(p));
}

/// An entry being surveyed: the candidate behind it (single files) or the
/// header of its data track (disc sets).
struct Pending {
    entry: SurveyEntry,
    candidate: Option<WalkCandidate>,
    set_sniff: Option<Sniff>,
}

/// Turns walk candidates into entries: list files (`.cue`, `.gdi`,
/// `.m3u`) take the files they name along as one set; broken sets stay.
fn build_entries(
    source: &Path,
    candidates: Vec<WalkCandidate>,
    walked: &mut Walked,
) -> Vec<Pending> {
    let paths: Vec<PathBuf> = candidates.iter().map(|c| c.path.clone()).collect();
    let mut warnings: Vec<ScanWarning> = Vec::new();
    let sets = group_sets(paths, &mut warnings);
    let by_path: HashMap<&Path, &WalkCandidate> =
        candidates.iter().map(|c| (c.path.as_path(), c)).collect();

    // Members referenced by more than one set.
    let mut uses: HashMap<String, usize> = HashMap::new();
    for (_, members) in &sets {
        for m in members {
            *uses.entry(m.to_string_lossy().to_lowercase()).or_default() += 1;
        }
    }
    let mut taken: BTreeSet<PathBuf> = BTreeSet::new();
    let mut out = Vec::new();
    for (primary, members) in sets {
        let Some(candidate) = by_path.get(primary.as_path()).map(|c| (*c).clone()) else {
            continue;
        };
        let is_list = is_list_extension(&candidate.ext);
        let mut entry = SurveyEntry {
            sources: vec![primary.clone()],
            kind: ItemKind::File,
            size: candidate.size,
            fps: vec![SourceFp {
                size: candidate.size,
                mtime_ns: candidate.mtime_ns,
            }],
            left: None,
            identification: None,
            inspection: None,
            facts: None,
            tree: None,
            console: None,
            disc_set: None,
            role_hint: None,
            game_dir: None,
        };
        if !is_list {
            if candidate.ext == "zip" || candidate.ext == "7z" {
                entry.kind = ItemKind::Archive;
            }
            out.push(Pending {
                entry,
                candidate: Some(candidate),
                set_sniff: None,
            });
            continue;
        }
        // The list file and every file of its closure.
        let mut lists: Vec<PathBuf> = vec![primary.clone()];
        lists.extend(
            members
                .iter()
                .filter(|m| is_list_extension(&extension(m)))
                .cloned(),
        );
        let missing: Vec<String> = warnings
            .iter()
            .filter(|w| lists.contains(&w.path))
            .filter_map(|w| missing_name(&w.message))
            .collect();
        let base = primary.parent().unwrap_or(source).to_path_buf();
        for m in &members {
            entry.sources.push(m.clone());
            let fp = fp_of(m);
            entry.size += fp.size;
            entry.fps.push(fp);
            taken.insert(m.clone());
        }
        if !members.is_empty() {
            entry.kind = ItemKind::Set;
        }
        // A track the walk left (still being written, a symlink, a
        // partial download, unreadable) holds the whole set back.
        let member_left = members
            .iter()
            .find_map(|m| walked.left.iter().find(|(p, _)| p == m))
            .map(|(_, reason)| reason.clone());
        entry.left = if !missing.is_empty() {
            Some(LeftReason::SetIncomplete { missing })
        } else if let Some(reason) = member_left {
            Some(reason)
        } else if members.iter().any(|m| reached_through_link(m, source)) {
            Some(LeftReason::Symlink)
        } else if members
            .iter()
            .any(|m| !is_within(m, &base) || !is_within(m, source) || !really_within(m, source))
        {
            Some(LeftReason::SetOutsideFolder)
        } else if members
            .iter()
            .any(|m| uses.get(&m.to_string_lossy().to_lowercase()) > Some(&1))
        {
            Some(LeftReason::SharedTrack)
        } else {
            None
        };
        let set_sniff = if entry.left.is_none() && entry.kind == ItemKind::Set {
            members
                .iter()
                .find(|m| !is_list_extension(&extension(m)))
                .and_then(|track| inspect_set_primary(track).ok())
        } else {
            None
        };
        out.push(Pending {
            entry,
            candidate: Some(candidate),
            set_sniff,
        });
    }
    // Files that travel with a set are not companions or other files.
    walked.companions.retain(|p| !taken.contains(p));
    walked.other_files.retain(|p| !taken.contains(p));
    walked.left.retain(|(p, _)| !taken.contains(p));
    out
}

/// `x` of the multifile warning "referenced file `x` not found".
fn missing_name(message: &str) -> Option<String> {
    message
        .strip_prefix("referenced file `")?
        .strip_suffix("` not found")
        .map(str::to_string)
}

type Outcome = Result<(Facts, Identification), LeftReason>;

/// Identifies `todo` (indices into `entries`) on `workers` scoped threads
/// pulling indices from a shared counter.
fn identify_all(
    entries: &[Pending],
    todo: &[usize],
    workers: usize,
    (env, ladder, limits, tools): (&OrganizeEnv, &LadderEnv, &Limits, &ToolsOnDemand),
    progress: &(dyn Fn(OrganizeProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<Vec<(usize, Outcome)>, OrganizeError> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let total = todo.len();
    let last = Mutex::new(Instant::now());
    progress(OrganizeProgress {
        phase: OrganizePhase::Identify,
        done: 0,
        total,
    });
    let results: Mutex<Vec<(usize, Outcome)>> = Mutex::new(Vec::with_capacity(total));
    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, total.max(1)) {
            scope.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(&i) = todo.get(k) else {
                    break;
                };
                let outcome = identify_entry(&entries[i], env, ladder, (limits, tools));
                results
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((i, outcome));
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                let mut at = last.lock().unwrap_or_else(|e| e.into_inner());
                if n == total || at.elapsed() >= PROGRESS_EVERY {
                    *at = Instant::now();
                    progress(OrganizeProgress {
                        phase: OrganizePhase::Identify,
                        done: n,
                        total,
                    });
                }
            });
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(OrganizeError::Cancelled);
    }
    let mut results = results.into_inner().unwrap_or_else(|e| e.into_inner());
    results.sort_by_key(|(i, _)| *i);
    Ok(results)
}

/// Facts (memoised or freshly inspected) and the ladder's verdict for one
/// entry.
fn identify_entry(
    pending: &Pending,
    env: &OrganizeEnv,
    ladder: &LadderEnv,
    (limits, tools): (&Limits, &ToolsOnDemand),
) -> Outcome {
    let candidate = pending
        .candidate
        .as_ref()
        .expect("entries to identify come from candidates");
    let mut facts = if pending.entry.kind == ItemKind::Set {
        set_facts(candidate, &pending.entry, pending.set_sniff.clone())
    } else if candidate.cloud_only {
        name_only_facts(candidate)
    } else {
        match memoised_facts(env.cache, candidate) {
            Some(facts) => facts,
            None => {
                let inspection =
                    inspect_file_with(&candidate.path, candidate.size, env.db, limits, Some(tools))
                        .map_err(|_| LeftReason::Unreadable)?;
                Facts {
                    inspection,
                    raw_crcs: Vec::new(),
                    hashed: false,
                }
            }
        }
    };
    let id = identify_one(candidate, &mut facts, ladder);
    if pending.entry.kind != ItemKind::Set && !candidate.cloud_only && memoisable(&facts) {
        remember_facts(env.cache, candidate, &facts);
    }
    Ok((facts, id))
}

fn identify_one(
    candidate: &WalkCandidate,
    facts: &mut Facts,
    ladder: &LadderEnv,
) -> Identification {
    let name = candidate
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    identify_candidate(
        &candidate.path,
        &name,
        candidate.size,
        facts,
        candidate.cloud_only,
        ladder,
    )
}

/// Version of the memoised facts: facts from before the archive reader
/// (no version) judged a 7z named `.zip` damaged, facts of version 1
/// judged a disc archive by its cartridge rules, and facts of version 2
/// lack the entry indices, the discs and the tool an extraction needs, so
/// they are read again; facts of version 4 call a patch-only archive an
/// unknown system.
/// 6: PS2, 3DO and Pico verdicts became systems, and a loose 3DO, PC-FX,
/// CD-i, CD32 or CDTV image is told by its disc signature (systems batch 1).
/// 7: Tanda 2 console ids on `Sniff.console`, and macOS installers and 3D
/// Studio models are no ROMs (systems batch 2 D23, D7).
/// 8: content ids, regions and Xbox product ids on `Sniff.console`
/// (systems batch 3 D28).
/// 9: game parts, title sources, homebrew, NCA ids, archive folder games
/// and disc counts (adjustments D33).
/// 10: system titles, emulator caches and configs, and the kinds, versions
/// and regions game folders read (game folders spec D11, D14, D18).
/// 11: addon verdicts (mods spec D10).
/// 12: 3DS bases and demos take their region from the verified letter
/// table and flag unknown letters (part C D12, P-1).
/// 13: music rips are `NotARom::Music`, never an unknown file (extras
/// spec D15).
pub(crate) const FACTS_VERSION: u32 = 13;

/// Facts as memoised, with their version.
#[derive(Serialize, Deserialize)]
struct StoredFacts {
    #[serde(default)]
    v: u32,
    #[serde(flatten)]
    facts: Facts,
}

fn memoised_facts(cache: &Cache, c: &WalkCandidate) -> Option<Facts> {
    let json = cache.organize_facts(&c.path, c.size, c.mtime_ns).ok()??;
    let stored: StoredFacts = serde_json::from_str(&json).ok()?;
    (stored.v == FACTS_VERSION).then_some(stored.facts)
}

/// The console verdict Organizar memoised for the file at `path` with
/// this `size` and `mtime_ns` (nanoseconds since the epoch), if any: the
/// first payload that has one (part C D3: the game panel reuses what a
/// plan already read, never writes facts of its own).
pub fn memoised_console(
    cache: &Cache,
    path: &Path,
    size: u64,
    mtime_ns: i64,
) -> Option<crate::consoles::ConsoleId> {
    let json = cache.organize_facts(path, size, mtime_ns).ok()??;
    let stored: StoredFacts = serde_json::from_str(&json).ok()?;
    (stored.v == FACTS_VERSION)
        .then_some(stored.facts)?
        .inspection
        .payloads
        .into_iter()
        .find_map(|p| p.sniff.console)
}

/// Facts that depend only on the file: not those cut short by a budget,
/// nor an archive that an external tool found later could open, nor one
/// an external tool listed or failed on (the tool, its version or the
/// setting may change while the file's size and mtime do not).
fn memoisable(facts: &Facts) -> bool {
    let insp = &facts.inspection;
    // A multi-volume set's parts are the files beside it, not this file.
    !insp.budget_spent
        && insp.tool.is_none()
        && !insp.notes.iter().any(|n| n == NOTE_TOOL_FAILED)
        && !matches!(
            insp.issue,
            Some(
                ArchiveIssue::Unsupported { need: Some(_), .. } | ArchiveIssue::MultiVolume { .. }
            )
        )
}

fn remember_facts(cache: &Cache, c: &WalkCandidate, facts: &Facts) {
    let stored = StoredFacts {
        v: FACTS_VERSION,
        facts: facts.clone(),
    };
    if let Ok(json) = serde_json::to_string(&stored) {
        let _ = cache.put_organize_facts(&c.path, c.size, c.mtime_ns, &json);
    }
}

/// A placeholder is judged by its name and extension only: never read.
fn name_only_facts(c: &WalkCandidate) -> Facts {
    let name = c
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Facts {
        inspection: Inspection {
            container: Container::Raw,
            payloads: vec![Payload {
                name,
                ext: c.ext.clone(),
                size: c.size,
                crc32: None,
                sniff: Sniff::default(),
            }],
            entries: Vec::new(),
            issue: None,
            notes: Vec::new(),
            head_crc: None,
            format: None,
            mismatch: None,
            listing_complete: true,
            budget_spent: false,
            disc_set: None,
            decoded: 0,
            tool: None,
            volume_parts: Vec::new(),
            folder_game: None,
        },
        raw_crcs: Vec::new(),
        hashed: true,
    }
}

/// A disc set is judged by its data track's header and extension; the
/// list file itself is never hashed.
fn set_facts(c: &WalkCandidate, entry: &SurveyEntry, sniff: Option<Sniff>) -> Facts {
    let track = entry
        .sources
        .iter()
        .skip(1)
        .find(|m| !is_list_extension(&extension(m)));
    let name = track
        .or(entry.sources.first())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Facts {
        inspection: Inspection {
            container: Container::Raw,
            payloads: vec![Payload {
                name,
                ext: track.map_or_else(|| c.ext.clone(), |t| extension(t)),
                size: entry.size,
                crc32: None,
                sniff: sniff.unwrap_or_default(),
            }],
            entries: Vec::new(),
            issue: None,
            notes: Vec::new(),
            head_crc: None,
            format: None,
            mismatch: None,
            listing_complete: true,
            budget_spent: false,
            disc_set: None,
            decoded: 0,
            tool: None,
            volume_parts: Vec::new(),
            folder_game: None,
        },
        raw_crcs: Vec::new(),
        hashed: true,
    }
}

/// `path` relative to `root` (unchanged when it is not inside).
fn rel(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
}

/// Builds the plan of `survey` under `opts` with the user's `decisions`.
/// Reads no pile ROM again: the target folders are listed (collisions),
/// files compared when a name is already taken, and the duplicates step
/// reads only files that share a size with another copy in their system
/// (the library side is cached when pile and library share a volume).
pub fn plan_from_survey(
    survey: &Survey,
    opts: &OrganizeOptions,
    decisions: &[Decision],
    env: &OrganizeEnv,
) -> Result<OrganizePlan, OrganizeError> {
    let (profile, _, _) = resolve_layout(&survey.source_root, opts, env)?;
    let target_root = match &opts.target {
        Some(t) => std::path::absolute(t).unwrap_or_else(|_| t.clone()),
        None => survey.target_root.clone(),
    };
    let roms_dir = roms_dir_of(&target_root, &profile, opts.flat);
    let src = &survey.source_root;
    super::translate_plan::check_emulator_data(opts, src, &target_root)?;

    // Switch names from the optional catalogue (adjustments D7, plan P-1).
    let named = with_switch_names(survey, env.titles);
    let survey = named.as_ref().unwrap_or(survey);

    // Items from the survey.
    let mut items: Vec<OrganizeItem> = Vec::with_capacity(survey.entries.len());
    let mut keys: Vec<Option<(String, u64)>> = Vec::with_capacity(survey.entries.len());
    for (id, entry) in survey.entries.iter().enumerate() {
        let mut item = OrganizeItem {
            id,
            sources: entry.sources.iter().map(|p| rel(src, p)).collect(),
            kind: entry.kind,
            size: entry.size,
            fps: entry.fps.clone(),
            target: None,
            set_targets: Vec::new(),
            system_id: None,
            folder: None,
            confidence: 0.0,
            method: None,
            why: Vec::new(),
            alternatives: Vec::new(),
            rename: None,
            notes: Vec::new(),
            companions: Vec::new(),
            catalogue_name: None,
            transfer: Transfer::AsIs,
            disc: None,
            action: OrganizeAction::Left {
                reason: LeftReason::Unknown,
            },
            title_id: None,
            content_id: None,
            playlist: None,
            title_source: None,
            game: None,
        };
        let mut key = None;
        match (&entry.left, &entry.identification) {
            (Some(reason), _) => {
                item.action = OrganizeAction::Left {
                    reason: reason.clone(),
                }
            }
            (None, Some(found)) => {
                let c = &found.combined;
                item.confidence = c.confidence;
                item.method = c.method;
                item.why = c.why.clone();
                item.alternatives = c.alternatives.clone();
                item.notes = found.notes.clone();
                item.catalogue_name = found.catalogue_name.clone();
                item.disc = found.disc.clone();
                key = found.content_key.clone();
                item.action = match (&found.left, c.verdict) {
                    (Some(reason), _) => OrganizeAction::Left {
                        reason: reason.clone(),
                    },
                    (None, Verdict::Auto) => OrganizeAction::Auto,
                    (None, Verdict::Review) => OrganizeAction::Review,
                    (None, Verdict::Unknown) => OrganizeAction::Left {
                        reason: LeftReason::Unknown,
                    },
                };
                if found.left.is_none() {
                    item.system_id = c.system.clone();
                }
                // The header's title id, for the «why» line (spec §10),
                // when the header names the system the item goes to.
                let header = entry
                    .console
                    .as_ref()
                    .filter(|con| c.system.as_deref() == Some(con.system.as_str()));
                item.title_id = header.and_then(crate::consoles::TitleIdRef::of);
                item.title_source = header.and_then(|con| con.title_source).filter(|s| {
                    matches!(
                        s,
                        crate::consoles::TitleSource::Titledb | crate::consoles::TitleSource::Nlib
                    )
                });
                // «Cifrado» once the catalogue and the DATs had their say
                // (D11); a DAT game name is a resolved name.
                if let Some(con) = header {
                    super::evidence::note_encrypted_name(
                        &mut item.notes,
                        con,
                        item.catalogue_name.as_deref(),
                    );
                }
                // A base game's catalogue name stands for a DAT name (D7);
                // the target-name sanitiser cleans it like any other.
                if item.catalogue_name.is_none() {
                    item.catalogue_name = header
                        .filter(|con| con.kind == TitleKind::Base && con.part.is_none())
                        .and_then(catalogue_title);
                }
                // The content id the same header read (systems batch 3 §10).
                item.content_id = header.and_then(|con| con.content_id.clone());
                // Homebrew goes to its system folder with a note (D13).
                if !item.is_left()
                    && header.is_some_and(|con| con.kind == TitleKind::Homebrew)
                    && !item.notes.contains(&ItemNote::Homebrew)
                {
                    item.notes.push(ItemNote::Homebrew);
                }
                // A 3DS region letter the verified table does not know
                // (part C P-1): said, never guessed.
                if let Some(flag) = header.and_then(|con| con.region_letter.as_ref()) {
                    let note = ItemNote::RegionLetterUnknown {
                        letter: flag.letter,
                    };
                    if !item.is_left() && !item.notes.contains(&note) {
                        item.notes.push(note);
                    }
                }
                // A part's title id, once the catalogue had its say (D6).
                if let OrganizeAction::Left {
                    reason:
                        LeftReason::GamePart {
                            title_id: title_id @ None,
                            ..
                        },
                } = &mut item.action
                {
                    *title_id = entry.console.as_ref().and_then(|con| con.title_id.clone());
                }
            }
            (None, None) => {}
        }
        items.push(item);
        keys.push(key);
    }
    overlay_titles(&mut items, survey);

    // The user's decisions.
    for decision in decisions {
        let Some(item) = items.get_mut(decision.item) else {
            return Err(OrganizeError::UnknownItem(decision.item));
        };
        match &decision.choice {
            Choice::Assign { system } => {
                if env.db.get(system).is_none() {
                    return Err(OrganizeError::UnknownSystem(system.clone()));
                }
                if !decidable(item) {
                    continue;
                }
                item.why
                    .insert(0, Evidence::new(IdLevel::User, system, 1.0, ""));
                item.alternatives.retain(|(s, _)| s != system);
                item.system_id = Some(system.clone());
                item.method = Some(IdLevel::User);
                item.confidence = 1.0;
                item.action = OrganizeAction::Auto;
                if let Some((crc, size)) = &keys[item.id] {
                    let _ = env.cache.put_organize_choice(crc, *size, system);
                }
            }
            Choice::Accept => {
                if !decidable(item) {
                    continue;
                }
                if item.system_id.is_none() {
                    if let Some((best, weight)) = item.alternatives.first().cloned() {
                        item.alternatives.remove(0);
                        item.system_id = Some(best);
                        item.confidence = weight;
                    }
                }
                if item.system_id.is_some() {
                    item.action = OrganizeAction::Auto;
                }
            }
            Choice::Leave => item.leave(LeftReason::Unknown),
            Choice::Decline => item.leave(LeftReason::NotConfirmed),
        }
    }

    // shadPS4 overlays follow their base's action, decisions included
    // (systems batch 3 D11 e, plan P-4).
    let pairs = overlay_pairs(
        &items,
        &|i| {
            survey
                .entries
                .get(i)
                .and_then(|e| e.console.as_ref())
                .is_some_and(|c| c.system == "ps4" && c.kind == TitleKind::Update)
        },
        &|i| {
            // A PS4 folder game extracted out of an archive (adjustments
            // spec D23) is known by its root folder's name, else the
            // archive stem.
            let entry = survey.entries.get(i)?;
            let Some(ArchiveGame::One { root, system, .. }) =
                entry.inspection.as_ref()?.folder_game.as_ref()
            else {
                return None;
            };
            if system != "ps4" {
                return None;
            }
            match root.rsplit('/').next().filter(|r| !r.is_empty()) {
                Some(last) => Some(last.to_string()),
                None => items
                    .get(i)?
                    .sources
                    .first()?
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned()),
            }
        },
    );
    let titles: HashMap<usize, String> = pairs
        .iter()
        .map(|pair| {
            let of = items[pair.base]
                .sources
                .first()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let base = survey.entries.get(pair.base).zip(items.get(pair.base));
            (pair.base, base_title(base, &of))
        })
        .collect();
    let title_of = |b: usize| titles.get(&b).cloned().unwrap_or_default();
    siblings_follow(&mut items, &pairs, &title_of);

    // Mode: archives are kept in copy mode.
    let mut warnings: Vec<PlanWarning> = Vec::new();
    let move_allowed = dir_writable(src);
    let mut mode = opts.mode.unwrap_or(if same_volume(src, &target_root) {
        TransferMode::Move
    } else {
        TransferMode::Copy
    });
    if !move_allowed && mode == TransferMode::Move {
        mode = TransferMode::Copy;
        warnings.push(PlanWarning::MoveUnavailable);
    }

    // Duplicates (against the pile and the target's system folders),
    // extractions, then targets. A copy set aside is never extracted.
    let dups = match opts.duplicates {
        OrganizeDedupeMode::Off => DupStep::default(),
        mode => duplicates_step(
            survey,
            &mut items,
            (&profile, &target_root, &roms_dir),
            mode,
            opts,
            env,
        ),
    };
    let mut shapes: Vec<Option<ExtractShape>> = vec![None; items.len()];
    for item in items.iter_mut().filter(|i| i.moves_in()) {
        let entry = &survey.entries[item.id];
        let (Some(system), Some(insp)) = (item.system_id.clone(), entry.inspection.as_ref()) else {
            continue;
        };
        let Some(system) = env.db.get(&system) else {
            continue;
        };
        match plan_transfer(item, insp, system, &profile, mode) {
            Ok(shape) => shapes[item.id] = shape,
            Err(reason) => item.leave(reason),
        }
    }
    // Loose multi-disc sets are laid out like an extracted multi-disc
    // archive (adjustments D24); PS2, Xbox and Xbox 360 get no playlist.
    for item in items
        .iter()
        .filter(|i| i.moves_in() && i.kind == ItemKind::Set)
    {
        let Some(set) = survey
            .entries
            .get(item.id)
            .and_then(|e| e.disc_set.as_ref())
        else {
            continue;
        };
        let playlist = !item
            .system_id
            .as_deref()
            .is_some_and(|s| NO_PLAYLIST.contains(&s));
        shapes[item.id] = Some(ExtractShape::LooseDiscs {
            discs: set.discs.iter().map(|d| rel(src, d)).collect(),
            title: set.title.clone(),
            playlist,
        });
    }
    let fat32 = file_system_is_fat32(&target_root);
    let game_folders = opts.game_folders.unwrap_or(profile.game_folders);
    let layout = TargetLayout {
        target_root: target_root.clone(),
        roms_dir: roms_dir.clone(),
        profile,
        overrides: opts.folder_names.clone(),
        fat32,
        windows_paths: windows_paths(),
        names: opts.names,
        companion_exts: opts.companion_exts.clone(),
        game_folders,
    };
    // A FAT32 member too big is known before any name is claimed (D27).
    if fat32 {
        for item in items.iter_mut().filter(|i| i.moves_in()) {
            let tree = survey.entries.get(item.id).and_then(|e| e.tree.as_ref());
            if tree.is_some_and(|t| folder_gate(Path::new(""), t, true, false).is_some()) {
                item.leave(LeftReason::TooBigForFat);
            }
        }
    }
    let same_content = |existing: &Path, item: &OrganizeItem| -> bool {
        !matches!(item.kind, ItemKind::Set | ItemKind::Folder)
            && item
                .sources
                .first()
                .is_some_and(|s| same_bytes(existing, &src.join(s)))
    };
    // A frontend's DLC folder (Batocera `ps4/DLC/<TITLE_ID>/`, D16): DLC
    // folders and `.zar` files only; a DLC PKG stays in the system folder.
    let mut subdirs: Vec<Option<Subdir>> = items
        .iter()
        .map(|item| {
            let c = survey.entries.get(item.id)?.console.as_ref()?;
            let zar = item.kind == ItemKind::File
                && item
                    .sources
                    .first()
                    .and_then(|p| p.extension())
                    .is_some_and(|e| e.eq_ignore_ascii_case("zar"));
            // A folder game extracted out of an archive lands as a folder
            // too (adjustments spec D23).
            let extracted = matches!(shapes.get(item.id), Some(Some(ExtractShape::Folder { .. })))
                && matches!(item.transfer, Transfer::Extract { .. });
            let placeable = item.kind == ItemKind::Folder || zar || extracted;
            if c.kind != TitleKind::Dlc
                || !placeable
                || item.system_id.as_deref() != Some(c.system.as_str())
                || !(matches!(item.transfer, Transfer::AsIs) || extracted)
            {
                return None;
            }
            dlc_target(&layout, &c.system, &base_title_id(c)?).map(Subdir::Dlc)
        })
        .collect();
    let mut index = DirIndex::new();
    // Game folders (game folders spec D5, plan P-3): bases get their
    // `<Game>` before targets, children after.
    let siblings: BTreeSet<usize> = pairs.iter().map(|p| p.sibling).collect();
    let mut dlc_names = if layout.game_folders {
        link_game_folders(
            &mut items,
            &survey.entries,
            &shapes,
            &mut subdirs,
            &siblings,
            &layout,
            env.db,
            &mut index,
        )
    } else {
        HashMap::new()
    };
    // Demos as their own game and orphan children (D12, D13, plan P-4).
    link_demos_and_orphans(
        &mut items,
        &survey.entries,
        &shapes,
        &mut subdirs,
        &siblings,
        &layout,
        (env, &opts.limits),
        &mut index,
        &mut dlc_names,
    );
    // Mods, patches and cheats to their game (mods spec D5–D9, D13).
    {
        let mut budget =
            LookupBudget::new(opts.limits.plan_head_budget, opts.limits.plan_wall_time);
        let mut library = |system: &str| {
            let (system_dir, _) = folder_dir(&layout, env.db, system);
            let mut read = |path: &Path, meta: &fs::Metadata| {
                library_header(path, meta, env, &opts.limits, &mut budget)
            };
            library_bases(&system_dir, system, env.db, true, &mut read)
        };
        let mut world = super::addons::LinkWorld {
            layout: &layout,
            db: env.db,
            hash_budget: opts.limits.plan_head_budget,
            library: &mut library,
        };
        super::addons::link_addons(
            &mut items,
            &survey.entries,
            &survey.walked.addons,
            &mut subdirs,
            &mut world,
        );
    }
    let mut left_companions = assign_targets_in(
        &mut items,
        &shapes,
        &subdirs,
        &layout,
        env.db,
        src,
        &survey.walked.companions,
        &mut index,
        &same_content,
    );
    // Overlay names from the base's final name (D11 b).
    sibling_targets(&mut items, &pairs, &layout);
    if layout.game_folders {
        game_children_targets(&mut items, &layout, &mut index, &same_content, &|j| {
            dlc_names.get(&j).cloned()
        });
        super::addons::fixed_folder_companions(&mut items, &layout, &mut index);
    }
    super::addons::addon_targets(&mut items, &layout, &mut index, src, &same_content);
    // Each system laid out for its emulator (mods spec D16).
    let translated = super::translate_plan::apply(
        &mut items,
        &super::translate_plan::TranslateWorld {
            layout: &layout,
            db: env.db,
            opts,
            entries: &survey.entries,
            addons: &survey.walked.addons,
            src,
        },
        &mut index,
        &same_content,
    );
    // A folder game's deepest member must fit below its target (D27).
    for item in items.iter_mut().filter(|i| i.moves_in()) {
        let (Some(target), Some(tree)) = (
            item.target.as_ref(),
            survey.entries.get(item.id).and_then(|e| e.tree.as_ref()),
        ) else {
            continue;
        };
        if let Some(reason) =
            folder_gate(&target_root.join(target), tree, false, layout.windows_paths)
        {
            item.leave(reason);
        }
    }
    left_companions.extend(attach_chd_folders(
        &mut items,
        &survey.walked.chd_folders,
        src,
        &target_root,
        env.db,
        fat32,
    ));
    if fat32 {
        limit_fat_folders(&mut items, &target_root);
    }
    // A base a gate left after its targets holds its siblings too.
    siblings_follow(&mut items, &pairs, &title_of);
    if layout.game_folders {
        game_children_follow(&mut items);
        // A licence goes with its game only by a rename (D17): move mode
        // with the target on the pile's volume.
        if mode == TransferMode::Move && same_volume(src, &target_root) {
            licence_companions(&mut items, &layout, &mut index);
        }
    }
    // An update's or DLC's own art follows it, or stays and is listed
    // (part C D10 d, D16).
    let is_child = |it: &OrganizeItem| {
        it.game
            .as_ref()
            .is_some_and(|g| matches!(g.role, GameRole::Update | GameRole::Dlc))
            || survey
                .entries
                .get(it.id)
                .is_some_and(|e| matches!(effective_kind(e), TitleKind::Update | TitleKind::Dlc))
    };
    child_art_companions(&mut items, &layout, src, &is_child, &mut index);
    point_at_keepers(&mut items, &dups.keepers);
    attach_set_aside_companions(
        &mut items,
        &survey.walked.companions,
        src,
        &opts.companion_exts,
    );
    for (path, reason) in left_companions {
        let abs = if path.is_absolute() {
            path.clone()
        } else {
            src.join(&path)
        };
        let fp = fp_of(&abs);
        items.push(OrganizeItem {
            id: items.len(),
            sources: vec![rel(src, &abs)],
            kind: ItemKind::File,
            size: fp.size,
            fps: vec![fp],
            target: None,
            set_targets: Vec::new(),
            system_id: None,
            folder: None,
            confidence: 0.0,
            method: None,
            why: Vec::new(),
            alternatives: Vec::new(),
            rename: None,
            notes: Vec::new(),
            companions: Vec::new(),
            catalogue_name: None,
            transfer: Transfer::AsIs,
            disc: None,
            action: OrganizeAction::Left { reason },
            title_id: None,
            content_id: None,
            playlist: None,
            title_source: None,
            game: None,
        });
    }

    // Warnings, counts.
    let missing: BTreeSet<String> = items
        .iter()
        .filter(|i| i.moves_in())
        .flat_map(|i| i.notes.iter())
        .filter_map(|n| match n {
            ItemNote::CatalogueMissing { system } => Some(system.clone()),
            _ => None,
        })
        .collect();
    warnings.extend(
        missing
            .into_iter()
            .map(|system| PlanWarning::CatalogueMissing { system }),
    );
    let target_detected = detect(&target_root)
        .into_iter()
        .next()
        .map(|d| d.profile_id)
        .filter(|id| *id != opts.layout);
    if let Some(profile) = &target_detected {
        warnings.push(PlanWarning::TargetLayoutMismatch {
            profile: profile.clone(),
        });
    }
    let candidates = survey
        .entries
        .iter()
        .filter(|e| e.identification.is_some() || e.kind == ItemKind::Set)
        .count();
    if candidates == 0 && !survey.walked.sorted_folders.is_empty() {
        warnings.push(PlanWarning::SortedOnly);
    }
    let copies = mode == TransferMode::Copy || !same_volume(src, &target_root);
    let cluster = cluster_size(&target_root).unwrap_or(DEFAULT_CLUSTER);
    let needed = space_needed(&items, src, copies, cluster);
    if let Some(free) = user_available(&target_root).filter(|&free| free < needed) {
        warnings.push(PlanWarning::NoSpace { needed, free });
    }
    let updates = note_updates(&mut items, &survey.entries);
    let mut counts = count(&items, &survey.walked);
    counts.in_library = dups.in_library;

    let target_anchor = super::roots::anchor_of(&target_root, &super::roots::OsRootProbe);
    let mut plan = OrganizePlan {
        source_root: src.clone(),
        target_root,
        roms_dir,
        layout: opts.layout.clone(),
        mode,
        names: opts.names,
        items,
        counts,
        sorted_folders: survey
            .walked
            .sorted_folders
            .iter()
            .map(|(p, s)| (rel(src, p), s.clone()))
            .collect(),
        deep_folders: survey
            .walked
            .deep_folders
            .iter()
            .map(|p| rel(src, p))
            .collect(),
        other_files: survey
            .walked
            .other_files
            .iter()
            .map(|p| rel(src, p))
            .collect(),
        warnings,
        broad_root: survey.broad_root,
        fat32,
        move_allowed,
        target_detected,
        created_at: crate::time::rfc3339_utc(env.now),
        set_aside_trash: opts.duplicates == OrganizeDedupeMode::SetAside
            && opts.dedupe.quarantine == QuarantineChoice::Trash,
        duplicates: dups.groups,
        updates,
        integration_notices: Vec::new(),
        target_anchor,
        translated: translated.views,
        emulator_roots: translated.roots,
    };
    // Informational: the gate runs on the decided plan at apply time; a
    // folder game extraction under the threshold is asked then, with no
    // warning (adjustments spec D15).
    if let Some(t) =
        large_transfer(&plan).filter(|t| t.bytes >= super::apply::large_transfer_threshold())
    {
        plan.warnings.push(PlanWarning::LargeTransfer {
            bytes: t.bytes,
            items: t.items,
        });
    }
    Ok(plan)
}

// --- The duplicates step (duplicates spec §4, §7.1) ---

/// What the duplicates step decided.
#[derive(Default)]
struct DupStep {
    /// Groups with at least one pile copy.
    groups: Vec<DupGroup>,
    /// `(extra, keeper)` item ids whose keeper is a pile copy: the extra
    /// points at where the keeper lands once targets are known.
    keepers: Vec<(usize, usize)>,
    /// Pile copies of games already in the library.
    in_library: usize,
}

/// Groups the pile items that move with each other and with the target's
/// system folders they go to, with the duplicates keys and keeper rules,
/// and settles the pile copies that are extras: set aside on the pile's
/// side (`SetAside` mode, groups that act) or left with
/// [`LeftReason::InLibrary`] (reported). Reads the library's colliding
/// files (cached by volume when pile and library share one); pile
/// archives answer from their inspection.
fn duplicates_step(
    survey: &Survey,
    items: &mut [OrganizeItem],
    (profile, target_root, roms_dir): (&Profile, &Path, &Path),
    mode: OrganizeDedupeMode,
    opts: &OrganizeOptions,
    env: &OrganizeEnv,
) -> DupStep {
    let src = &survey.source_root;
    let wanted = |system: &str| {
        opts.dedupe
            .systems
            .as_ref()
            .is_none_or(|list| list.iter().any(|s| s == system))
    };

    // Pile units, from what the survey read.
    let mut facts: Vec<(PathBuf, Facts)> = Vec::new();
    let mut fact_systems: Vec<String> = Vec::new();
    let mut sets: Vec<(Vec<PathBuf>, String)> = Vec::new();
    let mut folders: Vec<(PathBuf, String, u64, i64)> = Vec::new();
    let mut by_primary: HashMap<PathBuf, usize> = HashMap::new();
    let mut listed: HashMap<PathBuf, Vec<ListedEntry>> = HashMap::new();
    let mut discs: HashMap<PathBuf, Option<DiscId>> = HashMap::new();
    // Mods and cheats are never units (mods spec D10): they belong to
    // their game.
    let addon = |i: &OrganizeItem| {
        i.game
            .as_ref()
            .is_some_and(|g| matches!(g.role, GameRole::Mod | GameRole::Cheat))
    };
    for item in items.iter().filter(|i| i.moves_in() && !addon(i)) {
        let (Some(system), Some(entry)) = (&item.system_id, survey.entries.get(item.id)) else {
            continue;
        };
        let Some(primary) = entry.sources.first().cloned() else {
            continue;
        };
        if !wanted(system) {
            continue;
        }
        match (entry.kind, &entry.facts) {
            (ItemKind::Set, _) => sets.push((entry.sources.clone(), system.clone())),
            // A folder game is a unit too (D28): its title id or its tree
            // matches it, and it is set aside whole (adjustments D19).
            (ItemKind::Folder, _) => folders.push((
                primary.clone(),
                system.clone(),
                entry.size,
                entry.fps.first().map_or(0, |fp| fp.mtime_ns),
            )),
            (_, Some(f)) => {
                if f.inspection.container.is_archive() {
                    listed.insert(primary.clone(), listed_entries(&f.inspection));
                    discs.insert(primary.clone(), item.disc.clone());
                }
                facts.push((primary.clone(), f.clone()));
                fact_systems.push(system.clone());
            }
            _ => continue,
        }
        by_primary.insert(primary, item.id);
    }
    if by_primary.is_empty() {
        return DupStep::default();
    }
    let mut units = units_from_facts(&facts, &|i| fact_systems.get(i).cloned()).units;
    units.extend(units_from_sets(&sets).units);
    units.extend(units_from_folders(&folders).units);
    // Folder trees the survey measured (spec D19): a folder game's from
    // its metadata walk, an archive folder game's from its listing.
    for u in &mut units {
        let Some(entry) = by_primary
            .get(&u.primary)
            .and_then(|&id| survey.entries.get(id))
        else {
            continue;
        };
        u.tree_digest = match (&entry.tree, &entry.facts) {
            (Some(tree), _) => tree.digest.clone(),
            (None, Some(f)) => crate::organize::inspect::archive_tree_digest(&f.inspection),
            _ => None,
        };
        // An archive folder game: the title its listing's probe read.
        if u.kind != crate::dedupe::UnitKind::Folder && u.tree_digest.is_some() {
            u.tree_title = entry
                .console
                .as_ref()
                .and_then(|c| crate::dedupe::keys::title_key_string(c, &u.system));
        }
    }
    // What the survey's readers found stands for level 1's title keys: a
    // header or folder probe is opened once (systems batch 3 D24).
    for u in &units {
        let console = by_primary
            .get(&u.primary)
            .and_then(|&id| survey.entries.get(id))
            .and_then(|e| e.console.as_ref());
        if let Some(c) = console {
            crate::dedupe::keys::remember_title_key(env.cache, u, c);
        }
    }

    // Library units: only the system folders the pile goes to.
    let listing = ArchiveEntryListing::with_tools(opts.limits.clone(), &[src, target_root]);
    let systems: BTreeSet<String> = units.iter().map(|u| u.system.clone()).collect();
    if roms_dir.is_dir() && !systems.is_empty() {
        let scan_profile = Profile {
            roms_root: String::new(),
            ..profile.clone()
        };
        let mut scan_opts = ScanOptions::default();
        if !scan_opts
            .quarantine_names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&opts.quarantine_name))
        {
            scan_opts
                .quarantine_names
                .push(opts.quarantine_name.clone());
        }
        let only: Vec<String> = systems.into_iter().collect();
        let scan = scan_systems(roms_dir, &scan_profile, env.db, &scan_opts, &only);
        let mut unit_opts = UnitOptions::for_profile(profile);
        unit_opts.quarantine = Some(target_root.join(&opts.quarantine_name));
        let library = units_from_scan(&scan, env.db, &unit_opts, &listing).units;
        // A pile inside the library is never compared with itself, even
        // when it was reached by another spelling or through a link: a
        // library file that is a pile file is no other copy of it.
        let pile_files: std::collections::HashSet<FileKey> = units
            .iter()
            .flat_map(|u| u.files.iter())
            .filter_map(|f| file_key(f))
            .collect();
        units.extend(library.into_iter().filter(|u| {
            !by_primary.contains_key(&u.primary)
                && !u
                    .files
                    .iter()
                    .any(|f| file_key(f).is_some_and(|k| pile_files.contains(&k)))
        }));
    }

    // Group and choose the keepers.
    let pile_listing = PileListing {
        listed,
        real: &listing,
    };
    let reader = ReaderDiscProbe::new(ArchiveEntryListing::with_tools(
        opts.limits.clone(),
        &[src, target_root],
    ));
    let pile_discs = PileDiscs {
        discs,
        real: &reader,
    };
    let dats = CacheDats::new(env.cache, env.db);
    let volume = same_volume(src, target_root)
        .then(|| crate::volumes::volume_uuid(target_root))
        .flatten();
    let cancel = AtomicBool::new(false);
    let genv = GroupEnv {
        cache: env.cache,
        db: env.db,
        profile,
        root: target_root,
        roms_dir,
        listing: &pile_listing,
        discs: &pile_discs,
        dats: &dats,
        volume_uuid: volume.as_deref(),
        companion_exts: &opts.companion_exts,
        regions: &opts.regions,
        cancel: &cancel,
    };
    let groups = group_units(units, &opts.dedupe, &genv).unwrap_or_default();

    // Settle the pile extras.
    let quarantine = pile_quarantine(
        src,
        target_root,
        roms_dir,
        &opts.quarantine_name,
        env.db,
        profile,
    );
    let mut out = DupStep::default();
    for g in groups {
        if g.members.iter().all(|m| m.unit.in_library) {
            continue;
        }
        let keeper = &g.members[g.keeper];
        let has_library = g.members.iter().any(|m| m.unit.in_library);
        let keeper_item = if keeper.unit.in_library {
            None
        } else {
            by_primary.get(&keeper.unit.primary).copied()
        };
        let keeper_path = if keeper.unit.in_library {
            rel(target_root, &keeper.unit.primary)
        } else {
            rel(src, &keeper.unit.primary)
        };
        let acts = g.action == GroupAction::Act;
        // A copy only a CRC matched is still never copied over the
        // library's; other report-only groups (another header, build or
        // version, a conflict) move as usual.
        let unconfirmed = matches!(
            g.action,
            GroupAction::ReportOnly {
                why: ReportWhy::Unconfirmed
            }
        );
        for m in &g.members {
            if m.id == g.keeper || m.unit.in_library {
                continue;
            }
            let Some(&id) = by_primary.get(&m.unit.primary) else {
                continue;
            };
            let Some(item) = items.get_mut(id) else {
                continue;
            };
            if acts && mode == OrganizeDedupeMode::SetAside {
                let dest = quarantine.join(&item.sources[0]);
                set_aside(
                    item,
                    OrganizeAction::SetAside {
                        keeper: keeper_path.clone(),
                        relation: m.relation,
                        reason: g.reason.clone(),
                        dest,
                    },
                );
            } else if (acts || (unconfirmed && has_library)) && m.relation != Relation::Version {
                item.leave(LeftReason::InLibrary {
                    path: keeper_path.clone(),
                    relation: m.relation,
                });
            } else {
                continue;
            }
            if has_library {
                out.in_library += 1;
            }
            if let Some(k) = keeper_item {
                out.keepers.push((id, k));
            }
        }
        out.groups.push(g);
    }
    out
}

/// Where a pile's extras go: `<name>` inside the pile (relative to it),
/// or, for a pile inside one of the library's system folders, the
/// library's quarantine keeping the system folder's shape
/// (`<target>/<name>/GB/incoming`, absolute): a quarantine is never inside
/// a system folder, where the device would list its games.
fn pile_quarantine(
    src: &Path,
    target_root: &Path,
    roms_dir: &Path,
    name: &str,
    db: &SystemDb,
    profile: &Profile,
) -> PathBuf {
    let in_system_folder = src.strip_prefix(roms_dir).ok().is_some_and(|rel| {
        rel.components().next().is_some_and(|c| match c {
            std::path::Component::Normal(folder) => db
                .resolve_folder_in(Some(profile), &folder.to_string_lossy())
                .is_some(),
            _ => false,
        })
    });
    match src.strip_prefix(roms_dir) {
        Ok(rel) if in_system_folder => target_root.join(name).join(rel),
        _ => PathBuf::from(name),
    }
}

/// What makes two paths the same file: the device and inode (Unix), or
/// the canonical path ignoring case (elsewhere).
#[cfg(unix)]
type FileKey = (u64, u64);
#[cfg(not(unix))]
type FileKey = String;

#[cfg(unix)]
fn file_key(path: &Path) -> Option<FileKey> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

#[cfg(not(unix))]
fn file_key(path: &Path) -> Option<FileKey> {
    fs::canonicalize(path)
        .ok()
        .map(|p| p.to_string_lossy().to_lowercase())
}

/// Sets `item` aside with `action` (a [`OrganizeAction::SetAside`]): it
/// gets no target in the library, and nothing travels with it.
fn set_aside(item: &mut OrganizeItem, action: OrganizeAction) {
    item.action = action;
    item.target = None;
    item.set_targets.clear();
    item.companions.clear();
    item.rename = None;
    item.transfer = Transfer::AsIs;
}

/// Points each extra whose keeper is a pile copy at where that copy is
/// organised to (relative to the target root), or, when it stays on the
/// pile, at its place there.
fn point_at_keepers(items: &mut [OrganizeItem], keepers: &[(usize, usize)]) {
    for &(extra, keeper) in keepers {
        let Some(k) = items.get(keeper) else {
            continue;
        };
        let at = match (&k.target, k.moves_in()) {
            (Some(target), true) => target.clone(),
            _ => match k.sources.first() {
                Some(source) => source.clone(),
                None => continue,
            },
        };
        match items.get_mut(extra).map(|i| &mut i.action) {
            Some(OrganizeAction::SetAside { keeper, .. }) => *keeper = at,
            Some(OrganizeAction::Left {
                reason: LeftReason::InLibrary { path, .. },
            }) => *path = at,
            _ => {}
        }
    }
}

/// The file entries of an inspected archive as a listing states them.
fn listed_entries(insp: &Inspection) -> Vec<ListedEntry> {
    insp.entries
        .iter()
        .map(|e| ListedEntry {
            name: e.name.clone(),
            size: e.size,
            crc32: e
                .crc32
                .as_deref()
                .and_then(|c| u32::from_str_radix(c, 16).ok()),
        })
        .collect()
}

/// Pile archives listed from their organise inspection (the CRCs the
/// survey read); everything else, and every decode, by the archive
/// reader. Entry `n` of a pile listing is matched to the reader's entry
/// by name and size.
struct PileListing<'a> {
    listed: HashMap<PathBuf, Vec<ListedEntry>>,
    real: &'a ArchiveEntryListing,
}

impl PileListing<'_> {
    /// The reader's index of pile entry `entry` of `path` (`entry` itself
    /// for an archive not on the pile).
    fn real_index(&self, path: &Path, entry: usize) -> io::Result<Option<usize>> {
        let Some(ours) = self.listed.get(path) else {
            return Ok(Some(entry));
        };
        let Some(want) = ours.get(entry) else {
            return Ok(None);
        };
        Ok(self.real.list(path)?.and_then(|real| {
            real.iter()
                .position(|e| e.name == want.name && e.size == want.size)
        }))
    }
}

impl EntryListing for PileListing<'_> {
    fn list(&self, path: &Path) -> io::Result<Option<Vec<ListedEntry>>> {
        match self.listed.get(path) {
            Some(entries) => Ok(Some(entries.clone())),
            None => self.real.list(path),
        }
    }

    fn payload_sha1(
        &self,
        path: &Path,
        entry: usize,
        rule: HeaderRule,
        budget: u64,
    ) -> io::Result<Option<String>> {
        match self.real_index(path, entry)? {
            Some(n) => self.real.payload_sha1(path, n, rule, budget),
            None => Ok(None),
        }
    }

    fn head(&self, path: &Path, entry: usize, len: usize) -> io::Result<Option<Vec<u8>>> {
        match self.real_index(path, entry)? {
            Some(n) => self.real.head(path, n, len),
            None => Ok(None),
        }
    }
}

/// Pile archives answer with the disc identity their inspection read
/// (`OrganizeItem::disc`); loose discs and the library are read.
struct PileDiscs<'a> {
    discs: HashMap<PathBuf, Option<DiscId>>,
    real: &'a ReaderDiscProbe,
}

impl DiscProbe for PileDiscs<'_> {
    fn probe(&self, path: &Path, system: &str) -> io::Result<Option<DiscId>> {
        match self.discs.get(path) {
            Some(id) => Ok(id.clone()),
            None => self.real.probe(path, system),
        }
    }
}

/// Leftover names a note shows.
const LEFTOVER_EXAMPLES: usize = 3;

/// Kept free on the target beyond what a plan writes (archives spec §8.4).
pub const SPACE_RESERVE: u64 = 64 * 1024 * 1024;
/// The allocation unit assumed when the volume does not tell its own.
const DEFAULT_CLUSTER: u64 = 4096;

/// Bytes the target volume needs free for the items that move: every
/// extracted entry, generated playlist and, when `copies` (copy mode or
/// another volume), every source file and companion, each rounded up to
/// whole `cluster`s; plus 1 % and [`SPACE_RESERVE`]. Nothing to write
/// needs nothing. A move on one volume renames, and the Trash frees no
/// space on it, so an archive's own size is never taken off (spec §8.4).
pub fn space_needed(items: &[OrganizeItem], source_root: &Path, copies: bool, cluster: u64) -> u64 {
    let cluster = cluster.max(1);
    let round = |n: u64| n.div_ceil(cluster).saturating_mul(cluster);
    let mut sum: u64 = 0;
    for item in items.iter().filter(|i| i.moves_in() && i.target.is_some()) {
        let mut add = match &item.transfer {
            // A playlist is a few lines: one cluster.
            Transfer::Extract { entries, m3u, .. } => entries
                .iter()
                .map(|e| round(e.size))
                .chain(m3u.as_ref().map(|_| round(1)))
                .fold(0u64, u64::saturating_add),
            Transfer::AsIs if copies => item
                .fps
                .iter()
                .map(|fp| round(fp.size))
                .fold(0u64, u64::saturating_add),
            Transfer::AsIs => 0,
        };
        // A loose set's playlist is a few lines too.
        if item.playlist.is_some() {
            add = add.saturating_add(round(1));
        }
        if copies {
            for c in &item.companions {
                let size = fs::metadata(source_root.join(&c.source)).map_or(0, |m| m.len());
                add = add.saturating_add(round(size));
            }
        }
        sum = sum.saturating_add(add);
    }
    if sum == 0 {
        return 0;
    }
    sum.saturating_add(sum.div_ceil(100))
        .saturating_add(SPACE_RESERVE)
}

/// How an archive item reaches `system`'s folder under `profile`
/// (archives spec §8.1–§8.3): `Ok(None)` moves it as it is; `Ok(Some)`
/// sets `item.transfer` to an extraction (targets assigned later) and
/// returns its layout; `Err` leaves the item.
///
/// - Arcade sets are never extracted: a zip moves as it is, any other
///   container is `RepackArcade`.
/// - A disc archive always extracts its set (sheets, tracks, images).
/// - A cartridge archive moves as it is when the profile's emulators load
///   its container for the system, else its ROM entries are extracted.
/// - An incomplete listing is never extracted: the archive moves as it is.
/// - Move mode sends the archive to the Trash only when every file entry
///   is extracted or OS junk (an unsafe name
///   never counts as junk); anything else (a save, a readme) keeps it,
///   with a note. Copy mode always keeps it.
fn plan_transfer(
    item: &mut OrganizeItem,
    insp: &Inspection,
    system: &System,
    profile: &Profile,
    mode: TransferMode,
) -> Result<Option<ExtractShape>, LeftReason> {
    let Some(format) = insp.format else {
        return Ok(None);
    };
    // Before the listing rule: an arcade set that is not a zip never
    // loads, however much of it was listed.
    if system.arcade {
        return match insp.container {
            Container::Zip => Ok(None),
            _ => Err(LeftReason::RepackArcade),
        };
    }
    // Entries past an incomplete listing (a tar cut by the listing budget)
    // are unknown: such an archive moves as it is, never extracted (spec
    // §4.2).
    if !insp.listing_complete {
        return Ok(None);
    }
    // A folder game inside the archive (adjustments spec D23).
    if let Some(root) = insp.folder_game.as_ref().and_then(ArchiveGame::root) {
        return plan_folder_extract(item, insp, root, system, mode);
    }
    let by_name = |name: &str| insp.entries.iter().find(|e| e.name == name);
    let (chosen, shape, sheet_check): (Vec<&ZipEntryInfo>, ExtractShape, SheetCheck) =
        match &insp.disc_set {
            Some(set) => {
                let chosen = set.members.iter().filter_map(|m| by_name(m)).collect();
                let check = if set.extractable {
                    set.sheet_check
                } else {
                    SheetCheck::Deferred
                };
                let shape = ExtractShape::Disc {
                    discs: set.discs.clone(),
                    title: set.title.clone(),
                    multi_disc: set.multi_disc,
                };
                if profile.disc_layout.is_game_folder() {
                    // One folder per game: menus list it once.
                    item.notes
                        .retain(|n| !matches!(n, ItemNote::TwinSheets { .. }));
                }
                (chosen, shape, check)
            }
            None => {
                if profile.accepts_as_is(system, ContainerKind::of(format)) {
                    return Ok(None);
                }
                let chosen: Vec<&ZipEntryInfo> = insp
                    .payloads
                    .iter()
                    .filter_map(|p| by_name(&p.name))
                    .collect();
                if chosen.is_empty() {
                    return Ok(None);
                }
                if chosen.len() > 1 {
                    item.notes
                        .retain(|n| !matches!(n, ItemNote::MultiRomZip { .. }));
                    item.notes.push(ItemNote::MultiRomExtract {
                        count: chosen.len(),
                    });
                }
                (chosen, ExtractShape::Roms, SheetCheck::Read)
            }
        };
    set_extract(item, insp, &chosen, sheet_check, mode)?;
    Ok(Some(shape))
}

/// How a folder game found inside an archive at `root` reaches `system`'s
/// folder (adjustments spec D23): a PS3 disc (JB), PS4 or Wii U game
/// extracts the root's subtree into a folder named like a disk folder
/// game of that system (entries outside the root are leftovers); a Vita
/// game, a PS3 HDD/PSN title, an Xbox, Xbox 360 or PS5 game, or a game
/// the user filed under another system moves as it is.
fn plan_folder_extract(
    item: &mut OrganizeItem,
    insp: &Inspection,
    root: &str,
    system: &System,
    mode: TransferMode,
) -> Result<Option<ExtractShape>, LeftReason> {
    let extracts = match insp.folder_game.as_ref() {
        Some(ArchiveGame::One {
            system: found,
            probe,
            ..
        }) if *found == system.id => match found.as_str() {
            "ps3" => super::evidence::is_ps3_disc_probe(probe),
            "ps4" | "wiiu" => true,
            _ => false,
        },
        Some(ArchiveGame::Ps4OrVita { .. }) => system.id == "ps4",
        _ => false,
    };
    if !extracts {
        return Ok(None);
    }
    let prefix = format!("{}/", root.to_lowercase());
    let chosen: Vec<&ZipEntryInfo> = insp
        .entries
        .iter()
        .filter(|e| root.is_empty() || e.name.to_lowercase().starts_with(&prefix))
        .filter(|e| e.unsafe_why.is_some() || !is_metadata_entry(&e.name))
        .collect();
    if chosen.is_empty() {
        return Ok(None);
    }
    let name = match root.rsplit_once('/') {
        Some((_, last)) => last.to_string(),
        None if !root.is_empty() => root.to_string(),
        None => item
            .sources
            .first()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    set_extract(item, insp, &chosen, SheetCheck::Read, mode)?;
    if let Transfer::Extract { folder_game, .. } = &mut item.transfer {
        *folder_game = true;
    }
    Ok(Some(ExtractShape::Folder {
        root: root.to_string(),
        name,
    }))
}

/// Sets `item.transfer` to the extraction of `chosen` (targets assigned
/// later): the archive goes to the Trash in move mode only when nothing
/// else is left in it.
fn set_extract(
    item: &mut OrganizeItem,
    insp: &Inspection,
    chosen: &[&ZipEntryInfo],
    sheet_check: SheetCheck,
    mode: TransferMode,
) -> Result<(), LeftReason> {
    let mut entries = Vec::with_capacity(chosen.len());
    for e in chosen {
        if let Some(why) = e.unsafe_why {
            return Err(LeftReason::UnsafeArchiveEntry {
                name: e.name.clone(),
                why,
            });
        }
        // Facts from before extraction carry no index: never guessed.
        let Some(index) = e.index else {
            return Err(LeftReason::Unknown);
        };
        entries.push(ExtractEntry {
            index,
            name: e.name.clone(),
            size: e.size,
            crc32: e
                .crc32
                .as_deref()
                .and_then(|c| u32::from_str_radix(c, 16).ok()),
            target: PathBuf::new(),
        });
    }
    let leftovers: Vec<&ZipEntryInfo> = insp
        .entries
        .iter()
        // An unsafe name is never junk: `__MACOSX/../Game.srm` is a save
        // that is not extracted.
        .filter(|e| e.unsafe_why.is_some() || !is_metadata_entry(&e.name))
        .filter(|e| !entries.iter().any(|x| Some(x.index) == e.index))
        .collect();
    let archive_after = match mode {
        TransferMode::Copy => ArchiveAfter::Keep,
        TransferMode::Move if leftovers.is_empty() => ArchiveAfter::Trash,
        TransferMode::Move => {
            item.notes.push(ItemNote::ArchiveKeptLeftovers {
                count: leftovers.len(),
                examples: leftovers
                    .iter()
                    .take(LEFTOVER_EXAMPLES)
                    .map(|e| e.name.clone())
                    .collect(),
            });
            ArchiveAfter::Keep
        }
    };
    if entries.iter().any(|e| e.crc32.is_none())
        && !item.notes.contains(&ItemNote::NoStoredChecksum)
    {
        item.notes.push(ItemNote::NoStoredChecksum);
    }
    item.transfer = Transfer::Extract {
        bytes: entries.iter().map(|e| e.size).sum(),
        entries,
        archive_after,
        source: match &insp.tool {
            Some((tool, version)) => ExtractSource::Tool {
                tool: *tool,
                version: version.clone(),
            },
            None => ExtractSource::Builtin,
        },
        sheet_check,
        m3u: None,
        folder_game: false,
    };
    Ok(())
}

/// Whether the user may settle this item: it moves, or nothing placed it.
fn decidable(item: &OrganizeItem) -> bool {
    match &item.action {
        OrganizeAction::Auto | OrganizeAction::Review => true,
        OrganizeAction::Left { reason } => matches!(reason, LeftReason::Unknown),
        OrganizeAction::SetAside { .. } => false,
    }
}

fn count(items: &[OrganizeItem], walked: &Walked) -> Counts {
    let mut counts = Counts {
        junk: walked.junk,
        other_files: walked.other_files.len(),
        deep_folders: walked.deep_folders.len(),
        ..Counts::default()
    };
    let mut systems: BTreeSet<&str> = BTreeSet::new();
    for item in items {
        match item.action {
            OrganizeAction::Auto => counts.auto += 1,
            OrganizeAction::Review => counts.review += 1,
            OrganizeAction::Left { .. } => {
                counts.left += 1;
                continue;
            }
            OrganizeAction::SetAside { .. } => {
                counts.set_aside += 1;
                continue;
            }
        }
        counts.roms += 1;
        counts.bytes += item.size;
        if let Some(system) = &item.system_id {
            systems.insert(system);
        }
    }
    counts.systems = systems.len();
    counts
}

/// MAME CHD folders (spec D16): the `.chd` files of `<set>/` follow
/// `<set>.zip` into `<arcade folder>/<set>/` when the zip moves in as an
/// arcade set under its own name. Otherwise, or when a CHD cannot land
/// (already there, a different file there, too big for FAT32), the CHD is
/// returned to be left with its reason: a set already present in the
/// library reports each CHD already present beside it when it is.
fn attach_chd_folders(
    items: &mut [OrganizeItem],
    chd_folders: &[(PathBuf, Vec<PathBuf>)],
    src: &Path,
    target_root: &Path,
    db: &SystemDb,
    fat32: bool,
) -> Vec<(PathBuf, LeftReason)> {
    let mut left = Vec::new();
    for (zip, chds) in chd_folders {
        let zip_rel = rel(src, zip);
        let set = items.iter_mut().find(|i| {
            i.moves_in()
                && i.sources.len() == 1
                && i.sources[0] == zip_rel
                && i.rename.is_none()
                && i.target.is_some()
                && i.system_id
                    .as_deref()
                    .and_then(|s| db.get(s))
                    .is_some_and(|s| s.arcade)
        });
        let Some(set) = set else {
            let present = items.iter().find_map(|i| match &i.action {
                OrganizeAction::Left {
                    reason: LeftReason::AlreadyPresent { at },
                } if i.sources.len() == 1 && i.sources[0] == zip_rel => Some(at.clone()),
                _ => None,
            });
            left.extend(chds.iter().map(|c| {
                let reason = present
                    .as_deref()
                    .and_then(|at| Some(at.parent()?.join(at.file_stem()?).join(c.file_name()?)))
                    .filter(|to| same_bytes(&target_root.join(to), c))
                    .map_or(LeftReason::Unknown, |to| LeftReason::AlreadyPresent {
                        at: to,
                    });
                (c.clone(), reason)
            }));
            continue;
        };
        let target = set.target.clone().expect("checked above");
        let (Some(folder), Some(stem)) = (target.parent(), target.file_stem()) else {
            left.extend(chds.iter().map(|c| (c.clone(), LeftReason::Unknown)));
            continue;
        };
        let dir = folder.join(stem);
        for chd in chds {
            let Some(name) = chd.file_name() else {
                continue;
            };
            let to = dir.join(name);
            let at = target_root.join(&to);
            if fat32 && fs::metadata(chd).is_ok_and(|m| m.len() >= 4 << 30) {
                left.push((chd.clone(), LeftReason::TooBigForFat));
            } else if fs::symlink_metadata(&at).is_ok() {
                let reason = if same_bytes(&at, chd) {
                    LeftReason::AlreadyPresent { at: to }
                } else {
                    LeftReason::CollisionSet
                };
                left.push((chd.clone(), reason));
            } else {
                set.companions.push(Companion {
                    source: rel(src, chd),
                    target: to,
                    licence: false,
                    child_art: false,
                });
            }
        }
    }
    left
}

/// Same length and same bytes (CRC32 of both, streamed).
fn same_bytes(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::metadata(a), fs::metadata(b)) else {
        return false;
    };
    ma.len() == mb.len() && matches!((file_crc(a), file_crc(b)), (Some(x), Some(y)) if x == y)
}

fn file_crc(path: &Path) -> Option<u32> {
    use std::io::Read;
    let mut file = crate::consoles::open_rom(path).ok()?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Some(hasher.finalize()),
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

/// The nearest existing ancestor of `path` (itself when it exists).
fn existing_ancestor(path: &Path) -> Option<&Path> {
    path.ancestors().find(|p| p.exists())
}

/// Whether `a` and `b` (or their nearest existing ancestors) are on one
/// volume: same device on Unix, same drive or share on Windows (O-D3).
fn same_volume(a: &Path, b: &Path) -> bool {
    let (Some(a), Some(b)) = (existing_ancestor(a), existing_ancestor(b)) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (fs::metadata(a), fs::metadata(b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let prefix = |p: &Path| {
            p.components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        };
        prefix(a) == prefix(b)
    }
}

/// Whether files in `dir` can be moved away (the folder is writable).
fn dir_writable(dir: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let Ok(c_path) = CString::new(dir.as_os_str().as_bytes()) else {
            return false;
        };
        // SAFETY: `c_path` is a valid nul-terminated string.
        unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
    }
    // Windows does not honour FILE_ATTRIBUTE_READONLY on folders (Explorer
    // sets it on Documents, Desktop and customised folders), so it says
    // nothing about moving files out; a denied move pauses the apply.
    #[cfg(windows)]
    {
        fs::metadata(dir).is_ok_and(|m| m.is_dir())
    }
    #[cfg(not(any(unix, windows)))]
    {
        fs::metadata(dir).is_ok_and(|m| !m.permissions().readonly())
    }
}

/// [`survey`] and [`plan_from_survey`] without decisions.
pub fn plan_organize(
    source: &Path,
    opts: &OrganizeOptions,
    env: &OrganizeEnv,
    progress: &(dyn Fn(OrganizeProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<OrganizePlan, OrganizeError> {
    let survey = survey(source, opts, env, progress, cancel)?;
    plan_from_survey(&survey, opts, &[], env)
}

/// ROM-like files directly in `dir` (not in its subfolders), by name only:
/// what a library shows as "loose files" to organise (organise spec §4.5).
/// Partial downloads, hidden and system files do not count; an unreadable
/// or missing folder counts 0.
pub fn count_loose_roms(dir: &Path, db: &SystemDb) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            !(is_appledouble(&name)
                || is_junk_file(&name)
                || is_zone_identifier(&name)
                || is_hidden(&name)
                || is_partial_download(&name))
                && db.is_rom_like_extension(&extension(&entry.path()))
        })
        .count()
}

/// [`count_loose_roms`] of a library: its root plus its `roms_root` (found
/// ignoring case), where no system folder holds them.
pub fn count_library_loose_roms(root: &Path, roms_root: &str, db: &SystemDb) -> usize {
    let in_root = count_loose_roms(root, db);
    if roms_root.trim_matches(['/', '\\']).is_empty() {
        return in_root;
    }
    in_root + find_dir_ignoring_case(root, roms_root).map_or(0, |dir| count_loose_roms(&dir, db))
}

/// A quick look at `path` by names and metadata only (no file is read):
/// what the start screen needs to offer organising. Stops after
/// [`PROBE_LIMIT`] entries. `profile` is the layout the pile would go to,
/// when known: its own folder names count as sorted.
pub fn probe_folder(
    path: &Path,
    db: &SystemDb,
    profile: Option<&Profile>,
    probe: &dyn MetaProbe,
) -> io::Result<FolderProbe> {
    fs::read_dir(path)?;
    let mut out = FolderProbe {
        writable: dir_writable(path),
        broad_root: is_broad_root(path),
        ..FolderProbe::default()
    };
    // Same folders the walk never enters (`bios`, `saves`, …).
    let ignored = ScanOptions::default().ignored_dirs;
    let mut sorted: BTreeSet<String> = BTreeSet::new();
    let mut partial_dirs = 0usize;
    let walker = WalkDir::new(path)
        .follow_links(false)
        .same_file_system(true)
        .min_depth(1)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            if is_hidden(&name) || ignored.iter().any(|d| d.eq_ignore_ascii_case(&name)) {
                return false;
            }
            if is_partial_download(&name) {
                partial_dirs += 1;
                return false;
            }
            match db.resolve_folder_in(profile, &name) {
                Some(_) => {
                    sorted.insert(name.into_owned());
                    false
                }
                None => true,
            }
        });
    let mut seen = 0usize;
    for entry in walker {
        seen += 1;
        if seen > PROBE_LIMIT {
            out.truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if is_appledouble(&name)
            || is_junk_file(&name)
            || is_zone_identifier(&name)
            || is_hidden(&name)
        {
            continue;
        }
        if is_partial_download(&name) {
            out.partial_downloads += 1;
            continue;
        }
        if db.is_rom_like_extension(&extension(entry.path())) {
            out.rom_like += 1;
            if entry
                .metadata()
                .is_ok_and(|m| probe.cloud_only(entry.path(), &m))
            {
                out.cloud_only += 1;
            }
        }
    }
    out.partial_downloads += partial_dirs;
    out.sorted_folders = sorted.into_iter().collect();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(insp: Inspection) -> Facts {
        Facts {
            inspection: insp,
            raw_crcs: Vec::new(),
            hashed: false,
        }
    }

    #[test]
    fn tool_listed_archives_are_not_memoised() {
        // A tool upgrade or the setting turned off must be seen by the
        // next plan: the cache key (path, size, mtime) would not change.
        let mut insp = Inspection::new(Container::Rar);
        insp.tool = Some((Tool::Unrar, "7.01".into()));
        assert!(!memoisable(&facts(insp)));
    }

    #[test]
    fn a_failed_tool_is_not_memoised_as_damaged() {
        let mut insp = Inspection::new(Container::Rar);
        insp.issue = Some(ArchiveIssue::Damaged);
        insp.notes.push(NOTE_TOOL_FAILED.into());
        assert!(!memoisable(&facts(insp)));
    }

    #[test]
    fn builtin_damaged_archives_stay_memoised() {
        let mut insp = Inspection::new(Container::Zip);
        insp.issue = Some(ArchiveIssue::Damaged);
        assert!(memoisable(&facts(insp)));
    }

    #[test]
    fn new_variants_serde() {
        use serde_json::{json, to_value};
        assert_eq!(
            to_value(LeftReason::GameOverlay {
                of: "Bloodborne".into()
            })
            .unwrap(),
            json!({"kind": "game-overlay", "of": "Bloodborne"})
        );
        assert_eq!(
            to_value(NotARom::ConsoleSystem).unwrap(),
            json!("console-system")
        );
        assert_eq!(
            to_value(LeftReason::NotARom {
                what: NotARom::ConsoleSystem
            })
            .unwrap(),
            json!({"kind": "not-a-rom", "what": "console-system"})
        );
        for (note, wire) in [
            (ItemNote::ExtractFirst, json!({"kind": "extract-first"})),
            (ItemNote::NoEmulator, json!({"kind": "no-emulator"})),
            (ItemNote::ProEnhanced, json!({"kind": "pro-enhanced"})),
            (
                ItemNote::DlcFolder {
                    layout: "Batocera".into(),
                },
                json!({"kind": "dlc-folder", "layout": "Batocera"}),
            ),
        ] {
            assert_eq!(to_value(&note).unwrap(), wire);
            assert_eq!(serde_json::from_value::<ItemNote>(wire).unwrap(), note);
        }
        let warning = PlanWarning::LargeTransfer {
            bytes: 12_000_000_000,
            items: 3,
        };
        let wire = json!({"kind": "large-transfer", "bytes": 12_000_000_000u64, "items": 3});
        assert_eq!(to_value(&warning).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<PlanWarning>(wire).unwrap(),
            warning
        );
    }

    #[test]
    fn adjustment_variants_serde() {
        use serde_json::{json, to_value};
        for (reason, wire) in [
            (
                LeftReason::GamePart {
                    part: PartKind::ExeFs,
                    system: "switch".into(),
                    title_id: None,
                },
                json!({"kind": "game-part", "part": "exe-fs", "system": "switch", "title_id": null}),
            ),
            (
                LeftReason::GamePart {
                    part: PartKind::Backup,
                    system: "psvita".into(),
                    title_id: Some("PCSE00120".into()),
                },
                json!({"kind": "game-part", "part": "backup", "system": "psvita", "title_id": "PCSE00120"}),
            ),
            (LeftReason::NotConfirmed, json!({"kind": "not-confirmed"})),
        ] {
            assert_eq!(to_value(&reason).unwrap(), wire);
            assert_eq!(serde_json::from_value::<LeftReason>(wire).unwrap(), reason);
        }
        // A game part written without its title id still loads.
        assert_eq!(
            serde_json::from_value::<LeftReason>(
                json!({"kind": "game-part", "part": "nca", "system": "switch"})
            )
            .unwrap(),
            LeftReason::GamePart {
                part: PartKind::Nca,
                system: "switch".into(),
                title_id: None
            }
        );
        for (note, wire) in [
            (ItemNote::Homebrew, json!({"kind": "homebrew"})),
            (ItemNote::Vita7z, json!({"kind": "vita7z"})),
            (
                ItemNote::MissingDisc { n: 2 },
                json!({"kind": "missing-disc", "n": 2}),
            ),
            (ItemNote::EncryptedName, json!({"kind": "encrypted-name"})),
            (
                ItemNote::RegionLetterUnknown { letter: 'T' },
                json!({"kind": "region-letter-unknown", "letter": "T"}),
            ),
        ] {
            assert_eq!(to_value(&note).unwrap(), wire);
            assert_eq!(serde_json::from_value::<ItemNote>(wire).unwrap(), note);
        }
        let d = Decision {
            item: 4,
            choice: Choice::Decline,
        };
        assert_eq!(
            to_value(&d).unwrap(),
            json!({"item": 4, "choice": "decline"})
        );
        let back: Decision =
            serde_json::from_value(json!({"item": 4, "choice": "decline"})).unwrap();
        assert_eq!(back.choice, Choice::Decline);
    }

    #[test]
    fn facts_of_an_older_version_are_discarded() {
        let cache = Cache::in_memory().unwrap();
        let c = WalkCandidate {
            path: PathBuf::from("/pile/Game.zip"),
            ext: "zip".into(),
            size: 10,
            mtime_ns: 1_700_000_000_000_000_000,
            cloud_only: false,
        };
        let f = facts(Inspection::new(Container::Zip));
        remember_facts(&cache, &c, &f);
        assert!(memoised_facts(&cache, &c).is_some());
        let mut old = serde_json::to_value(StoredFacts {
            v: FACTS_VERSION,
            facts: f,
        })
        .unwrap();
        assert_eq!(FACTS_VERSION, 13);
        old["v"] = 11.into();
        cache
            .put_organize_facts(&c.path, c.size, c.mtime_ns, &old.to_string())
            .unwrap();
        assert!(memoised_facts(&cache, &c).is_none());
    }

    /// Part C D3: the game panel reuses the console verdict a plan
    /// memoised for the same file, size and mtime, and only of this facts
    /// version.
    #[test]
    fn the_memoised_console_verdict_is_read_back() {
        let cache = Cache::in_memory().unwrap();
        let c = WalkCandidate {
            path: PathBuf::from("/lib/ROMS/3DS/Game.3ds"),
            ext: "3ds".into(),
            size: 10,
            mtime_ns: 1_700_000_000_000_000_000,
            cloud_only: false,
        };
        assert_eq!(memoised_console(&cache, &c.path, c.size, c.mtime_ns), None);
        let mut sniff = Sniff::default();
        sniff.add_console(vec![crate::consoles::ConsoleId::proven("3ds", "NCSD")]);
        let mut insp = Inspection::new(Container::Raw);
        insp.payloads = vec![Payload {
            name: "Game.3ds".into(),
            ext: "3ds".into(),
            size: 10,
            crc32: None,
            sniff,
        }];
        remember_facts(&cache, &c, &facts(insp));
        let id = memoised_console(&cache, &c.path, c.size, c.mtime_ns).unwrap();
        assert_eq!((id.system.as_str(), id.detail.as_str()), ("3ds", "NCSD"));
        // Another size or mtime is another file.
        assert_eq!(memoised_console(&cache, &c.path, 11, c.mtime_ns), None);
        assert_eq!(memoised_console(&cache, &c.path, c.size, 1), None);
    }

    #[test]
    fn organize_item_content_id_is_optional() {
        let mut item = OrganizeItem {
            id: 0,
            sources: vec!["Bloodborne (CUSA00900)".into()],
            kind: ItemKind::Folder,
            size: 1,
            fps: Vec::new(),
            target: None,
            set_targets: Vec::new(),
            system_id: Some("ps4".into()),
            folder: None,
            confidence: 0.95,
            method: None,
            why: Vec::new(),
            alternatives: Vec::new(),
            rename: None,
            notes: Vec::new(),
            companions: Vec::new(),
            catalogue_name: None,
            transfer: Transfer::AsIs,
            disc: None,
            title_id: None,
            content_id: None,
            playlist: None,
            title_source: None,
            game: None,
            action: OrganizeAction::Auto,
        };
        let bare = serde_json::to_value(&item).unwrap();
        assert!(bare.get("content_id").is_none(), "{bare}");
        let back: OrganizeItem = serde_json::from_value(bare).unwrap();
        assert_eq!(back.content_id, None);
        item.content_id = Some("UP9000-CUSA00900_00-BLOODBORNE000000".into());
        let full = serde_json::to_value(&item).unwrap();
        assert_eq!(full["content_id"], "UP9000-CUSA00900_00-BLOODBORNE000000");
        assert_eq!(serde_json::from_value::<OrganizeItem>(full).unwrap(), item);
    }

    fn bare_item() -> OrganizeItem {
        OrganizeItem {
            id: 0,
            sources: vec!["Pokemon Y.3ds".into()],
            kind: ItemKind::File,
            size: 1,
            fps: Vec::new(),
            target: None,
            set_targets: Vec::new(),
            system_id: Some("3ds".into()),
            folder: None,
            confidence: 0.95,
            method: None,
            why: Vec::new(),
            alternatives: Vec::new(),
            rename: None,
            notes: Vec::new(),
            companions: Vec::new(),
            catalogue_name: None,
            transfer: Transfer::AsIs,
            disc: None,
            title_id: None,
            content_id: None,
            playlist: None,
            title_source: None,
            game: None,
            action: OrganizeAction::Auto,
        }
    }

    #[test]
    fn game_folder_variants_serde() {
        use serde_json::{json, to_value};
        let reason = LeftReason::GameFolderContent {
            folder: FixedFolder::Mods,
            of: "Pokemon Y".into(),
        };
        let wire = json!({"kind": "game-folder-content", "folder": "mods", "of": "Pokemon Y"});
        assert_eq!(to_value(&reason).unwrap(), wire);
        assert_eq!(serde_json::from_value::<LeftReason>(wire).unwrap(), reason);
        for (note, wire) in [
            (
                ItemNote::DemoOf {
                    of: "Pokemon Y".into(),
                },
                json!({"kind": "demo-of", "of": "Pokemon Y"}),
            ),
            (ItemNote::BaseMissing, json!({"kind": "base-missing"})),
        ] {
            assert_eq!(to_value(&note).unwrap(), wire);
            assert_eq!(serde_json::from_value::<ItemNote>(wire).unwrap(), note);
        }
        let rename = RenameReason::GameFolder {
            from: "pky.3ds".into(),
        };
        let wire = json!({"kind": "game-folder", "from": "pky.3ds"});
        assert_eq!(to_value(&rename).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<RenameReason>(wire).unwrap(),
            rename
        );
        for (role, wire) in [
            (GameRole::Base, "base"),
            (GameRole::Update, "update"),
            (GameRole::Dlc, "dlc"),
            (GameRole::Demo, "demo"),
        ] {
            assert_eq!(to_value(role).unwrap(), json!(wire));
        }
    }

    #[test]
    fn companion_licence_is_left_out_when_false() {
        use serde_json::{json, to_value};
        let mut c = Companion {
            source: "a.rap".into(),
            target: "b.rap".into(),
            licence: false,
            child_art: false,
        };
        let bare = to_value(&c).unwrap();
        assert!(bare.get("licence").is_none(), "{bare}");
        // A companion written before licences still loads.
        let old: Companion =
            serde_json::from_value(json!({"source": "a.sav", "target": "b.sav"})).unwrap();
        assert!(!old.licence);
        c.licence = true;
        let full = to_value(&c).unwrap();
        assert_eq!(full["licence"], true);
        assert_eq!(serde_json::from_value::<Companion>(full).unwrap(), c);
        // Part C D10 d: the child art flag, the same way.
        assert!(bare.get("child_art").is_none(), "{bare}");
        c.child_art = true;
        let art = to_value(&c).unwrap();
        assert_eq!(art["child_art"], true);
        assert_eq!(serde_json::from_value::<Companion>(art).unwrap(), c);
    }

    #[test]
    fn organize_item_game_is_optional() {
        let mut item = bare_item();
        let bare = serde_json::to_value(&item).unwrap();
        assert!(bare.get("game").is_none(), "{bare}");
        // A plan item from before game folders still loads.
        let back: OrganizeItem = serde_json::from_value(bare).unwrap();
        assert_eq!(back.game, None);
        let mut link = GameLink::new(GameRole::Update);
        link.version = Some("1.5".into());
        link.of = Some(3);
        link.folder = Some(PathBuf::from("3DS").join("Pokemon Y"));
        let link_wire = serde_json::to_value(&link).unwrap();
        assert!(link_wire.get("folder_exists").is_none(), "{link_wire}");
        assert!(link_wire.get("newest_here").is_none(), "{link_wire}");
        assert!(link_wire.get("region").is_none(), "{link_wire}");
        assert_eq!(link_wire["role"], "update");
        item.game = Some(link);
        let full = serde_json::to_value(&item).unwrap();
        assert_eq!(full["game"]["of"], 3);
        assert_eq!(serde_json::from_value::<OrganizeItem>(full).unwrap(), item);
    }

    #[test]
    fn a_dlc_stem_whose_lower_case_moves_bytes_never_panics() {
        // `İ` grows and `ẞ` shrinks when lower-cased: same total length,
        // different offsets. Untrusted file names must not cut a char.
        assert_eq!(dlc_rest("\u{130}a\u{1E9E}", "a"), "\u{130}a\u{1E9E}");
        assert_eq!(
            dlc_rest("Pok\u{e9}mon Y - Extra", "pok\u{e9}mon y"),
            "Extra"
        );
        assert_eq!(dlc_rest("Zelda (USA) [DLC] - Pack", "zelda"), "Pack");
    }

    #[test]
    fn demo_folder_stem_does_not_double_a_bare_region_word() {
        assert_eq!(
            demo_folder_stem("Game USA", Some(Region::Usa)),
            ("Game USA (Demo)".to_string(), None)
        );
        assert_eq!(
            demo_folder_stem("Game", Some(Region::Usa)),
            ("Game (USA) (Demo)".to_string(), None)
        );
        // A name that has its demo tag keeps the region for the folder,
        // which must not double a bare region word either.
        let (wanted, region) = demo_folder_stem("Game USA (Demo)", Some(Region::Usa));
        let tmp = tempfile::tempdir().unwrap();
        let folder = crate::organize::target::game_folder_name(
            &wanted,
            region,
            tmp.path(),
            &mut crate::organize::target::DirIndex::new(),
        )
        .unwrap()
        .0;
        assert_eq!(folder, "Game USA (Demo)");
    }

    #[test]
    fn addon_variants_serde() {
        use crate::addons::{AddonFormat, AddonKind, AddonLink, HeaderFit, LinkVia, UnlinkedWhy};
        use crate::translate::{EmulatorId, NotSupportedWhy};
        use serde_json::{json, to_value};
        for (role, wire) in [(GameRole::Mod, "mod"), (GameRole::Cheat, "cheat")] {
            assert_eq!(to_value(role).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<GameRole>(json!(wire)).unwrap(),
                role
            );
        }
        let reasons = [
            (
                LeftReason::AddonUnlinked {
                    kind: AddonKind::Patch,
                    format: AddonFormat::Ips,
                    why: UnlinkedWhy::NoKey,
                },
                // The addon kind cannot be `kind`: that is the tag.
                json!({"kind": "addon-unlinked", "addon": "patch", "format": "ips",
                       "why": {"kind": "no-key"}}),
            ),
            (
                LeftReason::AddonUnlinked {
                    kind: AddonKind::Mod,
                    format: AddonFormat::CemuGraphicPack,
                    why: UnlinkedWhy::Several { count: 2 },
                },
                json!({"kind": "addon-unlinked", "addon": "mod", "format": "cemu-graphic-pack",
                       "why": {"kind": "several", "count": 2}}),
            ),
            (
                LeftReason::AddonNameTaken {
                    at: PathBuf::from("Pokemon Y"),
                },
                json!({"kind": "addon-name-taken", "at": "Pokemon Y"}),
            ),
            (
                LeftReason::AddonNoPlace {
                    kind: AddonKind::Cheat,
                },
                json!({"kind": "addon-no-place", "addon": "cheat"}),
            ),
            (
                LeftReason::AddonForEmulator {
                    emulator: "DuckStation".into(),
                    path: PathBuf::from("SLUS-00594.cht"),
                },
                json!({"kind": "addon-for-emulator", "emulator": "DuckStation",
                       "path": "SLUS-00594.cht"}),
            ),
            (
                LeftReason::NotSupported {
                    emulator: "RPCS3".into(),
                    why: NotSupportedWhy::Rpcs3Mods,
                },
                json!({"kind": "not-supported", "emulator": "RPCS3", "why": "rpcs3-mods"}),
            ),
            (
                LeftReason::PatchNotApplied {
                    emulator: "RetroArch".into(),
                },
                json!({"kind": "patch-not-applied", "emulator": "RetroArch"}),
            ),
        ];
        for (reason, wire) in reasons {
            assert_eq!(to_value(&reason).unwrap(), wire);
            assert_eq!(serde_json::from_value::<LeftReason>(wire).unwrap(), reason);
        }
        let notes = [
            (
                ItemNote::ModOf { of: "Zelda".into() },
                json!({"kind": "mod-of", "of": "Zelda"}),
            ),
            (
                ItemNote::PatchOf { of: "Zelda".into() },
                json!({"kind": "patch-of", "of": "Zelda"}),
            ),
            (
                ItemNote::CheatOf { of: "Zelda".into() },
                json!({"kind": "cheat-of", "of": "Zelda"}),
            ),
            (
                ItemNote::HeaderMismatch {
                    needs: HeaderFit::NeedsHeaderless,
                },
                json!({"kind": "header-mismatch", "needs": "needs-headerless"}),
            ),
            (
                ItemNote::PointEmulatorHere {
                    emulator: "shadPS4".into(),
                    folder: "addcont".into(),
                },
                json!({"kind": "point-emulator-here", "emulator": "shadPS4", "folder": "addcont"}),
            ),
        ];
        for (note, wire) in notes {
            assert_eq!(to_value(&note).unwrap(), wire);
            assert_eq!(serde_json::from_value::<ItemNote>(wire).unwrap(), note);
        }
        // A link with an addon carries it; one without leaves it out.
        let mut link = GameLink::new(GameRole::Cheat);
        assert!(to_value(&link).unwrap().get("addon").is_none());
        link.addon = Some(AddonLink {
            kind: AddonKind::Cheat,
            format: AddonFormat::DuckStationCht,
            key_text: "SLUS-00594".into(),
            via: LinkVia::Key,
            header: None,
        });
        let wire = to_value(&link).unwrap();
        assert_eq!(
            wire,
            json!({"role": "cheat", "addon": {"kind": "cheat", "format": "duck-station-cht",
                   "key_text": "SLUS-00594", "via": "key"}})
        );
        assert_eq!(serde_json::from_value::<GameLink>(wire).unwrap(), link);
        // Emulator ids keep their product spelling, lower case.
        assert_eq!(to_value(EmulatorId::RetroArch).unwrap(), json!("retroarch"));
        assert_eq!(
            to_value(EmulatorId::DuckStation).unwrap(),
            json!("duckstation")
        );
        assert_eq!(
            to_value(EmulatorId::YuzuFamily).unwrap(),
            json!("yuzu-family")
        );
    }

    #[test]
    fn a_part_a_game_link_still_loads() {
        let link: GameLink = serde_json::from_value(serde_json::json!({
            "role": "update", "version": "1.5", "of": 1, "folder": "3DS/Pokemon Y",
            "newest_here": true
        }))
        .unwrap();
        assert_eq!(link.role, GameRole::Update);
        assert_eq!(link.addon, None);
    }

    #[test]
    fn an_older_plan_without_translated_layouts_still_loads() {
        use serde_json::json;
        let old = json!({
            "source_root": "/pile", "target_root": "/lib", "roms_dir": "/lib",
            "layout": "coverly-library", "mode": "move", "names": "keep", "items": [],
            "counts": serde_json::to_value(Counts::default()).unwrap(),
            "sorted_folders": [], "deep_folders": [], "other_files": [], "warnings": [],
            "broad_root": false, "fat32": false, "move_allowed": true,
            "target_detected": null, "created_at": "2026-10-24T00:00:00Z"
        });
        let plan: OrganizePlan = serde_json::from_value(old).unwrap();
        assert!(plan.translated.is_empty());
        assert_eq!(plan.target_anchor, None);
        let back = serde_json::to_value(&plan).unwrap();
        assert!(back.get("translated").is_none(), "{back}");
    }
}
