//! Sending files to the OS Trash and finding them there again (spec
//! 2026-10-23 §9.1, §9.2; plan deviation AP-3).
//!
//! Move-mode extraction never deletes an archive: it trashes it, and the
//! journal records where it went so undo can bring it back. Backends:
//!
//! - macOS: `NSFileManager trashItemAtURL:resultingItemURL:error:`, which
//!   returns the exact location (and refuses, rather than deletes, on a
//!   volume without a Trash).
//! - Windows: `SHFileOperationW` `FO_DELETE` with `FOF_ALLOWUNDO` (the
//!   Recycle Bin), only on fixed drives, with `FOF_WANTNUKEWARNING` so a
//!   file the bin cannot hold is never deleted silently. The location is
//!   found afterwards from the `$Recycle.Bin\<SID>\$I…` metadata.
//!   `windows-sys` has no COM vtables, so `IFileOperation` (the same engine)
//!   is not used directly.
//! - Linux: the freedesktop.org Trash specification, written here
//!   (`$XDG_DATA_HOME/Trash` on the home volume, `$topdir/.Trash/$uid` or
//!   `$topdir/.Trash-$uid` elsewhere), so the location is exact.
//!
//! [`FakeTrash`] is a tempdir stand-in for tests.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::transfer::{crc_of, mtime_ns};
use super::Fingerprint;

/// Where trashed files go. `trash` returns the location in the Trash when
/// the OS tells it (`Ok(None)`: trashed, location unknown); `find` looks a
/// file up in the Trash of the volume of `volume_of` by its original file
/// name and fingerprint; `available` says whether that volume has a Trash.
pub trait TrashBackend: Send + Sync {
    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, TrashError>;
    fn find(&self, volume_of: &Path, name: &str, fp: &Fingerprint) -> Option<PathBuf>;
    fn available(&self, path: &Path) -> bool;
}

/// Why a file was not sent to the Trash. The file is still where it was in
/// every case.
#[derive(Debug, thiserror::Error)]
pub enum TrashError {
    #[error("the volume of {0} has no Trash")]
    Unavailable(PathBuf),
    #[error("nothing to trash at {0}")]
    Missing(PathBuf),
    #[error("the Trash refused {path}: {detail}")]
    Refused { path: PathBuf, detail: String },
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The Trash of the operating system.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsTrash;

impl TrashBackend for OsTrash {
    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, TrashError> {
        platform::trash(path)
    }

    fn find(&self, volume_of: &Path, name: &str, fp: &Fingerprint) -> Option<PathBuf> {
        platform::find(volume_of, name, fp)
    }

    fn available(&self, path: &Path) -> bool {
        platform::available(path)
    }
}

/// `path` is a regular file with the size of `fp` and its modification
/// time, or (mtime changed) its CRC-32.
fn fp_matches(path: &Path, fp: &Fingerprint) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.file_type().is_file() || meta.len() != fp.size {
        return false;
    }
    if meta.modified().map(mtime_ns).ok() == Some(fp.mtime_ns) {
        return true;
    }
    fp.crc32.is_some_and(|crc| crc_of(path).ok() == Some(crc))
}

fn missing_or_io(e: io::Error, path: &Path) -> TrashError {
    if e.kind() == io::ErrorKind::NotFound {
        TrashError::Missing(path.to_path_buf())
    } else {
        TrashError::Io(e)
    }
}

/// Windows Recycle Bin `$I<id>` records, parsed without Windows APIs so
/// the rules that pick the file undo brings back are tested on every OS.
#[cfg(any(windows, test))]
mod recycle_info {
    /// What a `$I<id>` record says about `$R<id>`.
    #[derive(Debug, PartialEq, Eq)]
    pub(super) struct RecycleInfo {
        pub(super) size: u64,
        /// Deletion time, a FILETIME.
        pub(super) deleted: u64,
        pub(super) original: String,
    }

