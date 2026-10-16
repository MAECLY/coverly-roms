//! Manual identities (M4-05, D32): "this ROM is <name>", chosen by the
//! user in "Search by title". Kept in the app data dir (not the cache:
//! Clear cache keeps them; not the job data folder: moving the backup
//! location never forgets them), keyed by the ROM's absolute path.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

pub const OVERRIDES_FILE: &str = "identity-overrides.json";
const MAX_OVERRIDES_BYTES: u64 = 8 * 1024 * 1024;
const MAX_NAME_CHARS: usize = 255;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualIdentity {
    /// The name the sources know the game by (e.g. a libretro file name).
    pub name: String,
    /// The source it came from.
    pub source: String,
    /// Unix seconds.
    pub set_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overrides {
    #[serde(default)]
    entries: BTreeMap<String, ManualIdentity>,
}

fn key(rom: &Path) -> String {
    let abs = std::path::absolute(rom).unwrap_or_else(|_| rom.to_path_buf());
    abs.to_string_lossy().nfc().collect()
}

fn valid_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n.chars().count() <= MAX_NAME_CHARS && !n.chars().any(char::is_control)
}

impl Overrides {
    fn parse(path: &Path) -> Result<Overrides, ()> {
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Overrides::default()),
            Err(_) => return Err(()),
        };
        if !meta.is_file() || meta.len() > MAX_OVERRIDES_BYTES {
            return Err(());
        }
        // Bounded even if the file grows after the check.
        let mut bytes = Vec::new();
        fs::File::open(path)
            .and_then(|f| f.take(MAX_OVERRIDES_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|_| ())?;
        if bytes.len() as u64 > MAX_OVERRIDES_BYTES {
            return Err(());
        }
        let mut o: Overrides = serde_json::from_slice(&bytes).map_err(|_| ())?;
        o.entries.retain(|_, v| valid_name(&v.name));
        for v in o.entries.values_mut() {
            v.name = v.name.trim().to_string();
        }
        Ok(o)
    }

    /// The overrides at `path`; empty when missing or unreadable. Never
    /// touches the file.
    pub fn read(path: &Path) -> Overrides {
        Overrides::parse(path).unwrap_or_default()
    }

    /// Like [`Overrides::read`], but a file that cannot be read as
    /// overrides is renamed aside (`.bad-<unix>`) first, so saving never
    /// loses it.
    pub fn load_for_update(path: &Path) -> Overrides {
        match Overrides::parse(path) {
            Ok(o) => o,
            Err(()) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| OVERRIDES_FILE.to_string());
                let aside = path.with_file_name(format!("{name}.bad-{}", crate::time::now_unix()));
                let _ = fs::rename(path, aside);
                Overrides::default()
            }
        }
    }

    /// Writes the overrides atomically (temporary file, then rename).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                fs::create_dir_all(dir)?;
            }
        }
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)
    }

    pub fn get(&self, rom: &Path) -> Option<&ManualIdentity> {
        self.entries.get(&key(rom))
    }

    /// Sets the identity of `rom`; `false` (nothing changed) for an empty
    /// name, one with control characters or over 255 characters.
    pub fn set(&mut self, rom: &Path, mut id: ManualIdentity) -> bool {
        if !valid_name(&id.name) {
            return false;
        }
        id.name = id.name.trim().to_string();
        self.entries.insert(key(rom), id);
        true
    }

    pub fn remove(&mut self, rom: &Path) -> bool {
        self.entries.remove(&key(rom)).is_some()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Records "`rom` is `name`" in `data_dir`.
pub fn remember_manual(data_dir: &Path, rom: &Path, name: &str, source: &str) -> io::Result<()> {
    let path = data_dir.join(OVERRIDES_FILE);
    let mut o = Overrides::load_for_update(&path);
    let ok = o.set(
        rom,
        ManualIdentity {
            name: name.into(),
            source: source.into(),
            set_at: crate::time::now_unix(),
        },
    );
    if !ok {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a usable name",
        ));
    }
    o.save(&path)
}

/// Forgets the manual identity of `rom`; `false` when it had none.
pub fn forget_manual(data_dir: &Path, rom: &Path) -> io::Result<bool> {
    let path = data_dir.join(OVERRIDES_FILE);
    let mut o = Overrides::load_for_update(&path);
    let removed = o.remove(rom);
    if removed {
        o.save(&path)?;
    }
    Ok(removed)
}
