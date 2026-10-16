//! Settings on the wire, and the fixed links the About screen may open.

use coverly_core::settings::{FieldError, LoadNotice, LoadReport, Settings};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SettingsView {
    pub settings: Settings,
    /// What loading had to do (shown once as a notice).
    pub notice: Option<LoadNotice>,
    /// File holding what was on disk, when loading set it aside or copied it.
    pub backup: Option<String>,
    /// Fields that fell back to their defaults.
    pub fixed: Vec<String>,
}

impl SettingsView {
    pub fn new(settings: Settings, report: &LoadReport) -> SettingsView {
        SettingsView {
            settings,
            notice: report.notice,
            backup: report.backup.as_ref().map(|p| p.display().to_string()),
            fixed: report.fixed.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SettingsUpdateView {
    /// The saved settings (unchanged when `errors` is not empty).
    pub settings: Settings,
    pub errors: Vec<FieldError>,
    /// The open library must be reopened to show the change.
    pub reopen: bool,
}

/// Links the webview may ask to open (never an arbitrary URL).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkId {
    Repo,
    License,
    Notices,
    Trademarks,
    Releases,
    Issues,
}

pub fn link_url(id: LinkId) -> &'static str {
    match id {
        LinkId::Repo => "https://github.com/MAECLY/coverly-roms",
        LinkId::License => "https://github.com/MAECLY/coverly-roms/blob/main/LICENSE",
        LinkId::Notices => {
            "https://github.com/MAECLY/coverly-roms/blob/main/THIRD-PARTY-NOTICES.md"
        }
        LinkId::Trademarks => "https://github.com/MAECLY/coverly-roms/blob/main/TRADEMARKS.md",
        LinkId::Releases => "https://github.com/MAECLY/coverly-roms/releases",
        LinkId::Issues => "https://github.com/MAECLY/coverly-roms/issues",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_https_on_the_project_repo() {
        for id in [
            LinkId::Repo,
            LinkId::License,
            LinkId::Notices,
            LinkId::Trademarks,
            LinkId::Releases,
            LinkId::Issues,
        ] {
            assert!(
                link_url(id).starts_with("https://github.com/MAECLY/coverly-roms"),
                "{id:?}"
            );
        }
        assert_eq!(
            serde_json::to_value(LinkId::Trademarks).unwrap(),
            "trademarks"
        );
    }
}
