//! Renames that never replace an existing file (spec §9): `std::fs::rename`
//! silently replaces on Unix and on Windows.
//!
//! Order of attempts: an exclusive rename (`renamex_np(RENAME_EXCL)` on
//! macOS, `renameat2(RENAME_NOREPLACE)` on Linux, `MoveFileExW` without
//! `MOVEFILE_REPLACE_EXISTING` on Windows); where the volume does not
//! support it, a hard link plus deleting the source; where links are not
//! supported either (FAT/exFAT), a last twin check followed by a plain
//! rename. That last step leaves a tiny race (another process creating the
//! target between the check and the rename), documented in the spec.

use std::io::{self, ErrorKind};
use std::path::Path;

/// Moves `from` to `to` on one volume, never replacing a file. Fails with
/// [`ErrorKind::AlreadyExists`] when `to`, or a twin of it under another
/// case or Unicode normalisation, already exists.
pub fn rename_noclobber(from: &Path, to: &Path) -> io::Result<()> {
    rename_noclobber_using(
        from,
        to,
        &|f, t| with_sharing_retries(|| exclusive_rename(f, t)),
        &|f, t| with_sharing_retries(|| std::fs::hard_link(f, t)),
    )
}

/// [`rename_noclobber`] for a caller that keeps a directory index and has
/// checked that no case/NFC twin of `to` exists: the folder is not listed
/// first (once per file is O(n²) for thousands of files landing in one
/// folder). The exclusive rename and the hard link still refuse an
/// existing `to`, and the FAT/exFAT fallback still checks for twins.
pub(crate) fn rename_noclobber_indexed(from: &Path, to: &Path) -> io::Result<()> {
    rename_noclobber_inner(
        from,
        to,
        false,
        &|f, t| with_sharing_retries(|| exclusive_rename(f, t)),
        &|f, t| with_sharing_retries(|| std::fs::hard_link(f, t)),
    )
}

/// [`rename_noclobber`] with the exclusive rename and the hard link
/// injected, so the fallbacks can be tested on any file system.
#[doc(hidden)]
pub fn rename_noclobber_using(
    from: &Path,
    to: &Path,
    exclusive: &dyn Fn(&Path, &Path) -> io::Result<()>,
    link: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    rename_noclobber_inner(from, to, true, exclusive, link)
}

fn rename_noclobber_inner(
    from: &Path,
    to: &Path,
    list_first: bool,
    exclusive: &dyn Fn(&Path, &Path) -> io::Result<()>,
    link: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    // A twin under another case/NFC spelling counts as taken on every
    // volume (the library may later be copied to FAT).
    if list_first && twin_exists(to)? {
        return Err(io::Error::from(ErrorKind::AlreadyExists));
    }
    match exclusive(from, to) {
        Ok(()) => return Ok(()),
        Err(e) if !is_unsupported(&e) => return Err(e),
        Err(_) => {}
    }
    match link(from, to) {
        Ok(()) => {
            return std::fs::remove_file(from).inspect_err(|_| {
                // Linked but the source stays: undo the link, never leave
                // two names for one file.
                let _ = std::fs::remove_file(to);
            });
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => return Err(e),
        Err(e) if !is_unsupported(&e) && e.kind() != ErrorKind::PermissionDenied => return Err(e),
        Err(_) => {}
    }
    // FAT/exFAT: no exclusive rename, no hard links. Last check, then rename.
    if twin_exists(to)? {
        return Err(io::Error::from(ErrorKind::AlreadyExists));
    }
    std::fs::rename(from, to)
}

/// Whether `to` or a case/NFC twin of it exists. A missing folder is not
/// an error here: the rename itself reports it.
fn twin_exists(to: &Path) -> io::Result<bool> {
    if std::fs::symlink_metadata(to).is_ok() {
        return Ok(true);
    }
    let (Some(dir), Some(name)) = (to.parent(), to.file_name().and_then(|n| n.to_str())) else {
        return Ok(false);
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    match super::find_twin(dir, name) {
        Ok(found) => Ok(found.is_some()),
        Err(super::WriteError::Io(e)) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(super::WriteError::Io(e)) => Err(e),
        // Keep the kind so callers still tell "denied" and "full" apart.
        Err(e @ super::WriteError::Denied(_)) => {
            Err(io::Error::new(ErrorKind::PermissionDenied, e.to_string()))
        }
        Err(e @ super::WriteError::DiskFull) => {
            Err(io::Error::new(ErrorKind::StorageFull, e.to_string()))
        }
        Err(e) => Err(io::Error::other(e.to_string())),
    }
}

/// Errors that mean "this volume cannot do that", not "this move failed".
fn is_unsupported(e: &io::Error) -> bool {
    if e.kind() == ErrorKind::Unsupported {
        return true;
    }
    match e.raw_os_error() {
        #[cfg(unix)]
        Some(code) => [libc::EINVAL, libc::ENOSYS, libc::ENOTSUP, libc::EOPNOTSUPP].contains(&code),
        // ERROR_INVALID_FUNCTION, ERROR_NOT_SUPPORTED
        #[cfg(windows)]
        Some(code) => code == 1 || code == 50,
        _ => false,
    }
}

#[cfg(target_os = "macos")]
fn exclusive_rename(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let f = CString::new(from.as_os_str().as_bytes())?;
    let t = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: two valid NUL-terminated paths that outlive the call.
    let rc = unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), libc::RENAME_EXCL) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn exclusive_rename(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let f = CString::new(from.as_os_str().as_bytes())?;
    let t = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: two valid NUL-terminated paths relative to the cwd fd.
    let rc = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            f.as_ptr(),
            libc::AT_FDCWD,
            t.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn exclusive_rename(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<u16>>()
    };
    let (f, t) = (wide(from), wide(to));
    // Flags 0: no MOVEFILE_REPLACE_EXISTING, no MOVEFILE_COPY_ALLOWED.
    // SAFETY: two valid NUL-terminated UTF-16 paths that outlive the call.
    let ok = unsafe { MoveFileExW(f.as_ptr(), t.as_ptr(), 0) };
    if ok != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn exclusive_rename(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::from(ErrorKind::Unsupported))
}

/// Whether `a` and `b` (or their nearest existing ancestors) are on one
/// volume, so a move can be a rename instead of copy + delete.
pub fn same_volume(a: &Path, b: &Path) -> bool {
    let existing = |p: &Path| {
        p.ancestors()
            .find(|q| !q.as_os_str().is_empty() && q.exists())
            .map(Path::to_path_buf)
    };
    let (Some(a), Some(b)) = (existing(a), existing(b)) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        matches!(
            (std::fs::metadata(&a), std::fs::metadata(&b)),
            (Ok(x), Ok(y)) if x.dev() == y.dev()
        )
    }
    #[cfg(windows)]
    {
        // O-D3: compare the drive letter / UNC share.
        let prefix = |p: &Path| {
            std::path::absolute(p).ok().and_then(|p| {
                p.components()
                    .next()
                    .map(|c| c.as_os_str().to_ascii_lowercase())
            })
        };
        let pa = prefix(&a);
        pa.is_some() && pa == prefix(&b)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (a, b);
        false
    }
}

