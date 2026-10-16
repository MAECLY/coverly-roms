//! A whole fill behind one door (spec §7 steps 3–9), shared by
//! `coverly fill` and the desktop app: phase one + plan (the dry run),
//! phase two (apply, then prune old journals), the preview of one
//! candidate, and the manual writes of the detail drawer (a local image,
//! removing art). Every write is journaled, so `undo` reverts it.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::art::SystemArt;
use crate::cache::Cache;
use crate::gather::{gather, select_targets, GatherOptions, Scope, TargetFilter};
use crate::identify::dat::{DatStore, DATABASE_BASE};
use crate::identify::{Identifier, IdentifyOptions, MatchMethod};
use crate::image::{process_image, Fit, ImageError, ImageLimits, OutputSpec, ProcessedImage};
use crate::jobs::{apply_plan, ApplyOptions, ArtStore, JobReport};
use crate::matching::{default_region_order, RegionPref, StrategyKind};
use crate::net::Http;
use crate::plan::{build_plan, Plan, PlanOptions};
use crate::profiles::{Media, Profile};
use crate::progress::ProgressSink;
use crate::sources::libretro::{LibretroConfig, LibretroSource, LIBRETRO_BASE, LIBRETRO_RAW_BASE};
use crate::sources::{ArtSource, Candidate, SourceError};
use crate::systems::SystemDb;
use crate::write::{
    ensure_within, prune_jobs, safe_file_name, OverwritePolicy, RemoveOutcome, WriteError,
    WriteOutcome, Writer,
};

/// Journals (and their backups) kept for undo (spec §9 undo history depth).
pub const KEEP_JOBS: usize = 10;

/// Where the sources and DATs live (tests point them at a mock server).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    pub libretro: String,
    pub libretro_raw: String,
    pub database: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            libretro: LIBRETRO_BASE.to_string(),
            libretro_raw: LIBRETRO_RAW_BASE.to_string(),
            database: DATABASE_BASE.to_string(),
        }
    }
}

/// What one fill does (spec §9 defaults).
#[derive(Clone, Debug)]
pub struct FillSettings {
    /// System folders or ids (empty = all).
    pub systems: Vec<String>,
    pub media: Vec<Media>,
    pub scope: Scope,
    pub strategy: StrategyKind,
    pub regions: Vec<RegionPref>,
    pub match_method: MatchMethod,
    /// Ignore cached candidate lists and indexes.
    pub refresh: bool,
}

impl Default for FillSettings {
    fn default() -> Self {
        FillSettings {
            systems: Vec::new(),
            media: vec![Media::Cover],
            scope: Scope::Missing,
            strategy: StrategyKind::RomRegion,
            regions: default_region_order(),
            match_method: MatchMethod::HashThenName,
            refresh: false,
        }
    }
}

/// Why a candidate preview or a manual write failed.
#[derive(Debug, thiserror::Error)]
pub enum ArtError {
    #[error("this layout cannot store that media")]
    NoSlot,
    #[error("unknown image source `{0}`")]
    UnknownSource(String),
    #[error("{0}")]
    Source(String),
    #[error("{0}")]
    Paused(String),
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(transparent)]
    Write(#[from] WriteError),
}

impl From<SourceError> for ArtError {
    fn from(e: SourceError) -> Self {
        match e {
            SourceError::Net(n) if n.pauses() => ArtError::Paused(n.to_string()),
            other => ArtError::Source(other.to_string()),
        }
    }
}

/// What a fill needs from its host.
pub struct FillEnv<'a> {
    pub cache: &'a Cache,
    pub http: &'a dyn Http,
    pub endpoints: &'a Endpoints,
    /// Holds the art store (`<cache dir>/art`).
    pub cache_dir: &'a Path,
    /// Holds journals and backups.
    pub data_dir: &'a Path,
}

fn spec_for(profile: &Profile, media: Media) -> Result<OutputSpec, ArtError> {
    let slot = profile.slot(media).ok_or(ArtError::NoSlot)?;
    Ok(OutputSpec::for_slot(slot, Fit::Contain, 90))
}

