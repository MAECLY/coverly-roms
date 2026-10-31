//! Disc sheets: `.cue`, `.gdi` and `.m3u`, parsed from bytes with the
//! track details a disc archive needs (track modes of a cue, track numbers
//! and LBAs of a gdi), and the Redump track pattern used to infer a set
//! whose sheet cannot be read yet (spec §6.2).
//!
//! File names come from the `scan::multifile` parsers, so loose sets and
//! sets inside archives read a sheet the same way.

use serde::{Deserialize, Serialize};

use crate::archive::names::name_key;
use crate::scan::multifile::{parse_cue, parse_gdi, parse_m3u, tokenize};

/// Largest sheet parsed (spec §6.2); a larger one is not a disc sheet.
pub const MAX_SHEET_BYTES: usize = 64 * 1024;

/// Most tracks (and files) one sheet may list: a CD holds 99 tracks.
pub const MAX_TRACKS: usize = 99;

/// The kind of sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SheetKind {
    Cue,
    Gdi,
    M3u,
}

impl SheetKind {
    /// The sheet kind of extension `ext` (any case, no dot).
    pub fn from_extension(ext: &str) -> Option<SheetKind> {
        match ext.to_ascii_lowercase().as_str() {
            "cue" => Some(SheetKind::Cue),
            "gdi" => Some(SheetKind::Gdi),
            "m3u" => Some(SheetKind::M3u),
            _ => None,
        }
    }
}

/// What a track holds, as its sheet states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrackMode {
    /// CD audio (`AUDIO` in a cue, type 0 in a gdi).
    Audio,
    /// `MODE1/…` in a cue.
    Mode1,
    /// `MODE2/…` in a cue.
    Mode2,
    /// A data track whose mode the sheet does not state (type 4 in a gdi).
    Data,
    /// Anything else (`CDG`, `CDI/2352`, unknown gdi types).
    Other,
}

impl TrackMode {
    /// Whether the track holds data sectors.
    pub fn is_data(self) -> bool {
        matches!(self, TrackMode::Mode1 | TrackMode::Mode2 | TrackMode::Data)
    }
}

/// One track of a sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackInfo {
    /// Track number, 1–99.
    pub number: u8,
    pub mode: TrackMode,
    /// Stored bytes per sector, when the sheet states it.
    pub sector_size: Option<u16>,
    /// Index into [`Sheet::files`] of the file holding the track.
    pub file: usize,
    /// Start LBA (gdi only).
    pub lba: Option<u32>,
    /// Whether the track lies in a GD-ROM high-density area: after a
    /// `REM HIGH-DENSITY AREA` line of a cue, or at LBA 45000 or later in a
    /// gdi. The Dreamcast IP.BIN is in the first HD data track.
    #[serde(default)]
    pub high_density: bool,
    /// The cue session holding the track: 1, or the number of the last
    /// `REM SESSION nn` line before it (Redump multi-session cues). A
    /// gdi has one session.
    #[serde(default = "first_session", skip_serializing_if = "is_first_session")]
    pub session: u8,
}

fn first_session() -> u8 {
    1
}

fn is_first_session(n: &u8) -> bool {
    *n == 1
}

/// A parsed sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sheet {
    pub kind: SheetKind,
    /// Referenced file names, in sheet order, as written (unresolved).
    pub files: Vec<String>,
    /// Tracks in sheet order (empty for an m3u).
    pub tracks: Vec<TrackInfo>,
}

/// Why a sheet could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SheetError {
    #[error("sheet is {len} bytes, more than {MAX_SHEET_BYTES}")]
    TooLarge { len: usize },
    #[error("sheet is not text")]
    NotText,
    #[error("sheet line {line} is malformed")]
    Malformed { line: usize },
}

/// Parses a sheet of `kind` from its bytes (at most [`MAX_SHEET_BYTES`];
/// UTF-8, else Latin-1).
pub fn parse_sheet(kind: SheetKind, text: &[u8]) -> Result<Sheet, SheetError> {
    if text.len() > MAX_SHEET_BYTES {
        return Err(SheetError::TooLarge { len: text.len() });
    }
    if text.contains(&0) {
        return Err(SheetError::NotText);
    }
    let text = match std::str::from_utf8(text) {
        Ok(t) => t.to_owned(),
        // Latin-1: every byte is the code point of the same value.
        Err(_) => text.iter().map(|&b| char::from(b)).collect(),
    };
    let sheet = match kind {
        SheetKind::Cue => cue_sheet(&text)?,
        SheetKind::Gdi => gdi_sheet(&text)?,
        SheetKind::M3u => Sheet {
            kind,
            files: parse_m3u(&text),
            tracks: Vec::new(),
        },
    };
    if sheet.files.len() > MAX_TRACKS {
        return Err(SheetError::Malformed {
            line: text.lines().count(),
        });
    }
    Ok(sheet)
}

