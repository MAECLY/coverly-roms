//! A whole fill behind one door (spec §7 steps 3–9), shared by
//! `coverly fill` and the desktop app: phase one + plan (the dry run),
//! phase two (apply, then prune old journals), the preview of one
//! candidate, "Search by title" and the manual writes of the detail
//! drawer (a local image, a picked candidate, removing art). Every write is
//! journaled, so `undo` reverts it.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::art::SystemArt;
use crate::cache::Cache;
use crate::gamelist::{merge_into, rel_slash, GameArt};
use crate::gather::{gather, select_targets, GatherOptions, Scope, TargetFilter};
use crate::identify::dat::{DatStore, DATABASE_BASE};
use crate::identify::overrides::{Overrides, OVERRIDES_FILE};
use crate::identify::{IdMethod, Identifier, IdentifyOptions, MatchMethod};
use crate::image::{process_image, Fit, ImageError, ImageLimits, OutputSpec, ProcessedImage};
use crate::jobs::{apply_plan, ApplyOptions, ArtStore, JobReport};
use crate::matching::{default_region_order, RegionPref, StrategyKind};
use crate::net::Http;
use crate::plan::{build_plan, Plan, PlanAction, PlanItem, PlanOptions};
use crate::profiles::{Media, Profile};
use crate::progress::{NoProgress, ProgressSink};
use crate::sources::libretro::{
    LibretroConfig, LibretroSource, INDEX_TTL_SECS, LIBRETRO_BASE, LIBRETRO_RAW_BASE,
};
use crate::sources::{sort_candidates, ArtQuery, ArtSource, Candidate, SourceError};
use crate::systems::{System, SystemDb};
use crate::write::{
    ensure_within, prune_jobs, safe_file_name, OverwritePolicy, RemoveOutcome, WriteError,
    WriteOutcome, Writer,
};

/// Journals (and their backups) kept for undo (spec §9 undo history depth).
pub const KEEP_JOBS: usize = 10;

/// Most candidates a title search returns.
pub const SEARCH_LIMIT: usize = 12;
/// Lowest fuzzy score a title search keeps (lower than a fill's: the user
/// reads the list).
const SEARCH_MIN_SCORE: u8 = 50;
/// Longest title a search accepts, in characters.
const MAX_QUERY_CHARS: usize = 120;

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

/// What one fill does (spec §9 defaults; `Settings::fill_settings` builds
/// it from the user's settings).
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
    /// Larger ROMs are identified by name.
    pub hash_max_bytes: u64,
    pub arcade_translation: bool,
    pub fuzzy: bool,
    pub auto_threshold: f32,
    pub review_threshold: f32,
    /// Preferred languages (D28).
    pub languages: Vec<String>,
    /// Enabled source ids, preferred first.
    pub sources: Vec<String>,
    /// How long a listing stays fresh.
    pub index_ttl_secs: i64,
    /// Where manual identities are read (D32); `None` = the job data dir.
    /// The app sets its own data dir and the CLI the platform data dir, so
    /// a different job data folder (D27 backup location, `--data-dir`)
    /// never hides manual matches.
    pub overrides_dir: Option<PathBuf>,
}