    /// Version 1 (Vista–8.1) has a fixed 260-unit path, version 2 a
    /// counted one. A path that is not valid UTF-16 is never ours.
    pub(super) fn parse(b: &[u8]) -> Option<RecycleInfo> {
        let word = |at: usize| -> Option<u64> {
            Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
        };
        let units = match word(0)? {
            1 => b.get(24..24 + 520)?,
            2 => {
                let len = u32::from_le_bytes(b.get(24..28)?.try_into().ok()?) as usize;
                b.get(28..28usize.checked_add(len.checked_mul(2)?)?)?
            }
            _ => return None,
        };
        let wide: Vec<u16> = units
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|&c| c != 0)
            .collect();
        let original = String::from_utf16(&wide).ok().filter(|o| !o.is_empty())?;
        Some(RecycleInfo {
            size: word(8)?,
            deleted: word(16)?,
            original,
        })
    }

    /// `info` is the record of the file trashed from `original` (the
    /// absolute path, compared case-insensitively as Windows does) with
    /// `size` bytes: a same-named file from another folder is not.
    pub(super) fn matches(info: &RecycleInfo, original: &str, size: u64) -> bool {
        info.size == size && info.original.to_lowercase() == original.to_lowercase()
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use std::{fs, mem};

    use objc2_foundation::{NSFileManager, NSURL};

    use super::{fp_matches, missing_or_io, Fingerprint, TrashError};

    pub(super) fn trash(path: &Path) -> Result<Option<PathBuf>, TrashError> {
        fs::symlink_metadata(path).map_err(|e| missing_or_io(e, path))?;
        let abs = std::path::absolute(path)?;
        let url = NSURL::from_file_path(&abs).ok_or_else(|| TrashError::Refused {
            path: abs.clone(),
            detail: "path has no file URL".into(),
        })?;
        let manager = NSFileManager::defaultManager();
        let mut out = None;
        match manager.trashItemAtURL_resultingItemURL_error(&url, Some(&mut out)) {
            Ok(()) => Ok(out.and_then(|u| u.to_file_path())),
            Err(_) if !available(&abs) => Err(TrashError::Unavailable(abs)),
            Err(e) => Err(TrashError::Refused {
                path: abs,
                detail: format!("{} ({})", e.localizedDescription(), e.code()),
            }),
        }
    }

    /// A local, writable volume (APFS, HFS+, FAT, exFAT…). Network volumes
    /// have no Trash.
    pub(super) fn available(path: &Path) -> bool {
        statfs(path).is_some_and(|s| {
            let flags = s.f_flags;
            flags & libc::MNT_LOCAL as u32 != 0 && flags & libc::MNT_RDONLY as u32 == 0
        })
    }

    pub(super) fn find(volume_of: &Path, name: &str, fp: &Fingerprint) -> Option<PathBuf> {
        find_in_dir(&trash_dir(volume_of)?, name, fp)
    }

    /// The file in the Trash folder `dir` that was `name` and matches `fp`:
    /// `name` itself first, else a clash rename of it.
    pub(super) fn find_in_dir(dir: &Path, name: &str, fp: &Fingerprint) -> Option<PathBuf> {
        let wanted = Path::new(name);
        let (stem, ext) = (wanted.file_stem()?, wanted.extension());
        let mut hits: Vec<PathBuf> = fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let Some(file) = p.file_name() else {
                    return false;
                };
                // The Trash renames a clash: macOS 26 appends a time to the
                // whole name and repeats the extension ("name.ext
                // 15-11-48-881.ext"), older releases a number to the stem
                // ("name 2.ext").
                let spaced = |prefix: &[u8]| {
                    p.file_stem().is_some_and(|s| {
                        let s = s.as_bytes();
                        s.len() > prefix.len() + 1
                            && s.starts_with(prefix)
                            && s[prefix.len()] == b' '
                    })
                };
                file == name
                    || (p.extension() == ext
                        && (spaced(name.as_bytes()) || spaced(stem.as_bytes())))
            })
            .filter(|p| fp_matches(p, fp))
            .collect();
        hits.sort_by_key(|p| (p.file_name() != Some(wanted.as_os_str()), p.clone()));
        hits.into_iter().next()
    }

    /// `~/.Trash` on the home volume, `<mount>/.Trashes/<uid>` elsewhere.
    fn trash_dir(volume_of: &Path) -> Option<PathBuf> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let here = existing(volume_of)?;
        if fs::metadata(&home).ok()?.dev() == fs::metadata(&here).ok()?.dev() {
            return Some(home.join(".Trash"));
        }
        let mount = mount_point(&here)?;
        // SAFETY: getuid has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        Some(mount.join(".Trashes").join(uid.to_string()))
    }

    fn mount_point(path: &Path) -> Option<PathBuf> {
        let s = statfs(path)?;
        let raw: Vec<u8> = s
            .f_mntonname
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        Some(PathBuf::from(std::ffi::OsStr::from_bytes(&raw)))
    }

    /// `path` or its nearest existing ancestor.
    fn existing(path: &Path) -> Option<PathBuf> {
        let abs = std::path::absolute(path).ok()?;
        abs.ancestors()
            .find(|p| fs::symlink_metadata(p).is_ok())
            .map(Path::to_path_buf)
    }

    fn statfs(path: &Path) -> Option<libc::statfs> {
        let c = CString::new(existing(path)?.as_os_str().as_bytes()).ok()?;
        // SAFETY: an all-zero statfs is a valid value to be overwritten.
        let mut s: libc::statfs = unsafe { mem::zeroed() };
        // SAFETY: `c` is nul-terminated and `s` is a valid out-pointer.
        (unsafe { libc::statfs(c.as_ptr(), &mut s) } == 0).then_some(s)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::ffi::{CString, OsStr};
    use std::fs::{self, DirBuilder, OpenOptions};
    use std::io::{self, ErrorKind, Write};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    use percent_encoding::{percent_decode, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

    use super::super::noclobber::rename_noclobber;
    use super::{fp_matches, missing_or_io, Fingerprint, TrashError};

    /// RFC 2396 escaping of the `Path=` key; `/` stays.
    const PATH_SET: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~')
        .remove(b'/');
    const MAX_NAMES: u32 = 1000;

    pub(super) fn trash(path: &Path) -> Result<Option<PathBuf>, TrashError> {
        trash_in(path, home_data().as_deref())
    }

    pub(super) fn available(path: &Path) -> bool {
        available_in(path, home_data().as_deref())
    }

    pub(super) fn find(volume_of: &Path, name: &str, fp: &Fingerprint) -> Option<PathBuf> {
        find_in(volume_of, name, fp, home_data().as_deref())
    }

    /// `$XDG_DATA_HOME` (absolute only) or `$HOME/.local/share`.
    fn home_data() -> Option<PathBuf> {
        if let Some(x) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
            if x.is_absolute() {
                return Some(x);
            }
        }
        let home = PathBuf::from(std::env::var_os("HOME")?);
        home.is_absolute()
            .then(|| home.join(".local").join("share"))
    }

    /// A trash directory and, for a `$topdir` one, the top directory the
    /// `Path=` key is relative to.
    struct TrashDir {
        dir: PathBuf,
        topdir: Option<PathBuf>,
    }

    pub(super) fn trash_in(
        path: &Path,
        home_data: Option<&Path>,
    ) -> Result<Option<PathBuf>, TrashError> {
        let meta = fs::symlink_metadata(path).map_err(|e| missing_or_io(e, path))?;
        let abs = absolute_no_follow(path)?;
        if read_only(&abs) {
            return Err(TrashError::Unavailable(abs));
        }
        let Some(trash) = trash_dir(&abs, meta.dev(), home_data, true) else {
            return Err(TrashError::Unavailable(abs));
        };
        let files = trash.dir.join("files");
        let info = trash.dir.join("info");
        for d in [&files, &info] {
            DirBuilder::new().recursive(true).mode(0o700).create(d)?;
        }
        let original = match &trash.topdir {
            Some(top) => abs.strip_prefix(top).unwrap_or(&abs).to_path_buf(),
            None => abs.clone(),
        };
        let name = abs
            .file_name()
            .ok_or_else(|| TrashError::Missing(abs.clone()))?
            .to_string_lossy()
            .into_owned();
        let body = format!(
            "[Trash Info]\nPath={}\nDeletionDate={}\n",
            encode(&original),
            deletion_date()
        );
        for n in 1..=MAX_NAMES {
            let candidate = if n == 1 {
                name.clone()
            } else {
                numbered(&name, n)
            };
            let info_path = info.join(format!("{candidate}.trashinfo"));
            let mut file = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&info_path)
            {
                Ok(f) => f,
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            };
            let dest = files.join(&candidate);
            if fs::symlink_metadata(&dest).is_ok() {
                drop(file);
                let _ = fs::remove_file(&info_path);
                continue;
            }
            if let Err(e) = file
                .write_all(body.as_bytes())
                .and_then(|()| file.sync_all())
            {
                drop(file);
                let _ = fs::remove_file(&info_path);
                return Err(e.into());
            }
            drop(file);
            // Never `fs::rename`: it would replace a file that appeared in
            // `files/` since the check above.
            match rename_noclobber(&abs, &dest) {
                Ok(()) => {}
                Err(e)
                    if e.kind() == ErrorKind::AlreadyExists
                        && fs::symlink_metadata(&abs).is_ok() =>
                {
                    let _ = fs::remove_file(&info_path);
                    continue;
                }
                Err(e) => {
                    let _ = fs::remove_file(&info_path);
                    return Err(missing_or_io(e, &abs));
                }
            }
            if let Ok(dir) = fs::File::open(&files) {
                let _ = dir.sync_all();
            }
            return Ok(Some(dest));
        }
        Err(TrashError::Io(io::Error::new(
            ErrorKind::AlreadyExists,
            format!("no free name in {}", files.display()),
        )))
    }

    pub(super) fn available_in(path: &Path, home_data: Option<&Path>) -> bool {
        let Some(here) = existing(path) else {
            return false;
        };
        let Ok(meta) = fs::metadata(&here) else {
            return false;
        };
        !read_only(&here) && trash_dir(&here, meta.dev(), home_data, false).is_some()
    }

    pub(super) fn find_in(
        volume_of: &Path,
        name: &str,
        fp: &Fingerprint,
        home_data: Option<&Path>,
    ) -> Option<PathBuf> {
        let here = existing(volume_of)?;
        let trash = trash_dir(&here, fs::metadata(&here).ok()?.dev(), home_data, false)?;
        let mut best: Option<(String, PathBuf)> = None;
        for entry in fs::read_dir(trash.dir.join("info")).ok()?.flatten() {
            let info_name = entry.file_name();
            let Some(stem) = info_name
                .as_bytes()
                .strip_suffix(b".trashinfo")
                .map(OsStr::from_bytes)
            else {
                continue;
            };
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let (mut original, mut date) = (None, String::new());
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("Path=") {
                    original = Some(PathBuf::from(OsStr::from_bytes(
                        &percent_decode(v.as_bytes()).collect::<Vec<u8>>(),
                    )));
                } else if let Some(v) = line.strip_prefix("DeletionDate=") {
                    date = v.to_string();
                }
            }
            if original.as_deref().and_then(Path::file_name) != Some(OsStr::new(name)) {
                continue;
            }
            let file = trash.dir.join("files").join(stem);
            if fp_matches(&file, fp) && best.as_ref().is_none_or(|(d, _)| date >= *d) {
                best = Some((date, file));
            }
        }
        best.map(|(_, p)| p)
    }

    /// The home trash when `dev` is the home data volume, else the
    /// `$topdir` trash of the mount point holding `path`. `create` makes
    /// the per-user directory; without it, a missing one counts when it
    /// could be made.
    fn trash_dir(
        path: &Path,
        dev: u64,
        home_data: Option<&Path>,
        create: bool,
    ) -> Option<TrashDir> {
        if let Some(home) = home_data {
            let home_dev = existing(home)
                .and_then(|h| fs::metadata(h).ok())
                .map(|m| m.dev());
            if home_dev == Some(dev) {
                return Some(TrashDir {
                    dir: home.join("Trash"),
                    topdir: None,
                });
            }
        }
        let top = mount_point(path, dev)?;
        // SAFETY: getuid has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        let admin = top.join(".Trash");
        if fs::symlink_metadata(&admin)
            .is_ok_and(|m| m.is_dir() && m.permissions().mode() & 0o1000 != 0)
        {
            let dir = admin.join(uid.to_string());
            if usable_dir(&dir, uid, create) {
                return Some(TrashDir {
                    dir,
                    topdir: Some(top),
                });
            }
        }
        let dir = top.join(format!(".Trash-{uid}"));
        if usable_dir(&dir, uid, create) {
            return Some(TrashDir {
                dir,
                topdir: Some(top),
            });
        }
        if !create && fs::symlink_metadata(&dir).is_err() && writable(&top) {
            return Some(TrashDir {
                dir,
                topdir: Some(top),
            });
        }
        None
    }

    /// A real directory (no symlink) owned by `uid`, or made now.
    fn usable_dir(dir: &Path, uid: u32, create: bool) -> bool {
        match fs::symlink_metadata(dir) {
            Ok(m) => m.is_dir() && m.uid() == uid,
            Err(e) if e.kind() == ErrorKind::NotFound && create => {
                DirBuilder::new().mode(0o700).create(dir).is_ok()
            }
            Err(_) => false,
        }
    }

    fn mount_point(path: &Path, dev: u64) -> Option<PathBuf> {
        let mut top = existing(path)?;
        while let Some(parent) = top.parent() {
            if fs::metadata(parent).ok()?.dev() != dev {
                break;
            }
            top = parent.to_path_buf();
        }
        Some(top)
    }

    /// Absolute, with the parent resolved and the last component kept (a
    /// symlink is trashed itself, not its target).
    fn absolute_no_follow(path: &Path) -> io::Result<PathBuf> {
        let abs = std::path::absolute(path)?;
        match (abs.parent(), abs.file_name()) {
            (Some(parent), Some(name)) => Ok(fs::canonicalize(parent)?.join(name)),
            _ => Ok(abs),
        }
    }

    fn existing(path: &Path) -> Option<PathBuf> {
        let abs = std::path::absolute(path).ok()?;
        abs.ancestors()
            .find(|p| fs::symlink_metadata(p).is_ok())
            .and_then(|p| fs::canonicalize(p).ok())
    }

    fn cstr(path: &Path) -> Option<CString> {
        CString::new(path.as_os_str().as_bytes()).ok()
    }

    fn writable(path: &Path) -> bool {
        // SAFETY: the pointer is a nul-terminated path.
        cstr(path).is_some_and(|c| unsafe { libc::access(c.as_ptr(), libc::W_OK) } == 0)
    }

    fn read_only(path: &Path) -> bool {
        let Some(c) = existing(path).as_deref().and_then(cstr) else {
            return true;
        };
        // SAFETY: an all-zero statvfs is a valid value to be overwritten.
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: `c` is nul-terminated and `s` is a valid out-pointer.
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
            return true;
        }
        s.f_flag & libc::ST_RDONLY != 0
    }

    fn encode(path: &Path) -> String {
        match path.to_str() {
            Some(s) => utf8_percent_encode(s, PATH_SET).to_string(),
            None => {
                percent_encoding::percent_encode(path.as_os_str().as_bytes(), PATH_SET).to_string()
            }
        }
    }

    /// `name.N.ext` (no extension: `name.N`).
    fn numbered(name: &str, n: u32) -> String {
        match name.rfind('.').filter(|&i| i > 0) {
            Some(i) => format!("{}.{n}{}", &name[..i], &name[i..]),
            None => format!("{name}.{n}"),
        }
    }

    /// Local time, `YYYY-MM-DDThh:mm:ss` (the spec's format).
    fn deletion_date() -> String {
        // SAFETY: time(NULL) has no preconditions; localtime_r writes into
        // the zeroed `tm` we own.
        let tm = unsafe {
            let now = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            libc::localtime_r(&now, &mut tm);
            tm
        };
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    use windows_sys::core::PWSTR;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumePathNameW};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
        FOF_WANTNUKEWARNING, FO_DELETE, SHFILEOPSTRUCTW,
    };

    use super::{fp_matches, missing_or_io, mtime_ns, recycle_info, Fingerprint, TrashError};

    /// `GetDriveTypeW`: a fixed drive (removable and network drives have
    /// no Recycle Bin).
    const DRIVE_FIXED: u32 = 3;
    /// `$I` metadata files are a few hundred bytes.
    const MAX_INFO: u64 = 64 * 1024;

    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(Some(0)).collect()
    }

    fn volume_root(path: &Path) -> Option<PathBuf> {
        let abs = std::path::absolute(path).ok()?;
        let w = wide(abs.as_os_str());
        let mut root = [0u16; 1024];
        // SAFETY: `w` is nul-terminated and `root` holds `len` u16s.
        let ok = unsafe { GetVolumePathNameW(w.as_ptr(), root.as_mut_ptr(), root.len() as u32) };
        if ok == 0 {
            return None;
        }
        let end = root.iter().position(|&c| c == 0).unwrap_or(root.len());
        Some(PathBuf::from(OsString::from_wide(&root[..end])))
    }

    pub(super) fn available(path: &Path) -> bool {
        volume_root(path).is_some_and(|root| {
            let w = wide(root.as_os_str());
            // SAFETY: `w` is a nul-terminated root path.
            (unsafe { GetDriveTypeW(w.as_ptr()) }) == DRIVE_FIXED
        })
    }

    pub(super) fn trash(path: &Path) -> Result<Option<PathBuf>, TrashError> {
        let meta = fs::symlink_metadata(path).map_err(|e| missing_or_io(e, path))?;
        let abs = std::path::absolute(path)?;
        if !available(&abs) {
            return Err(TrashError::Unavailable(abs));
        }
        let fp = Fingerprint {
            size: meta.len(),
            mtime_ns: meta.modified().map_or(0, mtime_ns),
            crc32: None,
        };
        // pFrom is a list: each path nul-terminated, the list ends with
        // one more nul.
        let mut from = wide(abs.as_os_str());
        from.push(0);
        let flags =
            FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT | FOF_WANTNUKEWARNING;
        let mut op = SHFILEOPSTRUCTW {
            hwnd: std::ptr::null_mut(),
            wFunc: FO_DELETE,
            pFrom: from.as_ptr(),
            pTo: std::ptr::null(),
            fFlags: flags as u16,
            fAnyOperationsAborted: 0,
            hNameMappings: std::ptr::null_mut(),
            lpszProgressTitle: std::ptr::null(),
        };
        // SAFETY: `op` is fully initialised and `from` outlives the call.
        let rc = unsafe { SHFileOperationW(&mut op) };
        if rc != 0 || op.fAnyOperationsAborted != 0 || fs::symlink_metadata(&abs).is_ok() {
            return Err(TrashError::Refused {
                path: abs,
                detail: format!("SHFileOperationW returned {rc:#x}"),
            });
        }
        let name = abs.file_name().and_then(OsStr::to_str).unwrap_or_default();
        Ok(find(&abs, name, &fp))
    }

    /// The string SID (`S-1-5-21-…`) of the user this process runs as:
    /// the name of their folder in `$Recycle.Bin`.
    fn current_user_sid() -> Option<String> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the pseudo-handle of the current process needs no
        // closing; `token` receives a handle closed below.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return None;
        }
        let sid = token_user_sid(token);
        // SAFETY: `token` is the handle opened above.
        unsafe { CloseHandle(token) };
        sid
    }

    fn token_user_sid(token: HANDLE) -> Option<String> {
        let mut len = 0u32;
        // SAFETY: a null buffer of length 0 only asks for the size.
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len) };
        if len == 0 {
            return None;
        }
        // u64s keep the buffer aligned for `TOKEN_USER`.
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        // SAFETY: `buf` holds at least `len` bytes.
        let ok = unsafe {
            GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len)
        };
        if ok == 0 {
            return None;
        }
        // SAFETY: on success `buf` starts with a `TOKEN_USER` whose SID
        // points into `buf`.
        let user = unsafe { &*buf.as_ptr().cast::<TOKEN_USER>() };
        let mut text: PWSTR = std::ptr::null_mut();
        // SAFETY: the SID is valid while `buf` lives; `text` receives a
        // LocalAlloc'd nul-terminated string freed below.
        if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 || text.is_null() {
            return None;
        }
        // SAFETY: `text` is nul-terminated.
        let units = unsafe {
            let n = (0..).take_while(|&i| *text.add(i) != 0).count();
            std::slice::from_raw_parts(text, n).to_vec()
        };
        // SAFETY: `text` came from `ConvertSidToStringSidW`.
        unsafe { LocalFree(text.cast()) };
        String::from_utf16(&units).ok()
    }

    /// `<root>\$Recycle.Bin\<SID>\$I<id>` names the original path and
    /// size of `$R<id>`. Only the current user's folder is read, and only a
    /// record of exactly the absolute path `volume_of` (the file that was
    /// trashed) with the size of `fp` counts: a same-named file from
    /// another folder, or another user's, is never brought back.
    pub(super) fn find(volume_of: &Path, name: &str, fp: &Fingerprint) -> Option<PathBuf> {
        let original = std::path::absolute(volume_of).ok()?;
        let original = original.to_str()?;
        let same_name = Path::new(original)
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|n| n.to_lowercase() == name.to_lowercase());
        if !same_name {
            return None;
        }
        let bin = volume_root(volume_of)?
            .join("$Recycle.Bin")
            .join(current_user_sid()?);
        let mut best: Option<(u64, PathBuf)> = None;
        for entry in fs::read_dir(&bin).ok()?.flatten() {
            let file = entry.file_name();
            let Some(id) = file.to_str().and_then(|f| f.strip_prefix("$I")) else {
                continue;
            };
            if entry.metadata().map_or(true, |m| m.len() > MAX_INFO) {
                continue;
            }
            let Some(info) = fs::read(entry.path())
                .ok()
                .as_deref()
                .and_then(recycle_info::parse)
            else {
                continue;
            };
            let data = bin.join(format!("$R{id}"));
            if recycle_info::matches(&info, original, fp.size)
                && fp_matches(&data, fp)
                && best.as_ref().is_none_or(|(w, _)| info.deleted >= *w)
            {
                best = Some((info.deleted, data));
            }
        }
        best.map(|(_, p)| p)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod platform {
    use std::path::{Path, PathBuf};

    use super::{Fingerprint, TrashError};

    pub(super) fn trash(path: &Path) -> Result<Option<PathBuf>, TrashError> {
        Err(TrashError::Unavailable(path.to_path_buf()))
    }

    pub(super) fn find(_volume_of: &Path, _name: &str, _fp: &Fingerprint) -> Option<PathBuf> {
        None
    }

    pub(super) fn available(_path: &Path) -> bool {
        false
    }
}

