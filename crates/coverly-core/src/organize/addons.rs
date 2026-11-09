//! Addons in Organizar (mods spec D2–D4, D7, D13, D18; plan P-1): the
//! walk hook that tells a file or a folder of a pile for a patch, a cheat
//! or a mod, with the key its format carries.
//!
//! Every name is checked for key material before anything is opened
//! (D18): a key file is never read, and a mod folder holding one is left
//! whole. Reads are bounded: a patch's first 64 and last 12 bytes (at most
//! 4 KiB of an xdelta header), at most 1 MiB of a cheat's text, the small
//! manifests a mod folder names; never a whole patch.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::addons::cheat::{is_global_cheat_db, may_be_cheat, sniff_cheat};
use crate::addons::link::{
    resolve, resolve_by_readme, wanted_sizes, Candidate, Candidates, GameRef, Resolution,
};
use crate::addons::mods::{
    is_mod_dir_without_keys, n64_htc_name, sniff_mod_dir_on_disk, ModVerdict, SMALL_FILE_CAP,
};
use crate::addons::patch::{ips_chain_index, read_patch};
use crate::addons::{
    valid_game_id, valid_sony_serial, valid_title_id16, AddonFormat, AddonKey, AddonKind,
    AddonLink, HeaderFit, LinkVia, UnlinkedWhy,
};
use crate::cache::Cache;
use crate::consoles::is_key_material;
use crate::consoles::{base_title_id, TitleKind};
use crate::game_folder::FixedFolder;
use crate::identify::names::parse_tags;
use crate::systems::SystemDb;

use super::library_bases::{LibraryBase, LibraryIndex};
use super::plan::{
    effective_kind, GameLink, GameRole, ItemKind, ItemNote, LeftReason, OrganizeAction,
    OrganizeItem, SurveyEntry, Transfer,
};
use super::target::{DirIndex, Subdir, TargetLayout};

/// A file or a folder of the pile the walk took for an addon (D2–D4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonCandidate {
    /// Absolute, as walked: the file, the mod folder, or an MSU-1 set's
    /// `.msu`.
    pub path: PathBuf,
    pub is_dir: bool,
    pub kind: AddonKind,
    pub format: AddonFormat,
    pub key: AddonKey,
    /// D13: filed by the user in a pile game folder's `Mods/` or
    /// `Cheats/`, or a shadPS4 `-mods` overlay beside its base: the base's
    /// path (its file, or the folder game).
    pub filed_under: Option<PathBuf>,
    /// MSU-1: the other files of the set, relative to the folder of
    /// `path` (their file names).
    pub members: Vec<PathBuf>,
    /// The readme beside a keyless patch (≤ 64 KiB), if any.
    pub readme: Option<PathBuf>,
    /// One piece of a mod found without the rest (D7 `Partial`): a `.pcm`
    /// without its `.msu`, a `.msu` without a track, a Riivolution `.xml`
    /// without its pack root.
    pub partial: bool,
}

/// Extensions of translation patches (D2), besides the `.ips1`…`.ips9`
/// chain names.
const PATCH_EXTS: [&str; 8] = [
    "ips", "bps", "ups", "xdelta", "xdelta3", "vcdiff", "ppf", "aps",
];

/// Largest readme taken beside a keyless patch.
pub const README_MAX: u64 = 64 * 1024;

fn ext_lower(name: &str) -> Option<String> {
    name.rsplit_once('.')
        .filter(|(stem, ext)| !stem.is_empty() && !ext.is_empty())
        .map(|(_, ext)| ext.to_ascii_lowercase())
}

/// A patch by its extension (`.ips`, `.bps`, `.xdelta`, `.ips3` …): the
/// walk's cheap pre-filter before reading 64 bytes.
pub fn is_patch_name(name: &str) -> bool {
    ext_lower(name).is_some_and(|e| PATCH_EXTS.contains(&e.as_str()))
        || ips_chain_index(name).is_some_and(|i| i > 0)
}

/// An MSU-1 piece by its extension (`.msu`, `.pcm`, `.bml`).
pub(crate) fn is_msu1_piece(name: &str) -> bool {
    ext_lower(name).is_some_and(|e| matches!(e.as_str(), "msu" | "pcm" | "bml"))
}

/// Whether a file of the pile is worth an addon sniff, by name only.
pub(crate) fn may_be_addon_file(name: &str) -> bool {
    !is_key_material(name)
        && !is_global_cheat_db(name)
        && (is_patch_name(name)
            || may_be_cheat(name)
            || is_msu1_piece(name)
            || n64_htc_name(name).is_some())
}

/// D3, D7: `usrcheat.dat`, `cheat.db` … are system data, left.
pub(crate) fn is_global_db(name: &str) -> bool {
    is_global_cheat_db(name)
}

/// The walk hook (P-1): `Some` when the file at `path` is a patch, a cheat
/// or an emulator patch, an N64 compiled texture pack or a loose
/// Riivolution `.xml` (partial). `parents` are up to two parent folder
/// names, nearest first. Key material is never opened. MSU-1 sets are
/// gathered by the walk from a folder's names, not here.
pub fn addon_candidate(
    path: &Path,
    meta: &fs::Metadata,
    parents: &[&str],
) -> Option<AddonCandidate> {
    let verdict = sniff_file(path, meta, parents, None, false)?;
    Some(file_candidate(path, verdict, None))
}

/// A sniffed file as a candidate: the readme of a keyless patch, the
/// partial flag of a loose Riivolution `.xml`.
pub(crate) fn file_candidate(
    path: &Path,
    (format, key): (AddonFormat, AddonKey),
    filed_under: Option<PathBuf>,
) -> AddonCandidate {
    let kind = format.kind();
    let readme = (kind == AddonKind::Patch && key == AddonKey::None)
        .then(|| readme_beside(path))
        .flatten();
    AddonCandidate {
        path: path.to_path_buf(),
        is_dir: false,
        kind,
        format,
        key,
        filed_under,
        members: Vec::new(),
        readme,
        partial: format == AddonFormat::Riivolution,
    }
}

/// Memoised addon verdict of one file (D10: `FACTS_VERSION` 11), stored
/// beside the organiser's facts under the file's path, size and mtime.
/// `addon_v` is a required key, so facts of a ROM never read as one.
#[derive(Serialize, Deserialize)]
struct StoredAddon {
    addon_v: u32,
    verdict: Option<(AddonFormat, AddonKey)>,
}

