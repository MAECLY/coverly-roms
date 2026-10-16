//! Connected drives for the Start screen (spec §8, M3-06). Only lists
//! folders: never opens, writes or mounts anything.

use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;

/// A mounted drive or SD card the user can open as a library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Volume {
    pub path: String,
    pub name: String,
}

/// Folders whose children are mount points on this OS: macOS `/Volumes`;
/// Linux `/media/$USER`, `/run/media/$USER` and `/media`; none on Windows.
pub fn mount_parents() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![PathBuf::from("/Volumes")]
    }
    #[cfg(target_os = "linux")]
    {
        let mut out = Vec::new();
        if let Ok(user) = std::env::var("USER") {
            if !user.is_empty() && !user.contains('/') {
                out.push(Path::new("/media").join(&user));
                out.push(Path::new("/run/media").join(&user));
            }
        }
        out.push(PathBuf::from("/media"));
        out
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Vec::new()
    }
}

/// The child folders of `parents`: hidden names, files, missing parents and
/// anything that resolves to `/` (macOS lists the boot disk in `/Volumes`)
/// are skipped; the same folder reached twice is listed once; sorted by
/// name, ignoring case.
pub fn list_volumes(parents: &[PathBuf]) -> Vec<Volume> {
    let root = std::fs::canonicalize("/").ok();
    let mut found: BTreeMap<PathBuf, Volume> = BTreeMap::new();
    for parent in parents {
        let Ok(entries) = std::fs::read_dir(parent) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let Ok(real) = path.canonicalize() else {
                continue;
            };
            if !real.is_dir() || Some(&real) == root.as_ref() {
                continue;
            }
            found.entry(real).or_insert(Volume {
                path: path.display().to_string(),
                name,
            });
        }
    }
    let mut out: Vec<Volume> = found.into_values().collect();
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

/// The drives connected now. Windows: every existing `D:\`…`Z:\` except
/// the system drive; elsewhere the children of [`mount_parents`].
pub fn mounted_volumes() -> Vec<Volume> {
    #[cfg(windows)]
    {
        let system = std::env::var("SystemDrive")
            .unwrap_or_else(|_| "C:".into())
            .to_uppercase();
        (b'D'..=b'Z')
            .map(|c| format!("{}:\\", c as char))
            .filter(|d| !d.starts_with(&system) && std::path::Path::new(d).is_dir())
            .map(|d| Volume {
                name: d.trim_end_matches('\\').to_string(),
                path: d,
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        list_volumes(&mount_parents())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_parents_match_the_os() {
        let parents = mount_parents();
        if cfg!(target_os = "macos") {
            assert_eq!(parents, [PathBuf::from("/Volumes")]);
        } else if cfg!(target_os = "linux") {
            assert_eq!(parents.last(), Some(&PathBuf::from("/media")));
        } else {
            assert!(parents.is_empty());
        }
    }
}
