//! `coverly fill`: identify ROMs, gather art from the enabled sources
//! (libretro-thumbnails, SteamGridDB with a key, …), show the plan and
//! (with `--apply`) write the art. Source keys come from the environment:
//! the CLI has no keychain.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use clap::{Args, ValueEnum};
use coverly_core::art::resolve_art;
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::credentials::{SourceKeys, SGDB_KEY_ENV};
use coverly_core::fill::{Endpoints, FillEnv, FillSettings};
use coverly_core::gather::Scope;
use coverly_core::identify::dat::DATABASE_BASE;
use coverly_core::identify::MatchMethod;
use coverly_core::jobs::{ApplyOptions, ItemErrorKind, ItemStatus, JobReport};
use coverly_core::matching::{parse_region_order, StrategyKind};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::paths::AppDirs;
use coverly_core::plan::{NoneReason, Plan, PlanAction, PlanItem};
use coverly_core::profiles::Media;
use coverly_core::progress::{Phase, ProgressEvent, ProgressSink};
use coverly_core::scan::scan_library;
use coverly_core::settings::KNOWN_SOURCES;
use coverly_core::sources::libretro::{LIBRETRO_BASE, LIBRETRO_RAW_BASE};
use coverly_core::sources::screenscraper::SCREENSCRAPER_BASE;
use coverly_core::sources::steamgriddb::STEAMGRIDDB_BASE;
use coverly_core::sources::MatchKind;
use coverly_core::systems::SystemDb;
use coverly_core::write::OverwritePolicy;

use crate::audit::MediaArg;
use crate::profile::{check_root, choose_profile, detection_note, Chosen};
use crate::{media_name, print_stdout, table, Failure};

/// Review and no-art lines listed in the table, unless `--output` is used.
const PREVIEW: usize = 20;

#[derive(Debug, Args)]
#[command(after_help = "SteamGridDB key: COVERLY_SGDB_KEY (the CLI has no keychain).")]
pub struct FillArgs {
    /// Library root (e.g. the SD card), the folder holding `ROMS`, `Roms`…
    root: PathBuf,
    /// Layout profile id (see `coverly profiles`), or `auto` to detect it
    /// (default: the `default_profile` setting, else `auto`).
    #[arg(long, value_name = "ID|auto")]
    profile: Option<String>,
    /// System folders or ids to fill, comma-separated (default: all).
    #[arg(long, value_delimiter = ',')]
    system: Vec<String>,
    /// Media to fill, comma-separated (default: the media setting, kept to
    /// what the profile can store).
    #[arg(long, value_enum, value_delimiter = ',')]
    media: Vec<MediaArg>,
    /// How the art is picked when several images match (default: the
    /// strategy setting, `rom-region` out of the box).
    #[arg(long, value_enum)]
    strategy: Option<StrategyArg>,
    /// Region preference order (`rom` = the ROM's own regions; default:
    /// the regions setting, `rom,world,usa,europe,japan` out of the box).
    #[arg(long)]
    regions: Option<String>,
    /// How ROMs are identified (default: the matching method setting,
    /// `hash-then-name` out of the box).
    #[arg(long = "match", value_enum)]
    match_method: Option<MatchArg>,
    /// Fill only missing art, or replace all art (needs --overwrite backup).
    #[arg(long, value_enum, default_value_t = ScopeArg::Missing)]
    scope: ScopeArg,
    /// Show the plan only (the default unless the "dry run by default"
    /// setting is off).
    #[arg(long, conflicts_with = "apply")]
    dry_run: bool,
    /// Download and write the art (the default when the "dry run by
    /// default" setting is off).
    #[arg(long)]
    apply: bool,
    /// With --apply, skip items that need review.
    #[arg(long)]
    auto_only: bool,
    /// What to do with existing art (default: the overwrite setting;
    /// `ask` reads as `never`).
    #[arg(long, value_enum)]
    overwrite: Option<OverwriteArg>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = ReportArg::Table)]
    report: ReportArg,
    /// Write the report to this file instead of stdout.
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,
    /// Concurrent requests (1-6; default: the concurrency setting).
    #[arg(long)]
    concurrency: Option<usize>,
    /// Sources to ask, preferred first, comma-separated (overrides the
    /// sources setting), e.g. `libretro,steamgriddb`.
    #[arg(long, value_delimiter = ',', value_name = "IDS")]
    sources: Option<Vec<String>>,
    /// Ignore cached candidates and indexes younger than their TTL.
    #[arg(long)]
    refresh: bool,
    /// Cache folder (indexes, DATs, downloaded art).
    #[arg(long, value_name = "DIR")]
    cache_dir: Option<PathBuf>,
    /// Data folder (job journals and backups for `coverly undo`; default:
    /// the backup location setting, else the app data folder).
    #[arg(long, value_name = "DIR")]
    data_dir: Option<PathBuf>,
    #[arg(long, hide = true)]
    libretro_url: Option<String>,
    #[arg(long, hide = true)]
    libretro_raw_url: Option<String>,
    #[arg(long, hide = true)]
    database_url: Option<String>,
    #[arg(long, hide = true)]
    steamgriddb_url: Option<String>,
    #[arg(long, hide = true)]
    screenscraper_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum StrategyArg {
    Priority,
    RomRegion,
    Popularity,
}

