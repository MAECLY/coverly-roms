//! The optional Switch name catalogue in the app (adjustments D2–D10,
//! plan P-2): one handle per data folder in [`AppState`], dropped before
//! the file is deleted; downloads only while `catalogs.switch.enabled` is
//! on, from «Descargar ahora» or before a plan that has a Switch item.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use coverly_core::catalog::{CatalogStatus, SwitchCatalog, CATALOG_FILE};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::organize::Survey;
use coverly_core::settings::{LanguageChoice, Settings, SwitchCatalogSettings};

use super::{lock, AppState};
use crate::error::IpcError;

/// How long a plan waits for a first catalogue download (spec D10).
const PLAN_DEADLINE: Duration = Duration::from_secs(30);

/// The catalogue handle and the data folder it was opened in.
pub(super) type CatalogHandle = Option<(PathBuf, Arc<SwitchCatalog>)>;

/// The HTTP client the catalogue uses: the app's network settings, with
/// the user's fallback URL admitted when the fallback is on (as the CLI's
/// `catalog_http`).
fn catalog_http(net: &NetConfig, sw: &SwitchCatalogSettings) -> PoliteHttp<UreqHttp> {
    let user = if sw.fallback_on() {
        vec![sw.fallback_url.clone()]
    } else {
        Vec::new()
    };
    PoliteHttp::new(UreqHttp::new(net).with_user_urls(user), net)
}

/// `es` or `en` from a BCP 47 tag such as `es-ES`.
fn language_of(tag: &str) -> &'static str {
    if tag.trim().to_ascii_lowercase().starts_with("es") {
        "es"
    } else {
        "en"
    }
}

impl AppState {
    /// Reads the catalogue from `base` instead of `blawar/titledb` (tests
    /// point it at a local mock server).
    pub fn with_catalog_base(mut self, base: impl Into<String>) -> AppState {
        self.catalog_base = Some(base.into());
        self
    }

