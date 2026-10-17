//! `coverly undo`: remove the files a fill job wrote and restore what it
//! replaced, or put back the files an organise job moved (and remove its
//! copies and the folders it created).

use std::fmt::Write as _;
use std::path::PathBuf;

use clap::Args;
use coverly_core::paths::AppDirs;
use coverly_core::write::{list_jobs, undo_job, undo_last, JobKind, JobSummary, UndoReport};

use crate::{print_stdout, table, Failure};

#[derive(Debug, Args)]
pub struct UndoArgs {
    /// Undo this job instead of the last one (see --list).
    #[arg(long)]
    job: Option<String>,
    /// List recent jobs instead of undoing.
    #[arg(long, conflicts_with = "job")]
    list: bool,
    /// Data folder (job journals and backups; default: the backup location
    /// setting, else the app data folder).
    #[arg(long, value_name = "DIR")]
    data_dir: Option<PathBuf>,
}

fn fail(e: impl std::fmt::Display) -> Failure {
    Failure {
        code: 1,
        message: e.to_string(),
    }
}

pub fn run(args: &UndoArgs) -> Result<(), Failure> {
    let cli = crate::settings::load_cli_settings();
    let data = crate::settings::data_dir(args.data_dir.as_deref(), &cli.settings);
    let dirs = AppDirs::resolve(None, data.as_deref())
        .ok_or_else(|| fail("cannot find the app data folder; pass --data-dir"))?;
    if args.list {
        return print_stdout(&jobs_table(&list_jobs(&dirs.data).map_err(fail)?));
    }
    // The job about to be undone (the newest one not undone yet without
    // --job), for its kind.
    let jobs = list_jobs(&dirs.data).map_err(fail)?;
    let kind = match &args.job {
        Some(id) => jobs.iter().find(|j| &j.job == id && !j.undone),
        None => jobs.iter().find(|j| !j.undone),
    }
    .map(|j| j.kind)
    .unwrap_or_default();
    let report = match &args.job {
        Some(id) => undo_job(&dirs.data, id),
        None => undo_last(&dirs.data),
    }
    .map_err(fail)?;
    let text = match kind {
        JobKind::Organize => organize_text(&report),
        JobKind::Fill => fill_text(&report),
    };
    print_stdout(&text)
}

fn kind_name(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Fill => "fill",
        JobKind::Organize => "organize",
    }
}

fn fill_text(report: &UndoReport) -> String {
    let mut text = format!(
        "Undid job {}: removed {}, restored {}",
        report.job,
        report.removed.len(),
        report.restored.len()
    );
    if !report.skipped_modified.is_empty() {
        let _ = write!(
            text,
            ", kept {} file(s) changed since the job:",
            report.skipped_modified.len()
        );
        for p in &report.skipped_modified {
            let _ = write!(text, "\n  {}", p.display());
        }
    }
    text.push('\n');
    text
}

/// `Undid organise job …: moved back 2, removed 0 copies, kept 0 changed,
/// 2 folders removed`, then the files left as they are.
fn organize_text(report: &UndoReport) -> String {
    let mut text = format!(
        "Undid organise job {}: moved back {}, removed {} copies, kept {} changed, {} folders removed",
        report.job,
        report.restored.len(),
        report.removed.len(),
        report.skipped_modified.len(),
        report.dirs_removed.len()
    );
    if !report.skipped_occupied.is_empty() {
        let _ = write!(
            text,
            ", left {} whose original place is taken",
            report.skipped_occupied.len()
        );
    }
    for p in report
        .skipped_modified
        .iter()
        .chain(&report.skipped_occupied)
    {
        let _ = write!(text, "\n  kept {}", p.display());
    }
    text.push('\n');
    text
}

fn jobs_table(jobs: &[JobSummary]) -> String {
    if jobs.is_empty() {
        return "No jobs.\n".into();
    }
    let mut rows = vec![vec![
        "JOB".to_string(),
        "KIND".into(),
        "CREATED".into(),
        "ROOT".into(),
        "FILES".into(),
        "STATE".into(),
    ]];
    for j in jobs {
        let state = if j.undone {
            "undone"
        } else if j.complete {
            "done"
        } else {
            "unfinished"
        };
        rows.push(vec![
            j.job.clone(),
            kind_name(j.kind).into(),
            j.created_at.clone(),
            j.root.display().to_string(),
            j.entries.to_string(),
            state.into(),
        ]);
    }
    table::render(&rows)
}
