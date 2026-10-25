//! Progress reporting for long phases (gather, apply). The core sends
//! typed events; the CLI and the app decide how to show them.

use serde::{Deserialize, Serialize};

/// A long-running phase of a fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// Identifying ROMs and gathering art candidates.
    Gather,
    /// Downloading and writing art.
    Apply,
}

/// Why a phase or a job paused (spec §10). The app shows a reason and a
/// Resume button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PauseKind {
    /// HTTP 403 / 429 from a source.
    RateLimit,
    /// Offline, unreachable or timed out after the retries.
    Offline,
    /// The library volume is full, read-only or gone.
    Volume,
    /// An extraction ran out of space (archives spec §9.1): `needed`
    /// bytes to write the item again, `free` on the volume that filled up.
    /// The next items would fail the same way, so the job pauses.
    DiskFull { needed: u64, free: u64 },
}

/// One progress event.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgressEvent {
    /// A phase begins with `total` items.
    Started { phase: Phase, total: usize },
    /// Item `done` (0-based count of finished items) of `total` begins.
    Item {
        phase: Phase,
        done: usize,
        total: usize,
        label: String,
    },
    /// A problem that does not stop the phase.
    Warning(String),
    /// The phase ended (completed, paused or cancelled).
    Finished { phase: Phase },
}

/// Receives progress events; shared between worker threads.
pub trait ProgressSink: Send + Sync {
    fn event(&self, e: ProgressEvent);
}

/// Ignores every event.
pub struct NoProgress;

impl ProgressSink for NoProgress {
    fn event(&self, _: ProgressEvent) {}
}