/// Names the folder a test build of the core uses as the Trash instead of
/// the OS one (see [`process_trash`]).
#[cfg(feature = "test-util")]
pub const FAKE_TRASH_ENV: &str = "COVERLY_TEST_FAKE_TRASH";

/// With [`FAKE_TRASH_ENV`], any non-empty value makes the fake Trash
/// behave like a volume without one ([`FakeTrash::unavailable`]).
#[cfg(feature = "test-util")]
pub const FAKE_TRASH_UNAVAILABLE_ENV: &str = "COVERLY_TEST_FAKE_TRASH_UNAVAILABLE";

/// The Trash of this process: the OS Trash, or, in a build with the
/// `test-util` feature only, a [`FakeTrash`] in the folder
/// `COVERLY_TEST_FAKE_TRASH` names, so end-to-end tests of the CLI never
/// fill the real Trash (unavailable when `COVERLY_TEST_FAKE_TRASH_UNAVAILABLE`
/// is set). Release builds ignore both variables.
pub fn process_trash() -> Box<dyn TrashBackend> {
    #[cfg(feature = "test-util")]
    if let Some(dir) = std::env::var_os(FAKE_TRASH_ENV).filter(|d| !d.is_empty()) {
        let dir = PathBuf::from(dir);
        let unavailable =
            std::env::var_os(FAKE_TRASH_UNAVAILABLE_ENV).is_some_and(|v| !v.is_empty());
        return Box::new(if unavailable {
            FakeTrash::unavailable(dir)
        } else {
            FakeTrash::new(dir)
        });
    }
    Box::new(OsTrash)
}