impl FillEnv<'_> {
    fn libretro(&self, refresh: bool) -> LibretroSource<'_> {
        let mut cfg = LibretroConfig {
            base: self.endpoints.libretro.clone(),
            raw_base: self.endpoints.libretro_raw.clone(),
            ..LibretroConfig::default()
        };
        if refresh {
            cfg.ttl_secs = 0;
        }
        LibretroSource::new(self.http, self.cache, cfg)
    }

    /// Phase one and the plan of `art` (the dry run). Never writes to the
    /// library. A pause gives a partial plan (`plan.paused`). A cancel
    /// does not mark the plan: the items not gathered yet are just missing
    /// (a cancel before the start gives an empty plan), so the caller tells
    /// a cancel apart by its own `cancel` flag.
    #[allow(clippy::too_many_arguments)]
    pub fn plan(
        &self,
        root: &Path,
        profile: &Profile,
        systems: &SystemDb,
        art: &[SystemArt],
        settings: &FillSettings,
        progress: &dyn ProgressSink,
        cancel: &AtomicBool,
    ) -> Plan {
        let targets = select_targets(
            art,
            &TargetFilter {
                systems: settings.systems.clone(),
                media: settings.media.clone(),
                scope: settings.scope,
            },
        );
        let dats = DatStore::new(self.http, self.cache, self.endpoints.database.clone());
        let identifier = Identifier::new(
            self.cache,
            &dats,
            IdentifyOptions {
                method: settings.match_method,
                ..IdentifyOptions::default()
            },
        );
        let libretro = self.libretro(settings.refresh);
        let sources: Vec<&dyn ArtSource> = vec![&libretro];
        let gopts = GatherOptions {
            consensus: settings.strategy == StrategyKind::Popularity,
            refresh: settings.refresh,
            ..GatherOptions::default()
        };
        let gathered = gather(
            &targets,
            systems,
            &sources,
            &identifier,
            self.cache,
            &gopts,
            progress,
            cancel,
        );
        build_plan(
            root,
            profile,
            systems,
            &gathered,
            &PlanOptions {
                strategy: settings.strategy,
                regions: settings.regions.clone(),
                ..PlanOptions::default()
            },
        )
    }

    /// Phase two: applies `plan`, then keeps the newest [`KEEP_JOBS`] journals.
    /// A cancel is `report.cancelled` (never `paused`; the items left are
    /// `not-attempted`).
    /// The source never refreshes here: images are fetched by the URL the
    /// plan picked, and the index TTL (what `refresh` changes) only matters
    /// when listing candidates in [`FillEnv::plan`].
    pub fn apply(
        &self,
        plan: &Plan,
        profile: &Profile,
        opts: &ApplyOptions,
        progress: &dyn ProgressSink,
        cancel: &AtomicBool,
    ) -> Result<JobReport, WriteError> {
        let libretro = self.libretro(false);
        let sources: Vec<&dyn ArtSource> = vec![&libretro];
        let store = ArtStore::new(self.cache_dir, self.cache);
        let report = apply_plan(
            plan,
            profile,
            &sources,
            &store,
            self.data_dir,
            opts,
            progress,
            cancel,
        )?;
        // Pruning only frees space: failing to prune is not an error.
        let _ = prune_jobs(self.data_dir, KEEP_JOBS);
        Ok(report)
    }

    /// The image of `c` as it would be written (decoded with limits and
    /// re-encoded for its media's slot), through the art store: a preview
    /// downloads once and the later apply reuses it.
    pub fn candidate_image(
        &self,
        profile: &Profile,
        c: &Candidate,
    ) -> Result<ProcessedImage, ArtError> {
        let spec = spec_for(profile, c.media)?;
        let libretro = self.libretro(false);
        if c.source != libretro.id() {
            return Err(ArtError::UnknownSource(c.source.clone()));
        }
        let store = ArtStore::new(self.cache_dir, self.cache);
        let (bytes, cached) = match store.get(&c.url) {
            Some(bytes) => (bytes, true),
            None => (libretro.fetch(c)?, false),
        };
        let img = process_image(&bytes, &spec, &ImageLimits::default())?;
        if !cached {
            let _ = store.put(&c.url, &bytes);
        }
        Ok(img)
    }
}

