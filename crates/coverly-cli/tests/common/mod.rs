//! The one way CLI tests start `coverly`: no settings file and the OS
//! keychain turned off, so `cargo test` never reads a developer's saved
//! keys (a macOS prompt per test, a real key sent to a mock server).
//! A test that needs its own settings file sets `COVERLY_SETTINGS` again;
//! the later `.env` wins.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;

/// The built `coverly` binary.
pub fn coverly_exe() -> PathBuf {
    assert_cmd::cargo::cargo_bin("coverly")
}

/// `coverly` with no settings file and no keychain.
pub fn coverly() -> Command {
    coverly_at(&coverly_exe())
}

/// A copy of `coverly` at `exe` (the update tests), opted out the same way.
pub fn coverly_at(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    cmd.env("COVERLY_SETTINGS", "")
        .env("COVERLY_NO_KEYCHAIN", "1");
    cmd
}
