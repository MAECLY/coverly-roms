//! Phase two of a fill (spec §7 steps 8–9, §10): apply a plan. Downloads go
//! through a content-addressed art store in the cache dir (re-applying,
//! e.g. after undo or with another profile, does not download again), are
//! re-encoded for the profile slot and written through the safe writer.
//! One failing item never stops the job; a rate limit, going offline or a
//! timeout, a full or read-only volume, or a cancel stop it cleanly with
//! the journal kept, so `undo` and a re-run both work.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;

use crate::cache::Cache;
use crate::image::{process_image, Fit, ImageError, ImageLimits, OutputSpec};
use crate::plan::{Plan, PlanAction, PlanItem};
use crate::profiles::{Media, Profile};
use crate::progress::{PauseKind, Phase, ProgressEvent, ProgressSink};
use crate::sources::{ArtSource, Candidate, SourceError};
use crate::write::{sha1_hex, OverwritePolicy, WriteError, WriteOutcome, Writer};

/// Content-addressed downloads: `<cache dir>/art/<sha1[0..2]>/<sha1>`, with
/// the URL → SHA-1 map in the cache database.
pub struct ArtStore<'a> {
    dir: PathBuf,
    cache: &'a Cache,
}

/// Makes temp file names unique between threads of this process.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn is_sha1_hex(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

impl<'a> ArtStore<'a> {
    pub fn new(cache_dir: &Path, cache: &'a Cache) -> ArtStore<'a> {
        ArtStore {
            dir: cache_dir.join("art"),
            cache,
        }
    }

    fn path_for(&self, sha1: &str) -> PathBuf {
        self.dir.join(&sha1[..2]).join(sha1)
    }

    /// The stored bytes of `url`, only when they still match their SHA-1.
    pub fn get(&self, url: &str) -> Option<Vec<u8>> {
        let (sha1, _) = self.cache.art_file(url).ok()??;
        let sha1 = sha1.to_ascii_lowercase();
        if !is_sha1_hex(&sha1) {
            return None;
        }
        let bytes = fs::read(self.path_for(&sha1)).ok()?;
        (sha1_hex(&bytes).eq_ignore_ascii_case(&sha1)).then_some(bytes)
    }

    /// Stores `bytes` as the content of `url` (temp file + rename).
    pub fn put(&self, url: &str, bytes: &[u8]) -> io::Result<()> {
        let sha1 = sha1_hex(bytes).to_ascii_lowercase();
        let path = self.path_for(&sha1);
        let intact = fs::read(&path).is_ok_and(|b| sha1_hex(&b).eq_ignore_ascii_case(&sha1));
        if !intact {
            let dir = path.parent().expect("art store path has a parent");
            fs::create_dir_all(dir)?;
            let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let tmp = dir.join(format!(".{sha1}.{}.{n}.tmp", std::process::id()));
            if let Err(e) = fs::write(&tmp, bytes).and_then(|()| fs::rename(&tmp, &path)) {
                let _ = fs::remove_file(&tmp);
                return Err(e);
            }
        }
        self.cache
            .put_art_file(url, &sha1, bytes.len() as u64)
            .map_err(io::Error::other)
    }
}

/// Apply settings (spec §9 defaults).
#[derive(Clone, Debug)]
pub struct ApplyOptions {
    pub overwrite: OverwritePolicy,
    /// Whether `review` items are written too (spec §7.7; `--auto-only` turns it off).
    pub include_review: bool,
    /// Worker threads, clamped to 1–6.
    pub concurrency: usize,
    pub image_limits: ImageLimits,
    pub fit: Fit,
    pub jpeg_quality: u8,
    /// How many alternatives are tried after the chosen candidate fails.
    pub max_alternatives: usize,
    /// Continue this paused job's journal instead of starting a new one (D21).
    pub resume_job: Option<String>,
}

impl Default for ApplyOptions {
    fn default() -> Self {
        ApplyOptions {
            overwrite: OverwritePolicy::Never,
            include_review: true,
            concurrency: 4,
            image_limits: ImageLimits::default(),
            fit: Fit::Contain,
            jpeg_quality: 90,
            max_alternatives: 2,
            resume_job: None,
        }
    }
}

/// Why one item failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemErrorKind {
    NoArt,
    Network,
    Decode,
    WriteDenied,
    DiskFull,
    NameInvalid,
    OutsideRoot,
    /// The library volume was ejected or went away.
    VolumeGone,
    Io,
}

