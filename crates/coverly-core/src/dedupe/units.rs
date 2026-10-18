//! Units: what a duplicate is made of (duplicates spec §5.1). A unit is one
//! playable thing (a ROM, a zip, an arcade set, a disc set) and every file
//! that belongs to it.
//!
//! Units are built from a library scan ([`units_from_scan`]) or from the
//! organise facts of a pile ([`units_from_facts`]). Everything that must
//! never take part in a group is set aside here, before any key is read:
//! out-of-scope folders, the quarantine, BIOS files wherever they are,
//! files that are not ROMs, links and archives holding several games.
//! No name ever joins two units; a name may only exclude one.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::identify::dat::DatGame;
use crate::identify::sniff::{sniff, ContainerMagic, NotARom, SliceAt};
use crate::organize::evidence::Facts;
use crate::organize::inspect::{ArchiveIssue, Container};
use crate::profiles::Profile;
use crate::scan::classify::{extension, is_bios_name, is_partial_download, is_skipped_entry};
use crate::scan::{LibraryScan, RomEntry, ScanOptions};
use crate::systems::{System, SystemDb};

use super::source::{EntryListing, ZipListing};
use super::UnitRef;

/// Folder names that are out of scope at any depth (spec §5.1, hard rule
/// 10), compared case-insensitively. The profile's art folders are added
/// by [`UnitOptions::for_profile`].
pub const OUT_OF_SCOPE_DIRS: [&str; 5] = ["ports", "bios", "apps", "saves", "states"];

/// Soft-patch extensions: a same-stem patch changes what the ROM plays.
const PATCH_EXTS: [&str; 5] = ["ips", "bps", "ups", "xdelta", "ppf"];

/// Bytes read to check that a file is what its extension says.
const MAGIC_PROBE: usize = 512;

/// The shape of a unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum UnitKind {
    /// One loose ROM file (also a 7z or rar, compared as a whole file
    /// until the archive reader lands).
    Single,
    /// A zip with one ROM inside.
    Zip,
    /// An arcade set (a zip named after the set).
    Arcade,
    /// A cue/gdi/ccd sheet and its tracks.
    DiscSet,
    /// A CHD image.
    Chd,
    /// A single-file disc image: pbp, cso, rvz, iso.
    Image,
    /// An m3u playlist over several discs.
    MultiDisc,
}

/// One playable thing and every file that belongs to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    /// Index in [`UnitSet::units`].
    pub id: usize,
    /// System id (from the system folder or the organise identification).
    pub system: String,
    /// The file that stands for the unit (the m3u, the cue, the zip, the
    /// ROM).
    pub primary: PathBuf,
    /// Every file of the unit, `primary` first. Soft patches are not here.
    pub files: Vec<PathBuf>,
    pub kind: UnitKind,
    /// Bytes on disk of every file in `files`.
    pub size: u64,
    /// Size of the game data when known without decoding (a loose ROM, the
    /// one entry of a zip, the entries of an arcade set); `None` for sets
    /// and compressed images. Headers are not taken off here.
    pub payload_size: Option<u64>,
    /// Modification time of `primary`, nanoseconds since the Unix epoch.
    pub mtime_ns: i64,
    /// File id (inode) of `primary`, where the platform has one.
    pub file_id: Option<u64>,
    /// Same-stem soft patches (`.ips`, `.bps`, `.ups`, `.xdelta`, `.ppf`)
    /// beside `primary`.
    pub patches: Vec<PathBuf>,
    /// Folder of `primary` relative to its system folder.
    pub rel_dir: PathBuf,
    /// File name of `primary` without its extension.
    pub stem: String,
    /// Lowercase extension of `primary`, without the dot.
    pub ext: String,
    /// Already in a system folder (false for an organise pile item).
    pub in_library: bool,
}