/// Lines with their 1-based numbers, BOM and surrounding blanks removed.
fn numbered_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim().trim_start_matches('\u{feff}').trim()))
}

fn cue_sheet(text: &str) -> Result<Sheet, SheetError> {
    let mut files: Vec<String> = Vec::new();
    let mut tracks = Vec::new();
    let mut high_density = false;
    let mut session = 1;
    for (line, l) in numbered_lines(text) {
        if let Some(area) = density_marker(l) {
            high_density = area;
            continue;
        }
        if let Some(n) = session_marker(l) {
            session = n;
            continue;
        }
        // A FILE line, exactly as the loose-set parser reads it.
        if let Some(name) = parse_cue(l).into_iter().next() {
            files.push(name);
            continue;
        }
        let mut words = l.split_whitespace();
        if !words
            .next()
            .is_some_and(|w| w.eq_ignore_ascii_case("TRACK"))
        {
            continue;
        }
        let bad = SheetError::Malformed { line };
        let (Some(file), Some(number), Some(mode)) = (
            files.len().checked_sub(1),
            words.next().and_then(track_number),
            words.next(),
        ) else {
            return Err(bad);
        };
        let (mode, sector_size) = cue_mode(mode).ok_or(bad)?;
        if tracks.len() == MAX_TRACKS {
            return Err(SheetError::Malformed { line });
        }
        tracks.push(TrackInfo {
            number,
            mode,
            sector_size,
            file,
            lba: None,
            high_density,
            session,
        });
    }
    Ok(Sheet {
        kind: SheetKind::Cue,
        files,
        tracks,
    })
}

/// First LBA of the GD-ROM high-density area.
pub const HD_AREA_LBA: u32 = 45_000;

/// Stored sector sizes a sheet may state (`MODE1/2048`, `MODE2/2324`,
/// `MODE2/2336`, raw 2352, `CDG` 2448). Anything else (0, 4096…) is not a
/// CD sector and the sheet is malformed.
const SECTOR_SIZES: [u16; 5] = [2048, 2324, 2336, 2352, 2448];

fn known_sector_size(size: u16) -> bool {
    SECTOR_SIZES.contains(&size)
}

/// `Some(true)` for `REM HIGH-DENSITY AREA`, `Some(false)` for
/// `REM SINGLE-DENSITY AREA` (any case and spacing), else `None`.
fn density_marker(line: &str) -> Option<bool> {
    let mut words = line.split_whitespace();
    if !words.next()?.eq_ignore_ascii_case("REM") {
        return None;
    }
    let area = words.next()?;
    let rest = words.next()?;
    if !rest.eq_ignore_ascii_case("AREA") || words.next().is_some() {
        return None;
    }
    if area.eq_ignore_ascii_case("HIGH-DENSITY") {
        Some(true)
    } else if area.eq_ignore_ascii_case("SINGLE-DENSITY") {
        Some(false)
    } else {
        None
    }
}

/// `n` of a `REM SESSION nn` line (any case), 1–99; else `None`.
fn session_marker(line: &str) -> Option<u8> {
    let mut words = line.split_whitespace();
    if !words.next()?.eq_ignore_ascii_case("REM") || !words.next()?.eq_ignore_ascii_case("SESSION")
    {
        return None;
    }
    let n = track_number(words.next()?)?;
    words.next().is_none().then_some(n)
}

/// Index into [`Sheet::tracks`] of the first track of session `n` of a
/// multi-session cue; `None` when no `REM SESSION` line names `n`.
pub fn first_of_session(sheet: &Sheet, n: u8) -> Option<usize> {
    sheet.tracks.iter().position(|t| t.session == n)
}

/// A track number 1–99.
fn track_number(s: &str) -> Option<u8> {
    s.parse::<u8>()
        .ok()
        .filter(|n| (1..=MAX_TRACKS as u8).contains(n))
}

/// The mode and sector size of a cue `TRACK` type (`MODE1/2352`, `AUDIO`).
/// `None` when a stated sector size is not a number.
fn cue_mode(word: &str) -> Option<(TrackMode, Option<u16>)> {
    let (name, size) = match word.split_once('/') {
        Some((name, size)) => (
            name,
            Some(size.parse::<u16>().ok().filter(|&s| known_sector_size(s))?),
        ),
        None => (word, None),
    };
    let mode = match name.to_ascii_uppercase().as_str() {
        "AUDIO" => TrackMode::Audio,
        "MODE1" => TrackMode::Mode1,
        "MODE2" => TrackMode::Mode2,
        _ => TrackMode::Other,
    };
    let size = size.or(match name.to_ascii_uppercase().as_str() {
        "AUDIO" => Some(2352),
        "CDG" => Some(2448),
        _ => None,
    });
    Some((mode, size))
}

