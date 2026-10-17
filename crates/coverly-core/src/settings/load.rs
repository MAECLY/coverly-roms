//! Reading and writing `settings.json` (D24).

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde_json::Value;

use super::model::{MaxSize, RecentLibrary, Settings, SETTINGS_VERSION};
use super::validate::FieldErrorCode;

pub const SETTINGS_FILE: &str = "settings.json";
/// The M3 file holding ignored folders and recents (migrated once).
pub const LEGACY_LIBRARY_FILE: &str = "library.json";
/// Larger files are not settings.
pub const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;

/// What loading had to do, for a notice in the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoadNotice {
    /// `library.json` was turned into `settings.json`.
    Migrated,
    /// The file was not settings: defaults are used, the file set aside.
    ResetCorrupt,
    /// Some fields were wrong and fell back to their defaults.
    Fixed,
    /// Written by a newer version: what this version understands is used.
    FutureVersion,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadReport {
    pub notice: Option<LoadNotice>,
    /// The copy (or the set-aside file) holding what was on disk.
    pub backup: Option<PathBuf>,
    /// Dotted fields (`group.field`) that fell back to their defaults; a
    /// bare group name when the whole group did, `*` when everything did.
    pub fixed: Vec<String>,
}

enum Read_ {
    Missing,
    TooLarge,
    Unreadable,
    Text(String),
}

fn read_capped(path: &Path) -> Read_ {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Read_::Missing,
        Err(_) => return Read_::Unreadable,
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_SETTINGS_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Read_::Unreadable;
    }
    if bytes.len() as u64 > MAX_SETTINGS_BYTES {
        return Read_::TooLarge;
    }
    match String::from_utf8(bytes) {
        // A leading UTF-8 BOM (some editors add it) is not part of the JSON.
        Ok(text) => Read_::Text(match text.strip_prefix('\u{feff}') {
            Some(rest) => rest.to_string(),
            None => text,
        }),
        Err(_) => Read_::TooLarge,
    }
}

fn now() -> i64 {
    crate::time::now_unix()
}

/// `settings.json.<tag>-<unix>` (with `-<n>` if taken).
fn aside_path(path: &Path, tag: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = format!("{name}.{tag}-{}", now());
    let mut candidate = path.with_file_name(&base);
    let mut n = 1;
    while candidate.exists() {
        candidate = path.with_file_name(format!("{base}-{n}"));
        n += 1;
    }
    candidate
}

/// The `settings.json.<tag>-*` files in `dir`, sorted.
fn asides(dir: &Path, tag: &str) -> Vec<PathBuf> {
    let prefix = format!("{SETTINGS_FILE}.{tag}-");
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// Copies `settings.json` aside (or names an identical copy already there); `None` when there is no file.
pub fn backup_settings(dir: &Path, tag: &str) -> io::Result<Option<PathBuf>> {
    let path = dir.join(SETTINGS_FILE);
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    // The same content already copied aside (loading again): reuse it.
    if let Some(same) = asides(dir, tag).into_iter().find(|p| {
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.len() == bytes.len() as u64)
            && fs::read(p).is_ok_and(|b| b == bytes)
    }) {
        return Ok(Some(same));
    }
    let dest = aside_path(&path, tag);
    write_synced(&dest, &bytes)?;
    Ok(Some(dest))
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Writes `settings.json` in `dir` atomically (temp file, fsync, rename).
pub fn save_settings(dir: &Path, s: &Settings) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let mut text = serde_json::to_string_pretty(s).map_err(io::Error::other)?;
    text.push('\n');
    // A name of its own per save: concurrent saves never share a temp file.
    static SAVES: AtomicU64 = AtomicU64::new(0);
    let n = SAVES.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{SETTINGS_FILE}.{}-{n}.tmp", std::process::id()));
    if let Err(e) =
        write_synced(&tmp, text.as_bytes()).and_then(|()| fs::rename(&tmp, dir.join(SETTINGS_FILE)))
    {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

struct Parsed {
    settings: Settings,
    fixed: Vec<String>,
    future: bool,
}

fn get_path<'a>(v: &'a Value, dotted: &str) -> Option<&'a Value> {
    dotted.split('.').try_fold(v, |v, k| v.get(k))
}

/// Resets the group field `group.field` of `s` (only the first two parts
/// of `dotted` count) to its default.
fn reset_field(s: &mut Settings, dotted: &str) {
    let mut parts = dotted.splitn(3, '.');
    let (Some(group), Some(field)) = (parts.next(), parts.next()) else {
        return;
    };
    let defaults = serde_json::to_value(Settings::default()).expect("defaults serialise");
    let mut v = serde_json::to_value(&*s).expect("settings serialise");
    if let (Some(slot), Some(def)) = (
        v.get_mut(group).and_then(|g| g.get_mut(field)),
        get_path(&defaults, &format!("{group}.{field}")),
    ) {
        *slot = def.clone();
        if let Ok(next) = serde_json::from_value(v) {
            *s = next;
        }
    }
}