impl From<StrategyArg> for StrategyKind {
    fn from(s: StrategyArg) -> StrategyKind {
        match s {
            StrategyArg::Priority => StrategyKind::Priority,
            StrategyArg::RomRegion => StrategyKind::RomRegion,
            StrategyArg::Popularity => StrategyKind::Popularity,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum MatchArg {
    HashThenName,
    HashOnly,
    NameOnly,
}

impl From<MatchArg> for MatchMethod {
    fn from(m: MatchArg) -> MatchMethod {
        match m {
            MatchArg::HashThenName => MatchMethod::HashThenName,
            MatchArg::HashOnly => MatchMethod::HashOnly,
            MatchArg::NameOnly => MatchMethod::NameOnly,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ScopeArg {
    Missing,
    All,
}

impl From<ScopeArg> for Scope {
    fn from(s: ScopeArg) -> Scope {
        match s {
            ScopeArg::Missing => Scope::Missing,
            ScopeArg::All => Scope::All,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum OverwriteArg {
    Never,
    Backup,
}

impl From<OverwriteArg> for OverwritePolicy {
    fn from(o: OverwriteArg) -> OverwritePolicy {
        match o {
            OverwriteArg::Never => OverwritePolicy::Never,
            OverwriteArg::Backup => OverwritePolicy::Backup,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ReportArg {
    Table,
    Json,
}

/// Prints `gather 40/631` lines to stderr, only when it is a terminal.
struct StderrProgress {
    tty: bool,
}

impl StderrProgress {
    fn new() -> StderrProgress {
        StderrProgress {
            tty: io::stderr().is_terminal(),
        }
    }
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Gather => "gather",
        Phase::Apply => "apply",
    }
}

impl ProgressSink for StderrProgress {
    fn event(&self, e: ProgressEvent) {
        if !self.tty {
            return;
        }
        let line = match e {
            ProgressEvent::Started { phase, total } => format!("{} 0/{total}", phase_name(phase)),
            ProgressEvent::Item {
                phase, done, total, ..
            } if done > 0 && done % 25 == 0 => format!("{} {done}/{total}", phase_name(phase)),
            ProgressEvent::Warning(w) => format!("warning: {w}"),
            _ => return,
        };
        eprintln!("{}", table::terminal_safe(&line));
    }
}

fn fail(e: impl std::fmt::Display) -> Failure {
    Failure {
        code: 1,
        message: e.to_string(),
    }
}

pub fn run(args: &FillArgs) -> Result<(), Failure> {
    check_root(&args.root)?;
    // Absolute (not canonical: no `\\?\` on Windows) so the journal's root
    // and targets stay valid for `coverly undo` from any working directory.
    let root = std::path::absolute(&args.root)?;
    let cli = crate::settings::load_cli_settings();
    let s = &cli.settings;
    let overwrite: OverwritePolicy = args
        .overwrite
        .map(Into::into)
        .unwrap_or_else(|| s.overwrite_policy());
    if args.scope == ScopeArg::All && overwrite != OverwritePolicy::Backup {
        return Err(Failure::usage(
            "--scope all replaces existing art and needs --overwrite backup",
        ));
    }
    let concurrency = args.concurrency.unwrap_or(s.network.concurrency as usize);
    if !(1..=6).contains(&concurrency) {
        return Err(Failure::usage("--concurrency must be between 1 and 6"));
    }
    let mut fill = s.fill_settings();
    if let Some(r) = &args.regions {
        fill.regions = parse_region_order(r).map_err(Failure::usage)?;
    }
    if let Some(st) = args.strategy {
        fill.strategy = st.into();
    }
    if let Some(m) = args.match_method {
        fill.match_method = m.into();
    }
    if let Some(ids) = &args.sources {
        fill.sources = chosen_sources(ids)?;
    }
    let apply = args.apply || (!args.dry_run && !s.safety.dry_run_default);
    let mut chosen = choose_profile(&root, args.profile.as_deref(), &cli)?;
    chosen.profile = s.art_profile(&chosen.profile);
    let profile = &chosen.profile;
    let media = chosen_media(args, &s.art.media, &chosen)?;
    if args.report != ReportArg::Table {
        if let Some(evidence) = &chosen.detected {
            eprintln!(
                "{}",
                table::terminal_safe(&detection_note(&chosen, evidence))
            );
        }
    }
    let db = SystemDb::builtin();
    let scan = scan_library(&root, profile, &db, &s.scan_options());
    let art = resolve_art(&scan, profile, &db, &media);

    let data = crate::settings::data_dir(args.data_dir.as_deref(), s);
    let dirs = AppDirs::resolve(args.cache_dir.as_deref(), data.as_deref()).ok_or_else(|| {
        fail("cannot find the app cache/data folders; pass --cache-dir and --data-dir")
    })?;
    std::fs::create_dir_all(&dirs.cache).map_err(|e| {
        fail(format!(
            "cannot create the cache folder {}: {e}",
            dirs.cache.display()
        ))
    })?;
    let cache = Cache::open(&dirs.cache.join(CACHE_FILE)).map_err(fail)?;
    let net = NetConfig {
        concurrency,
        ..s.net_config()
    };
    let http = PoliteHttp::new(UreqHttp::new(&net), &net);
    let endpoints = Endpoints {
        libretro: args
            .libretro_url
            .clone()
            .unwrap_or_else(|| LIBRETRO_BASE.into()),
        libretro_raw: args
            .libretro_raw_url
            .clone()
            .unwrap_or_else(|| LIBRETRO_RAW_BASE.into()),
        database: args
            .database_url
            .clone()
            .unwrap_or_else(|| DATABASE_BASE.into()),
        steamgriddb: args
            .steamgriddb_url
            .clone()
            .unwrap_or_else(|| STEAMGRIDDB_BASE.into()),
        screenscraper: args
            .screenscraper_url
            .clone()
            .unwrap_or_else(|| SCREENSCRAPER_BASE.into()),
    };
    let keys = SourceKeys::from_env(|name| std::env::var(name).ok());
    let env = FillEnv {
        cache: &cache,
        http: &http,
        endpoints: &endpoints,
        cache_dir: &dirs.cache,
        data_dir: &dirs.data,
        keys: &keys,
    };
    // Manual identities live in the app data dir even when the backup
    // location moves the journals (D32); an explicit --data-dir (tests,
    // portable runs) keeps everything in that folder.
    let overrides_dir = match args.data_dir {
        Some(_) => None,
        None => AppDirs::resolve(None, None).map(|d| d.data),
    };
    let settings = FillSettings {
        systems: args.system.clone(),
        media: media.clone(),
        scope: args.scope.into(),
        refresh: args.refresh,
        overrides_dir,
        ..fill
    };
    let cancel = AtomicBool::new(false);
    let progress = StderrProgress::new();
    let plan = env.plan(&root, profile, &db, &art, &settings, &progress, &cancel);
    for w in &plan.warnings {
        eprintln!("{}", table::terminal_safe(&format!("warning: {w}")));
    }
    if let Some(reason) = &plan.paused {
        emit(args, &chosen, &plan, None)?;
        return Err(Failure {
            code: 5,
            message: format!(
                "{reason}; nothing was written. Run the same command later to continue."
            ),
        });
    }
    if !apply {
        return emit(args, &chosen, &plan, None);
    }

    let aopts = ApplyOptions {
        overwrite,
        include_review: !args.auto_only,
        concurrency,
        ..s.apply_options(&root)
    };
    let report = env
        .apply(&plan, profile, &aopts, &progress, &cancel)
        .map_err(fail)?;
    for w in &report.warnings {
        eprintln!("{}", table::terminal_safe(&format!("warning: {w}")));
    }
    emit(args, &chosen, &plan, Some(&report))?;
    match &report.paused {
        Some(reason) => Err(Failure {
            code: 5,
            message: format!(
                "{reason}; the job was paused. Run the same command later to continue, or `coverly undo`."
            ),
        }),
        None => Ok(()),
    }
}

/// `--sources` trimmed and deduplicated; an id the app does not know is a
/// usage error (one this build cannot use is skipped with a warning by
/// the plan). None left is a usage error too.
fn chosen_sources(ids: &[String]) -> Result<Vec<String>, Failure> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        let id = id.trim().to_ascii_lowercase();
        if id.is_empty() {
            continue;
        }
        if !KNOWN_SOURCES.contains(&id.as_str()) {
            return Err(Failure::usage(format!(
                "unknown source `{id}` (known: {}; the SteamGridDB key comes from {SGDB_KEY_ENV})",
                KNOWN_SOURCES.join(", ")
            )));
        }
        if !out.contains(&id) {
            out.push(id);
        }
    }
    if out.is_empty() {
        return Err(Failure::usage("--sources needs at least one source"));
    }
    Ok(out)
}

/// `--media` deduplicated, or the media setting (`default`); media the
/// profile cannot store are dropped (none left is a usage error).
fn chosen_media(
    args: &FillArgs,
    default: &[Media],
    chosen: &Chosen,
) -> Result<Vec<Media>, Failure> {
    let wanted: Vec<Media> = if args.media.is_empty() {
        default.to_vec()
    } else {
        args.media.iter().map(|m| Media::from(*m)).collect()
    };
    let mut media = Vec::new();
    for m in wanted {
        if chosen.profile.slot(m).is_some() && !media.contains(&m) {
            media.push(m);
        }
    }
    if media.is_empty() {
        return Err(Failure::usage(format!(
            "profile {} cannot store the requested media",
            chosen.profile.id
        )));
    }
    Ok(media)
}

/// Writes the report (table or JSON) to `--output` or stdout.
fn emit(
    args: &FillArgs,
    chosen: &Chosen,
    plan: &Plan,
    job: Option<&JobReport>,
) -> Result<(), Failure> {
    let text = match args.report {
        ReportArg::Json => {
            let mut v = serde_json::json!({
                "mode": if job.is_some() { "apply" } else { "dry-run" },
                "plan": plan,
            });
            if let Some(job) = job {
                v["job"] = serde_json::to_value(job).map_err(fail)?;
            }
            let mut s = serde_json::to_string_pretty(&v).map_err(fail)?;
            s.push('\n');
            s
        }
        ReportArg::Table => render_table(chosen, plan, job, args.output.is_some()),
    };
    match &args.output {
        Some(path) => write_file(path, &text),
        None => print_stdout(&text),
    }
}

/// Writes `text` to `path` (the `--output` file).
pub(crate) fn write_file(path: &Path, text: &str) -> Result<(), Failure> {
    let cannot = |e: io::Error| fail(format!("cannot write {}: {e}", path.display()));
    let mut file = BufWriter::new(File::create(path).map_err(cannot)?);
    file.write_all(text.as_bytes()).map_err(cannot)?;
    file.flush().map_err(cannot)
}

fn item_label(item: &PlanItem) -> String {
    format!(
        "{}/{}  {}",
        item.folder,
        item.rel_path.display(),
        media_name(item.media)
    )
}

fn kind_name(kind: MatchKind, score: u8) -> String {
    match kind {
        MatchKind::Canonical => "canonical".into(),
        MatchKind::NameExact => "name".into(),
        MatchKind::TitleExact => "title".into(),
        MatchKind::Fuzzy => format!("fuzzy {score}"),
    }
}

fn none_reason(reason: &NoneReason) -> String {
    match reason {
        NoneReason::NoCandidates => "no candidates".into(),
        NoneReason::BelowThreshold { best } => {
            format!("best match too weak ({:.0}%)", best * 100.0)
        }
        NoneReason::SourceError { message } => format!("source error: {message}"),
        NoneReason::NoTarget => "the profile has no place for it".into(),
        NoneReason::Skipped => "skipped in review".into(),
    }
}

fn error_kind_name(kind: ItemErrorKind) -> &'static str {
    match kind {
        ItemErrorKind::NoArt => "no-art",
        ItemErrorKind::Network => "network",
        ItemErrorKind::Decode => "decode",
        ItemErrorKind::WriteDenied => "write-denied",
        ItemErrorKind::DiskFull => "disk-full",
        ItemErrorKind::NameInvalid => "name-invalid",
        ItemErrorKind::OutsideRoot => "outside-root",
        ItemErrorKind::VolumeGone => "volume-gone",
        ItemErrorKind::Io => "io",
    }
}

/// Appends `title` and up to [`PREVIEW`] of `lines` (all when `all`).
fn section(out: &mut String, title: &str, lines: &[String], all: bool) {
    if lines.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n{title}");
    let shown = if all {
        lines.len()
    } else {
        lines.len().min(PREVIEW)
    };
    for l in &lines[..shown] {
        let _ = writeln!(out, "  {l}");
    }
    if shown < lines.len() {
        let _ = writeln!(
            out,
            "  ... and {} more (use --output to list all)",
            lines.len() - shown
        );
    }
}

fn render_table(chosen: &Chosen, plan: &Plan, job: Option<&JobReport>, all: bool) -> String {
    let mut out = String::new();
    match &chosen.detected {
        Some(evidence) => {
            let _ = writeln!(out, "{}", detection_note(chosen, evidence));
        }
        None => {
            let _ = writeln!(out, "profile: {}", chosen.profile.id);
        }
    }
    if job.is_none() {
        out.push_str("Dry run: nothing was written. Re-run with --apply to write the art.\n");
    }
    if let Some(reason) = &plan.paused {
        let _ = writeln!(out, "Paused: {reason} (the plan is partial).");
    }
    out.push('\n');

    let mut rows = vec![vec![
        "FOLDER".to_string(),
        "AUTO".into(),
        "REVIEW".into(),
        "NONE".into(),
    ]];
    for (folder, c) in &plan.per_system {
        rows.push(vec![
            folder.clone(),
            c.auto.to_string(),
            c.review.to_string(),
            c.none.to_string(),
        ]);
    }
    let t = &plan.totals;
    rows.push(vec![
        "TOTAL".into(),
        t.auto.to_string(),
        t.review.to_string(),
        t.none.to_string(),
    ]);
    out.push_str(&table::render(&rows));

    let mut review = Vec::new();
    let mut none = Vec::new();
    for item in &plan.items {
        match &item.action {
            PlanAction::Review { chosen, .. } => review.push(format!(
                "{}  -> {}  {}{}",
                item_label(item),
                chosen.name,
                kind_name(chosen.kind, chosen.score),
                if item.note.is_some() {
                    "  (beta/prototype/hack variant)"
                } else {
                    ""
                }
            )),
            PlanAction::None { reason } => {
                none.push(format!("{}  {}", item_label(item), none_reason(reason)))
            }
            PlanAction::Auto { .. } => {}
        }
    }
    section(
        &mut out,
        "Review (best guess, applied unless --auto-only):",
        &review,
        all,
    );
    section(&mut out, "No art found:", &none, all);

    if let Some(job) = job {
        let _ = writeln!(
            out,
            "\nJob {}: filled {}, replaced {}, skipped {}, no art {}, failed {}",
            job.job_id.as_deref().unwrap_or("-"),
            job.filled,
            job.replaced,
            job.skipped,
            job.no_art,
            job.failed
        );
        for r in &job.items {
            if let ItemStatus::Failed { kind, message } = &r.status {
                let _ = writeln!(
                    out,
                    "  {}/{} {}: {}: {message}",
                    r.folder,
                    r.rel_path.display(),
                    media_name(r.media),
                    error_kind_name(*kind)
                );
            }
        }
        if job.job_id.is_some() {
            out.push_str("Undo with: coverly undo\n");
        }
    }
    out
}
