//! IPC commands exposed to the webview. Each is a thin wrapper over
//! `state`/`jobs`/`library`; blocking work runs on the blocking thread pool;
//! fill jobs report through Tauri events.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use coverly_core::credentials::CredentialStatus;
use coverly_core::profiles::custom::{PreviewExample, ProfileFile};
use coverly_core::profiles::Media;
use coverly_core::sources::KeyCheck;
use coverly_core::systems::SystemDb;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::dedupe::view::{
    DedupeJobView, DedupePlanView, DedupeProgressEvent, DedupeRequest, DEDUPE_CHANGED_EVENT,
    DEDUPE_PROGRESS_EVENT,
};
use crate::dedupe::DedupeEvents;
use crate::error::IpcError;
use crate::jobs::view::{
    CandidateView, DecisionIn, EntriesUpdatedEvent, ExportFormat, FillRequest, ItemFilter,
    JobHistoryView, JobItemView, JobProgressEvent, JobView, UndoView,
};
use crate::jobs::JobEvents;
use crate::library::art::{entry_path, read_art_bytes};
use crate::library::refresh::EntryArtUpdate;
use crate::library::view::{LibrarySettingsView, LibraryView, RecentView};
use crate::organize::view::{
    FolderProbeView, OrganizeJobView, OrganizePlanView, OrganizeProgressEvent, OrganizeRequest,
    OrganizeUndoView, ORGANIZE_CHANGED_EVENT, ORGANIZE_PROGRESS_EVENT,
};
use crate::organize::OrganizeEvents;
use crate::prefs::{link_url, LinkId, SettingsUpdateView, SettingsView};
use crate::profiles::{ProfileInfoView, ProfilePreviewView, ProfileSaveView};
use crate::sources::SourceInfoView;
use crate::state::AppState;
use crate::update::{
    job_blocks_update, CheckTrigger, UpdateCheckView, UpdateInstallView, UpdateManager,
    UpdateProgressEvent,
};

/// Product name shown in the UI and reported over IPC.
pub const APP_NAME: &str = "Coverly ROMs";
/// Event carrying `ScanProgressEvent` while a library opens.
pub const SCAN_PROGRESS_EVENT: &str = "library-scan-progress";
/// Event carrying `JobProgressEvent` (throttled).
pub const JOB_PROGRESS_EVENT: &str = "fill-job-progress";
/// Event carrying the `JobView` after every state change.
pub const JOB_CHANGED_EVENT: &str = "fill-job-changed";
/// Event carrying `EntriesUpdatedEvent` after art was written.
pub const ENTRIES_UPDATED_EVENT: &str = "library-entries-updated";
/// Event carrying the saved `Settings` after an update or a reset.
pub const SETTINGS_CHANGED_EVENT: &str = "settings-changed";
/// Event carrying `UpdateProgressEvent` while an update downloads (throttled).
pub const UPDATE_PROGRESS_EVENT: &str = "update-progress";

/// Name and versions of the app and of the core library it is built on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub core_version: String,
}

/// Who we are and which versions are running.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: APP_NAME.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        core_version: coverly_core::VERSION.to_owned(),
    }
}

/// One system Coverly knows, for the organise screen's system pickers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemInfo {
    pub id: String,
    pub name: String,
}

/// The built-in systems, by display name (case-insensitive).
#[tauri::command]
pub fn list_systems() -> Vec<SystemInfo> {
    let mut systems: Vec<SystemInfo> = SystemDb::builtin()
        .all()
        .iter()
        .map(|s| SystemInfo {
            id: s.id.clone(),
            name: s.name.clone(),
        })
        .collect();
    systems.sort_by_cached_key(|s| (s.name.to_lowercase(), s.id.clone()));
    systems
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, IpcError> + Send + 'static,
) -> Result<T, IpcError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| IpcError::Io(e.to_string()))?
}

