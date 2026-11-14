//! Reading, serializing and listing `Extras/extras.json` (extras spec D2–D5).

use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

use super::{
    type_of, CategoryMeta, CategoryRef, ExtraRecord, ExtrasCategory, ExtrasFile, Origin, OriginHow,
    Provenance, EXTRAS_JSON,
};
use crate::scan::classify::is_skipped_entry;
use crate::scan::ScanOptions;
use crate::time::rfc3339_utc;
use crate::write::{mtime_ns as write_mtime_ns, name_key};

/// A larger `extras.json` is never parsed and counts as malformed (D5).
pub const MAX_INDEX_BYTES: u64 = 8 << 20;

/// Why `extras.json` could not even be read (D5); never followed or parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnreadableKind {
    Symlink,
    Folder,
    TooLarge,
    Io,
}

/// What reading `Extras/extras.json` found (D2, D5). Reading never writes.
#[derive(Clone, Debug, PartialEq)]
pub enum IndexRead {
    /// A well-formed version 1 file.
    Ok(ExtrasFile),
    /// No `extras.json`.
    Absent,
    /// Not JSON, the wrong shape, records without a valid `path`, or not a
    /// readable regular file. `salvaged` holds the records that parse on
    /// their own when the top level is an object with a `files` array;
    /// `bad_records` counts the ones skipped.
    Malformed {
        salvaged: Option<ExtrasFile>,
        bad_records: usize,
        unreadable_kind: Option<UnreadableKind>,
    },
    /// A newer `version`: view only, with what this build understands.
    Newer { version: u32, partial: ExtrasFile },
}

impl IndexRead {
    fn unreadable(kind: UnreadableKind) -> Self {
        Self::Malformed {
            salvaged: None,
            bad_records: 0,
            unreadable_kind: Some(kind),
        }
    }

    fn not_parsed() -> Self {
        Self::Malformed {
            salvaged: None,
            bad_records: 0,
            unreadable_kind: None,
        }
    }
}

/// Reads `extras_dir/extras.json` without following a symlink and without
/// parsing more than [`MAX_INDEX_BYTES`].
pub fn read_index(extras_dir: &Path) -> IndexRead {
    let path = extras_dir.join(EXTRAS_JSON);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return IndexRead::Absent,
        Err(_) => return IndexRead::unreadable(UnreadableKind::Io),
    };
    if let Some(kind) = unreadable_kind(&meta) {
        return IndexRead::unreadable(kind);
    }
    let file = match open_index(&path) {
        Ok(f) => f,
        Err(kind) => return IndexRead::unreadable(kind),
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_INDEX_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return IndexRead::unreadable(UnreadableKind::Io);
    }
    if bytes.len() as u64 > MAX_INDEX_BYTES {
        return IndexRead::unreadable(UnreadableKind::TooLarge);
    }
    parse_index(&bytes)
}

/// The raw bytes of `extras_dir/extras.json`, read like [`read_index`]
/// (never through a symlink, never more than [`MAX_INDEX_BYTES`]): what a
/// malformed-file backup copies (D5). `None` when it cannot be read so.
pub(crate) fn read_index_bytes(extras_dir: &Path) -> Option<Vec<u8>> {
    let path = extras_dir.join(EXTRAS_JSON);
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if unreadable_kind(&meta).is_some() {
        return None;
    }
    let file = open_index(&path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_INDEX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= MAX_INDEX_BYTES).then_some(bytes)
}

/// Opens `path` for reading without following a final symlink (`O_NOFOLLOW`
/// on Unix, the reparse point itself on Windows) and without blocking on a
/// FIFO (`O_NONBLOCK`), then checks the open handle again: the path may have
/// been swapped since the `symlink_metadata` check.
fn open_index(path: &Path) -> Result<File, UnreadableKind> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = opts.open(path).map_err(|e| open_error_kind(&e))?;
    let meta = file.metadata().map_err(|_| UnreadableKind::Io)?;
    match unreadable_kind(&meta) {
        Some(kind) => Err(kind),
        None => Ok(file),
    }
}

