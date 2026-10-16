//! `coverly update check|install` (spec §4, M7): the same manifest as the
//! app, a minisign-verified download, and an atomic self-replace.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand, ValueEnum};
use coverly_core::net::UreqHttp;
use coverly_core::settings::UpdateChannel;
use coverly_core::update::{
    artifact_net_config, check, cli_platform_key, download_verified, resolve_endpoint, Check,
    UpdateError, ENDPOINT_ENV,
};

use crate::{print_stdout, Failure};

/// Debug builds only: trust this key instead of `updater.pub` (tests).
pub const PUBKEY_ENV: &str = "COVERLY_UPDATE_PUBKEY";

#[derive(Debug, Args)]
pub struct UpdateArgs {
    #[command(subcommand)]
    pub action: UpdateAction,
}

#[derive(Debug, Subcommand)]
pub enum UpdateAction {
    /// Say whether a newer version is published (reads the manifest only).
    Check(CommonArgs),
    /// Download, verify and install the newest version of this binary.
    Install(InstallArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ChannelArg {
    Stable,
    Beta,
}

#[derive(Debug, Args)]
pub struct CommonArgs {
    /// Update channel (default: the app's setting).
    #[arg(long, value_enum)]
    pub channel: Option<ChannelArg>,
    /// Print JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    /// Download and verify, but do not replace the binary.
    #[arg(long)]
    pub dry_run: bool,
}

const CURRENT: &str = env!("CARGO_PKG_VERSION");

fn failure(e: UpdateError) -> Failure {
    let code = match e {
        UpdateError::Signature(_) | UpdateError::NoKey => 6,
        _ => 1,
    };
    Failure {
        code,
        message: e.to_string(),
    }
}

fn pubkey() -> Option<String> {
    if cfg!(debug_assertions) {
        if let Some(k) = std::env::var(PUBKEY_ENV)
            .ok()
            .filter(|k| !k.trim().is_empty())
        {
            return Some(k.trim().to_string());
        }
    }
    coverly_core::update::release_pubkey().map(str::to_string)
}

pub fn run(args: &UpdateArgs) -> Result<(), Failure> {
    // Windows: the binary an earlier update moved aside, now that it no
    // longer runs.
    #[cfg(windows)]
    if let Ok(exe) = std::env::current_exe() {
        remove_stale_old(&install_target(&exe));
    }
    let cli = crate::settings::load_cli_settings();
    let common = match &args.action {
        UpdateAction::Check(c) => c,
        UpdateAction::Install(i) => &i.common,
    };
    let channel = match common.channel {
        Some(ChannelArg::Stable) => UpdateChannel::Stable,
        Some(ChannelArg::Beta) => UpdateChannel::Beta,
        None => cli.settings.general.update_channel,
    };
    let url =
        resolve_endpoint(std::env::var(ENDPOINT_ENV).ok().as_deref(), channel).map_err(failure)?;
    let net = cli.settings.net_config();
    let http = UreqHttp::new(&net);
    let outcome = check(&http, &url, CURRENT).map_err(failure)?;
    match (&args.action, outcome) {
        (UpdateAction::Check(c), outcome) => report(c.json, &outcome),
        (UpdateAction::Install(_), Check::UpToDate { latest }) => print_stdout(&format!(
            "coverly is up to date ({CURRENT}; latest {latest}).\n"
        )),
        (UpdateAction::Install(i), Check::Available(m)) => {
            let key = pubkey().ok_or_else(|| failure(UpdateError::NoKey))?;
            let platform = cli_platform_key()
                .ok_or_else(|| failure(UpdateError::NoPlatform(std::env::consts::OS.into())))?;
            // The settings' timeout is sized for small answers; the binary
            // gets at least MIN_ARTIFACT_TIMEOUT.
            let artifact_http = UreqHttp::new(&artifact_net_config(&net));
            let bytes = download_verified(&artifact_http, &m, &platform, &key).map_err(failure)?;
            if i.dry_run {
                return print_stdout(&format!(
                    "{} {} verified ({} bytes); not installed (dry run).\n",
                    platform,
                    m.version,
                    bytes.len()
                ));
            }
            let exe = install_target(&std::env::current_exe()?);
            replace_executable(&exe, &bytes).map_err(|e| Failure {
                code: 1,
                message: format!(
                    "could not replace {}: {e}; the current binary is unchanged",
                    exe.display()
                ),
            })?;
            // Usually refused right now (it is this running process); the
            // next `coverly update` removes it.
            #[cfg(windows)]
            remove_stale_old(&exe);
            print_stdout(&format!("Updated to {} (was {CURRENT}).\n", m.version))
        }
    }
}

fn report(json: bool, outcome: &Check) -> Result<(), Failure> {
    let (latest, available, notes) = match outcome {
        Check::UpToDate { latest } => (latest.clone(), false, None),
        Check::Available(m) => (m.version.clone(), true, m.notes.clone()),
    };
    if json {
        let v = serde_json::json!({ "current": CURRENT, "latest": latest, "available": available, "notes": notes });
        return print_stdout(&format!("{v}\n"));
    }
    if available {
        print_stdout(&format!(
            "coverly {latest} is available (you have {CURRENT}). Run `coverly update install`.\n"
        ))
    } else {
        print_stdout(&format!("coverly is up to date ({CURRENT}).\n"))
    }
}

/// The file to replace: `exe` with every symlink resolved, so an update
/// through a link (`/usr/local/bin/coverly` → the real binary) replaces the
/// real file and keeps the link.
pub fn install_target(exe: &Path) -> PathBuf {
    fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf())
}

/// Where Windows parks the running binary while it is replaced.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn old_path(exe: &Path) -> PathBuf {
    exe.with_extension("old.exe")
}