impl Default for FillSettings {
    fn default() -> Self {
        let plan = PlanOptions::default();
        FillSettings {
            systems: Vec::new(),
            media: vec![Media::Cover],
            scope: Scope::Missing,
            strategy: StrategyKind::RomRegion,
            regions: default_region_order(),
            match_method: MatchMethod::HashThenName,
            refresh: false,
            hash_max_bytes: crate::identify::hash::DEFAULT_HASH_MAX_BYTES,
            arcade_translation: true,
            fuzzy: true,
            auto_threshold: plan.auto_threshold,
            review_threshold: plan.review_threshold,
            languages: Vec::new(),
            sources: plan.sources,
            index_ttl_secs: INDEX_TTL_SECS,
            overrides_dir: None,
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
    #[error("type a title of 1 to 120 characters")]
    BadQuery,
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

/// One ROM, for writes outside a plan.
#[derive(Clone, Copy, Debug)]
pub struct RomRef<'a> {
    pub system_id: &'a str,
    /// The system folder name as on disk.
    pub folder: &'a str,
    /// The system folder (where a gamelist lives); empty when unknown.
    pub system_dir: &'a Path,
    /// The ROM path relative to `system_dir`.
    pub rel_path: &'a Path,
    pub stem: &'a str,
}

/// A typed title, trimmed; refused when empty, over 120 characters or
/// holding control characters.
pub fn clean_query(q: &str) -> Result<String, ArtError> {
    let t = q.trim();
    if t.is_empty() || t.chars().count() > MAX_QUERY_CHARS || t.chars().any(char::is_control) {
        return Err(ArtError::BadQuery);
    }
    Ok(t.to_string())
}

/// Output and history settings of the drawer's manual writes.
#[derive(Clone, Copy, Debug)]
pub struct ManualOptions {
    pub fit: Fit,
    pub jpeg_quality: u8,
    pub keep_jobs: usize,
}

impl Default for ManualOptions {
    fn default() -> Self {
        ManualOptions {
            fit: Fit::Contain,
            jpeg_quality: 90,
            keep_jobs: KEEP_JOBS,
        }
    }
}

fn spec_for(
    profile: &Profile,
    media: Media,
    fit: Fit,
    quality: u8,
) -> Result<OutputSpec, ArtError> {
    let slot = profile.slot(media).ok_or(ArtError::NoSlot)?;
    Ok(OutputSpec::for_slot(slot, fit, quality))
}

impl FillEnv<'_> {
    fn libretro(&self, refresh: bool, ttl_secs: i64) -> LibretroSource<'_> {
        let cfg = LibretroConfig {
            base: self.endpoints.libretro.clone(),
            raw_base: self.endpoints.libretro_raw.clone(),
            ttl_secs: if refresh { 0 } else { ttl_secs },
        };
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
        let overrides_dir = settings.overrides_dir.as_deref().unwrap_or(self.data_dir);
        let overrides = Overrides::read(&overrides_dir.join(OVERRIDES_FILE));
        let identifier = Identifier::new(
            self.cache,
            &dats,
            IdentifyOptions {
                method: settings.match_method,
                hash_max_bytes: settings.hash_max_bytes,
                arcade_translation: settings.arcade_translation,
            },
        )
        .with_overrides(&overrides);
        let libretro = self.libretro(settings.refresh, settings.index_ttl_secs);
        let sources: Vec<&dyn ArtSource> = if settings.sources.iter().any(|s| s == libretro.id()) {
            vec![&libretro]
        } else {
            Vec::new()
        };
        let gopts = GatherOptions {
            consensus: settings.strategy == StrategyKind::Popularity,
            refresh: settings.refresh,
            fuzzy: settings.fuzzy,
            ttl_secs: settings.index_ttl_secs,
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
                sources: settings.sources.clone(),
                auto_threshold: settings.auto_threshold,
                review_threshold: settings.review_threshold,
                languages: settings.languages.clone(),
                ..PlanOptions::default()
            },
        )
    }

    /// Phase two: applies `plan`, then keeps the newest `opts.keep_jobs`
    /// journals and trims the art store to `opts.cache_max_bytes`.
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
        let libretro = self.libretro(false, INDEX_TTL_SECS);
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
        // Pruning and trimming only free space: failing is not an error.
        let _ = prune_jobs(self.data_dir, opts.keep_jobs.max(1));
        if let Some(max) = opts.cache_max_bytes {
            let _ = crate::jobs::trim_art_store(self.cache_dir, max);
        }
        Ok(report)
    }

    /// "Search by title" (M4-05): candidates of every enabled source for a
    /// typed title, best first, at most [`SEARCH_LIMIT`]. The title is
    /// untrusted: see [`clean_query`].
    pub fn search_title(
        &self,
        system: &System,
        media: Media,
        query: &str,
        settings: &FillSettings,
    ) -> Result<Vec<Candidate>, ArtError> {
        let q = clean_query(query)?;
        let libretro = self.libretro(settings.refresh, settings.index_ttl_secs);
        if !settings.sources.iter().any(|s| s == libretro.id()) {
            return Ok(Vec::new());
        }
        let parsed = crate::identify::names::parse_name(&q, false);
        let query = ArtQuery {
            system,
            media,
            canonical: None,
            rom: &parsed,
            stem: &q,
            by_name: true,
            fuzzy: true,
            min_score: SEARCH_MIN_SCORE,
            max_candidates: SEARCH_LIMIT,
            hash: None,
            size: 0,
            file_name: &q,
        };
        let mut found = libretro.candidates(&query)?;
        sort_candidates(&mut found);
        let mut seen = std::collections::BTreeSet::new();
        found.retain(|c| seen.insert((c.source.clone(), c.name.clone())));
        found.truncate(SEARCH_LIMIT);
        Ok(found)
    }

    /// Writes candidate `c` as the `media` art of `rom` at `target` (the
    /// ROM's own target: never renamed), replacing existing art with a
    /// backup, in a journaled job of its own (discarded when nothing
    /// changed).
    #[allow(clippy::too_many_arguments)]
    pub fn apply_candidate(
        &self,
        root: &Path,
        profile: &Profile,
        rom: &RomRef,
        media: Media,
        target: &Path,
        c: &Candidate,
        keep_jobs: usize,
    ) -> Result<JobReport, WriteError> {
        let item = PlanItem {
            system_id: rom.system_id.to_string(),
            folder: rom.folder.to_string(),
            rel_path: rom.rel_path.to_path_buf(),
            stem: rom.stem.to_string(),
            media,
            target: target.to_path_buf(),
            identity_method: IdMethod::Manual,
            canonical: Some(c.name.clone()),
            replaces_existing: target.exists(),
            note: None,
            reason: Some(crate::matching::PickReason::User),
            system_dir: rom.system_dir.to_path_buf(),
            action: PlanAction::Auto {
                chosen: c.clone(),
                confidence: 1.0,
                alternatives: Vec::new(),
            },
        };
        let plan = Plan::single(root, profile, item);
        let opts = ApplyOptions {
            overwrite: OverwritePolicy::Backup,
            include_review: false,
            concurrency: 1,
            discard_empty: true,
            keep_jobs,
            ..ApplyOptions::default()
        };
        self.apply(&plan, profile, &opts, &NoProgress, &AtomicBool::new(false))
    }

    /// The image of `c` as it would be written (decoded with limits and
    /// re-encoded for its media's slot), through the art store: a preview
    /// downloads once and the later apply reuses it.
    pub fn candidate_image(
        &self,
        profile: &Profile,
        c: &Candidate,
    ) -> Result<ProcessedImage, ArtError> {
        let spec = spec_for(profile, c.media, Fit::Contain, 90)?;
        let libretro = self.libretro(false, INDEX_TTL_SECS);
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
/// For a gamelist profile (Batocera/Knulli) the art path is also merged
/// into the system's `gamelist.xml` in the same job; a gamelist that
/// cannot be merged never fails the image write.
#[allow(clippy::too_many_arguments)]
pub fn write_local_image(
    root: &Path,
    profile: &Profile,
    rom: &RomRef,
    media: Media,
    target: &Path,
    bytes: &[u8],
    data_dir: &Path,
    opts: &ManualOptions,
) -> Result<(Option<String>, WriteOutcome), ArtError> {
    let spec = spec_for(profile, media, opts.fit, opts.jpeg_quality)?;
    let img = process_image(bytes, &spec, &ImageLimits::default())?;
    precheck(root, target)?;
    let mut writer = Writer::begin(root, data_dir)?;
    let outcome = writer.write(target, &img.bytes, OverwritePolicy::Backup);
    if profile.gamelist
        && !rom.system_dir.as_os_str().is_empty()
        && outcome
            .as_ref()
            .is_ok_and(|o| !matches!(o, WriteOutcome::SkippedExists { .. }))
    {
        if let Ok(art_rel) = target.strip_prefix(rom.system_dir) {
            let game = GameArt {
                rom: rel_slash(rom.rel_path),
                name: rom.stem.to_string(),
                art: vec![(media, rel_slash(art_rel))],
            };
            let _ = merge_into(&mut writer, rom.system_dir, &[game], true);
        }
    }
    settle(
        close(writer, data_dir, opts.keep_jobs),
        outcome.map_err(ArtError::from),
    )
}

/// Removes the art file `target` (backed up, journaled) in a job of its
/// own. The job id is `None` when there was nothing to remove; a refused
/// remove leaves no job behind (see [`write_local_image`]).
pub fn remove_art(
    root: &Path,
    target: &Path,
    data_dir: &Path,
    keep_jobs: usize,
) -> Result<(Option<String>, RemoveOutcome), WriteError> {
    ensure_within(root, target)?;
    let mut writer = Writer::begin(root, data_dir)?;
    let outcome = writer.remove(target);
    settle(close(writer, data_dir, keep_jobs), outcome)
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
/// library (keeping the newest `keep` journals); discards one that did not.
fn close(writer: Writer, data_dir: &Path, keep: usize) -> Result<Option<String>, WriteError> {
    if !writer.has_entries() {
        writer.discard()?;
        return Ok(None);
    }
    let job = writer.finish()?;
    // Pruning only frees space: failing to prune is not an error.
    let _ = prune_jobs(data_dir, keep.max(1));
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
