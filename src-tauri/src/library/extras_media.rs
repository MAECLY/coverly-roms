//! Extras shown or played in the app (part D D12, D17, P-6): a file named
//! by its handle, checked against the disk, then granted alone to the asset
//! protocol (media), read as text (at most [`TEXT_MAX`]), revealed, or
//! handed to another app when its type is allow-listed. No path from the
//! webview is ever read.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use coverly_core::archive::names::decode_zip_name;
use coverly_core::consoles::is_key_material;
use coverly_core::extras::ops::extras_dir;
use serde::Serialize;

use super::extras::{playable_of, ExtrasHandle, Playable};
use crate::error::IpcError;

/// The most bytes of a text extra shown (D17).
pub const TEXT_MAX: u64 = 1 << 20;

/// How the viewer shows a granted file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaKind {
    Audio,
    Video,
    Image,
    Pdf,
}

/// A media file granted to the asset protocol: `src_path` goes through
/// `convertFileSrc` (it is the checked canonical path, never sent back).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MediaView {
    pub src_path: String,
    pub mime: String,
    pub kind: MediaKind,
}

/// How a text extra was decoded (D17).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextEncoding {
    #[serde(rename = "utf-8")]
    Utf8,
    Cp437,
    #[serde(rename = "latin-1")]
    Latin1,
}

/// A text extra, shown as text (never as HTML).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TextView {
    pub text: String,
    /// Longer than [`TEXT_MAX`]: only the start is shown.
    pub truncated: bool,
    pub encoding: TextEncoding,
}

/// A modification time as the listing records it (the writer's rule:
/// negative before the epoch).
fn mtime_ns(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_nanos()).unwrap_or(i64::MAX),
    }
}

fn refused(why: &str) -> IpcError {
    IpcError::ExtrasRefused { why: why.into() }
}

fn ext_of(name: &str) -> Option<String> {
    name.rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map(|(_, e)| e.to_ascii_lowercase())
}

/// The viewer kind and MIME type of a media file the app plays (D17).
pub fn media_of(name: &str) -> Option<(MediaKind, &'static str)> {
    use MediaKind::{Audio, Image, Pdf, Video};
    Some(match ext_of(name)?.as_str() {
        "mp3" => (Audio, "audio/mpeg"),
        "m4a" => (Audio, "audio/mp4"),
        "aac" => (Audio, "audio/aac"),
        "wav" => (Audio, "audio/wav"),
        "flac" => (Audio, "audio/flac"),
        "ogg" => (Audio, "audio/ogg"),
        "opus" => (Audio, "audio/ogg; codecs=opus"),
        "mp4" | "m4v" => (Video, "video/mp4"),
        "webm" => (Video, "video/webm"),
        "mov" => (Video, "video/quicktime"),
        "png" => (Image, "image/png"),
        "jpg" | "jpeg" => (Image, "image/jpeg"),
        "gif" => (Image, "image/gif"),
        "webp" => (Image, "image/webp"),
        "avif" => (Image, "image/avif"),
        "bmp" => (Image, "image/bmp"),
        "svg" => (Image, "image/svg+xml"),
        "heic" => (Image, "image/heic"),
        "tif" | "tiff" => (Image, "image/tiff"),
        "pdf" => (Pdf, "application/pdf"),
        _ => return None,
    })
}

/// Whether the app shows a file as plain text (D17).
pub fn is_text(name: &str) -> bool {
    matches!(ext_of(name).as_deref(), Some("txt" | "md" | "nfo"))
}

/// The listed file below the game's `Extras/`, joined by components; a
/// listed path never holds `..`, but it is refused all the same.
fn listed_path(handle: &ExtrasHandle) -> Result<PathBuf, IpcError> {
    let mut path = extras_dir(&handle.game_dir);
    for part in handle.file.path.split('/') {
        let mut parts = Path::new(part).components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(n)), None) => path.push(n),
            _ => return Err(IpcError::OutsideLibrary(handle.file.path.clone())),
        }
    }
    Ok(path)
}