/// What happened to one plan item.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ItemStatus {
    Filled {
        source: String,
        name: String,
    },
    Replaced {
        source: String,
        name: String,
        backup: PathBuf,
    },
    SkippedExists,
    SkippedReview,
    SkippedNoMatch,
    NotAttempted,
    Failed {
        kind: ItemErrorKind,
        message: String,
    },
}

/// One plan item and its outcome.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ItemResult {
    pub folder: String,
    pub rel_path: PathBuf,
    pub media: Media,
    pub target: PathBuf,
    #[serde(flatten)]
    pub status: ItemStatus,
}

/// The outcome of [`apply_plan`].
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct JobReport {
    /// The journal id, for `undo`; `None` when nothing was to be written.
    pub job_id: Option<String>,
    pub filled: usize,
    pub replaced: usize,
    /// Skipped (exists, review, no match) and not attempted items.
    pub skipped: usize,
    pub no_art: usize,
    pub failed: usize,
    pub items: Vec<ItemResult>,
    /// Why the job stopped early, when it did.
    pub paused: Option<String>,
    /// The kind of `paused`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_kind: Option<PauseKind>,
    pub cancelled: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn wanted(action: &PlanAction, opts: &ApplyOptions) -> bool {
    match action {
        PlanAction::Auto { .. } => true,
        PlanAction::Review { .. } => opts.include_review,
        PlanAction::None { .. } => false,
    }
}

/// Applies `plan`: downloads, re-encodes and writes each `auto` item (and
/// `review` items when `opts.include_review`), in plan order, with
/// `opts.concurrency` workers. Errors only when the job journal cannot be
/// started or finished; every item outcome is in the report.
#[allow(clippy::too_many_arguments)]
pub fn apply_plan(
    plan: &Plan,
    profile: &Profile,
    sources: &[&dyn ArtSource],
    store: &ArtStore,
    data_dir: &Path,
    opts: &ApplyOptions,
    progress: &dyn ProgressSink,
    cancel: &AtomicBool,
) -> Result<JobReport, WriteError> {
    let todo: Vec<usize> = plan
        .items
        .iter()
        .enumerate()
        .filter(|(_, i)| wanted(&i.action, opts))
        .map(|(n, _)| n)
        .collect();
    let mut results: Vec<Option<ItemStatus>> = plan
        .items
        .iter()
        .map(|i| match i.action {
            PlanAction::Review { .. } if !opts.include_review => Some(ItemStatus::SkippedReview),
            PlanAction::None { .. } => Some(ItemStatus::SkippedNoMatch),
            _ => None,
        })
        .collect();
    let mut report = JobReport::default();
    if cancel.load(Ordering::SeqCst) {
        report.cancelled = true;
    } else if !todo.is_empty() {
        let started = match &opts.resume_job {
            Some(job) => Writer::resume(&plan.root, data_dir, job)?,
            None => Writer::begin(&plan.root, data_dir)?,
        };
        let writer = Mutex::new(started);
        report.job_id = Some(lock(&writer).job_id().to_string());
        let next = AtomicUsize::new(0);
        let stop: Mutex<Option<(String, PauseKind)>> = Mutex::new(None);
        let done = Mutex::new(Vec::<(usize, ItemStatus)>::new());
        let total = todo.len();
        progress.event(ProgressEvent::Started {
            phase: Phase::Apply,
            total,
        });
        let ctx = Ctx {
            root: &plan.root,
            profile,
            sources,
            store,
            writer: &writer,
            opts,
            stop: &stop,
            cancel,
        };
        std::thread::scope(|s| {
            for _ in 0..opts.concurrency.clamp(1, 6) {
                s.spawn(|| loop {
                    let k = next.fetch_add(1, Ordering::SeqCst);
                    let Some(&n) = todo.get(k) else { break };
                    if ctx.halted() {
                        lock(&done).push((n, ItemStatus::NotAttempted));
                        continue;
                    }
                    let item = &plan.items[n];
                    progress.event(ProgressEvent::Item {
                        phase: Phase::Apply,
                        done: k,
                        total,
                        label: item.stem.clone(),
                    });
                    let status = ctx.apply_item(item);
                    lock(&done).push((n, status));
                });
            }
        });
        for (n, st) in done.into_inner().unwrap_or_else(|p| p.into_inner()) {
            results[n] = Some(st);
        }
        report.cancelled = cancel.load(Ordering::SeqCst);
        let stopped = stop.into_inner().unwrap_or_else(|p| p.into_inner());
        report.paused = stopped.as_ref().map(|(why, _)| why.clone());
        report.pause_kind = stopped.map(|(_, kind)| kind);
        progress.event(ProgressEvent::Finished {
            phase: Phase::Apply,
        });
        writer
            .into_inner()
            .unwrap_or_else(|p| p.into_inner())
            .finish()?;
    }
    for (item, st) in plan.items.iter().zip(results) {
        report.items.push(ItemResult {
            folder: item.folder.clone(),
            rel_path: item.rel_path.clone(),
            media: item.media,
            target: item.target.clone(),
            status: st.unwrap_or(ItemStatus::NotAttempted),
        });
    }
    report.recount();
    Ok(report)
}

impl JobReport {
    /// Items a resumed job must still try: never attempted, or stopped by
    /// a full, read-only or ejected volume.
    pub fn resumable_items(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                matches!(
                    i.status,
                    ItemStatus::NotAttempted
                        | ItemStatus::Failed {
                            kind: ItemErrorKind::WriteDenied
                                | ItemErrorKind::DiskFull
                                | ItemErrorKind::VolumeGone,
                            ..
                        }
                )
            })
            .map(|(n, _)| n)
            .collect()
    }

    /// Puts the outcomes of a resumed run (`other.items[k]` is item
    /// `indices[k]` of this report) into this report and recounts.
    pub fn merge_resumed(&mut self, indices: &[usize], other: JobReport) {
        for (&n, item) in indices.iter().zip(other.items) {
            if let Some(slot) = self.items.get_mut(n) {
                *slot = item;
            }
        }
        self.paused = other.paused;
        self.pause_kind = other.pause_kind;
        self.cancelled = other.cancelled;
        if self.job_id.is_none() {
            self.job_id = other.job_id;
        }
        self.recount();
    }

    fn recount(&mut self) {
        let (mut filled, mut replaced, mut skipped, mut no_art, mut failed) = (0, 0, 0, 0, 0);
        for item in &self.items {
            match &item.status {
                ItemStatus::Filled { .. } => filled += 1,
                ItemStatus::Replaced { .. } => replaced += 1,
                ItemStatus::Failed {
                    kind: ItemErrorKind::NoArt,
                    ..
                } => no_art += 1,
                ItemStatus::Failed { .. } => failed += 1,
                _ => skipped += 1,
            }
        }
        self.filled = filled;
        self.replaced = replaced;
        self.skipped = skipped;
        self.no_art = no_art;
        self.failed = failed;
    }
}

