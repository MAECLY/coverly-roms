//! Sector sources: logical 2048-byte user sectors over the ways a disc
//! track is stored (ISO 2048, raw 2352 Mode 1 / Mode 2, 2336 Mode 2).
//!
//! A [`WindowSource`] is the decoded head of an archive entry (at most
//! [`MAX_WINDOW`] bytes, so a disc inside a solid 7z costs one bounded
//! decode); a [`FileSource`] is a track on disk, read one sector at a time.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes of user data in one logical sector.
pub const USER_BYTES: usize = 2048;

/// Largest decoded head a [`WindowSource`] keeps (spec §6.2: a window of at
/// most 4 MiB within the plan head budget).
pub const MAX_WINDOW: usize = 4 * 1024 * 1024;

/// How much of a track's head [`detect_layout`] looks at: seventeen raw
/// sectors, enough to reach the volume descriptor at LBA 16 in any layout.
pub const DETECT_BYTES: usize = 17 * 2352;

/// The CD sync pattern that starts every raw data sector.
const SYNC: [u8; 12] = [
    0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00,
];

/// How the sectors of a track are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SectorLayout {
    /// 2048-byte user sectors (`.iso`, `MODE1/2048`).
    Iso2048,
    /// Raw 2352-byte Mode 1 sectors: sync, header, 2048 user bytes, EDC/ECC.
    Raw2352Mode1,
    /// Raw 2352-byte Mode 2 Form 1 sectors (PlayStation, CD-XA): sync,
    /// header, 8-byte subheader, 2048 user bytes.
    Raw2352Mode2,
    /// 2336-byte Mode 2 sectors (no sync or header): subheader first.
    Mode2_2336,
}

impl SectorLayout {
    /// Stored bytes per sector.
    pub fn sector_size(self) -> usize {
        match self {
            SectorLayout::Iso2048 => 2048,
            SectorLayout::Raw2352Mode1 | SectorLayout::Raw2352Mode2 => 2352,
            SectorLayout::Mode2_2336 => 2336,
        }
    }

    /// Offset of the 2048 user bytes within a stored sector.
    pub fn user_offset(self) -> usize {
        match self {
            SectorLayout::Iso2048 => 0,
            SectorLayout::Raw2352Mode1 => 16,
            SectorLayout::Raw2352Mode2 => 24,
            SectorLayout::Mode2_2336 => 8,
        }
    }

    fn user_at(self, lba: u32) -> u64 {
        u64::from(lba) * self.sector_size() as u64 + self.user_offset() as u64
    }
}

/// Reads logical sectors of one disc track.
pub trait SectorSource {
    /// How the track stores its sectors.
    fn layout(&self) -> SectorLayout;

    /// The 2048 user bytes of logical sector `lba` into `out`. `Ok(false)`
    /// when the source ends before the whole sector (out of range, or past
    /// the end of a window); `out` is then unspecified.
    fn read_user(&mut self, lba: u32, out: &mut [u8; USER_BYTES]) -> io::Result<bool>;

    /// Stored sector `n` as it is ([`SectorLayout::sector_size`] bytes,
    /// sync and headers included). `None` past the end.
    fn raw_sector(&mut self, n: u32) -> io::Result<Option<Vec<u8>>>;
}

/// The layout of a track from its first stored sectors (ideally
/// [`DETECT_BYTES`] of them).
///
/// A raw sector starts with the sync `00 FF×10 00`; its mode byte (offset
/// 15) tells Mode 1 from Mode 2. Without a sync, an ISO 9660 descriptor
/// (`CD001`) at LBA 16 of 2336-byte sectors means Mode 2/2336; anything
/// else reads as 2048-byte sectors.
pub fn detect_layout(first_sectors: &[u8]) -> SectorLayout {
    if first_sectors.starts_with(&SYNC) {
        // The first raw sector that states Mode 1 or Mode 2 decides; a
        // track whose head has no such sector is still raw (Mode 1).
        return first_sectors
            .as_chunks::<2352>()
            .0
            .iter()
            .take(DETECT_BYTES / 2352)
            .filter(|s| s.starts_with(&SYNC))
            .find_map(|s| match s[15] {
                1 => Some(SectorLayout::Raw2352Mode1),
                2 => Some(SectorLayout::Raw2352Mode2),
                _ => None,
            })
            .unwrap_or(SectorLayout::Raw2352Mode1);
    }
    let pvd_2336 = SectorLayout::Mode2_2336.user_at(16) as usize + 1;
    if first_sectors.get(pvd_2336..pvd_2336 + 5) == Some(b"CD001") {
        return SectorLayout::Mode2_2336;
    }
    SectorLayout::Iso2048
}

