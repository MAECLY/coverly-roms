//! Multi-file sets: `.cue` sheets, `.m3u` playlists and `.gdi` track lists.
//!
//! A list file names other files (tracks, discs). Those become companions
//! of the list file and are not games of their own: an `.m3u` listing two
//! `.cue` sheets is one game whose companions are both sheets and their
//! tracks.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use super::classify::extension;
use super::ScanWarning;

/// List files larger than this are not text sheets; only this much is read.
const MAX_LIST_BYTES: u64 = 1024 * 1024;

/// Whether files with extension `ext` (lowercase) list other files.
pub(crate) fn is_list_extension(ext: &str) -> bool {
    matches!(ext, "cue" | "m3u" | "gdi")
}

/// File names from the `FILE` lines of a cue sheet.
///
/// Accepts quoted (`FILE "a b.bin" BINARY`) and unquoted
/// (`FILE a b.bin BINARY`) names; the last word of an unquoted line is the
/// file type.
pub(crate) fn parse_cue(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim().trim_start_matches('\u{feff}');
            let keyword = line.get(..4)?;
            let rest = line.get(4..)?;
            if !keyword.eq_ignore_ascii_case("FILE") || !rest.starts_with(char::is_whitespace) {
                return None;
            }
            let rest = rest.trim_start();
            if let Some(quoted) = rest.strip_prefix('"') {
                return quoted.split('"').next().map(str::to_string);
            }
            let words: Vec<&str> = rest.split_whitespace().collect();
            match words.len() {
                0 => None,
                1 => Some(words[0].to_string()),
                n => Some(words[..n - 1].join(" ")),
            }
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// Paths listed in an `.m3u` playlist (blank lines and `#` comments
/// skipped).
pub(crate) fn parse_m3u(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim().trim_start_matches('\u{feff}').trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Track file names of a `.gdi` file.
///
/// The first line is the track count; each track line is
/// `number lba type sector_size file_name offset`, with the file name
/// optionally quoted.
pub(crate) fn parse_gdi(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim().trim_start_matches('\u{feff}').trim())
        .filter(|line| !line.is_empty())
        .skip(1)
        .filter_map(|line| tokenize(line).into_iter().nth(4))
        .filter(|name| !name.is_empty())
        .collect()
}

/// Splits on whitespace, keeping `"quoted words"` together.
fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut in_token = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                in_token = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            c => {
                current.push(c);
                in_token = true;
            }
        }
    }
    if in_token {
        tokens.push(current);
    }
    tokens
}

/// Groups ROM candidates into games.
///
/// Every candidate that is a list file gets the files it names,
/// transitively (an `.m3u` -> its `.cue` sheets -> their tracks), as
/// companions in listing order. Candidates named by another candidate's
/// list are dropped. A named file that does not exist is a warning on the
/// list file.
pub(crate) fn group_sets(
    candidates: Vec<PathBuf>,
    warnings: &mut Vec<ScanWarning>,
) -> Vec<(PathBuf, Vec<PathBuf>)> {
    let mut sets = Sets::default();
    let closures: Vec<Vec<PathBuf>> = candidates
        .iter()
        .map(|c| sets.closure(c, warnings))
        .collect();
    let keys: Vec<String> = candidates.iter().map(|c| key(c)).collect();
    let closure_keys: Vec<HashSet<String>> = closures
        .iter()
        .map(|cl| cl.iter().map(|p| key(p)).collect())
        .collect();

    // `j` is a companion when some other candidate `i` names it, unless `j`
    // also names `i` (a cycle: keep both rather than lose a game).
    let is_companion = |j: usize| {
        (0..keys.len()).any(|i| {
            i != j && closure_keys[i].contains(&keys[j]) && !closure_keys[j].contains(&keys[i])
        })
    };
    let companion: Vec<bool> = (0..keys.len()).map(is_companion).collect();

    candidates
        .into_iter()
        .zip(closures)
        .zip(companion)
        .filter(|(_, is_companion)| !is_companion)
        .map(|(set, _)| set)
        .collect()
}

/// Comparison key of a path: its lowercase text.
fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// Parsed list files and directory listings, shared by one system scan.
#[derive(Default)]
struct Sets {
    /// Direct references of each list file, by [`key`].
    refs: HashMap<String, Vec<PathBuf>>,
    /// Entry names of each directory looked into.
    listings: HashMap<PathBuf, Vec<OsString>>,
}

impl Sets {
    /// Every file `path` names, transitively, depth-first in listing order,
    /// without `path` itself and without repeats.
    fn closure(&mut self, path: &Path, warnings: &mut Vec<ScanWarning>) -> Vec<PathBuf> {
        let mut seen = HashSet::from([key(path)]);
        let mut out = Vec::new();
        self.visit(path, &mut seen, &mut out, warnings);
        out
    }

    fn visit(
        &mut self,
        path: &Path,
        seen: &mut HashSet<String>,
        out: &mut Vec<PathBuf>,
        warnings: &mut Vec<ScanWarning>,
    ) {
        for child in self.direct_refs(path, warnings) {
            if seen.insert(key(&child)) {
                out.push(child.clone());
                self.visit(&child, seen, out, warnings);
            }
        }
    }

