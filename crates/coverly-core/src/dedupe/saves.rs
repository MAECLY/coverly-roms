//! Saves of a unit (duplicates spec §8.2, §8.3): where they are found,
//! when a save is blank (and so counts as no save), and when two copies
//! hold saves that differ (a conflict: nothing is touched).
//!
//! A save is tied to its unit by the unit's stem (same folder, or the
//! card's `saves/` and `states/` folders and their per-core subfolders),
//! by a disc serial (DuckStation `memcards/<serial>*.mcd`, PPSSPP
//! `PSP/SAVEDATA/<serial>*/`) or by an arcade set (`nvram/<set>/`). Saves
//! keyed by serial or set are shared by every copy of the game: they always
//! stay and never make a conflict.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use super::units::{mtime_ns, Unit};
use super::UnitKind;

/// How a save was tied to its unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SaveKey {
    /// Same file stem as the ROM.
    Stem,
    /// Named after the disc serial (DuckStation memory cards).
    Serial,
    /// Named after the arcade set.
    Set,
}

/// A real save or state of a unit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveInfo {
    pub path: PathBuf,
    pub size: u64,
    pub mtime_ns: i64,
    /// Lowercase hex.
    pub sha1: String,
    pub keyed_by: SaveKey,
}

/// Where a profile's device may keep saves besides the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum OffCard {
    /// Every save is on the card, where the scan sees it.
    None,
    /// Saves may live on the device's internal storage: having a save
    /// cannot be seen, only having played.
    Internal,
}

/// Off-card save locations per profile id (plan DD-5). Profiles not
/// listed keep their saves on the card.
pub const SAVE_LOCATIONS: &[(&str, OffCard)] = &[
    ("muos", OffCard::Internal),
    ("anbernic-stock", OffCard::None),
];

/// Where `profile_id` may keep saves besides the card.
pub fn off_card(profile_id: &str) -> OffCard {
    SAVE_LOCATIONS
        .iter()
        .find(|(id, _)| *id == profile_id)
        .map_or(OffCard::None, |(_, at)| *at)
}

/// Save extensions (spec §8.2), compared case-insensitively against
/// everything after a dot that follows the stem. `state`, `state1`,
/// `state.auto`… are matched by prefix.
pub const SAVE_EXTS: [&str; 10] = [
    "sav", "srm", "rtc", "eep", "sra", "fla", "mpk", "dsv", "mcd", "mcr",
];

/// Folders of the card root holding saves named after the ROM stem, flat
/// or in per-core subfolders (RetroArch "sort by core").
const STEM_DIRS: [&str; 2] = ["saves", "states"];

/// Size of a PS1 memory card image.
const PS_CARD_LEN: u64 = 128 * 1024;
/// Header of a DexDrive (`.gme`) card before the card itself.
const GME_HEADER_LEN: u64 = 3904;
const PS_FRAME: usize = 128;
/// Bytes of a card that hold the header frame and the fifteen directory
/// frames.
const PS_DIRECTORY_LEN: usize = 16 * PS_FRAME;

/// Whether `suffix` (everything after the stem's dot) names a save.
fn is_save_suffix(suffix: &str) -> bool {
    let suffix = suffix.to_ascii_lowercase();
    suffix.starts_with("state") || SAVE_EXTS.contains(&suffix.as_str())
}

/// Whether a save holds nothing: empty, all `0x00`, all `0xFF`, or a PS1
/// memory card (`MC` header; `.gme` after its 3904-byte header) with no
/// used directory frame. Reads `reader` to the end; `len` is the expected
/// length.
pub fn is_blank(mut reader: impl Read, len: u64, ext: &str) -> io::Result<bool> {
    let skip = if ext.eq_ignore_ascii_case("gme") {
        GME_HEADER_LEN
    } else {
        0
    };
    let maybe_card = len == skip + PS_CARD_LEN;
    // `skip` is at most 3904, so the sum fits any usize.
    let head_len = usize::try_from(skip).unwrap_or(usize::MAX) + PS_DIRECTORY_LEN;
    let mut head = Vec::new();
    let (mut all_zero, mut all_ff) = (true, true);
    let mut total = 0u64;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        let chunk = &buf[..n];
        all_zero = all_zero && chunk.iter().all(|b| *b == 0);
        all_ff = all_ff && chunk.iter().all(|b| *b == 0xFF);
        if maybe_card && head.len() < head_len {
            let take = (head_len - head.len()).min(n);
            head.extend_from_slice(&chunk[..take]);
        }
        total += n as u64;
    }
    if total == 0 || all_zero || all_ff {
        return Ok(true);
    }
    Ok(maybe_card && total == len && ps_card_is_empty(&head[head_len - PS_DIRECTORY_LEN..]))
}

