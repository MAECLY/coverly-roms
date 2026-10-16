//! Reading and writing `settings.json` (D24).

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::model::{RecentLibrary, Settings, SETTINGS_VERSION};
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
    /// Dotted fields that fell back to their defaults.
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
        Ok(text) => Read_::Text(text),
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

/// Copies `settings.json` aside; `None` when there is no file.
pub fn backup_settings(dir: &Path, tag: &str) -> io::Result<Option<PathBuf>> {
    let path = dir.join(SETTINGS_FILE);
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
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
    let tmp = dir.join(format!(".{SETTINGS_FILE}.tmp"));
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
            let before = merged[group.as_str()][field.as_str()].clone();
            merged[group.as_str()][field.as_str()] = value.clone();
            if serde_json::from_value::<Settings>(merged.clone()).is_err() {
                merged[group.as_str()][field.as_str()] = before;
                fixed.push(format!("{group}.{field}"));
            }
        }
    }
    let mut settings: Settings = serde_json::from_value(merged).ok()?;
    for _ in 0..3 {
        let errors = settings.validate(known);
        if errors.is_empty() {
            break;
        }
        for e in errors {
            if e.code == FieldErrorCode::Order {
                reset_field(&mut settings, "matching.auto_threshold");
                reset_field(&mut settings, "matching.review_threshold");
            } else {
                reset_field(&mut settings, &e.field);
            }
            let key: String = e.field.splitn(3, '.').take(2).collect::<Vec<_>>().join(".");
            if !fixed.contains(&key) {
                fixed.push(key);
            }
        }
    }
    if !settings.validate(known).is_empty() {
        return Some(Parsed {
            settings: Settings::default(),
            fixed,
            future,
        });
    }
    settings.normalise();
    Some(Parsed {
        settings,
        fixed,
        future,
    })
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

/// Like [`load_settings`] without touching the disk (the CLI).
pub fn read_settings(path: &Path, known: &[String]) -> Settings {
    match read_capped(path) {
        Read_::Text(text) => parse_tolerant(&text, known)
            .map(|p| p.settings)
            .unwrap_or_default(),
        _ => Settings::default(),
    }
}

/// `library.json` → `settings.json` (ignored folders + recents), once.
fn migrate_legacy(dir: &Path) -> (Settings, LoadReport) {
    let legacy = dir.join(LEGACY_LIBRARY_FILE);
    let Read_::Text(text) = read_capped(&legacy) else {
        return (Settings::default(), LoadReport::default());
    };
    let Ok(Value::Object(old)) = serde_json::from_str::<Value>(&text) else {
        return (
            Settings::default(),
            LoadReport {
                notice: Some(LoadNotice::ResetCorrupt),
                backup: Some(legacy),
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
