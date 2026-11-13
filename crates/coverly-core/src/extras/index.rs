//! Reading, serializing and listing `Extras/extras.json` (extras spec D2–D5).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path};

use serde::Serialize;
use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;

use super::{CategoryMeta, ExtraRecord, ExtrasFile, EXTRAS_JSON};

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
    let Ok(file) = File::open(&path) else {
        return IndexRead::unreadable(UnreadableKind::Io);
    };
    // The open handle is checked again: the path may have changed since.
    match file.metadata() {
        Ok(m) => {
            if let Some(kind) = unreadable_kind(&m) {
                return IndexRead::unreadable(kind);
            }
        }
        Err(_) => return IndexRead::unreadable(UnreadableKind::Io),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