/// `card` starts at the header frame and holds the directory frames.
fn ps_card_is_empty(card: &[u8]) -> bool {
    if card.len() < PS_DIRECTORY_LEN || &card[..2] != b"MC" {
        return false;
    }
    // Allocation state 0x51/0x52/0x53: first, middle, last block in use.
    // 0xA0 is free and 0xA1–0xA3 deleted.
    (1..=15).all(|i| card[i * PS_FRAME] & 0xF0 != 0x50)
}

/// Two or more members with real saves that differ (spec §8.3): every
/// save of those members, sorted by path. Only saves tied by stem count;
/// saves tied by serial or set are shared and stay. Byte-identical saves
/// are no conflict: the keeper keeps its own.
pub fn conflict(members: &[(usize, Vec<SaveInfo>)]) -> Option<Vec<SaveInfo>> {
    let with: Vec<Vec<&SaveInfo>> = members
        .iter()
        .map(|(_, saves)| {
            saves
                .iter()
                .filter(|s| s.keyed_by == SaveKey::Stem)
                .collect::<Vec<_>>()
        })
        .filter(|saves| !saves.is_empty())
        .collect();
    if with.len() < 2 {
        return None;
    }
    let contents: Vec<BTreeSet<&str>> = with
        .iter()
        .map(|saves| saves.iter().map(|s| s.sha1.as_str()).collect())
        .collect();
    if contents.windows(2).all(|w| w[0] == w[1]) {
        return None;
    }
    let mut out: Vec<SaveInfo> = with.into_iter().flatten().cloned().collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    Some(out)
}

/// The real saves of `unit` (blank saves left out), sorted by path.
/// Builds a one-off [`SaveIndex`]; use the index to look up many units.
pub fn saves_for(unit: &Unit, profile_id: &str, root: &Path, serials: &[String]) -> Vec<SaveInfo> {
    SaveIndex::new(profile_id, root).saves_for(unit, serials)
}

/// File names of one folder: lowercase stem candidate (the name up to any
/// dot) → `(path, suffix after that dot)`.
type StemMap = HashMap<String, Vec<(PathBuf, String)>>;

/// Size, mtime and lowercase SHA-1 of a real save.
type Hashed = (u64, i64, String);

/// The save folders of a library root, listed once and shared by every
/// unit looked up. Files are hashed once.
#[derive(Debug)]
pub struct SaveIndex {
    off_card: OffCard,
    /// `saves/`, `states/` and their subfolders.
    stems: StemMap,
    /// Name to match a serial against → files: `memcards/` files and the
    /// files inside each `PSP/SAVEDATA/` folder.
    serials: Vec<(String, Vec<PathBuf>)>,
    /// Lowercase set name → files under `nvram/<set>/`.
    sets: HashMap<String, Vec<PathBuf>>,
    folders: Mutex<HashMap<PathBuf, Arc<StemMap>>>,
    /// Hashed files: `None` when blank or unreadable.
    hashed: Mutex<HashMap<PathBuf, Option<Hashed>>>,
}

