//! `coverly undo`: remove the files a fill job wrote and restore what it
//! replaced, or put back the files an organise job moved (and remove its
//! copies and the folders it created).

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;

use clap::Args;
use coverly_core::paths::AppDirs;
use coverly_core::write::trash::process_trash;
use coverly_core::write::{
    job_archive_steps, later_jobs, list_jobs, undo_jobs_in_order_with, undo_last_with, ArchiveStep,
    JobKind, JobSummary, UndoReport,
};

use crate::{print_stdout, table, Failure};

#[derive(Debug, Args)]
pub struct UndoArgs {
    /// Undo this job instead of the last one (see --list).
    #[arg(long)]
    job: Option<String>,
    /// List recent jobs instead of undoing.
    #[arg(long, conflicts_with = "job")]
    list: bool,
    /// With --job: also undo the newer jobs that wrote under its folder or
    /// took files from it (newest first), instead of refusing.
    #[arg(long, requires = "job")]
    include_later: bool,
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
    let jobs = list_jobs(&dirs.data).map_err(fail)?;
    let trash = process_trash();
    let reports = match &args.job {
        Some(id) => {
            // Undoing a job under newer ones that wrote below its folder
            // or moved its files on would orphan their work: refuse, or
            // undo them first with --include-later.
            let pending = jobs.iter().any(|j| &j.job == id && !j.undone);
            let later = if pending {
                later_jobs(&dirs.data, id).map_err(fail)?
            } else {
                Vec::new()
            };
            if !later.is_empty() && !args.include_later {
                let ids: Vec<&str> = later.iter().map(|j| j.job.as_str()).collect();
                return Err(fail(format!(
                    "newer jobs changed the same folder: {}; undo them first, or pass --include-later to undo them too",
                    ids.join(", ")
                )));
            }
            let mut ids: Vec<String> = later.into_iter().map(|j| j.job).collect();
            ids.push(id.clone());
            let trashed = trashed_archives(&dirs.data, &ids);
            let reports = undo_jobs_in_order_with(&dirs.data, &ids, &*trash).map_err(fail)?;
            (reports, trashed)
        }
        None => {
            // The journal the last undo picks: the newest not undone.
            let targets: Vec<String> = jobs
                .iter()
                .find(|j| !j.undone)
                .map(|j| j.job.clone())
                .into_iter()
                .collect();
            let trashed = trashed_archives(&dirs.data, &targets);
            let report = undo_last_with(&dirs.data, &*trash).map_err(fail)?;
            (vec![report], trashed)
        }
    };
    let (reports, trashed) = reports;
    let kind_of = |job: &str| {
        jobs.iter()
            .find(|j| j.job == job)
            .map(|j| j.kind)
            .unwrap_or_default()
    };
    let text: String = reports
        .iter()
        .map(|r| match kind_of(&r.job) {
            JobKind::Organize => organize_text(r, &trashed),
            JobKind::Fill => fill_text(r),
        })
        .collect();
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

/// The archives the jobs `ids` sent (or were about to send) to the Trash,
/// read before the undo marks their journals undone. A journal that
/// cannot be read names none: the undo itself reports its error.
fn trashed_archives(data: &std::path::Path, ids: &[String]) -> BTreeSet<PathBuf> {
    ids.iter()
        .filter_map(|id| job_archive_steps(data, id).ok())
        .flatten()
        .filter(|r| !matches!(r.step, ArchiveStep::Kept))
        .map(|r| r.from)
        .collect()
}

/// `Undid organise job …: moved back 2, removed 0 copies, kept 0 changed,
/// 2 folders removed`, then the files left as they are, the archives
/// brought back from the Trash and the ones that could not be.
fn organize_text(report: &UndoReport, trashed: &BTreeSet<PathBuf>) -> String {
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
    for p in report.restored.iter().filter(|p| trashed.contains(*p)) {
        let _ = write!(
            text,
            "\n  archive restored: {}",
            table::one_line(&p.display().to_string())
        );
    }
    for p in &report.archive_lost {
        let _ = write!(
            text,
            "\n  archive lost: {}; the archive couldn't be recovered from the Trash, the extracted files stay",
            table::one_line(&p.display().to_string())
        );
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