/// The game's `Extras/`, canonical and inside the library.
fn extras_canon(handle: &ExtrasHandle, root_canon: &Path) -> Result<PathBuf, IpcError> {
    let extras = extras_dir(&handle.game_dir);
    let canon = fs::canonicalize(&extras).map_err(|_| IpcError::ExtrasChanged)?;
    if canon.starts_with(root_canon) {
        Ok(canon)
    } else {
        Err(IpcError::OutsideLibrary(extras.display().to_string()))
    }
}

/// The handle's file as the listing showed it, checked: not key material
/// (never opened), not a symlink, still the same size and time, and,
/// resolved, a regular file under the game's canonical `Extras/` inside
/// the library root. Returns the canonical path.
pub fn checked_file(handle: &ExtrasHandle, root_canon: &Path) -> Result<PathBuf, IpcError> {
    let file = &handle.file;
    if is_key_material(&file.name) {
        return Err(refused("key-file"));
    }
    if file.link {
        return Err(refused("link"));
    }
    let path = listed_path(handle)?;
    let meta = fs::symlink_metadata(&path).map_err(|_| IpcError::ExtrasChanged)?;
    if meta.file_type().is_symlink() {
        return Err(refused("link"));
    }
    if !meta.is_file()
        || meta.len() != file.size
        || meta.modified().map_or(0, mtime_ns) != file.mtime_ns
    {
        return Err(IpcError::ExtrasChanged);
    }
    let extras = extras_canon(handle, root_canon)?;
    let canon = fs::canonicalize(&path).map_err(|_| IpcError::ExtrasChanged)?;
    if !canon.starts_with(&extras) || canon == extras {
        return Err(IpcError::OutsideLibrary(path.display().to_string()));
    }
    let name = canon
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(IpcError::ExtrasChanged)?;
    if is_key_material(name) {
        return Err(refused("key-file"));
    }
    if !fs::metadata(&canon).is_ok_and(|m| m.is_file()) {
        return Err(IpcError::ExtrasChanged);
    }
    Ok(canon)
}

/// A media file to grant: its canonical path and view (D12). Text,
/// packages and anything not played here are refused (`not-media`).
pub fn media_target(
    handle: &ExtrasHandle,
    root_canon: &Path,
) -> Result<(PathBuf, MediaView), IpcError> {
    let file = &handle.file;
    if is_key_material(&file.name) {
        return Err(refused("key-file"));
    }
    if file.link {
        return Err(refused("link"));
    }
    let (kind, mime) = media_of(&file.name)
        .filter(|_| playable_of(&file.name, file.link) == Playable::Inline)
        .ok_or_else(|| refused("not-media"))?;
    let canon = checked_file(handle, root_canon)?;
    Ok((
        canon.clone(),
        MediaView {
            src_path: url_path(&canon),
            mime: mime.to_string(),
            kind,
        },
    ))
}

/// The path the webview turns into an asset URL: on Windows without the
/// `\\?\` prefix `canonicalize` adds (the scope holds both spellings).
fn url_path(canon: &Path) -> String {
    let s = canon.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_string(),
        _ => s.into_owned(),
    }
}

/// A text extra (txt, md, nfo), at most [`TEXT_MAX`] bytes of it.
pub fn read_text(handle: &ExtrasHandle, root_canon: &Path) -> Result<TextView, IpcError> {
    if is_key_material(&handle.file.name) {
        return Err(refused("key-file"));
    }
    if !is_text(&handle.file.name) || handle.file.link {
        return Err(refused("not-text"));
    }
    let canon = checked_file(handle, root_canon)?;
    let mut bytes = Vec::new();
    fs::File::open(&canon)
        .and_then(|f| f.take(TEXT_MAX + 1).read_to_end(&mut bytes))
        .map_err(|e| IpcError::Io(e.to_string()))?;
    let truncated = bytes.len() as u64 > TEXT_MAX;
    bytes.truncate(usize::try_from(TEXT_MAX).unwrap_or(usize::MAX));
    let nfo = ext_of(&handle.file.name).as_deref() == Some("nfo");
    let (text, encoding) = decode_text(&bytes, truncated, nfo);
    Ok(TextView {
        text,
        truncated,
        encoding,
    })
}

