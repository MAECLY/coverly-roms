//! `coverly audit`: scan a library and report missing art (read-only).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use coverly_core::art::ArtState;
use coverly_core::audit::{build_report, write_csv, AuditReport, SystemReport};
use coverly_core::profiles::Media;
use coverly_core::systems::SystemDb;

use crate::profile::{check_root, choose_profile, detection_note, Chosen};
use crate::{media_name, print_stdout, table, Failure};

/// Missing entries listed per system in the table, unless `--missing-only`.
const MISSING_PREVIEW: usize = 20;

#[derive(Debug, Args)]
pub struct AuditArgs {
    /// Library root (e.g. the SD card), the folder holding `ROMS`, `Roms`…
    root: PathBuf,
    /// Layout profile id (see `coverly profiles`), or `auto` to detect it
    /// (default: the `default_profile` setting, else `auto`).
    #[arg(long, value_name = "ID|auto")]
    profile: Option<String>,
    /// Media to audit, comma-separated.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "cover,screenshot,title"
    )]
    media: Vec<MediaArg>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Table)]
    format: Format,
    /// Only list ROMs missing at least one audited media (the table then
    /// lists all of them instead of the first 20 per system).
    #[arg(long)]
    missing_only: bool,
    /// Write the report to this file instead of stdout.
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum MediaArg {
    Cover,
    Screenshot,
    Title,
}

impl From<MediaArg> for Media {
    fn from(m: MediaArg) -> Media {
        match m {
            MediaArg::Cover => Media::Cover,
            MediaArg::Screenshot => Media::Screenshot,
            MediaArg::Title => Media::Title,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Format {
    Table,
    Json,
    Csv,
}

pub fn run(args: &AuditArgs) -> Result<(), Failure> {
    check_root(&args.root)?;
    let cli = crate::settings::load_cli_settings();
    let mut chosen = choose_profile(&args.root, args.profile.as_deref(), &cli)?;
    chosen.profile = cli.settings.art_profile(&chosen.profile);
    let mut media: Vec<Media> = Vec::new();
    for m in args.media.iter().map(|m| Media::from(*m)) {
        if !media.contains(&m) {
            media.push(m);
        }
    }

    if args.format != Format::Table {
        if let Some(evidence) = &chosen.detected {
            eprintln!(
                "{}",
                table::terminal_safe(&detection_note(&chosen, evidence))
            );
        }
    }

    let report = build_report(
        &args.root,
        &chosen.profile,
        &SystemDb::builtin(),
        &cli.settings.scan_options(),
        &media,
    );
    for w in &report.warnings {
        eprintln!(
            "{}",
            table::terminal_safe(&format!("warning: {}: {}", w.path.display(), w.message))
        );
    }

    let mut buf: Vec<u8> = Vec::new();
    match args.format {
        Format::Table => {
            buf.extend(render_table(&report, &chosen, args.missing_only).into_bytes());
        }
        Format::Json => {
            serde_json::to_writer_pretty(&mut buf, &report).map_err(io::Error::from)?;
            buf.push(b'\n');
        }
        Format::Csv => write_csv(&report, &mut buf, args.missing_only)?,
    }

    match &args.output {
        Some(path) => {
            let mut file = BufWriter::new(File::create(path).map_err(|e| Failure {
                code: 1,
                message: format!("cannot write {}: {e}", path.display()),
            })?);
            file.write_all(&buf)?;
            file.flush()?;
            Ok(())
        }
        None => print_stdout(&String::from_utf8_lossy(&buf)),
    }
}

/// The plain-text report: header, one row per folder, totals, then the
/// ROMs missing art per system.
fn render_table(report: &AuditReport, chosen: &Chosen, missing_only: bool) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "library: {}", report.root.display());
    match &chosen.detected {
        Some(evidence) => {
            let _ = writeln!(out, "{}", detection_note(chosen, evidence));
        }
        None => {
            let _ = writeln!(out, "profile: {}", chosen.profile.id);
        }
    }
    out.push('\n');

    let supported = |m: &Media| chosen.profile.slot(*m).is_some();
    let count = |missing: &BTreeMap<Media, usize>, m: &Media| {
        if supported(m) {
            missing.get(m).copied().unwrap_or(0).to_string()
        } else {
            "-".to_string()
        }
    };

    let mut header = vec!["FOLDER".to_string(), "SYSTEM".into(), "ROMS".into()];
    header.extend(
        report
            .media
            .iter()
            .map(|m| format!("MISSING-{}", media_name(*m).to_uppercase())),
    );
    let mut rows = vec![header];
    for s in &report.systems {
        let mut row = vec![s.folder.clone()];
        match &s.system_id {
            Some(id) => {
                row.push(id.clone());
                row.push(s.roms.to_string());
                row.extend(report.media.iter().map(|m| count(&s.missing, m)));
            }
            None => {
                row.push("(unknown)".into());
                row.push("-".into());
                row.extend(report.media.iter().map(|_| "-".to_string()));
            }
        }
        rows.push(row);
    }
    let t = &report.totals;
    let mut total = vec![
        "TOTAL".to_string(),
        format!("{} systems", t.systems_known),
        t.roms.to_string(),
    ];
    total.extend(report.media.iter().map(|m| count(&t.missing, m)));
    rows.push(total);
    out.push_str(&table::render(&rows));

    let _ = writeln!(
        out,
        "\n{} ROMs, {} complete, {} orphan images, {} unknown folders skipped, {} warnings",
        t.roms,
        t.complete,
        t.orphans,
        t.systems_unknown,
        report.warnings.len()
    );

    for s in &report.systems {
        missing_section(&mut out, s, missing_only);
    }
    out
}

/// Lists the ROMs of `system` missing any audited media (the first
/// [`MISSING_PREVIEW`] unless `all`).
fn missing_section(out: &mut String, system: &SystemReport, all: bool) {
    let missing: Vec<_> = system
        .entries
        .iter()
        .filter(|e| e.is_missing_any())
        .collect();
    if missing.is_empty() {
        return;
    }
    let id = system.system_id.as_deref().unwrap_or("?");
    let _ = writeln!(
        out,
        "\n{} ({id}): {} missing art",
        system.folder,
        missing.len()
    );
    let shown = if all {
        missing.len()
    } else {
        missing.len().min(MISSING_PREVIEW)
    };
    let rows: Vec<Vec<String>> = missing[..shown]
        .iter()
        .map(|e| {
            let lacks: Vec<&str> = e
                .art
                .iter()
                .filter(|(_, state)| **state == ArtState::Missing)
                .map(|(m, _)| media_name(*m))
                .collect();
            vec![format!("  {}", e.rel_path.display()), lacks.join(",")]
        })
        .collect();
    out.push_str(&table::render(&rows));
    if shown < missing.len() {
        let _ = writeln!(
            out,
            "  ... and {} more (use --missing-only to list all)",
            missing.len() - shown
        );
    }
}