fn mtime_ns(meta: &fs::Metadata) -> i64 {
    let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
    match mtime.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
        Err(e) => i64::try_from(e.duration().as_nanos()).map_or(i64::MIN, |n| -n),
    }
}

/// The bounded sniff of one file (D2, D3): the format and key, or `None`.
/// `any_ext`: a file the user filed in a game's `Mods/` or `Cheats/`
/// (D13) is tried as a patch by its magic whatever its extension.
pub(crate) fn sniff_file(
    path: &Path,
    meta: &fs::Metadata,
    parents: &[&str],
    memo: Option<&Cache>,
    any_ext: bool,
) -> Option<(AddonFormat, AddonKey)> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    if is_key_material(&name) || is_global_cheat_db(&name) {
        return None;
    }
    if let Some(name) = n64_htc_name(&name) {
        return Some((AddonFormat::N64HiresTextures, AddonKey::N64Name { name }));
    }
    let size = meta.len();
    let mtime = mtime_ns(meta);
    let version = super::plan::FACTS_VERSION;
    if let Some(cache) = memo {
        if let Some(stored) = cache
            .organize_facts(path, size, mtime)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<StoredAddon>(&json).ok())
            .filter(|s| s.addon_v == version)
        {
            // A verdict of a sniff made without `any_ext` says nothing of
            // the magic of an unknown extension: read again then.
            if stored.verdict.is_some() || !any_ext {
                return stored.verdict;
            }
        }
    }
    let mut verdict = None;
    if any_ext || is_patch_name(&name) {
        verdict = read_patch(path).ok().flatten();
    }
    if verdict.is_none() && may_be_cheat(&name) {
        if let Some(text) = read_text(path) {
            verdict = riivolution_xml(&name, &text).or_else(|| sniff_cheat(&name, parents, &text));
        }
    }
    if let Some(cache) = memo {
        let stored = StoredAddon {
            addon_v: version,
            verdict: verdict.clone(),
        };
        if let Ok(json) = serde_json::to_string(&stored) {
            let _ = cache.put_organize_facts(path, size, mtime, &json);
        }
    }
    verdict
}

/// A loose Riivolution `.xml` (`<wiidisc>` with `<id game="RMC">`): a
/// piece of a pack whose data folders are elsewhere (D4: `Partial`).
fn riivolution_xml(name: &str, text: &str) -> Option<(AddonFormat, AddonKey)> {
    if ext_lower(name).as_deref() != Some("xml") || !text.contains("<wiidisc") {
        return None;
    }
    let id = text.split("<id").skip(1).find_map(|rest| {
        let tag = rest.split('>').next()?;
        let after = tag.split("game=").nth(1)?;
        let value = after
            .trim_start_matches(['"', '\''])
            .split(['"', '\''])
            .next()?;
        valid_game_id(value).filter(|id| id.len() != 9)
    });
    let key = id.map_or(AddonKey::None, |id| AddonKey::GameId { id });
    Some((AddonFormat::Riivolution, key))
}

/// At most [`SMALL_FILE_CAP`] bytes of a text file, UTF-8 lossy, opened
/// through the logged ROM opener (which refuses key material by name).
fn read_text(path: &Path) -> Option<String> {
    let file = crate::consoles::open_rom(path).ok()?;
    let mut bytes = Vec::new();
    file.take(SMALL_FILE_CAP).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The readme of a keyless patch: `<patch stem>.txt` (or `.nfo`) beside
/// it, else the one `readme*` text file of its folder. At most
/// [`README_MAX`] bytes, never key material, nothing opened.
fn readme_beside(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let small_file = |p: &Path| {
        fs::symlink_metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0 && m.len() <= README_MAX)
    };
    for ext in ["txt", "nfo"] {
        let p = dir.join(format!("{stem}.{ext}"));
        if small_file(&p) {
            return Some(p);
        }
    }
    let mut readmes: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            let text = matches!(ext_lower(&name).as_deref(), Some("txt" | "nfo" | "md"));
            (lower.starts_with("readme") && text && !is_key_material(&name)).then(|| e.path())
        })
        .filter(|p| small_file(p))
        .collect();
    readmes.sort();
    (readmes.len() == 1).then(|| readmes.remove(0))
}

/// What a folder is to the addon detection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DirAddon {
    Mod {
        kind: AddonKind,
        format: AddonFormat,
        key: AddonKey,
    },
    /// A mod folder holding key material by name: left whole, the key
    /// never opened (D18).
    HoldsKeys,
    /// Not a mod: walked as today.
    No,
}

/// The parent folder names a mod sniff looks at.
const MOD_PARENTS: [&str; 6] = [
    "rePatch",
    "reAddcont",
    "plugins",
    "exefs_patches",
    "GraphicMods",
    "textures",
];

/// Whether the folder `name` (in `parent`, listing `listed`) can be a mod
/// at all (D4), by names only: a mod sniff walks the folder's tree for key
/// names, so it runs only for names that can be a key or for a listing
/// that holds a mod's manifest — never for every folder.
pub(crate) fn may_be_mod_dir(name: &str, parent: Option<&str>, listed: &[(String, bool)]) -> bool {
    if FixedFolder::from_dir_name(name).is_some() {
        return false;
    }
    let has_file = |want: &str| {
        listed
            .iter()
            .any(|(n, dir)| !dir && n.eq_ignore_ascii_case(want))
    };
    let has_dir = |want: &str| {
        listed
            .iter()
            .any(|(n, dir)| *dir && n.eq_ignore_ascii_case(want))
    };
    if valid_title_id16(name).is_some()
        || parent.is_some_and(|p| MOD_PARENTS.iter().any(|m| m.eq_ignore_ascii_case(p)))
        || has_file("rules.txt")
        || has_file("hires.txt")
        || has_dir("riivolution")
    {
        return true;
    }
    if valid_sony_serial(name).is_some()
        && name.contains(['-', '_'])
        && (has_dir("replacements") || has_dir("dumps"))
    {
        return true;
    }
    // A game id as Dolphin and PPSSPP write it: upper-case letters and
    // digits («GALE01», «RMC», «ULUS10041»), so «Images» is never one.
    let typed_id = name
        .bytes()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    // A PSP id (`ULUS10041`, nine characters) is a PPSSPP pack only with
    // its `textures.ini`; a 3- or 6-character Dolphin id holds `tex1_`
    // files here or one folder down.
    if typed_id && name.len() == 9 && valid_game_id(name).is_some() {
        return has_file("textures.ini") || has_file("textures.zip");
    }
    if typed_id && valid_game_id(name).is_some() {
        let tex1 = listed
            .iter()
            .any(|(n, dir)| !dir && n.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("tex1_")));
        let sub = listed.iter().any(|(_, dir)| *dir);
        if tex1 || sub || has_file("textures.ini") || has_file("textures.zip") {
            return true;
        }
    }
    // N64 Rice pack: `<NAME>/<NAME>#<CRC>#….png`.
    let prefix = format!("{name}#");
    listed.iter().any(|(n, dir)| {
        !dir && n.len() > prefix.len()
            && n.is_char_boundary(prefix.len())
            && n[..prefix.len()].eq_ignore_ascii_case(&prefix)
    })
}

