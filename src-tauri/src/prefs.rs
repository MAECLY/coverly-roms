//! Settings on the wire, and the fixed links the About screen may open.

use coverly_core::archive::external::FoundTool;
use coverly_core::archive::Tool;
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
    /// The external archive tools found on this machine, in preference
    /// order (Configuración → Organizar; archives spec §5.2). Listed
    /// whatever `settings.organize.external_tools` says, so the screen can
    /// show what turning it on would use.
    pub external_tools_found: Vec<FoundToolView>,
}

impl SettingsView {
    /// The view with no external tool listed; see [`Self::with_tools`].
    pub fn new(settings: Settings, report: &LoadReport) -> SettingsView {
        SettingsView {
            settings,
            notice: report.notice,
            backup: report.backup.as_ref().map(|p| p.display().to_string()),
            fixed: report.fixed.clone(),
            external_tools_found: Vec::new(),
        }
    }

    /// Lists `found` as the external tools of this machine.
    pub fn with_tools(mut self, found: &[FoundTool]) -> SettingsView {
        self.external_tools_found = found.iter().map(FoundToolView::from).collect();
        self
    }
}

/// One external archive tool found on this machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FoundToolView {
    pub tool: Tool,
    /// From the tool's banner (`24.08`, `7.01`).
    pub version: String,
    /// Absolute path of the executable.
    pub path: String,
}

impl From<&FoundTool> for FoundToolView {
    fn from(t: &FoundTool) -> Self {
        FoundToolView {
            tool: t.tool,
            version: t.version.clone(),
            path: t.path.display().to_string(),
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
    fn found_tools_are_listed_with_kind_version_and_path() {
        let found = FoundTool {
            tool: Tool::Unrar,
            path: "/opt/homebrew/bin/unrar".into(),
            version: "7.01".into(),
            rar5: true,
        };
        let view = SettingsView::new(Settings::default(), &LoadReport::default())
            .with_tools(std::slice::from_ref(&found));
        assert_eq!(
            serde_json::to_value(&view.external_tools_found).unwrap(),
            serde_json::json!([{
                "tool": "unrar",
                "version": "7.01",
                "path": "/opt/homebrew/bin/unrar",
            }])
        );
    }

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