impl Unit {
    /// The part of the unit a plan carries.
    pub fn to_ref(&self) -> UnitRef {
        UnitRef {
            primary: self.primary.clone(),
            files: self.files.clone(),
            kind: self.kind,
            mtime_ns: self.mtime_ns,
            in_library: self.in_library,
        }
    }
}

/// Why a file never takes part in a group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Exclusion {
    /// Under `PORTS/`, `BIOS/`, `APPS/`, an art or a save folder.
    Folder,
    /// A BIOS or firmware file: each core asks for its own name.
    Bios,
    /// Empty, or its bytes contradict its extension (a web page as `.zip`).
    NotARom,
    /// A symlink, or a unit with a file a symlink points to: moving the
    /// target would break the link.
    Symlink,
    /// Another path of a file already taken (a hard link): moving it frees
    /// nothing.
    HardLinkTwin,
    /// A download still being written.
    Partial,
    /// Could not be read.
    Unreadable,
    /// An archive holding several games.
    MultiGameArchive,
    /// Inside the quarantine folder.
    Quarantine,
}

/// Result of [`units_from_scan`] and [`units_from_facts`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitSet {
    /// In scan order; `units[i].id == i`.
    pub units: Vec<Unit>,
    /// What was set aside and why, sorted by path.
    pub excluded: Vec<(PathBuf, Exclusion)>,
}

/// What [`units_from_scan`] leaves out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOptions {
    /// Folder names out of scope at any depth, compared case-insensitively.
    pub out_of_scope_dirs: Vec<String>,
    /// The quarantine folder: nothing below it is a unit.
    pub quarantine: Option<PathBuf>,
    /// Lowercase DAT names flagged as BIOS, per system id: game names (an
    /// arcade set is its zip stem) and ROM file names.
    pub dat_bios: HashMap<String, HashSet<String>>,
}

impl Default for UnitOptions {
    fn default() -> Self {
        UnitOptions {
            out_of_scope_dirs: OUT_OF_SCOPE_DIRS.iter().map(|d| d.to_string()).collect(),
            quarantine: None,
            dat_bios: HashMap::new(),
        }
    }
}

impl UnitOptions {
    /// The defaults plus the art folders of `profile`.
    pub fn for_profile(profile: &Profile) -> UnitOptions {
        let mut opts = UnitOptions::default();
        opts.out_of_scope_dirs
            .extend(profile.art_dir_names.iter().cloned());
        opts
    }

    /// Records `game` as a BIOS of `system` when its DAT says so (`isbios`
    /// or a `[BIOS]` name); other games are ignored.
    pub fn add_dat_bios(&mut self, system: &str, game: &DatGame) {
        if !game.is_bios {
            return;
        }
        let names = self.dat_bios.entry(system.to_string()).or_default();
        names.insert(game.name.to_lowercase());
        for rom in &game.roms {
            names.insert(rom.rom_name.to_lowercase());
        }
    }

    fn out_of_scope(&self, name: &str) -> bool {
        self.out_of_scope_dirs
            .iter()
            .any(|d| d.eq_ignore_ascii_case(name))
    }

    fn in_quarantine(&self, path: &Path) -> bool {
        self.quarantine
            .as_deref()
            .is_some_and(|q| starts_with_ignoring_case(path, q))
    }

    fn dat_bios(&self, system: &str, name: &str, stem: &str) -> bool {
        self.dat_bios.get(system).is_some_and(|names| {
            names.contains(&name.to_lowercase()) || names.contains(&stem.to_lowercase())
        })
    }
}

/// A unit before merging, with the size of each of its files.
struct Draft {
    unit: Unit,
    sizes: Vec<u64>,
    dev_ino: Option<(u64, u64)>,
}