/// Writes `bytes` (a local image the user picked: untrusted) as the
/// `media` art at `target`: decoded with limits, re-encoded for the slot,
/// the existing art backed up, in a journaled job of its own. The job id
/// is `None` when nothing was written (a folder or symlink in the way): a
/// write that is refused or does nothing leaves no job behind, so it is
/// never undone in place of a real job nor pushes one out of the history.
pub fn write_local_image(
    root: &Path,
    profile: &Profile,
    media: Media,
    target: &Path,
    bytes: &[u8],
    data_dir: &Path,
) -> Result<(Option<String>, WriteOutcome), ArtError> {
    let spec = spec_for(profile, media)?;
    let img = process_image(bytes, &spec, &ImageLimits::default())?;
    precheck(root, target)?;
    let mut writer = Writer::begin(root, data_dir)?;
    let outcome = writer.write(target, &img.bytes, OverwritePolicy::Backup);
    settle(close(writer, data_dir), outcome.map_err(ArtError::from))
}

/// Removes the art file `target` (backed up, journaled) in a job of its
/// own. The job id is `None` when there was nothing to remove; a refused
/// remove leaves no job behind (see [`write_local_image`]).
pub fn remove_art(
    root: &Path,
    target: &Path,
    data_dir: &Path,
) -> Result<(Option<String>, RemoveOutcome), WriteError> {
    ensure_within(root, target)?;
    let mut writer = Writer::begin(root, data_dir)?;
    let outcome = writer.remove(target);
    settle(close(writer, data_dir), outcome)
}

/// The checks `Writer::write` makes before touching anything, made before
/// a journal exists.
fn precheck(root: &Path, target: &Path) -> Result<(), WriteError> {
    let target = ensure_within(root, target)?;
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| WriteError::NameInvalid(target.display().to_string()))?;
    if safe_file_name(name)? != name {
        return Err(WriteError::NameInvalid(name.to_string()));
    }
    Ok(())
}

/// The result of a manual write: its own error first (the real cause, e.g.
/// `Denied` or `DiskFull`), then the error of closing its job.
fn settle<T, E: From<WriteError>>(
    closed: Result<Option<String>, WriteError>,
    outcome: Result<T, E>,
) -> Result<(Option<String>, T), E> {
    let value = outcome?;
    Ok((closed?, value))
}

/// Finishes (and prunes old journals after) a job that changed the
/// library; discards one that did not.
fn close(writer: Writer, data_dir: &Path) -> Result<Option<String>, WriteError> {
    if !writer.has_entries() {
        writer.discard()?;
        return Ok(None);
    }
    let job = writer.finish()?;
    // Pruning only frees space: failing to prune is not an error.
    let _ = prune_jobs(data_dir, KEEP_JOBS);
    Ok(Some(job))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_write_error_wins_over_a_failed_close() {
        let closed: Result<Option<String>, WriteError> = Err(WriteError::Journal("discard".into()));
        let outcome: Result<(), WriteError> = Err(WriteError::DiskFull);
        assert!(matches!(settle(closed, outcome), Err(WriteError::DiskFull)));
    }

    #[test]
    fn a_failed_close_after_a_good_write_is_an_error() {
        let closed: Result<Option<String>, WriteError> = Err(WriteError::DiskFull);
        assert!(matches!(
            settle(closed, Ok::<_, WriteError>(())),
            Err(WriteError::DiskFull)
        ));
        let closed: Result<Option<String>, WriteError> = Ok(Some("job".into()));
        assert_eq!(
            settle(closed, Ok::<_, WriteError>(7)).unwrap(),
            (Some("job".into()), 7)
        );
    }
}