impl SaveIndex {
    /// Lists the save folders under `root` (the library root, the card).
    pub fn new(profile_id: &str, root: &Path) -> SaveIndex {
        let mut stems = StemMap::new();
        for name in STEM_DIRS {
            for dir in child_dirs_named(root, name) {
                add_stems(&mut stems, &dir);
                for sub in subdirs(&dir) {
                    add_stems(&mut stems, &sub);
                }
            }
        }
        let mut serials = Vec::new();
        for dir in child_dirs_named(root, "memcards") {
            for file in files_in(&dir) {
                if let Some(stem) = file.file_stem() {
                    serials.push((stem.to_string_lossy().into_owned(), vec![file.clone()]));
                }
            }
        }
        for psp in child_dirs_named(root, "psp") {
            for savedata in child_dirs_named(&psp, "savedata") {
                for game in subdirs(&savedata) {
                    let name = dir_name(&game);
                    serials.push((name, files_in(&game)));
                }
            }
        }
        let mut sets = HashMap::new();
        for nvram in child_dirs_named(root, "nvram") {
            for set in subdirs(&nvram) {
                sets.entry(dir_name(&set).to_lowercase())
                    .or_insert_with(Vec::new)
                    .extend(files_in(&set));
            }
        }
        SaveIndex {
            off_card: off_card(profile_id),
            stems,
            serials,
            sets,
            folders: Mutex::new(HashMap::new()),
            hashed: Mutex::new(HashMap::new()),
        }
    }

    /// Where the profile's device may keep saves besides the card.
    pub fn off_card(&self) -> OffCard {
        self.off_card
    }

    /// The real saves of `unit` (blank saves left out), sorted by path.
    /// `serials` are the unit's disc serials, in any spelling
    /// (`SCUS-94570`, `SCUS_945.70`, `scus94570`).
    pub fn saves_for(&self, unit: &Unit, serials: &[String]) -> Vec<SaveInfo> {
        let mut found: Vec<(PathBuf, SaveKey)> = Vec::new();
        for file in &unit.files {
            let Some(stem) = file.file_stem() else {
                continue;
            };
            let stem = stem.to_string_lossy().to_lowercase();
            let mut candidates: Vec<&(PathBuf, String)> = Vec::new();
            let folder = file.parent().map(|dir| self.folder(dir));
            if let Some(list) = folder.as_ref().and_then(|f| f.get(&stem)) {
                candidates.extend(list);
            }
            if let Some(list) = self.stems.get(&stem) {
                candidates.extend(list);
            }
            found.extend(
                candidates
                    .into_iter()
                    .filter(|(_, suffix)| is_save_suffix(suffix))
                    .map(|(path, _)| (path.clone(), SaveKey::Stem)),
            );
        }
        let wanted: Vec<String> = serials.iter().map(|s| alnum_upper(s)).collect();
        for (name, files) in &self.serials {
            if wanted
                .iter()
                .any(|s| !s.is_empty() && serial_prefix(name, s))
            {
                found.extend(files.iter().map(|f| (f.clone(), SaveKey::Serial)));
            }
        }
        if unit.kind == UnitKind::Arcade {
            if let Some(files) = self.sets.get(&unit.stem.to_lowercase()) {
                found.extend(files.iter().map(|f| (f.clone(), SaveKey::Set)));
            }
        }
        found.retain(|(path, _)| !unit.files.contains(path));
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found.dedup_by(|a, b| a.0 == b.0);
        found
            .into_iter()
            .filter_map(|(path, keyed_by)| {
                let (size, mtime_ns, sha1) = self.hash(&path)?;
                Some(SaveInfo {
                    path,
                    size,
                    mtime_ns,
                    sha1,
                    keyed_by,
                })
            })
            .collect()
    }

    fn folder(&self, dir: &Path) -> Arc<StemMap> {
        let mut folders = self.folders.lock().unwrap_or_else(|e| e.into_inner());
        folders
            .entry(dir.to_path_buf())
            .or_insert_with(|| {
                let mut map = StemMap::new();
                add_stems(&mut map, dir);
                Arc::new(map)
            })
            .clone()
    }

