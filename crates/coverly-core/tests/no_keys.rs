//! Key-handling guard (adjustments spec D1): no shipped source names a
//! console key file outside the `is_key_material` name list, and no type
//! of the Switch catalogue has a field that could hold a key or a rights
//! id.
//!
//! The scan walks `crates/*/src`, `src-tauri/src` and `src/` by path
//! component (the Windows CI leg), drops each Rust file's test module
//! (from its `mod tests` line, with the `#[cfg(test)]` above it, to the
//! end of the file: the repo convention) and skips `*.test.ts(x)` files.
//! `src/ipc/mock` is skipped too: it is the browser-only fixture backend
//! of `pnpm dev`, whose pile shows that key files are left untouched.

use std::fs;
use std::path::{Component, Path, PathBuf};

/// Names of console key files, compared without case.
const NEEDLES: &[&str] = &[
    "prod.keys",
    "title.keys",
    "keys.txt",
    "aes_keys.txt",
    "seeddb.bin",
    "boot9",
];

/// Field names a catalogue type must never have.
const FORBIDDEN_FIELDS: &[&str] = &["key", "rights_id", "rightsId", "title_key", "titleKey"];

/// Directory names never walked.
const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", "dist"];

/// Folders (as path components below the repo root) never walked:
/// generated icons and the mock fixture backend.
const SKIP_PATHS: &[&[&str]] = &[&["src", "assets", "system-icons"], &["src", "ipc", "mock"]];

/// Text files the scan reads.
const TEXT_EXTS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "json", "html", "css",
];

/// The file that holds the one allowed list, below the repo root.
const KEY_LIST_FILE: &[&str] = &["crates", "coverly-core", "src", "consoles", "mod.rs"];

/// The catalogue module, below the repo root.
const CATALOG_DIR: &[&str] = &["crates", "coverly-core", "src", "catalog"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The normal components of `path` below `root`, as text.
fn parts(root: &Path, path: &Path) -> Vec<String> {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

fn is_path(parts: &[String], want: &[&str]) -> bool {
    parts.len() == want.len() && parts.iter().zip(want).all(|(a, b)| a == b)
}

fn starts_with(parts: &[String], prefix: &[&str]) -> bool {
    parts.len() >= prefix.len() && parts.iter().zip(prefix).all(|(a, b)| a == b)
}

/// The roots the scan walks below `root`.
fn scan_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(crates) = fs::read_dir(root.join("crates")) {
        for c in crates.flatten() {
            let src = c.path().join("src");
            if src.is_dir() {
                roots.push(src);
            }
        }
    }
    for extra in [root.join("src-tauri").join("src"), root.join("src")] {
        if extra.is_dir() {
            roots.push(extra);
        }
    }
    roots.sort();
    roots
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            let rel = parts(root, &path);
            if SKIP_DIRS.contains(&name.as_str()) || SKIP_PATHS.iter().any(|s| is_path(&rel, s)) {
                continue;
            }
            walk(root, &path, out);
        } else {
            let ext = path
                .extension()
                .map(|x| x.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let test_file = name.ends_with(".test.ts") || name.ends_with(".test.tsx");
            if TEXT_EXTS.contains(&ext.as_str()) && !test_file {
                out.push(path);
            }
        }
    }
}

/// The lines of a Rust file before its test module.
fn without_tests(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(at) = lines.iter().position(|l| {
        let t = l.trim_start();
        (t.starts_with("mod tests") || t.starts_with("mod unix_tests"))
            && t.trim_end().ends_with('{')
    }) else {
        return lines;
    };
    let mut start = at;
    while start > 0 && lines[start - 1].trim_start().starts_with("#[") {
        start -= 1;
    }
    lines[..start].to_vec()
}

/// The 0-based line range `[first, last]` of the allowed list in the key
/// list file: from `const KEY_NAMES` to the brace closing
/// `is_key_material`.
fn allowed_lines(lines: &[&str]) -> Option<(usize, usize)> {
    let first = lines
        .iter()
        .position(|l| l.starts_with("const KEY_NAMES"))?;
    let func = first
        + lines[first..]
            .iter()
            .position(|l| l.starts_with("pub fn is_key_material"))?;
    let last = func + lines[func..].iter().position(|l| *l == "}")?;
    Some((first, last))
}

/// Field name of a struct field line (`pub(crate) name: Type,`).
fn field_name(line: &str) -> Option<&str> {
    let mut t = line.trim_start();
    if t.starts_with("//") || t.starts_with("#[") {
        return None;
    }
    if let Some(rest) = t.strip_prefix("pub") {
        t = rest.trim_start();
        if t.starts_with('(') {
            t = t[t.find(')')? + 1..].trim_start();
        }
    }
    let (name, _) = t.split_once(':')?;
    let name = name.trim().trim_start_matches("r#");
    (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')).then_some(name)
}

/// Forbidden field names (or serde renames to them) inside the struct
/// bodies of `lines`.
fn forbidden_fields(lines: &[&str]) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut depth: Option<i32> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        let opens_struct = depth.is_none()
            && (t.starts_with("struct ") || (t.starts_with("pub") && t.contains(" struct ")))
            && t.contains('{');
        if opens_struct {
            depth = Some(0);
        }
        let Some(d) = depth.as_mut() else {
            continue;
        };
        if !opens_struct && *d == 1 {
            if let Some(name) = field_name(line) {
                if FORBIDDEN_FIELDS.contains(&name) {
                    found.push((i, name.to_string()));
                }
            }
            for f in FORBIDDEN_FIELDS {
                for attr in ["rename", "alias"] {
                    if t.starts_with("#[serde")
                        && t.contains(attr)
                        && t.contains(&format!("\"{f}\""))
                    {
                        found.push((i, (*f).to_string()));
                    }
                }
            }
        }
        *d += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        if *d <= 0 {
            depth = None;
        }
    }
    found
}