/// Opens a library folder, emitting `library-scan-progress` events.
#[tauri::command]
pub async fn open_library(
    app: AppHandle,
    path: String,
    profile_id: Option<String>,
) -> Result<LibraryView, IpcError> {
    blocking(move || {
        let state = app.state::<AppState>();
        state.open(
            Path::new(&path),
            profile_id.as_deref(),
            &SystemDb::builtin(),
            &mut |event| {
                let _ = app.emit(SCAN_PROGRESS_EVENT, event);
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn recent_libraries(app: AppHandle) -> Result<Vec<RecentView>, IpcError> {
    blocking(move || Ok(app.state::<AppState>().recent())).await
}

/// The drives and SD cards connected now (only lists folders, never writes).
#[tauri::command]
pub async fn mounted_volumes() -> Result<Vec<coverly_core::volumes::Volume>, IpcError> {
    blocking(|| Ok(coverly_core::volumes::mounted_volumes())).await
}

#[tauri::command]
pub fn forget_recent_library(
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<RecentView>, IpcError> {
    state.forget(Path::new(&path))
}

#[tauri::command]
pub fn library_settings(state: State<'_, AppState>) -> LibrarySettingsView {
    state.library_settings()
}

#[tauri::command]
pub fn set_library_settings(
    state: State<'_, AppState>,
    ignored_dirs: Vec<String>,
) -> Result<LibrarySettingsView, IpcError> {
    state.set_ignored_dirs(ignored_dirs)
}

/// The raw bytes of one art image (an `ArrayBuffer` in JS).
#[tauri::command]
pub async fn read_art(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
) -> Result<tauri::ipc::Response, IpcError> {
    blocking(move || {
        let session = app.state::<AppState>().session(generation)?;
        read_art_bytes(&session, entry_id, media).map(tauri::ipc::Response::new)
    })
    .await
}

/// Identifies one entry (manual match, else hash → DAT, else name); never
/// fails for lack of network.
#[tauri::command]
pub async fn identify_entry(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
) -> Result<crate::library::identify::IdentityView, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .identify_entry(generation, entry_id, &SystemDb::builtin())
    })
    .await
}

/// Shows the entry's file in Finder / Explorer / the file manager.
#[tauri::command]
pub fn reveal_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    generation: u64,
    entry_id: u32,
) -> Result<(), IpcError> {
    let session = state.session(generation)?;
    let path = entry_path(&session, entry_id)?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| IpcError::Io(e.to_string()))
}

/// Job events as Tauri events; written art refreshes the open library.
struct TauriEvents(AppHandle);

impl JobEvents for TauriEvents {
    fn progress(&self, event: JobProgressEvent) {
        let _ = self.0.emit(JOB_PROGRESS_EVENT, event);
    }
    fn changed(&self, view: JobView) {
        let _ = self.0.emit(JOB_CHANGED_EVENT, view);
    }
    fn written(&self, generation: u64, ids: Vec<u32>) {
        let state = self.0.state::<AppState>();
        if let Ok(entries) = state.refresh_entries(generation, &ids, &SystemDb::builtin()) {
            let _ = self.0.emit(
                ENTRIES_UPDATED_EVENT,
                EntriesUpdatedEvent {
                    generation,
                    entries,
                },
            );
        }
    }
}

fn job_events(app: &AppHandle) -> Arc<dyn JobEvents> {
    Arc::new(TauriEvents(app.clone()))
}

/// Starts the dry run of a fill (progress and the result arrive as events).
#[tauri::command]
pub async fn start_fill(
    app: AppHandle,
    generation: u64,
    request: FillRequest,
) -> Result<JobView, IpcError> {
    let events = job_events(&app);
    blocking(move || {
        app.state::<AppState>()
            .start_fill(generation, &request, events)
    })
    .await
}

/// The current (or last) fill job of this app session, if any.
#[tauri::command]
pub fn fill_job(state: State<'_, AppState>) -> Option<JobView> {
    state.jobs().current()
}

/// The plan items of `job` that match `filter`.
#[tauri::command]
pub async fn fill_job_items(
    app: AppHandle,
    job: u64,
    filter: ItemFilter,
) -> Result<Vec<JobItemView>, IpcError> {
    blocking(move || app.state::<AppState>().jobs().items(job, filter)).await
}

/// Records review decisions of a planned job.
#[tauri::command]
pub fn decide_fill(
    state: State<'_, AppState>,
    job: u64,
    decisions: Vec<DecisionIn>,
) -> Result<JobView, IpcError> {
    state.jobs().decide(job, &decisions)
}

/// Applies a planned job (progress and the result arrive as events).
#[tauri::command]
pub fn run_fill(app: AppHandle, state: State<'_, AppState>, job: u64) -> Result<JobView, IpcError> {
    state.jobs().run(job, job_events(&app))
}

/// Continues a paused job into the same journal.
#[tauri::command]
pub fn resume_fill(
    app: AppHandle,
    state: State<'_, AppState>,
    job: u64,
) -> Result<JobView, IpcError> {
    state.jobs().resume(job, job_events(&app))
}

/// Stops a running job or ends a planned or paused one.
#[tauri::command]
pub fn cancel_fill(state: State<'_, AppState>, job: u64) -> Result<JobView, IpcError> {
    state.jobs().cancel(job)
}

/// A candidate image of a plan item, re-encoded as it would be written.
#[tauri::command]
pub async fn candidate_art(
    app: AppHandle,
    job: u64,
    index: u32,
    candidate: u32,
) -> Result<tauri::ipc::Response, IpcError> {
    blocking(move || {
        let state = app.state::<AppState>();
        state
            .jobs()
            .candidate_image(&state.fill_config(), job, index, candidate)
            .map(tauri::ipc::Response::new)
    })
    .await
}

/// Undoes the newest not-undone journal of the open library.
#[tauri::command]
pub async fn undo_last_job(app: AppHandle, generation: u64) -> Result<UndoView, IpcError> {
    blocking(move || app.state::<AppState>().undo_last_job(generation)).await
}

/// Every journal kept, newest first.
#[tauri::command]
pub async fn job_history(app: AppHandle) -> Result<Vec<JobHistoryView>, IpcError> {
    blocking(move || app.state::<AppState>().job_history()).await
}

const IMAGE_FILTER: &str = "PNG / JPEG / WebP / GIF / BMP";
const IMAGE_EXTS: [&str; 6] = ["png", "jpg", "jpeg", "webp", "gif", "bmp"];

fn ext(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Csv => "csv",
        ExportFormat::Json => "json",
    }
}