/// A Trash in a tempdir: each trashed file lands in `<dir>/<nnnn>/<name>`.
/// `unavailable` refuses everything (a volume without a Trash);
/// `fail_after(n)` refuses every call after `n` successful ones.
#[cfg(any(test, feature = "test-util"))]
#[derive(Debug)]
pub struct FakeTrash {
    dir: PathBuf,
    unavailable: bool,
    fail_after: Option<usize>,
    done: std::sync::atomic::AtomicUsize,
}

#[cfg(any(test, feature = "test-util"))]
impl FakeTrash {
    pub fn new(dir: impl Into<PathBuf>) -> FakeTrash {
        FakeTrash {
            dir: dir.into(),
            unavailable: false,
            fail_after: None,
            done: Default::default(),
        }
    }

    pub fn unavailable(dir: impl Into<PathBuf>) -> FakeTrash {
        FakeTrash {
            unavailable: true,
            ..FakeTrash::new(dir)
        }
    }

    pub fn fail_after(mut self, n: usize) -> FakeTrash {
        self.fail_after = Some(n);
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How many files it has taken.
    pub fn trashed(&self) -> usize {
        self.done.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "test-util"))]
impl TrashBackend for FakeTrash {
    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, TrashError> {
        use std::sync::atomic::Ordering;

        if self.unavailable {
            return Err(TrashError::Unavailable(path.to_path_buf()));
        }
        fs::symlink_metadata(path).map_err(|e| missing_or_io(e, path))?;
        let n = self.done.load(Ordering::SeqCst);
        if self.fail_after.is_some_and(|limit| n >= limit) {
            return Err(TrashError::Refused {
                path: path.to_path_buf(),
                detail: "injected failure".into(),
            });
        }
        let name = path
            .file_name()
            .ok_or_else(|| TrashError::Missing(path.to_path_buf()))?;
        // A fresh slot, taken exclusively: another FakeTrash over the same
        // directory (a resumed run) or another thread never shares it.
        fs::create_dir_all(&self.dir)?;
        let mut k = n;
        let slot = loop {
            let slot = self.dir.join(format!("{k:04}"));
            match fs::create_dir(&slot) {
                Ok(()) => break slot,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => k += 1,
                Err(e) => return Err(e.into()),
            }
        };
        let dest = slot.join(name);
        if let Err(e) = super::noclobber::rename_noclobber(path, &dest) {
            let _ = fs::remove_dir(&slot);
            return Err(missing_or_io(e, path));
        }
        self.done.fetch_add(1, Ordering::SeqCst);
        Ok(Some(dest))
    }

