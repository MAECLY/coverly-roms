//! Opt-in Switch icons (adjustments D8): fetched on demand from the
//! `iconUrl` the catalogue kept (only on `img-eshop.cdn.nintendo.net`, a
//! redirect only to the same host, at most 1 MiB), decoded as JPEG or PNG
//! with the image module's caps and re-encoded to a PNG of at most 256 px
//! in the art cache. They are shown only inside Coverly, through the
//! thumbnail IPC; Fill never writes them anywhere.

use std::io;
use std::path::{Path, PathBuf};

use crate::image::{process_image, Fit, ImageError, ImageLimits, OutputSpec};
use crate::net::{Http, HttpRequest, NetError};
use crate::profiles::ImageFormat;

/// The icons' folder inside the art cache.
pub const ICON_DIR: &str = "eshop-icons";

/// Largest icon download.
pub const MAX_ICON_BYTES: u64 = 1024 * 1024;

/// Longest side of a cached icon, in pixels.
pub const ICON_SIZE: u32 = 256;

/// Why an icon could not be fetched. Messages never carry the icon URL.
#[derive(Debug, thiserror::Error)]
pub enum IconError {
    #[error("icon request: {0}")]
    Net(NetError),
    #[error("icon request refused")]
    Refused,
    #[error("icon answered HTTP {0}")]
    Status(u16),
    #[error("icon image: {0}")]
    Image(#[from] ImageError),
    #[error("icon file: {0}")]
    Io(#[from] io::Error),
}

impl From<NetError> for IconError {
    fn from(e: NetError) -> IconError {
        match e {
            NetError::Status { status, .. } => IconError::Status(status),
            NetError::Insecure(_) | NetError::Unregistered(_) => IconError::Refused,
            other => IconError::Net(other),
        }
    }
}

/// The cached icon's file name for a title id.
pub fn file_name(tid: u64) -> String {
    format!("{tid:016X}.png")
}

/// Downloads `url`, re-encodes it and stores it in `dir` as
/// [`file_name`]`(tid)`. A 404 or 410 is `Ok(None)`.
pub(crate) fn fetch(
    http: &dyn Http,
    url: &str,
    dir: &Path,
    tid: u64,
) -> Result<Option<PathBuf>, IconError> {
    let req = HttpRequest::get(url)
        .max_bytes(MAX_ICON_BYTES)
        .same_host_redirects();
    let resp = http.get(&req)?;
    match resp.status {
        200..=299 => {}
        404 | 410 => return Ok(None),
        other => return Err(IconError::Status(other)),
    }
    // The transport caps the body; a test fake or wrapper may not.
    if resp.body.len() as u64 > MAX_ICON_BYTES {
        return Err(NetError::TooLarge {
            limit: MAX_ICON_BYTES,
        }
        .into());
    }
    match ::image::guess_format(&resp.body) {
        Ok(::image::ImageFormat::Jpeg | ::image::ImageFormat::Png) => {}
        Ok(other) => return Err(ImageError::Unsupported(format!("{other:?}")).into()),
        Err(_) => return Err(ImageError::NotAnImage.into()),
    }
    let spec = OutputSpec {
        format: ImageFormat::Png,
        max_size: Some((ICON_SIZE, ICON_SIZE)),
        fit: Fit::Contain,
        jpeg_quality: 90,
    };
    let limits = ImageLimits {
        max_input_bytes: MAX_ICON_BYTES as usize,
        ..ImageLimits::default()
    };
    let png = process_image(&resp.body, &spec, &limits)?;
    std::fs::create_dir_all(dir)?;
    let dest = dir.join(file_name(tid));
    // Written beside and renamed, so a concurrent reader never sees half.
    let tmp = dir.join(format!("{}.{}.tmp", file_name(tid), std::process::id()));
    std::fs::write(&tmp, &png.bytes)?;
    if let Err(e) = std::fs::rename(&tmp, &dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(Some(dest))
}

/// Deletes every cached icon; a missing folder is fine.
pub(crate) fn remove_all(dir: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    }
    // The art folder itself goes too when nothing else is in it.
    if let Some(parent) = dir.parent() {
        let _ = std::fs::remove_dir(parent);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_use_upper_case_hex() {
        assert_eq!(file_name(0x0100_0000_00aa_0000), "0100000000AA0000.png");
    }

    #[test]
    fn errors_never_carry_the_url() {
        let url = "https://img-eshop.cdn.nintendo.net/i/secret.jpg".to_string();
        for e in [
            NetError::Status {
                status: 500,
                url: url.clone(),
            },
            NetError::Unregistered(url.clone()),
        ] {
            assert!(!IconError::from(e).to_string().contains("secret"));
        }
    }

    #[test]
    fn removing_a_missing_folder_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        remove_all(&dir.path().join("art").join(ICON_DIR)).unwrap();
    }
}
