//! Guessing a library's layout profile from marker folders and files.

use std::fs;
use std::path::{Path, PathBuf};

use super::builtin_profiles;
use crate::systems::SystemDb;

/// Profile suggested when `fallback` is not a built-in profile id.
const DEFAULT_FALLBACK: &str = "anbernic-stock";
/// Evidence label of the weak rule in [`detect_with_fallback`].
const SYSTEM_FOLDERS: &str = "system-folders";

/// One candidate profile for a library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    /// Id of the built-in profile.
    pub profile_id: String,
    /// Higher is more likely.
    pub score: u32,
    /// Human-readable markers that were found, e.g. `"anbernic/ folder"`.
    pub evidence: Vec<String>,
}

/// Scores every built-in profile against `library_root`, returning the
/// profiles with any evidence, sorted by score (highest first; ties keep
/// the [`builtin_profiles`] order).
///
/// Read-only and shallow: it lists the root, the system dirs (children of
/// the root and of any `roms` folder, case-insensitively) and their direct
/// children. Unreadable entries are ignored and AppleDouble `._*` twins are
/// never evidence.
pub fn detect(library_root: &Path) -> Vec<Detection> {
    let markers = Markers::probe(library_root);
    let mut found: Vec<Detection> = builtin_profiles()
        .iter()
        .filter_map(|p| {
            let rules = rules_for(&p.id);
            let mut score = 0;
            let mut evidence = Vec::new();
            for (marker, points, label) in rules {
                if markers.has(*marker) {
                    score += points;
                    evidence.push((*label).to_string());
                }
            }
            (score > 0).then(|| Detection {
                profile_id: p.id.clone(),
                score,
                evidence,
            })
        })
        .collect();
    found.sort_by_key(|d| std::cmp::Reverse(d.score));
    found
}

/// [`detect`], or, when no marker scored, a weak suggestion for a tree that
/// is already organised by system (e.g. freshly organised loose ROMs).
///
/// The weak rule fires when a `roms` child (any casing) holds at least one
/// folder that resolves to a system in `db` and directly holds a file, or
/// the root itself holds at least two such folders. It returns a single
/// `Detection { profile_id: fallback, score: 1, evidence: ["system-folders"] }`;
/// `fallback` must be a built-in profile id, otherwise `anbernic-stock` is
/// used. Hidden files (`.DS_Store`, AppleDouble `._*`) never fill a folder.
pub fn detect_with_fallback(library_root: &Path, fallback: &str, db: &SystemDb) -> Vec<Detection> {
    let found = detect(library_root);
    if !found.is_empty() || !organised_by_system(library_root, db) {
        return found;
    }
    let profile_id = if builtin_profiles().iter().any(|p| p.id == fallback) {
        fallback
    } else {
        DEFAULT_FALLBACK
    };
    vec![Detection {
        profile_id: profile_id.to_string(),
        score: 1,
        evidence: vec![SYSTEM_FOLDERS.to_string()],
    }]
}

/// Whether `root` looks organised by system (see [`detect_with_fallback`]).
fn organised_by_system(root: &Path, db: &SystemDb) -> bool {
    let root_entries = list(root);
    let filled_system_dirs = |entries: &[Entry]| {
        entries
            .iter()
            .filter(|e| e.is_dir && db.resolve_folder(&e.lower).is_some())
            .filter(|e| holds_a_file(&e.path))
            .count()
    };
    if let Some(roms) = find_dir(&root_entries, "roms") {
        if filled_system_dirs(&list(&roms)) >= 1 {
            return true;
        }
    }
    filled_system_dirs(&root_entries) >= 2
}

/// Whether `dir` directly holds a regular, non-hidden file.
fn holds_a_file(dir: &Path) -> bool {
    list(dir)
        .iter()
        .any(|e| !e.is_dir && !e.lower.starts_with('.') && e.path.is_file())
}