    fn find(&self, _volume_of: &Path, name: &str, fp: &Fingerprint) -> Option<PathBuf> {
        let mut slots: Vec<PathBuf> = fs::read_dir(&self.dir)
            .ok()?
            .flatten()
            .map(|e| e.path().join(name))
            .filter(|p| fp_matches(p, fp))
            .collect();
        slots.sort();
        slots.pop()
    }

    fn available(&self, _path: &Path) -> bool {
        !self.unavailable
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    fn fp_of(path: &Path, crc32: Option<u32>) -> Fingerprint {
        let meta = fs::metadata(path).unwrap();
        Fingerprint {
            size: meta.len(),
            mtime_ns: meta.modified().map_or(0, mtime_ns),
            crc32,
        }
    }

    #[test]
    fn fake_trash_moves_and_reports_location() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("Tetris Plus.zip");
        fs::write(&src, b"7z archive").unwrap();
        let trash = FakeTrash::new(tmp.path().join("trash"));
        assert!(trash.available(&src));

        let at = trash.trash(&src).unwrap().expect("location");
        assert!(!src.exists());
        assert_eq!(fs::read(&at).unwrap(), b"7z archive");
        assert_eq!(at.file_name().unwrap(), "Tetris Plus.zip");
        assert!(at.starts_with(trash.dir()));
        assert_eq!(trash.trashed(), 1);

        // A second file of the same name does not clobber the first.
        fs::write(&src, b"other").unwrap();
        let again = trash.trash(&src).unwrap().unwrap();
        assert_ne!(again, at);
        assert_eq!(fs::read(&at).unwrap(), b"7z archive");
    }