    /// Size, mtime and lowercase SHA-1 of a real save; `None` when the
    /// file is blank or cannot be read (an unreadable file decides
    /// nothing; it still travels as a companion).
    fn hash(&self, path: &Path) -> Option<Hashed> {
        let mut hashed = self.hashed.lock().unwrap_or_else(|e| e.into_inner());
        hashed
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let meta = fs::metadata(path).ok()?;
                let bytes = fs::read(path).ok()?;
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let len = bytes.len() as u64;
                if is_blank(Cursor::new(&bytes), len, &ext).ok()? {
                    return None;
                }
                Some((len, mtime_ns(&meta), hex_lower(&Sha1::digest(&bytes))))
            })
            .clone()
    }
}

/// Adds the files of `dir` to `map` under every stem candidate: the name
/// up to each dot (`Game.state.auto` → `game` with `state.auto`, and
/// `game.state` with `auto`).
fn add_stems(map: &mut StemMap, dir: &Path) {
    for file in files_in(dir) {
        let Some(name) = file.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        for (at, _) in name.match_indices('.').filter(|(at, _)| *at > 0) {
            map.entry(name[..at].to_lowercase())
                .or_default()
                .push((file.clone(), name[at + 1..].to_string()));
        }
    }
}

/// Regular files of `dir`, hidden files (`._` twins, `.DS_Store`) left
/// out, sorted.
fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = entries(dir)
        .into_iter()
        .filter(|(_, is_dir)| !is_dir)
        .map(|(path, _)| path)
        .collect();
    out.sort();
    out
}

/// Subfolders of `dir`, hidden ones left out, sorted.
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = entries(dir)
        .into_iter()
        .filter(|(_, is_dir)| *is_dir)
        .map(|(path, _)| path)
        .collect();
    out.sort();
    out
}

/// Subfolders of `dir` named `name`, compared case-insensitively.
fn child_dirs_named(dir: &Path, name: &str) -> Vec<PathBuf> {
    subdirs(dir)
        .into_iter()
        .filter(|d| dir_name(d).eq_ignore_ascii_case(name))
        .collect()
}

/// Entries of `dir` that are not hidden, with whether each is a folder.
/// Symlinks are followed for the kind; an unreadable folder is empty.
fn entries(dir: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    read.filter_map(Result::ok)
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| {
            let path = e.path();
            let meta = fs::metadata(&path).ok()?;
            Some((path, meta.is_dir()))
        })
        .collect()
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// ASCII letters and digits of `s`, upper-cased.
fn alnum_upper(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Whether `name` starts with `serial` (letters and digits only, upper
/// case), ignoring separators in `name`, and the serial ends there:
/// `SCUS-94570_1` and `ULUS10046DATA00` match, `SCUS-945701_1` does not.
fn serial_prefix(name: &str, serial: &str) -> bool {
    let mut want = serial.chars().peekable();
    let mut rest = name.chars();
    while want.peek().is_some() {
        let Some(c) = rest.next() else {
            return false;
        };
        if !c.is_ascii_alphanumeric() {
            continue;
        }
        if want.next() != Some(c.to_ascii_uppercase()) {
            return false;
        }
    }
    // The serial must not run on into more digits; a letter may follow
    // (PPSSPP appends `DATA00`).
    !rest.next().is_some_and(|c| c.is_ascii_digit())
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_suffixes() {
        for s in ["srm", "SAV", "state", "state1", "state.auto", "mcd", "dsv"] {
            assert!(is_save_suffix(s), "{s}");
        }
        for s in ["cht", "png", "txt", "ips", "zip", "srm.bak"] {
            assert!(!is_save_suffix(s), "{s}");
        }
    }

    #[test]
    fn serials_match_any_spelling_and_end_there() {
        assert!(serial_prefix("SCUS-94570_1", "SCUS94570"));
        assert!(serial_prefix("scus_945.70", "SCUS94570"));
        assert!(serial_prefix("ULUS10046DATA00", "ULUS10046"));
        assert!(!serial_prefix("SCUS-945701_1", "SCUS94570"));
        assert!(!serial_prefix("SCUS-9457", "SCUS94570"));
        assert!(!serial_prefix("SLUS-01161_1", "SCUS94570"));
    }
}