/// What every worker shares.
struct Ctx<'a, 's> {
    root: &'a Path,
    profile: &'a Profile,
    sources: &'a [&'a dyn ArtSource],
    store: &'a ArtStore<'s>,
    writer: &'a Mutex<Writer>,
    opts: &'a ApplyOptions,
    /// Why the queue stopped (the first reason wins).
    stop: &'a Mutex<Option<(String, PauseKind)>>,
    cancel: &'a AtomicBool,
}

impl Ctx<'_, '_> {
    fn halted(&self) -> bool {
        self.cancel.load(Ordering::SeqCst) || lock(self.stop).is_some()
    }

    fn pause(&self, why: String, kind: PauseKind) {
        lock(self.stop).get_or_insert((why, kind));
    }

    /// Pauses the job when the library root is gone or unreachable (an
    /// ejected volume, spec §10).
    fn volume_gone(&self) -> bool {
        match fs::metadata(self.root) {
            Ok(meta) if meta.is_dir() => false,
            Ok(_) => {
                self.pause_gone("not a folder".into());
                true
            }
            Err(e) => {
                self.pause_gone(e.to_string());
                true
            }
        }
    }

    fn pause_gone(&self, why: String) {
        self.pause(
            format!("the library volume is gone: {}: {why}", self.root.display()),
            PauseKind::Volume,
        );
    }

