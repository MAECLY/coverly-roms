//! The bases a target system folder already holds (game folders spec
//! D12, D13, plan P-4): what a demo is linked to by name and what an
//! orphan update or DLC joins by its base title id.
//!
//! One listing of `<roms>/<System>/` (top-level entries only) per system
//! that has a demo or an orphan in the plan. Names are free; a header is
//! read only when the caller asks for title ids (systems with an orphan),
//! through the caller's reader, which answers from the memoised facts
//! (size and mtime keyed) and charges the plan budget for a fresh read.
//! Licences and other key files are skipped by name, never opened.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::consoles::{base_title_id, is_key_material, link_kind, ConsoleId, TitleKind};
use crate::identify::names::{parse_tags, Region};
use crate::systems::SystemDb;

/// What a fresh header read of the lookup costs against the plan's head
/// budget (`Limits::plan_head_budget`): a reader looks at a few KiB of a
/// header, so 64 MiB allow about a thousand fresh reads per plan.
pub const LIBRARY_READ_COST: u64 = 64 * 1024;

/// The part of the plan budget the library lookup may spend: fresh header
/// reads (memoised ones are free) and wall time.
#[derive(Clone, Debug)]
pub struct LookupBudget {
    bytes_left: u64,
    /// `None`: a wall time too long for an `Instant` (no deadline).
    deadline: Option<Instant>,
}

impl LookupBudget {
    /// `bytes` of fresh reads within `wall` from now.
    pub fn new(bytes: u64, wall: Duration) -> LookupBudget {
        LookupBudget {
            bytes_left: bytes,
            deadline: Instant::now().checked_add(wall),
        }
    }

    /// Charges one fresh header read; `false` (nothing charged) once the
    /// bytes or the time ran out.
    pub fn spend_read(&mut self) -> bool {
        let late = self.deadline.is_some_and(|d| Instant::now() >= d);
        if self.bytes_left < LIBRARY_READ_COST || late {
            self.bytes_left = 0;
            return false;
        }
        self.bytes_left -= LIBRARY_READ_COST;
        true
    }
}

/// A base found in a target system folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryBase {
    /// Absolute: the game folder, or the flat file.
    pub path: PathBuf,
    /// A directory at the top of the system folder (a game folder or a
    /// folder game), not a flat file.
    pub in_game_folder: bool,
    /// The entry's name as shown (folder name, file stem).
    pub title: Option<String>,
    /// The single region its name tags (`(USA)`), if exactly one.
    pub region: Option<Region>,
}

/// The top-level bases of one target system folder.
#[derive(Clone, Debug, Default)]
pub struct LibraryIndex {
    ids: HashMap<String, LibraryBase>,
    /// Every base by title key, in bytes order of the name.
    keys: HashMap<String, Vec<LibraryBase>>,
    /// The budget ran out before every header was read: a title id not
    /// found may still be there (never guessed, D13).
    pub incomplete: bool,
}

impl LibraryIndex {
    /// The base whose header gives `base_id` (upper-case compared).
    pub fn by_title_id(&self, base_id: &str) -> Option<&LibraryBase> {
        self.ids.get(&base_id.trim().to_ascii_uppercase())
    }

    /// The base whose name has the title key `key` ([`parse_tags`]):
    /// of several, the first by the D20 order ([`pick_by_region`]).
    pub fn by_title_key(&self, key: &str) -> Option<&LibraryBase> {
        pick_by_region(
            self.by_title_key_all(key),
            None,
            |b| b.region,
            |b| b.title.as_deref().unwrap_or(""),
        )
    }