/// Units of a library scan (spec §5.1).
///
/// Every ROM entry of a known system folder becomes a unit unless it is
/// excluded; entries that share a file (two cue sheets over one bin, an
/// m3u and a loose cue of the same disc, a ccd beside its img) merge into
/// one unit. Out-of-scope top-level folders are reported once as
/// [`Exclusion::Folder`]. Reads only file metadata, the first bytes of
/// each file and zip central directories.
pub fn units_from_scan(scan: &LibraryScan, db: &SystemDb, opts: &UnitOptions) -> UnitSet {
    let mut excluded = Vec::new();
    let mut drafts = Vec::new();
    let mut link_targets = HashSet::new();
    for sys in &scan.systems {
        if opts.out_of_scope(&sys.folder_name) {
            excluded.push((sys.dir.clone(), Exclusion::Folder));
            continue;
        }
        let Some(system) = sys.system_id.as_deref().and_then(|id| db.get(id)) else {
            continue;
        };
        let mut dir_cache = DirCache::default();
        let mut found = Vec::new();
        for rom in &sys.roms {
            match draft_from_rom(rom, system, db, opts, &mut dir_cache) {
                Ok(draft) => found.push(draft),
                Err(Some(why)) => {
                    if why == Exclusion::Symlink {
                        let files = std::iter::once(&rom.path).chain(&rom.companions);
                        link_targets.extend(files.filter_map(|f| symlink_target(f)));
                    }
                    excluded.push((rom.path.clone(), why));
                }
                Err(None) => {}
            }
        }
        attach_ccd_tracks(&mut found, &mut dir_cache);
        drafts.extend(merge_overlaps(found));
    }
    finish(drafts, excluded, &link_targets)
}

/// Units of an organise pile: `facts[i]` belongs to the system
/// `system_of(i)`; items with no system are not units and not reported.
/// The organise inspection says what each file holds, so nothing but
/// metadata and folder listings is read here.
pub fn units_from_facts(
    facts: &[(PathBuf, Facts)],
    system_of: &dyn Fn(usize) -> Option<String>,
) -> UnitSet {
    let mut excluded = Vec::new();
    let mut drafts = Vec::new();
    let mut link_targets = HashSet::new();
    let mut dir_cache = DirCache::default();
    for (i, (path, fact)) in facts.iter().enumerate() {
        let Some(system) = system_of(i) else {
            continue;
        };
        match draft_from_facts(path, fact, &system, &mut dir_cache) {
            Ok(draft) => drafts.push(draft),
            Err(Some(why)) => {
                if why == Exclusion::Symlink {
                    link_targets.extend(symlink_target(path));
                }
                excluded.push((path.clone(), why));
            }
            Err(None) => {}
        }
    }
    finish(drafts, excluded, &link_targets)
}

/// Drops hard-link twins and units a symlink points into, numbers the
/// units and sorts the exclusions. `link_targets` are canonical paths.
fn finish(
    drafts: Vec<Draft>,
    mut excluded: Vec<(PathBuf, Exclusion)>,
    link_targets: &HashSet<PathBuf>,
) -> UnitSet {
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let mut units = Vec::new();
    for draft in drafts {
        if let Some(id) = draft.dev_ino {
            if !seen.insert(id) {
                excluded.push((draft.unit.primary, Exclusion::HardLinkTwin));
                continue;
            }
        }
        if !link_targets.is_empty()
            && draft
                .unit
                .files
                .iter()
                .any(|f| fs::canonicalize(f).is_ok_and(|c| link_targets.contains(&c)))
        {
            excluded.push((draft.unit.primary, Exclusion::Symlink));
            continue;
        }
        let mut unit = draft.unit;
        unit.id = units.len();
        units.push(unit);
    }
    excluded.sort();
    excluded.dedup();
    UnitSet { units, excluded }
}

/// The checks every candidate file passes, whatever its source. `Err(None)`
/// skips silently (AppleDouble, hidden, OS metadata).
fn common_checks(path: &Path, opts: Option<&UnitOptions>) -> Result<(), Option<Exclusion>> {
    let name = file_name(path);
    if is_partial_download(&name) {
        return Err(Some(Exclusion::Partial));
    }
    if is_skipped_entry(&name, &ScanOptions::default()) {
        return Err(None);
    }
    if opts.is_some_and(|o| o.in_quarantine(path)) {
        return Err(Some(Exclusion::Quarantine));
    }
    Ok(())
}

