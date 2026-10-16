//! App-wide state behind the IPC commands: the open library session, the
//! generation counter that lets a newer open supersede an older one, and
//! the library settings file. Free of Tauri types so it is unit-testable.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use coverly_core::art::{resolve_art, target_path};
use coverly_core::audit::{build_report, write_csv};
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::fill::{remove_art, write_local_image, FillSettings};
use coverly_core::gather::Scope;
use coverly_core::jobs::{write_report_csv, ApplyOptions, ItemErrorKind, ItemStatus};
use coverly_core::plan::{Decision, Plan};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::settings::{LibrarySettings, RecentLibrary};
use coverly_core::systems::SystemDb;
use coverly_core::write::{list_jobs, undo_job, OverwritePolicy};

use crate::error::IpcError;
use crate::jobs::view::{
    candidate_views, candidates_of, CandidateView, ExportFormat, FillRequest, JobHistoryView,
    JobState, JobView, UndoView,
};
use crate::jobs::{with_env, FillConfig, JobEvents, JobManager};
use crate::library::art::{inside_root, media_name, MAX_ART_BYTES};
use crate::library::refresh::{ids_for_art_paths, refresh_entries, subset_scan, EntryArtUpdate};
use crate::library::view::{LibrarySettingsView, LibraryView, RecentView, ScanProgressEvent};
use crate::library::{open_session, OpenRequest, Session};

pub struct AppState {
    latest: AtomicU64,
    session: Mutex<Option<Arc<Session>>>,
    settings_path: PathBuf,
    fill: Arc<FillConfig>,
    jobs: JobManager,
    picks: Mutex<Option<EntryPick>>,
}

/// The candidates last listed in the drawer, so a pick names one of them.
struct EntryPick {
    generation: u64,
    id: u32,
    media: Media,
    plan: Plan,
}

fn item_error(kind: ItemErrorKind, message: &str) -> IpcError {
    let m = message.to_string();
    match kind {
        ItemErrorKind::NoArt | ItemErrorKind::Network => IpcError::Network(m),
        ItemErrorKind::Decode => IpcError::BadImage(m),
        ItemErrorKind::WriteDenied => IpcError::WriteDenied(m),
        ItemErrorKind::DiskFull => IpcError::DiskFull,
        ItemErrorKind::NameInvalid => IpcError::NameInvalid(m),
        ItemErrorKind::OutsideRoot => IpcError::OutsideLibrary(m),
        ItemErrorKind::VolumeGone | ItemErrorKind::Io => IpcError::Io(m),
    }
}

fn absolute_root(session: &Session) -> Result<PathBuf, IpcError> {
    std::path::absolute(&session.root).map_err(|e| IpcError::Io(e.to_string()))
}

/// Where the profile stores `media` for entry `id`.
fn art_target(session: &Session, id: u32, media: Media) -> Result<PathBuf, IpcError> {
    let entry = &session.entries[id as usize];
    let sys = &session.systems[entry.system as usize];
    let db = SystemDb::builtin();
    let system = db.get(&entry.system_id).ok_or(IpcError::UnknownEntry(id))?;
    target_path(
        &absolute_root(session)?,
        &session.profile,
        &sys.dir,
        system,
        &entry.rom.rel_dir,
        &entry.rom.stem,
        media,
    )
    .ok_or_else(|| IpcError::BadRequest("this layout cannot store that media".into()))
}