/// Asks where to save `default_name`; `None` when the user cancels.
fn save_path(
    app: &AppHandle,
    default_name: &str,
    format: ExportFormat,
) -> Result<Option<PathBuf>, IpcError> {
    let Some(file) = app
        .dialog()
        .file()
        .set_file_name(default_name)
        .add_filter(ext(format).to_uppercase(), &[ext(format)])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    file.into_path()
        .map(Some)
        .map_err(|e| IpcError::Io(e.to_string()))
}

fn shown_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn entry_candidates(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
) -> Result<Vec<CandidateView>, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .entry_candidates(generation, entry_id, media)
    })
    .await
}

#[tauri::command]
pub async fn entry_candidate_art(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
    candidate: u32,
) -> Result<tauri::ipc::Response, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .entry_candidate_art(generation, entry_id, media, candidate)
            .map(tauri::ipc::Response::new)
    })
    .await
}

#[tauri::command]
pub async fn set_entry_art(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
    candidate: u32,
) -> Result<Vec<EntryArtUpdate>, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .set_entry_art(generation, entry_id, media, candidate)
    })
    .await
}

/// Opens the native image picker; `None` when the user cancels.
#[tauri::command]
pub async fn set_entry_art_from_file(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
) -> Result<Option<Vec<EntryArtUpdate>>, IpcError> {
    blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter(IMAGE_FILTER, &IMAGE_EXTS)
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| IpcError::Io(e.to_string()))?;
        app.state::<AppState>()
            .set_entry_art_from_file(generation, entry_id, media, &path)
            .map(Some)
    })
    .await
}

#[tauri::command]
pub async fn remove_entry_art(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
) -> Result<Vec<EntryArtUpdate>, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .remove_entry_art(generation, entry_id, media)
    })
    .await
}

/// Saves the audit through the native save dialog; returns the file name.
#[tauri::command]
pub async fn export_audit(
    app: AppHandle,
    generation: u64,
    format: ExportFormat,
) -> Result<Option<String>, IpcError> {
    blocking(move || {
        let name = format!("coverly-audit.{}", ext(format));
        let Some(path) = save_path(&app, &name, format)? else {
            return Ok(None);
        };
        app.state::<AppState>()
            .export_audit(generation, format, &path)?;
        Ok(Some(shown_name(&path)))
    })
    .await
}