/// Whether `name` (with `ext`, lowercase) is a BIOS by name.
fn is_bios_file(name: &str, ext: &str) -> bool {
    is_bios_name(name) || ext == "bios" || name.to_lowercase().contains("[bios]")
}

fn draft_from_rom(
    rom: &RomEntry,
    system: &System,
    db: &SystemDb,
    opts: &UnitOptions,
    dirs: &mut DirCache,
) -> Result<Draft, Option<Exclusion>> {
    let primary = &rom.path;
    common_checks(primary, Some(opts))?;
    if rom
        .rel_dir
        .components()
        .any(|c| matches!(c, Component::Normal(n) if opts.out_of_scope(&n.to_string_lossy())))
    {
        return Err(Some(Exclusion::Folder));
    }
    let files: Vec<PathBuf> = std::iter::once(primary.clone())
        .chain(rom.companions.iter().cloned())
        .collect();
    if files.iter().any(|f| opts.in_quarantine(f)) {
        return Err(Some(Exclusion::Quarantine));
    }
    let name = file_name(primary);
    let ext = rom.ext.clone();
    if is_bios_file(&name, &ext) || opts.dat_bios(&system.id, &name, &rom.stem) {
        return Err(Some(Exclusion::Bios));
    }
    let (meta, sizes) = read_metas(&files)?;
    if meta.len() == 0 {
        return Err(Some(Exclusion::NotARom));
    }
    let (kind, payload_size) = classify(primary, &ext, meta.len(), system, db)?;
    Ok(Draft {
        unit: Unit {
            id: 0,
            system: system.id.clone(),
            primary: primary.clone(),
            size: sizes.iter().sum(),
            files,
            kind,
            payload_size,
            mtime_ns: mtime_ns(&meta),
            file_id: file_id(&meta),
            patches: patches_of(primary, dirs),
            rel_dir: rom.rel_dir.clone(),
            stem: rom.stem.clone(),
            ext,
            in_library: true,
        },
        sizes,
        dev_ino: dev_ino(&meta),
    })
}

fn draft_from_facts(
    path: &Path,
    facts: &Facts,
    system: &str,
    dirs: &mut DirCache,
) -> Result<Draft, Option<Exclusion>> {
    common_checks(path, None)?;
    let name = file_name(path);
    let ext = extension(path);
    if is_bios_file(&name, &ext) {
        return Err(Some(Exclusion::Bios));
    }
    let (meta, sizes) = read_metas(&[path.to_path_buf()])?;
    if meta.len() == 0 {
        return Err(Some(Exclusion::NotARom));
    }
    let inspection = &facts.inspection;
    match &inspection.issue {
        Some(ArchiveIssue::NotARom { .. }) => return Err(Some(Exclusion::NotARom)),
        Some(ArchiveIssue::TooManyRoms { .. }) => return Err(Some(Exclusion::MultiGameArchive)),
        Some(_) => return Err(Some(Exclusion::Unreadable)),
        None => {}
    }
    let (kind, payload_size) = match inspection.container {
        Container::Zip => match inspection.payloads.as_slice() {
            [one] => (UnitKind::Zip, Some(one.size)),
            [] if !inspection.entries.is_empty() => (
                UnitKind::Arcade,
                Some(inspection.entries.iter().map(|e| e.size).sum()),
            ),
            [] => return Err(Some(Exclusion::NotARom)),
            _ => return Err(Some(Exclusion::MultiGameArchive)),
        },
        Container::Chd => (UnitKind::Chd, None),
        Container::SevenZip | Container::Rar => (UnitKind::Single, None),
        Container::Raw => raw_kind(&ext, meta.len()),
    };
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Draft {
        unit: Unit {
            id: 0,
            system: system.to_string(),
            primary: path.to_path_buf(),
            files: vec![path.to_path_buf()],
            kind,
            size: sizes.iter().sum(),
            payload_size,
            mtime_ns: mtime_ns(&meta),
            file_id: file_id(&meta),
            patches: patches_of(path, dirs),
            rel_dir: PathBuf::new(),
            stem,
            ext,
            in_library: false,
        },
        sizes,
        dev_ino: dev_ino(&meta),
    })
}