/// A picked file, at most [`MAX_ART_BYTES`].
fn read_capped(file: &Path) -> Result<Vec<u8>, IpcError> {
    let shown = file.display().to_string();
    let meta = fs::metadata(file).map_err(|_| IpcError::Unreadable(shown.clone()))?;
    if !meta.is_file() {
        return Err(IpcError::BadImage(shown));
    }
    if meta.len() > MAX_ART_BYTES {
        return Err(IpcError::ArtTooLarge(meta.len()));
    }
    let mut bytes = Vec::new();
    File::open(file)
        .and_then(|f| f.take(MAX_ART_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| IpcError::Io(e.to_string()))?;
    if bytes.len() as u64 > MAX_ART_BYTES {
        return Err(IpcError::ArtTooLarge(bytes.len() as u64));
    }
    Ok(bytes)
}

fn write_export(
    dest: &Path,
    f: impl FnOnce(&mut BufWriter<File>) -> io::Result<()>,
) -> Result<(), IpcError> {
    let mut w = BufWriter::new(File::create(dest).map_err(|e| IpcError::Io(e.to_string()))?);
    f(&mut w)
        .and_then(|()| w.flush())
        .map_err(|e| IpcError::Io(e.to_string()))
}

impl AppState {
    pub fn new(settings_path: PathBuf, fill: FillConfig) -> AppState {
        AppState {
            latest: AtomicU64::new(0),
            session: Mutex::new(None),
            settings_path,
            fill: Arc::new(fill),
            jobs: JobManager::default(),
            picks: Mutex::new(None),
        }
    }

    /// Reserves the generation of a new open; any older open in flight
    /// will be [`IpcError::Superseded`].
    pub fn begin_open(&self) -> u64 {
        self.latest.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Makes `session` the open library unless a newer open began.
    pub fn install(&self, session: Session) -> Result<(), IpcError> {
        let mut slot = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        if self.latest.load(Ordering::SeqCst) != session.generation {
            return Err(IpcError::Superseded);
        }
        *slot = Some(Arc::new(session));
        Ok(())
    }

    /// The open library, if it is `generation`.
    pub fn session(&self, generation: u64) -> Result<Arc<Session>, IpcError> {
        let slot = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        match slot.as_ref() {
            None => Err(IpcError::NoLibrary),
            Some(s) if s.generation != generation => Err(IpcError::StaleLibrary),
            Some(s) => Ok(Arc::clone(s)),
        }
    }

    /// The fill settings shared with job threads.
    pub fn fill_config(&self) -> Arc<FillConfig> {
        Arc::clone(&self.fill)
    }

    /// The one fill job of the app.
    pub fn jobs(&self) -> &JobManager {
        &self.jobs
    }

    /// Starts the dry run of a fill on the open library `generation`.
    pub fn start_fill(
        &self,
        generation: u64,
        req: &FillRequest,
        events: Arc<dyn JobEvents>,
    ) -> Result<JobView, IpcError> {
        let session = self.session(generation)?;
        self.jobs.start(self.fill_config(), session, req, events)
    }

    /// Undoes the newest not-undone journal of the open library
    /// `generation` (fill, drawer change, removal) and refreshes the
    /// entries it touched. Refused while a job gathers, applies or is
    /// paused (a paused job resumes into its journal); journals of other
    /// libraries are never touched. Journals keep `absolute(session.root)`
    /// (the root of every app write).
    pub fn undo_last_job(&self, generation: u64) -> Result<UndoView, IpcError> {
        let paused = self
            .jobs
            .current()
            .is_some_and(|j| j.state == JobState::Paused);
        if self.jobs.busy() || paused {
            return Err(IpcError::JobRunning);
        }
        let session = self.session(generation)?;
        let root = std::path::absolute(&session.root).map_err(|e| IpcError::Io(e.to_string()))?;
        let job = list_jobs(&self.fill.data_dir)?
            .into_iter()
            .find(|j| !j.undone && j.root == root)
            .ok_or(IpcError::NothingToUndo)?;
        let report = undo_job(&self.fill.data_dir, &job.job)?;
        let paths: Vec<PathBuf> = report
            .removed
            .iter()
            .chain(&report.restored)
            .cloned()
            .collect();
        let ids = ids_for_art_paths(&session, &paths);
        // The undo is done; a library reopened meanwhile only loses the refresh.
        let entries = self
            .refresh_entries(generation, &ids, &SystemDb::builtin())
            .unwrap_or_default();
        Ok(UndoView::new(&report, entries))
    }

    /// The open session, if entry `id` exists and no job is writing.
    fn writable_entry(&self, generation: u64, id: u32) -> Result<Arc<Session>, IpcError> {
        if self.jobs.busy() {
            return Err(IpcError::JobRunning);
        }
        let session = self.session(generation)?;
        if session.entries.get(id as usize).is_none() {
            return Err(IpcError::UnknownEntry(id));
        }
        Ok(session)
    }

    fn picked(&self, generation: u64, id: u32, media: Media) -> Result<Plan, IpcError> {
        let slot = self.picks.lock().unwrap_or_else(PoisonError::into_inner);
        match slot.as_ref() {
            Some(p) if p.generation == generation && p.id == id && p.media == media => {
                Ok(p.plan.clone())
            }
            _ => Err(IpcError::BadRequest("list the candidates first".into())),
        }
    }

    /// Every candidate for `media` of entry `id`, best first (the drawer's
    /// "Change…"). Existing art does not hide candidates.
    pub fn entry_candidates(
        &self,
        generation: u64,
        id: u32,
        media: Media,
    ) -> Result<Vec<CandidateView>, IpcError> {
        let session = self.writable_entry(generation, id)?;
        if session.profile.slot(media).is_none() {
            return Err(IpcError::BadRequest(
                "this layout cannot store that media".into(),
            ));
        }
        let plan = with_env(&self.fill, |env| {
            let db = SystemDb::builtin();
            let (scan, _) = subset_scan(&session, &[id]);
            let art = resolve_art(&scan, &session.profile, &db, &[media]);
            let settings = FillSettings {
                media: vec![media],
                scope: Scope::All,
                ..FillSettings::default()
            };
            Ok(env.plan(
                &absolute_root(&session)?,
                &session.profile,
                &db,
                &art,
                &settings,
                &NoProgress,
                &AtomicBool::new(false),
            ))
        })?;
        if let Some(why) = &plan.paused {
            return Err(IpcError::Network(why.clone()));
        }
        let candidates = plan.items.first().map(candidate_views).unwrap_or_default();
        *self.picks.lock().unwrap_or_else(PoisonError::into_inner) = Some(EntryPick {
            generation,
            id,
            media,
            plan,
        });
        Ok(candidates)
    }

    /// Candidate `candidate` of the listed ones, re-encoded as it would be written.
    pub fn entry_candidate_art(
        &self,
        generation: u64,
        id: u32,
        media: Media,
        candidate: u32,
    ) -> Result<Vec<u8>, IpcError> {
        let session = self.session(generation)?;
        let plan = self.picked(generation, id, media)?;
        let c = plan
            .items
            .first()
            .and_then(|item| {
                candidates_of(item)
                    .get(candidate as usize)
                    .map(|c| (*c).clone())
            })
            .ok_or_else(|| IpcError::BadRequest(format!("no candidate {candidate}")))?;
        with_env(&self.fill, |env| {
            Ok(env.candidate_image(&session.profile, &c)?.bytes)
        })
    }

    /// Writes candidate `candidate` of the listed ones (existing art backed up).
    pub fn set_entry_art(
        &self,
        generation: u64,
        id: u32,
        media: Media,
        candidate: u32,
    ) -> Result<Vec<EntryArtUpdate>, IpcError> {
        let session = self.writable_entry(generation, id)?;
        let mut plan = self.picked(generation, id, media)?;
        let known = plan
            .items
            .first()
            .map_or(0, |item| candidates_of(item).len());
        if candidate as usize >= known {
            return Err(IpcError::BadRequest(format!("no candidate {candidate}")));
        }
        plan.decide(&BTreeMap::from([(
            0,
            Decision::Pick {
                candidate: candidate as usize,
            },
        )]));
        let opts = ApplyOptions {
            overwrite: OverwritePolicy::Backup,
            include_review: false,
            concurrency: 1,
            ..ApplyOptions::default()
        };
        let report = with_env(&self.fill, |env| {
            env.apply(
                &plan,
                &session.profile,
                &opts,
                &NoProgress,
                &AtomicBool::new(false),
            )
            .map_err(IpcError::from)
        })?;
        if let Some(ItemStatus::Failed { kind, message }) = report.items.first().map(|i| &i.status)
        {
            return Err(item_error(*kind, message));
        }
        if let Some(why) = report.paused {
            return Err(IpcError::Network(why));
        }
        self.refresh_entries(generation, &[id], &SystemDb::builtin())
    }

    /// Writes the image file the user picked (decoded and re-encoded).
    pub fn set_entry_art_from_file(
        &self,
        generation: u64,
        id: u32,
        media: Media,
        file: &Path,
    ) -> Result<Vec<EntryArtUpdate>, IpcError> {
        let session = self.writable_entry(generation, id)?;
        let target = art_target(&session, id, media)?;
        let bytes = read_capped(file)?;
        write_local_image(
            &absolute_root(&session)?,
            &session.profile,
            media,
            &target,
            &bytes,
            &self.fill.data_dir,
        )?;
        self.refresh_entries(generation, &[id], &SystemDb::builtin())
    }

    /// Removes the art file of `media` (backed up, undoable).
    pub fn remove_entry_art(
        &self,
        generation: u64,
        id: u32,
        media: Media,
    ) -> Result<Vec<EntryArtUpdate>, IpcError> {
        let session = self.writable_entry(generation, id)?;
        let path = session.entries[id as usize]
            .art
            .get(&media)
            .cloned()
            .ok_or_else(|| IpcError::ArtMissing(media_name(media)))?;
        inside_root(&session, &path)?;
        remove_art(&absolute_root(&session)?, &path, &self.fill.data_dir)?;
        self.refresh_entries(generation, &[id], &SystemDb::builtin())
    }

    /// Writes the audit of the open library (read again from disk) to `dest`.
    pub fn export_audit(
        &self,
        generation: u64,
        format: ExportFormat,
        dest: &Path,
    ) -> Result<(), IpcError> {
        let session = self.session(generation)?;
        let media: Vec<Media> = Media::ALL
            .into_iter()
            .filter(|m| session.profile.slot(*m).is_some())
            .collect();
        let report = build_report(
            &session.root,
            &session.profile,
            &SystemDb::builtin(),
            &self.settings().scan_options(),
            &media,
        );
        write_export(dest, |w| match format {
            ExportFormat::Csv => write_csv(&report, w, false),
            ExportFormat::Json => {
                serde_json::to_writer_pretty(w, &report).map_err(io::Error::other)
            }
        })
    }

    /// Writes the item outcomes of job `job` to `dest`.
    pub fn export_job_report(
        &self,
        job: u64,
        format: ExportFormat,
        dest: &Path,
    ) -> Result<(), IpcError> {
        let report = self.jobs.report(job)?;
        write_export(dest, |w| match format {
            ExportFormat::Csv => write_report_csv(&report, w),
            ExportFormat::Json => {
                serde_json::to_writer_pretty(w, &report).map_err(io::Error::other)
            }
        })
    }

    /// Every journal kept, newest first.
    pub fn job_history(&self) -> Result<Vec<JobHistoryView>, IpcError> {
        Ok(list_jobs(&self.fill.data_dir)?
            .into_iter()
            .map(JobHistoryView::from)
            .collect())
    }

    /// Resolves the art of `ids` again and installs the updated session,
    /// if `generation` is still the open library.
    pub fn refresh_entries(
        &self,
        generation: u64,
        ids: &[u32],
        systems: &SystemDb,
    ) -> Result<Vec<EntryArtUpdate>, IpcError> {
        let mut slot = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        let current = match slot.as_ref() {
            None => return Err(IpcError::NoLibrary),
            Some(s) if s.generation != generation => return Err(IpcError::StaleLibrary),
            Some(s) => Arc::clone(s),
        };
        let (next, updates) = refresh_entries(&current, ids, systems);
        *slot = Some(Arc::new(next));
        Ok(updates)
    }

    /// Opens `path` (blocking), installs it and remembers it as recent. A
    /// settings file that cannot be written never fails the open.
    pub fn open(
        &self,
        path: &Path,
        profile_id: Option<&str>,
        systems: &SystemDb,
        emit: &mut dyn FnMut(ScanProgressEvent),
    ) -> Result<LibraryView, IpcError> {
        let mut settings = self.settings();
        let generation = self.begin_open();
        // Announce the new generation at once so the UI can drop progress
        // from a superseded scan that is still running.
        emit(ScanProgressEvent {
            generation,
            done: 0,
            total: 0,
            folder: String::new(),
            roms: 0,
        });
        let (session, view) = open_session(
            &OpenRequest {
                path,
                profile_id,
                scan: &settings.scan_options(),
                generation,
            },
            systems,
            &mut |p| emit(ScanProgressEvent::new(generation, p)),
        )?;
        self.install(session)?;
        settings.remember(RecentLibrary {
            path: path.to_path_buf(),
            name: view.name.clone(),
            profile_id: view.profile.id.clone(),
            roms: u32::try_from(view.entries.len()).unwrap_or(u32::MAX),
            opened_at: now_unix(),
        });
        if let Err(e) = settings.save(&self.settings_path) {
            eprintln!("warning: cannot save {}: {e}", self.settings_path.display());
        }
        Ok(view)
    }

    pub fn settings(&self) -> LibrarySettings {
        LibrarySettings::load(&self.settings_path)
    }

    fn save(&self, settings: &LibrarySettings) -> Result<(), IpcError> {
        settings
            .save(&self.settings_path)
            .map_err(|e| IpcError::Settings(e.to_string()))
    }

    /// Recent libraries, most recent first, with their availability.
    pub fn recent(&self) -> Vec<RecentView> {
        recent_views(&self.settings())
    }

    pub fn forget(&self, path: &Path) -> Result<Vec<RecentView>, IpcError> {
        let mut settings = self.settings();
        settings.forget(path);
        self.save(&settings)?;
        Ok(recent_views(&settings))
    }

    pub fn library_settings(&self) -> LibrarySettingsView {
        LibrarySettingsView {
            ignored_dirs: self.settings().ignored_dirs,
        }
    }

    pub fn set_ignored_dirs(&self, dirs: Vec<String>) -> Result<LibrarySettingsView, IpcError> {
        let mut settings = self.settings();
        settings.set_ignored_dirs(dirs);
        self.save(&settings)?;
        Ok(LibrarySettingsView {
            ignored_dirs: settings.ignored_dirs,
        })
    }

    /// The shared SQLite cache (hash memo, DATs), created on first use.
    pub fn cache(&self) -> Result<Cache, IpcError> {
        std::fs::create_dir_all(&self.fill.cache_dir).map_err(|e| IpcError::Io(e.to_string()))?;
        Cache::open(&self.fill.cache_dir.join(CACHE_FILE)).map_err(|e| IpcError::Io(e.to_string()))
    }
}

fn recent_views(settings: &LibrarySettings) -> Vec<RecentView> {
    settings
        .recent
        .iter()
        .map(|r| RecentView {
            path: r.path.display().to_string(),
            name: r.name.clone(),
            profile_id: r.profile_id.clone(),
            roms: r.roms,
            opened_at: r.opened_at,
            available: r.path.is_dir(),
        })
        .collect()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
