//! Disc games: reading sectors from disc images and archive windows, the
//! sheets (`.cue`, `.gdi`, `.m3u`) that tie tracks together, ISO 9660 and
//! the system rules over it, and serials in one normal form.
//!
//! Every read here is bounded: a [`source::WindowSource`] holds at most
//! [`source::MAX_WINDOW`] decoded bytes, a [`source::FileSource`] reads one
//! sector at a time, a sheet is parsed only when it is at most
//! [`sheets::MAX_SHEET_BYTES`] long, and a root directory is walked for at
//! most [`iso9660::MAX_ROOT_RECORDS`] records. Nothing recurses.

pub mod iso9660;
pub mod sega;
pub mod serial;
pub mod sheets;
pub mod signatures;
pub mod source;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;

/// The volume fingerprint of dedupe spec §5.4, computed here.
pub use crate::dedupe::source::{DiscIdSource, VolumeFingerprint};
pub use iso9660::{
    find_root_file, identify_playstation, identify_playstation_with, read_extent, read_pvd,
    DirRecord, FileExtent, PlayStationDisc, Pvd,
};
pub use sega::{parse_ip_bin, IpBin, SegaSystem};
pub use serial::normalise_serial;
pub use sheets::{
    infer_redump_set, parse_sheet, primary_track, Sheet, SheetError, SheetKind, TrackInfo,
    TrackMode,
};
pub use source::{detect_layout, FileSource, SectorLayout, SectorSource, WindowSource};

use std::io;
use std::ops::Range;

use iso9660::SYSTEM_CNF_CAP;
use source::USER_BYTES;