    /// The UI language the region file follows (spec D3): the language
    /// setting, else what the webview last said it shows, else the
    /// system's (`LC_ALL`, `LC_MESSAGES`, `LANG`).
    fn catalog_locale(&self, s: &Settings) -> &'static str {
        match s.general.language {
            LanguageChoice::Es => "es",
            LanguageChoice::En => "en",
            LanguageChoice::Auto => {
                if let Some(hint) = lock(&self.ui_locale).as_deref() {
                    return language_of(hint);
                }
                let system = ["LC_ALL", "LC_MESSAGES", "LANG"]
                    .iter()
                    .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
                    .unwrap_or_default();
                language_of(&system)
            }
        }
    }

    fn remember_locale(&self, locale: Option<&str>) {
        if let Some(tag) = locale.filter(|t| !t.trim().is_empty()) {
            *lock(&self.ui_locale) = Some(tag.to_string());
        }
    }

    /// The catalogue of `data_dir`, opened (and created) on first use.
    fn switch_catalog(&self, data_dir: &Path) -> Result<Arc<SwitchCatalog>, IpcError> {
        let mut handle = lock(&self.catalog);
        if let Some((dir, catalog)) = handle.as_ref() {
            if dir == data_dir {
                return Ok(Arc::clone(catalog));
            }
        }
        let mut catalog = SwitchCatalog::open(data_dir).map_err(|e| IpcError::Io(e.to_string()))?;
        if let Some(base) = &self.catalog_base {
            catalog = catalog.with_base(base.clone());
        }
        let catalog = Arc::new(catalog);
        *handle = Some((data_dir.to_path_buf(), Arc::clone(&catalog)));
        Ok(catalog)
    }

    /// What the Sources group shows, without any request. Off creates no
    /// file. `locale` is the language the webview shows (used while the
    /// language setting is automatic).
    pub fn catalog_status(&self, locale: Option<&str>) -> CatalogStatus {
        self.remember_locale(locale);
        let cfg = self.fill_config();
        let sw = &cfg.settings.catalogs.switch;
        if !sw.enabled {
            return CatalogStatus::Off;
        }
        match self.switch_catalog(&cfg.data_dir) {
            Ok(catalog) => catalog.status(sw),
            Err(_) => CatalogStatus::FailedNone,
        }
    }

    /// Downloads the catalogue now, or refreshes it when due («Descargar
    /// ahora» / «Buscar actualización»; at most once every `refresh_days`,
    /// D4). `progress` gets `Downloading` while bytes arrive;
    /// [`Self::catalog_cancel`] stops it (nothing is kept). With the
    /// setting off: a bad request, nothing is asked.
    pub fn catalog_update(
        &self,
        locale: Option<&str>,
        progress: &dyn Fn(CatalogStatus),
    ) -> Result<CatalogStatus, IpcError> {
        self.remember_locale(locale);
        let cfg = self.fill_config();
        let sw = &cfg.settings.catalogs.switch;
        if !sw.enabled {
            return Err(IpcError::BadRequest(
                "the Switch catalogue is off; turn it on in Sources first".into(),
            ));
        }
        let catalog = self.switch_catalog(&cfg.data_dir)?;
        self.catalog_cancel.store(false, Ordering::Relaxed);
        let http = catalog_http(&cfg.net, sw);
        let status = catalog.ensure(
            &http,
            sw,
            self.catalog_locale(&cfg.settings),
            SystemTime::now(),
            &self.catalog_cancel,
            None,
            &|done, total| progress(CatalogStatus::Downloading { done, total }),
        );
        // Turned off while it downloaded: nothing is kept (spec §11).
        if self.release_catalog(Some(catalog), &cfg.data_dir) {
            return Ok(CatalogStatus::Off);
        }
        Ok(status)
    }

    /// Stops a running catalogue download (the plan's or «Descargar
    /// ahora»'s) before its next chunk.
    pub fn catalog_cancel(&self) {
        self.catalog_cancel.store(true, Ordering::Relaxed);
    }

    /// «Borrar catálogo»: deletes `catalog.sqlite` and the cached icons.
    /// The handle is dropped first (Windows keeps open files); while a
    /// download or a plan still uses it, the download is stopped and the
    /// answer is `job-running`. Another process holding the file gives
    /// `RemoveFailed`. The setting stays as it is.
    pub fn catalog_remove(&self) -> Result<CatalogStatus, IpcError> {
        self.catalog_cancel();
        let cfg = self.fill_config();
        if !self.remove_catalog(&cfg.data_dir)? {
            return Ok(CatalogStatus::RemoveFailed);
        }
        Ok(if cfg.settings.catalogs.switch.enabled {
            CatalogStatus::Pending
        } else {
            CatalogStatus::Off
        })
    }

    /// Deletes the catalogue of `data_dir`: `Ok(false)` when the file could
    /// not be deleted, `job-running` while the handle is in use.
    fn remove_catalog(&self, data_dir: &Path) -> Result<bool, IpcError> {
        let mut handle = lock(&self.catalog);
        let catalog = match handle.take() {
            Some((dir, catalog)) if dir == data_dir => match Arc::try_unwrap(catalog) {
                Ok(catalog) => catalog,
                Err(shared) => {
                    *handle = Some((dir, shared));
                    return Err(IpcError::JobRunning);
                }
            },
            _ => {
                if !data_dir.join(CATALOG_FILE).exists() {
                    return Ok(true);
                }
                SwitchCatalog::open(data_dir).map_err(|e| IpcError::Io(e.to_string()))?
            }
        };
        drop(handle);
        Ok(catalog.remove(data_dir).is_ok())
    }

    /// Lets go of `catalog` once a download, a plan or an icon is done
    /// with it; when the setting went off meanwhile, the file is deleted
    /// now (spec §11: nothing is kept). True when it was.
    pub(super) fn release_catalog(
        &self,
        catalog: Option<Arc<SwitchCatalog>>,
        data_dir: &Path,
    ) -> bool {
        let Some(catalog) = catalog else {
            return false;
        };
        drop(catalog);
        if self.settings().catalogs.switch.enabled {
            return false;
        }
        self.drop_catalog(data_dir);
        true
    }

    /// Forgets the catalogue after the setting went off (D2: the data is
    /// a cache): a running download is stopped; a handle still in use is
    /// deleted when that use ends ([`Self::catalog_update`]).
    fn drop_catalog(&self, data_dir: &Path) {
        self.catalog_cancel();
        if let Err(e) = self.remove_catalog(data_dir) {
            log::info!("Switch catalogue not deleted yet: {e}");
        }
    }

    /// After a settings change: off deletes the catalogue; icons or the
    /// fallback turned off forget what they allowed (D8, D9).
    pub(super) fn catalog_settings_changed(&self, before: &Settings, after: &Settings) {
        let (old, new) = (&before.catalogs.switch, &after.catalogs.switch);
        if old == new {
            return;
        }
        let data_dir = after.data_dir(&self.base.data_dir);
        if old.enabled && !new.enabled {
            self.drop_catalog(&data_dir);
            return;
        }
        let open = lock(&self.catalog)
            .as_ref()
            .filter(|(dir, _)| *dir == data_dir)
            .map(|(_, c)| Arc::clone(c));
        if let Some(catalog) = open {
            if let Err(e) = catalog.apply_settings(new) {
                log::warn!("Switch catalogue settings: {e}");
            }
        }
    }

    /// The cached eShop icon of title `tid` (16 hex digits, as the plan
    /// shows it), fetched now when it is not cached yet (D8): PNG bytes,
    /// never a URL. `art-missing` while icons are off or when the title
    /// has none.
    pub fn catalog_icon(&self, tid: &str) -> Result<Vec<u8>, IpcError> {
        let id = (tid.len() == 16)
            .then(|| u64::from_str_radix(tid, 16).ok())
            .flatten()
            .ok_or_else(|| IpcError::BadRequest(format!("`{tid}` is not a title id")))?;
        let cfg = self.fill_config();
        let sw = &cfg.settings.catalogs.switch;
        if !sw.icons_on() {
            return Err(IpcError::ArtMissing("icon".into()));
        }
        let catalog = self.switch_catalog(&cfg.data_dir)?;
        let http = catalog_http(&cfg.net, sw);
        let icon = match catalog.icon(&http, sw, id) {
            Ok(Some(path)) => std::fs::read(path).map_err(|e| IpcError::Io(e.to_string())),
            Ok(None) => Err(IpcError::ArtMissing("icon".into())),
            Err(e) => Err(IpcError::Network(e.to_string())),
        };
        if self.release_catalog(Some(catalog), &cfg.data_dir) {
            return Err(IpcError::ArtMissing("icon".into()));
        }
        icon
    }

    /// The catalogue a plan of `survey` names Switch titles with (plan
    /// P-1, P-2): `None` while the setting is off or the pile has no
    /// Switch item. Online, it is downloaded or refreshed first when due
    /// (a first download waits at most 30 s), with `ncas.json` only for
    /// NCA ids without a title id and the nlib fallback only when it is
    /// on; offline, the rows already kept are used as they are.
    pub(super) fn plan_catalog(
        &self,
        settings: &Settings,
        net: &NetConfig,
        data_dir: &Path,
        survey: &Survey,
        offline: bool,
    ) -> Option<Arc<SwitchCatalog>> {
        let sw = &settings.catalogs.switch;
        if !sw.enabled || !survey.has_switch() {
            return None;
        }
        if offline && !data_dir.join(CATALOG_FILE).exists() {
            return None;
        }
        let catalog = self.switch_catalog(data_dir).ok()?;
        if offline {
            return Some(catalog);
        }
        self.catalog_cancel.store(false, Ordering::Relaxed);
        let http = catalog_http(net, sw);
        let now = SystemTime::now();
        let deadline = Some(Instant::now() + PLAN_DEADLINE);
        let locale = self.catalog_locale(settings);
        let cancel = &self.catalog_cancel;
        let quiet = |_: u64, _: Option<u64>| {};
        catalog.ensure(&http, sw, locale, now, cancel, deadline, &quiet);
        if survey.wants_ncas() {
            catalog.ensure_ncas(&http, sw, now, cancel, deadline, &quiet);
        }
        if sw.fallback_on() {
            let ids = survey.switch_title_ids();
            catalog.ensure_nlib(&http, sw, locale, &ids, now, cancel, deadline);
        }
        // Turned off while it downloaded: the plan names nothing from it
        // and the file goes (spec §11).
        if !self.settings().catalogs.switch.enabled {
            self.release_catalog(Some(catalog), data_dir);
            return None;
        }
        Some(catalog)
    }

    /// The catalogue a re-plan from a kept survey reads (no request).
    pub(super) fn kept_catalog(
        &self,
        settings: &Settings,
        data_dir: &Path,
        survey: &Survey,
    ) -> Option<Arc<SwitchCatalog>> {
        let sw = &settings.catalogs.switch;
        if !sw.enabled || !survey.has_switch() || !data_dir.join(CATALOG_FILE).exists() {
            return None;
        }
        self.switch_catalog(data_dir).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::language_of;

    #[test]
    fn webview_tags_map_to_the_two_ui_languages() {
        assert_eq!(language_of("es-ES"), "es");
        assert_eq!(language_of("es"), "es");
        assert_eq!(language_of("en-GB"), "en");
        assert_eq!(language_of("fr-FR"), "en");
        assert_eq!(language_of(""), "en");
    }
}