/// UTF-8 (a character cut by the cap dropped), else CP437 for an `.nfo`,
/// else Latin-1 (D17). A leading BOM is dropped.
pub fn decode_text(bytes: &[u8], truncated: bool, nfo: bool) -> (String, TextEncoding) {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => return (s.to_string(), TextEncoding::Utf8),
        // Valid up to a sequence the cap cut short.
        Err(e) if truncated && e.error_len().is_none() => {
            let head = &bytes[..e.valid_up_to()];
            if let Ok(s) = std::str::from_utf8(head) {
                return (s.to_string(), TextEncoding::Utf8);
            }
        }
        Err(_) => {}
    }
    if nfo {
        // Never fails without the UTF-8 flag.
        if let Ok(s) = decode_zip_name(bytes, false, None) {
            return (s, TextEncoding::Cp437);
        }
    }
    (
        bytes.iter().map(|&b| char::from(b)).collect(),
        TextEncoding::Latin1,
    )
}

/// The canonical path of a file another app may open: only the types of
/// the allow-list (D17); packages, executables, scripts, shortcuts, links,
/// key material and unknown types are `reveal-only`.
pub fn open_target(handle: &ExtrasHandle, root_canon: &Path) -> Result<PathBuf, IpcError> {
    if playable_of(&handle.file.name, handle.file.link) == Playable::RevealOnly {
        return Err(refused("reveal-only"));
    }
    checked_file(handle, root_canon)
}

/// The listed path to show in the file manager (any file, a link
/// included: revealing runs nothing); its folder must still resolve under
/// the game's `Extras/` inside the library.
pub fn reveal_target(handle: &ExtrasHandle, root_canon: &Path) -> Result<PathBuf, IpcError> {
    let path = listed_path(handle)?;
    fs::symlink_metadata(&path).map_err(|_| IpcError::ExtrasChanged)?;
    let extras = extras_canon(handle, root_canon)?;
    let parent = path.parent().ok_or(IpcError::ExtrasChanged)?;
    let parent = fs::canonicalize(parent).map_err(|_| IpcError::ExtrasChanged)?;
    if !parent.starts_with(&extras) {
        return Err(IpcError::OutsideLibrary(path.display().to_string()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_types_follow_d17() {
        assert_eq!(
            media_of("01 Opening.FLAC"),
            Some((MediaKind::Audio, "audio/flac"))
        );
        assert_eq!(media_of("intro.m4v"), Some((MediaKind::Video, "video/mp4")));
        assert_eq!(
            media_of("scan.svg"),
            Some((MediaKind::Image, "image/svg+xml"))
        );
        assert_eq!(
            media_of("manual.pdf"),
            Some((MediaKind::Pdf, "application/pdf"))
        );
        for name in [
            "track.spc",
            "notes.txt",
            "movie.mkv",
            "theme.p3t",
            "pdf",
            ".png",
        ] {
            assert_eq!(media_of(name), None, "{name}");
        }
        assert!(is_text("readme.TXT") && is_text("guide.md") && is_text("x.nfo"));
        assert!(!is_text("page.html") && !is_text("txt"));
    }

    #[test]
    fn text_decoding_never_panics_on_any_cut() {
        let sample = "Guía ░ é\u{1F3AE}".as_bytes();
        for n in 0..=sample.len() {
            for nfo in [false, true] {
                let _ = decode_text(&sample[..n], true, nfo);
                let _ = decode_text(&sample[..n], false, nfo);
            }
        }
        let mut seed = 0x2545_f491_u32;
        let mut bytes = sample.to_vec();
        for _ in 0..2000 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let at = seed as usize % bytes.len();
            bytes[at] ^= (seed >> 8) as u8;
            let _ = decode_text(&bytes, seed.is_multiple_of(2), seed.is_multiple_of(3));
        }
    }

    #[test]
    fn a_verbatim_windows_path_loses_its_prefix_for_the_url() {
        assert_eq!(url_path(Path::new(r"\\?\C:\Lib\a.png")), r"C:\Lib\a.png");
        assert_eq!(
            url_path(Path::new(r"\\?\UNC\srv\a.png")),
            r"\\?\UNC\srv\a.png"
        );
        assert_eq!(url_path(Path::new("/lib/a.png")), "/lib/a.png");
    }
}
