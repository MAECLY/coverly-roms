//! App-wide state behind the IPC commands: the open library session, the
//! generation counter that lets a newer open supersede an older one, and
//! the settings (loaded once, saved on every change; each job keeps the
//! snapshot it started with). Free of Tauri types so it is unit-testable.

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
use coverly_core::credentials::{CredentialStore, MemoryStore};
use coverly_core::fill::{remove_art, write_local_image, FillSettings, RomRef};
use coverly_core::gather::Scope;
use coverly_core::jobs::{write_report_csv, ApplyOptions, ItemErrorKind, ItemStatus};
use coverly_core::net::NetConfig;
use coverly_core::plan::{Decision, Plan};
use coverly_core::profiles::custom::{ProfileFile, ProfileStore, PROFILES_DIR};
use coverly_core::profiles::{builtin_profiles, Media};
use coverly_core::progress::NoProgress;
use coverly_core::settings::{
    backup_settings, load_settings, save_settings, LoadReport, RecentLibrary, Settings,
};
use coverly_core::systems::SystemDb;
use coverly_core::write::{list_jobs, undo_job, OverwritePolicy};

use crate::error::IpcError;
use crate::jobs::view::{
    candidate_views, candidates_of, CandidateView, ExportFormat, FillRequest, JobHistoryView,
    JobState, JobView, UndoView,
};
use crate::jobs::{with_env, FillConfig, JobEvents, JobManager, ManualWrite};
use crate::library::art::{inside_root, media_name, MAX_ART_BYTES};
use crate::library::refresh::{ids_for_art_paths, refresh_entries, subset_scan, EntryArtUpdate};
use crate::library::view::{LibrarySettingsView, LibraryView, RecentView, ScanProgressEvent};
use crate::library::{open_session, OpenRequest, Session};
use crate::prefs::{SettingsUpdateView, SettingsView};

pub struct AppState {
    latest: AtomicU64,
    session: Mutex<Option<Arc<Session>>>,
    config_dir: PathBuf,
    prefs: Mutex<Prefs>,
    /// Folders, endpoints and test timings; the settings complete it.
    base: FillConfig,
    jobs: JobManager,
    picks: Mutex<Option<EntryPick>>,
    credentials: Arc<dyn CredentialStore>,
}