#[tauri::command]
pub async fn export_job_report(
    app: AppHandle,
    job: u64,
    format: ExportFormat,
) -> Result<Option<String>, IpcError> {
    blocking(move || {
        let name = format!("coverly-job-{job}.{}", ext(format));
        let Some(path) = save_path(&app, &name, format)? else {
            return Ok(None);
        };
        app.state::<AppState>()
            .export_job_report(job, format, &path)?;
        Ok(Some(shown_name(&path)))
    })
    .await
}

/// The settings with what loading them had to do (a one-time notice) and
/// the external archive tools found. Off the main thread: finding a tool
/// runs its version banner.
#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<SettingsView, IpcError> {
    blocking(move || Ok(app.state::<AppState>().settings_view())).await
}

/// Validates and saves a JSON merge patch of the settings; field problems
/// come back as data. Emits `settings-changed` and applies the log level
/// when something was saved.
#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    patch: serde_json::Value,
) -> Result<SettingsUpdateView, IpcError> {
    blocking(move || {
        let state = app.state::<AppState>();
        let out = state.update_settings(&patch)?;
        if out.errors.is_empty() {
            crate::logging::set_level(out.settings.advanced.log_level);
            let _ = app.emit(SETTINGS_CHANGED_EVENT, &out.settings);
        }
        Ok(out)
    })
    .await
}

/// Defaults again (recent libraries kept, a backup copy first).
#[tauri::command]
pub async fn reset_settings(app: AppHandle) -> Result<SettingsView, IpcError> {
    blocking(move || {
        let view = app.state::<AppState>().reset_settings()?;
        crate::logging::set_level(view.settings.advanced.log_level);
        let _ = app.emit(SETTINGS_CHANGED_EVENT, &view.settings);
        Ok(view)
    })
    .await
}

fn open_dir(app: &AppHandle, dir: &Path) -> Result<(), IpcError> {
    std::fs::create_dir_all(dir).map_err(|e| IpcError::Io(e.to_string()))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| IpcError::Io(e.to_string()))
}

/// Shows the settings folder (chosen here, never by the webview).
#[tauri::command]
pub fn open_config_folder(app: AppHandle, state: State<'_, AppState>) -> Result<(), IpcError> {
    open_dir(&app, state.config_dir())
}

/// Shows the logs folder.
#[tauri::command]
pub fn open_logs_folder(app: AppHandle, state: State<'_, AppState>) -> Result<(), IpcError> {
    open_dir(&app, &state.logs_dir())
}

/// Opens one of the fixed project links in the browser.
#[tauri::command]
pub fn open_link(app: AppHandle, link: LinkId) -> Result<(), IpcError> {
    app.opener()
        .open_url(link_url(link), None::<&str>)
        .map_err(|e| IpcError::Io(e.to_string()))
}

/// Bytes used by the cache.
#[tauri::command]
pub async fn cache_usage(app: AppHandle) -> Result<u64, IpcError> {
    blocking(move || Ok(app.state::<AppState>().cache_usage())).await
}

/// Clears the cache (refused while a job runs); returns the bytes freed.
#[tauri::command]
pub async fn clear_cache(app: AppHandle) -> Result<u64, IpcError> {
    blocking(move || app.state::<AppState>().clear_cache()).await
}

// Profiles, credentials and "Search by title". Keychain calls block and may
// raise an OS prompt: they run on the blocking pool, never the main thread.

/// Built-in (read-only) and custom layout profiles.
#[tauri::command]
pub async fn list_profiles(app: AppHandle) -> Result<Vec<ProfileInfoView>, IpcError> {
    blocking(move || Ok(app.state::<AppState>().list_profiles())).await
}

/// Saves a custom profile (`replaces` = the id it had); field problems come back as data.
#[tauri::command]
pub async fn save_profile(
    app: AppHandle,
    profile: ProfileFile,
    replaces: Option<String>,
) -> Result<ProfileSaveView, IpcError> {
    blocking(move || app.state::<AppState>().save_profile(profile, replaces)).await
}

#[tauri::command]
pub async fn delete_profile(app: AppHandle, id: String) -> Result<Vec<ProfileInfoView>, IpcError> {
    blocking(move || app.state::<AppState>().delete_profile(&id)).await
}

/// Where a profile being edited puts the art of an example ROM.
#[tauri::command]
pub fn preview_profile(
    state: State<'_, AppState>,
    profile: ProfileFile,
    example: PreviewExample,
) -> ProfilePreviewView {
    state.preview_profile(&profile, &example)
}