/// Copies `out.len()` bytes at `offset` of `bytes`; `false` when they do
/// not all fit.
fn copy_at(bytes: &[u8], offset: u64, out: &mut [u8]) -> bool {
    let Ok(start) = usize::try_from(offset) else {
        return false;
    };
    let Some(src) = start
        .checked_add(out.len())
        .and_then(|end| bytes.get(start..end))
    else {
        return false;
    };
    out.copy_from_slice(src);
    true
}

/// A sector source over a decoded head held in memory (an archive entry's
/// first bytes). Never larger than [`MAX_WINDOW`].
#[derive(Clone, Debug)]
pub struct WindowSource {
    bytes: Vec<u8>,
    layout: SectorLayout,
    /// The stored sector `bytes` starts at (0 for a head).
    first: u32,
}

impl WindowSource {
    /// A window over `bytes` (cut to [`MAX_WINDOW`]), layout detected.
    pub fn new(bytes: Vec<u8>) -> WindowSource {
        let layout = detect_layout(&bytes);
        WindowSource::with_layout(bytes, layout)
    }

    /// A window over `bytes` (cut to [`MAX_WINDOW`]) with a known layout,
    /// for a sheet that states the track mode.
    pub fn with_layout(mut bytes: Vec<u8>, layout: SectorLayout) -> WindowSource {
        bytes.truncate(MAX_WINDOW);
        WindowSource {
            bytes,
            layout,
            first: 0,
        }
    }

    /// A window over stored sectors that start at sector `first` of the
    /// track (cut to [`MAX_WINDOW`]): a later stretch of an entry, such as
    /// a boot file listed in the head's root directory but stored past it.
    /// Sectors before `first` read as out of range.
    pub fn at(first: u32, mut bytes: Vec<u8>, layout: SectorLayout) -> WindowSource {
        bytes.truncate(MAX_WINDOW);
        WindowSource {
            bytes,
            layout,
            first,
        }
    }

    /// Offset within `bytes` of stored byte `offset` of the track.
    fn local(&self, offset: u64) -> Option<u64> {
        let base = u64::from(self.first) * self.layout.sector_size() as u64;
        offset.checked_sub(base)
    }

    /// Reads at most `cap` bytes (never more than [`MAX_WINDOW`]) from
    /// `reader`, an entry stream, and detects the layout.
    pub fn from_reader(reader: impl Read, cap: usize) -> io::Result<WindowSource> {
        let cap = cap.min(MAX_WINDOW);
        let mut bytes = Vec::with_capacity(cap.min(DETECT_BYTES));
        reader.take(cap as u64).read_to_end(&mut bytes)?;
        Ok(WindowSource::new(bytes))
    }

    /// Bytes held.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the window holds no bytes.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl SectorSource for WindowSource {
    fn layout(&self) -> SectorLayout {
        self.layout
    }

    fn read_user(&mut self, lba: u32, out: &mut [u8; USER_BYTES]) -> io::Result<bool> {
        let Some(at) = self.local(self.layout.user_at(lba)) else {
            return Ok(false);
        };
        Ok(copy_at(&self.bytes, at, out))
    }