/// A marker folder or file looked for by [`detect`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Marker {
    /// `anbernic/` at the root.
    AnbernicDir,
    /// `.tmp_update/` at the root (Onion).
    TmpUpdateDir,
    /// `MUOS/info/catalogue/` from the root.
    MuosCatalogue,
    /// `.system/` at the root (MinUI / NextUI).
    SystemDotDir,
    /// `downloaded_media/` at the root (ES-DE).
    DownloadedMedia,
    /// `thumbnails/<playlist>/Named_Boxarts|Named_Snaps|Named_Titles/`.
    RetroarchThumbnails,
    /// `<SYS>/Imgs/`.
    ImgsDirs,
    /// `<SYS>/.media/`.
    MediaDirs,
    /// `<SYS>/images/`.
    ImagesDirs,
    /// `<SYS>/gamelist.xml`.
    Gamelists,
}

/// `(marker, points, evidence label)` rules of one built-in profile.
fn rules_for(profile_id: &str) -> &'static [(Marker, u32, &'static str)] {
    const IMGS: (Marker, u32, &str) = (Marker::ImgsDirs, 20, "Imgs folders");
    match profile_id {
        "anbernic-stock" => &[(Marker::AnbernicDir, 50, "anbernic/ folder"), IMGS],
        "garlicos" => &[IMGS],
        "onion" => &[(Marker::TmpUpdateDir, 50, ".tmp_update/ folder"), IMGS],
        "muos" => &[(Marker::MuosCatalogue, 60, "MUOS/info/catalogue/ folder")],
        "minui" => &[
            (Marker::SystemDotDir, 30, ".system/ folder"),
            (Marker::MediaDirs, 30, ".media folders"),
        ],
        "esde" => &[(Marker::DownloadedMedia, 60, "downloaded_media/ folder")],
        "batocera" => &[
            (Marker::Gamelists, 30, "gamelist.xml files"),
            (Marker::ImagesDirs, 30, "images folders"),
        ],
        "retroarch" => &[(
            Marker::RetroarchThumbnails,
            60,
            "thumbnails/<playlist>/Named_* folders",
        )],
        _ => &[],
    }
}

/// Markers found in one library.
#[derive(Default)]
struct Markers {
    found: Vec<Marker>,
}

impl Markers {
    fn probe(root: &Path) -> Markers {
        let mut m = Markers::default();
        let root_entries = list(root);
        let root_dir = |name: &str| find_dir(&root_entries, name);

        if root_dir("anbernic").is_some() {
            m.found.push(Marker::AnbernicDir);
        }
        if root_dir(".tmp_update").is_some() {
            m.found.push(Marker::TmpUpdateDir);
        }
        if root_dir(".system").is_some() {
            m.found.push(Marker::SystemDotDir);
        }
        if root_dir("downloaded_media").is_some() {
            m.found.push(Marker::DownloadedMedia);
        }
        if let Some(muos) = root_dir("muos") {
            let info = find_dir(&list(&muos), "info");
            if info.is_some_and(|info| find_dir(&list(&info), "catalogue").is_some()) {
                m.found.push(Marker::MuosCatalogue);
            }
        }
        if let Some(thumbs) = root_dir("thumbnails") {
            let named = list(&thumbs).iter().filter(|e| e.is_dir).any(|playlist| {
                let kinds = list(&playlist.path);
                ["named_boxarts", "named_snaps", "named_titles"]
                    .iter()
                    .any(|k| find_dir(&kinds, k).is_some())
            });
            if named {
                m.found.push(Marker::RetroarchThumbnails);
            }
        }

        // System dirs: children of the root and of any `roms` folder.
        let mut system_dirs: Vec<PathBuf> = Vec::new();
        for e in root_entries.iter().filter(|e| e.is_dir) {
            system_dirs.push(e.path.clone());
            if e.lower == "roms" {
                system_dirs.extend(
                    list(&e.path)
                        .into_iter()
                        .filter(|c| c.is_dir)
                        .map(|c| c.path),
                );
            }
        }
        for dir in &system_dirs {
            let children = list(dir);
            let checks = [
                (Marker::ImgsDirs, "imgs", true),
                (Marker::MediaDirs, ".media", true),
                (Marker::ImagesDirs, "images", true),
                (Marker::Gamelists, "gamelist.xml", false),
            ];
            for (marker, name, is_dir) in checks {
                if !m.has(marker)
                    && children
                        .iter()
                        .any(|c| c.is_dir == is_dir && c.lower == name)
                {
                    m.found.push(marker);
                }
            }
        }
        m
    }