/// Metadata of the primary (`files[0]`) and the size of every file. Any
/// symlink excludes the unit; any unreadable file makes it unreadable.
fn read_metas(files: &[PathBuf]) -> Result<(Metadata, Vec<u64>), Option<Exclusion>> {
    let mut sizes = Vec::with_capacity(files.len());
    let mut primary = None;
    for file in files {
        let link = fs::symlink_metadata(file).map_err(|_| Some(Exclusion::Unreadable))?;
        if link.file_type().is_symlink() {
            return Err(Some(Exclusion::Symlink));
        }
        if !link.is_file() {
            return Err(Some(Exclusion::Unreadable));
        }
        sizes.push(link.len());
        if primary.is_none() {
            primary = Some(link);
        }
    }
    let primary = primary.ok_or(Some(Exclusion::Unreadable))?;
    Ok((primary, sizes))
}

/// Kind and payload size of a scanned primary file, after checking that
/// its first bytes agree with its extension.
fn classify(
    path: &Path,
    ext: &str,
    len: u64,
    system: &System,
    db: &SystemDb,
) -> Result<(UnitKind, Option<u64>), Option<Exclusion>> {
    match ext {
        "m3u" => return Ok((UnitKind::MultiDisc, None)),
        "cue" | "gdi" | "ccd" => return Ok((UnitKind::DiscSet, None)),
        _ => {}
    }
    let probe = probe_magic(path, ext)?;
    let want = match ext {
        "zip" => Some(ContainerMagic::Zip),
        "7z" => Some(ContainerMagic::SevenZip),
        "chd" => Some(ContainerMagic::Chd),
        "rar" => Some(ContainerMagic::Rar),
        _ => None,
    };
    match want {
        Some(magic) => {
            // A container must open with its own magic: text or anything
            // else saved under its name is not a ROM.
            if probe.not_a_rom.is_some() || probe.container != Some(magic) {
                return Err(Some(Exclusion::NotARom));
            }
        }
        None => {
            if probe.not_a_rom == Some(NotARom::WebPage) {
                return Err(Some(Exclusion::NotARom));
            }
        }
    }
    match ext {
        "zip" => zip_kind(path, system, db),
        "chd" => Ok((UnitKind::Chd, None)),
        "7z" | "rar" if system.arcade => Ok((UnitKind::Arcade, None)),
        "7z" | "rar" => Ok((UnitKind::Single, None)),
        _ => Ok(raw_kind(ext, len)),
    }
}

/// Kind of a raw (not archived) file by its extension.
fn raw_kind(ext: &str, len: u64) -> (UnitKind, Option<u64>) {
    match ext {
        "iso" | "img" => (UnitKind::Image, Some(len)),
        "pbp" | "cso" | "rvz" => (UnitKind::Image, None),
        "chd" => (UnitKind::Chd, None),
        _ => (UnitKind::Single, Some(len)),
    }
}