fn gdi_sheet(text: &str) -> Result<Sheet, SheetError> {
    let mut lines = numbered_lines(text).filter(|(_, l)| !l.is_empty());
    let (count_line, count) = match lines.next() {
        Some((line, l)) => (
            line,
            l.parse::<usize>()
                .ok()
                .filter(|n| *n <= MAX_TRACKS)
                .ok_or(SheetError::Malformed { line })?,
        ),
        None => (1, 0),
    };
    let mut tracks = Vec::new();
    for (line, l) in lines {
        let bad = SheetError::Malformed { line };
        let cols = tokenize(l);
        if cols.len() < 5 || cols[4].is_empty() || tracks.len() == MAX_TRACKS {
            return Err(bad);
        }
        let number = track_number(&cols[0]).ok_or(bad.clone())?;
        let lba = cols[1].parse::<u32>().map_err(|_| bad.clone())?;
        let mode = match cols[2].parse::<u8>().map_err(|_| bad.clone())? {
            0 => TrackMode::Audio,
            4 => TrackMode::Data,
            _ => TrackMode::Other,
        };
        let sector_size = cols[3]
            .parse::<u16>()
            .ok()
            .filter(|&s| known_sector_size(s))
            .ok_or(bad.clone())?;
        tracks.push(TrackInfo {
            number,
            mode,
            sector_size: Some(sector_size),
            file: tracks.len(),
            lba: Some(lba),
            high_density: lba >= HD_AREA_LBA,
            session: 1,
        });
    }
    let files = parse_gdi(text);
    if tracks.len() != count || files.len() != count {
        return Err(SheetError::Malformed { line: count_line });
    }
    Ok(Sheet {
        kind: SheetKind::Gdi,
        files,
        tracks,
    })
}

/// Index into [`Sheet::tracks`] of the primary data track: the first
/// `MODE1`/`MODE2` track of a cue (of its high-density area when the cue
/// marks one, as Redump Dreamcast cues do: the low-density track 01 may
/// repeat IP.BIN but holds a different, small volume, so the fingerprint
/// comes from track 03), track 03 of a gdi. `None` for an m3u or a sheet without one.
pub fn primary_track(sheet: &Sheet) -> Option<usize> {
    match sheet.kind {
        SheetKind::Cue => {
            let gd_rom = sheet.tracks.iter().any(|t| t.high_density);
            sheet.tracks.iter().position(|t| {
                matches!(t.mode, TrackMode::Mode1 | TrackMode::Mode2) && (!gd_rom || t.high_density)
            })
        }
        SheetKind::Gdi => sheet
            .tracks
            .iter()
            .position(|t| t.number == 3)
            .filter(|&i| sheet.tracks[i].mode.is_data()),
        SheetKind::M3u => None,
    }
}

/// The tracks of a Redump-style set inferred from an archive listing when
/// its sheet `sheet_name` cannot be read yet (spec §6.2).
///
/// The tracks are the files in the sheet's directory named
/// `<stem> (Track NN).bin` or, below ten tracks, `<stem> (Track N).bin`
/// (one width per set) (or the one file `<stem>.bin` of a single-track
/// cue; or `trackNN.bin`/`trackNN.raw` beside a `.gdi`), numbered
/// contiguously from 01. Every other file in that directory must be the
/// sheet itself or its `.cue`/`.gdi` twin of the same stem; otherwise
/// `None`. Names compare case-insensitively and NFC-folded and come back
/// as listed, in track order. Directory entries (ending in `/`) are
/// skipped.
pub fn infer_redump_set(listing: &[&str], sheet_name: &str) -> Option<Vec<String>> {
    let (dir, file) = split_dir(sheet_name);
    let (stem, ext) = file.rsplit_once('.')?;
    let kind = SheetKind::from_extension(ext)?;
    if kind == SheetKind::M3u {
        return None;
    }
    let dir_key = name_key(dir);
    let stem_key = name_key(stem);
    let sheet_key = name_key(sheet_name);

    let mut redump: Vec<(u8, &str)> = Vec::new();
    let mut redump_width: Option<usize> = None;
    let mut gdi_style: Vec<(u8, &str)> = Vec::new();
    let mut single: Vec<&str> = Vec::new();
    for &name in listing {
        if name.ends_with('/') {
            continue;
        }
        let (d, f) = split_dir(name);
        if name_key(d) != dir_key || name_key(name) == sheet_key {
            continue;
        }
        let (fstem, fext) = f.rsplit_once('.')?;
        let fext = fext.to_ascii_lowercase();
        let same_stem = name_key(fstem) == stem_key;
        if same_stem && matches!(fext.as_str(), "cue" | "gdi") {
            continue; // the sheet's twin
        }
        if fext == "bin" {
            if let Some((n, width)) = redump_track(fstem, &stem_key) {
                // One digit width per set (Redump: `(Track N)` below ten
                // tracks, `(Track NN)` from ten).
                if *redump_width.get_or_insert(width) != width {
                    return None;
                }
                redump.push((n, name));
                continue;
            }
            if same_stem {
                single.push(name);
                continue;
            }
        }
        match gdi_track(fstem, &fext) {
            Some(n) if kind == SheetKind::Gdi => gdi_style.push((n, name)),
            _ => return None,
        }
    }

    let mut tracks = match (redump.is_empty(), gdi_style.is_empty(), single.as_slice()) {
        (false, true, []) => redump,
        (true, false, []) => gdi_style,
        (true, true, [one]) if kind == SheetKind::Cue => return Some(vec![(*one).to_owned()]),
        _ => return None,
    };
    tracks.sort_by_key(|&(n, _)| n);
    let contiguous = tracks
        .iter()
        .enumerate()
        .all(|(i, &(n, _))| usize::from(n) == i + 1);
    contiguous.then(|| tracks.into_iter().map(|(_, n)| n.to_owned()).collect())
}