/// The system a disc belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiscSystem {
    /// A system of `systems.toml`, by id (`psx`, `saturn`, `dreamcast`…).
    Known(&'static str),
    /// A system Coverly recognises but has no folder for yet, by display
    /// name (`PlayStation 2`, `3DO`): the item is left with
    /// `UnsupportedSystem`.
    Unsupported(&'static str),
}

/// Who a disc is: system, serials and the volume fingerprint, as data for
/// the plan, the UI and dedupe level 3 (spec §7, §10).
#[derive(Clone, Debug, PartialEq)]
pub struct DiscIdentity {
    pub system: DiscSystem,
    /// Normalised serials (`SLUS-00862`, `T-5704H`); empty when the disc
    /// carries none (PC Engine CD, Neo Geo CD, 3DO).
    pub serials: Vec<String>,
    /// 1-based disc number, from a Sega device field (`GD-ROM1/1`).
    pub disc_no: Option<u8>,
    /// `None` when the boot file or the volume descriptor was not read.
    pub volume: Option<VolumeFingerprint>,
    /// Where the serials came from.
    pub source: DiscIdSource,
    /// Header confidence for `organize/evidence.rs` (spec §7 table).
    pub confidence: f32,
    /// What decided, from a fixed vocabulary (`SEGA SEGASATURN`,
    /// `SYSTEM.CNF BOOT + licence text`): data for the UI and the CLI to
    /// render, never bytes of the disc.
    pub detail: String,
}

impl DiscIdentity {
    /// The identity dedupe level 3 groups by (spec §10): the same serials,
    /// disc number, volume fingerprint and source, with nothing re-derived.
    /// The system, confidence and detail stay with organise: dedupe asks a
    /// probe per system already.
    pub fn to_dedupe_id(&self) -> crate::dedupe::source::DiscId {
        crate::dedupe::source::DiscId {
            serials: self.serials.clone(),
            disc_no: self.disc_no,
            volume: self.volume.clone(),
            source: self.source,
        }
    }
}

/// Reads later stored bytes of the track a [`SectorSource`] window was cut
/// from: given a byte range of the stored track, the bytes, or `None` when
/// the caller's budget does not reach them (the verdict then stays as the
/// window alone gives it). A plan reads them with a skip on a fresh
/// `open_entry`, charged to the same head budget.
pub type FetchStored<'a> = &'a mut dyn FnMut(Range<u64>) -> io::Result<Option<Vec<u8>>>;

/// Identifies a disc from its primary data track (spec §7): track 1 for
/// PlayStation, Saturn, Sega CD, Neo Geo CD and 3DO, track 03 (the
/// high-density area) for Dreamcast, the first data track for PC Engine
/// CD, the image itself for an ISO.
///
/// Checks run on fixed sectors first (Opera, Sega `IP.BIN`, PC Engine CD
/// at sector 1), then on the ISO 9660 volume (PlayStation, PSP, Neo Geo
/// CD), so a Sega or NEC disc never reads as Neo Geo CD and a Sega CD never
/// as a Mega Drive cartridge.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiscReader;

impl DiscReader {
    /// The identity of the track behind `src`; `None` when no rule matches
    /// or a read fails.
    pub fn identify(src: &mut dyn SectorSource) -> Option<DiscIdentity> {
        DiscReader::identify_with(src, None, None).ok().flatten()
    }

    /// [`DiscReader::identify`] with the whole image's length when known
    /// (a PlayStation volume on more than a CD holds is a PS2 DVD) and a
    /// way to fetch stored bytes past a window: a PlayStation `SYSTEM.CNF`
    /// listed in the root but stored past the head is fetched through
    /// `fetch` for the 0.95 verdict and the volume fingerprint. Read
    /// errors of `src` and of `fetch` are returned.
    pub fn identify_with(
        src: &mut dyn SectorSource,
        image_len: Option<u64>,
        fetch: Option<FetchStored<'_>>,
    ) -> io::Result<Option<DiscIdentity>> {
        let mut sector0 = [0u8; USER_BYTES];
        if src.read_user(0, &mut sector0)? {
            if signatures::is_opera(&sector0) {
                return Ok(Some(DiscIdentity {
                    system: DiscSystem::Unsupported("3DO"),
                    serials: Vec::new(),
                    disc_no: None,
                    volume: None,
                    source: DiscIdSource::Signature,
                    confidence: 0.90,
                    detail: "Opera volume header".to_owned(),
                }));
            }
            if let Some(ip) = sega::parse_ip_bin(&sector0) {
                return sega_identity(src, ip).map(Some);
            }
        }
        if signatures::is_pce_cd(src)? {
            return Ok(Some(DiscIdentity {
                system: DiscSystem::Known("pcenginecd"),
                serials: Vec::new(),
                disc_no: None,
                volume: None,
                source: DiscIdSource::Signature,
                confidence: 0.90,
                detail: "PC Engine CD-ROM SYSTEM".to_owned(),
            }));
        }
        if let Some(ps) = playstation(src, image_len, fetch)? {
            return Ok(Some(ps));
        }
        let Some(pvd) = iso9660::read_pvd(src)? else {
            return Ok(None);
        };
        if signatures::is_umd(&pvd) {
            let serial = signatures::umd_serial(src, &pvd)?;
            let source = if serial.is_some() {
                DiscIdSource::UmdData
            } else {
                DiscIdSource::Signature
            };
            return Ok(Some(DiscIdentity {
                system: DiscSystem::Known("psp"),
                serials: serial.into_iter().collect(),
                disc_no: None,
                volume: None,
                source,
                confidence: 0.85,
                detail: "PSP GAME".to_owned(),
            }));
        }
        if signatures::is_neocd(src, &pvd)? {
            return Ok(Some(DiscIdentity {
                system: DiscSystem::Known("neocd"),
                serials: Vec::new(),
                disc_no: None,
                volume: None,
                source: DiscIdSource::Signature,
                confidence: 0.85,
                detail: "IPL.TXT".to_owned(),
            }));
        }
        Ok(None)
    }
}

/// A Sega disc: the header's verdict, and the fingerprint from the PVD and
/// the first 32 KiB of the track when the source holds both.
fn sega_identity(src: &mut dyn SectorSource, ip: IpBin) -> io::Result<DiscIdentity> {
    let volume = match (iso9660::read_pvd(src)?, sega::ip_bin_bytes(src)?) {
        (Some(pvd), Some(bytes)) => Some(iso9660::volume_fingerprint(&pvd, &bytes)),
        _ => None,
    };
    Ok(DiscIdentity {
        system: DiscSystem::Known(ip.system.id()),
        serials: ip.serial.into_iter().collect(),
        disc_no: ip.disc.map(|(n, _)| n),
        volume,
        source: DiscIdSource::IpBin,
        confidence: 0.95,
        detail: ip.detail.to_owned(),
    })
}

/// A PlayStation disc, its pending `SYSTEM.CNF` fetched when `fetch`
/// reaches it.
fn playstation(
    src: &mut dyn SectorSource,
    image_len: Option<u64>,
    fetch: Option<FetchStored<'_>>,
) -> io::Result<Option<DiscIdentity>> {
    let Some(mut disc) = iso9660::identify_playstation(src, image_len)? else {
        return Ok(None);
    };
    if let (Some(extent), Some(fetch)) = (disc.pending_boot, fetch) {
        let layout = src.layout();
        if let Some(range) = extent.stored_range(layout, SYSTEM_CNF_CAP) {
            if let Some(bytes) = fetch(range)? {
                let mut tail = WindowSource::at(extent.lba, bytes, layout);
                if let Some(cnf) = iso9660::read_extent(&mut tail, extent, SYSTEM_CNF_CAP)? {
                    if let Some(done) =
                        iso9660::identify_playstation_with(src, image_len, Some(&cnf))?
                    {
                        disc = done;
                    }
                }
            }
        }
    }
    let source = if disc.detail.starts_with("SYSTEM.CNF") {
        DiscIdSource::SystemCnf
    } else {
        DiscIdSource::Signature
    };
    Ok(Some(DiscIdentity {
        system: disc.system,
        serials: disc.serial.into_iter().collect(),
        disc_no: None,
        volume: disc.volume,
        source,
        confidence: disc.confidence,
        detail: disc.detail,
    }))
}
