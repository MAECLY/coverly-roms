//! Layout profile and library root helpers shared by `audit` and `fill`.

use std::fmt::Write as _;
use std::io;
use std::path::Path;

use coverly_core::profiles::{builtin_profiles, detect, Profile};

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
    let message = match root.metadata() {
        Ok(meta) if meta.is_dir() => return Ok(()),
        Ok(_) => format!("library root is not a folder: {}", root.display()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            format!("library root not found: {}", root.display())
        }
        Err(e) => format!("cannot read library root {}: {e}", root.display()),
    };
    Err(Failure { code: 3, message })
}

pub(crate) fn profile_ids() -> String {
    builtin_profiles()
        .into_iter()
        .map(|p| p.id)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The profile named `id`, or the best detected one for `auto`.
pub(crate) fn choose_profile(root: &Path, id: &str) -> Result<Chosen, Failure> {
    if !id.eq_ignore_ascii_case("auto") {
        let profile = builtin_profiles()
            .into_iter()
            .find(|p| p.id.eq_ignore_ascii_case(id))
            .ok_or_else(|| {
                Failure::usage(format!(
                    "unknown profile `{id}`; expected `auto` or one of: {}",
                    profile_ids()
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
                profile_ids()
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