/// `ELOOP` is what `O_NOFOLLOW` answers for a symlink.
fn open_error_kind(e: &std::io::Error) -> UnreadableKind {
    #[cfg(unix)]
    if e.raw_os_error() == Some(libc::ELOOP) {
        return UnreadableKind::Symlink;
    }
    #[cfg(not(unix))]
    let _ = e;
    UnreadableKind::Io
}

fn unreadable_kind(meta: &std::fs::Metadata) -> Option<UnreadableKind> {
    let ft = meta.file_type();
    if ft.is_symlink() {
        Some(UnreadableKind::Symlink)
    } else if ft.is_dir() {
        Some(UnreadableKind::Folder)
    } else if !ft.is_file() {
        Some(UnreadableKind::Io)
    } else if meta.len() > MAX_INDEX_BYTES {
        Some(UnreadableKind::TooLarge)
    } else {
        None
    }
}

/// Parses the bytes of an `extras.json` (D5).
fn parse_index(bytes: &[u8]) -> IndexRead {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return IndexRead::not_parsed();
    };
    let Value::Object(top) = value else {
        return IndexRead::not_parsed();
    };
    let version = top.get("version").and_then(Value::as_u64);
    let records = top.get("files").map(|f| f.as_array());
    if let Some(v) = version.filter(|&v| v > u64::from(ExtrasFile::VERSION)) {
        let version = u32::try_from(v).unwrap_or(u32::MAX);
        let (files, _) = parse_records(records.flatten().map_or(&[][..], Vec::as_slice));
        let mut partial = salvage(&top, files);
        partial.version = version;
        return IndexRead::Newer { version, partial };
    }
    let (files, bad_records) = match records {
        // A missing `files` is an empty index.
        None => (Vec::new(), 0),
        Some(Some(list)) => parse_records(list),
        Some(None) => return IndexRead::not_parsed(),
    };
    if bad_records == 0 && version == Some(u64::from(ExtrasFile::VERSION)) {
        if let Ok(whole) = serde_json::from_value::<ExtrasFile>(Value::Object(top.clone())) {
            return IndexRead::Ok(whole);
        }
    }
    IndexRead::Malformed {
        salvaged: Some(salvage(&top, files)),
        bad_records,
        unreadable_kind: None,
    }
}

/// Each record parsed on its own; those with a bad shape or a `path` that is
/// not plain relative components are counted, not kept.
fn parse_records(list: &[Value]) -> (Vec<ExtraRecord>, usize) {
    let mut good = Vec::with_capacity(list.len());
    let mut bad = 0;
    for v in list {
        match serde_json::from_value::<ExtraRecord>(v.clone()) {
            Ok(r) if rel_components(&r.path).is_some() => good.push(r),
            _ => bad += 1,
        }
    }
    (good, bad)
}

/// A version 1 index from what parses: categories one by one, the good
/// records, and every other top-level field kept as it was.
fn salvage(top: &Map<String, Value>, files: Vec<ExtraRecord>) -> ExtrasFile {
    let mut out = ExtrasFile::empty();
    out.files = files;
    if let Some(Value::Object(cats)) = top.get("categories") {
        for (name, meta) in cats {
            if let Ok(m) = serde_json::from_value::<CategoryMeta>(meta.clone()) {
                out.categories.insert(name.clone(), m);
            }
        }
    }
    out.rest = top
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "version" | "categories" | "files"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<BTreeMap<_, _>>();
    out
}

/// The parts of a stored `path`: `/`-separated, each one a plain name on
/// every system (no empty part, `.`, `..`, `\`, NUL, root or drive prefix).
pub(crate) fn rel_components(path: &str) -> Option<Vec<&str>> {
    if path.is_empty() || path.contains(['\\', '\0']) {
        return None;
    }
    let parts: Vec<&str> = path.split('/').collect();
    for part in &parts {
        if part.is_empty() || is_drive_prefix(part) {
            return None;
        }
        let mut comps = Path::new(part).components();
        match (comps.next(), comps.next()) {
            (Some(Component::Normal(n)), None) if n == *part => {}
            _ => return None,
        }
    }
    Some(parts)
}