/// Field-level tolerant parse (see the module docs); `None` when `text` is
/// not a JSON object.
fn parse_tolerant(text: &str, known: &[String]) -> Option<Parsed> {
    let Ok(Value::Object(user)) = serde_json::from_str::<Value>(text) else {
        return None;
    };
    let future = user
        .get("version")
        .and_then(Value::as_u64)
        .is_some_and(|v| v > u64::from(SETTINGS_VERSION));
    let mut merged = serde_json::to_value(Settings::default()).expect("defaults serialise");
    let mut fixed = Vec::new();
    for (group, fields) in &user {
        if group == "version" {
            continue;
        }
        let Some(defaults) = merged.get(group).and_then(Value::as_object).cloned() else {
            continue; // an unknown group (newer version): ignored
        };
        let Value::Object(fields) = fields else {
            fixed.push(group.clone());
            continue;
        };
        for (field, value) in fields {
            if !defaults.contains_key(field) {
                continue; // an unknown field: ignored
            }
            let (value, dropped) = keep_good_entries(group, field, value);
            if dropped {
                fixed.push(format!("{group}.{field}"));
            }
            let before = merged[group.as_str()][field.as_str()].clone();
            merged[group.as_str()][field.as_str()] = value;
            if serde_json::from_value::<Settings>(merged.clone()).is_err() {
                merged[group.as_str()][field.as_str()] = before;
                fixed.push(format!("{group}.{field}"));
            }
        }
    }
    let mut settings: Settings = serde_json::from_value(merged).ok()?;
    // What can be cleaned (spelling, duplicates, unknown sources) is
    // cleaned before validating, so it is not reset.
    settings.normalise();
    let note = |fixed: &mut Vec<String>, key: &str| {
        if !fixed.iter().any(|f| f == key) {
            fixed.push(key.to_string());
        }
    };
    for _ in 0..3 {
        let errors = settings.validate(known);
        if errors.is_empty() {
            break;
        }
        for e in errors {
            // Cross-field rules reset every field of the rule, so the reset
            // itself cannot fail again.
            let fields: &[&str] = if e.code == FieldErrorCode::Order {
                if e.field.starts_with("organize.") {
                    &["organize.auto_threshold", "organize.review_threshold"]
                } else {
                    &["matching.auto_threshold", "matching.review_threshold"]
                }
            } else if e.field == "network.proxy_url" {
                &["network.proxy", "network.proxy_url"]
            } else {
                &[]
            };
            if let Some(id) = e.field.strip_prefix("art.max_size.") {
                // One bad entry: only that entry goes.
                let id = id.split('.').next().unwrap_or_default();
                settings.art.max_size.remove(id);
                note(&mut fixed, "art.max_size");
            } else if let Some(rest) = e.field.strip_prefix("organize.folder_names.") {
                // One bad folder (or every folder of an unknown layout).
                let mut parts = rest.splitn(2, '.');
                let profile = parts.next().unwrap_or_default().to_string();
                let names = &mut settings.organize.folder_names;
                match parts.next() {
                    Some(system) => {
                        if let Some(map) = names.get_mut(&profile) {
                            map.remove(system);
                            if map.is_empty() {
                                names.remove(&profile);
                            }
                        }
                    }
                    None => {
                        names.remove(&profile);
                    }
                }
                note(&mut fixed, "organize.folder_names");
            } else if let Some(profile) = e.field.strip_prefix("organize.arcade_order.") {
                settings.organize.arcade_order.remove(profile);
                note(&mut fixed, "organize.arcade_order");
            } else if fields.is_empty() {
                reset_field(&mut settings, &e.field);
                let key: String = e.field.splitn(3, '.').take(2).collect::<Vec<_>>().join(".");
                note(&mut fixed, &key);
            } else {
                for f in fields {
                    reset_field(&mut settings, f);
                    note(&mut fixed, f);
                }
            }
        }
    }
    // Still wrong: only the groups with a problem fall back to defaults.
    for e in settings.validate(known) {
        let group = e.field.split('.').next().unwrap_or_default().to_string();
        reset_group(&mut settings, &group);
        note(&mut fixed, &group);
    }
    if !settings.validate(known).is_empty() {
        settings = Settings::default();
        note(&mut fixed, "*");
    }
    settings.normalise();
    Some(Parsed {
        settings,
        fixed,
        future,
    })
}

/// `value` without the list/map entries of `group.field` that cannot be
/// read (recent libraries, per-profile sizes), and whether any was dropped.
fn keep_good_entries(group: &str, field: &str, value: &Value) -> (Value, bool) {
    match (group, field, value) {
        ("library", "recent", Value::Array(items)) => {
            let good: Vec<Value> = items
                .iter()
                .filter(|v| serde_json::from_value::<RecentLibrary>((*v).clone()).is_ok())
                .cloned()
                .collect();
            let dropped = good.len() != items.len();
            (Value::Array(good), dropped)
        }
        ("art", "max_size", Value::Object(map)) => {
            let good: serde_json::Map<String, Value> = map
                .iter()
                .filter(|(_, v)| serde_json::from_value::<MaxSize>((*v).clone()).is_ok())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let dropped = good.len() != map.len();
            (Value::Object(good), dropped)
        }
        _ => (value.clone(), false),
    }
}