/// The settings in use and what loading them had to do.
struct Prefs {
    settings: Settings,
    report: LoadReport,
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
        ItemErrorKind::VolumeGone => IpcError::VolumeGone(m),
        ItemErrorKind::Io => IpcError::Io(m),
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

/// Refuses a job data folder inside the library (backups would become
/// library files, and a scan would list them).
fn check_data_dir(cfg: &FillConfig, session: &Session) -> Result<(), IpcError> {
    let root = absolute_root(session)?;
    let data = std::path::absolute(&cfg.data_dir).map_err(|e| IpcError::Io(e.to_string()))?;
    let canon = fs::canonicalize(&data).ok();
    if data.starts_with(&root)
        || data.starts_with(&session.root_canon)
        || canon.is_some_and(|c| c.starts_with(&session.root_canon))
    {
        return Err(IpcError::BadRequest(
            "the backup folder is inside the library".into(),
        ));
    }
    Ok(())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Built-in and custom profile ids.
fn known_ids(custom: &[ProfileFile]) -> Vec<String> {
    builtin_profiles()
        .into_iter()
        .map(|p| p.id)
        .chain(custom.iter().map(|c| c.id.clone()))
        .collect()
}

/// The custom profiles folder: the setting, else `<config>/profiles`.
fn custom_dir_store(config_dir: &Path, custom_dir: Option<&Path>) -> ProfileStore {
    ProfileStore::new(custom_dir.map_or_else(|| config_dir.join(PROFILES_DIR), Path::to_path_buf))
}

/// The top-level groups that differ (never values: they may be paths).
fn changed_groups(before: &Settings, after: &Settings) -> Vec<String> {
    let (Ok(serde_json::Value::Object(a)), Ok(serde_json::Value::Object(b))) =
        (serde_json::to_value(before), serde_json::to_value(after))
    else {
        return Vec::new();
    };
    b.iter()
        .filter(|(k, v)| a.get(*k) != Some(*v))
        .map(|(k, _)| k.clone())
        .collect()
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
    /// Loads the settings of `config_dir` once (a broken file never fails:
    /// defaults for the bad parts, a backup, a notice in [`Self::settings_view`]).
    pub fn new(config_dir: PathBuf, base: FillConfig) -> AppState {
        let custom = custom_dir_store(&config_dir, None).load().0;
        let (mut settings, mut report) = load_settings(&config_dir, &known_ids(&custom));
        // Custom profiles elsewhere: the default profile may be one of them.
        if let Some(dir) = settings.advanced.custom_dir.clone() {
            let custom = ProfileStore::new(dir).load().0;
            let (s, r) = load_settings(&config_dir, &known_ids(&custom));
            settings = s;
            if r.notice.is_some() {
                report = r;
            }
        }
        AppState {
            latest: AtomicU64::new(0),
            session: Mutex::new(None),
            config_dir,
            prefs: Mutex::new(Prefs { settings, report }),
            base,
            jobs: JobManager::default(),
            picks: Mutex::new(None),
            credentials: Arc::new(MemoryStore::default()),
        }
    }

    /// Uses `store` for source credentials (the OS keychain in the app).
    pub fn with_credentials(mut self, store: Arc<dyn CredentialStore>) -> AppState {
        self.credentials = store;
        self
    }

    /// Where source credentials are kept.
    pub fn credentials(&self) -> &dyn CredentialStore {
        self.credentials.as_ref()
    }

    /// The settings in use.
    pub fn settings(&self) -> Settings {
        lock(&self.prefs).settings.clone()
    }

    /// The settings with what loading them had to do.
    pub fn settings_view(&self) -> SettingsView {
        let p = lock(&self.prefs);
        SettingsView::new(p.settings.clone(), &p.report)
    }

    /// The custom profiles folder.
    pub fn custom_store(&self) -> ProfileStore {
        custom_dir_store(
            &self.config_dir,
            self.settings().advanced.custom_dir.as_deref(),
        )
    }

    /// The custom profiles that load (bad files are skipped).
    pub fn custom_profiles(&self) -> Vec<ProfileFile> {
        self.custom_store().load().0
    }

    /// Built-in and custom profile ids.
    pub fn known_profiles(&self) -> Vec<String> {
        known_ids(&self.custom_profiles())
    }

    /// Validates and saves `patch`; field problems come back as data and
    /// nothing is saved then.
    pub fn update_settings(
        &self,
        patch: &serde_json::Value,
    ) -> Result<SettingsUpdateView, IpcError> {
        let known = self.known_profiles();
        let mut p = lock(&self.prefs);
        match p.settings.with_patch(patch, &known) {
            Err(errors) => Ok(SettingsUpdateView {
                settings: p.settings.clone(),
                errors,
                reopen: false,
            }),
            Ok(next) => {
                save_settings(&self.config_dir, &next)
                    .map_err(|e| IpcError::Settings(e.to_string()))?;
                let reopen = p.settings.needs_reopen(&next);
                log::info!(
                    "settings saved: {}",
                    changed_groups(&p.settings, &next).join(", ")
                );
                p.settings = next.clone();
                p.report = LoadReport::default();
                Ok(SettingsUpdateView {
                    settings: next,
                    errors: Vec::new(),
                    reopen,
                })
            }
        }
    }

    /// Defaults again (recent libraries kept), after a backup copy.
    pub fn reset_settings(&self) -> Result<SettingsView, IpcError> {
        let mut p = lock(&self.prefs);
        backup_settings(&self.config_dir, "reset")
            .map_err(|e| IpcError::Settings(e.to_string()))?;
        let mut next = Settings::default();
        next.library.recent = p.settings.library.recent.clone();
        save_settings(&self.config_dir, &next).map_err(|e| IpcError::Settings(e.to_string()))?;
        log::info!("settings reset to defaults");
        p.settings = next;
        p.report = LoadReport::default();
        Ok(SettingsView::new(p.settings.clone(), &p.report))
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// `<app data dir>/logs` (never the backup location).
    pub fn logs_dir(&self) -> PathBuf {
        self.base.data_dir.join("logs")
    }

    /// The fill configuration with the current settings: each job keeps
    /// the snapshot it started with.
    pub fn fill_config(&self) -> Arc<FillConfig> {
        let s = self.settings();
        Arc::new(FillConfig {
            cache_dir: self.base.cache_dir.clone(),
            data_dir: s.data_dir(&self.base.data_dir),
            endpoints: self.base.endpoints.clone(),
            net: NetConfig {
                min_interval: self.base.net.min_interval,
                backoff_base: self.base.net.backoff_base,
                ..s.net_config()
            },
            overrides_dir: self.base.overrides_dir.clone(),
            settings: s,
        })
    }

    /// Bytes used by the cache (database and downloaded art).
    pub fn cache_usage(&self) -> u64 {
        coverly_core::cache::cache_usage(&self.base.cache_dir)
    }

    /// Clears the cache; refused while a job or a manual write runs.
    pub fn clear_cache(&self) -> Result<u64, IpcError> {
        let _writing = self.jobs.begin_manual()?;
        coverly_core::cache::clear_cache(&self.base.cache_dir)
            .map_err(|e| IpcError::Io(e.to_string()))
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
        let cfg = self.fill_config();
        check_data_dir(&cfg, &session)?;
        let view = self.jobs.start(cfg, session, req, events)?;
        log::info!("fill job {} started", view.id);
        Ok(view)
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
        if paused {
            return Err(IpcError::JobRunning);
        }
        let _writing = self.jobs.begin_manual()?;
        let session = self.session(generation)?;
        let root = std::path::absolute(&session.root).map_err(|e| IpcError::Io(e.to_string()))?;
        let data_dir = self.fill_config().data_dir.clone();
        let job = list_jobs(&data_dir)?
            .into_iter()
            .find(|j| !j.undone && j.root == root)
            .ok_or(IpcError::NothingToUndo)?;
        let report = undo_job(&data_dir, &job.job)?;
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
    fn known_entry(&self, generation: u64, id: u32) -> Result<Arc<Session>, IpcError> {
        if self.jobs.busy() {
            return Err(IpcError::JobRunning);
        }
        let session = self.session(generation)?;
        if session.entries.get(id as usize).is_none() {
            return Err(IpcError::UnknownEntry(id));
        }
        Ok(session)
    }

    /// [`Self::known_entry`], marked as the one writer of the library
    /// until the guard drops, with the settings snapshot of the write. The
    /// job data folder is checked first.
    fn writable_entry(
        &self,
        generation: u64,
        id: u32,
    ) -> Result<(ManualWrite<'_>, Arc<Session>, Arc<FillConfig>), IpcError> {
        let writing = self.jobs.begin_manual()?;
        let session = self.known_entry(generation, id)?;
        let cfg = self.fill_config();
        check_data_dir(&cfg, &session)?;
        Ok((writing, session, cfg))
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
        let session = self.known_entry(generation, id)?;
        if session.profile.slot(media).is_none() {
            return Err(IpcError::BadRequest(
                "this layout cannot store that media".into(),
            ));
        }
        let cfg = self.fill_config();
        let plan = with_env(&cfg, |env| {
            let db = SystemDb::builtin();
            let (scan, _) = subset_scan(&session, &[id]);
            let art = resolve_art(&scan, &session.profile, &db, &[media]);
            let settings = FillSettings {
                media: vec![media],
                scope: Scope::All,
                overrides_dir: Some(cfg.overrides_dir.clone()),
                ..cfg.settings.fill_settings()
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
        with_env(&self.fill_config(), |env| {
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
        let (_writing, session, cfg) = self.writable_entry(generation, id)?;
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
            // A failed or paused change leaves no job (like a local image).
            discard_empty: true,
            resume_job: None,
            ..cfg.settings.apply_options(&absolute_root(&session)?)
        };
        let report = with_env(&cfg, |env| {
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
        let (_writing, session, cfg) = self.writable_entry(generation, id)?;
        let target = art_target(&session, id, media)?;
        let bytes = read_capped(file)?;
        let entry = &session.entries[id as usize];
        let sys = &session.systems[entry.system as usize];
        let rel_path = entry
            .rom
            .path
            .strip_prefix(&sys.dir)
            .unwrap_or(&entry.rom.path);
        let rom = RomRef {
            system_id: &entry.system_id,
            folder: &sys.folder,
            system_dir: &sys.dir,
            rel_path,
            stem: &entry.rom.stem,
        };
        write_local_image(
            &absolute_root(&session)?,
            &session.profile,
            &rom,
            media,
            &target,
            &bytes,
            &cfg.data_dir,
            &cfg.settings.manual_options(),
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
        let (_writing, session, cfg) = self.writable_entry(generation, id)?;
        let path = session.entries[id as usize]
            .art
            .get(&media)
            .cloned()
            .ok_or_else(|| IpcError::ArtMissing(media_name(media)))?;
        inside_root(&session, &path)?;
        remove_art(
            &absolute_root(&session)?,
            &path,
            &cfg.data_dir,
            cfg.settings.manual_options().keep_jobs,
        )?;
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
        Ok(list_jobs(&self.fill_config().data_dir)?
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
        let settings = self.settings();
        let custom = self.custom_profiles();
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
                custom: &custom,
                default_profile: settings.library.default_profile.as_deref(),
                settings: Some(&settings),
            },
            systems,
            &mut |p| emit(ScanProgressEvent::new(generation, p)),
        )?;
        self.install(session)?;
        log::info!(
            "library opened: {} ({} ROMs)",
            view.name,
            view.entries.len()
        );
        let mut p = lock(&self.prefs);
        let mut next = p.settings.clone();
        next.remember(RecentLibrary {
            path: path.to_path_buf(),
            name: view.name.clone(),
            profile_id: view.profile.id.clone(),
            roms: u32::try_from(view.entries.len()).unwrap_or(u32::MAX),
            opened_at: now_unix(),
        });
        match save_settings(&self.config_dir, &next) {
            Ok(()) => p.settings = next,
            Err(e) => {
                // Remembered for this session anyway.
                p.settings = next;
                log::warn!("cannot save settings in {}: {e}", self.config_dir.display());
            }
        }
        Ok(view)
    }

    /// Applies `edit` to the settings and saves them.
    fn edit(&self, edit: impl FnOnce(&mut Settings)) -> Result<Settings, IpcError> {
        let mut p = lock(&self.prefs);
        let mut next = p.settings.clone();
        edit(&mut next);
        save_settings(&self.config_dir, &next).map_err(|e| IpcError::Settings(e.to_string()))?;
        p.settings = next.clone();
        Ok(next)
    }

    /// Recent libraries, most recent first, with their availability.
    pub fn recent(&self) -> Vec<RecentView> {
        recent_views(&self.settings())
    }

    pub fn forget(&self, path: &Path) -> Result<Vec<RecentView>, IpcError> {
        let settings = self.edit(|s| s.forget(path))?;
        Ok(recent_views(&settings))
    }

    pub fn library_settings(&self) -> LibrarySettingsView {
        LibrarySettingsView {
            ignored_dirs: self.settings().library.ignored_dirs,
        }
    }

    /// Saves the ignored folders (normalised) through the settings update.
    pub fn set_ignored_dirs(&self, dirs: Vec<String>) -> Result<LibrarySettingsView, IpcError> {
        let out =
            self.update_settings(&serde_json::json!({ "library": { "ignored_dirs": dirs } }))?;
        if let Some(e) = out.errors.first() {
            return Err(IpcError::BadRequest(format!("{} is not valid", e.field)));
        }
        Ok(LibrarySettingsView {
            ignored_dirs: out.settings.library.ignored_dirs,
        })
    }

    /// The shared SQLite cache (hash memo, DATs), created on first use.
    pub fn cache(&self) -> Result<Cache, IpcError> {
        let dir = &self.base.cache_dir;
        std::fs::create_dir_all(dir).map_err(|e| IpcError::Io(e.to_string()))?;
        Cache::open(&dir.join(CACHE_FILE)).map_err(|e| IpcError::Io(e.to_string()))
    }
}

fn recent_views(settings: &Settings) -> Vec<RecentView> {
    settings
        .library
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pulled_volume_is_its_own_error() {
        let e = item_error(ItemErrorKind::VolumeGone, "/sd gone");
        assert_eq!(e, IpcError::VolumeGone("/sd gone".into()));
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            serde_json::json!({ "code": "volume-gone", "detail": "/sd gone" })
        );
        assert_eq!(item_error(ItemErrorKind::Io, "x"), IpcError::Io("x".into()));
    }
}