    fn has(&self, marker: Marker) -> bool {
        self.found.contains(&marker)
    }
}

/// One directory entry, name lowercased.
struct Entry {
    path: PathBuf,
    lower: String,
    is_dir: bool,
}

/// Lists `dir`, skipping AppleDouble `._*` twins; unreadable → empty.
fn list(dir: &Path) -> Vec<Entry> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    read.filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("._") {
                return None;
            }
            let path = e.path();
            Some(Entry {
                is_dir: path.is_dir(),
                lower: name.to_lowercase(),
                path,
            })
        })
        .collect()
}

/// The sub-directory named `lower_name` (case-insensitively), if any.
fn find_dir(entries: &[Entry], lower_name: &str) -> Option<PathBuf> {
    entries
        .iter()
        .find(|e| e.is_dir && e.lower == lower_name)
        .map(|e| e.path.clone())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn tree(dirs: &[&str], files: &[&str]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        for d in dirs {
            fs::create_dir_all(tmp.path().join(d)).unwrap();
        }
        for f in files {
            let p = tmp.path().join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"").unwrap();
        }
        tmp
    }

    fn top(tmp: &TempDir) -> Detection {
        detect(tmp.path())
            .into_iter()
            .next()
            .expect("at least one detection")
    }

    #[test]
    fn empty_dir_detects_nothing() {
        let tmp = tree(&[], &[]);
        assert!(detect(tmp.path()).is_empty());
    }

    #[test]
    fn missing_dir_detects_nothing() {
        let tmp = tree(&[], &[]);
        assert!(detect(&tmp.path().join("nope")).is_empty());
    }

    #[test]
    fn anbernic_stock() {
        let tmp = tree(&["anbernic", "ROMS/SFC/Imgs", "ROMS/GBA"], &[]);
        let d = top(&tmp);
        assert_eq!(d.profile_id, "anbernic-stock");
        assert!(
            d.evidence.contains(&"anbernic/ folder".to_string()),
            "{d:?}"
        );
        assert!(d.evidence.contains(&"Imgs folders".to_string()), "{d:?}");
    }

    #[test]
    fn markers_are_case_insensitive() {
        let tmp = tree(&["Anbernic", "roms/sfc/imgs"], &[]);
        let d = top(&tmp);
        assert_eq!(d.profile_id, "anbernic-stock");
        assert_eq!(d.evidence.len(), 2, "{d:?}");
    }

    #[test]
    fn appledouble_twins_are_not_evidence() {
        let tmp = tree(&[], &["._anbernic", "ROMS/SFC/._Imgs"]);
        assert!(detect(tmp.path()).is_empty());
    }

    #[test]
    fn muos() {
        let tmp = tree(&["MUOS/info/catalogue/snes/box", "ROMS/SFC"], &[]);
        assert_eq!(top(&tmp).profile_id, "muos");
    }

    #[test]
    fn onion() {
        let tmp = tree(&[".tmp_update", "Roms/SFC/Imgs", "Roms/GBA/Imgs"], &[]);
        let d = top(&tmp);
        assert_eq!(d.profile_id, "onion");
        assert!(d.evidence.contains(&"Imgs folders".to_string()), "{d:?}");
    }

    #[test]
    fn batocera() {
        let tmp = tree(&["roms/snes/images"], &["roms/snes/gamelist.xml"]);
        let d = top(&tmp);
        assert_eq!(d.profile_id, "batocera");
        assert_eq!(d.evidence.len(), 2, "{d:?}");
    }

    #[test]
    fn esde() {
        let tmp = tree(&["downloaded_media/snes/covers", "ROMs/snes"], &[]);
        assert_eq!(top(&tmp).profile_id, "esde");
    }

    #[test]
    fn retroarch() {
        let tmp = tree(
            &["thumbnails/Nintendo - Super Nintendo Entertainment System/Named_Boxarts"],
            &[],
        );
        assert_eq!(top(&tmp).profile_id, "retroarch");
    }

    #[test]
    fn minui() {
        let tmp = tree(&[".system", "Roms/Super Nintendo (SFC)/.media"], &[]);
        assert_eq!(top(&tmp).profile_id, "minui");
    }

    #[test]
    fn sorted_by_score_descending() {
        let tmp = tree(&["anbernic", "ROMS/SFC/Imgs"], &[]);
        let all = detect(tmp.path());
        assert!(all.len() > 1, "Imgs also hints Garlic and Onion: {all:?}");
        assert!(all.windows(2).all(|w| w[0].score >= w[1].score), "{all:?}");
        assert!(all.iter().all(|d| d.score > 0 && !d.evidence.is_empty()));
    }

    #[test]
    fn a_freshly_organised_tree_is_suggested_with_the_fallback() {
        let tmp = tree(&["ROMS/GBA"], &["ROMS/GBA/a.gba"]);
        let found = detect_with_fallback(tmp.path(), "anbernic-stock", &SystemDb::builtin());
        assert_eq!(found[0].profile_id, "anbernic-stock");
        assert_eq!(found[0].evidence, ["system-folders"]);
        assert_eq!(found[0].score, 1);
    }

    #[test]
    fn bare_root_needs_two_system_folders_with_files() {
        let one = tree(&["gb"], &["gb/a.gb"]);
        assert!(detect_with_fallback(one.path(), "esde", &SystemDb::builtin()).is_empty());
        let two = tree(&["gb", "GBA"], &["gb/a.gb", "GBA/b.gba"]);
        assert_eq!(
            detect_with_fallback(two.path(), "esde", &SystemDb::builtin())[0].profile_id,
            "esde"
        );
    }

    #[test]
    fn empty_system_folders_are_not_enough() {
        let tmp = tree(&["ROMS/GBA", "ROMS/GB"], &[]);
        assert!(
            detect_with_fallback(tmp.path(), "anbernic-stock", &SystemDb::builtin()).is_empty()
        );
    }

    #[test]
    fn markers_win_over_the_weak_rule() {
        let tmp = tree(&["anbernic", "ROMS/GBA"], &["ROMS/GBA/a.gba"]);
        let found = detect_with_fallback(tmp.path(), "esde", &SystemDb::builtin());
        assert_eq!(found[0].profile_id, "anbernic-stock");
        assert!(found.iter().all(|d| d.evidence != ["system-folders"]));
    }

    #[test]
    fn an_unknown_fallback_suggests_anbernic_stock() {
        let tmp = tree(&["Roms/GBA"], &["Roms/GBA/a.gba"]);
        let found = detect_with_fallback(tmp.path(), "nope", &SystemDb::builtin());
        assert_eq!(found[0].profile_id, "anbernic-stock");
    }

    #[test]
    fn appledouble_and_hidden_files_do_not_fill_a_folder() {
        let tmp = tree(
            &["ROMS/GBA/sub"],
            &["ROMS/GBA/._a.gba", "ROMS/GBA/.DS_Store"],
        );
        assert!(
            detect_with_fallback(tmp.path(), "anbernic-stock", &SystemDb::builtin()).is_empty()
        );
    }

    #[test]
    fn folders_that_are_not_systems_do_not_count() {
        let tmp = tree(
            &["ROMS/Stuff", "Docs", "Music"],
            &["ROMS/Stuff/a.gba", "Docs/a.txt", "Music/b.mp3"],
        );
        assert!(
            detect_with_fallback(tmp.path(), "anbernic-stock", &SystemDb::builtin()).is_empty()
        );
    }
}