/// `C:` and `c:rest`, a drive prefix on Windows whatever the host.
fn is_drive_prefix(part: &str) -> bool {
    let b = part.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// `extras.json` bytes: records sorted by NFC path, object keys sorted,
/// two-space indent and a trailing newline, so diffs stay stable (D2).
pub fn serialize(f: &ExtrasFile) -> Vec<u8> {
    let mut sorted = f.clone();
    sorted.files.sort_by_cached_key(|r| {
        let nfc: String = r.path.nfc().collect();
        (nfc, r.path.clone())
    });
    let value = serde_json::to_value(&sorted).expect("an ExtrasFile is always JSON");
    let mut out = Vec::new();
    serde_json::to_writer_pretty(&mut out, &sort_value(value)).expect("writing to a Vec");
    out.push(b'\n');
    out
}

/// `value` with every object's keys in order, whatever map type
/// `serde_json` was built with.
fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> =
                map.into_iter().map(|(k, v)| (k, sort_value(v))).collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_value).collect()),
        other => other,
    }
}

/// Most files one listing returns (D4).
pub const LIST_CAP: usize = 10_000;
/// Deepest a listed file sits below `Extras/` (D4): `Category/a/b/file`.
pub const LIST_DEPTH: usize = 4;

/// How a listed file found its record (D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Match {
    /// The record's `path`, compared by NFC components.
    Path,
    /// Moved or renamed outside the app: one unclaimed file with the
    /// record's size and mtime, or with its file name and size.
    Moved,
    /// No record: an implicit `user` record with `origin.how = "found"`.
    Found,
}

/// What reading `extras.json` found, for the panel (D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum IndexState {
    Ok,
    Absent,
    Malformed {
        bad_records: usize,
        unreadable: Option<UnreadableKind>,
    },
    Newer {
        version: u32,
    },
}

/// One file below `Extras/`, with its reconciled record.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListedFile {
    /// The on-disk spelling, `/`-joined, relative to `Extras/`.
    pub path: String,
    pub category: CategoryRef,
    /// The file name on disk.
    pub name: String,
    pub size: u64,
    pub mtime_ns: i64,
    /// A symlink: listed, never followed or served.
    pub link: bool,
    /// The matched record carried to this path, or an implicit `found` one.
    pub record: ExtraRecord,
    pub matched: Match,
}

/// One category folder (or the files loose in `Extras/`, [`CategoryRef::Root`]).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListedCategory {
    pub category: CategoryRef,
    /// Every spelling on disk of this one category (D1), in name order.
    pub spellings: Vec<String>,
    /// A spelling is a symlink: listed, never entered.
    pub link: bool,
    pub count: usize,
}

/// `Extras/` as it is on disk, reconciled with `extras.json` in memory
/// (D3, D4). Built without writing anything.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Listing {
    pub index: IndexState,
    /// Fixed categories in their order, then user ones by name, then
    /// [`CategoryRef::Root`].
    pub categories: Vec<ListedCategory>,
    pub files: Vec<ListedFile>,
    /// Records with no file («No encontrados»): never dropped.
    pub missing: Vec<ExtraRecord>,
    /// Records whose file is still on disk but was not listed (past the
    /// cap, deeper than [`LIST_DEPTH`], a hidden name): neither shown nor
    /// «No encontrados», and kept as they are by the next write.
    pub unreached: Vec<ExtraRecord>,
    /// The walk stopped at [`LIST_CAP`] files.
    pub capped: bool,
    /// What was read (whole, salvaged or partial), the base of the next write.
    pub base: Option<ExtrasFile>,
}

