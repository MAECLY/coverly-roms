//! Connected drives for the Start screen (spec §8, M3-06). Only lists
//! folders: never opens, writes or mounts anything.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

/// Names macOS mounts for itself (APFS system volumes, snapshots, firmware
/// helpers); never a drive a person plugged in.
const MACOS_SYSTEM_NAMES: [&str; 7] = [
    "Recovery",
    "Preboot",
    "VM",
    "Update",
    "xarts",
    "iSCPreboot",
    "Hardware",
];

/// Whether a mount named `name` (resolving to `real`) belongs to the system
/// rather than to the user, on this OS ([`is_system_mount_on`]).
pub fn is_system_mount(name: &str, real: &Path) -> bool {
    is_system_mount_on(cfg!(target_os = "macos"), name, real)
}

/// [`is_system_mount`] for a given OS. Everywhere: hidden names and
/// `com.apple.*` (Time Machine local snapshots). On macOS only: the APFS
/// helper volumes in [`MACOS_SYSTEM_NAMES`] and anything under
/// `/System/Volumes`, so a Linux or Windows drive labelled `Update` or `VM`
/// is still listed.
pub fn is_system_mount_on(macos: bool, name: &str, real: &Path) -> bool {
    name.starts_with('.')
        || name.starts_with("com.apple.")
        || (macos && (MACOS_SYSTEM_NAMES.contains(&name) || real.starts_with("/System/Volumes")))
}

/// The child folders of `parents`: system mounts ([`is_system_mount`]),
/// files, missing parents and anything that resolves to `/` (macOS lists the
/// boot disk in `/Volumes`) are skipped; the same folder reached twice is
/// listed once; sorted by name, ignoring case.
pub fn list_volumes(parents: &[PathBuf]) -> Vec<Volume> {
    let root = std::fs::canonicalize("/").ok();
    let mut found: BTreeMap<PathBuf, Volume> = BTreeMap::new();
    for parent in parents {
        let Ok(entries) = std::fs::read_dir(parent) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_system_mount(&name, Path::new("")) {
                continue;
            }
            let path = entry.path();
            let Ok(real) = path.canonicalize() else {
                continue;
            };
            if !real.is_dir() || Some(&real) == root.as_ref() || is_system_mount(&name, &real) {
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

/// A stable identity for the volume that holds `path`, for cache keys
/// that must survive a card being ejected and reinserted (spec §10;
/// `dev_t` changes on every mount). The file system's UUID where the OS
/// gives one (macOS `ATTR_VOL_UUID`, Linux `/dev/disk/by-uuid`), else
/// `path:<mount point>`. `None` only when `path` cannot be resolved.
pub fn volume_uuid(path: &Path) -> Option<String> {
    let real = std::fs::canonicalize(path).ok()?;
    let mount = mount_point(&real);
    Some(os_volume_uuid(&mount).unwrap_or_else(|| format!("path:{}", mount.display())))
}

/// The top folder of the file system holding `real` (a canonical path).
#[cfg(unix)]
fn mount_point(real: &Path) -> PathBuf {
    use std::os::unix::fs::MetadataExt;
    let Ok(dev) = std::fs::metadata(real).map(|m| m.dev()) else {
        return real.to_path_buf();
    };
    let mut mount = real;
    while let Some(parent) = mount.parent() {
        match std::fs::metadata(parent) {
            Ok(m) if m.dev() == dev => mount = parent,
            _ => break,
        }
    }
    mount.to_path_buf()
}

/// The drive root of `real` (`C:\`, `\\server\share\`).
#[cfg(not(unix))]
fn mount_point(real: &Path) -> PathBuf {
    real.ancestors()
        .last()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| real.to_path_buf())
}

/// The volume UUID macOS reports for the file system mounted at `mount`.
#[cfg(target_os = "macos")]
fn os_volume_uuid(mount: &Path) -> Option<String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    #[repr(C, packed(4))]
    struct Reply {
        len: u32,
        uuid: [u8; 16],
    }
    let c_path = CString::new(mount.as_os_str().as_bytes()).ok()?;
    // SAFETY: `attrlist` is a plain C struct, valid when zeroed.
    let mut list: libc::attrlist = unsafe { std::mem::zeroed() };
    list.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
    list.volattr = libc::ATTR_VOL_INFO | libc::ATTR_VOL_UUID;
    let mut reply = Reply {
        len: 0,
        uuid: [0; 16],
    };
    // SAFETY: the path is NUL-terminated, `list` and `reply` live across the
    // call and the size given is the size of `reply`.
    let rc = unsafe {
        libc::getattrlist(
            c_path.as_ptr(),
            (&mut list as *mut libc::attrlist).cast(),
            (&mut reply as *mut Reply).cast(),
            std::mem::size_of::<Reply>(),
            libc::FSOPT_NOFOLLOW,
        )
    };
    let (len, uuid) = (reply.len, reply.uuid);
    if rc != 0 || (len as usize) < std::mem::size_of::<Reply>() || uuid == [0; 16] {
        return None;
    }
    Some(format_uuid(&uuid))
}

/// The UUID under `/dev/disk/by-uuid` whose device is mounted at `mount`.
#[cfg(target_os = "linux")]
fn os_volume_uuid(mount: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let dev = std::fs::metadata(mount).ok()?.dev();
    std::fs::read_dir("/dev/disk/by-uuid")
        .ok()?
        .flatten()
        .find(|e| std::fs::metadata(e.path()).is_ok_and(|m| m.rdev() == dev))
        .map(|e| e.file_name().to_string_lossy().into_owned())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn os_volume_uuid(_mount: &Path) -> Option<String> {
    None
}

/// `uuid` in the upper-case hyphenated form `diskutil` prints.
#[cfg(any(target_os = "macos", test))]
fn format_uuid(uuid: &[u8; 16]) -> String {
    let hex: String = uuid.iter().map(|b| format!("{b:02X}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
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

    #[test]
    fn uuids_print_like_diskutil() {
        let uuid = [
            0x8b, 0x1e, 0x2d, 0x3c, 0x4b, 0x5a, 0x69, 0x78, 0x87, 0x96, 0xa5, 0xb4, 0xc3, 0xd2,
            0xe1, 0xf0,
        ];
        assert_eq!(format_uuid(&uuid), "8B1E2D3C-4B5A-6978-8796-A5B4C3D2E1F0");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_boot_volume_has_a_real_uuid() {
        let id = volume_uuid(Path::new("/")).unwrap();
        assert!(!id.starts_with("path:"), "{id}");
        assert_eq!(id.len(), 36, "{id}");
    }
}
