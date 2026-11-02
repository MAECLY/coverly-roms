//! Folder games inside zip and 7z archives (adjustments spec D20–D23).
//!
//! The folder probes of `consoles` run unchanged over an [`ArchiveFs`]: a
//! virtual tree built from the entries the inspection already listed (no
//! second open of the archive), whose reads go through the archive layer
//! under the plan head [`Budget`]. A name pass over the listing finds the
//! candidate roots first (spec §8.4), so an ordinary cartridge zip pays
//! nothing more than one look at each name.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::archive::{ArchiveError, ArchiveReader, Budget, Entry};
use crate::consoles::{
    identify_dir, ArtFormat, ArtLocator, ConsoleId, DirConflict, DirListing, EntryMeta,
    FolderMatch, ListingFs, MAX_CHILD_ENTRIES,
};
use crate::identify::sniff::ReadAt;

/// Deepest root a marker may sit under (spec D21): `a/b/c/PS3_GAME/…`.
pub const MAX_ROOT_DEPTH: usize = 3;

/// Most bytes [`ArchiveFs::open`] decodes of one entry: probes that read
/// fields past the head (XBE, XEX) see at most this much, and never more
/// than the budget has left.
const OPEN_CAP: u64 = 1 << 20;

/// The Vita3K installer aborts on this module (spec §8.4).
pub const VITA_STEROID: &str = "sce_module/steroid.suprx";

/// One folder game found inside an archive.
#[derive(Clone, Debug, PartialEq)]
pub struct ArchiveFolderGame {
    /// The root inside the archive, `/`-separated, without a trailing
    /// `/` (`""` when the markers sit at the top).
    pub root: String,
    pub matched: FolderMatch,
    /// The metadata file could not be read within budget (solid 7z): the
    /// system comes from the structure alone.
    pub structure_only: bool,
}

/// What [`archive_folder_game`] found.
#[derive(Clone, Debug, PartialEq)]
pub enum ArchiveGameOutcome {
    /// No marker: not a folder game (today's behaviour).
    None,
    One(Box<ArchiveFolderGame>),
    /// Markers under this many roots: review.
    Many(usize),
    /// Two probes named different systems for the one root: review.
    Conflict(DirConflict),
    /// Two entries differ only in case: extracting them collides on
    /// macOS and Windows, so review.
    CaseCollision,
    /// An `sce_sys/param.sfo` root whose SFO lay beyond the budget: PS4
    /// and Vita share the structure, so review with both (D22).
    Ps4OrVita {
        root: String,
    },
}

/// The folder game an archive's inspection found, memoised with the
/// facts (spec D20–D23).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ArchiveGame {
    /// One game at `root` (`/`-separated, `""` at the top); `probe` is
    /// the probe file relative to the root; the console id rides on the
    /// payload named `<root>/<probe>`.
    One {
        root: String,
        system: String,
        probe: String,
        #[serde(default)]
        structure_only: bool,
        /// A Vita game holding [`VITA_STEROID`].
        #[serde(default)]
        steroid: bool,
    },
    Many {
        count: usize,
    },
    Conflict {
        systems: Vec<String>,
    },
    CaseCollision,
    Ps4OrVita {
        root: String,
    },
}

impl ArchiveGame {
    /// The root of a one-game archive (also when PS4 and Vita could not
    /// be told apart).
    pub fn root(&self) -> Option<&str> {
        match self {
            ArchiveGame::One { root, .. } | ArchiveGame::Ps4OrVita { root } => Some(root),
            _ => None,
        }
    }
}

/// The marker that made a root a candidate (spec §8.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Marker {
    /// `PS3_GAME/PARAM.SFO` and `PS3_GAME/USRDIR/EBOOT.BIN`.
    Ps3Disc,
    /// `<ID>/PARAM.SFO` and `<ID>/USRDIR/EBOOT.BIN`.
    Ps3Hdd,
    /// `sce_sys/param.sfo`.
    SceSfo,
    /// `sce_sys/param.json`.
    SceJson,
    /// `code/*.rpx` with `content/` and `meta/` siblings.
    Loadiine,
    /// `title.tmd` with `.app` siblings.
    Nus,
    Xbe,
    Xex,
}