    fn raw_sector(&mut self, n: u32) -> io::Result<Option<Vec<u8>>> {
        let mut out = vec![0u8; self.layout.sector_size()];
        let Some(at) = self.local(u64::from(n) * out.len() as u64) else {
            return Ok(None);
        };
        Ok(copy_at(&self.bytes, at, &mut out).then_some(out))
    }
}

/// A sector source over a track file on disk. Every read is one sector at
/// a known offset; nothing past the file's length is attempted.
#[derive(Debug)]
pub struct FileSource {
    file: File,
    len: u64,
    layout: SectorLayout,
}

impl FileSource {
    /// Opens `path` and detects its layout from its first sectors.
    pub fn open(path: &Path) -> io::Result<FileSource> {
        FileSource::new(crate::consoles::open_rom(path)?)
    }

    /// A source over `file`, layout detected from its first sectors.
    pub fn new(mut file: File) -> io::Result<FileSource> {
        file.seek(SeekFrom::Start(0))?;
        let mut head = Vec::with_capacity(DETECT_BYTES);
        (&mut file)
            .take(DETECT_BYTES as u64)
            .read_to_end(&mut head)?;
        let layout = detect_layout(&head);
        FileSource::with_layout(file, layout)
    }

    /// A source over `file` with a known layout.
    pub fn with_layout(file: File, layout: SectorLayout) -> io::Result<FileSource> {
        let len = file.metadata()?.len();
        Ok(FileSource { file, len, layout })
    }

    /// Reads exactly `out.len()` bytes at `offset`; `false` when they lie
    /// past the end of the file (as it was opened, or as it is now).
    fn read_at(&mut self, offset: u64, out: &mut [u8]) -> io::Result<bool> {
        let fits = offset
            .checked_add(out.len() as u64)
            .is_some_and(|end| end <= self.len);
        if !fits {
            return Ok(false);
        }
        self.file.seek(SeekFrom::Start(offset))?;
        match self.file.read_exact(out) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
            Err(e) => Err(e),
        }
    }
}

impl SectorSource for FileSource {
    fn layout(&self) -> SectorLayout {
        self.layout
    }

    fn read_user(&mut self, lba: u32, out: &mut [u8; USER_BYTES]) -> io::Result<bool> {
        let offset = self.layout.user_at(lba);
        self.read_at(offset, out)
    }