    fn apply_item(&self, item: &PlanItem) -> ItemStatus {
        if self.volume_gone() {
            // Nothing is downloaded for a library that is not there.
            return ItemStatus::NotAttempted;
        }
        let (chosen, alternatives) = match &item.action {
            PlanAction::Auto {
                chosen,
                alternatives,
                ..
            }
            | PlanAction::Review {
                chosen,
                alternatives,
                ..
            } => (chosen, alternatives),
            PlanAction::None { .. } => return ItemStatus::SkippedNoMatch,
        };
        let Some(slot) = self.profile.slot(item.media) else {
            return failed(
                ItemErrorKind::NameInvalid,
                format!("profile {} has no {:?} slot", self.profile.id, item.media),
            );
        };
        let spec = OutputSpec::for_slot(slot, self.opts.fit, self.opts.jpeg_quality);
        let mut last = (
            ItemErrorKind::NoArt,
            "no candidate could be fetched".to_string(),
        );
        let candidates =
            std::iter::once(chosen).chain(alternatives.iter().take(self.opts.max_alternatives));
        for c in candidates {
            if self.halted() {
                // Another worker paused the job, or the user cancelled.
                return ItemStatus::NotAttempted;
            }
            let Some(src) = self.sources.iter().find(|s| s.id() == c.source) else {
                continue;
            };
            let (bytes, cached) = match self.store.get(&c.url) {
                Some(b) => (b, true),
                None => match src.fetch(c) {
                    Ok(b) => (b, false),
                    Err(SourceError::Net(e)) if e.pauses() => {
                        self.pause(e.to_string(), e.pause_kind().unwrap_or(PauseKind::Offline));
                        return ItemStatus::NotAttempted;
                    }
                    Err(e @ SourceError::NotFound(_)) => {
                        last = (ItemErrorKind::NoArt, e.to_string());
                        continue;
                    }
                    Err(e) => {
                        last = (ItemErrorKind::Network, e.to_string());
                        continue;
                    }
                },
            };
            let img = match process_image(&bytes, &spec, &self.opts.image_limits) {
                Ok(img) => img,
                Err(e) => {
                    last = (ItemErrorKind::Decode, decode_message(c, &e));
                    continue;
                }
            };
            if !cached {
                // Only images that decode are kept: an error page served
                // with a 200 is downloaded again next time.
                let _ = self.store.put(&c.url, &bytes);
            }
            return self.write(item, c, &img.bytes);
        }
        failed(last.0, last.1)
    }

    fn write(&self, item: &PlanItem, c: &Candidate, bytes: &[u8]) -> ItemStatus {
        let outcome = lock(self.writer).write(&item.target, bytes, self.opts.overwrite);
        match outcome {
            Ok(WriteOutcome::Created { .. }) => ItemStatus::Filled {
                source: c.source.clone(),
                name: c.name.clone(),
            },
            Ok(WriteOutcome::Replaced { backup, .. }) => ItemStatus::Replaced {
                source: c.source.clone(),
                name: c.name.clone(),
                backup,
            },
            Ok(WriteOutcome::SkippedExists { .. }) => ItemStatus::SkippedExists,
            Err(e @ (WriteError::DiskFull | WriteError::Denied(_))) => {
                self.pause(
                    format!("the library volume is full or read-only: {e}"),
                    PauseKind::Volume,
                );
                let kind = if matches!(e, WriteError::DiskFull) {
                    ItemErrorKind::DiskFull
                } else {
                    ItemErrorKind::WriteDenied
                };
                failed(kind, e.to_string())
            }
            Err(e @ WriteError::NameInvalid(_)) => {
                failed(ItemErrorKind::NameInvalid, e.to_string())
            }
            Err(e @ WriteError::OutsideRoot(_)) => {
                failed(ItemErrorKind::OutsideRoot, e.to_string())
            }
            Err(e) if self.volume_gone() => failed(ItemErrorKind::VolumeGone, e.to_string()),
            Err(e) => failed(ItemErrorKind::Io, e.to_string()),
        }
    }
}

fn decode_message(c: &Candidate, e: &ImageError) -> String {
    format!("{}: {e}", c.name)
}

fn failed(kind: ItemErrorKind, message: String) -> ItemStatus {
    ItemStatus::Failed { kind, message }
}