/// The mod sniff of one folder (D4, D18). A folder holding key material
/// is [`DirAddon::HoldsKeys`] only when it is a mod without its keys;
/// any other folder with a key is walked as today, its key left alone.
pub(crate) fn sniff_dir(path: &Path) -> DirAddon {
    match sniff_mod_dir_on_disk(path) {
        Ok(ModVerdict::Mod { format, kind, key }) => DirAddon::Mod { kind, format, key },
        Ok(ModVerdict::HoldsKeys) if is_mod_dir_without_keys(path).unwrap_or(false) => {
            DirAddon::HoldsKeys
        }
        _ => DirAddon::No,
    }
}

/// A mod folder as a candidate.
pub(crate) fn dir_candidate(
    path: &Path,
    kind: AddonKind,
    format: AddonFormat,
    key: AddonKey,
    filed_under: Option<PathBuf>,
) -> AddonCandidate {
    AddonCandidate {
        path: path.to_path_buf(),
        is_dir: true,
        kind,
        format,
        key,
        filed_under,
        members: Vec::new(),
        readme: None,
        partial: false,
    }
}

/// The MSU-1 pieces of one folder (D4): each set as one candidate (the
/// `.msu`, its `-N.pcm` tracks with the `MSU1` magic and a same-named
/// `.bml`), and each stray `.msu` or track as a partial one. `names` are
/// the folder's file names that [`is_msu1_piece`] accepts.
pub(crate) fn msu1_candidates(
    dir: &Path,
    names: &[String],
    filed_under: Option<&Path>,
) -> Vec<AddonCandidate> {
    // A `.pcm` without the MSU-1 magic is no track (4 bytes read).
    let names: Vec<String> = names
        .iter()
        .filter(|n| ext_lower(n).as_deref() != Some("pcm") || is_msu1_track(&dir.join(n)))
        .cloned()
        .collect();
    let (sets, strays) = crate::addons::mods::msu1_sets(&names);
    let mut out = Vec::new();
    for set in sets {
        let Some((msu, rest)) = set.files.split_first() else {
            continue;
        };
        out.push(AddonCandidate {
            path: dir.join(msu),
            is_dir: false,
            kind: AddonKind::Mod,
            format: AddonFormat::Msu1,
            key: AddonKey::RomName { name: set.name },
            filed_under: filed_under.map(Path::to_path_buf),
            members: rest.iter().map(PathBuf::from).collect(),
            readme: None,
            partial: false,
        });
    }
    for stray in strays {
        out.push(AddonCandidate {
            path: dir.join(&stray),
            is_dir: false,
            kind: AddonKind::Mod,
            format: AddonFormat::Msu1,
            key: AddonKey::None,
            filed_under: filed_under.map(Path::to_path_buf),
            members: Vec::new(),
            readme: None,
            partial: true,
        });
    }
    out
}