/// Whether the keychain works and which keys are set (never the values).
#[tauri::command]
pub async fn credential_status(app: AppHandle) -> Result<CredentialStatus, IpcError> {
    blocking(move || Ok(app.state::<AppState>().credential_status())).await
}

/// Stores a credential in the keychain; the value never comes back.
#[tauri::command]
pub async fn set_credential(
    app: AppHandle,
    key: String,
    value: String,
) -> Result<CredentialStatus, IpcError> {
    blocking(move || app.state::<AppState>().set_credential(&key, &value)).await
}

#[tauri::command]
pub async fn clear_credential(app: AppHandle, key: String) -> Result<CredentialStatus, IpcError> {
    blocking(move || app.state::<AppState>().clear_credential(&key)).await
}

/// Which sources this build has and which are ready to run.
#[tauri::command]
pub async fn sources_info(app: AppHandle) -> Result<Vec<SourceInfoView>, IpcError> {
    blocking(move || Ok(app.state::<AppState>().sources_info())).await
}

/// One cheap authenticated request with the stored key; never returns it.
#[tauri::command]
pub async fn test_source_key(app: AppHandle, source: String) -> Result<KeyCheck, IpcError> {
    blocking(move || app.state::<AppState>().test_source_key(&source)).await
}

/// "Search by title" for one entry's media.
#[tauri::command]
pub async fn search_entry_art(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
    query: String,
) -> Result<Vec<CandidateView>, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .search_entry_art(generation, entry_id, media, &query)
    })
    .await
}

/// A found candidate's image, re-encoded as it would be written (raw bytes).
#[tauri::command]
pub async fn entry_search_art(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
    candidate: u32,
) -> Result<tauri::ipc::Response, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .entry_search_art(generation, entry_id, media, candidate)
            .map(tauri::ipc::Response::new)
    })
    .await
}

/// Writes a found candidate under the ROM's own name and remembers the match.
#[tauri::command]
pub async fn set_entry_art_from_search(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
    candidate: u32,
) -> Result<Vec<EntryArtUpdate>, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .set_entry_art_from_search(generation, entry_id, media, candidate)
    })
    .await
}

/// "Forget manual match".
#[tauri::command]
pub async fn clear_identity_override(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
) -> Result<(), IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .clear_identity_override(generation, entry_id)
    })
    .await
}

/// Checks the channel's manifest. A startup check with "check on start" off
/// requests nothing and answers `skipped`.
#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    trigger: CheckTrigger,
) -> Result<UpdateCheckView, IpcError> {
    blocking(move || {
        let general = app.state::<AppState>().settings().general;
        app.state::<UpdateManager>()
            .check(trigger, &general, env!("CARGO_PKG_VERSION"))
    })
    .await
}

/// Downloads (and verifies) the offered update, emitting `update-progress`.
#[tauri::command]
pub async fn download_update(app: AppHandle) -> Result<(), IpcError> {
    blocking(move || {
        let step = std::time::Duration::from_millis(100);
        let mut last: Option<std::time::Instant> = None;
        let manager = app.state::<UpdateManager>();
        manager.download(&mut |downloaded, total| {
            let done = total == Some(downloaded);
            if done || last.is_none_or(|t| t.elapsed() >= step) {
                last = Some(std::time::Instant::now());
                let _ = app.emit(
                    UPDATE_PROGRESS_EVENT,
                    UpdateProgressEvent { downloaded, total },
                );
            }
        })
    })
    .await
}

/// Installs the downloaded update; refused while a fill job is running or paused.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<UpdateInstallView, IpcError> {
    blocking(move || {
        let held = job_blocks_update(app.state::<AppState>().jobs().current().map(|j| j.state));
        app.state::<UpdateManager>().install(held)
    })
    .await
}

/// Restarts into the installed update; refused while a fill job is running or paused.
#[tauri::command]
pub fn restart_app(
    app: AppHandle,
    state: State<'_, AppState>,
    updates: State<'_, UpdateManager>,
) -> Result<(), IpcError> {
    updates.can_restart(job_blocks_update(state.jobs().current().map(|j| j.state)))?;
    app.restart()
}

/// A quick, name-only look at a folder no layout was detected in.
#[tauri::command]
pub async fn probe_folder(app: AppHandle, path: String) -> Result<FolderProbeView, IpcError> {
    blocking(move || app.state::<AppState>().probe_folder(Path::new(&path))).await
}