    /// Every base whose name has the title key `key`.
    pub fn by_title_key_all(&self, key: &str) -> &[LibraryBase] {
        self.keys.get(key).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Where a region falls in the preference order (mods spec D20): World,
/// USA, Europe, Japan, then every other region.
fn region_rank(region: Option<Region>) -> u8 {
    match region {
        Some(Region::World) => 1,
        Some(Region::Usa) => 2,
        Some(Region::Europe) => 3,
        Some(Region::Japan) => 4,
        _ => 5,
    }
}

/// The base a demo links to among `cands` (mods spec D20): one whose
/// region is `want`; else, by preference, World, USA, Europe, Japan; else
/// the rest. Ties go by name (bytes), so the pick is deterministic.
pub fn pick_by_region<T>(
    cands: &[T],
    want: Option<Region>,
    region: impl Fn(&T) -> Option<Region>,
    name: impl Fn(&T) -> &str,
) -> Option<&T> {
    let rank = |c: &T| {
        let r = region(c);
        if want.is_some() && r == want {
            0
        } else {
            region_rank(r)
        }
    };
    cands
        .iter()
        .min_by(|a, b| (rank(a), name(a).as_bytes()).cmp(&(rank(b), name(b).as_bytes())))
}

/// The single region a name tags (`Game (USA)`); `None` for none or
/// several (`(USA, Europe)`).
pub fn single_name_region(name: &str) -> Option<Region> {
    match parse_tags(name).regions.as_slice() {
        [r] => Some(*r),
        _ => None,
    }
}

/// A `(…)` group naming a demo (game folders spec D12, D25): `Demo`,
/// `Demo 2`, `Trial`, `Kiosk Demo`; `(Kiosk)` alone is not one.
fn is_demo_group(group: &str) -> bool {
    let lower = group.trim().to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    match words.as_slice() {
        ["demo" | "trial", rest @ ..] => {
            rest.is_empty() || (rest.len() == 1 && rest[0].chars().all(|c| c.is_ascii_digit()))
        }
        ["kiosk", "demo"] => true,
        _ => false,
    }
}

/// The `(…)` groups of `name`, in order.
fn paren_groups(name: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, ch) in name.char_indices() {
        match ch {
            '(' => start = Some(i + 1),
            ')' => {
                if let Some(s) = start.take() {
                    out.push(&name[s..i]);
                }
            }
            _ => {}
        }
    }
    out
}

/// `name` carries a demo tag (`(Demo)`, `(Trial)`, `(Kiosk Demo)`).
pub fn has_demo_tag(name: &str) -> bool {
    paren_groups(name).into_iter().any(is_demo_group)
}

/// The title key a demo and its base share (D12): [`parse_tags`]'s key of
/// `name` without its demo tags.
pub fn demo_free_key(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')').map(|c| open + c) else {
            break;
        };
        out.push_str(&rest[..open]);
        let group = &rest[open + 1..close];
        if !is_demo_group(group) {
            out.push_str(&rest[open..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    parse_tags(&out).title_key
}

/// A file of `system` by its extension (lower-case compared).
fn has_system_ext(name: &str, exts: &[String]) -> bool {
    name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && exts.iter().any(|e| e.eq_ignore_ascii_case(ext))
    })
}

/// `name` without its last extension.
fn stem(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// The title id of a base header, upper-case.
fn base_id(c: &ConsoleId) -> Option<String> {
    (link_kind(c) == TitleKind::Base)
        .then(|| base_title_id(c))
        .flatten()
        .map(|id| id.trim().to_ascii_uppercase())
}

/// Indexes the top-level entries of `system_dir` (absolute) for `system`:
/// every directory and every file with one of the system's extensions by
/// its name's title key (demo-tagged names are no bases), and, when
/// `read_ids`, by the base title id `read` gives for a flat file or for
/// the single file of the system at a directory's root. `read` answers
/// `None` when the budget ran out: no header is read after that and the
/// index is `incomplete`. Hidden entries, links and key files are skipped
/// by name and metadata only.
pub fn library_bases(
    system_dir: &Path,
    system: &str,
    db: &SystemDb,
    read_ids: bool,
    read: &mut dyn FnMut(&Path, &fs::Metadata) -> Option<Option<ConsoleId>>,
) -> LibraryIndex {
    let mut index = LibraryIndex::default();
    let exts: Vec<String> = db
        .get(system)
        .map(|s| s.extensions.clone())
        .unwrap_or_default();
    let Ok(listing) = fs::read_dir(system_dir) else {
        return index;
    };
    let mut entries: Vec<(String, PathBuf, fs::Metadata)> = listing
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let meta = fs::symlink_metadata(e.path()).ok()?;
            Some((name, e.path(), meta))
        })
        .filter(|(name, _, meta)| {
            !name.starts_with('.') && !meta.file_type().is_symlink() && !is_key_material(name)
        })
        .collect();
    // Bytes order: the same index on every platform.
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    // A spent budget (`incomplete`) stops every later read.
    let mut read_one =
        |path: &Path, meta: &fs::Metadata, index: &mut LibraryIndex| match read(path, meta) {
            Some(console) => console.as_ref().and_then(base_id),
            None => {
                index.incomplete = true;
                None
            }
        };
    for (name, path, meta) in entries {
        let (shown, dir) = if meta.is_dir() {
            (name.clone(), true)
        } else if meta.is_file() && has_system_ext(&name, &exts) {
            (stem(&name).to_string(), false)
        } else {
            continue;
        };
        let base = LibraryBase {
            path: path.clone(),
            in_game_folder: dir,
            title: Some(shown.clone()),
            region: single_name_region(&shown),
        };
        if !has_demo_tag(&shown) {
            let key = parse_tags(&shown).title_key;
            if !key.is_empty() {
                index.keys.entry(key).or_default().push(base.clone());
            }
        }
        let id = if !read_ids || index.incomplete {
            // Names only (demos), or the budget ran out: no header, so no
            // listing of the folder either.
            None
        } else if dir {
            // The one file of the system at the folder's root.
            let roots: Vec<(PathBuf, fs::Metadata)> = fs::read_dir(&path)
                .map(|l| {
                    l.filter_map(Result::ok)
                        .filter_map(|e| {
                            let n = e.file_name().to_string_lossy().into_owned();
                            let m = fs::symlink_metadata(e.path()).ok()?;
                            (m.is_file()
                                && !n.starts_with('.')
                                && !is_key_material(&n)
                                && has_system_ext(&n, &exts))
                            .then(|| (e.path(), m))
                        })
                        .collect()
                })
                .unwrap_or_default();
            match roots.as_slice() {
                [(file, m)] => read_one(file, m, &mut index),
                _ => None,
            }
        } else {
            read_one(&path, &meta, &mut index)
        };
        if let Some(id) = id {
            index.ids.entry(id).or_insert(base);
        }
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_tags_and_keys() {
        assert!(has_demo_tag("Resistance (USA) (Demo)"));
        assert!(has_demo_tag("Resistance (USA) (Kiosk Demo)"));
        assert!(has_demo_tag("Halo (Trial)"));
        assert!(has_demo_tag("Halo (Demo 2)"));
        assert!(!has_demo_tag("Halo (Kiosk)"));
        assert!(!has_demo_tag("Demolition (USA)"));
        assert_eq!(
            demo_free_key("Resistance (USA) (Demo)"),
            parse_tags("Resistance (USA)").title_key
        );
        assert_eq!(demo_free_key("Halo (Trial)"), parse_tags("Halo").title_key);
    }

    #[test]
    fn pick_by_region_prefers_the_match_then_world_usa_europe_japan() {
        use Region::*;
        type C = (&'static str, Option<Region>);
        let pick = |v: &[C], w| pick_by_region(v, w, |x| x.1, |x| x.0).map(|x| x.0);
        let bases: Vec<C> = vec![
            ("Game (Japan)", Some(Japan)),
            ("Game (USA)", Some(Usa)),
            ("Game (Europe)", Some(Europe)),
        ];
        assert_eq!(pick(&bases, Some(Europe)), Some("Game (Europe)"));
        assert_eq!(pick(&bases, None), Some("Game (USA)"));
        assert_eq!(pick(&bases, Some(Korea)), Some("Game (USA)"));
        assert_eq!(
            pick(
                &[("Game (Japan)", Some(Japan)), ("Game (World)", Some(World))],
                None
            ),
            Some("Game (World)")
        );
        assert_eq!(
            pick(
                &[
                    ("Game (Spain)", Some(Spain)),
                    ("Game (France)", Some(France))
                ],
                None
            ),
            Some("Game (France)")
        );
        // Past the four, the rest (no region too) by name.
        assert_eq!(
            pick(&[("Game (Korea)", Some(Korea)), ("Game", None)], None),
            Some("Game")
        );
        assert_eq!(pick(&[], None), None);
    }

    #[test]
    fn single_name_region_needs_exactly_one() {
        assert_eq!(single_name_region("Game (USA)"), Some(Region::Usa));
        assert_eq!(single_name_region("Game (USA, Europe)"), None);
        assert_eq!(single_name_region("Game"), None);
    }

    #[test]
    fn the_budget_stops_fresh_reads() {
        let mut b = LookupBudget::new(LIBRARY_READ_COST, Duration::from_secs(60));
        assert!(b.spend_read());
        assert!(!b.spend_read());
        let mut none = LookupBudget::new(0, Duration::from_secs(60));
        assert!(!none.spend_read());
        // A wall time past what an `Instant` holds is no deadline, not a
        // panic.
        let mut endless = LookupBudget::new(LIBRARY_READ_COST, Duration::MAX);
        assert!(endless.spend_read());
    }
}