impl Marker {
    /// The system the structure alone names, and the probe file relative
    /// to the root.
    fn structure(self) -> Option<(&'static str, &'static str)> {
        match self {
            Marker::Ps3Disc => Some(("ps3", "PS3_GAME/PARAM.SFO")),
            Marker::Ps3Hdd => Some(("ps3", "PARAM.SFO")),
            Marker::SceSfo => None,
            Marker::SceJson => Some(("ps5", "sce_sys/param.json")),
            Marker::Loadiine => Some(("wiiu", "code")),
            Marker::Nus => Some(("wiiu", "title.tmd")),
            Marker::Xbe => Some(("xbox", "default.xbe")),
            Marker::Xex => Some(("xbox360", "default.xex")),
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Marker::Ps3Disc => "PS3_GAME structure",
            Marker::Ps3Hdd => "PS3 HDD structure",
            Marker::SceSfo | Marker::SceJson => "sce_sys structure",
            Marker::Loadiine => "loadiine structure",
            Marker::Nus => "NUS structure",
            Marker::Xbe => "default.xbe structure",
            Marker::Xex => "default.xex structure",
        }
    }
}

/// A PS3 HDD/PSN title folder name: four letters and five digits.
fn is_ps3_id(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() == 9
        && b[..4].iter().all(u8::is_ascii_alphabetic)
        && b[4..].iter().all(u8::is_ascii_digit)
}

/// The usable file entries (safe names only) by lower-case name, and every
/// directory (named by an entry or implied by a path) by lower-case path.
struct Names<'e> {
    /// Lower-case path → (entry, its stored components).
    files: HashMap<String, &'e Entry>,
    dirs: HashSet<String>,
    /// Lower-case directory path → files directly in it (lower-case
    /// names).
    in_dir: HashMap<String, Vec<String>>,
}