/// `(directory, file name)` of a `/`-separated name; the directory is empty
/// at the archive root.
fn split_dir(name: &str) -> (&str, &str) {
    name.rsplit_once('/').unwrap_or(("", name))
}

/// `(N, digits)` of `<stem> (Track N)` / `<stem> (Track NN)`: one or two
/// digits, 1–99.
fn redump_track(fstem: &str, stem_key: &str) -> Option<(u8, usize)> {
    let (head, tail) = fstem.rsplit_once(" (")?;
    let tail = tail.strip_suffix(')')?;
    let (word, digits) = tail.split_at_checked(6)?;
    if !word.eq_ignore_ascii_case("Track ") || name_key(head) != stem_key {
        return None;
    }
    let n = match digits.len() {
        1 => digits.parse::<u8>().ok().filter(|&n| n >= 1),
        2 => two_digits(digits),
        _ => None,
    }?;
    digits
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then_some((n, digits.len()))
}

/// `NN` of `trackNN.bin` / `trackNN.raw` (any case).
fn gdi_track(fstem: &str, fext: &str) -> Option<u8> {
    if !matches!(fext, "bin" | "raw") {
        return None;
    }
    let (word, digits) = fstem.split_at_checked(5)?;
    if !word.eq_ignore_ascii_case("track") {
        return None;
    }
    two_digits(digits)
}