    /// The existing files a list file names directly (empty for other
    /// files). Parsed once; warnings are emitted once.
    fn direct_refs(&mut self, path: &Path, warnings: &mut Vec<ScanWarning>) -> Vec<PathBuf> {
        let ext = extension(path);
        if !is_list_extension(&ext) {
            return Vec::new();
        }
        if let Some(refs) = self.refs.get(&key(path)) {
            return refs.clone();
        }
        let names = match read_text(path) {
            Ok(text) => match ext.as_str() {
                "cue" => parse_cue(&text),
                "m3u" => parse_m3u(&text),
                _ => parse_gdi(&text),
            },
            Err(err) => {
                warnings.push(ScanWarning {
                    path: path.to_path_buf(),
                    message: format!("cannot read: {err}"),
                });
                Vec::new()
            }
        };
        let base = path.parent().unwrap_or(Path::new(""));
        let mut refs = Vec::new();
        for name in names {
            match self.resolve(base, &name) {
                Some(found) => refs.push(found),
                None => warnings.push(ScanWarning {
                    path: path.to_path_buf(),
                    message: format!("referenced file `{name}` not found"),
                }),
            }
        }
        self.refs.insert(key(path), refs.clone());
        refs
    }

    /// Finds the file `reference` (relative to `base`, `/` or `\`
    /// separated) on disk, matching each component case-insensitively and
    /// returning the on-disk spelling.
    fn resolve(&mut self, base: &Path, reference: &str) -> Option<PathBuf> {
        let reference = reference.trim().replace('\\', "/");
        let reference = Path::new(&reference);
        let mut current = if reference.is_absolute() {
            PathBuf::new()
        } else {
            base.to_path_buf()
        };
        for component in reference.components() {
            match component {
                Component::Normal(name) => {
                    let found = self.find_in_dir(&current, name)?;
                    current.push(found);
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    current.pop();
                }
                Component::RootDir | Component::Prefix(_) => current.push(component),
            }
        }
        fs::metadata(&current)
            .ok()
            .filter(|m| m.is_file())
            .map(|_| current)
    }

    /// The entry of `dir` named `name`: exact spelling first, then any
    /// case.
    fn find_in_dir(&mut self, dir: &Path, name: &std::ffi::OsStr) -> Option<OsString> {
        let listing = self.listings.entry(dir.to_path_buf()).or_insert_with(|| {
            fs::read_dir(dir)
                .map(|entries| entries.flatten().map(|e| e.file_name()).collect())
                .unwrap_or_default()
        });
        if let Some(exact) = listing.iter().find(|n| n.as_os_str() == name) {
            return Some(exact.clone());
        }
        let wanted = name.to_string_lossy().to_lowercase();
        listing
            .iter()
            .find(|n| n.to_string_lossy().to_lowercase() == wanted)
            .cloned()
    }
}

/// Reads at most [`MAX_LIST_BYTES`] of a list file as (lossy) UTF-8.
fn read_text(path: &Path) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_LIST_BYTES)
        .read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_file_lines() {
        let cue =
            "\u{feff}REM comment\nFILE \"Game (Track 1).bin\" BINARY\n  TRACK 01 MODE1/2352\n\
                   file Game (Track 2).bin BINARY\nFILE single.wav\nFILENAME nope\n";
        assert_eq!(
            parse_cue(cue),
            ["Game (Track 1).bin", "Game (Track 2).bin", "single.wav"]
        );
    }

    #[test]
    fn m3u_lines() {
        let m3u = "#EXTM3U\n\nDisc 1.cue\r\n  .hidden/Disc 2.cue  \n# comment\n";
        assert_eq!(parse_m3u(m3u), ["Disc 1.cue", ".hidden/Disc 2.cue"]);
    }

    #[test]
    fn gdi_track_names_are_the_fifth_column() {
        let gdi = "3\r\n1 0 4 2352 track01.bin 0\r\n2 756 0 2352 \"track 02.raw\" 0\r\n\
                   3 45000 4 2352 track03.bin 0\r\n";
        assert_eq!(
            parse_gdi(gdi),
            ["track01.bin", "track 02.raw", "track03.bin"]
        );
    }

    #[test]
    fn list_extensions() {
        assert!(is_list_extension("cue"));
        assert!(is_list_extension("m3u"));
        assert!(is_list_extension("gdi"));
        assert!(!is_list_extension("chd"));
    }

    #[test]
    fn references_resolve_case_insensitively_and_through_subfolders() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".discs")).unwrap();
        fs::write(dir.path().join(".discs/Disc 1.CUE"), "").unwrap();
        let mut sets = Sets::default();
        assert_eq!(
            sets.resolve(dir.path(), ".discs\\disc 1.cue"),
            Some(dir.path().join(".discs/Disc 1.CUE"))
        );
        assert_eq!(
            sets.resolve(dir.path(), "./.discs/../.discs/Disc 1.CUE"),
            Some(dir.path().join(".discs/Disc 1.CUE"))
        );
        assert_eq!(sets.resolve(dir.path(), "missing.cue"), None);
        assert_eq!(
            sets.resolve(dir.path(), ".discs"),
            None,
            "folders are not files"
        );
    }

    #[test]
    fn cycles_keep_both_games() {
        let dir = tempfile::TempDir::new().unwrap();
        let a = dir.path().join("a.m3u");
        let b = dir.path().join("b.m3u");
        fs::write(&a, "b.m3u\n").unwrap();
        fs::write(&b, "a.m3u\n").unwrap();
        let mut warnings = Vec::new();
        let games = group_sets(vec![a.clone(), b.clone()], &mut warnings);
        assert!(warnings.is_empty());
        let primaries: Vec<&PathBuf> = games.iter().map(|(p, _)| p).collect();
        assert_eq!(primaries, [&a, &b]);
    }
}