/// Lists `extras_dir` and reconciles its files with `extras.json` (D1, D4):
/// regular files at most [`LIST_DEPTH`] deep, symlinks listed but never
/// followed, hidden and junk names always skipped, and the index and its
/// malformed-file backups at the root left out. Never writes.
pub fn list(extras_dir: &Path) -> Listing {
    let (index, base) = match read_index(extras_dir) {
        IndexRead::Ok(f) => (IndexState::Ok, Some(f)),
        IndexRead::Absent => (IndexState::Absent, None),
        IndexRead::Malformed {
            salvaged,
            bad_records,
            unreadable_kind,
        } => (
            IndexState::Malformed {
                bad_records,
                unreadable: unreadable_kind,
            },
            salvaged,
        ),
        IndexRead::Newer { version, partial } => (IndexState::Newer { version }, Some(partial)),
    };
    let walked = walk(extras_dir);
    let records = base.as_ref().map_or(&[][..], |b| b.files.as_slice());
    let (mut files, missing, unreached) = reconcile(extras_dir, walked.files, records);
    let categories = categories(&walked.dirs, &files);
    canonical_categories(&mut files, &categories);
    Listing {
        index,
        categories,
        files,
        missing,
        unreached,
        capped: walked.capped,
        base,
    }
}

/// A file found by the walk, before reconcile.
struct OnDisk {
    parts: Vec<String>,
    size: u64,
    mtime_ns: i64,
    link: bool,
}

/// A direct sub-folder (or symlink to one) of `Extras/`.
struct TopDir {
    name: String,
    link: bool,
}

struct Walked {
    dirs: Vec<TopDir>,
    files: Vec<OnDisk>,
    capped: bool,
}

/// Whether a name is never listed: hidden whatever `include_hidden` says,
/// AppleDouble twins, OS junk and partial downloads (D4).
fn skipped_name(name: &str) -> bool {
    name.starts_with('.') || is_skipped_entry(name, &ScanOptions::default())
}

/// The index and its malformed-file backups (D4, D9), ignoring ASCII case:
/// on the case-insensitive disks of macOS and Windows `EXTRAS.JSON` is the
/// index itself, never a file to show, rename or trash.
fn is_index_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == EXTRAS_JSON || (name.starts_with("extras.broken-") && name.ends_with(".json"))
}

fn walk(extras_dir: &Path) -> Walked {
    let mut out = Walked {
        dirs: Vec::new(),
        files: Vec::new(),
        capped: false,
    };
    if !std::fs::symlink_metadata(extras_dir).is_ok_and(|m| m.is_dir()) {
        return out;
    }
    let walker = WalkDir::new(extras_dir)
        .min_depth(1)
        .max_depth(LIST_DEPTH)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !skipped_name(&name) && !(e.depth() == 1 && is_index_name(&name))
        });
    for entry in walker.flatten() {
        let ft = entry.file_type();
        let name = entry.file_name().to_string_lossy().into_owned();
        // A symlink at the top is a category when it points at a folder;
        // the target is only stat'ed, never entered.
        let link_to_dir =
            ft.is_symlink() && std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir());
        if entry.depth() == 1 && (ft.is_dir() || link_to_dir) {
            out.dirs.push(TopDir {
                name,
                link: ft.is_symlink(),
            });
            continue;
        }
        if !(ft.is_file() || (ft.is_symlink() && !link_to_dir)) {
            continue;
        }
        if out.files.len() == LIST_CAP {
            out.capped = true;
            break;
        }
        let Ok(meta) = entry.path().symlink_metadata() else {
            continue;
        };
        let Ok(rel) = entry.path().strip_prefix(extras_dir) else {
            continue;
        };
        out.files.push(OnDisk {
            parts: rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect(),
            size: meta.len(),
            mtime_ns: meta.modified().map_or(0, write_mtime_ns),
            link: ft.is_symlink(),
        });
    }
    out
}

/// The category of a file from its parts: a fixed or user folder, or the
/// root for a file loose in `Extras/` (D1).
fn category_of(parts: &[String]) -> CategoryRef {
    match parts {
        [_] | [] => CategoryRef::Root,
        [first, ..] => dir_category(first),
    }
}