    #[test]
    fn fake_trash_never_clobbers_an_earlier_trash_in_the_same_dir() {
        // A resumed run builds a new FakeTrash over the same directory: its
        // counter starts again at 0, but the first archive must survive.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("trash");
        let src = tmp.path().join("Tetris Plus.zip");
        fs::write(&src, b"first run").unwrap();
        let first = FakeTrash::new(&dir).trash(&src).unwrap().unwrap();
        fs::write(&src, b"second run").unwrap();
        let second = FakeTrash::new(&dir).trash(&src).unwrap().unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), b"first run");
        assert_eq!(fs::read(&second).unwrap(), b"second run");
    }

    #[test]
    fn fake_trash_unavailable() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("game.zip");
        fs::write(&src, b"x").unwrap();
        let trash = FakeTrash::unavailable(tmp.path().join("trash"));
        assert!(!trash.available(&src));
        assert!(matches!(trash.trash(&src), Err(TrashError::Unavailable(_))));
        assert!(src.exists(), "the file stays where it was");
    }

    #[test]
    fn fake_trash_fail_after_refuses_and_keeps_file() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = FakeTrash::new(tmp.path().join("trash")).fail_after(1);
        let (a, b) = (tmp.path().join("a.zip"), tmp.path().join("b.zip"));
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();
        trash.trash(&a).unwrap();
        assert!(matches!(trash.trash(&b), Err(TrashError::Refused { .. })));
        assert!(b.exists());
    }

    #[test]
    fn fake_trash_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = FakeTrash::new(tmp.path().join("trash"));
        let gone = tmp.path().join("gone.zip");
        assert!(matches!(trash.trash(&gone), Err(TrashError::Missing(_))));
    }

    #[test]
    fn fake_trash_find_by_name_and_fp() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = FakeTrash::new(tmp.path().join("trash"));
        let src = tmp.path().join("Next Tetris, The.zip");
        fs::write(&src, b"first").unwrap();
        let fp_first = fp_of(&src, None);
        let first = trash.trash(&src).unwrap().unwrap();
        fs::write(&src, b"second!").unwrap();
        let fp_second = fp_of(&src, None);
        let second = trash.trash(&src).unwrap().unwrap();

        let name = "Next Tetris, The.zip";
        assert_eq!(trash.find(tmp.path(), name, &fp_first), Some(first));
        assert_eq!(trash.find(tmp.path(), name, &fp_second), Some(second));
        assert_eq!(trash.find(tmp.path(), "Other.zip", &fp_first), None);
        let wrong = Fingerprint {
            size: 999,
            ..fp_first
        };
        assert_eq!(trash.find(tmp.path(), name, &wrong), None);
    }

    #[test]
    fn fp_matches_falls_back_to_crc_when_mtime_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("a.bin");
        fs::write(&f, b"abc").unwrap();
        let crc = crc32fast::hash(b"abc");
        let moved = Fingerprint {
            mtime_ns: 1,
            ..fp_of(&f, Some(crc))
        };
        assert!(fp_matches(&f, &moved));
        let no_crc = Fingerprint {
            crc32: None,
            ..moved
        };
        assert!(!fp_matches(&f, &no_crc));
        let bad_crc = Fingerprint {
            crc32: Some(crc ^ 1),
            ..moved
        };
        assert!(!fp_matches(&f, &bad_crc));
    }

    /// Trashes a real tempfile; run with `-- --ignored os_trash_smoke`.
    #[test]
    #[ignore = "touches the real OS Trash"]
    fn os_trash_smoke() {
        // Twice under one name: the second clashes with the first in the
        // Trash, and `find` tells them apart by fingerprint.
        let tmp = tempfile::tempdir().unwrap();
        let name = "coverly-trash-smoke.bin";
        let src = tmp.path().join(name);
        let mut trashed = Vec::new();
        for body in [
            &b"coverly trash smoke"[..],
            &b"coverly trash smoke, again"[..],
        ] {
            fs::write(&src, body).unwrap();
            let fp = fp_of(&src, None);
            assert!(OsTrash.available(&src));
            let at = OsTrash.trash(&src).unwrap();
            assert!(!src.exists());
            let at = at
                .or_else(|| OsTrash.find(tmp.path(), name, &fp))
                .expect("the Trash location");
            assert!(at.exists(), "{} exists", at.display());
            trashed.push((at, fp));
        }
        let found: Vec<_> = trashed
            .iter()
            .map(|(_, fp)| OsTrash.find(tmp.path(), name, fp))
            .collect();
        for (at, _) in &trashed {
            let _ = fs::remove_file(at);
            #[cfg(target_os = "linux")]
            if let (Some(dir), Some(file)) = (at.parent().and_then(Path::parent), at.file_name()) {
                let mut info = file.to_os_string();
                info.push(".trashinfo");
                let _ = fs::remove_file(dir.join("info").join(info));
            }
        }
        assert_ne!(trashed[0].0, trashed[1].0);
        for ((at, _), found) in trashed.iter().zip(found) {
            assert_eq!(
                found.as_ref(),
                Some(at),
                "found again by name and fingerprint"
            );
        }
    }

    mod recycle_info_tests {
        use super::super::recycle_info::{matches, parse, RecycleInfo};

        fn info_v2(path: &str, size: u64, when: u64) -> Vec<u8> {
            let units: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            let mut b = Vec::new();
            b.extend_from_slice(&2u64.to_le_bytes());
            b.extend_from_slice(&size.to_le_bytes());
            b.extend_from_slice(&when.to_le_bytes());
            b.extend_from_slice(&(units.len() as u32).to_le_bytes());
            for u in units {
                b.extend_from_slice(&u.to_le_bytes());
            }
            b
        }

        fn info_v1(path: &str, size: u64, when: u64) -> Vec<u8> {
            let mut b = Vec::new();
            b.extend_from_slice(&1u64.to_le_bytes());
            b.extend_from_slice(&size.to_le_bytes());
            b.extend_from_slice(&when.to_le_bytes());
            let mut units: Vec<u16> = path.encode_utf16().collect();
            units.resize(260, 0);
            for u in units {
                b.extend_from_slice(&u.to_le_bytes());
            }
            b
        }

        fn tetris(original: &str) -> RecycleInfo {
            RecycleInfo {
                size: 123,
                deleted: 42,
                original: original.to_owned(),
            }
        }

        #[test]
        fn recycle_bin_info_v2_parses() {
            let b = info_v2(r"D:\ROMs\Tetris Plus.zip", 123, 42);
            assert_eq!(parse(&b), Some(tetris(r"D:\ROMs\Tetris Plus.zip")));
        }

        #[test]
        fn recycle_bin_info_v1_parses() {
            let b = info_v1(r"D:\ROMs\Tetris Plus.zip", 123, 42);
            assert_eq!(parse(&b), Some(tetris(r"D:\ROMs\Tetris Plus.zip")));
        }

        #[test]
        fn recycle_bin_info_truncated_is_ignored() {
            let b = info_v2(r"D:\ROMs\Tetris Plus.zip", 123, 42);
            assert_eq!(parse(&b[..30]), None);
            let mut huge = b.clone();
            huge[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
            assert_eq!(parse(&huge), None);
            let mut unknown = b;
            unknown[0] = 3;
            assert_eq!(parse(&unknown), None);
        }

        #[test]
        fn recycle_bin_info_matches_only_the_exact_path_and_size() {
            let info = tetris(r"D:\ROMs\Tetris Plus.zip");
            assert!(matches(&info, r"D:\ROMs\Tetris Plus.zip", 123));
            // Windows paths are case-insensitive.
            assert!(matches(&info, r"d:\roms\TETRIS PLUS.ZIP", 123));
            // Same file name, another folder (or another user's file).
            assert!(!matches(&info, r"D:\Other\Tetris Plus.zip", 123));
            assert!(!matches(&info, r"E:\ROMs\Tetris Plus.zip", 123));
            assert!(!matches(&info, r"D:\ROMs\Tetris Plus.zip.bak", 123));
            // Same path, different contents.
            assert!(!matches(&info, r"D:\ROMs\Tetris Plus.zip", 124));
        }
    }

    #[cfg(target_os = "macos")]
    mod macos_tests {
        use std::fs;

        use super::super::platform::find_in_dir;
        use super::fp_of;

        #[test]
        fn macos_find_prefers_the_name_then_a_clash_rename() {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path();
            let name = "Tetris Plus.zip";
            fs::write(dir.join(name), b"saturn").unwrap();
            let fp = fp_of(&dir.join(name), None);
            assert_eq!(find_in_dir(dir, name, &fp), Some(dir.join(name)));

            // The Trash renamed ours on a clash; the exact name is another file.
            fs::write(dir.join(name), b"another game").unwrap();
            fs::rename(dir.join(name), dir.join("other")).unwrap();
            fs::rename(dir.join("other"), dir.join(name)).unwrap();
            // macOS 26 appends a time to the whole name and repeats the
            // extension ("a.zip 15-11-48-881.zip", "noext 15-11-48-884");
            // older releases put a number after the stem ("a 2.zip").
            for (wanted, renamed) in [
                (name, "Tetris Plus.zip 15-11-48-881.zip"),
                (name, "Tetris Plus 2.zip"),
                ("noext", "noext 15-11-48-884"),
                ("a.tar.gz", "a.tar.gz 15-11-48-887.gz"),
            ] {
                let renamed = dir.join(renamed);
                fs::write(&renamed, b"saturn").unwrap();
                let fp = fp_of(&renamed, None);
                assert_eq!(find_in_dir(dir, wanted, &fp), Some(renamed.clone()));
                fs::remove_file(&renamed).unwrap();
            }

            // Neither a longer stem without the space nor another extension.
            for other in ["Tetris Plusx.zip", "Tetris Plus 2.7z"] {
                let f = dir.join(other);
                fs::write(&f, b"saturn").unwrap();
                let fp = fp_of(&f, None);
                assert_ne!(find_in_dir(dir, name, &fp), Some(f.clone()));
                fs::remove_file(&f).unwrap();
            }
            assert_eq!(find_in_dir(&dir.join("missing"), name, &fp), None);
        }
    }

    #[cfg(target_os = "linux")]
    mod linux_tests {
        use std::fs;

        use super::super::platform::{available_in, find_in, trash_in};
        use super::fp_of;

        #[test]
        fn linux_trash_writes_info_file() {
            let tmp = tempfile::tempdir().unwrap();
            let data = tmp.path().join("data");
            fs::create_dir(&data).unwrap();
            let src = tmp.path().join("Tetris Plus [T-5704H].zip");
            fs::write(&src, b"saturn").unwrap();
            let fp = fp_of(&src, None);
            assert!(available_in(&src, Some(&data)));

            let at = trash_in(&src, Some(&data)).unwrap().unwrap();
            assert!(!src.exists());
            assert_eq!(at, data.join("Trash/files/Tetris Plus [T-5704H].zip"));
            let info =
                fs::read_to_string(data.join("Trash/info/Tetris Plus [T-5704H].zip.trashinfo"))
                    .unwrap();
            let mut lines = info.lines();
            assert_eq!(lines.next(), Some("[Trash Info]"));
            let path_line = lines.next().unwrap();
            assert!(
                path_line.ends_with("/Tetris%20Plus%20%5BT-5704H%5D.zip"),
                "{path_line}"
            );
            assert!(path_line.starts_with("Path=/"), "{path_line}");
            let date = lines.next().unwrap().strip_prefix("DeletionDate=").unwrap();
            assert_eq!(date.len(), 19, "{date}");

            // Same name again: a second slot, the first kept.
            fs::write(&src, b"saturn 2").unwrap();
            let again = trash_in(&src, Some(&data)).unwrap().unwrap();
            assert_eq!(again, data.join("Trash/files/Tetris Plus [T-5704H].2.zip"));

            assert_eq!(
                find_in(tmp.path(), "Tetris Plus [T-5704H].zip", &fp, Some(&data)),
                Some(at)
            );
        }
    }
}
