//! `coverly catalog switch status|update|remove`: the optional Switch name
//! catalogue (`blawar/titledb`, adjustments spec D2–D5). The CLI never
//! changes the setting that turns it on: `update` with the setting off is
//! an error that says how to turn it on.

use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Instant, SystemTime};

use clap::{Args, Subcommand};
use coverly_core::catalog::{CatalogStatus, SwitchCatalog, CATALOG_FILE};
use coverly_core::net::{PoliteHttp, UreqHttp};
use coverly_core::paths::AppDirs;
use coverly_core::settings::{LanguageChoice, Settings};

use crate::table::human_bytes;
use crate::{print_stdout, Failure};

#[derive(Debug, Args)]
pub struct CatalogArgs {
    #[command(subcommand)]
    catalog: CatalogKind,
}

#[derive(Debug, Subcommand)]
enum CatalogKind {
    /// The optional Switch name catalogue (blawar/titledb, MIT), off unless
    /// the «Catálogo de Switch» setting is on.
    Switch(SwitchArgs),
}

#[derive(Debug, Args)]
pub struct SwitchArgs {
    #[command(subcommand)]
    action: SwitchAction,
    /// Data folder holding `catalog.sqlite` (default: the backup location
    /// setting, else the app data folder).
    #[arg(long, value_name = "DIR", global = true)]
    data_dir: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum SwitchAction {
    /// Show whether the catalogue is on, downloaded and how old it is.
    Status,
    /// Download the catalogue now, or refresh it when it is due.
    Update {
        #[arg(long, hide = true)]
        titledb_url: Option<String>,
    },
    /// Delete the downloaded catalogue and its cached icons.
    Remove,
}

/// Where the setting lives, for the `update` refusal.
pub const ENABLE_HINT: &str = "turn on «Catálogo de Switch» in the app's Sources settings \
(`catalogs.switch.enabled: true` in the settings file)";

fn fail(e: impl std::fmt::Display) -> Failure {
    Failure {
        code: 1,
        message: e.to_string(),
    }
}

/// The UI language the catalogue's region file follows (spec D3): the
/// language setting, else the system's (`LC_ALL`, `LC_MESSAGES`, `LANG`).
pub fn ui_locale(s: &Settings) -> &'static str {
    match s.general.language {
        LanguageChoice::Es => "es",
        LanguageChoice::En => "en",
        LanguageChoice::Auto => {
            let system = ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
                .unwrap_or_default();
            if system.to_ascii_lowercase().starts_with("es") {
                "es"
            } else {
                "en"
            }
        }
    }
}

/// The HTTP client the catalogue uses: the app's network settings, with
/// the user's fallback URL admitted when the fallback is on.
pub fn catalog_http(s: &Settings) -> PoliteHttp<UreqHttp> {
    let net = s.net_config();
    let sw = &s.catalogs.switch;
    let user = if sw.fallback_on() {
        vec![sw.fallback_url.clone()]
    } else {
        Vec::new()
    };
    PoliteHttp::new(UreqHttp::new(&net).with_user_urls(user), &net)
}

/// One line for a status (English, like the rest of the CLI).
pub fn status_text(status: &CatalogStatus) -> String {
    match status {
        CatalogStatus::Off => "off".into(),
        CatalogStatus::Pending => "on; not downloaded yet".into(),
        CatalogStatus::Downloading { done, total } => match total {
            Some(t) => format!("downloading {} of {}", human_bytes(*done), human_bytes(*t)),
            None => format!("downloading {}", human_bytes(*done)),
        },
        CatalogStatus::Ready {
            count,
            updated,
            fallback,
        } => {
            let list = if *fallback {
                " (the United States list: the one for your language was not available)"
            } else {
                ""
            };
            format!("ready: {count} titles, updated {updated}{list}")
        }
        CatalogStatus::FailedOld { updated } => {
            format!("the last update failed; using the list of {updated}")
        }
        CatalogStatus::FailedNone => "the download failed; no list yet".into(),
        CatalogStatus::Rejected => {
            "the catalogue carries data Coverly does not accept; it was not used".into()
        }
        CatalogStatus::RemoveFailed => {
            "could not be deleted; close other Coverly windows and try again".into()
        }
    }
}