fn dir_category(name: &str) -> CategoryRef {
    ExtrasCategory::from_dir_name(name)
        .map_or_else(|| CategoryRef::User(name.to_owned()), CategoryRef::Fixed)
}

/// A path compared by its NFC components, case kept (D4).
fn nfc_key<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    parts
        .into_iter()
        .map(|c| c.nfc().collect::<String>())
        .collect::<Vec<_>>()
        .join("/")
}

/// Matches records to files in the three passes of D4. A pass only
/// pairs a record and a file that are each other's only candidate, so
/// identical twins never merge and two records never share one file.
/// Returns the listed files, the missing records and the unreached ones.
fn reconcile(
    extras_dir: &Path,
    disk: Vec<OnDisk>,
    records: &[ExtraRecord],
) -> (Vec<ListedFile>, Vec<ExtraRecord>, Vec<ExtraRecord>) {
    let mut file_rec: Vec<Option<(usize, Match)>> = vec![None; disk.len()];
    let mut rec_done = vec![false; records.len()];
    // A second record with a claimed path: never paired by fingerprint or
    // name, which would hand its tags to an unrelated file.
    let mut rec_dup = vec![false; records.len()];

    // (1) Same path by NFC components.
    let mut by_path: HashMap<String, usize> = HashMap::new();
    for (i, f) in disk.iter().enumerate() {
        by_path
            .entry(nfc_key(f.parts.iter().map(String::as_str)))
            .or_insert(i);
    }
    for (r, rec) in records.iter().enumerate() {
        let Some(parts) = rel_components(&rec.path) else {
            continue;
        };
        if let Some(&i) = by_path.get(&nfc_key(parts)) {
            if file_rec[i].is_none() {
                file_rec[i] = Some((r, Match::Path));
                rec_done[r] = true;
            } else {
                rec_dup[r] = true;
            }
        }
    }

    // (2) Same size and mtime; (3) same file name (NFC) and size. Links
    // never match: their size is the link's, not the file's.
    let file_name = |p: &str| p.nfc().collect::<String>();
    let fingerprint = |size: u64, mtime: i64, _: &str| (size, mtime, String::new());
    let name_size = |size: u64, _: i64, name: &str| (size, 0, file_name(name));
    for key in [
        &fingerprint as &dyn Fn(u64, i64, &str) -> (u64, i64, String),
        &name_size,
    ] {
        let mut files_by: HashMap<(u64, i64, String), Vec<usize>> = HashMap::new();
        for (i, f) in disk.iter().enumerate() {
            if file_rec[i].is_none() && !f.link {
                let name = f.parts.last().map_or("", String::as_str);
                files_by
                    .entry(key(f.size, f.mtime_ns, name))
                    .or_default()
                    .push(i);
            }
        }
        let mut recs_by: HashMap<(u64, i64, String), Vec<usize>> = HashMap::new();
        for (r, rec) in records.iter().enumerate() {
            if !rec_done[r] && !rec_dup[r] {
                let name = rec.path.rsplit('/').next().unwrap_or("");
                recs_by
                    .entry(key(rec.size, rec.mtime_ns, name))
                    .or_default()
                    .push(r);
            }
        }
        for (k, recs) in recs_by {
            if let ([r], Some([i])) = (recs.as_slice(), files_by.get(&k).map(Vec::as_slice)) {
                file_rec[*i] = Some((*r, Match::Moved));
                rec_done[*r] = true;
            }
        }
    }

    let listed_keys: std::collections::HashSet<String> = disk
        .iter()
        .map(|f| {
            f.parts
                .iter()
                .map(|p| name_key(p))
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    let files = disk
        .into_iter()
        .zip(file_rec)
        .map(|(f, matched)| listed(f, matched.map(|(r, m)| (&records[r], m))))
        .collect();
    let (mut missing, mut unreached) = (Vec::new(), Vec::new());
    for (rec, _) in records.iter().zip(rec_done).filter(|(_, done)| !done) {
        if unreached_on_disk(extras_dir, rec, &listed_keys) {
            unreached.push(rec.clone());
        } else {
            missing.push(rec.clone());
        }
    }
    (files, missing, unreached)
}

/// A record whose path still holds a file the walk did not list (past
/// the cap, too deep, a hidden name) was only not reached: it is not «No
/// encontrado». On a case-insensitive disk a listed file in another case
/// is the same file, so that record stays missing and visible.
fn unreached_on_disk(
    extras_dir: &Path,
    rec: &ExtraRecord,
    listed_keys: &std::collections::HashSet<String>,
) -> bool {
    rel_components(&rec.path).is_some_and(|parts| {
        let key = parts
            .iter()
            .map(|p| name_key(p))
            .collect::<Vec<_>>()
            .join("/");
        if listed_keys.contains(&key) {
            return false;
        }
        let path = parts
            .iter()
            .fold(extras_dir.to_path_buf(), |p, c| p.join(c));
        std::fs::symlink_metadata(path).is_ok_and(|m| !m.is_dir())
    })
}

fn listed(f: OnDisk, matched: Option<(&ExtraRecord, Match)>) -> ListedFile {
    let path = f.parts.join("/");
    let name = f.parts.last().cloned().unwrap_or_default();
    let kind = type_of(
        Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(""),
    );
    let (record, matched) = match matched {
        Some((rec, how)) => {
            let mut rec = rec.clone();
            rec.path.clone_from(&path);
            rec.kind = kind;
            rec.size = f.size;
            rec.mtime_ns = f.mtime_ns;
            (rec, how)
        }
        None => (
            ExtraRecord {
                path: path.clone(),
                kind,
                provenance: Provenance::User,
                origin: Origin {
                    how: OriginHow::Found,
                    format: None,
                    from_name: None,
                    rip: None,
                    rest: BTreeMap::new(),
                },
                tags: Vec::new(),
                size: f.size,
                mtime_ns: f.mtime_ns,
                added_at: rfc3339_utc(time_of(f.mtime_ns)),
                rest: BTreeMap::new(),
            },
            Match::Found,
        ),
    };
    ListedFile {
        category: category_of(&f.parts),
        path,
        name,
        size: f.size,
        mtime_ns: f.mtime_ns,
        link: f.link,
        record,
        matched,
    }
}

fn time_of(ns: i64) -> SystemTime {
    let d = Duration::from_nanos(ns.unsigned_abs());
    if ns >= 0 {
        UNIX_EPOCH + d
    } else {
        UNIX_EPOCH - d
    }
}

/// The categories: every top folder (spellings of one category merged by
/// name key, D1) and the root when files sit loose in `Extras/`.
fn categories(dirs: &[TopDir], files: &[ListedFile]) -> Vec<ListedCategory> {
    let mut out: Vec<ListedCategory> = Vec::new();
    for d in dirs {
        let cat = dir_category(&d.name);
        let key = cat_key(&cat);
        match out.iter_mut().find(|c| cat_key(&c.category) == key) {
            Some(c) => {
                c.spellings.push(d.name.clone());
                c.link |= d.link;
            }
            None => out.push(ListedCategory {
                category: cat,
                spellings: vec![d.name.clone()],
                link: d.link,
                count: 0,
            }),
        }
    }
    let mut root = 0;
    for f in files {
        if f.category == CategoryRef::Root {
            root += 1;
            continue;
        }
        let key = cat_key(&f.category);
        if let Some(c) = out.iter_mut().find(|c| cat_key(&c.category) == key) {
            c.count += 1;
        }
    }
    out.sort_by_cached_key(|c| sort_key(&c.category));
    if root > 0 {
        out.push(ListedCategory {
            category: CategoryRef::Root,
            spellings: Vec::new(),
            link: false,
            count: root,
        });
    }
    out
}

/// A file in a second spelling of a user category carries the category
/// under its first spelling, as the category list does (D1).
fn canonical_categories(files: &mut [ListedFile], cats: &[ListedCategory]) {
    for f in files {
        if let CategoryRef::User(_) = f.category {
            let key = cat_key(&f.category);
            if let Some(c) = cats.iter().find(|c| cat_key(&c.category) == key) {
                f.category.clone_from(&c.category);
            }
        }
    }
}

/// One key per category whatever its spelling (D1).
fn cat_key(c: &CategoryRef) -> (u8, String) {
    match c {
        CategoryRef::Fixed(f) => (0, f.dir_name().to_owned()),
        CategoryRef::User(n) => (1, name_key(n)),
        CategoryRef::Root => (2, String::new()),
    }
}

/// Fixed in their order, user by name ignoring case, root last.
fn sort_key(c: &CategoryRef) -> (u8, usize, String) {
    match c {
        CategoryRef::Fixed(f) => (
            0,
            ExtrasCategory::ALL.iter().position(|a| a == f).unwrap_or(0),
            String::new(),
        ),
        CategoryRef::User(n) => (1, 0, name_key(n)),
        CategoryRef::Root => (2, 0, String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The open itself refuses a symlink, so one swapped in after the
    /// `symlink_metadata` check is never followed either.
    #[cfg(unix)]
    #[test]
    fn opening_never_follows_a_symlink() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("outside.json");
        std::fs::write(&target, b"{\"version\":1}").unwrap();
        let link = d.path().join(EXTRAS_JSON);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(open_index(&link).err(), Some(UnreadableKind::Symlink));
        assert!(open_index(&target).is_ok());
    }

    /// A FIFO swapped in after the check is refused without blocking.
    #[cfg(unix)]
    #[test]
    fn opening_a_fifo_never_blocks() {
        let d = tempfile::tempdir().unwrap();
        let fifo = d.path().join(EXTRAS_JSON);
        let c = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert_eq!(open_index(&fifo).err(), Some(UnreadableKind::Io));
    }

    fn on_disk(rel: &str) -> OnDisk {
        OnDisk {
            parts: rel.split('/').map(str::to_owned).collect(),
            size: 1,
            mtime_ns: 0,
            link: false,
        }
    }

    /// Spellings of one category merge on any disk; the test that writes
    /// both spellings only runs on case-sensitive disks.
    #[test]
    fn spellings_of_one_category_merge_and_files_carry_the_first() {
        let dirs: Vec<TopDir> = ["Remixes", "Soundtrack", "remixes", "soundtrack"]
            .into_iter()
            .map(|n| TopDir {
                name: n.into(),
                link: false,
            })
            .collect();
        let mut files: Vec<ListedFile> =
            ["Soundtrack/a.flac", "soundtrack/b.flac", "remixes/c.flac"]
                .into_iter()
                .map(|r| listed(on_disk(r), None))
                .collect();
        let cats = categories(&dirs, &files);
        canonical_categories(&mut files, &cats);
        assert_eq!(cats.len(), 2);
        assert_eq!(
            cats[0].category,
            CategoryRef::Fixed(ExtrasCategory::Soundtrack)
        );
        assert_eq!(cats[0].spellings, ["Soundtrack", "soundtrack"]);
        assert_eq!(cats[0].count, 2);
        assert_eq!(cats[1].category, CategoryRef::User("Remixes".into()));
        assert_eq!(cats[1].spellings, ["Remixes", "remixes"]);
        assert_eq!(cats[1].count, 1);
        assert_eq!(files[2].category, CategoryRef::User("Remixes".into()));
        assert_eq!(files[2].path, "remixes/c.flac");
    }

    #[test]
    fn plain_relative_paths_split_into_components() {
        assert_eq!(
            rel_components("Soundtrack/01 Opening.flac"),
            Some(vec!["Soundtrack", "01 Opening.flac"])
        );
        assert_eq!(rel_components("loose.txt"), Some(vec!["loose.txt"]));
        assert_eq!(rel_components("Remixes/live/a b.flac").unwrap().len(), 3);
        for bad in [
            "", "/a", "a//b", "./a", "a/.", "a/..", "..", "a\\b", "C:", "c:x/y", "a/", "a\0b",
        ] {
            assert_eq!(rel_components(bad), None, "{bad:?}");
        }
    }
}
