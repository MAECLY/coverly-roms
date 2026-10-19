//! Batocera/Knulli `gamelist.xml` (spec §16.8, M2-10, D31): after a fill,
//! the art paths are merged into `<system dir>/gamelist.xml`. The file is
//! untrusted. It is parsed with `quick-xml` only to find byte offsets; the
//! output is the input with a few spliced edits, so every byte the merge
//! does not change (declaration, BOM, comments, unknown elements and
//! attributes, line endings) is kept. Entities are never expanded and a
//! DOCTYPE is refused. The write goes through the job's `Writer` (backup +
//! journal), so Undo restores the file byte for byte.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use unicode_normalization::UnicodeNormalization;

use crate::profiles::Media;
use crate::write::{OverwritePolicy, Writer};

pub const GAMELIST_FILE: &str = "gamelist.xml";
pub const MAX_GAMELIST_BYTES: usize = 32 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

/// The art written for one ROM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameArt {
    /// ROM path relative to the system dir, `./`-prefixed, `/`-separated.
    pub rom: String,
    /// Display name for a new `<game>` (the ROM stem).
    pub name: String,
    /// Media and art path (relative like `rom`).
    pub art: Vec<(Media, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MergeError {
    #[error("gamelist.xml is larger than 32 MiB")]
    TooLarge,
    #[error("gamelist.xml is not UTF-8")]
    NotUtf8,
    #[error("gamelist.xml declares a DOCTYPE; it is left as it is")]
    Doctype,
    #[error("gamelist.xml is not well-formed: {0}")]
    Malformed(String),
    #[error("gamelist.xml has no <gameList> root")]
    NoRoot,
    #[error("gamelist.xml is not a regular file")]
    NotAFile,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Set when missing, empty or pointing to a missing file; replaced in
    /// a "replace existing art" job.
    Normal,
    /// Set only when missing, empty or pointing to a missing file.
    OnlyIfUnusable,
}

/// D31: cover → thumbnail (+ image when unusable), screenshot → image,
/// title → titleshot.
fn wanted(art: &[(Media, String)]) -> Vec<(&'static str, String, Mode)> {
    let of = |m: Media| art.iter().find(|(x, _)| *x == m).map(|(_, v)| v.clone());
    let (cover, shot, title) = (of(Media::Cover), of(Media::Screenshot), of(Media::Title));
    let mut out = Vec::new();
    if let Some(c) = &cover {
        out.push(("thumbnail", c.clone(), Mode::Normal));
    }
    match (shot, &cover) {
        (Some(s), _) => out.push(("image", s, Mode::Normal)),
        (None, Some(c)) => out.push(("image", c.clone(), Mode::OnlyIfUnusable)),
        _ => {}
    }
    if let Some(t) = title {
        out.push(("titleshot", t, Mode::Normal));
    }
    out
}

/// Byte offsets of one element: `start..inner_start` is the start tag,
/// `inner_end..end` the end tag (all equal ends for `<x/>`).
#[derive(Clone, Debug)]
struct Elem {
    name: String,
    start: usize,
    inner_start: usize,
    inner_end: usize,
    end: usize,
    empty: bool,
}

struct GameNode {
    elem: Elem,
    children: Vec<Elem>,
}

struct Doc {
    root: Elem,
    games: Vec<GameNode>,
}

/// Offset in the whole file of the reader's position in the parsed part
/// (`base` bytes in: quick-xml skips a BOM without counting it).
fn pos(reader: &Reader<&[u8]>, base: usize) -> usize {
    usize::try_from(reader.buffer_position())
        .ok()
        .and_then(|p| p.checked_add(base))
        .unwrap_or(usize::MAX)
}

/// The offsets of `e` lie inside `xml`, in order and on char boundaries,
/// so slicing with them can never panic.
fn sliceable(xml: &str, e: &Elem) -> bool {
    let offs = [e.start, e.inner_start, e.inner_end, e.end];
    offs.windows(2).all(|w| w[0] <= w[1]) && offs.iter().all(|&o| xml.is_char_boundary(o))
}

fn scan(xml: &str) -> Result<Doc, MergeError> {
    // The BOM is parsed away by hand so every offset counts it.
    let (body, base) = match xml.strip_prefix('\u{feff}') {
        Some(rest) => (rest, xml.len() - rest.len()),
        None => (xml, 0),
    };
    let mut reader = Reader::from_str(body);
    reader.config_mut().check_end_names = true;
    let mut stack: Vec<(String, usize, usize)> = Vec::new();
    let mut root: Option<Elem> = None;
    let mut games = Vec::new();
    let mut current: Option<GameNode> = None;
    let malformed = |m: &str| MergeError::Malformed(m.to_string());
    loop {
        let before = pos(&reader, base);
        let event = reader
            .read_event()
            .map_err(|e| MergeError::Malformed(e.to_string()))?;
        let after = pos(&reader, base);
        if after > xml.len() || !xml.is_char_boundary(before) || !xml.is_char_boundary(after) {
            return Err(malformed("offsets out of place"));
        }
        match event {
            Event::DocType(_) => return Err(MergeError::Doctype),
            Event::Text(t) if stack.is_empty() => {
                if !t.iter().all(u8::is_ascii_whitespace) {
                    return Err(malformed("text outside the root element"));
                }
            }
            Event::CData(_) | Event::GeneralRef(_) if stack.is_empty() => {
                return Err(malformed("text outside the root element"));
            }
            Event::Start(e) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(malformed("nested too deeply"));
                }
                if stack.is_empty() && root.is_some() {
                    return Err(malformed("two root elements"));
                }
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if stack.len() == 1 && name == "game" {
                    current = Some(GameNode {
                        elem: Elem {
                            name: name.clone(),
                            start: before,
                            inner_start: after,
                            inner_end: after,
                            end: after,
                            empty: false,
                        },
                        children: Vec::new(),
                    });
                }
                stack.push((name, before, after));
            }
            Event::Empty(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let elem = Elem {
                    name,
                    start: before,
                    inner_start: after,
                    inner_end: after,
                    end: after,
                    empty: true,
                };
                match stack.len() {
                    0 if root.is_some() => return Err(malformed("two root elements")),
                    0 => root = Some(elem),
                    2 => {
                        if let Some(g) = current.as_mut() {
                            g.children.push(elem);
                        }
                    }
                    _ => {}
                }
            }
            Event::End(_) => {
                let (name, start, inner_start) =
                    stack.pop().ok_or_else(|| malformed("unexpected end tag"))?;
                let elem = Elem {
                    name,
                    start,
                    inner_start,
                    inner_end: before,
                    end: after,
                    empty: false,
                };
                match stack.len() {
                    0 => root = Some(elem),
                    1 if elem.name == "game" => {
                        if let Some(mut g) = current.take() {
                            g.elem = elem;
                            games.push(g);
                        }
                    }
                    2 => {
                        if let Some(g) = current.as_mut() {
                            g.children.push(elem);
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(malformed("unclosed element"));
    }
    let root = root.ok_or(MergeError::NoRoot)?;
    if root.name != "gameList" {
        return Err(MergeError::NoRoot);
    }
    let all_sliceable = sliceable(xml, &root)
        && games
            .iter()
            .all(|g| sliceable(xml, &g.elem) && g.children.iter().all(|c| sliceable(xml, c)));
    if !all_sliceable {
        return Err(malformed("offsets out of place"));
    }
    Ok(Doc { root, games })
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// Decodes the five predefined entities and character references; `None`
/// for any other entity (never expanded).
fn unescape(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp + 1..];
        let semi = tail.find(';')?;
        let name = &tail[..semi];
        let c = match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            n if n.starts_with("#x") || n.starts_with("#X") => {
                char::from_u32(u32::from_str_radix(&n[2..], 16).ok()?)?
            }
            n if n.starts_with('#') => char::from_u32(n[1..].parse().ok()?)?,
            _ => return None,
        };
        out.push(c);
        rest = &tail[semi + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// The plain text of `e` (`None` when it holds markup or an unknown entity).
fn text_of(xml: &str, e: &Elem) -> Option<String> {
    if e.empty {
        return Some(String::new());
    }
    let raw = xml.get(e.inner_start..e.inner_end)?.trim();
    if let Some(inner) = raw
        .strip_prefix("<![CDATA[")
        .and_then(|r| r.strip_suffix("]]>"))
    {
        return Some(inner.to_string());
    }
    if raw.contains('<') {
        return None;
    }
    unescape(raw)
}

/// Comparison form of a gamelist path.
/// Control characters are dropped, as [`escape`] drops them on write.
/// Surrounding whitespace is trimmed (as ES-DE does when it reads a path),
/// so two ROMs whose names differ only by leading or trailing whitespace
/// share one key: only the first gets its art in the gamelist. Such names
/// are accepted as a known edge; the art files themselves are written.
fn norm(p: &str) -> String {
    let p = p.trim().replace('\\', "/");
    let p = p.strip_prefix("./").unwrap_or(&p);
    p.nfc().filter(|c| !c.is_control() || *c == '\t').collect()
}

/// The games by normalised `<path>`: exact spelling and lower case, each
/// in file order (a game without a plain-text path is not indexed).
struct PathIndex {
    exact: HashMap<String, usize>,
    folded: HashMap<String, Vec<usize>>,
}

impl PathIndex {
    fn new(xml: &str, doc: &Doc) -> PathIndex {
        let mut index = PathIndex {
            exact: HashMap::new(),
            folded: HashMap::new(),
        };
        for (i, g) in doc.games.iter().enumerate() {
            let Some(p) = g
                .children
                .iter()
                .find(|c| c.name == "path")
                .and_then(|c| text_of(xml, c))
            else {
                continue;
            };
            let p = norm(&p);
            index.folded.entry(p.to_lowercase()).or_default().push(i);
            index.exact.entry(p).or_insert(i);
        }
        index
    }

    /// The game whose path is `key`: the exact spelling first, then the
    /// first one equal ignoring case (as ES does), among the games no other
    /// ROM of this merge claimed (`used`: two edits of one game would
    /// overlap).
    fn find(&self, key: &str, used: &[bool]) -> Option<usize> {
        let free = |i: usize| !used.get(i).copied().unwrap_or(false);
        if let Some(&i) = self.exact.get(key) {
            return free(i).then_some(i);
        }
        self.folded
            .get(&key.to_lowercase())?
            .iter()
            .copied()
            .find(|&i| free(i))
    }
}

/// Whitespace before `at` on its line (the indentation of an element).
fn indent_before(xml: &str, at: usize) -> &str {
    let Some(head) = xml.get(..at) else {
        return "";
    };
    let line_start = head.rfind('\n').map_or(0, |i| i + 1);
    let ws = &head[line_start..];
    if ws.chars().all(|c| c == ' ' || c == '\t') {
        ws
    } else {
        ""
    }
}

fn newline_of(xml: &str) -> &'static str {
    if xml.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Merges `games` into `existing` (a new file when `None`). `exists` says
/// whether a gamelist path (as written in the file) points to a file.
pub fn merge(
    existing: Option<&str>,
    games: &[GameArt],
    replace: bool,
    exists: &dyn Fn(&str) -> bool,
) -> Result<Option<String>, MergeError> {
    if games.is_empty() {
        return Ok(None);
    }
    let fresh = "<?xml version=\"1.0\"?>\n<gameList>\n</gameList>\n";
    let xml = existing.unwrap_or(fresh);
    if xml.len() > MAX_GAMELIST_BYTES {
        return Err(MergeError::TooLarge);
    }
    let doc = scan(xml)?;
    let nl = newline_of(xml);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut appended = String::new();
    let index = PathIndex::new(xml, &doc);
    let mut used = vec![false; doc.games.len()];
    let mut seen: HashSet<String> = HashSet::new();
    // ROMs spelled exactly as in the file claim their games before any
    // match ignoring case; the same ROM twice is merged once.
    let mut order: Vec<(String, &GameArt)> = games.iter().map(|g| (norm(&g.rom), g)).collect();
    order.sort_by_key(|(key, _)| !index.exact.contains_key(key));
    for (key, game) in order {
        if !seen.insert(key.clone()) {
            continue;
        }
        let tags = wanted(&game.art);
        match index.find(&key, &used) {
            Some(i) => {
                used[i] = true;
                let g = &doc.games[i];
                let child_indent = g
                    .children
                    .first()
                    .map(|c| indent_before(xml, c.start).to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "\t\t".to_string());
                let mut inserted = String::new();
                for (tag, value, mode) in tags {
                    let escaped = escape(&value);
                    match g.children.iter().find(|c| c.name == tag) {
                        Some(child) => {
                            let current = text_of(xml, child).unwrap_or_default();
                            if norm(&current) == norm(&value) {
                                continue;
                            }
                            let usable = !current.trim().is_empty() && exists(&current);
                            let set = match mode {
                                Mode::Normal => !usable || replace,
                                Mode::OnlyIfUnusable => !usable,
                            };
                            if !set {
                                continue;
                            }
                            if child.empty {
                                edits.push((
                                    child.start,
                                    child.end,
                                    format!("<{tag}>{escaped}</{tag}>"),
                                ));
                            } else {
                                edits.push((child.inner_start, child.inner_end, escaped));
                            }
                        }
                        None => {
                            inserted
                                .push_str(&format!("{nl}{child_indent}<{tag}>{escaped}</{tag}>"));
                        }
                    }
                }
                if !inserted.is_empty() {
                    let inner = xml
                        .get(g.elem.inner_start..g.elem.inner_end)
                        .ok_or_else(|| MergeError::Malformed("offsets out of place".into()))?;
                    let at = g.elem.inner_start + inner.trim_end().len();
                    let compact = !inner.contains('\n');
                    let text = if compact {
                        inserted.replace(&format!("{nl}{child_indent}"), "")
                    } else {
                        inserted
                    };
                    edits.push((at, at, text));
                }
            }
            None => {
                appended.push_str(&format!(
                    "{nl}\t<game>{nl}\t\t<path>{}</path>{nl}\t\t<name>{}</name>",
                    escape(&game.rom),
                    escape(&game.name)
                ));
                for (tag, value, _) in tags {
                    appended.push_str(&format!("{nl}\t\t<{tag}>{}</{tag}>", escape(&value)));
                }
                appended.push_str(&format!("{nl}\t</game>"));
            }
        }
    }
    if !appended.is_empty() {
        let r = &doc.root;
        if r.empty {
            edits.push((
                r.start,
                r.end,
                format!("<gameList>{appended}{nl}</gameList>"),
            ));
        } else {
            let inner = xml
                .get(r.inner_start..r.inner_end)
                .ok_or_else(|| MergeError::Malformed("offsets out of place".into()))?;
            let at = r.inner_start + inner.trim_end().len();
            let tail = if inner[inner.trim_end().len()..].contains('\n') {
                ""
            } else {
                nl
            };
            edits.push((at, at, format!("{appended}{tail}")));
        }
    }
    if edits.is_empty() {
        return Ok(None);
    }
    // One pass: edits never overlap (each game is edited by one ROM), and
    // an insertion sorts before a replacement starting at the same byte.
    edits.sort_by_key(|(start, end, _)| (*start, *end));
    let mut out = String::with_capacity(xml.len() + edits.iter().map(|e| e.2.len()).sum::<usize>());
    let out_of_place = || MergeError::Malformed("offsets out of place".into());
    let mut at = 0;
    for (start, end, text) in edits {
        if start < at {
            return Err(MergeError::Malformed("overlapping edits".into()));
        }
        out.push_str(xml.get(at..start).ok_or_else(out_of_place)?);
        out.push_str(&text);
        at = end;
    }
    out.push_str(xml.get(at..).ok_or_else(out_of_place)?);
    // Defense in depth: a result that does not parse is never written.
    scan(&out)
        .map_err(|e| MergeError::Malformed(format!("the merged file would be broken: {e}")))?;
    Ok(Some(out))
}

/// `./a/b` form of a relative path.
pub fn rel_slash(p: &Path) -> String {
    let parts: Vec<String> = p
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    format!("./{}", parts.join("/"))
}

/// The gamelist of `system_dir` as spelled on disk. `Writer` treats a
/// case/NFC variant (`GameList.xml`) as the same file, so reading only the
/// canonical name on a case-sensitive disk would make the merge start from
/// an empty list and then *replace* the user's file: read the variant.
fn gamelist_path(system_dir: &Path) -> PathBuf {
    let mut variant = None;
    if let Ok(read) = fs::read_dir(system_dir) {
        for e in read.flatten() {
            let name = e.file_name();
            let Some(n) = name.to_str() else { continue };
            if n == GAMELIST_FILE {
                return e.path();
            }
            if variant.is_none() && n.nfc().collect::<String>().to_lowercase() == GAMELIST_FILE {
                variant = Some(e.path());
            }
        }
    }
    variant.unwrap_or_else(|| system_dir.join(GAMELIST_FILE))
}

/// The text of the gamelist at `path`, bounded; `None` when there is none.
fn read_existing(path: &Path) -> Result<Option<String>, String> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => {
            if m.len() > MAX_GAMELIST_BYTES as u64 {
                return Err(MergeError::TooLarge.to_string());
            }
            // Bounded even if the file grows or is swapped after the check.
            let mut bytes = Vec::new();
            fs::File::open(path)
                .and_then(|f| {
                    f.take(MAX_GAMELIST_BYTES as u64 + 1)
                        .read_to_end(&mut bytes)
                })
                .map_err(|e| e.to_string())?;
            if bytes.len() > MAX_GAMELIST_BYTES {
                return Err(MergeError::TooLarge.to_string());
            }
            Ok(Some(
                String::from_utf8(bytes).map_err(|_| MergeError::NotUtf8.to_string())?,
            ))
        }
        Ok(_) => Err(MergeError::NotAFile.to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// The plain-text fields of the `<game>` of `system_dir/gamelist.xml`
/// whose `<path>` is `rom` (relative to the system dir), in file order:
/// the exact spelling first, then ignoring case (as ES does). `None` when
/// there is no gamelist, it cannot be read or parsed, or no game matches.
/// Fields holding markup are left out.
pub fn game_fields(system_dir: &Path, rom: &Path) -> Option<Vec<(String, String)>> {
    let xml = read_existing(&gamelist_path(system_dir)).ok()??;
    let doc = scan(&xml).ok()?;
    let index = PathIndex::new(&xml, &doc);
    let i = index.find(&norm(&rel_slash(rom)), &[])?;
    let game = doc.games.get(i)?;
    Some(
        game.children
            .iter()
            .filter(|c| sliceable(&xml, c))
            .filter_map(|c| text_of(&xml, c).map(|t| (c.name.clone(), t)))
            .collect(),
    )
}

/// Merges `games` into `system_dir/gamelist.xml` (or its on-disk spelling)
/// through `writer` (backed up and journaled; Undo restores it).
/// `Ok(false)` when nothing changed. Errors are warnings for the job
/// report: the art is written either way.
pub fn merge_into(
    writer: &mut Writer,
    system_dir: &Path,
    games: &[GameArt],
    replace: bool,
) -> Result<bool, String> {
    if system_dir.as_os_str().is_empty() {
        return Err("no system folder for gamelist.xml".into());
    }
    let path = gamelist_path(system_dir);
    let existing = read_existing(&path)?;
    let exists = |rel: &str| {
        let n = norm(rel);
        let p = Path::new(&n);
        // Absolute or `..` paths are the user's business: treat as usable.
        if p.components().any(|c| !matches!(c, Component::Normal(_))) {
            return true;
        }
        system_dir.join(p).exists()
    };
    match merge(existing.as_deref(), games, replace, &exists).map_err(|e| e.to_string())? {
        None => Ok(false),
        Some(text) => {
            writer
                .write(&path, text.as_bytes(), OverwritePolicy::Backup)
                .map_err(|e| e.to_string())?;
            Ok(true)
        }
    }
}