impl<'e> Names<'e> {
    fn of(files: &'e [Entry], dirs: &[String]) -> Names<'e> {
        let mut n = Names {
            files: HashMap::new(),
            dirs: HashSet::new(),
            in_dir: HashMap::new(),
        };
        for e in files.iter().filter(|e| e.unsafe_name.is_none()) {
            let lower = e.name.as_str().to_lowercase();
            let (dir, file) = lower.rsplit_once('/').unwrap_or(("", lower.as_str()));
            n.in_dir
                .entry(dir.to_string())
                .or_default()
                .push(file.to_string());
            n.add_dirs(dir);
            n.files.entry(lower.clone()).or_insert(e);
        }
        for d in dirs {
            n.add_dirs(&d.to_lowercase());
        }
        n
    }

    fn add_dirs(&mut self, dir: &str) {
        let mut at = dir;
        while !at.is_empty() && self.dirs.insert(at.to_string()) {
            at = at.rsplit_once('/').map_or("", |(parent, _)| parent);
        }
    }
}

/// `prefix/name` for a lower-case prefix that may be empty.
fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

/// The candidate roots (lower-case path → markers) of a listing, by a
/// name pass only (spec D21). Roots deeper than [`MAX_ROOT_DEPTH`] are
/// ignored.
fn roots(names: &Names) -> BTreeMap<String, Vec<Marker>> {
    let mut out: BTreeMap<String, Vec<Marker>> = BTreeMap::new();
    let mut add = |root: &str, marker: Marker| {
        let depth = if root.is_empty() {
            0
        } else {
            root.split('/').count()
        };
        if depth <= MAX_ROOT_DEPTH {
            let markers = out.entry(root.to_string()).or_default();
            if !markers.contains(&marker) {
                markers.push(marker);
            }
        }
    };
    for path in names.files.keys() {
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
        let (parent, own) = dir.rsplit_once('/').unwrap_or(("", dir));
        match file {
            "param.sfo" if own == "ps3_game" => {
                if names.files.contains_key(&join(dir, "usrdir/eboot.bin")) {
                    add(parent, Marker::Ps3Disc);
                }
            }
            "param.sfo" if own == "sce_sys" => add(parent, Marker::SceSfo),
            "param.json" if own == "sce_sys" => add(parent, Marker::SceJson),
            "param.sfo" if !dir.is_empty() && is_ps3_id(own) => {
                if names.files.contains_key(&join(dir, "usrdir/eboot.bin")) {
                    add(dir, Marker::Ps3Hdd);
                }
            }
            "title.tmd" => {
                let apps = names
                    .in_dir
                    .get(dir)
                    .is_some_and(|files| files.iter().any(|f| f.len() > 4 && f.ends_with(".app")));
                if apps {
                    add(dir, Marker::Nus);
                }
            }
            "default.xbe" => add(dir, Marker::Xbe),
            "default.xex" => add(dir, Marker::Xex),
            f if own == "code"
                && f.len() > 4
                && f.ends_with(".rpx")
                && names.dirs.contains(&join(parent, "content"))
                && names.dirs.contains(&join(parent, "meta")) =>
            {
                add(parent, Marker::Loadiine);
            }
            _ => {}
        }
    }
    // `names.files` hashes its keys: a fixed order keeps a structure-only
    // verdict the same from one plan to the next.
    for markers in out.values_mut() {
        markers.sort();
    }
    out
}

/// Whether two names of the listing (files or folders) differ only in
/// case (spec D20).
fn case_collision(files: &[Entry], dirs: &[String]) -> bool {
    let mut seen: HashMap<String, String> = HashMap::new();
    let names = files
        .iter()
        .filter(|e| e.unsafe_name.is_none())
        .map(|e| e.name.as_str())
        .chain(dirs.iter().map(String::as_str));
    for name in names {
        let mut at = 0;
        loop {
            let end = name[at..].find('/').map_or(name.len(), |i| at + i);
            let path = &name[..end];
            let lower = path.to_lowercase();
            match seen.get(&lower) {
                Some(stored) if stored != path => return true,
                Some(_) => {}
                None => {
                    seen.insert(lower, path.to_string());
                }
            }
            if end == name.len() {
                break;
            }
            at = end + 1;
        }
    }
    false
}

/// The stored spelling of the lower-case root `root` (its components as
/// the first entry below it spells them).
fn stored_root(files: &[Entry], root: &str) -> String {
    if root.is_empty() {
        return String::new();
    }
    let depth = root.split('/').count();
    files
        .iter()
        .filter(|e| e.unsafe_name.is_none())
        .find_map(|e| {
            let name = e.name.as_str();
            let parts: Vec<&str> = name.split('/').take(depth).collect();
            (parts.len() == depth && parts.join("/").to_lowercase() == root)
                .then(|| parts.join("/"))
        })
        .unwrap_or_else(|| root.to_string())
}

/// The folder game of an archive whose `files` (file entries, archive
/// order) and `dirs` (directory entry names) the inspection listed
/// (spec D20, D21, D22). `stem` names a root at the archive's top.
pub fn archive_folder_game(
    reader: &mut dyn ArchiveReader,
    files: &[Entry],
    dirs: &[String],
    stem: &str,
    budget: &mut Budget,
) -> ArchiveGameOutcome {
    let names = Names::of(files, dirs);
    let found = roots(&names);
    if found.is_empty() {
        return ArchiveGameOutcome::None;
    }
    if case_collision(files, dirs) {
        return ArchiveGameOutcome::CaseCollision;
    }
    if found.len() > 1 {
        return ArchiveGameOutcome::Many(found.len());
    }
    let (root, markers) = found.into_iter().next().expect("one root");
    let stored = stored_root(files, &root);
    let fs = ArchiveFs::new(reader, files, dirs, budget);
    let children = fs.list(Path::new(&stored)).unwrap_or_default();
    let name = match stored.rsplit_once('/') {
        Some((_, last)) => last.to_string(),
        None if stored.is_empty() => stem.to_string(),
        None => stored.clone(),
    };
    let parent = match stored.rsplit_once('/') {
        Some((p, _)) => p.rsplit_once('/').map_or(p, |(_, last)| last).to_string(),
        None => String::new(),
    };
    let listing = DirListing {
        fs: &fs,
        dir: Path::new(&stored),
        name: &name,
        parent: &parent,
        names: &children,
    };
    match identify_dir(&listing) {
        Ok(Some(mut matched)) => {
            if matched.leave.is_some() {
                return ArchiveGameOutcome::None;
            }
            // A byte range is relative to a file on disk: an archive's
            // entry is only read by name.
            matched
                .id
                .art
                .retain(|a| matches!(a, ArtLocator::ZipEntry { .. }));
            ArchiveGameOutcome::One(Box::new(ArchiveFolderGame {
                root: stored,
                matched,
                structure_only: false,
            }))
        }
        Err(conflict) => ArchiveGameOutcome::Conflict(conflict),
        Ok(None) if fs.spent.get() => {
            if markers.contains(&Marker::SceSfo) {
                return ArchiveGameOutcome::Ps4OrVita { root: stored };
            }
            let Some((system, probe)) = markers.iter().find_map(|m| m.structure()) else {
                return ArchiveGameOutcome::None;
            };
            let detail = markers[0].detail();
            ArchiveGameOutcome::One(Box::new(ArchiveFolderGame {
                root: stored,
                matched: FolderMatch {
                    id: ConsoleId::magic(system, detail),
                    probe: probe.into(),
                    leave: None,
                },
                structure_only: true,
            }))
        }
        Ok(None) => ArchiveGameOutcome::None,
    }
}

/// An archive's listing as a file system (spec D20): directories implied
/// by the file names, names compared ignoring case as the probes do,
/// reads through the archive layer under the inspection's budget. No
/// entry has the FAT archive bit, so `nx::probe_split` never matches.
pub struct ArchiveFs<'a> {
    reader: RefCell<&'a mut dyn ArchiveReader>,
    budget: RefCell<&'a mut Budget>,
    files: &'a [Entry],
    /// Lower-case path → position in `files`.
    by_name: HashMap<String, usize>,
    /// Lower-case directory path → children as stored (each once).
    children: HashMap<String, Vec<String>>,
    /// A read was refused by the budget.
    spent: Cell<bool>,
}