fn two_digits(s: &str) -> Option<u8> {
    (s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
        .filter(|&n| n >= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(text: &str) -> Sheet {
        parse_sheet(SheetKind::Cue, text.as_bytes()).unwrap()
    }

    #[test]
    fn cue_tracks_and_modes() {
        let sheet = cue("\u{feff}REM DISC\r\n\
             FILE \"Game (USA) (Track 01).bin\" BINARY\r\n\
             \x20 TRACK 01 MODE2/2352\r\n\
             \x20   INDEX 01 00:00:00\r\n\
             FILE \"Game (USA) (Track 02).bin\" BINARY\r\n\
             \x20 TRACK 02 AUDIO\r\n\
             \x20   INDEX 00 00:00:00\r\n\
             \x20 TRACK 03 MODE1/2048\r\n\
             FILE plain.img BINARY\r\n\
             \x20 track 04 cdg\r\n");
        assert_eq!(sheet.kind, SheetKind::Cue);
        assert_eq!(
            sheet.files,
            [
                "Game (USA) (Track 01).bin",
                "Game (USA) (Track 02).bin",
                "plain.img"
            ]
        );
        let t: Vec<_> = sheet
            .tracks
            .iter()
            .map(|t| (t.number, t.mode, t.sector_size, t.file, t.lba))
            .collect();
        assert_eq!(
            t,
            [
                (1, TrackMode::Mode2, Some(2352), 0, None),
                (2, TrackMode::Audio, Some(2352), 1, None),
                (3, TrackMode::Mode1, Some(2048), 1, None),
                (4, TrackMode::Other, Some(2448), 2, None),
            ]
        );
        // The file list is exactly what the loose-set parser reads.
        let text =
            "FILE \"a.bin\" BINARY\n TRACK 01 MODE1/2352\nFILE b.bin BINARY\n TRACK 02 AUDIO\n";
        assert_eq!(cue(text).files, parse_cue(text));
        assert_eq!(primary_track(&sheet), Some(0));

        // Audio first: the primary track is the first data track.
        let audio_first = cue("FILE a.bin BINARY\n TRACK 01 AUDIO\n TRACK 02 MODE1/2352\n");
        assert_eq!(primary_track(&audio_first), Some(1));
        assert_eq!(
            primary_track(&cue("FILE a.bin BINARY\n TRACK 01 AUDIO\n")),
            None
        );

        for bad in [
            " TRACK 01 MODE1/2352\n",                    // track before any FILE
            "FILE a.bin BINARY\n TRACK 00 MODE1/2352\n", // track 0
            "FILE a.bin BINARY\n TRACK 100 AUDIO\n",     // past 99
            "FILE a.bin BINARY\n TRACK xx AUDIO\n",
            "FILE a.bin BINARY\n TRACK 01\n",
            "FILE a.bin BINARY\n TRACK 01 MODE1/abc\n",
        ] {
            assert!(
                matches!(
                    parse_sheet(SheetKind::Cue, bad.as_bytes()),
                    Err(SheetError::Malformed { .. })
                ),
                "{bad:?}"
            );
        }
        assert_eq!(
            parse_sheet(
                SheetKind::Cue,
                b"FILE a.bin BINARY\n TRACK 02 x\n TRACK 01 MODE1/99999\n"
            ),
            Err(SheetError::Malformed { line: 3 })
        );
    }

    #[test]
    fn cue_latin1_and_binary() {
        // "Pokémon.bin" in Latin-1 decodes to the same name as in UTF-8.
        let latin1 = b"FILE \"Pok\xe9mon.bin\" BINARY\n TRACK 01 MODE1/2352\n";
        let sheet = parse_sheet(SheetKind::Cue, latin1).unwrap();
        assert_eq!(sheet.files, ["Pokémon.bin"]);
        assert_eq!(
            parse_sheet(SheetKind::Cue, b"FILE a\0.bin BINARY\n"),
            Err(SheetError::NotText)
        );
    }

    #[test]
    fn gdi_tracks_and_track03_primary() {
        let gdi = "3\r\n\
                   1 0 4 2352 track01.bin 0\r\n\
                   2 756 0 2352 \"track 02.raw\" 0\r\n\
                   3 45000 4 2352 track03.bin 0\r\n";
        let sheet = parse_sheet(SheetKind::Gdi, gdi.as_bytes()).unwrap();
        assert_eq!(sheet.files, parse_gdi(gdi));
        assert_eq!(sheet.files, ["track01.bin", "track 02.raw", "track03.bin"]);
        let t: Vec<_> = sheet
            .tracks
            .iter()
            .map(|t| (t.number, t.mode, t.sector_size, t.file, t.lba))
            .collect();
        assert_eq!(
            t,
            [
                (1, TrackMode::Data, Some(2352), 0, Some(0)),
                (2, TrackMode::Audio, Some(2352), 1, Some(756)),
                (3, TrackMode::Data, Some(2352), 2, Some(45000)),
            ]
        );
        // Track 03, not the low-density data track 01.
        assert_eq!(primary_track(&sheet), Some(2));

        // Without a track 03 (or with an audio one) there is no primary.
        let two = parse_sheet(
            SheetKind::Gdi,
            b"2\n1 0 4 2352 a.bin 0\n2 600 0 2352 b.raw 0\n",
        )
        .unwrap();
        assert_eq!(primary_track(&two), None);

        for bad in [
            "2\n1 0 4 2352 a.bin 0\n",    // count mismatch
            "x\n1 0 4 2352 a.bin 0\n",    // count not a number
            "1\n1 0 4 2352\n",            // no file column
            "1\n1 zero 4 2352 a.bin 0\n", // lba not a number
            "1\n0 0 4 2352 a.bin 0\n",    // track 0
            "1\n1 0 4 2352 \"\" 0\n",     // empty name
            "100\n",                      // more than 99
        ] {
            assert!(
                matches!(
                    parse_sheet(SheetKind::Gdi, bad.as_bytes()),
                    Err(SheetError::Malformed { .. })
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn m3u_lines() {
        let m3u = "#EXTM3U\n\nGame (Disc 1).cue\r\n  sub/Game (Disc 2).cue  \n# comment\n";
        let sheet = parse_sheet(SheetKind::M3u, m3u.as_bytes()).unwrap();
        assert_eq!(sheet.kind, SheetKind::M3u);
        assert_eq!(sheet.files, ["Game (Disc 1).cue", "sub/Game (Disc 2).cue"]);
        assert!(sheet.tracks.is_empty());
        assert_eq!(primary_track(&sheet), None);
        let many = "a.cue\n".repeat(MAX_TRACKS + 1);
        assert!(matches!(
            parse_sheet(SheetKind::M3u, many.as_bytes()),
            Err(SheetError::Malformed { .. })
        ));
    }

    #[test]
    fn sheet_over_64k_rejected() {
        let mut text = b"FILE a.bin BINARY\n TRACK 01 MODE1/2352\n".to_vec();
        text.resize(MAX_SHEET_BYTES, b' ');
        assert!(parse_sheet(SheetKind::Cue, &text).is_ok());
        text.push(b'\n');
        for kind in [SheetKind::Cue, SheetKind::Gdi, SheetKind::M3u] {
            assert_eq!(
                parse_sheet(kind, &text),
                Err(SheetError::TooLarge {
                    len: MAX_SHEET_BYTES + 1
                })
            );
        }
    }

    #[test]
    fn sheet_kinds_from_extension() {
        assert_eq!(SheetKind::from_extension("CUE"), Some(SheetKind::Cue));
        assert_eq!(SheetKind::from_extension("gdi"), Some(SheetKind::Gdi));
        assert_eq!(SheetKind::from_extension("M3u"), Some(SheetKind::M3u));
        assert_eq!(SheetKind::from_extension("bin"), None);
    }

    fn redump(dir: &str, stem: &str, n: u8) -> Vec<String> {
        (1..=n)
            .map(|i| format!("{dir}{stem} (Track {i:02}).bin"))
            .collect()
    }

    #[test]
    fn infer_redump_cue_set_contiguous() {
        // The real case: the sheet is listed after its tracks, in a folder.
        let dir = "Tetris Plus (USA)/";
        let mut names = redump(dir, "Tetris Plus (USA)", 51);
        names.reverse();
        names.push(format!("{dir}Tetris Plus (USA).cue"));
        names.push("Other/readme.txt".into());
        let listing: Vec<&str> = names.iter().map(String::as_str).collect();
        let set = infer_redump_set(&listing, "Tetris Plus (USA)/Tetris Plus (USA).cue").unwrap();
        assert_eq!(set, redump(dir, "Tetris Plus (USA)", 51));

        // Case and NFC differences do not matter; names come back as listed.
        let listing = [
            "Poke\u{301}mon (TRACK 01).BIN",
            "Poke\u{301}mon (Track 02).bin",
            "pokémon.cue",
        ];
        assert_eq!(
            infer_redump_set(&listing, "pokémon.cue").unwrap(),
            [
                "Poke\u{301}mon (TRACK 01).BIN",
                "Poke\u{301}mon (Track 02).bin"
            ]
        );

        // A single-track cue set is the one `<stem>.bin`.
        assert_eq!(
            infer_redump_set(&["Game.bin", "Game.cue"], "Game.cue").unwrap(),
            ["Game.bin"]
        );

        // The Dreamcast real case: a cue and a gdi twin over Redump tracks.
        let dir = "Next Tetris, The - On-Line Edition (USA)/";
        let stem = "Next Tetris, The - On-Line Edition (USA)";
        let mut names = redump(dir, stem, 14);
        names.push(format!("{dir}{stem}.cue"));
        names.push(format!("{dir}{stem}.gdi"));
        let listing: Vec<&str> = names.iter().map(String::as_str).collect();
        for sheet in [&names[14], &names[15]] {
            assert_eq!(
                infer_redump_set(&listing, sheet).unwrap(),
                redump(dir, stem, 14),
                "{sheet}"
            );
        }
        // A directory entry in the listing is ignored.
        let mut with_dir = listing.clone();
        with_dir.push(dir);
        assert!(infer_redump_set(&with_dir, &names[14]).is_some());
    }

    #[test]
    fn infer_redump_gap_rejected() {
        let mut names = redump("", "Game", 4);
        names.remove(2); // no Track 03
        names.push("Game.cue".into());
        let listing: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(infer_redump_set(&listing, "Game.cue"), None);

        // Not starting at 01, duplicated, or with three digits.
        for listing in [
            &["Game (Track 02).bin", "Game.cue"][..],
            &["Game (Track 01).bin", "game (track 01).bin", "Game.cue"][..],
            &["Game (Track 1).bin", "Game (Track 01).bin", "Game.cue"][..],
            &["Game (Track 001).bin", "Game.cue"][..],
            &["Game (Track 0).bin", "Game.cue"][..],
        ] {
            assert_eq!(infer_redump_set(listing, "Game.cue"), None, "{listing:?}");
        }
        // Mixed patterns, or tracks of another stem.
        assert_eq!(
            infer_redump_set(&["Game (Track 01).bin", "Game.bin", "Game.cue"], "Game.cue"),
            None
        );
        assert_eq!(
            infer_redump_set(&["Other (Track 01).bin", "Game.cue"], "Game.cue"),
            None
        );
        // No tracks at all, an m3u, or a name that is not a sheet.
        assert_eq!(infer_redump_set(&["Game.cue"], "Game.cue"), None);
        assert_eq!(
            infer_redump_set(&["Game (Track 01).bin", "Game.m3u"], "Game.m3u"),
            None
        );
        assert_eq!(
            infer_redump_set(&["Game (Track 01).bin"], "Game (Track 01).bin"),
            None
        );
    }

    /// The Dreamcast real case (`Next Tetris, The - On-Line Edition (USA)`):
    /// its Redump cue, as stored in the archive.
    const DC_CUE: &str = "REM SINGLE-DENSITY AREA\r\n\
        FILE \"T (Track 01).bin\" BINARY\r\n  TRACK 01 MODE1/2352\r\n    INDEX 01 00:00:00\r\n\
        FILE \"T (Track 02).bin\" BINARY\r\n  TRACK 02 AUDIO\r\n    INDEX 01 00:02:00\r\n\
        REM HIGH-DENSITY AREA\r\n\
        FILE \"T (Track 03).bin\" BINARY\r\n  TRACK 03 MODE1/2352\r\n    INDEX 01 00:00:00\r\n\
        FILE \"T (Track 04).bin\" BINARY\r\n  TRACK 04 AUDIO\r\n    INDEX 01 00:02:00\r\n\
        FILE \"T (Track 05).bin\" BINARY\r\n  TRACK 05 MODE1/2352\r\n    INDEX 01 00:03:00\r\n";

    #[test]
    fn cue_sessions_mark_their_tracks() {
        // A Redump Jaguar CD cue: the boot track opens session 2.
        let sheet = cue("REM SESSION 01\nFILE a.bin BINARY\n TRACK 01 AUDIO\n\
                         FILE b.bin BINARY\n TRACK 02 AUDIO\n\
                         rem session 02\nFILE c.bin BINARY\n TRACK 03 AUDIO\n");
        let sessions: Vec<_> = sheet.tracks.iter().map(|t| t.session).collect();
        assert_eq!(sessions, [1, 1, 2]);
        assert_eq!(first_of_session(&sheet, 2), Some(2));
        assert_eq!(primary_track(&sheet), None, "every track is audio");
        // One session: no track of session 2; malformed markers are ignored.
        let one = cue("REM SESSION\nREM SESSION 2 X\nFILE a.bin BINARY\n TRACK 01 MODE1/2352\n");
        assert_eq!(one.tracks[0].session, 1);
        assert_eq!(first_of_session(&one, 2), None);
        // Session 1 is not written out, so older facts read the same.
        let json = serde_json::to_value(&one.tracks[0]).unwrap();
        assert!(json.get("session").is_none());
        let back: TrackInfo = serde_json::from_value(json).unwrap();
        assert_eq!(back.session, 1);
    }

    #[test]
    fn cue_dreamcast_primary_is_the_high_density_data_track() {
        let sheet = cue(DC_CUE);
        let hd: Vec<_> = sheet.tracks.iter().map(|t| t.high_density).collect();
        assert_eq!(hd, [false, false, true, true, true]);
        // Track 03 (the first data track of the HD area holds IP.BIN), not
        // the low-density track 01.
        assert_eq!(primary_track(&sheet), Some(2));
        // An HD area with only audio has no primary.
        let audio = cue("FILE a.bin BINARY\n TRACK 01 MODE1/2352\n\
                         REM HIGH-DENSITY AREA\nFILE b.bin BINARY\n TRACK 02 AUDIO\n");
        assert_eq!(primary_track(&audio), None);
        // A later SINGLE-DENSITY marker ends the HD area.
        let back = cue(
            "REM HIGH-DENSITY AREA\nFILE a.bin BINARY\n TRACK 01 AUDIO\n\
                        REM SINGLE-DENSITY AREA\nFILE b.bin BINARY\n TRACK 02 MODE1/2352\n",
        );
        assert_eq!(
            back.tracks
                .iter()
                .map(|t| t.high_density)
                .collect::<Vec<_>>(),
            [true, false]
        );
    }

    #[test]
    fn gdi_real_dreamcast_sheet() {
        // The real gdi: padded columns, quoted names with spaces and commas.
        let mut gdi = String::from("14\r\n");
        let lbas = [
            0, 1363, 45000, 369940, 389040, 402472, 411991, 420591, 431884, 449934, 467885, 492914,
            510955, 527775,
        ];
        for (i, lba) in lbas.iter().enumerate() {
            let n = i + 1;
            let ty = if matches!(n, 1 | 3 | 14) { 4 } else { 0 };
            gdi.push_str(&format!(
                "{n:2} {lba:6} {ty} 2352 \"Next Tetris, The - On-Line Edition (USA) (Track {n:02}).bin\" 0\r\n"
            ));
        }
        let sheet = parse_sheet(SheetKind::Gdi, gdi.as_bytes()).unwrap();
        assert_eq!(sheet.tracks.len(), 14);
        assert_eq!(
            sheet.files[2],
            "Next Tetris, The - On-Line Edition (USA) (Track 03).bin"
        );
        assert_eq!(primary_track(&sheet), Some(2));
        let hd: Vec<_> = sheet.tracks.iter().map(|t| t.high_density).collect();
        assert_eq!(hd[..3], [false, false, true]);
        assert!(hd[2..].iter().all(|&h| h));
    }

    #[test]
    fn sheets_reject_unknown_sector_sizes() {
        for bad in [
            "FILE a.bin BINARY\n TRACK 01 MODE1/0\n",
            "FILE a.bin BINARY\n TRACK 01 MODE1/2353\n",
            "FILE a.bin BINARY\n TRACK 01 MODE2/1\n",
        ] {
            assert!(
                matches!(
                    parse_sheet(SheetKind::Cue, bad.as_bytes()),
                    Err(SheetError::Malformed { .. })
                ),
                "{bad:?}"
            );
        }
        for good in [
            "MODE1/2048",
            "MODE2/2324",
            "MODE2/2336",
            "CDI/2352",
            "AUDIO",
        ] {
            let text = format!("FILE a.bin BINARY\n TRACK 01 {good}\n");
            assert!(
                parse_sheet(SheetKind::Cue, text.as_bytes()).is_ok(),
                "{good}"
            );
        }
        for bad in ["1\n1 0 4 0 a.bin 0\n", "1\n1 0 4 4096 a.bin 0\n"] {
            assert!(
                matches!(
                    parse_sheet(SheetKind::Gdi, bad.as_bytes()),
                    Err(SheetError::Malformed { .. })
                ),
                "{bad:?}"
            );
        }
        assert!(parse_sheet(SheetKind::Gdi, b"1\n1 0 4 2048 a.iso 0\n").is_ok());
    }

    #[test]
    fn infer_redump_single_digit_tracks() {
        // Redump names discs with fewer than ten tracks `(Track N)`.
        let listing = ["Game (Track 1).bin", "Game (Track 2).bin", "Game.cue"];
        assert_eq!(
            infer_redump_set(&listing, "Game.cue").unwrap(),
            ["Game (Track 1).bin", "Game (Track 2).bin"]
        );
        // One width per set: mixed widths are not one Redump set.
        assert_eq!(
            infer_redump_set(
                &["Game (Track 1).bin", "Game (Track 02).bin", "Game.cue"],
                "Game.cue"
            ),
            None
        );
        // Ten tracks or more take two digits, so `(Track 1)`..`(Track 10)`
        // mixes widths and is rejected.
        let mut names: Vec<String> = (1..=10).map(|i| format!("Game (Track {i}).bin")).collect();
        names.push("Game.cue".into());
        let listing: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(infer_redump_set(&listing, "Game.cue"), None);
    }

    #[test]
    #[allow(non_snake_case)]
    fn infer_gdi_trackNN() {
        let listing = [
            "DC/track01.bin",
            "DC/TRACK02.raw",
            "DC/track03.bin",
            "DC/disc.gdi",
            "elsewhere/track04.bin",
        ];
        assert_eq!(
            infer_redump_set(&listing, "DC/disc.gdi").unwrap(),
            ["DC/track01.bin", "DC/TRACK02.raw", "DC/track03.bin"]
        );
        // `trackNN` names belong to a gdi, not to a cue.
        assert_eq!(
            infer_redump_set(&["track01.bin", "disc.cue"], "disc.cue"),
            None
        );
        // Mixed `trackNN` and Redump names are not one set.
        assert_eq!(
            infer_redump_set(
                &["track01.bin", "disc (Track 02).bin", "disc.gdi"],
                "disc.gdi"
            ),
            None
        );
    }

    #[test]
    fn infer_extra_file_in_dir_rejected() {
        let mut names = redump("Game/", "Game", 2);
        names.push("Game/Game.cue".into());
        names.push("Game/readme.txt".into());
        let listing: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(infer_redump_set(&listing, "Game/Game.cue"), None);

        // A cue of another stem in the same folder is an extra file too.
        let mut names = redump("", "Game", 2);
        names.push("Game.cue".into());
        names.push("Other.cue".into());
        let listing: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(infer_redump_set(&listing, "Game.cue"), None);

        // Files in a subfolder are not in the sheet's directory.
        let mut names = redump("", "Game", 2);
        names.push("Game.cue".into());
        names.push("extras/readme.txt".into());
        let listing: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(infer_redump_set(&listing, "Game.cue").unwrap().len(), 2);
    }
}
