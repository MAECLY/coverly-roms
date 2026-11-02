//! The installer carries every licence text Coverly must ship (adjustments
//! Task 22, `credits-inventory.md` §6): the whole `LICENSES/` folder (the
//! texts, its README and the generated third-party notices) and each font's
//! OFL next to them, as `bundle.resources` in `tauri.conf.json` maps them.
//!
//! Tauri walks a mapped directory recursively and keeps its structure under
//! the target; a mapped file lands at the target itself. This test resolves
//! the map the same way and compares paths by component (Windows CI).

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// A path as its normal components, so `a/b` and `a\b` compare equal.
fn parts(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

/// (source, bundle target) for every file `bundle.resources` ships.
fn resolved() -> Vec<(PathBuf, Vec<String>)> {
    let tauri = repo().join("src-tauri");
    let conf: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tauri.join("tauri.conf.json")).unwrap())
            .unwrap();
    let map = conf["bundle"]["resources"]
        .as_object()
        .expect("bundle.resources is a source → target map");
    let mut out = Vec::new();
    for (source, target) in map {
        let source = tauri.join(source);
        let target = Path::new(target.as_str().unwrap());
        assert!(
            source.exists(),
            "mapped resource {} does not exist",
            source.display()
        );
        if source.is_dir() {
            for file in files_under(&source) {
                let mut dest = parts(target);
                dest.extend(parts(file.strip_prefix(&source).unwrap()));
                out.push((file, dest));
            }
        } else {
            out.push((source, parts(target)));
        }
    }
    out
}

fn targets() -> BTreeSet<Vec<String>> {
    resolved().into_iter().map(|(_, t)| t).collect()
}

fn target(path: &str) -> Vec<String> {
    parts(Path::new(path))
}

#[test]
fn every_mapped_source_exists() {
    for (source, _) in resolved() {
        assert!(source.is_file(), "{} is not a file", source.display());
    }
}

#[test]
fn licences_folder_ships_whole() {
    let licences = repo().join("LICENSES");
    let shipped = targets();
    let files = files_under(&licences);
    for want in ["README.md", "generated/rust.txt", "generated/js.txt"] {
        assert!(
            files
                .iter()
                .any(|f| parts(f.strip_prefix(&licences).unwrap()) == target(want)),
            "LICENSES/{want} is missing from the repository"
        );
    }
    assert!(
        files
            .iter()
            .any(|f| f.extension().is_some_and(|e| e == "txt") && f.parent() == Some(&licences)),
        "LICENSES/ holds the licence texts"
    );
    for file in files {
        let mut dest = vec!["LICENSES".to_string()];
        dest.extend(parts(file.strip_prefix(&licences).unwrap()));
        assert!(
            shipped.contains(&dest),
            "{} is not bundled as {}",
            file.display(),
            dest.join("/")
        );
    }
}

#[test]
fn every_font_licence_ships() {
    let fonts = repo().join("src").join("assets").join("fonts");
    let shipped = targets();
    let ofl: Vec<_> = files_under(&fonts)
        .into_iter()
        .filter(|f| {
            let name = f.file_name().unwrap().to_string_lossy();
            name.starts_with("OFL-") && name.ends_with(".txt")
        })
        .collect();
    assert_eq!(ofl.len(), 3, "one OFL per bundled font family");
    for file in ofl {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let dest = vec!["LICENSES".to_string(), "fonts".to_string(), name];
        assert!(
            shipped.contains(&dest),
            "{} is not bundled as {}",
            file.display(),
            dest.join("/")
        );
    }
}

#[test]
fn licence_and_notices_still_ship() {
    let shipped = targets();
    assert!(shipped.contains(&target("LICENSE")));
    assert!(shipped.contains(&target("THIRD-PARTY-NOTICES.md")));
}

#[test]
fn release_checks_the_notices_before_building() {
    let wf = std::fs::read_to_string(repo().join(".github").join("workflows").join("release.yml"))
        .unwrap()
        .replace("\r\n", "\n");
    let check = wf
        .find("run: pnpm notices:check")
        .expect("the release app job runs pnpm notices:check");
    let build = wf.find("pnpm tauri build").expect("the app job builds");
    assert!(check < build, "notices:check runs before tauri build");
    assert!(
        !wf.contains("cargo about"),
        "the release never regenerates the notices"
    );
}