impl<'a> ArchiveFs<'a> {
    pub fn new(
        reader: &'a mut dyn ArchiveReader,
        files: &'a [Entry],
        dirs: &[String],
        budget: &'a mut Budget,
    ) -> ArchiveFs<'a> {
        let mut by_name = HashMap::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut child_keys: HashSet<(String, String)> = HashSet::new();
        let mut add_path = |name: &str| {
            let mut parent = String::new();
            for part in name.split('/') {
                let lower = part.to_lowercase();
                if child_keys.insert((parent.clone(), lower.clone())) {
                    children
                        .entry(parent.clone())
                        .or_default()
                        .push(part.to_string());
                }
                parent = join(&parent, &lower);
            }
        };
        for (at, e) in files.iter().enumerate() {
            if e.unsafe_name.is_some() {
                continue;
            }
            add_path(e.name.as_str());
            by_name.entry(e.name.as_str().to_lowercase()).or_insert(at);
        }
        for d in dirs {
            add_path(d);
        }
        children.entry(String::new()).or_default();
        ArchiveFs {
            reader: RefCell::new(reader),
            budget: RefCell::new(budget),
            files,
            by_name,
            children,
            spent: Cell::new(false),
        }
    }

    /// Whether a read was refused by the budget.
    pub fn budget_spent(&self) -> bool {
        self.spent.get()
    }

    fn key(rel: &Path) -> String {
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>()
            .join("/")
    }

    fn file(&self, rel: &Path) -> Option<(usize, &Entry)> {
        let at = *self.by_name.get(&Self::key(rel))?;
        Some((at, &self.files[at]))
    }

    /// At most `cap` decoded bytes of the file at `rel`.
    fn read_capped(&self, rel: &Path, cap: u64) -> Option<Vec<u8>> {
        let (at, e) = self.file(rel)?;
        if e.encrypted {
            return None;
        }
        let mut budget = self.budget.borrow_mut();
        let want = cap.min(e.size);
        if !super::reachable(self.files, at, want, &budget) {
            self.spent.set(true);
            return None;
        }
        let mut reader = self.reader.borrow_mut();
        let read = super::read_entry(&mut **reader, e.index, want, &mut budget);
        match read {
            Ok(bytes) => Some(bytes),
            Err(err) => {
                self.note(&err);
                None
            }
        }
    }

    fn note(&self, err: &ArchiveError) {
        if matches!(err, ArchiveError::BudgetSpent { .. }) {
            self.spent.set(true);
        }
    }
}

impl ListingFs for ArchiveFs<'_> {
    fn list(&self, rel: &Path) -> Option<Vec<String>> {
        let names = self.children.get(&Self::key(rel))?;
        Some(names.iter().take(MAX_CHILD_ENTRIES).cloned().collect())
    }

    fn meta(&self, rel: &Path) -> Option<EntryMeta> {
        if let Some((_, e)) = self.file(rel) {
            return Some(EntryMeta {
                file: true,
                len: e.size,
                ..EntryMeta::default()
            });
        }
        self.children.get(&Self::key(rel)).map(|_| EntryMeta {
            dir: true,
            ..EntryMeta::default()
        })
    }

    fn read(&self, rel: &Path, cap: u64) -> Option<Vec<u8>> {
        self.read_capped(rel, cap)
    }

    fn open(&self, rel: &Path) -> Option<(Box<dyn ReadAt + '_>, u64)> {
        let (_, e) = self.file(rel)?;
        let len = e.size;
        let left = self.budget.borrow().remaining().unwrap_or(u64::MAX);
        let bytes = self.read_capped(rel, OPEN_CAP.min(left))?;
        Some((Box::new(HeadAt(bytes)), len))
    }

    fn art(&self, rel: &Path, format: ArtFormat) -> ArtLocator {
        let name = match self.file(rel) {
            Some((_, e)) => e.name.as_str().to_string(),
            None => rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/"),
        };
        ArtLocator::ZipEntry { name, format }
    }
}