fn is_msu1_track(path: &Path) -> bool {
    let Ok(file) = crate::consoles::open_rom(path) else {
        return false;
    };
    let mut head = [0u8; 4];
    let mut take = file.take(4);
    let mut got = 0;
    while got < 4 {
        match take.read(&mut head[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    crate::addons::mods::is_msu1_pcm(&head[..got])
}

/// Why a candidate stays where it is when no game takes it (D6, D7): the
/// mapping from its key.
pub fn unlinked_why(c: &AddonCandidate) -> UnlinkedWhy {
    if c.partial {
        return UnlinkedWhy::Partial;
    }
    match &c.key {
        AddonKey::Many { count } => UnlinkedWhy::Many { count: *count },
        // A keyless patch is linked by its name or its readme (D5): none
        // matched.
        AddonKey::None if c.kind == AddonKind::Patch => UnlinkedWhy::NoMatch,
        AddonKey::None | AddonKey::BuildId { title_id: None, .. } => UnlinkedWhy::NoKey,
        _ => UnlinkedWhy::NoMatch,
    }
}

/// The left row of each candidate (and of each MSU-1 member) before any
/// link is made: a `-mods` overlay is the [`LeftReason::GameOverlay`] of
/// its base, an entry the user filed in a game folder's `Mods/` or
/// `Cheats/` is that folder's [`LeftReason::GameFolderContent`], any other
/// addon is [`LeftReason::AddonUnlinked`] with [`unlinked_why`]. The
/// plan's link pass takes over the candidates it links.
pub fn unlinked_rows(addons: &[AddonCandidate]) -> Vec<(PathBuf, LeftReason)> {
    let mut out = Vec::new();
    for c in addons {
        let reason = unlinked_reason(c);
        let dir = c.path.parent().unwrap_or(Path::new(""));
        out.extend(c.members.iter().map(|m| (dir.join(m), reason.clone())));
        out.push((c.path.clone(), reason));
    }
    out
}

/// The reason a candidate stays where it is until the plan links it (see
/// [`unlinked_rows`]).
fn unlinked_reason(c: &AddonCandidate) -> LeftReason {
    match &c.filed_under {
        Some(base) if base.parent() == c.path.parent() => {
            LeftReason::GameOverlay { of: name_of(base) }
        }
        Some(base) => {
            let folder = c
                .path
                .parent()
                .and_then(Path::file_name)
                .and_then(|n| FixedFolder::from_dir_name(&n.to_string_lossy()))
                .unwrap_or(c.kind.fixed_folder());
            // A folder game is its own game folder; a file base names it
            // by its stem.
            let of = if c.path.parent().and_then(Path::parent) == Some(base.as_path()) {
                name_of(base)
            } else {
                base.file_stem()
                    .map_or_else(|| name_of(base), |s| s.to_string_lossy().into_owned())
            };
            LeftReason::GameFolderContent { folder, of }
        }
        None => LeftReason::AddonUnlinked {
            kind: c.kind,
            format: c.format,
            why: unlinked_why(c),
        },
    }
}

/// One survey entry per candidate (mods spec D10): a mod folder is one
/// [`ItemKind::Folder`], an MSU-1 set one [`ItemKind::Set`] (the `.msu`
/// first, then its members), any other addon one [`ItemKind::File`]; each
/// with the reason it stays until the plan's link pass takes it.
pub fn unlinked_entries(addons: &[AddonCandidate]) -> Vec<(Vec<PathBuf>, ItemKind, LeftReason)> {
    addons
        .iter()
        .map(|c| {
            let dir = c.path.parent().unwrap_or(Path::new(""));
            let mut sources = vec![c.path.clone()];
            sources.extend(c.members.iter().map(|m| dir.join(m)));
            let kind = if c.is_dir {
                ItemKind::Folder
            } else if sources.len() > 1 {
                ItemKind::Set
            } else {
                ItemKind::File
            };
            (sources, kind, unlinked_reason(c))
        })
        .collect()
}

// --- The plan's link pass (D5, D6, D8, D9, D13) ---

#[cfg(any(test, feature = "test-util"))]
thread_local! {
    static HASHED: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Every ROM the link pass hashed on this thread since the last call
/// (tests: a BPS never hashes a ROM of another size, D5).
#[cfg(any(test, feature = "test-util"))]
pub fn take_addon_hashes() -> Vec<PathBuf> {
    HASHED.with(|h| std::mem::take(&mut *h.borrow_mut()))
}

/// The largest ROM hashed to test a patch's source CRC (D5).
pub const SOURCE_HASH_MAX: u64 = 512 << 20;

/// The systems an addon of `format` can belong to (its emulator's), or
/// none when the format says nothing (patches, RetroArch cheats).
pub fn systems_of(format: AddonFormat) -> &'static [&'static str] {
    use AddonFormat as F;
    match format {
        F::DuckStationCht => &["psx"],
        F::Pnach => &["ps2"],
        F::DolphinIni | F::DolphinGraphicMod | F::Riivolution => &["gamecube", "wii"],
        F::PpssppIni => &["psp"],
        F::CtrCheats | F::LayeredFs => &["3ds"],
        F::SwitchCheats | F::Pchtxt | F::SwitchMod | F::ExefsPatches => &["switch"],
        F::Rpcs3Patch => &["ps3"],
        F::Shadps4Cheat | F::Shadps4Patch | F::Shadps4Mods => &["ps4"],
        F::XeniaPatch | F::XeniaPlugin => &["xbox360"],
        F::Vita3kPatch | F::VitaRepatch => &["psvita"],
        F::MelonDsCheats | F::DesmumeCheats => &["nds"],
        F::MgbaCheats => &["gba", "gb", "gbc"],
        F::CemuGraphicPack => &["wiiu"],
        F::MesenHdPack | F::FceuxCht => &["nes"],
        F::Msu1 => &["snes"],
        F::N64HiresTextures => &["n64"],
        F::Bps
        | F::Ups
        | F::Ips
        | F::Xdelta
        | F::Ppf
        | F::Aps
        | F::RetroArchCht
        | F::CodeText
        | F::TexturePack => &[],
    }
}

/// The key as a row shows it (mono): «SLUS-00594», «0004000000055D00»,
/// «CRC32 1A2B3C4D».
pub fn key_text(key: &AddonKey) -> String {
    match key {
        AddonKey::SourceCrc { crc32, .. } => format!("CRC32 {crc32:08X}"),
        AddonKey::SourceName { name } | AddonKey::RomName { name } | AddonKey::N64Name { name } => {
            name.clone()
        }
        AddonKey::Serial { serial } => serial.clone(),
        AddonKey::GameId { id } | AddonKey::TitleId { id } | AddonKey::N64Id { id } => id.clone(),
        AddonKey::TitleIds { ids } => ids.join(", "),
        AddonKey::SerialVersion { serial, version } => format!("{serial} {version}"),
        AddonKey::BuildId { build_id, title_id } => match title_id {
            Some(t) => format!("{t} {build_id}"),
            None => build_id.clone(),
        },
        AddonKey::Many { .. } | AddonKey::None => String::new(),
    }
}

fn role_of(kind: AddonKind) -> GameRole {
    match kind {
        AddonKind::Cheat => GameRole::Cheat,
        AddonKind::Patch | AddonKind::Mod => GameRole::Mod,
    }
}

/// What the link pass needs of the plan around it.
pub(crate) struct LinkWorld<'a> {
    pub layout: &'a TargetLayout,
    pub db: &'a SystemDb,
    /// Bytes the pass may hash to test source CRCs (the plan's head
    /// budget).
    pub hash_budget: u64,
    /// The library bases of a target system folder, with their header ids
    /// (D9), asked only for systems with an addon the plan did not link.
    pub library: &'a mut dyn FnMut(&str) -> LibraryIndex,
}

/// Whether an item is still the row the survey gave an addon.
fn untouched(item: &OrganizeItem) -> bool {
    matches!(
        &item.action,
        OrganizeAction::Left {
            reason: LeftReason::AddonUnlinked { .. }
                | LeftReason::GameOverlay { .. }
                | LeftReason::GameFolderContent { .. }
        }
    )
}

fn hex_crc(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim(), 16).ok()
}

/// A plan item as a game an addon may link to.
fn plan_candidate(
    i: usize,
    item: &OrganizeItem,
    entry: &SurveyEntry,
    system: String,
    wanted: &BTreeSet<u64>,
    budget: &mut u64,
    w: &LinkWorld<'_>,
) -> Candidate {
    let file_name = item
        .sources
        .first()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = match item.kind {
        ItemKind::Folder => file_name.clone(),
        _ => Path::new(&file_name)
            .file_stem()
            .map_or_else(|| file_name.clone(), |s| s.to_string_lossy().into_owned()),
    };
    let tags = parse_tags(&stem);
    let console = entry.console.as_ref().filter(|c| c.system == system);
    let mut serials: Vec<String> = item
        .disc
        .as_ref()
        .map(|d| d.serials.clone())
        .unwrap_or_default();
    serials.extend(console.and_then(|c| c.serial.clone()));
    let raw_id = console.and_then(|c| c.title_id.clone());
    let mut cand = Candidate {
        game: GameRef::Item(i),
        system: system.clone(),
        file_name,
        stem,
        title_key: tags.title_key,
        regions: tags.regions,
        catalogue_name: item.catalogue_name.clone(),
        size: item.size,
        title_id: console.and_then(base_title_id),
        game_id: raw_id.filter(|_| matches!(system.as_str(), "gamecube" | "wii" | "psp")),
        serials,
        ..Candidate::default()
    };
    // Digests the survey already has (D5), else a fresh hash of a ROM of
    // a wanted size, within the budget.
    if let Some(facts) = entry.facts.as_ref().filter(|f| !f.raw_crcs.is_empty()) {
        cand.crc_full = facts.raw_crcs.first().and_then(|(c, _)| hex_crc(c));
        cand.crc_headerless = facts.raw_crcs.get(1).and_then(|(c, _)| hex_crc(c));
        cand.has_header = cand.crc_headerless.is_some();
        return cand;
    }
    let raw = item.kind == ItemKind::File
        && matches!(item.transfer, Transfer::AsIs)
        && entry
            .facts
            .as_ref()
            .is_none_or(|f| !f.inspection.container.is_archive());
    if !raw || !wanted.contains(&item.size) || item.size > SOURCE_HASH_MAX || *budget < item.size {
        return cand;
    }
    let Some(path) = entry.sources.first() else {
        return cand;
    };
    *budget -= item.size;
    let rule = w.db.get(&system).map(|s| s.header).unwrap_or_default();
    #[cfg(any(test, feature = "test-util"))]
    HASHED.with(|h| h.borrow_mut().push(path.clone()));
    let hashed = crate::consoles::open_rom(path)
        .and_then(|f| crate::identify::hash::hash_stream(f, item.size, rule));
    if let Ok(h) = hashed {
        cand.crc_full = hex_crc(&h.full.crc32);
        cand.crc_headerless = h.headerless.as_ref().and_then(|d| hex_crc(&d.crc32));
        cand.has_header = h.headerless.is_some();
    }
    cand
}

/// A library base as a game an addon may link to (D9).
fn library_candidate(system: &str, base: &LibraryBase) -> Candidate {
    let title = base.title.clone().unwrap_or_default();
    let tags = parse_tags(&title);
    let file_name = base
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Candidate {
        game: GameRef::Library(base.path.clone()),
        system: system.to_string(),
        file_name,
        stem: title,
        title_key: tags.title_key,
        regions: tags.regions,
        title_id: base.title_id.clone(),
        serials: base.serial.iter().cloned().collect(),
        game_id: base
            .raw_id
            .clone()
            .filter(|_| matches!(system, "gamecube" | "wii" | "psp")),
        ..Candidate::default()
    }
}

/// The readme beside a keyless patch, at most [`README_MAX`] bytes.
fn read_readme(path: &Path) -> Option<String> {
    let file = crate::consoles::open_rom(path).ok()?;
    let mut bytes = Vec::new();
    file.take(README_MAX).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// D5's order for one addon against `list`: its key, then a keyless
/// patch's readme, then a patch's name.
fn resolve_addon(
    c: &AddonCandidate,
    own_stem: &str,
    list: &[Candidate],
    budget_out: bool,
) -> Resolution {
    let none = |_: u32| Vec::new();
    let cands = Candidates {
        list,
        dat_by_crc: &none,
        budget_out,
    };
    let first = resolve(&c.key, None, own_stem, &cands);
    if matches!(first, Resolution::One { .. } | Resolution::Several(_))
        || c.kind != AddonKind::Patch
    {
        return first;
    }
    if c.key == AddonKey::None {
        if let Some(text) = c.readme.as_deref().and_then(read_readme) {
            let by_readme = resolve_by_readme(&text, &cands);
            if !matches!(by_readme, Resolution::None) {
                return by_readme;
            }
        }
        return first;
    }
    // A patch whose key found nothing: its name (D5 name fallback).
    match resolve(&AddonKey::None, None, own_stem, &cands) {
        Resolution::None => first,
        other => other,
    }
}

/// The link pass (D5, D6, D8, D9, D13), after demos and orphans and
/// before targets: every addon the survey left is linked to exactly one
/// game, a plan base first, then a library base of its system, or stays
/// with why. A linked addon moves in (`Auto`) as a [`GameRole::Mod`] or
/// [`GameRole::Cheat`] child ([`Subdir::Child`]: no target here) with its
/// [`AddonLink`]; one linked to a library base names it by
/// [`ItemNote::ModOf`], [`ItemNote::PatchOf`] or [`ItemNote::CheatOf`].
/// With game folders off, a user's own filing (a pile game folder's
/// `Mods/`, a shadPS4 `-mods` overlay) keeps part A's row.
pub(crate) fn link_addons(
    items: &mut [OrganizeItem],
    entries: &[SurveyEntry],
    addons: &[AddonCandidate],
    subdirs: &mut [Option<Subdir>],
    w: &mut LinkWorld<'_>,
) {
    if addons.is_empty() {
        return;
    }
    let by_path: HashMap<&Path, usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| Some((e.sources.first()?.as_path(), i)))
        .collect();
    let todo: Vec<(usize, &AddonCandidate)> = addons
        .iter()
        .filter_map(|c| {
            let i = *by_path.get(c.path.as_path())?;
            items.get(i).filter(|it| untouched(it)).map(|_| (i, c))
        })
        .collect();
    if todo.is_empty() {
        return;
    }
    let is_addon: HashSet<usize> = todo.iter().map(|(i, _)| *i).collect();
    let wanted: BTreeSet<u64> = todo
        .iter()
        .filter_map(|(_, c)| match c.key {
            AddonKey::SourceCrc { size, .. } => Some(wanted_sizes(size)),
            _ => None,
        })
        .flatten()
        .collect();
    let mut budget = w.hash_budget;
    let mut plan: Vec<Candidate> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let (Some(system), Some(entry)) = (item.system_id.clone(), entries.get(i)) else {
            continue;
        };
        let child = matches!(
            effective_kind(entry),
            TitleKind::Update | TitleKind::Dlc | TitleKind::System
        ) || item.game.as_ref().is_some_and(|g| g.role != GameRole::Base);
        if is_addon.contains(&i) || !item.moves_in() || child {
            continue;
        }
        plan.push(plan_candidate(
            i,
            item,
            entry,
            system,
            &wanted,
            &mut budget,
            w,
        ));
    }
    let mut libraries: HashMap<String, (Vec<Candidate>, bool)> = HashMap::new();
    for (pos, c) in todo {
        let link = AddonLink {
            kind: c.kind,
            format: c.format,
            key_text: key_text(&c.key),
            via: LinkVia::Key,
            header: None,
        };
        // D13: the user's own filing, or a shadPS4 overlay beside its base.
        if let Some(base_path) = &c.filed_under {
            if !w.layout.game_folders {
                continue;
            }
            let Some(&b) = by_path.get(base_path.as_path()) else {
                continue;
            };
            // A folder game's own fixed folder travels inside it.
            if !items[b].moves_in()
                || (items[b].kind == ItemKind::Folder && c.path.starts_with(base_path))
            {
                continue;
            }
            let overlay = base_path.parent() == c.path.parent();
            let via = if overlay {
                LinkVia::Key
            } else {
                LinkVia::Folder
            };
            link_to_item(items, pos, b, AddonLink { via, ..link });
            subdirs[pos] = Some(Subdir::Child);
            continue;
        }
        let systems = systems_of(c.format);
        let own = c
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let own_stem = if c.is_dir {
            own.clone()
        } else {
            Path::new(&own)
                .file_stem()
                .map_or_else(|| own.clone(), |s| s.to_string_lossy().into_owned())
        };
        let in_systems =
            |cand: &&Candidate| systems.is_empty() || systems.contains(&cand.system.as_str());
        let list: Vec<Candidate> = plan.iter().filter(in_systems).cloned().collect();
        let mut res = resolve_addon(c, &own_stem, &list, false);
        if !matches!(res, Resolution::One { .. } | Resolution::Several(_)) && !systems.is_empty() {
            let mut list = list;
            let mut out = false;
            for system in systems {
                let (cands, incomplete) =
                    libraries.entry((*system).to_string()).or_insert_with(|| {
                        let index = (w.library)(system);
                        let cands = index
                            .bases()
                            .iter()
                            .map(|b| library_candidate(system, b))
                            .collect();
                        (cands, index.incomplete)
                    });
                list.extend(cands.iter().cloned());
                out |= *incomplete;
            }
            res = resolve_addon(c, &own_stem, &list, out);
        }
        match res {
            Resolution::One {
                game: GameRef::Item(b),
                via,
                header,
            } => {
                link_to_item(
                    items,
                    pos,
                    b,
                    AddonLink {
                        via,
                        header,
                        ..link
                    },
                );
                subdirs[pos] = Some(Subdir::Child);
            }
            Resolution::One {
                game: GameRef::Library(path),
                via,
                header,
            } => {
                let Some((system, base)) = systems.iter().find_map(|s| {
                    let found = libraries
                        .get(*s)?
                        .0
                        .iter()
                        .find(|cand| cand.game == GameRef::Library(path.clone()))?;
                    Some((*s, found.stem.clone()))
                }) else {
                    continue;
                };
                let in_game_folder = fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir());
                let (system_dir, choice) = super::target::folder_dir(w.layout, w.db, system);
                let it = &mut items[pos];
                it.action = OrganizeAction::Auto;
                it.system_id = Some(system.to_string());
                it.folder = system_dir
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .or(Some(choice.folder));
                let mut game = GameLink::new(role_of(c.kind));
                if in_game_folder {
                    game.folder = Some(
                        path.strip_prefix(&w.layout.target_root)
                            .map_or_else(|_| path.clone(), Path::to_path_buf),
                    );
                    game.folder_exists = true;
                }
                game.addon = Some(AddonLink {
                    via,
                    header,
                    ..link
                });
                it.game = Some(game);
                let note = match c.kind {
                    AddonKind::Mod => ItemNote::ModOf { of: base },
                    AddonKind::Patch => ItemNote::PatchOf { of: base },
                    AddonKind::Cheat => ItemNote::CheatOf { of: base },
                };
                it.notes.push(note);
                header_note(it, header);
                subdirs[pos] = Some(Subdir::Child);
            }
            Resolution::Several(count) => {
                items[pos].leave(LeftReason::AddonUnlinked {
                    kind: c.kind,
                    format: c.format,
                    why: UnlinkedWhy::Several { count },
                });
            }
            Resolution::Budget => {
                items[pos].leave(LeftReason::AddonUnlinked {
                    kind: c.kind,
                    format: c.format,
                    why: UnlinkedWhy::Budget,
                });
            }
            Resolution::None => {}
        }
    }
}