/// An arcade set, a one-ROM zip, or several games in one zip, from its
/// central directory only.
fn zip_kind(
    path: &Path,
    system: &System,
    db: &SystemDb,
) -> Result<(UnitKind, Option<u64>), Option<Exclusion>> {
    let entries = match ZipListing.list(path) {
        Ok(Some(entries)) => entries,
        Ok(None) | Err(_) => return Err(Some(Exclusion::Unreadable)),
    };
    if entries.is_empty() {
        // An empty zip holds no game; identical ones would group.
        return Err(Some(Exclusion::NotARom));
    }
    if system.arcade {
        return Ok((UnitKind::Arcade, Some(entries.iter().map(|e| e.size).sum())));
    }
    let roms: Vec<u64> = entries
        .iter()
        .filter(|e| {
            let ext = extension(Path::new(&e.name));
            !matches!(ext.as_str(), "zip" | "7z" | "rar") && db.is_rom_like_extension(&ext)
        })
        .map(|e| e.size)
        .collect();
    match roms.as_slice() {
        [one] => Ok((UnitKind::Zip, Some(*one))),
        // Nothing ROM-like by name: still one unit, compared whole.
        [] => Ok((UnitKind::Zip, None)),
        _ => Err(Some(Exclusion::MultiGameArchive)),
    }
}

/// Sniffs the first [`MAGIC_PROBE`] bytes of `path`.
fn probe_magic(path: &Path, ext: &str) -> Result<crate::identify::sniff::Sniff, Option<Exclusion>> {
    let mut head = Vec::with_capacity(MAGIC_PROBE);
    File::open(path)
        .and_then(|f| f.take(MAGIC_PROBE as u64).read_to_end(&mut head))
        .map_err(|_| Some(Exclusion::Unreadable))?;
    Ok(sniff(&mut SliceAt(&head), head.len() as u64, ext))
}

/// Gives each `.ccd` its same-stem `.img` and `.sub` (a CloneCD triple):
/// the img then merges into the ccd's unit.
fn attach_ccd_tracks(drafts: &mut [Draft], dirs: &mut DirCache) {
    for draft in drafts.iter_mut().filter(|d| d.unit.ext == "ccd") {
        let primary = draft.unit.primary.clone();
        for want in ["img", "sub"] {
            if let Some(found) = dirs.sibling(&primary, want) {
                if draft.unit.files.contains(&found) {
                    continue;
                }
                let Ok(meta) = fs::symlink_metadata(&found) else {
                    continue;
                };
                if !meta.is_file() {
                    continue;
                }
                draft.sizes.push(meta.len());
                draft.unit.size += meta.len();
                draft.unit.files.push(found);
            }
        }
    }
}

/// Merges drafts that share a file into one unit, so a shared track never
/// leaves a keeper that needs it (spec §5.1, hard rule 11). The m3u wins
/// the primary, then a sheet, then the first path.
fn merge_overlaps(drafts: Vec<Draft>) -> Vec<Draft> {
    let n = drafts.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut root = i;
        while parent[root] != root {
            root = parent[root];
        }
        let mut i = i;
        while parent[i] != root {
            let next = parent[i];
            parent[i] = root;
            i = next;
        }
        root
    }
    let mut owner: HashMap<&Path, usize> = HashMap::new();
    for (i, draft) in drafts.iter().enumerate() {
        for file in &draft.unit.files {
            if let Some(&j) = owner.get(file.as_path()) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[a.max(b)] = a.min(b);
                }
            } else {
                owner.insert(file, i);
            }
        }
    }
    let roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
    if roots.iter().enumerate().all(|(i, &r)| r == i) {
        return drafts;
    }

    let mut groups: Vec<Vec<Draft>> = Vec::new();
    let mut slot: HashMap<usize, usize> = HashMap::new();
    for (draft, root) in drafts.into_iter().zip(roots) {
        let at = *slot.entry(root).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[at].push(draft);
    }
    groups.into_iter().map(merge_group).collect()
}

fn primary_rank(kind: UnitKind) -> u8 {
    match kind {
        UnitKind::MultiDisc => 0,
        UnitKind::DiscSet => 1,
        _ => 2,
    }
}