/// Planning and apply progress, and the end of an apply, as Tauri events.
struct TauriOrganizeEvents(AppHandle);

impl OrganizeEvents for TauriOrganizeEvents {
    fn progress(&self, event: OrganizeProgressEvent) {
        let _ = self.0.emit(ORGANIZE_PROGRESS_EVENT, event);
    }

    fn changed(&self, view: OrganizeJobView) {
        let _ = self.0.emit(ORGANIZE_CHANGED_EVENT, view);
    }
}

/// Plans organising a loose pile (a dry run), emitting `organize-progress`.
#[tauri::command]
pub async fn organize_plan(
    app: AppHandle,
    request: OrganizeRequest,
    decisions: Option<Vec<coverly_core::organize::Decision>>,
) -> Result<OrganizePlanView, IpcError> {
    blocking(move || {
        let events: Arc<dyn OrganizeEvents> = Arc::new(TauriOrganizeEvents(app.clone()));
        app.state::<AppState>()
            .organize_plan(&request, &decisions.unwrap_or_default(), events)
    })
    .await
}

#[tauri::command]
pub fn organize_cancel(state: State<'_, AppState>, plan_id: u64) -> Result<(), IpcError> {
    state.organize_cancel(plan_id)
}

/// Saves plan `plan_id` as JSON where the user picks; `None` when cancelled.
#[tauri::command]
pub async fn organize_export_plan(
    app: AppHandle,
    plan_id: u64,
) -> Result<Option<String>, IpcError> {
    blocking(move || {
        let state = app.state::<AppState>();
        state.organize().get(plan_id)?;
        let Some(path) = save_path(&app, "coverly-organize-plan.json", ExportFormat::Json)? else {
            return Ok(None);
        };
        state.organize_export_plan(plan_id, &path)?;
        Ok(Some(shown_name(&path)))
    })
    .await
}

/// Applies plan `plan_id` with `decisions` (the backend re-plans from its
/// own survey); progress arrives as `organize-progress`, the end as
/// `organize-changed`.
#[tauri::command]
pub async fn organize_apply(
    app: AppHandle,
    plan_id: u64,
    decisions: Vec<coverly_core::organize::Decision>,
) -> Result<OrganizeJobView, IpcError> {
    blocking(move || {
        let events: Arc<dyn OrganizeEvents> = Arc::new(TauriOrganizeEvents(app.clone()));
        app.state::<AppState>()
            .organize_apply(plan_id, &decisions, events)
    })
    .await
}

/// Continues a paused or interrupted organise job into its journal.
#[tauri::command]
pub async fn organize_resume(app: AppHandle, job: String) -> Result<OrganizeJobView, IpcError> {
    blocking(move || {
        let events: Arc<dyn OrganizeEvents> = Arc::new(TauriOrganizeEvents(app.clone()));
        app.state::<AppState>().organize_resume(&job, events)
    })
    .await
}

/// Stops the running organise apply before its next item.
#[tauri::command]
pub fn organize_cancel_apply(state: State<'_, AppState>) -> Result<(), IpcError> {
    state.organize_cancel_apply()
}

/// Undoes organise job `job`; with `include_later`, the newer jobs that
/// wrote under it too (newest first), else those make it `later-jobs`.
#[tauri::command]
pub async fn organize_undo(
    app: AppHandle,
    job: String,
    include_later: bool,
) -> Result<OrganizeUndoView, IpcError> {
    blocking(move || app.state::<AppState>().organize_undo(&job, include_later)).await
}

/// Duplicates planning and apply progress, and the end of an apply, as
/// Tauri events.
struct TauriDedupeEvents(AppHandle);

impl DedupeEvents for TauriDedupeEvents {
    fn progress(&self, event: DedupeProgressEvent) {
        let _ = self.0.emit(DEDUPE_PROGRESS_EVENT, event);
    }

    fn changed(&self, view: DedupeJobView) {
        let _ = self.0.emit(DEDUPE_CHANGED_EVENT, view);
    }
}

fn dedupe_events(app: &AppHandle) -> Arc<dyn DedupeEvents> {
    Arc::new(TauriDedupeEvents(app.clone()))
}