/// Windows sharing/lock violations (antivirus, indexer): up to 3 retries
/// with 200/500/1000 ms back-off. Other systems call `f` once.
pub(crate) fn with_sharing_retries<T>(f: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    #[cfg(windows)]
    {
        use std::time::Duration;
        retry_when(
            f,
            is_sharing_violation,
            &[
                Duration::from_millis(200),
                Duration::from_millis(500),
                Duration::from_millis(1000),
            ],
        )
    }
    #[cfg(not(windows))]
    {
        let mut f = f;
        f()
    }
}

/// ERROR_SHARING_VIOLATION (32), ERROR_LOCK_VIOLATION (33).
#[cfg(any(windows, test))]
fn is_sharing_violation(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32 | 33))
}

/// Calls `f`, and again after each wait while it fails with a retryable
/// error; the last attempt's result is returned as is.
#[cfg(any(windows, test))]
fn retry_when<T>(
    mut f: impl FnMut() -> io::Result<T>,
    retryable: impl Fn(&io::Error) -> bool,
    waits: &[std::time::Duration],
) -> io::Result<T> {
    for wait in waits {
        match f() {
            Err(e) if retryable(&e) => std::thread::sleep(*wait),
            other => return other,
        }
    }
    f()
}

/// A rename across volumes: `EXDEV` (18) on Unix, `ERROR_NOT_SAME_DEVICE`
/// (17) on Windows. The caller falls back to copy + verify + delete.
pub(crate) fn is_cross_device(e: &io::Error) -> bool {
    #[cfg(unix)]
    const CODE: Option<i32> = Some(libc::EXDEV);
    #[cfg(windows)]
    const CODE: Option<i32> = Some(17);
    #[cfg(not(any(unix, windows)))]
    const CODE: Option<i32> = None;
    e.kind() == ErrorKind::CrossesDevices || (CODE.is_some() && e.raw_os_error() == CODE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::time::Duration;

    #[test]
    fn cross_device_errors_are_recognised() {
        #[cfg(unix)]
        assert!(is_cross_device(&io::Error::from_raw_os_error(libc::EXDEV)));
        #[cfg(windows)]
        assert!(is_cross_device(&io::Error::from_raw_os_error(17)));
        assert!(is_cross_device(&io::Error::from(ErrorKind::CrossesDevices)));
        assert!(!is_cross_device(&io::Error::from(ErrorKind::NotFound)));
    }

    #[test]
    fn unsupported_errors_are_recognised() {
        assert!(is_unsupported(&io::Error::from(ErrorKind::Unsupported)));
        #[cfg(unix)]
        assert!(is_unsupported(&io::Error::from_raw_os_error(libc::ENOTSUP)));
        assert!(!is_unsupported(&io::Error::from(ErrorKind::AlreadyExists)));
    }

    #[test]
    fn sharing_violations_are_retried_three_times_then_returned() {
        let calls = Cell::new(0);
        let waits = [Duration::ZERO; 3];
        let result: io::Result<()> = retry_when(
            || {
                calls.set(calls.get() + 1);
                Err(io::Error::from_raw_os_error(32))
            },
            is_sharing_violation,
            &waits,
        );
        assert_eq!(result.unwrap_err().raw_os_error(), Some(32));
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn a_retry_that_succeeds_stops_retrying() {
        let calls = Cell::new(0);
        let waits = [Duration::ZERO; 3];
        let result = retry_when(
            || {
                calls.set(calls.get() + 1);
                if calls.get() < 2 {
                    Err(io::Error::from_raw_os_error(33))
                } else {
                    Ok(7)
                }
            },
            is_sharing_violation,
            &waits,
        );
        assert_eq!(result.unwrap(), 7);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn other_errors_are_not_retried() {
        let calls = Cell::new(0);
        let result: io::Result<()> = retry_when(
            || {
                calls.set(calls.get() + 1);
                Err(io::Error::from(ErrorKind::NotFound))
            },
            is_sharing_violation,
            &[Duration::ZERO; 3],
        );
        assert!(result.is_err());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn without_windows_the_operation_runs_once() {
        let calls = Cell::new(0);
        let _ = with_sharing_retries(|| {
            calls.set(calls.get() + 1);
            Err::<(), _>(io::Error::from(ErrorKind::NotFound))
        });
        assert_eq!(calls.get(), 1);
    }
}
