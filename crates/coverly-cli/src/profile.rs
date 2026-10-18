//! Layout profile and library root helpers shared by `audit` and `fill`.

use std::fmt::Write as _;
use std::io;
use std::path::Path;

use coverly_core::profiles::custom::{all_profiles, find_profile, ProfileFile};
use coverly_core::profiles::{builtin_profiles, detect, Profile};

use crate::settings::CliSettings;
use crate::Failure;

/// How the profile was chosen, for the report header.
pub(crate) struct Chosen {
    pub(crate) profile: Profile,
    /// `Some(evidence)` when auto-detected.
    pub(crate) detected: Option<Vec<String>>,
    /// Other profiles that scored as high as the chosen one.
    pub(crate) tied: Vec<String>,
}

/// Exit 3 unless `root` is an existing folder.
pub(crate) fn check_root(root: &Path) -> Result<(), Failure> {
    check_folder(root, "library root")
}

/// Exit 3 unless `path` is an existing folder; `what` names it in the
/// message (`library root`, `source`).
pub(crate) fn check_folder(path: &Path, what: &str) -> Result<(), Failure> {
    let message = match path.metadata() {
        Ok(meta) if meta.is_dir() => return Ok(()),
        Ok(_) => format!("{what} is not a folder: {}", path.display()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            format!("{what} not found: {}", path.display())
        }
        Err(e) => format!("cannot read {what} {}: {e}", path.display()),
    };
    Err(Failure { code: 3, message })
}

/// Built-in then custom profile ids, comma-separated.
pub(crate) fn profile_ids(custom: &[ProfileFile]) -> String {
    all_profiles(custom)
        .into_iter()
        .map(|(p, _)| p.id)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The profile named by the flag `id`, else by the `default_profile`
/// setting (built-in or custom), else the best detected one (`auto`).
pub(crate) fn choose_profile(
    root: &Path,
    id: Option<&str>,
    cli: &CliSettings,
) -> Result<Chosen, Failure> {
    let id = id.or(cli.settings.library.default_profile.as_deref());
    if let Some(id) = id.filter(|id| !id.eq_ignore_ascii_case("auto")) {
        let profile = find_profile(&cli.custom, id).ok_or_else(|| {
            Failure::usage(format!(
                "unknown profile `{id}`; expected `auto` or one of: {}",
                profile_ids(&cli.custom)
            ))
        })?;
        return Ok(Chosen {
            profile,
            detected: None,
            tied: Vec::new(),
        });
    }

    let found = detect(root);
    let Some(best) = found.first() else {
        return Err(Failure {
            code: 4,
            message: format!(
                "no layout profile detected in {}; pass --profile <ID> with one of: {}",
                root.display(),
                profile_ids(&cli.custom)
            ),
        });
    };
    let tied = found[1..]
        .iter()
        .filter(|d| d.score == best.score)
        .map(|d| d.profile_id.clone())
        .collect();
    let profile = builtin_profiles()
        .into_iter()
        .find(|p| p.id == best.profile_id)
        .expect("detect only returns built-in profile ids");
    Ok(Chosen {
        profile,
        detected: Some(best.evidence.clone()),
        tied,
    })
}

pub(crate) fn detection_note(chosen: &Chosen, evidence: &[String]) -> String {
    let mut note = format!(
        "profile: {} (auto-detected from {})",
        chosen.profile.id,
        evidence.join(", ")
    );
    if !chosen.tied.is_empty() {
        let _ = write!(
            note,
            "; also matches {}, pass --profile to choose",
            chosen.tied.join(", ")
        );
    }
    note
}
