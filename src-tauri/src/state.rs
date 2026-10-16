//! App-wide state behind the IPC commands: the open library session, the
//! generation counter that lets a newer open supersede an older one, and
//! the library settings file. Free of Tauri types so it is unit-testable.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::settings::{LibrarySettings, RecentLibrary};
use coverly_core::systems::SystemDb;

use crate::error::IpcError;
use crate::library::view::{LibrarySettingsView, LibraryView, RecentView, ScanProgressEvent};
use crate::library::{open_session, OpenRequest, Session};

pub struct AppState {
    latest: AtomicU64,
    session: Mutex<Option<Arc<Session>>>,
    settings_path: PathBuf,
    cache_dir: PathBuf,
}

impl AppState {
    pub fn new(settings_path: PathBuf, cache_dir: PathBuf) -> AppState {
        AppState {
            latest: AtomicU64::new(0),
            session: Mutex::new(None),
            settings_path,
            cache_dir,
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
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| IpcError::Io(e.to_string()))?;
        Cache::open(&self.cache_dir.join(CACHE_FILE)).map_err(|e| IpcError::Io(e.to_string()))
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
