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
use coverly_core::credentials::{
    store_secret, CredentialKey, CredentialStatus, CredentialStore, MemoryStore, SourceKeys,
};
use coverly_core::fill::{remove_art, write_local_image, FillSettings, RomRef};
use coverly_core::gather::Scope;
use coverly_core::identify::dat::DatStore;
use coverly_core::identify::overrides::{
    forget_manual, remember_manual, Overrides, OVERRIDES_FILE,
};
use coverly_core::jobs::{write_report_csv, ApplyOptions, ItemErrorKind, ItemStatus};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::organize::evidence::OnlineLookup;
use coverly_core::organize::online::ScreenScraperLookup;
use coverly_core::organize::{
    apply_organize, load_plan, plan_from_survey, probe_folder, resume_organize, survey,
    Decision as OrganizeDecision, OrganizeEnv, OrganizeOptions, OrganizePlan, OsMetaProbe,
};
use coverly_core::plan::{Decision, Plan};
use coverly_core::profiles::custom::{
    all_profiles, preview_paths, validate_profile, PreviewExample, ProfileFile, ProfileStore,
    PROFILES_DIR,
};
use coverly_core::profiles::{builtin_profiles, Media, Profile};
use coverly_core::progress::NoProgress;
use coverly_core::settings::{
    backup_settings, load_settings, save_settings, FieldError, FieldErrorCode, LoadReport,
    RecentLibrary, Settings,
};
use coverly_core::sources::screenscraper::{ScreenScraperSource, SsConfig};
use coverly_core::sources::Candidate;
use coverly_core::systems::SystemDb;
use coverly_core::write::{
    later_jobs, list_jobs, undo_job, undo_jobs_in_order_with, JobKind, OverwritePolicy,
};

use crate::error::IpcError;
use crate::jobs::view::{
    candidate_view, candidate_views, candidates_of, CandidateView, ExportFormat, FillRequest,
    JobHistoryView, JobState, JobView, UndoView,
};
use crate::jobs::{with_env, FillConfig, JobEvents, JobManager, ManualWrite};
use crate::library::art::{inside_root, media_name, MAX_ART_BYTES};
use crate::library::identify::IdentityView;
use crate::library::refresh::{ids_for_art_paths, refresh_entries, subset_scan, EntryArtUpdate};
use crate::library::view::{LibrarySettingsView, LibraryView, RecentView, ScanProgressEvent};
use crate::library::{open_session, OpenRequest, Session};
use crate::organize::view::{
    FolderProbeView, OrganizeJobView, OrganizePlanView, OrganizeRequest, OrganizeUndoView,
};
use crate::organize::{ApplyStart, OrganizeEvents, OrganizeManager, StoredPlan, Throttle};
use crate::prefs::{SettingsUpdateView, SettingsView};
use crate::profiles::{ProfileInfoView, ProfilePreviewView, ProfileSaveView};
use coverly_core::archive::external::{discover, discover_with, FoundTool, Search};
use coverly_core::archive::{Limits, ToolsOnDemand};
use coverly_core::dedupe::archive::ArchiveEntryListing;
use coverly_core::dedupe::disc::ReaderDiscProbe;
use coverly_core::dedupe::keys::{DiskBytes, FileBytes};
use coverly_core::dedupe::{
    apply_dedupe, load_dedupe_plan, plan_dedupe, regroup, resume_dedupe, set_keeper, DedupeEnv,
    DedupeOptions, QuarantineChoice,
};
use coverly_core::scan::scan_library;
use coverly_core::write::trash::{OsTrash, TrashBackend};

use crate::dedupe::view::{DedupeJobView, DedupePlanView, DedupeRequest};
use crate::dedupe::{
    ArtSource as DedupeArt, DedupeEvents, DedupeManager, StoredDedupe, Throttle as DedupeThrottle,
};
use coverly_core::dedupe::art::{libretro_cached_url, OfflineHttp};
use coverly_core::jobs::ArtStore;
use coverly_core::sources::libretro::{LibretroConfig, LibretroSource, INDEX_TTL_SECS};

pub struct AppState {
    latest: AtomicU64,
    session: Mutex<Option<Arc<Session>>>,
    config_dir: PathBuf,
    prefs: Mutex<Prefs>,
    /// Folders, endpoints and test timings; the settings complete it.
    base: FillConfig,
    jobs: JobManager,
    picks: Mutex<Option<EntryPick>>,
    searches: Mutex<Option<EntrySearch>>,
    credentials: Arc<dyn CredentialStore>,
    /// The source keys last read from `credentials`, so drawing thumbnails
    /// or listing history never goes back to the keychain.
    keys: Mutex<KeyCache>,
    /// Organise plans by id and the survey in progress.
    organize: OrganizeManager,
    /// Duplicates plans by id, the plan being made and the apply.
    dedupe: DedupeManager,
    /// Where duplicates go when sent to the Trash, and where undo looks for
    /// them (the OS Trash; tests use a fake one).
    trash: Arc<dyn TrashBackend>,
    /// How duplicates planning opens loose files (a seam to count reads).
    file_bytes: Arc<dyn FileBytes>,
    /// Where external archive tools are looked for (`None` = this
    /// process's `PATH` and the fixed locations; tests point it elsewhere).
    tool_search: Option<Search>,
}