/// Looks for duplicates in the library at `root` (a dry run), emitting
/// `dedupe-progress`.
#[tauri::command]
pub async fn dedupe_plan(
    app: AppHandle,
    root: String,
    opts: Option<DedupeRequest>,
) -> Result<DedupePlanView, IpcError> {
    blocking(move || {
        let events = dedupe_events(&app);
        app.state::<AppState>()
            .dedupe_plan(Path::new(&root), &opts.unwrap_or_default(), events)
    })
    .await
}

#[tauri::command]
pub fn dedupe_cancel(state: State<'_, AppState>, plan_id: u64) -> Result<(), IpcError> {
    state.dedupe_cancel(plan_id)
}

/// Plan `plan_id` with other levels, rules or destination (nothing is read
/// again).
#[tauri::command]
pub async fn dedupe_regroup(
    app: AppHandle,
    plan_id: u64,
    opts: DedupeRequest,
) -> Result<DedupePlanView, IpcError> {
    blocking(move || app.state::<AppState>().dedupe_regroup(plan_id, &opts)).await
}

#[tauri::command]
pub async fn dedupe_set_keeper(
    app: AppHandle,
    plan_id: u64,
    group: u64,
    member: usize,
) -> Result<DedupePlanView, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .dedupe_set_keeper(plan_id, group, member)
    })
    .await
}

#[tauri::command]
pub async fn dedupe_skip_group(
    app: AppHandle,
    plan_id: u64,
    group: u64,
    skip: bool,
) -> Result<DedupePlanView, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .dedupe_skip_group(plan_id, group, skip)
    })
    .await
}

#[tauri::command]
pub async fn dedupe_skip_system(
    app: AppHandle,
    plan_id: u64,
    system: String,
    skip: bool,
) -> Result<DedupePlanView, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .dedupe_skip_system(plan_id, &system, skip)
    })
    .await
}

/// Forgets the keepers chosen for `root` (`systems`, or all); returns how
/// many.
#[tauri::command]
pub async fn dedupe_reset_choices(
    app: AppHandle,
    root: String,
    systems: Option<Vec<String>>,
) -> Result<usize, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .dedupe_reset_choices(Path::new(&root), systems.as_deref())
    })
    .await
}

/// Sets the extras of plan `plan_id` aside; progress arrives as
/// `dedupe-progress`, the end as `dedupe-changed`. Undo with `organize_undo`.
#[tauri::command]
pub async fn dedupe_apply(app: AppHandle, plan_id: u64) -> Result<DedupeJobView, IpcError> {
    blocking(move || {
        let events = dedupe_events(&app);
        app.state::<AppState>().dedupe_apply(plan_id, events)
    })
    .await
}

/// Stops the running duplicates apply between groups.
#[tauri::command]
pub fn dedupe_cancel_apply(state: State<'_, AppState>) -> Result<(), IpcError> {
    state.dedupe_cancel_apply()
}

/// The current (or last) duplicates apply of this app session.
#[tauri::command]
pub fn dedupe_job(state: State<'_, AppState>) -> Option<DedupeJobView> {
    state.dedupe_job()
}

/// Continues a paused or interrupted duplicates job into its journal.
#[tauri::command]
pub async fn dedupe_resume(app: AppHandle, job: String) -> Result<DedupeJobView, IpcError> {
    blocking(move || {
        let events = dedupe_events(&app);
        app.state::<AppState>().dedupe_resume(&job, events)
    })
    .await
}

/// Saves plan `plan_id` as JSON where the user picks; `None` when cancelled.
#[tauri::command]
pub async fn dedupe_export_plan(app: AppHandle, plan_id: u64) -> Result<Option<String>, IpcError> {
    blocking(move || {
        let state = app.state::<AppState>();
        state.dedupe().get(plan_id)?;
        let Some(path) = save_path(&app, "coverly-duplicates-plan.json", ExportFormat::Json)?
        else {
            return Ok(None);
        };
        state.dedupe_export_plan(plan_id, &path)?;
        Ok(Some(shown_name(&path)))
    })
    .await
}

/// Shows `path` (a copy, its quarantine folder) in Finder / Explorer / the
/// file manager: only inside the open library or a duplicates plan's
/// library or quarantine.
#[tauri::command]
pub fn reveal_path(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), IpcError> {
    let path = state.revealable(Path::new(&path))?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| IpcError::Io(e.to_string()))
}