pub fn run(args: &CatalogArgs) -> Result<(), Failure> {
    match &args.catalog {
        CatalogKind::Switch(sw) => run_switch(sw),
    }
}

fn data_folder(flag: Option<&Path>, s: &Settings) -> Result<PathBuf, Failure> {
    let data = crate::settings::data_dir(flag, s);
    AppDirs::resolve(None, data.as_deref())
        .map(|d| d.data)
        .ok_or_else(|| fail("cannot find the app data folder; pass --data-dir"))
}

fn run_switch(args: &SwitchArgs) -> Result<(), Failure> {
    let cli = crate::settings::load_cli_settings();
    let s = &cli.settings;
    let sw = &s.catalogs.switch;
    let data = data_folder(args.data_dir.as_deref(), s)?;
    match &args.action {
        SwitchAction::Status => {
            // Off asks nothing of the disk: no file is created.
            if !sw.enabled {
                return print_stdout(&format!("{}\n", status_text(&CatalogStatus::Off)));
            }
            let catalog = SwitchCatalog::open(&data).map_err(fail)?;
            print_stdout(&format!("{}\n", status_text(&catalog.status(sw))))
        }
        SwitchAction::Update { titledb_url } => {
            if !sw.enabled {
                return Err(Failure::usage(format!(
                    "the Switch catalogue is off; {ENABLE_HINT}"
                )));
            }
            let mut catalog = SwitchCatalog::open(&data).map_err(fail)?;
            if let Some(base) = titledb_url {
                catalog = catalog.with_base(base.clone());
            }
            let http = catalog_http(s);
            let cancel = AtomicBool::new(false);
            let tty = io::stderr().is_terminal();
            let step = std::sync::Mutex::new(Instant::now());
            let progress = |done: u64, total: Option<u64>| {
                let mut last = step
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if tty && last.elapsed().as_secs() >= 1 {
                    *last = Instant::now();
                    eprintln!(
                        "{}",
                        status_text(&CatalogStatus::Downloading { done, total })
                    );
                }
            };
            let status = catalog.ensure(
                &http,
                sw,
                ui_locale(s),
                SystemTime::now(),
                &cancel,
                None,
                &progress,
            );
            let text = status_text(&status);
            match status {
                CatalogStatus::Ready { .. } | CatalogStatus::Pending => {
                    print_stdout(&format!("{text}\n"))
                }
                _ => {
                    let detail = catalog
                        .last_error()
                        .map(|e| format!(" ({e})"))
                        .unwrap_or_default();
                    Err(fail(format!("Switch catalogue: {text}{detail}")))
                }
            }
        }
        SwitchAction::Remove => {
            if !data.join(CATALOG_FILE).exists() {
                return print_stdout("no Switch catalogue to remove\n");
            }
            let catalog = SwitchCatalog::open(&data).map_err(fail)?;
            catalog.remove(&data).map_err(|_| {
                fail(format!(
                    "Switch catalogue {}",
                    status_text(&CatalogStatus::RemoveFailed)
                ))
            })?;
            print_stdout("removed the Switch catalogue\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_follows_the_language_setting() {
        let mut s = Settings::default();
        s.general.language = LanguageChoice::Es;
        assert_eq!(ui_locale(&s), "es");
        s.general.language = LanguageChoice::En;
        assert_eq!(ui_locale(&s), "en");
    }

    #[test]
    fn status_lines() {
        assert_eq!(status_text(&CatalogStatus::Off), "off");
        assert_eq!(
            status_text(&CatalogStatus::Ready {
                count: 3,
                updated: "2026-11-02".into(),
                fallback: false
            }),
            "ready: 3 titles, updated 2026-11-02"
        );
    }
}