/// A snapshot of the stored keys, dropped when a credential is saved or
/// removed. `generation` keeps a read that raced a save from being kept.
#[derive(Default)]
struct KeyCache {
    generation: u64,
    keys: Option<SourceKeys>,
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

/// The candidates last found by "Search by title", so a pick names one of them.
struct EntrySearch {
    generation: u64,
    id: u32,
    media: Media,
    candidates: Vec<Candidate>,
}

/// The IPC error of a failed (or paused) item.
pub(crate) fn item_error(kind: ItemErrorKind, message: &str) -> IpcError {
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

/// The art file `path` a duplicates plan recorded, when its canonical path
/// stays inside `root`'s (a symlink out of the library is refused) and it
/// is a file of at most [`MAX_ART_BYTES`]. Every failure is `None`.
fn read_plan_art(root: &Path, path: &Path) -> Option<Vec<u8>> {
    let (canon_root, canon) = match (fs::canonicalize(root), fs::canonicalize(path)) {
        (Ok(r), Ok(p)) => (r, p),
        _ => {
            log::debug!("duplicates art: {} is unreadable", path.display());
            return None;
        }
    };
    if !canon.starts_with(&canon_root) {
        log::warn!(
            "duplicates art: {} is outside the library, not served",
            path.display()
        );
        return None;
    }
    let meta = fs::metadata(&canon).ok()?;
    if !meta.is_file() || meta.len() > MAX_ART_BYTES {
        log::debug!("duplicates art: {} is not a servable file", path.display());
        return None;
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    File::open(&canon)
        .and_then(|f| f.take(MAX_ART_BYTES + 1).read_to_end(&mut bytes))
        .ok()?;
    (bytes.len() as u64 <= MAX_ART_BYTES).then_some(bytes)
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

/// Refuses an organise write whose backup folder (journals, saved plans)
/// is inside the pile or the organised folder: like a fill, the journal
/// must never live in the tree it writes to (a later fill there would be
/// refused, and the files of the job would be organised with the pile).
fn check_organize_data_dir(cfg: &FillConfig, roots: &[&Path]) -> Result<(), IpcError> {
    let data = std::path::absolute(&cfg.data_dir).map_err(|e| IpcError::Io(e.to_string()))?;
    let canon = fs::canonicalize(&data).ok();
    for root in roots {
        let abs = std::path::absolute(root).map_err(|e| IpcError::Io(e.to_string()))?;
        let root_canon = fs::canonicalize(&abs).ok();
        let inside = data.starts_with(&abs)
            || root_canon.as_ref().is_some_and(|r| {
                data.starts_with(r) || canon.as_ref().is_some_and(|c| c.starts_with(r))
            });
        if inside {
            return Err(IpcError::BadRequest(
                "the backup folder is inside the folder being organised".into(),
            ));
        }
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
            searches: Mutex::new(None),
            credentials: Arc::new(MemoryStore::default()),
            keys: Mutex::new(KeyCache::default()),
            organize: OrganizeManager::default(),
            dedupe: DedupeManager::default(),
            trash: Arc::new(OsTrash),
            file_bytes: Arc::new(DiskBytes),
            tool_search: None,
        }
    }

    /// Looks for external archive tools with `search` instead of this
    /// process's `PATH` (tests).
    pub fn with_tool_search(mut self, search: Search) -> AppState {
        self.tool_search = Some(search);
        self
    }

    /// Sends duplicates to `trash` (and undoes from it) instead of the OS
    /// Trash (tests).
    pub fn with_trash(mut self, trash: Arc<dyn TrashBackend>) -> AppState {
        self.trash = trash;
        self
    }

    /// Opens loose files for duplicates planning through `bytes` (tests
    /// count the reads).
    pub fn with_file_bytes(mut self, bytes: Arc<dyn FileBytes>) -> AppState {
        self.file_bytes = bytes;
        self
    }

    /// The external archive tools of this machine (each banner is read
    /// once per call: a tool installed meanwhile shows up).
    fn found_tools(&self) -> Vec<FoundTool> {
        match &self.tool_search {
            Some(search) => discover_with(search, &[]),
            None => discover(&[]),
        }
    }

    /// Uses `store` for source credentials (the OS keychain in the app).
    pub fn with_credentials(mut self, store: Arc<dyn CredentialStore>) -> AppState {
        self.credentials = store;
        self.forget_keys();
        self
    }

    /// The usable source keys (what a fill sends, and what makes a source
    /// ready): read from the store once, then remembered until a
    /// credential is saved or removed. A failing read is not remembered.
    pub fn source_keys(&self) -> SourceKeys {
        let generation = {
            let cache = lock(&self.keys);
            if let Some(k) = &cache.keys {
                return k.clone();
            }
            cache.generation
        };
        let (keys, complete) = SourceKeys::read_store(self.credentials.as_ref());
        let mut cache = lock(&self.keys);
        if complete && cache.generation == generation {
            cache.keys = Some(keys.clone());
        }
        keys
    }

    fn forget_keys(&self) {
        let mut cache = lock(&self.keys);
        cache.generation += 1;
        cache.keys = None;
    }

    /// Where source credentials are kept.
    pub fn credentials(&self) -> &dyn CredentialStore {
        self.credentials.as_ref()
    }

    /// The settings in use.
    pub fn settings(&self) -> Settings {
        lock(&self.prefs).settings.clone()
    }

    /// The settings with what loading them had to do, and the external
    /// archive tools found.
    pub fn settings_view(&self) -> SettingsView {
        let view = {
            let p = lock(&self.prefs);
            SettingsView::new(p.settings.clone(), &p.report)
        };
        view.with_tools(&self.found_tools())
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
        let view = SettingsView::new(p.settings.clone(), &p.report);
        drop(p);
        Ok(view.with_tools(&self.found_tools()))
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
            keys: self.source_keys(),
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
        // Fill jobs only: an organise job moves the ROMs themselves and is
        // undone from the organise screen, which checks later jobs.
        let job = list_jobs(&data_dir)?
            .into_iter()
            .find(|j| !j.undone && j.root == root && j.kind == JobKind::Fill)
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
            &cfg.settings.manual_options(),
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

/// Profiles, credentials and "Search by title".
impl AppState {
    /// Built-ins (read-only) first, then the custom profiles that load.
    pub fn list_profiles(&self) -> Vec<ProfileInfoView> {
        all_profiles(&self.custom_profiles())
            .iter()
            .map(|(p, builtin)| ProfileInfoView::of(p, *builtin))
            .collect()
    }

    /// Saves custom profile `p`, replacing custom profile `replaces` (a
    /// rename moves the default profile and the max size with it). Problems
    /// come back as field errors and nothing is saved then.
    pub fn save_profile(
        &self,
        p: ProfileFile,
        replaces: Option<String>,
    ) -> Result<ProfileSaveView, IpcError> {
        let store = self.custom_store();
        let customs = store.load().0;
        if let Some(old) = &replaces {
            if !customs.iter().any(|c| &c.id == old) {
                return Err(IpcError::BadRequest(format!("no custom profile `{old}`")));
            }
        }
        let builtin_ids: Vec<String> = builtin_profiles().into_iter().map(|b| b.id).collect();
        let mut errors = validate_profile(&p, &builtin_ids);
        let taken = customs.iter().any(|c| {
            c.id.eq_ignore_ascii_case(&p.id) && replaces.as_deref() != Some(c.id.as_str())
        });
        if taken && !errors.iter().any(|e| e.field == "id") {
            errors.push(FieldError::new("id", FieldErrorCode::Duplicate));
        }
        if !errors.is_empty() {
            return Ok(ProfileSaveView {
                profiles: self.list_profiles(),
                errors,
            });
        }
        store
            .save(&p, replaces.as_deref())
            .map_err(|e| IpcError::Settings(e.to_string()))?;
        log::info!("custom profile saved: {}", p.id);
        if let Some(old) = replaces.filter(|o| *o != p.id) {
            self.rename_profile_refs(&old, Some(&p.id))?;
        }
        Ok(ProfileSaveView {
            profiles: self.list_profiles(),
            errors: Vec::new(),
        })
    }

    /// Deletes custom profile `id`; the default profile falls back to
    /// detection and its max size goes with it. Built-ins are read-only.
    pub fn delete_profile(&self, id: &str) -> Result<Vec<ProfileInfoView>, IpcError> {
        if builtin_profiles()
            .iter()
            .any(|b| b.id.eq_ignore_ascii_case(id))
        {
            return Err(IpcError::BadRequest(
                "built-in profiles are read-only".into(),
            ));
        }
        let deleted = self
            .custom_store()
            .delete(id)
            .map_err(|_| IpcError::UnknownProfile(id.to_string()))?;
        if !deleted {
            return Err(IpcError::UnknownProfile(id.to_string()));
        }
        log::info!("custom profile deleted: {id}");
        self.rename_profile_refs(id, None)?;
        Ok(self.list_profiles())
    }

    /// The default profile and the per-profile max size follow a rename
    /// (or are dropped with a delete).
    fn rename_profile_refs(&self, old: &str, new: Option<&str>) -> Result<(), IpcError> {
        let s = self.settings();
        let mut patch = serde_json::Map::new();
        if s.library.default_profile.as_deref() == Some(old) {
            patch.insert(
                "library".into(),
                serde_json::json!({ "default_profile": new }),
            );
        }
        if let Some(size) = s.art.max_size.get(old) {
            // A merge patch merges maps: the old key is removed with an
            // explicit null, or it stays (and fails validation as an
            // unknown profile once the file is gone).
            let mut sizes = serde_json::Map::new();
            sizes.insert(old.to_string(), serde_json::Value::Null);
            if let Some(n) = new {
                sizes.insert(n.to_string(), serde_json::json!(size));
            }
            patch.insert("art".into(), serde_json::json!({ "max_size": sizes }));
        }
        if patch.is_empty() {
            return Ok(());
        }
        let out = self.update_settings(&serde_json::Value::Object(patch))?;
        match out.errors.first() {
            None => Ok(()),
            Some(e) => Err(IpcError::Settings(format!("{} is not valid", e.field))),
        }
    }

    /// Where `p` puts the art of `ex`, with its layout problems (the id of
    /// a profile being created may still be empty: id errors are left out).
    pub fn preview_profile(&self, p: &ProfileFile, ex: &PreviewExample) -> ProfilePreviewView {
        let builtin_ids: Vec<String> = builtin_profiles().into_iter().map(|b| b.id).collect();
        ProfilePreviewView {
            paths: preview_paths(p, ex, &SystemDb::builtin()),
            errors: validate_profile(p, &builtin_ids)
                .into_iter()
                .filter(|e| e.field != "id")
                .collect(),
        }
    }

    /// Whether the keychain works and which keys are set (never values).
    pub fn credential_status(&self) -> CredentialStatus {
        coverly_core::credentials::status(self.credentials.as_ref())
    }

    /// Stores a credential (an empty value clears it); only the status comes back.
    pub fn set_credential(&self, key: &str, value: &str) -> Result<CredentialStatus, IpcError> {
        let key = credential_key(key)?;
        let stored = store_secret(self.credentials.as_ref(), key, value);
        self.forget_keys();
        stored?;
        log::info!("credential set: {}", key.account());
        Ok(self.credential_status())
    }

    pub fn clear_credential(&self, key: &str) -> Result<CredentialStatus, IpcError> {
        let key = credential_key(key)?;
        if !self.credentials.available() {
            return Err(IpcError::KeychainUnavailable);
        }
        let cleared = self.credentials.clear(key);
        self.forget_keys();
        cleared?;
        log::info!("credential cleared: {}", key.account());
        Ok(self.credential_status())
    }

    /// Identifies entry `id`: a manual identity (kept in the app data dir,
    /// never the backup folder) first, else hash → DAT, else the name.
    pub fn identify_entry(
        &self,
        generation: u64,
        id: u32,
        systems: &SystemDb,
    ) -> Result<IdentityView, IpcError> {
        let session = self.session(generation)?;
        let cache = self.cache()?;
        let cfg = self.fill_config();
        let http = PoliteHttp::new(UreqHttp::new(&cfg.net), &cfg.net);
        let overrides = Overrides::read(&cfg.overrides_dir.join(OVERRIDES_FILE));
        crate::library::identify::identify_entry(
            &session,
            id,
            systems,
            &cache,
            &http,
            &cfg.endpoints.database,
            &overrides,
        )
    }

    /// "Search by title": candidates for a typed title (untrusted: empty,
    /// over-long or control-character queries are refused), best first.
    pub fn search_entry_art(
        &self,
        generation: u64,
        id: u32,
        media: Media,
        query: &str,
    ) -> Result<Vec<CandidateView>, IpcError> {
        let session = self.known_entry(generation, id)?;
        if session.profile.slot(media).is_none() {
            return Err(IpcError::BadRequest(
                "this layout cannot store that media".into(),
            ));
        }
        let cfg = self.fill_config();
        let db = SystemDb::builtin();
        let system = db
            .get(&session.entries[id as usize].system_id)
            .ok_or(IpcError::UnknownEntry(id))?;
        let settings = cfg.settings.fill_settings();
        let found = with_env(&cfg, |env| {
            Ok(env.search_title(system, media, query, &settings)?)
        })?;
        let views = found
            .iter()
            .enumerate()
            // Search results are not a pick: no reason.
            .map(|(i, c)| candidate_view(i, c, None))
            .collect();
        *lock(&self.searches) = Some(EntrySearch {
            generation,
            id,
            media,
            candidates: found,
        });
        Ok(views)
    }

    fn searched(
        &self,
        generation: u64,
        id: u32,
        media: Media,
        candidate: u32,
    ) -> Result<Candidate, IpcError> {
        match lock(&self.searches).as_ref() {
            Some(s) if s.generation == generation && s.id == id && s.media == media => s
                .candidates
                .get(candidate as usize)
                .cloned()
                .ok_or_else(|| IpcError::BadRequest(format!("no candidate {candidate}"))),
            _ => Err(IpcError::BadRequest("search first".into())),
        }
    }

    /// Found candidate `candidate`, re-encoded as it would be written.
    pub fn entry_search_art(
        &self,
        generation: u64,
        id: u32,
        media: Media,
        candidate: u32,
    ) -> Result<Vec<u8>, IpcError> {
        let session = self.session(generation)?;
        let c = self.searched(generation, id, media, candidate)?;
        with_env(&self.fill_config(), |env| {
            Ok(env.candidate_image(&session.profile, &c)?.bytes)
        })
    }

    /// Writes found candidate `candidate` to the ROM's own art target
    /// (never renamed; existing art backed up, undoable) and remembers the
    /// pick as the ROM's manual identity.
    pub fn set_entry_art_from_search(
        &self,
        generation: u64,
        id: u32,
        media: Media,
        candidate: u32,
    ) -> Result<Vec<EntryArtUpdate>, IpcError> {
        let (_writing, session, cfg) = self.writable_entry(generation, id)?;
        let c = self.searched(generation, id, media, candidate)?;
        let target = art_target(&session, id, media)?;
        let root = absolute_root(&session)?;
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
        let history = cfg.settings.manual_options();
        let report = with_env(&cfg, |env| {
            env.apply_candidate(&root, &session.profile, &rom, media, &target, &c, &history)
                .map_err(IpcError::from)
        })?;
        if let Some(ItemStatus::Failed { kind, message }) = report.items.first().map(|i| &i.status)
        {
            return Err(item_error(*kind, message));
        }
        if let Some(why) = report.paused {
            return Err(IpcError::Network(why));
        }
        remember_manual(&cfg.overrides_dir, &entry.rom.path, &c.name, &c.source)
            .map_err(|e| IpcError::Io(e.to_string()))?;
        self.refresh_entries(generation, &[id], &SystemDb::builtin())
    }

    /// "Forget manual match": entry `id` is identified as before again.
    pub fn clear_identity_override(&self, generation: u64, id: u32) -> Result<(), IpcError> {
        let session = self.known_entry(generation, id)?;
        forget_manual(
            &self.fill_config().overrides_dir,
            &session.entries[id as usize].rom.path,
        )
        .map(|_| ())
        .map_err(|e| IpcError::Io(e.to_string()))
    }
}

/// Only the closed set of credential names.
fn credential_key(name: &str) -> Result<CredentialKey, IpcError> {
    CredentialKey::parse(name)
        .ok_or_else(|| IpcError::BadRequest(format!("unknown credential `{name}`")))
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

/// The folder at `path`, or why it cannot be used.
fn existing_folder(path: &Path) -> Result<(), IpcError> {
    let shown = path.display().to_string();
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(IpcError::NotAFolder(shown)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(IpcError::NotFound(shown)),
        Err(_) => Err(IpcError::Unreadable(shown)),
    }
}

/// ScreenScraper is asked by CRC only when it is enabled, its account is
/// set and the plan is not offline (organise spec §6.5).
fn online_wanted(s: &Settings, keys: &SourceKeys, offline: bool) -> bool {
    !offline
        && keys.screenscraper_dev.is_some()
        && keys.screenscraper_user.is_some()
        && s.sources
            .order
            .iter()
            .any(|e| e.id == "screenscraper" && e.enabled)
}

/// Organising a loose pile (organise spec §4): the probe, plans kept by id.
impl AppState {
    /// The plans of the organise screen (apply and undo use them too).
    pub fn organize(&self) -> &OrganizeManager {
        &self.organize
    }

    /// A quick, name-only look at `path` (no file is read).
    pub fn probe_folder(&self, path: &Path) -> Result<FolderProbeView, IpcError> {
        existing_folder(path)?;
        probe_folder(path, &SystemDb::builtin(), &OsMetaProbe)
            .map(FolderProbeView::from)
            .map_err(|_| IpcError::Unreadable(path.display().to_string()))
    }

    /// The options of `req` over `settings.organize`, with absolute paths.
    fn organize_options(
        settings: &Settings,
        req: &OrganizeRequest,
    ) -> Result<(PathBuf, OrganizeOptions), IpcError> {
        if req.source.trim().is_empty() {
            return Err(IpcError::BadRequest("no folder to organise".into()));
        }
        let source =
            std::path::absolute(Path::new(&req.source)).map_err(|e| IpcError::Io(e.to_string()))?;
        let mut opts = OrganizeOptions::from_settings(settings, req.layout.as_deref());
        opts.target = req
            .target
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .map(|t| std::path::absolute(Path::new(t)).map_err(|e| IpcError::Io(e.to_string())))
            .transpose()?;
        if let Some(flat) = req.flat {
            opts.flat = flat;
        }
        if let Some(mode) = req.mode {
            opts.mode = Some(mode);
        }
        if let Some(names) = req.names {
            opts.names = names;
        }
        if let Some(duplicates) = req.duplicates {
            opts.duplicates = duplicates;
        }
        opts.offline = req.offline.unwrap_or(false);
        Ok((source, opts))
    }

    /// Plans organising `req.source` (blocking) with `decisions`. A survey
    /// kept for the same pile and options is reused (no file is read and
    /// no progress is sent); otherwise the pile is surveyed with throttled
    /// progress and can be cancelled with [`Self::organize_cancel`]. The
    /// plan is kept under a new id.
    pub fn organize_plan(
        &self,
        req: &OrganizeRequest,
        decisions: &[OrganizeDecision],
        events: Arc<dyn OrganizeEvents>,
    ) -> Result<OrganizePlanView, IpcError> {
        let cfg = self.fill_config();
        let (source, opts) = Self::organize_options(&cfg.settings, req)?;
        existing_folder(&source)?;
        if let Some(target) = &opts.target {
            existing_folder(target)?;
        }
        let profiles: Vec<Profile> = all_profiles(&self.custom_profiles())
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        let layout = profiles
            .iter()
            .find(|p| p.id == opts.layout)
            .cloned()
            .ok_or_else(|| IpcError::UnknownProfile(opts.layout.clone()))?;
        let db = SystemDb::builtin();
        fs::create_dir_all(&cfg.cache_dir).map_err(|e| IpcError::Io(e.to_string()))?;
        let cache = Cache::open(&cfg.cache_dir.join(CACHE_FILE))
            .map_err(|e| IpcError::Io(e.to_string()))?;
        let http = PoliteHttp::new(UreqHttp::new(&cfg.net), &cfg.net);
        let store = DatStore::new(&http, &cache, cfg.endpoints.database.clone());
        let screenscraper = online_wanted(&cfg.settings, &cfg.keys, opts.offline)
            .then(|| {
                let dev = cfg.keys.screenscraper_dev.clone()?;
                Some(ScreenScraperSource::new(
                    &http,
                    &cache,
                    dev,
                    cfg.keys.screenscraper_user.clone(),
                    SsConfig {
                        base: cfg.endpoints.screenscraper.clone(),
                        ..SsConfig::default()
                    },
                ))
            })
            .flatten();
        let lookup = screenscraper
            .as_ref()
            .map(|source| ScreenScraperLookup::new(source, &db));
        let env = OrganizeEnv {
            db: &db,
            profiles: &profiles,
            cache: &cache,
            dats: (!opts.offline && opts.download_dats).then_some(&store),
            probe: &OsMetaProbe,
            online: lookup.as_ref().map(|l| l as &dyn OnlineLookup),
            now: SystemTime::now(),
        };

        let plan_id = self.organize.next_id();
        let online = lookup.is_some();
        let kept = if req.rescan {
            None
        } else {
            self.organize.survey_for(&source, &opts, &layout, online)
        };
        let survey = match kept {
            Some(kept) => kept,
            None => {
                let cancel = self.organize.begin(plan_id);
                let throttle = Throttle::new(plan_id, events);
                let found = survey(&source, &opts, &env, &|p| throttle.progress(p), &cancel);
                self.organize.finish(plan_id);
                let found = found?;
                log::info!(
                    "organise plan {plan_id}: surveyed {} items",
                    found.entries.len()
                );
                Arc::new(found)
            }
        };
        let plan = plan_from_survey(&survey, &opts, decisions, &env)?;
        let stored = self.organize.store(
            plan_id,
            StoredPlan {
                // A kept plan never asks to read the folder again.
                request: OrganizeRequest {
                    rescan: false,
                    ..req.clone()
                },
                source,
                opts,
                layout,
                online,
                survey,
                plan,
            },
        );
        Ok(OrganizePlanView {
            plan_id,
            plan: stored.plan.clone(),
        })
    }

    /// Cancels plan `plan_id` while its survey runs; a no-op otherwise.
    pub fn organize_cancel(&self, plan_id: u64) -> Result<(), IpcError> {
        self.organize.cancel(plan_id);
        Ok(())
    }

    /// Plan `plan_id` again from its kept survey with `decisions` (no file
    /// is read): what apply runs, so the webview never sends a plan.
    fn decided_plan(
        &self,
        plan_id: u64,
        decisions: &[OrganizeDecision],
        cfg: &FillConfig,
    ) -> Result<OrganizePlan, IpcError> {
        let stored = self.organize.get(plan_id)?;
        let db = SystemDb::builtin();
        fs::create_dir_all(&cfg.cache_dir).map_err(|e| IpcError::Io(e.to_string()))?;
        let cache = Cache::open(&cfg.cache_dir.join(CACHE_FILE))
            .map_err(|e| IpcError::Io(e.to_string()))?;
        // The layout as surveyed, even if its custom profile changed since.
        let profiles = [stored.layout.clone()];
        let env = OrganizeEnv {
            db: &db,
            profiles: &profiles,
            cache: &cache,
            dats: None,
            probe: &OsMetaProbe,
            online: None,
            now: SystemTime::now(),
        };
        Ok(plan_from_survey(
            &stored.survey,
            &stored.opts,
            decisions,
            &env,
        )?)
    }

    /// Takes the manual-write guard for an organise write: refused while a
    /// fill gathers, applies or is paused (it resumes into its journal),
    /// or another write (or apply) runs.
    fn organize_guard(&self) -> Result<crate::jobs::OwnedManualWrite, IpcError> {
        if self.jobs.paused() {
            return Err(IpcError::JobRunning);
        }
        self.jobs.begin_manual_owned()
    }

    /// Applies plan `plan_id` with `decisions` on a worker thread (the
    /// backend re-plans from its own survey). Fills and other writes are
    /// refused until it ends; progress and the end arrive through
    /// `events`. The kept plans are dropped: the pile changes.
    pub fn organize_apply(
        &self,
        plan_id: u64,
        decisions: &[OrganizeDecision],
        events: Arc<dyn OrganizeEvents>,
    ) -> Result<OrganizeJobView, IpcError> {
        let cfg = self.fill_config();
        let plan = self.decided_plan(plan_id, decisions, &cfg)?;
        let external_tools = self.organize.get(plan_id)?.opts.limits.external_tools;
        check_organize_data_dir(&cfg, &[&plan.source_root, &plan.target_root])?;
        let guard = self.organize_guard()?;
        fs::create_dir_all(&cfg.data_dir).map_err(|e| IpcError::Io(e.to_string()))?;
        self.organize.forget_all();
        log::info!(
            "organise plan {plan_id}: applying {} items",
            plan.counts.roms
        );
        let start = ApplyStart {
            plan_id,
            job: None,
            target_root: plan.target_root.display().to_string(),
            layout: plan.layout.clone(),
            data_dir: cfg.data_dir.clone(),
            external_tools,
        };
        Ok(self
            .organize
            .spawn_apply(start, guard, events, move |env| apply_organize(&plan, env)))
    }

    /// Continues the paused or interrupted organise job `job` from the
    /// plan saved with it, into the same journal (like [`Self::organize_apply`]).
    pub fn organize_resume(
        &self,
        job: &str,
        events: Arc<dyn OrganizeEvents>,
    ) -> Result<OrganizeJobView, IpcError> {
        let cfg = self.fill_config();
        let plan = load_plan(&cfg.data_dir, job)?;
        let summary = list_jobs(&cfg.data_dir)?
            .into_iter()
            .find(|j| j.job == job)
            .ok_or(IpcError::NoPlan)?;
        if summary.undone || summary.complete {
            return Err(IpcError::BadRequest(format!(
                "job {job} has nothing left to resume"
            )));
        }
        check_organize_data_dir(&cfg, &[&plan.source_root, &plan.target_root])?;
        let guard = self.organize_guard()?;
        self.organize.forget_all();
        let start = ApplyStart {
            plan_id: self.organize.next_id(),
            job: Some(job.to_string()),
            target_root: plan.target_root.display().to_string(),
            layout: plan.layout.clone(),
            data_dir: cfg.data_dir.clone(),
            external_tools: OrganizeOptions::from_settings(&cfg.settings, None)
                .limits
                .external_tools,
        };
        let job = job.to_string();
        Ok(self
            .organize
            .spawn_apply(start, guard, events, move |env| resume_organize(&job, env)))
    }

    /// Stops the running apply before its next item (a no-op otherwise).
    pub fn organize_cancel_apply(&self) -> Result<(), IpcError> {
        self.organize.cancel_apply();
        Ok(())
    }

    /// The current (or last) organise apply of this app session.
    pub fn organize_job(&self) -> Option<OrganizeJobView> {
        self.organize.current_apply()
    }

    /// Undoes job `job`. Newer jobs that wrote under its folders (a fill
    /// that added covers to the organised tree) would be orphaned: refused
    /// with [`IpcError::LaterJobs`] (their ids as detail) unless
    /// `include_later`, which undoes them too, newest first.
    pub fn organize_undo(
        &self,
        job: &str,
        include_later: bool,
    ) -> Result<OrganizeUndoView, IpcError> {
        if self.jobs.paused() {
            return Err(IpcError::JobRunning);
        }
        let _writing = self.jobs.begin_manual()?;
        let data_dir = self.fill_config().data_dir.clone();
        if !list_jobs(&data_dir)?
            .iter()
            .any(|j| j.job == job && !j.undone)
        {
            return Err(IpcError::NothingToUndo);
        }
        let later = later_jobs(&data_dir, job)?;
        if !later.is_empty() && !include_later {
            let ids: Vec<&str> = later.iter().map(|j| j.job.as_str()).collect();
            return Err(IpcError::LaterJobs(ids.join(", ")));
        }
        let mut ids: Vec<String> = later.iter().map(|j| j.job.clone()).collect();
        ids.push(job.to_string());
        let report = undo_jobs_in_order_with(&data_dir, &ids, self.trash.as_ref())?;
        self.organize.forget_all();
        self.dedupe.forget_all();
        log::info!("undid organise job {job} ({} jobs)", report.len());
        Ok(OrganizeUndoView {
            report,
            later_jobs: later.into_iter().map(JobHistoryView::from).collect(),
        })
    }

    /// Writes plan `plan_id` (as planned, decisions included) to `dest` as JSON.
    pub fn organize_export_plan(&self, plan_id: u64, dest: &Path) -> Result<(), IpcError> {
        let stored = self.organize.get(plan_id)?;
        write_export(dest, |w| {
            serde_json::to_writer_pretty(w, &stored.plan).map_err(io::Error::other)
        })
    }
}

/// Duplicates (duplicates spec §11, §12): plans kept by id, picks, the
/// apply behind the manual-write guard. Undo is `organize_undo`.
impl AppState {
    /// The plans of the duplicates screen and its apply.
    pub fn dedupe(&self) -> &DedupeManager {
        &self.dedupe
    }

    /// The options of `req` over `settings.dedupe`.
    fn dedupe_options(settings: &Settings, req: &DedupeRequest) -> Result<DedupeOptions, IpcError> {
        let mut opts = DedupeOptions::from_settings(&settings.dedupe);
        Self::regroup_options(&mut opts, req)?;
        if let Some(systems) = &req.systems {
            opts.systems = Some(systems.clone());
        }
        opts.offline = req.offline.unwrap_or(false);
        opts.reset_choices = req.reset_choices;
        opts.forced_keepers = req
            .forced_keepers
            .iter()
            .filter(|p| !p.trim().is_empty())
            .map(|p| std::path::absolute(Path::new(p)).map_err(|e| IpcError::Io(e.to_string())))
            .collect::<Result<_, _>>()?;
        Ok(opts)
    }

    /// What a regroup may change: levels, 1G1R, rules and where the copies go.
    fn regroup_options(opts: &mut DedupeOptions, req: &DedupeRequest) -> Result<(), IpcError> {
        if let Some(levels) = &req.levels {
            opts.levels = levels.clone();
        }
        if let Some(on) = req.one_game_one_rom {
            opts.one_game_one_rom = on;
        }
        if let Some(rules) = &req.rules {
            opts.rules = rules.clone();
        }
        if let Some(q) = &req.quarantine {
            opts.quarantine = match q {
                QuarantineChoice::Folder { path } => QuarantineChoice::Folder {
                    path: std::path::absolute(path).map_err(|e| IpcError::Io(e.to_string()))?,
                },
                other => other.clone(),
            };
        }
        Ok(())
    }

    /// Plans the duplicates of the library at `root` (blocking, a dry run):
    /// throttled progress through `events`, cancelled by
    /// [`Self::dedupe_cancel`]. The plan is kept under a new id.
    pub fn dedupe_plan(
        &self,
        root: &Path,
        req: &DedupeRequest,
        events: Arc<dyn DedupeEvents>,
    ) -> Result<DedupePlanView, IpcError> {
        existing_folder(root)?;
        let root = std::path::absolute(root).map_err(|e| IpcError::Io(e.to_string()))?;
        let cfg = self.fill_config();
        let settings = &cfg.settings;
        let opts = Self::dedupe_options(settings, req)?;
        let db = SystemDb::builtin();
        // The open library's profile when it is this library (what its
        // screen shows), else picked as opening it would.
        let open = lock(&self.session).clone();
        let profile = match open {
            Some(s)
                if req.profile.is_none()
                    && std::path::absolute(&s.root).ok() == Some(root.clone()) =>
            {
                s.profile.clone()
            }
            _ => crate::library::library_profile(
                &root,
                req.profile.as_deref(),
                &self.custom_profiles(),
                settings,
                &db,
            )?,
        };
        let cache = self.cache()?;
        let http = PoliteHttp::new(UreqHttp::new(&cfg.net), &cfg.net);
        let store = DatStore::new(&http, &cache, cfg.endpoints.database.clone());
        // External tools as organise uses them (one setting), never found
        // inside the library (it holds the quarantine) or a custom
        // quarantine.
        let limits = Limits {
            external_tools: OrganizeOptions::from_settings(settings, None)
                .limits
                .external_tools,
            ..Limits::default()
        };
        let mut avoid: Vec<&Path> = vec![&root];
        if let QuarantineChoice::Folder { path } = &opts.quarantine {
            avoid.push(path);
        }
        let tools = Arc::new(match &self.tool_search {
            Some(search) => ToolsOnDemand::with_search(search.clone(), &avoid),
            None => ToolsOnDemand::new(&avoid),
        });
        let listing = ArchiveEntryListing::with_tools_on_demand(limits.clone(), Arc::clone(&tools));
        let discs = ReaderDiscProbe::new(ArchiveEntryListing::with_tools_on_demand(limits, tools));

        let plan_id = self.dedupe.next_id();
        let cancel = self.dedupe.begin(plan_id);
        let throttle = DedupeThrottle::new(plan_id, events);
        let progress = |p: coverly_core::dedupe::DedupeProgress| throttle.progress(&p);
        let scan = scan_library(&root, &profile, &db, &settings.scan_options());
        let env = DedupeEnv {
            cache: &cache,
            db: &db,
            profile: &profile,
            settings,
            listing: &listing,
            discs: &discs,
            dats: (!opts.offline).then_some(&store),
            bytes: self.file_bytes.as_ref(),
            progress: &progress,
            cancel: &cancel,
        };
        let planned = plan_dedupe(&scan, &opts, &env);
        self.dedupe.finish(plan_id);
        let plan = planned?;
        log::info!(
            "duplicates plan {plan_id}: {} groups, {} extras",
            plan.totals.groups,
            plan.totals.extras
        );
        let trash_available = self.trash.available(&root);
        Ok(self.dedupe.store(
            plan_id,
            StoredDedupe {
                root,
                opts,
                plan,
                skipped_groups: Default::default(),
                skipped_systems: Default::default(),
                trash_available,
            },
        ))
    }

    /// Cancels plan `plan_id` while it is being made; a no-op otherwise.
    pub fn dedupe_cancel(&self, plan_id: u64) -> Result<(), IpcError> {
        self.dedupe.cancel(plan_id);
        Ok(())
    }

    /// Plan `plan_id` again with other levels, rules or destination, from
    /// what was read (no ROM is read again). Same id; picks and skips stay.
    pub fn dedupe_regroup(
        &self,
        plan_id: u64,
        req: &DedupeRequest,
    ) -> Result<DedupePlanView, IpcError> {
        self.dedupe.edit(plan_id, |stored| {
            let mut opts = stored.opts.clone();
            Self::regroup_options(&mut opts, req)?;
            stored.plan = regroup(&stored.plan, &opts)?;
            stored.opts = opts;
            stored.reapply_skips(None);
            Ok(())
        })
    }

    /// Makes `member` the copy of `group` that stays, remembered by content
    /// for later runs.
    pub fn dedupe_set_keeper(
        &self,
        plan_id: u64,
        group: u64,
        member: usize,
    ) -> Result<DedupePlanView, IpcError> {
        let cache = self.cache()?;
        self.dedupe.edit(plan_id, |stored| {
            set_keeper(&mut stored.plan, group, member, &cache)?;
            stored.reapply_skips(None);
            Ok(())
        })
    }

    /// Skips group `group` (nothing moves) or takes it back.
    pub fn dedupe_skip_group(
        &self,
        plan_id: u64,
        group: u64,
        skip: bool,
    ) -> Result<DedupePlanView, IpcError> {
        self.dedupe
            .edit(plan_id, |stored| stored.skip_group(group, skip))
    }

    /// Skips every group of `system` or takes them back.
    pub fn dedupe_skip_system(
        &self,
        plan_id: u64,
        system: &str,
        skip: bool,
    ) -> Result<DedupePlanView, IpcError> {
        self.dedupe.edit(plan_id, |stored| {
            stored.skip_system(system, skip);
            Ok(())
        })
    }

    /// Forgets the keepers chosen for the library at `root` (`systems`, or
    /// every system): the rules decide again on the next plan. Returns how
    /// many picks were forgotten.
    pub fn dedupe_reset_choices(
        &self,
        root: &Path,
        systems: Option<&[String]>,
    ) -> Result<usize, IpcError> {
        existing_folder(root)?;
        let all: Vec<String>;
        let systems = match systems {
            Some(s) => s,
            None => {
                all = SystemDb::builtin()
                    .all()
                    .iter()
                    .map(|s| s.id.clone())
                    .collect();
                &all
            }
        };
        let cleared = self
            .cache()?
            .clear_dedupe_choices(systems)
            .map_err(|e| IpcError::Io(e.to_string()))?;
        log::info!("duplicates: {cleared} remembered picks forgotten");
        Ok(cleared)
    }

    /// Applies plan `plan_id` with its picks and skips on a worker thread:
    /// the extras go to the quarantine (or the Trash), journaled. Refused
    /// while a fill, an organise job or another write runs; progress and
    /// the end arrive through `events`. The kept plans are dropped: the
    /// library changes.
    pub fn dedupe_apply(
        &self,
        plan_id: u64,
        events: Arc<dyn DedupeEvents>,
    ) -> Result<DedupeJobView, IpcError> {
        let cfg = self.fill_config();
        let kept = self.dedupe.get(plan_id)?;
        let (plan, decisions) = {
            let stored = lock(&kept);
            (stored.plan.clone(), stored.decisions())
        };
        check_organize_data_dir(&cfg, &[&plan.root, &plan.quarantine])?;
        let guard = self.organize_guard()?;
        fs::create_dir_all(&cfg.data_dir).map_err(|e| IpcError::Io(e.to_string()))?;
        self.dedupe.forget_all();
        log::info!(
            "duplicates plan {plan_id}: setting aside {} copies",
            plan.totals.extras
        );
        let start = crate::dedupe::ApplyStart {
            plan_id,
            job: None,
            root: plan.root.display().to_string(),
            quarantine: plan.quarantine.display().to_string(),
            trash: plan.trash,
            data_dir: cfg.data_dir.clone(),
            cache_dir: cfg.cache_dir.clone(),
            backend: Arc::clone(&self.trash),
        };
        Ok(self.dedupe.spawn_apply(start, guard, events, move |env| {
            apply_dedupe(&plan, &decisions, env)
        }))
    }

    /// Continues the paused or interrupted duplicates job `job` from the
    /// plan saved with it, into the same journal.
    pub fn dedupe_resume(
        &self,
        job: &str,
        events: Arc<dyn DedupeEvents>,
    ) -> Result<DedupeJobView, IpcError> {
        let cfg = self.fill_config();
        let plan = load_dedupe_plan(&cfg.data_dir, job).map_err(|e| match e {
            coverly_core::organize::apply::ApplyError::NoPlan(_) => IpcError::NoDedupePlan,
            e => e.into(),
        })?;
        let summary = list_jobs(&cfg.data_dir)?
            .into_iter()
            .find(|j| j.job == job)
            .ok_or(IpcError::NoDedupePlan)?;
        if summary.undone || summary.complete {
            return Err(IpcError::BadRequest(format!(
                "job {job} has nothing left to resume"
            )));
        }
        check_organize_data_dir(&cfg, &[&plan.root, &plan.quarantine])?;
        let guard = self.organize_guard()?;
        self.dedupe.forget_all();
        let start = crate::dedupe::ApplyStart {
            plan_id: self.dedupe.next_id(),
            job: Some(job.to_string()),
            root: plan.root.display().to_string(),
            quarantine: plan.quarantine.display().to_string(),
            trash: plan.trash,
            data_dir: cfg.data_dir.clone(),
            cache_dir: cfg.cache_dir.clone(),
            backend: Arc::clone(&self.trash),
        };
        let job = job.to_string();
        Ok(self
            .dedupe
            .spawn_apply(start, guard, events, move |env| resume_dedupe(&job, env)))
    }

    /// Stops the running duplicates apply between groups (a no-op otherwise).
    pub fn dedupe_cancel_apply(&self) -> Result<(), IpcError> {
        self.dedupe.cancel_apply();
        Ok(())
    }

    /// The current (or last) duplicates apply of this app session.
    pub fn dedupe_job(&self) -> Option<DedupeJobView> {
        self.dedupe.current_apply()
    }

    /// Writes plan `plan_id` (picks and skips applied) to `dest` as JSON.
    pub fn dedupe_export_plan(&self, plan_id: u64, dest: &Path) -> Result<(), IpcError> {
        let kept = self.dedupe.get(plan_id)?;
        let plan = lock(&kept).plan.clone();
        write_export(dest, |w| {
            serde_json::to_writer_pretty(w, &plan).map_err(io::Error::other)
        })
    }

    /// The bytes of group `group`'s cover (`member: None`) or of one copy's
    /// own art in plan `plan_id` (dedupe UI spec D4, D16). Offline only: a
    /// recorded art file, read only when its canonical path stays inside
    /// the plan root's and it is at most [`MAX_ART_BYTES`], else an
    /// already downloaded libretro-thumbnails image of the catalogue name.
    /// Anything else, an unknown id included, is `ArtMissing`.
    pub fn dedupe_art_bytes(
        &self,
        plan_id: u64,
        group: u64,
        member: Option<usize>,
        media: Media,
    ) -> Result<Vec<u8>, IpcError> {
        let missing = || IpcError::ArtMissing(media_name(media));
        let source = match self.dedupe.art_source(plan_id, group, member, media) {
            Err(IpcError::NoDedupePlan) => return Err(missing()),
            other => other?,
        };
        let bytes = match source {
            DedupeArt::Library { root, path } => read_plan_art(&root, &path),
            DedupeArt::Libretro { system, name } => {
                self.libretro_cover(plan_id, group, media, &system, &name)
            }
        };
        bytes.ok_or_else(missing)
    }

    /// The libretro cover of `name` already in the art store, through the
    /// plan's memo; never reaches the network.
    fn libretro_cover(
        &self,
        plan_id: u64,
        group: u64,
        media: Media,
        system: &str,
        name: &str,
    ) -> Option<Vec<u8>> {
        let cache = match self.cache() {
            Ok(c) => c,
            Err(e) => {
                log::debug!("duplicates art: no cache: {e:?}");
                return None;
            }
        };
        let store = ArtStore::new(&self.base.cache_dir, &cache);
        let url = match self.dedupe.libretro_memo(plan_id, group, media) {
            Some(url) => url,
            None => {
                let cfg = LibretroConfig {
                    base: self.base.endpoints.libretro.clone(),
                    raw_base: self.base.endpoints.libretro_raw.clone(),
                    ttl_secs: INDEX_TTL_SECS,
                };
                let src = LibretroSource::new(&OfflineHttp, &cache, cfg);
                let db = SystemDb::builtin();
                let url = db
                    .get(system)
                    .and_then(|s| libretro_cached_url(&src, &store, s, media, name));
                self.dedupe
                    .remember_libretro(plan_id, group, media, url.clone());
                url
            }
        };
        url.and_then(|u| store.get(&u))
    }

    /// `path` when the file manager may show it: inside the open library,
    /// a kept duplicates plan's library or quarantine, or the last
    /// duplicates job's, a kept Organise plan's pile or target, or the
    /// current Organise apply's target; the nearest existing folder when
    /// it is gone. Matched by path component, never by text prefix.
    pub fn revealable(&self, path: &Path) -> Result<PathBuf, IpcError> {
        let shown = path.display().to_string();
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(IpcError::OutsideLibrary(shown));
        }
        let mut folders = self.dedupe.known_folders();
        folders.extend(self.organize.known_folders());
        if let Some(s) = lock(&self.session).as_ref() {
            folders.push(s.root.clone());
            folders.push(s.root_canon.clone());
        }
        if !folders
            .iter()
            .filter(|f| !f.as_os_str().is_empty())
            .any(|f| path.starts_with(f))
        {
            return Err(IpcError::OutsideLibrary(shown));
        }
        path.ancestors()
            .find(|p| p.exists())
            .map(Path::to_path_buf)
            .ok_or(IpcError::NotFound(shown))
    }
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

    use coverly_core::dedupe::{
        ArtSeen, DedupePlan, DupGroup, GroupAction, KeepReason, Member, MemberFacts, Relation,
        UnitKind, UnitRef,
    };
    use tempfile::TempDir;

    const TETRIS: &str = "Tetris (World) (Rev 1)";
    const TETRIS_URL: &str = "https://thumbnails.libretro.com/Nintendo%20-%20Game%20Boy/Named_Boxarts/Tetris%20(World)%20(Rev%201).png";

    /// An app state over temp config and cache folders.
    fn art_state(dirs: &TempDir) -> AppState {
        let base = FillConfig::new(dirs.path().join("cache"), dirs.path().join("data"));
        AppState::new(dirs.path().join("config"), base)
    }

    fn art_member(id: usize, primary: PathBuf, art: Option<PathBuf>) -> Member {
        Member {
            id,
            unit: UnitRef {
                files: vec![primary.clone()],
                primary,
                kind: UnitKind::Single,
                mtime_ns: 0,
                in_library: true,
                mtimes_ns: Vec::new(),
            },
            relation: Relation::Exact,
            size: 32,
            travels: Vec::new(),
            stays: Vec::new(),
            dest: None,
            catalogue_name: None,
            serials: Vec::new(),
            confidence: 1.0,
            gamelist: None,
            art_leaves: Default::default(),
            facts: MemberFacts {
                art: art
                    .into_iter()
                    .map(|path| ArtSeen {
                        media: Media::Cover,
                        path,
                        size: 4,
                    })
                    .collect(),
                ..MemberFacts::default()
            },
        }
    }

    /// Keeps a plan of group 7 (keeper 0 without art, member 1 with
    /// `art`) over `root`, and returns its id.
    fn keep_art_plan(st: &AppState, root: &Path, art: Option<PathBuf>) -> u64 {
        let gb = root.join("ROMS").join("GB");
        let mut keeper = art_member(0, gb.join("Tetris.gb"), None);
        keeper.catalogue_name = Some(TETRIS.into());
        let plan = DedupePlan {
            root: root.to_path_buf(),
            quarantine: root.join("_Duplicados"),
            groups: vec![DupGroup {
                id: 7,
                system: "gb".into(),
                title: "Tetris".into(),
                keeper: 0,
                reason: KeepReason::InLibrary,
                runner_up: None,
                members: vec![keeper, art_member(1, gb.join("Tetris (1).gb"), art)],
                action: GroupAction::Act,
                tie: false,
                choice_dropped: false,
            }],
            ..DedupePlan::default()
        };
        let id = st.dedupe.next_id();
        st.dedupe.store(
            id,
            StoredDedupe {
                root: root.to_path_buf(),
                opts: DedupeOptions::default(),
                plan,
                skipped_groups: Default::default(),
                skipped_systems: Default::default(),
                trash_available: false,
            },
        );
        id
    }

    fn library_with_art(bytes: &[u8]) -> (TempDir, PathBuf) {
        let lib = tempfile::tempdir().unwrap();
        let imgs = lib.path().join("ROMS").join("GB").join("Imgs");
        fs::create_dir_all(&imgs).unwrap();
        let art = imgs.join("a.png");
        fs::write(&art, bytes).unwrap();
        (lib, art)
    }

    fn is_art_missing(r: Result<Vec<u8>, IpcError>) -> bool {
        matches!(r, Err(IpcError::ArtMissing(_)))
    }

    #[test]
    fn dedupe_art_returns_member_art_bytes() {
        let dirs = tempfile::tempdir().unwrap();
        let st = art_state(&dirs);
        let (lib, art) = library_with_art(b"PNG1");
        let id = keep_art_plan(&st, lib.path(), Some(art));
        assert_eq!(
            st.dedupe_art_bytes(id, 7, Some(1), Media::Cover).unwrap(),
            b"PNG1"
        );
        assert_eq!(
            st.dedupe_art_bytes(id, 7, None, Media::Cover).unwrap(),
            b"PNG1",
            "the group cover falls back to the first member with art"
        );
    }

    #[test]
    fn dedupe_art_missing_cases_are_art_missing() {
        let dirs = tempfile::tempdir().unwrap();
        let st = art_state(&dirs);
        let (lib, art) = library_with_art(b"PNG1");
        let id = keep_art_plan(&st, lib.path(), Some(art.clone()));
        assert!(
            is_art_missing(st.dedupe_art_bytes(id + 9, 7, None, Media::Cover)),
            "unknown plan"
        );
        assert!(
            is_art_missing(st.dedupe_art_bytes(id, 8, None, Media::Cover)),
            "unknown group"
        );
        assert!(
            is_art_missing(st.dedupe_art_bytes(id, 7, Some(5), Media::Cover)),
            "unknown member"
        );
        assert!(
            is_art_missing(st.dedupe_art_bytes(id, 7, Some(0), Media::Cover)),
            "no art"
        );
        assert!(
            is_art_missing(st.dedupe_art_bytes(id, 7, Some(1), Media::Title)),
            "other media"
        );

        File::create(&art)
            .unwrap()
            .set_len(MAX_ART_BYTES + 1)
            .unwrap();
        assert!(
            is_art_missing(st.dedupe_art_bytes(id, 7, Some(1), Media::Cover)),
            "too large"
        );

        fs::remove_file(&art).unwrap();
        assert!(
            is_art_missing(st.dedupe_art_bytes(id, 7, Some(1), Media::Cover)),
            "deleted"
        );
    }

    #[test]
    fn dedupe_art_refuses_art_recorded_outside_the_root() {
        let dirs = tempfile::tempdir().unwrap();
        let st = art_state(&dirs);
        let (lib, _) = library_with_art(b"PNG1");
        let (other, outside) = library_with_art(b"SECRET");
        let id = keep_art_plan(&st, lib.path(), Some(outside));
        assert!(is_art_missing(st.dedupe_art_bytes(
            id,
            7,
            Some(1),
            Media::Cover
        )));
        drop(other);
    }

    #[test]
    fn dedupe_art_libretro_fallback_reads_the_store() {
        let dirs = tempfile::tempdir().unwrap();
        let st = art_state(&dirs);
        {
            let cache = st.cache().unwrap();
            cache
                .put_json(
                    "libretro-root-2",
                    "root",
                    &vec!["Nintendo - Game Boy".to_string()],
                    None,
                )
                .unwrap();
            cache
                .put_json(
                    "libretro-index-2",
                    "Nintendo - Game Boy/Named_Boxarts",
                    &vec![format!("{TETRIS}.png")],
                    None,
                )
                .unwrap();
            coverly_core::jobs::ArtStore::new(&dirs.path().join("cache"), &cache)
                .put(TETRIS_URL, b"PNGL")
                .unwrap();
        }
        let lib = tempfile::tempdir().unwrap();
        let id = keep_art_plan(&st, lib.path(), None);
        assert_eq!(st.dedupe.libretro_memo(id, 7, Media::Cover), None);
        assert_eq!(
            st.dedupe_art_bytes(id, 7, None, Media::Cover).unwrap(),
            b"PNGL"
        );
        assert_eq!(
            st.dedupe.libretro_memo(id, 7, Media::Cover),
            Some(Some(TETRIS_URL.to_string()))
        );
        assert_eq!(
            st.dedupe_art_bytes(id, 7, None, Media::Cover).unwrap(),
            b"PNGL",
            "the memo serves the second call"
        );
        // A copy's own art never falls back to libretro.
        assert!(is_art_missing(st.dedupe_art_bytes(
            id,
            7,
            Some(0),
            Media::Cover
        )));
        // Nothing downloaded for that media: remembered as none.
        assert!(is_art_missing(st.dedupe_art_bytes(
            id,
            7,
            None,
            Media::Title
        )));
        assert_eq!(st.dedupe.libretro_memo(id, 7, Media::Title), Some(None));
        st.dedupe.forget_all();
        assert_eq!(st.dedupe.libretro_memo(id, 7, Media::Cover), None);
    }

    /// Keeps an Organise plan of the pile `source` into `target` and
    /// returns its id.
    fn keep_organize_plan(st: &AppState, source: &Path, target: &Path) -> u64 {
        use coverly_core::organize::{Counts as OrganizeCounts, NameMode, Survey, TransferMode};
        let plan = OrganizePlan {
            source_root: source.to_path_buf(),
            target_root: target.to_path_buf(),
            roms_dir: target.join("ROMS"),
            layout: "anbernic-stock".into(),
            mode: TransferMode::Move,
            names: NameMode::Keep,
            items: Vec::new(),
            counts: OrganizeCounts::default(),
            sorted_folders: Vec::new(),
            deep_folders: Vec::new(),
            other_files: Vec::new(),
            warnings: Vec::new(),
            broad_root: false,
            fat32: false,
            move_allowed: true,
            target_detected: None,
            created_at: String::new(),
            duplicates: Vec::new(),
            set_aside_trash: false,
        };
        let id = st.organize.next_id();
        st.organize.store(
            id,
            StoredPlan {
                request: OrganizeRequest::default(),
                source: source.to_path_buf(),
                opts: OrganizeOptions::default(),
                layout: builtin_profiles().remove(0),
                online: false,
                survey: Arc::new(Survey {
                    source_root: source.to_path_buf(),
                    target_root: target.to_path_buf(),
                    broad_root: false,
                    entries: Vec::new(),
                    walked: Default::default(),
                }),
                plan,
            },
        );
        id
    }

    fn is_outside(r: Result<PathBuf, IpcError>) -> bool {
        matches!(r, Err(IpcError::OutsideLibrary(_)))
    }

    #[test]
    fn revealable_accepts_an_organise_pile_path_while_kept() {
        let dirs = tempfile::tempdir().unwrap();
        let st = art_state(&dirs);
        let tmp = tempfile::tempdir().unwrap();
        let pile = tmp.path().join("Downloads");
        let target = tmp.path().join("SD");
        fs::create_dir_all(&pile).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(pile.join("a.gb"), b"GB").unwrap();
        assert!(is_outside(st.revealable(&pile.join("a.gb"))), "no plan yet");

        keep_organize_plan(&st, &pile, &target);
        assert_eq!(
            st.revealable(&pile.join("a.gb")).unwrap(),
            pile.join("a.gb")
        );
        assert_eq!(
            st.revealable(&target.join("ROMS").join("GB").join("a.gb"))
                .unwrap(),
            target,
            "a target file not there yet shows its nearest folder"
        );

        st.organize.forget_all();
        assert!(is_outside(st.revealable(&pile.join("a.gb"))), "forgotten");

        keep_organize_plan(&st, &pile, &target);
        for _ in 0..crate::organize::KEPT_PLANS {
            let other = tmp.path().join("Other");
            fs::create_dir_all(&other).unwrap();
            keep_organize_plan(&st, &other, &other);
        }
        assert!(is_outside(st.revealable(&pile.join("a.gb"))), "evicted");
    }

    #[test]
    fn revealable_refuses_a_sibling_prefix() {
        let dirs = tempfile::tempdir().unwrap();
        let st = art_state(&dirs);
        let tmp = tempfile::tempdir().unwrap();
        let sd = tmp.path().join("SD");
        let sd2 = tmp.path().join("SD2");
        fs::create_dir_all(&sd).unwrap();
        fs::create_dir_all(&sd2).unwrap();
        fs::write(sd2.join("x.gb"), b"GB").unwrap();
        keep_organize_plan(&st, &sd, &sd);
        keep_art_plan(&st, &sd, None);
        assert!(st.revealable(&sd).is_ok());
        assert!(is_outside(st.revealable(&sd2.join("x.gb"))));
        assert!(is_outside(st.revealable(&sd2)));
    }

    #[test]
    fn revealable_refuses_parent_components_and_relative_paths() {
        let dirs = tempfile::tempdir().unwrap();
        let st = art_state(&dirs);
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("SD");
        fs::create_dir_all(root.join("ROMS")).unwrap();
        keep_organize_plan(&st, &root, &root);
        keep_art_plan(&st, &root, None);
        let escape = root.join("ROMS").join("..").join("..").join("etc");
        assert!(is_outside(st.revealable(&escape)));
        let relative = PathBuf::from_iter(["ROMS", "x.gb"]);
        assert!(is_outside(st.revealable(&relative)));
    }

    #[cfg(windows)]
    mod windows_tests {
        use super::*;

        #[test]
        fn revealable_matches_by_component() {
            let dirs = tempfile::tempdir().unwrap();
            let st = art_state(&dirs);
            let tmp = tempfile::tempdir().unwrap();
            let root = PathBuf::from_iter([tmp.path(), Path::new("Roms"), Path::new("SD")]);
            let gb = root.join("GB");
            fs::create_dir_all(&gb).unwrap();
            fs::write(gb.join("x.gb"), b"GB").unwrap();
            keep_organize_plan(&st, &root, &root);
            assert_eq!(st.revealable(&gb.join("x.gb")).unwrap(), gb.join("x.gb"));
            let sibling = PathBuf::from_iter([tmp.path(), Path::new("Roms"), Path::new("SD2")]);
            fs::create_dir_all(&sibling).unwrap();
            assert!(is_outside(st.revealable(&sibling)));
        }
    }

    #[cfg(unix)]
    mod unix_tests {
        use super::*;

        #[test]
        fn dedupe_art_refuses_a_symlink_out_of_the_root() {
            let dirs = tempfile::tempdir().unwrap();
            let st = art_state(&dirs);
            let (lib, art) = library_with_art(b"PNG1");
            let other = tempfile::tempdir().unwrap();
            let secret = other.path().join("secret.png");
            fs::write(&secret, b"SECRET").unwrap();
            fs::remove_file(&art).unwrap();
            std::os::unix::fs::symlink(&secret, &art).unwrap();
            let id = keep_art_plan(&st, lib.path(), Some(art));
            assert!(is_art_missing(st.dedupe_art_bytes(
                id,
                7,
                Some(1),
                Media::Cover
            )));
            assert!(is_art_missing(st.dedupe_art_bytes(
                id,
                7,
                None,
                Media::Cover
            )));
        }
    }
}