/// The row says when a patch expects the ROM with or without its header
/// (D8).
fn header_note(item: &mut OrganizeItem, header: Option<HeaderFit>) {
    if let Some(needs @ (HeaderFit::NeedsHeaderless | HeaderFit::NeedsHeadered)) = header {
        item.notes.push(ItemNote::HeaderMismatch { needs });
    }
}

fn link_to_item(items: &mut [OrganizeItem], pos: usize, b: usize, link: AddonLink) {
    let system = items[b].system_id.clone();
    let region = items[b].game.as_ref().and_then(|g| g.region);
    let header = link.header;
    let it = &mut items[pos];
    it.action = OrganizeAction::Auto;
    it.system_id = system;
    let mut game = GameLink::new(role_of(link.kind));
    game.of = Some(b);
    game.region = region;
    game.addon = Some(link);
    it.game = Some(game);
    header_note(it, header);
}

/// Master placement (D11, D12), after the game folders' children: every
/// linked addon goes to `<game folder>/Mods/<own name>` or
/// `<game folder>/Cheats/<own name>`, never renamed (its name is often
/// its key). The fixed folder is reused under its spelling on disk; a
/// name taken there is *already present* (same bytes) or
/// [`LeftReason::AddonNameTaken`], never ` (2)`. No game folder (game
/// folders off, a flat or retro base): [`LeftReason::AddonNoPlace`]. A
/// base that stays leaves its addons with its reason.
pub(crate) fn addon_targets(
    items: &mut [OrganizeItem],
    layout: &TargetLayout,
    index: &mut DirIndex,
    src: &Path,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) {
    for pos in 0..items.len() {
        let Some(link) = items[pos].game.clone() else {
            continue;
        };
        if !matches!(link.role, GameRole::Mod | GameRole::Cheat)
            || !items[pos].moves_in()
            || items[pos].target.is_some()
        {
            continue;
        }
        let kind = link.addon.as_ref().map_or(AddonKind::Mod, |a| a.kind);
        let no_place = LeftReason::AddonNoPlace { kind };
        if !layout.game_folders {
            items[pos].leave(no_place);
            continue;
        }
        let (system_folder, folder, folder_exists) = match link.of {
            Some(b) => {
                let Some(base) = items.get(b) else {
                    continue;
                };
                match super::target::base_game_folder(base) {
                    Some((folder, exists)) => (base.folder.clone(), folder, exists),
                    None => {
                        let reason = match &base.action {
                            OrganizeAction::Left { reason } => reason.clone(),
                            OrganizeAction::SetAside { .. } => LeftReason::Unknown,
                            _ => no_place,
                        };
                        items[pos].leave(reason);
                        continue;
                    }
                }
            }
            None => match link.folder.clone() {
                Some(folder) => (items[pos].folder.clone(), folder, link.folder_exists),
                None => {
                    items[pos].leave(no_place);
                    continue;
                }
            },
        };
        let folder_abs = layout.target_root.join(&folder);
        let fixed = kind.fixed_folder();
        let fixed_dir = match index.existing_first(&folder_abs, fixed.dir_name()) {
            Some(existing) => {
                if !fs::symlink_metadata(&existing).is_ok_and(|m| m.is_dir()) {
                    items[pos].leave(LeftReason::UnsafeTarget);
                    continue;
                }
                existing
            }
            None => folder_abs.join(fixed.dir_name()),
        };
        let item = &items[pos];
        let names: Vec<String> = item.sources.iter().map(|s| name_of(s)).collect();
        let rel = |p: &Path| {
            p.strip_prefix(&layout.target_root)
                .map_or_else(|_| p.to_path_buf(), Path::to_path_buf)
        };
        // Already where it belongs (a library organised in place): it
        // stays, it is no move.
        if layout.target_root.join(rel(&fixed_dir.join(&names[0]))) == src.join(&item.sources[0]) {
            let at = rel(&fixed_dir.join(&names[0]));
            items[pos].leave(LeftReason::AlreadyPresent { at });
            continue;
        }
        let mut outcome: Option<LeftReason> = None;
        for name in &names {
            let path = fixed_dir.join(name);
            if let Some(r) =
                super::target::check_lengths(&layout.target_root, &path, layout.windows_paths)
            {
                outcome = Some(r);
                break;
            }
            if index.taken(&fixed_dir, name) {
                let present = item.kind == ItemKind::File
                    && index
                        .existing(&fixed_dir, name)
                        .is_some_and(|e| same_content(&e, item));
                outcome = Some(if present {
                    LeftReason::AlreadyPresent { at: rel(&path) }
                } else {
                    LeftReason::AddonNameTaken {
                        at: rel(&fixed_dir),
                    }
                });
                break;
            }
        }
        if let Some(reason) = outcome {
            items[pos].leave(reason);
            continue;
        }
        index.claim(&folder_abs, &name_of(&fixed_dir));
        for name in &names {
            index.claim(&fixed_dir, name);
        }
        let item = &mut items[pos];
        let mut targets = names.iter().map(|n| rel(&fixed_dir.join(n)));
        item.target = targets.next();
        item.set_targets = targets.collect();
        item.companions.clear();
        item.rename = None;
        item.folder = system_folder;
        if let Some(game) = item.game.as_mut() {
            game.folder = Some(folder);
            game.folder_exists = folder_exists;
        }
    }
}

