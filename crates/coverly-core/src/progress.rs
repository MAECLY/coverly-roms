//! Progress reporting for long phases (gather, apply). The core sends
//! typed events; the CLI and the app decide how to show them.

use serde::Serialize;

/// A long-running phase of a fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// Identifying ROMs and gathering art candidates.
    Gather,
    /// Downloading and writing art.
    Apply,
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