/// Deletes the `coverly.old.exe` an earlier update left next to `exe`.
/// Best effort: Windows refuses while that file still runs. True when one
/// was removed.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn remove_stale_old(exe: &Path) -> bool {
    let old = old_path(exe);
    old.is_file() && fs::remove_file(&old).is_ok()
}

/// Writes `bytes` next to `exe` and renames it over `exe` (Windows: moves the
/// running file aside first). On failure `exe` is unchanged and no temp file is left.
pub fn replace_executable(exe: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = exe
        .parent()
        .ok_or_else(|| io::Error::other("no parent folder"))?;
    let name = exe
        .file_name()
        .ok_or_else(|| io::Error::other("no file name"))?
        .to_string_lossy();
    let tmp = dir.join(format!(".{name}.new-{}", std::process::id()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
        }
        #[cfg(windows)]
        {
            let old = old_path(exe);
            let _ = fs::remove_file(&old);
            fs::rename(exe, &old)?;
            if let Err(e) = fs::rename(&tmp, exe) {
                let _ = fs::rename(&old, exe);
                return Err(e);
            }
            Ok(())
        }
        #[cfg(not(windows))]
        fs::rename(&tmp, exe)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stale_old_binary_is_removed_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("coverly.exe");
        fs::write(&exe, b"new").unwrap();
        assert_eq!(old_path(&exe), dir.path().join("coverly.old.exe"));
        assert!(!remove_stale_old(&exe), "nothing to remove");
        fs::write(old_path(&exe), b"old").unwrap();
        assert!(remove_stale_old(&exe));
        assert!(!old_path(&exe).exists());
        assert_eq!(fs::read(&exe).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn the_install_target_is_the_real_file_behind_links() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real").join("coverly");
        fs::create_dir(real.parent().unwrap()).unwrap();
        fs::write(&real, b"x").unwrap();
        let hop = dir.path().join("hop");
        let link = dir.path().join("coverly");
        std::os::unix::fs::symlink(&real, &hop).unwrap();
        std::os::unix::fs::symlink(&hop, &link).unwrap();
        assert_eq!(install_target(&link), fs::canonicalize(&real).unwrap());
        let missing = dir.path().join("missing");
        assert_eq!(install_target(&missing), missing, "unresolvable: as given");
    }
}