/// D11, Q13: with game folders on, a base's same-stem cheats (`.cht`) go
/// in its game folder's `Cheats/` and its same-stem patches in `Mods/`,
/// under their own names, instead of the folder's root. A name taken
/// there keeps today's place.
pub(crate) fn fixed_folder_companions(
    items: &mut [OrganizeItem],
    layout: &TargetLayout,
    index: &mut DirIndex,
) {
    for item in items.iter_mut() {
        let Some(folder) = item
            .game
            .as_ref()
            .filter(|g| g.role == GameRole::Base)
            .and_then(|g| g.folder.clone())
        else {
            continue;
        };
        if !item.moves_in() || item.target.is_none() || item.kind == ItemKind::Folder {
            continue;
        }
        let folder_abs = layout.target_root.join(&folder);
        for comp in item.companions.iter_mut().filter(|c| !c.licence) {
            let name = name_of(&comp.source);
            let fixed = if ext_lower(&name).as_deref() == Some("cht") {
                FixedFolder::Cheats
            } else if is_patch_name(&name) {
                FixedFolder::Mods
            } else {
                continue;
            };
            let dir = match index.existing_first(&folder_abs, fixed.dir_name()) {
                Some(existing) if fs::symlink_metadata(&existing).is_ok_and(|m| m.is_dir()) => {
                    existing
                }
                Some(_) => continue,
                None => folder_abs.join(fixed.dir_name()),
            };
            if index.taken(&dir, &name) {
                continue;
            }
            index.claim(&folder_abs, &name_of(&dir));
            index.claim(&dir, &name);
            comp.target = dir
                .join(&name)
                .strip_prefix(&layout.target_root)
                .map_or_else(
                    |_| folder.join(fixed.dir_name()).join(&name),
                    Path::to_path_buf,
                );
        }
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(key: AddonKey, partial: bool) -> AddonCandidate {
        AddonCandidate {
            path: PathBuf::from("p"),
            is_dir: false,
            kind: AddonKind::Cheat,
            format: AddonFormat::Rpcs3Patch,
            key,
            filed_under: None,
            members: Vec::new(),
            readme: None,
            partial,
        }
    }

    #[test]
    fn the_unlinked_reason_follows_the_key() {
        assert_eq!(
            unlinked_why(&cand(AddonKey::Many { count: 400 }, false)),
            UnlinkedWhy::Many { count: 400 }
        );
        assert_eq!(
            unlinked_why(&cand(AddonKey::None, false)),
            UnlinkedWhy::NoKey
        );
        let mut ips = cand(AddonKey::None, false);
        ips.kind = AddonKind::Patch;
        ips.format = AddonFormat::Ips;
        assert_eq!(unlinked_why(&ips), UnlinkedWhy::NoMatch);
        assert_eq!(
            unlinked_why(&cand(
                AddonKey::BuildId {
                    build_id: "AB".repeat(16),
                    title_id: None
                },
                false
            )),
            UnlinkedWhy::NoKey
        );
        assert_eq!(
            unlinked_why(&cand(
                AddonKey::Serial {
                    serial: "SLUS-00594".into()
                },
                false
            )),
            UnlinkedWhy::NoMatch
        );
        assert_eq!(
            unlinked_why(&cand(AddonKey::RomName { name: "x".into() }, true)),
            UnlinkedWhy::Partial
        );
    }

    #[test]
    fn patch_names_and_chain_names() {
        for n in [
            "a.ips", "a.BPS", "a.ups", "a.xdelta", "a.ppf", "a.aps", "a.ips3",
        ] {
            assert!(is_patch_name(n), "{n}");
        }
        for n in ["a.sfc", "ips", ".ips", "a.ips0", "a.ipsx"] {
            assert!(!is_patch_name(n), "{n}");
        }
    }

    #[test]
    fn mod_dir_prefilter_takes_keys_and_manifests_only() {
        let e = |list: &[&str]| -> Vec<(String, bool)> {
            list.iter()
                .map(|n| match n.strip_suffix('/') {
                    Some(d) => (d.to_string(), true),
                    None => (n.to_string(), false),
                })
                .collect()
        };
        assert!(may_be_mod_dir("0004000000055D00", None, &e(&["romfs/"])));
        assert!(may_be_mod_dir("SLUS-00594", Some("textures"), &e(&[])));
        assert!(may_be_mod_dir("SLUS-00594", None, &e(&["replacements/"])));
        assert!(may_be_mod_dir("GALE01", None, &e(&["tex1_1.png"])));
        assert!(may_be_mod_dir("ULUS10041", None, &e(&["textures.ini"])));
        assert!(may_be_mod_dir("Pack", None, &e(&["rules.txt"])));
        assert!(may_be_mod_dir("SMB HD", None, &e(&["hires.txt"])));
        assert!(may_be_mod_dir(
            "MARIOKART64",
            None,
            &e(&["MARIOKART64#1#0.png"])
        ));
        // Ordinary folders are never sniffed.
        assert!(!may_be_mod_dir("Images", None, &e(&["a/", "b.png"])));
        assert!(!may_be_mod_dir("CUSA00900", None, &e(&["sce_sys/"])));
        assert!(!may_be_mod_dir("SLUS-00594", None, &e(&["a.bin"])));
        assert!(!may_be_mod_dir("Mods", None, &e(&["rules.txt"])));
        assert!(!may_be_mod_dir("SNES", None, &e(&["a.sfc"])));
    }

    #[test]
    fn a_loose_riivolution_xml_is_partial() {
        let xml = "<wiidisc version=\"1\"><id game=\"RMC\"/></wiidisc>";
        assert_eq!(
            riivolution_xml("ctgp.xml", xml),
            Some((
                AddonFormat::Riivolution,
                AddonKey::GameId { id: "RMC".into() }
            ))
        );
        assert_eq!(riivolution_xml("ctgp.txt", xml), None);
        assert_eq!(riivolution_xml("x.xml", "<patch/>"), None);
        let c = file_candidate(
            Path::new("/p/ctgp.xml"),
            (AddonFormat::Riivolution, AddonKey::None),
            None,
        );
        assert!(c.partial);
        assert_eq!(unlinked_why(&c), UnlinkedWhy::Partial);
    }

    #[test]
    fn rows_name_the_overlay_base_and_the_filed_folder() {
        let root = Path::new("/pile");
        let mut overlay = cand(AddonKey::None, false);
        overlay.path = root.join("CUSA00900-mods");
        overlay.filed_under = Some(root.join("CUSA00900"));
        let mut filed = cand(AddonKey::None, false);
        filed.path = root.join("Pokemon Y").join("Mods").join("m.ips");
        filed.filed_under = Some(root.join("Pokemon Y").join("Pokemon Y.cia"));
        let mut folder_game = cand(AddonKey::None, false);
        folder_game.path = root.join("CUSA1").join("Cheats").join("c.json");
        folder_game.filed_under = Some(root.join("CUSA1"));
        let mut msu = cand(AddonKey::RomName { name: "g".into() }, false);
        msu.path = root.join("g.msu");
        msu.members = vec![PathBuf::from("g-1.pcm")];
        let rows = unlinked_rows(&[overlay, filed, folder_game, msu]);
        assert_eq!(
            rows[0],
            (
                root.join("CUSA00900-mods"),
                LeftReason::GameOverlay {
                    of: "CUSA00900".into()
                }
            )
        );
        assert_eq!(
            rows[1].1,
            LeftReason::GameFolderContent {
                folder: FixedFolder::Mods,
                of: "Pokemon Y".into()
            }
        );
        assert_eq!(
            rows[2].1,
            LeftReason::GameFolderContent {
                folder: FixedFolder::Cheats,
                of: "CUSA1".into()
            }
        );
        assert_eq!(rows[3].0, root.join("g-1.pcm"));
        assert_eq!(rows[4].0, root.join("g.msu"));
        assert!(matches!(
            rows[4].1,
            LeftReason::AddonUnlinked {
                why: UnlinkedWhy::NoMatch,
                ..
            }
        ));
    }
}