fn merge_group(mut group: Vec<Draft>) -> Draft {
    if group.len() == 1 {
        return group.remove(0);
    }
    group.sort_by(|a, b| {
        primary_rank(a.unit.kind)
            .cmp(&primary_rank(b.unit.kind))
            .then_with(|| a.unit.primary.cmp(&b.unit.primary))
    });
    let mut rest = group.into_iter();
    let mut head = rest.next().expect("a merge group is never empty");
    let mut seen: HashSet<PathBuf> = head.unit.files.iter().cloned().collect();
    for other in rest {
        for (file, size) in other.unit.files.into_iter().zip(other.sizes) {
            if seen.insert(file.clone()) {
                head.unit.files.push(file);
                head.sizes.push(size);
            }
        }
        for patch in other.unit.patches {
            if !head.unit.patches.contains(&patch) {
                head.unit.patches.push(patch);
            }
        }
    }
    head.unit.patches.sort();
    head.unit.size = head.sizes.iter().sum();
    head.unit.payload_size = None;
    head
}

/// Directory listings, read once per folder.
#[derive(Default)]
struct DirCache {
    listings: HashMap<PathBuf, Vec<PathBuf>>,
}

impl DirCache {
    fn listing(&mut self, dir: &Path) -> &[PathBuf] {
        self.listings.entry(dir.to_path_buf()).or_insert_with(|| {
            let mut files: Vec<PathBuf> = fs::read_dir(dir)
                .map(|entries| entries.flatten().map(|e| e.path()).collect())
                .unwrap_or_default();
            files.sort();
            files
        })
    }

    /// Files beside `path` with the same stem (any case) and an extension
    /// accepted by `want`, not skipped entries, sorted.
    fn same_stem(&mut self, path: &Path, want: impl Fn(&str) -> bool) -> Vec<PathBuf> {
        let Some(dir) = path.parent() else {
            return Vec::new();
        };
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        self.listing(dir)
            .iter()
            .filter(|p| p.as_path() != path)
            .filter(|p| !is_skipped_entry(&file_name(p), &ScanOptions::default()))
            .filter(|p| {
                p.file_stem()
                    .is_some_and(|s| s.to_string_lossy().to_lowercase() == stem)
                    && want(&extension(p))
            })
            .cloned()
            .collect()
    }

    /// The same-stem sibling of `path` with extension `ext`, if any.
    fn sibling(&mut self, path: &Path, ext: &str) -> Option<PathBuf> {
        self.same_stem(path, |e| e == ext).into_iter().next()
    }
}

fn patches_of(path: &Path, dirs: &mut DirCache) -> Vec<PathBuf> {
    dirs.same_stem(path, |e| PATCH_EXTS.contains(&e))
}

/// The canonical path `path` points to, when it is a symlink.
fn symlink_target(path: &Path) -> Option<PathBuf> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    fs::canonicalize(path).ok()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Whether `path` is `prefix` or below it, comparing components without
/// regard to case (card file systems are case-insensitive).
fn starts_with_ignoring_case(path: &Path, prefix: &Path) -> bool {
    let mut path = path.components();
    prefix.components().all(|want| {
        path.next().is_some_and(|got| {
            got.as_os_str().to_string_lossy().to_lowercase()
                == want.as_os_str().to_string_lossy().to_lowercase()
        })
    })
}

fn mtime_ns(meta: &Metadata) -> i64 {
    let modified = meta.modified().unwrap_or(UNIX_EPOCH);
    match modified.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_nanos()).map_or(i64::MIN, |n| -n),
    }
}

#[cfg(unix)]
fn file_id(meta: &Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.ino())
}

#[cfg(not(unix))]
fn file_id(_meta: &Metadata) -> Option<u64> {
    None
}

/// Device and inode, only for files with more than one link.
#[cfg(unix)]
fn dev_ino(meta: &Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (meta.nlink() > 1).then(|| (meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn dev_ino(_meta: &Metadata) -> Option<(u64, u64)> {
    None
}