/// Resets the whole group `group` of `s` to its defaults.
fn reset_group(s: &mut Settings, group: &str) {
    let defaults = serde_json::to_value(Settings::default()).expect("defaults serialise");
    let mut v = serde_json::to_value(&*s).expect("settings serialise");
    if let (Some(slot), Some(def)) = (v.get_mut(group), defaults.get(group)) {
        *slot = def.clone();
        if let Ok(next) = serde_json::from_value(v) {
            *s = next;
        }
    }
}

/// Loads `dir/settings.json` (see the module docs). Side effects: a file
/// that is not settings is renamed aside; a file loading changed is
/// copied aside; a missing file is migrated from `library.json`.
pub fn load_settings(dir: &Path, known: &[String]) -> (Settings, LoadReport) {
    let path = dir.join(SETTINGS_FILE);
    let corrupt = |path: &Path| {
        let dest = aside_path(path, "bad");
        let backup = fs::rename(path, &dest).ok().map(|()| dest);
        (
            Settings::default(),
            LoadReport {
                notice: Some(LoadNotice::ResetCorrupt),
                backup,
                fixed: Vec::new(),
            },
        )
    };
    match read_capped(&path) {
        Read_::Missing => migrate_legacy(dir),
        // Unreadable (a folder, no permission): set aside as well, so the
        // next save never replaces what the user had.
        Read_::Unreadable | Read_::TooLarge => corrupt(&path),
        Read_::Text(text) => match parse_tolerant(&text, known) {
            None => corrupt(&path),
            Some(p) => {
                let notice = if p.future {
                    Some(LoadNotice::FutureVersion)
                } else if !p.fixed.is_empty() {
                    Some(LoadNotice::Fixed)
                } else {
                    None
                };
                let backup = notice.and_then(|_| backup_settings(dir, "bak").ok().flatten());
                (
                    p.settings,
                    LoadReport {
                        notice,
                        backup,
                        fixed: p.fixed,
                    },
                )
            }
        },
    }
}

/// Why [`read_settings_checked`] fell back to the defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadProblem {
    /// Not JSON settings (garbage, too large, not UTF-8, not an object).
    NotSettings,
    /// A folder, or no permission.
    Unreadable,
}

/// Like [`read_settings`], also saying why the defaults were used. Never
/// touches the disk; a missing file is not a problem.
pub fn read_settings_checked(path: &Path, known: &[String]) -> (Settings, Option<ReadProblem>) {
    match read_capped(path) {
        Read_::Missing => (Settings::default(), None),
        Read_::Unreadable => (Settings::default(), Some(ReadProblem::Unreadable)),
        Read_::TooLarge => (Settings::default(), Some(ReadProblem::NotSettings)),
        Read_::Text(text) => match parse_tolerant(&text, known) {
            Some(p) => (p.settings, None),
            None => (Settings::default(), Some(ReadProblem::NotSettings)),
        },
    }
}

/// Like [`load_settings`] without touching the disk (the CLI).
pub fn read_settings(path: &Path, known: &[String]) -> Settings {
    read_settings_checked(path, known).0
}

/// `library.json` → `settings.json` (ignored folders + recents), once.
fn migrate_legacy(dir: &Path) -> (Settings, LoadReport) {
    // A settings file existed once (it was set aside or copied): the old
    // file was migrated before and must not come back.
    if !asides(dir, "bad").is_empty() || !asides(dir, "bak").is_empty() {
        return (Settings::default(), LoadReport::default());
    }
    let legacy = dir.join(LEGACY_LIBRARY_FILE);
    let Read_::Text(text) = read_capped(&legacy) else {
        return (Settings::default(), LoadReport::default());
    };
    let Ok(Value::Object(old)) = serde_json::from_str::<Value>(&text) else {
        // Set aside once and start from defaults saved to settings.json, so
        // the next start neither migrates nor warns again.
        let dest = aside_path(&legacy, "bad");
        let backup = fs::rename(&legacy, &dest).ok().map(|()| dest);
        let s = Settings::default();
        let _ = save_settings(dir, &s);
        return (
            s,
            LoadReport {
                notice: Some(LoadNotice::ResetCorrupt),
                backup: backup.or(Some(legacy)),
                fixed: Vec::new(),
            },
        );
    };
    let mut s = Settings::default();
    if let Some(Value::Array(dirs)) = old.get("ignored_dirs") {
        s.library.set_ignored_dirs(
            dirs.iter()
                .filter_map(|d| d.as_str().map(String::from))
                .collect(),
        );
    }
    if let Some(Value::Array(recent)) = old.get("recent") {
        s.library.recent = recent
            .iter()
            .filter_map(|r| serde_json::from_value::<RecentLibrary>(r.clone()).ok())
            .collect();
    }
    s.normalise();
    let _ = save_settings(dir, &s);
    (
        s,
        LoadReport {
            notice: Some(LoadNotice::Migrated),
            backup: Some(legacy),
            fixed: Vec::new(),
        },
    )
}