    fn raw_sector(&mut self, n: u32) -> io::Result<Option<Vec<u8>>> {
        let mut out = vec![0u8; self.layout.sector_size()];
        let offset = u64::from(n) * out.len() as u64;
        Ok(self.read_at(offset, &mut out)?.then_some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `count` stored sectors in `layout`; the user bytes of sector `n` are
    /// filled with `n as u8`, and sector 16 carries an ISO 9660 descriptor.
    fn track(layout: SectorLayout, count: u32, mode2_subheader: bool) -> Vec<u8> {
        let size = layout.sector_size();
        let mut out = vec![0u8; size * count as usize];
        for n in 0..count {
            let s = &mut out[n as usize * size..][..size];
            match layout {
                SectorLayout::Raw2352Mode1 | SectorLayout::Raw2352Mode2 => {
                    s[..12].copy_from_slice(&SYNC);
                    s[15] = if layout == SectorLayout::Raw2352Mode1 {
                        1
                    } else {
                        2
                    };
                }
                SectorLayout::Mode2_2336 if mode2_subheader => s[2] = 0x08,
                _ => {}
            }
            let user = &mut s[layout.user_offset()..][..USER_BYTES];
            user.fill(n as u8);
            if n == 16 {
                user[..6].copy_from_slice(b"\x01CD001");
            }
        }
        out
    }

    #[test]
    fn layout_2048_2352_2336_detected() {
        for layout in [
            SectorLayout::Iso2048,
            SectorLayout::Raw2352Mode1,
            SectorLayout::Raw2352Mode2,
            SectorLayout::Mode2_2336,
        ] {
            let bytes = track(layout, 17, true);
            assert_eq!(detect_layout(&bytes), layout, "{layout:?}");
            let mut src = WindowSource::new(bytes);
            assert_eq!(src.layout(), layout);
            let mut out = [0u8; USER_BYTES];
            assert!(src.read_user(3, &mut out).unwrap(), "{layout:?}");
            assert!(out.iter().all(|&b| b == 3), "{layout:?}");
            assert!(src.read_user(16, &mut out).unwrap());
            assert_eq!(&out[1..6], b"CD001", "{layout:?}");
        }
        // Raw Mode 1 whose first sector is not data still reads as raw.
        let mut saturn = track(SectorLayout::Raw2352Mode1, 2, false);
        saturn[15] = 0;
        assert_eq!(detect_layout(&saturn), SectorLayout::Raw2352Mode1);
        // Short or empty heads are 2048-byte sectors.
        assert_eq!(detect_layout(&[]), SectorLayout::Iso2048);
        assert_eq!(detect_layout(&SYNC[..8]), SectorLayout::Iso2048);
    }

    #[test]
    fn window_source_out_of_range_is_none() {
        let bytes = track(SectorLayout::Raw2352Mode2, 4, false);
        let mut src = WindowSource::new(bytes.clone());
        let mut out = [0u8; USER_BYTES];
        assert!(src.read_user(3, &mut out).unwrap());
        assert!(!src.read_user(4, &mut out).unwrap());
        assert!(!src.read_user(u32::MAX, &mut out).unwrap());
        assert_eq!(src.raw_sector(0).unwrap().as_deref(), Some(&bytes[..2352]));
        assert_eq!(src.raw_sector(4).unwrap(), None);
        assert_eq!(src.raw_sector(u32::MAX).unwrap(), None);

        // A sector cut short by the window's end is out of range too.
        let mut cut = WindowSource::new(bytes[..2352 * 3 + 100].to_vec());
        assert!(!cut.read_user(3, &mut out).unwrap());
        assert_eq!(cut.raw_sector(3).unwrap(), None);

        // The window never holds more than MAX_WINDOW bytes.
        let big = WindowSource::with_layout(vec![0; MAX_WINDOW + 4096], SectorLayout::Iso2048);
        assert_eq!(big.len(), MAX_WINDOW);
        let read = WindowSource::from_reader(io::repeat(0).take(MAX_WINDOW as u64 * 2), usize::MAX)
            .unwrap();
        assert_eq!(read.len(), MAX_WINDOW);
        let capped = WindowSource::from_reader(io::repeat(0), 10_000).unwrap();
        assert_eq!(capped.len(), 10_000);
    }

    #[test]
    fn file_source_reads_sectors_and_stops_at_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.bin");
        let bytes = track(SectorLayout::Raw2352Mode1, 18, false);
        std::fs::write(&path, &bytes).unwrap();
        let mut src = FileSource::open(&path).unwrap();
        assert_eq!(src.layout(), SectorLayout::Raw2352Mode1);
        let mut out = [0u8; USER_BYTES];
        assert!(src.read_user(17, &mut out).unwrap());
        assert!(out.iter().all(|&b| b == 17));
        assert!(!src.read_user(18, &mut out).unwrap());
        assert!(!src.read_user(u32::MAX, &mut out).unwrap());
        assert_eq!(
            src.raw_sector(1).unwrap().as_deref(),
            Some(&bytes[2352..2 * 2352])
        );
        assert_eq!(src.raw_sector(18).unwrap(), None);

        // A file handed over mid-read is still detected from its start.
        let mut file = File::open(&path).unwrap();
        file.seek(SeekFrom::Start(5000)).unwrap();
        assert_eq!(
            FileSource::new(file).unwrap().layout(),
            SectorLayout::Raw2352Mode1
        );

        let iso = dir.path().join("game.iso");
        std::fs::write(&iso, track(SectorLayout::Iso2048, 17, false)).unwrap();
        let mut src = FileSource::open(&iso).unwrap();
        assert_eq!(src.layout(), SectorLayout::Iso2048);
        assert!(src.read_user(16, &mut out).unwrap());
        assert_eq!(&out[1..6], b"CD001");
    }
}