/// The decoded head of an entry: reads past it fail, so a probe that
/// needs more sees an unreadable file, never zeros.
struct HeadAt(Vec<u8>);

impl ReadAt for HeadAt {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        if start > self.0.len() || (start == self.0.len() && !buf.is_empty()) {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "past the decoded head",
            ));
        }
        let n = buf.len().min(self.0.len() - start);
        buf[..n].copy_from_slice(&self.0[start..start + n]);
        Ok(n)
    }
}

/// The tree digest of an archive's folder game (adjustments spec D19,
/// D20): [`folder_tree_digest`](crate::dedupe::keys::folder_tree_digest)
/// of the files below its root, from the listing alone, so a zipped game
/// already extracted in the library matches the folder there without
/// extracting. `None` for an archive with no single folder game, an
/// incomplete listing, an unsafe name, or nothing below the root. OS
/// litter is left out, as the tree on disk leaves it.
pub fn archive_tree_digest(insp: &super::Inspection) -> Option<String> {
    let Some(ArchiveGame::One { root, .. }) = &insp.folder_game else {
        return None;
    };
    if !insp.listing_complete || insp.entries.iter().any(|e| e.unsafe_why.is_some()) {
        return None;
    }
    let root = root.replace('\\', "/").trim_matches('/').to_lowercase();
    let names: Vec<(String, u64)> = insp
        .entries
        .iter()
        .map(|e| (e.name.replace('\\', "/"), e.size))
        .collect();
    // A name that ends in `/`, or that other names sit below, is a folder.
    let folders: HashSet<String> = names
        .iter()
        .flat_map(|(n, _)| {
            let parts: Vec<&str> = n.trim_matches('/').split('/').collect();
            (1..parts.len()).map(move |k| parts[..k].join("/").to_lowercase())
        })
        .collect();
    let mut listed: Vec<(String, u64)> = Vec::new();
    for (name, size) in names {
        if name.ends_with('/') {
            continue;
        }
        // The digest folds case anyway: work on the lower-case name.
        let lower = name.trim_matches('/').to_lowercase();
        if folders.contains(&lower) {
            continue;
        }
        let rel = if root.is_empty() {
            lower.as_str()
        } else {
            match lower.strip_prefix(&root).and_then(|r| r.strip_prefix('/')) {
                Some(rel) => rel,
                None => continue,
            }
        };
        let base = rel.rsplit('/').next().unwrap_or(rel);
        if crate::scan::classify::is_appledouble(base)
            || crate::scan::classify::is_junk_file(base)
            || crate::scan::classify::is_zone_identifier(base)
        {
            continue;
        }
        listed.push((rel.to_string(), size));
    }
    if listed.is_empty() {
        return None;
    }
    Some(crate::dedupe::keys::folder_tree_digest(&mut listed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::{EntryKind, Method, RelPath};

    fn entry(index: usize, name: &str) -> Entry {
        Entry {
            index,
            name: RelPath::parse(name).unwrap(),
            raw_name: name.as_bytes().to_vec(),
            kind: EntryKind::File,
            size: 1,
            packed: None,
            crc32: None,
            encrypted: false,
            method: Method::Stored,
            solid_block: None,
            mtime: None,
            unsafe_name: None,
        }
    }

    /// The markers of one root come in a fixed order whatever order the
    /// listing hashes them in, so a structure-only verdict never changes
    /// from one plan to the next.
    #[test]
    fn markers_of_a_root_are_in_a_fixed_order() {
        for names in [
            ["Game/default.xex", "Game/default.xbe"],
            ["Game/default.xbe", "Game/default.xex"],
        ] {
            let files: Vec<Entry> = names.iter().enumerate().map(|(i, n)| entry(i, n)).collect();
            for _ in 0..16 {
                let found = roots(&Names::of(&files, &[]));
                assert_eq!(found.get("game"), Some(&vec![Marker::Xbe, Marker::Xex]));
            }
        }
    }
}