/// Every breach of D1 below `root`, as `path:line: what`.
fn scan(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for r in scan_roots(root) {
        walk(root, &r, &mut files);
    }
    files.sort();
    let mut breaches = Vec::new();
    for path in files {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let rel = parts(root, &path);
        let shown = rel.join("/");
        let is_rust = rel.last().is_some_and(|n| n.ends_with(".rs"));
        let lines = if is_rust {
            without_tests(&text)
        } else {
            text.lines().collect()
        };
        let allowed = is_path(&rel, KEY_LIST_FILE)
            .then(|| allowed_lines(&lines))
            .flatten();
        for (i, line) in lines.iter().enumerate() {
            if allowed.is_some_and(|(a, b)| (a..=b).contains(&i)) {
                continue;
            }
            let lower = line.to_ascii_lowercase();
            for needle in NEEDLES {
                if lower.contains(needle) {
                    breaches.push(format!("{shown}:{}: names {needle}", i + 1));
                }
            }
        }
        if is_rust && starts_with(&rel, CATALOG_DIR) {
            for (i, name) in forbidden_fields(&lines) {
                breaches.push(format!("{shown}:{}: catalogue field {name}", i + 1));
            }
        }
    }
    breaches.sort();
    breaches
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn the_tree_names_no_key_file_and_keeps_no_key_field() {
    let breaches = scan(&repo_root());
    assert!(breaches.is_empty(), "{breaches:#?}");
}

#[test]
fn the_allowed_list_is_where_the_guard_expects_it() {
    // Without it, the key names of `is_key_material` would breach the
    // scan; a moved list must move the guard too.
    let root = repo_root();
    let path = KEY_LIST_FILE.iter().fold(root, |p, c| p.join(c));
    let text = fs::read_to_string(path).unwrap();
    let lines = without_tests(&text);
    let (first, last) = allowed_lines(&lines).expect("KEY_NAMES and is_key_material");
    let region = lines[first..=last].join("\n");
    assert!(region.contains("\"prod.keys\"") && region.contains("pub fn is_key_material"));
    // The walk found the roots of every shipped source.
    let roots: Vec<Vec<String>> = scan_roots(&repo_root())
        .iter()
        .map(|r| parts(&repo_root(), r))
        .collect();
    for want in [
        &["crates", "coverly-core", "src"][..],
        &["crates", "coverly-cli", "src"],
        &["src-tauri", "src"],
        &["src"],
    ] {
        assert!(
            roots.iter().any(|r| is_path(r, want)),
            "{want:?} in {roots:?}"
        );
    }
}

#[test]
fn an_injected_key_name_or_field_is_found() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // Clean tree: nothing found.
    write(&root.join("crates/a/src/lib.rs"), "pub fn f() {}\n");
    assert_eq!(scan(root), Vec::<String>::new());

    // A key name in shipped Rust, front-end and Tauri sources.
    write(
        &root.join("crates/a/src/lib.rs"),
        "pub const P: &str = \"prod.keys\";\n#[cfg(test)]\nmod tests {\n    const T: &str = \"title.keys\";\n}\n",
    );
    write(
        &root.join("src/lib/x.ts"),
        "export const s = 'SeedDB.bin';\n",
    );
    write(&root.join("src-tauri/src/main.rs"), "// boot9 dump\n");
    // Tests, test files, the mock backend and node_modules are not shipped.
    write(&root.join("src/lib/x.test.ts"), "'aes_keys.txt'\n");
    write(&root.join("src/ipc/mock/m.ts"), "'prod.keys'\n");
    write(&root.join("src/node_modules/y/i.js"), "'prod.keys'\n");
    // A catalogue type with a key or rights-id field.
    write(
        &root.join("crates/coverly-core/src/catalog/store.rs"),
        "pub(crate) struct Row {\n    pub id: u64,\n    pub(crate) rights_id: String,\n}\nstruct Raw {\n    #[serde(rename = \"key\")]\n    k: String,\n    key: Option<String>,\n}\nenum Field { Key }\n",
    );
    let found = scan(root);
    assert_eq!(
        found,
        vec![
            "crates/a/src/lib.rs:1: names prod.keys".to_string(),
            "crates/coverly-core/src/catalog/store.rs:3: catalogue field rights_id".into(),
            "crates/coverly-core/src/catalog/store.rs:6: catalogue field key".into(),
            "crates/coverly-core/src/catalog/store.rs:8: catalogue field key".into(),
            "src-tauri/src/main.rs:1: names boot9".into(),
            "src/lib/x.ts:1: names seeddb.bin".into(),
        ],
    );
}

#[test]
fn the_key_list_file_is_allowed_only_inside_its_list() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let path = KEY_LIST_FILE
        .iter()
        .fold(root.to_path_buf(), |p, c| p.join(c));
    write(
        &path,
        "/// Names.\nconst KEY_NAMES: &[&str] = &[\"prod.keys\"];\n\n/// `keys.txt` is absent.\npub fn is_key_material(n: &str) -> bool {\n    KEY_NAMES.contains(&n)\n}\n\npub const LATER: &str = \"title.keys\";\n",
    );
    assert_eq!(
        scan(root),
        vec!["crates/coverly-core/src/consoles/mod.rs:9: names title.keys".to_string()]
    );
}
