//! `coverly organize`: identify the ROMs of a loose pile, propose one
//! folder per system in the chosen layout, and show the plan (dry run).
//! Applying the plan is `--apply` (organise jobs, undone by `coverly undo`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, IsTerminal};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use clap::{Args, ValueEnum};
use coverly_core::archive::{FoundTool, Tool};
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::credentials::SourceKeys;
use coverly_core::identify::dat::{DatStore, DATABASE_BASE};
use coverly_core::identify::evidence::{Evidence, IdLevel};
use coverly_core::identify::sniff::NotARom;
use coverly_core::net::{PoliteHttp, UreqHttp};
use coverly_core::organize::online::ScreenScraperLookup;
use coverly_core::organize::{
    apply_organize, load_plan, plan_from_survey, resume_organize, survey, ApplyEnv, ApplyError,
    Choice, Decision, ItemNote, ItemOutcome, ItemStatus, LeftReason, NameMode, OrganizeAction,
    OrganizeEnv, OrganizeError, OrganizeItem, OrganizeOptions, OrganizePhase, OrganizePlan,
    OrganizeProgress, OrganizeReport, OsMetaProbe, PlanWarning, RenameReason, SkipReason, Survey,
    Transfer, TransferMode,
};
use coverly_core::paths::AppDirs;
use coverly_core::profiles::custom::all_profiles;
use coverly_core::profiles::Profile;
use coverly_core::settings::{OrganizeDedupeMode, Settings};
use coverly_core::sources::screenscraper::{ScreenScraperSource, SsConfig, SCREENSCRAPER_BASE};
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::process_trash;

use crate::dedupe::{reason_text, relation_text};
use crate::fill::write_file;
use crate::profile::{check_folder, profile_ids};
use crate::{print_stdout, table, Failure};

#[derive(Debug, Args)]
pub struct OrganizeArgs {
    /// The folder holding the loose ROMs.
    source: PathBuf,
    /// Target layout profile id (see `coverly profiles`; default: the
    /// organise layout setting, else `anbernic-stock`).
    #[arg(long, value_name = "PROFILE_ID")]
    layout: Option<String>,
    /// Where the system folders go (default: the source, in place).
    #[arg(long, value_name = "DIR")]
    target: Option<PathBuf>,
    /// System folders directly in the target, without the layout's ROMs
    /// folder.
    #[arg(long)]
    flat: bool,
    /// Copy the files and leave the source untouched.
    #[arg(long, conflicts_with = "move_")]
    copy: bool,
    /// Move the files (default on the same volume; copy, verify, delete
    /// across volumes).
    #[arg(long = "move")]
    move_: bool,
    /// Keep the file names or use the catalogue names (default: the names
    /// setting, `keep` out of the box).
    #[arg(long, value_enum)]
    names: Option<NamesArg>,
    /// Settle an item: FILE (relative to the source) goes to SYSTEM_ID
    /// (see `coverly systems`). Repeatable; split on the last `=`.
    #[arg(long = "assign", value_name = "FILE=SYSTEM_ID")]
    assign: Vec<String>,
    /// Continue an interrupted organise job (its saved plan decides the
    /// layout, target, mode and names).
    #[arg(
        long,
        value_name = "JOB_ID",
        conflicts_with_all = [
            "apply", "dry_run", "layout", "target", "flat", "copy", "move_",
            "names", "assign", "include_review", "duplicates",
        ]
    )]
    resume: Option<String>,
    /// What to do with pile copies of games the library already holds, or
    /// that the pile holds twice: set them aside in `<source>/_Duplicados`
    /// (or the Trash, when the duplicates action setting says so), only
    /// report them, or not look (default: the duplicates setting, else
    /// set-aside).
    #[arg(long, value_enum, value_name = "MODE")]
    duplicates: Option<DuplicatesArg>,
    /// Move the items to review with their best candidate system.
    #[arg(long)]
    include_review: bool,
    /// Cached catalogues only: no download, no online lookup.
    #[arg(long)]
    offline: bool,
    /// Never run unrar, 7-Zip or bsdtar: RAR archives (and 7z coders the
    /// built-in reader lacks) stay where they are.
    #[arg(long)]
    no_external_tools: bool,
    /// Show the plan only (the default unless the "dry run by default"
    /// setting is off).
    #[arg(long, conflicts_with = "apply")]
    dry_run: bool,
    /// Move or copy the files.
    #[arg(long)]
    apply: bool,
    /// Output format.
    #[arg(long, value_enum, default_value_t = ReportArg::Table)]
    report: ReportArg,
    /// Write the report to this file instead of stdout.
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,
    /// Data folder (job journals for `coverly undo`; default: the backup
    /// location setting, else the app data folder).
    #[arg(long, value_name = "DIR")]
    data_dir: Option<PathBuf>,
    /// Cache folder (catalogues, identification memo).
    #[arg(long, value_name = "DIR")]
    cache_dir: Option<PathBuf>,
    #[arg(long, hide = true)]
    database_url: Option<String>,
    #[arg(long, hide = true)]
    screenscraper_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum DuplicatesArg {
    SetAside,
    Report,
    Off,
}

impl From<DuplicatesArg> for OrganizeDedupeMode {
    fn from(d: DuplicatesArg) -> OrganizeDedupeMode {
        match d {
            DuplicatesArg::SetAside => OrganizeDedupeMode::SetAside,
            DuplicatesArg::Report => OrganizeDedupeMode::Report,
            DuplicatesArg::Off => OrganizeDedupeMode::Off,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum NamesArg {
    Keep,
    Catalogue,
}

impl From<NamesArg> for NameMode {
    fn from(n: NamesArg) -> NameMode {
        match n {
            NamesArg::Keep => NameMode::Keep,
            NamesArg::Catalogue => NameMode::Catalogue,
        }
    }
}

/// What a run does: show the plan, apply it, or continue a job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    DryRun,
    Apply,
    Resume,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::DryRun => "dry-run",
            Mode::Apply => "apply",
            Mode::Resume => "resume",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ReportArg {
    Table,
    Json,
}

fn fail(e: impl std::fmt::Display) -> Failure {
    Failure {
        code: 1,
        message: e.to_string(),
    }
}

fn organize_failure(e: OrganizeError) -> Failure {
    let code = match &e {
        OrganizeError::SourceMissing(_) => 3,
        OrganizeError::UnknownLayout(_)
        | OrganizeError::UnknownSystem(_)
        | OrganizeError::UnknownItem(_) => 2,
        OrganizeError::Unreadable(..) | OrganizeError::Cancelled => 1,
    };
    Failure {
        code,
        message: e.to_string(),
    }
}

/// Not enough space is an I/O failure that names both numbers; a missing
/// plan or a journal error is a job error (exit 1).
fn apply_failure(e: ApplyError, plan_target: Option<&Path>) -> Failure {
    match (&e, plan_target) {
        (ApplyError::NoSpace { needed, free }, Some(target)) => fail(format!(
            "not enough space in {}: {needed} bytes needed, {free} free; nothing was written",
            target.display()
        )),
        _ => fail(e),
    }
}

/// `FILE=SYSTEM_ID` split on the last `=` (names may contain `=`); a
/// missing `=` or an empty side is a usage error.
pub(crate) fn parse_assign(s: &str) -> Result<(PathBuf, String), Failure> {
    let bad = || {
        Failure::usage(format!(
            "--assign `{s}` must look like FILE=SYSTEM_ID (see `coverly systems`)"
        ))
    };
    let (file, system) = s.rsplit_once('=').ok_or_else(bad)?;
    let system = system.trim().to_ascii_lowercase();
    if file.is_empty() || system.is_empty() {
        return Err(bad());
    }
    Ok((PathBuf::from(file), system))
}

/// Decides which progress events get a terminal line: phase starts and
/// ends and every 25th file (the hash phase reports every file); for an
/// extraction, its first event, the one with every entry staged, and each
/// other change of its staged entries at most once a second.
#[derive(Default)]
struct ProgressGate {
    /// The extraction last printed: item, staged entries, when.
    last: Mutex<Option<(usize, usize, Instant)>>,
}

impl ProgressGate {
    fn due(&self, p: OrganizeProgress, now: Instant) -> bool {
        let OrganizePhase::Extract {
            item,
            entries_done,
            entries_total,
            ..
        } = p.phase
        else {
            return p.done == 0 || p.done == p.total || p.done.is_multiple_of(25);
        };
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        // A new item is its first event; all entries staged is its last
        // one (the core reports it twice: at the edge and after commit).
        // Byte ticks never print a line of their own, and a count already
        // printed is never printed again.
        let due = match *last {
            Some((i, e, at)) => {
                i != item
                    || (e != entries_done
                        && (entries_done == entries_total
                            || now.duration_since(at) >= Duration::from_secs(1)))
            }
            None => true,
        };
        if due {
            *last = Some((item, entries_done, now));
            return true;
        }
        false
    }
}

/// ScreenScraper is asked by CRC only when this build has it, the user
/// enabled it, their account is set and the run is not offline.
fn online_wanted(s: &Settings, keys: &SourceKeys, offline: bool) -> bool {
    !offline
        && keys.screenscraper_dev.is_some()
        && keys.screenscraper_user.is_some()
        && s.sources
            .order
            .iter()
            .any(|e| e.id == "screenscraper" && e.enabled)
}

/// `identify 40/631`, or `extract item 19/40 files 4/12` (plan item, then
/// the item's staged entries).
fn progress_text(p: OrganizeProgress) -> Option<String> {
    let phase = match p.phase {
        OrganizePhase::Walk => "walk",
        OrganizePhase::Identify => "identify",
        OrganizePhase::Dats => "catalogues",
        OrganizePhase::Hash => "hash",
        OrganizePhase::Apply => "apply",
        OrganizePhase::Extract {
            entries_done,
            entries_total,
            ..
        } => {
            return Some(format!(
                "extract item {}/{} files {entries_done}/{entries_total}",
                p.done + 1,
                p.total
            ));
        }
    };
    Some(if p.total == 0 {
        phase.to_string()
    } else {
        format!("{phase} {}/{}", p.done, p.total)
    })
}

pub fn run(args: &OrganizeArgs) -> Result<(), Failure> {
    check_folder(&args.source, "source")?;
    // Absolute, never canonical (no `\\?\` on Windows): journals keep
    // paths valid from any working directory.
    let source = std::path::absolute(&args.source)?;
    let target = args
        .target
        .as_deref()
        .map(std::path::absolute)
        .transpose()?;
    let cli = crate::settings::load_cli_settings();
    let s = &cli.settings;

    let mut opts = OrganizeOptions::from_settings(s, args.layout.as_deref());
    opts.target = target;
    if args.flat {
        opts.flat = true;
    }
    if args.copy {
        opts.mode = Some(TransferMode::Copy);
    } else if args.move_ {
        opts.mode = Some(TransferMode::Move);
    }
    if let Some(n) = args.names {
        opts.names = n.into();
    }
    if let Some(d) = args.duplicates {
        opts.duplicates = d.into();
    }
    opts.offline = args.offline;
    if args.no_external_tools {
        opts.limits.external_tools = false;
    }
    let profiles: Vec<Profile> = all_profiles(&cli.custom)
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    if !profiles.iter().any(|p| p.id == opts.layout) {
        return Err(Failure::usage(format!(
            "unknown layout `{}`; expected one of: {}",
            opts.layout,
            profile_ids(&cli.custom)
        )));
    }
    let db = SystemDb::builtin();
    let mut assigned: Vec<(PathBuf, String)> = Vec::with_capacity(args.assign.len());
    for a in &args.assign {
        let (file, system) = parse_assign(a)?;
        if db.get(&system).is_none() {
            return Err(Failure::usage(format!(
                "unknown system `{system}` in --assign (see `coverly systems`)"
            )));
        }
        assigned.push((file, system));
    }
    let data = crate::settings::data_dir(args.data_dir.as_deref(), s);
    let dirs = AppDirs::resolve(args.cache_dir.as_deref(), data.as_deref()).ok_or_else(|| {
        fail("cannot find the app cache/data folders; pass --cache-dir and --data-dir")
    })?;
    let tty = io::stderr().is_terminal();
    // Lines go to stderr, only when it is a terminal.
    let gate = ProgressGate::default();
    let progress = move |p: OrganizeProgress| {
        if tty && gate.due(p, Instant::now()) {
            if let Some(line) = progress_text(p) {
                eprintln!("{line}");
            }
        }
    };
    let cancel = AtomicBool::new(false);
    let trash = process_trash();
    // No tools: an extraction never looks for one (`None` finds them on
    // first need).
    let no_tools: &[FoundTool] = &[];
    let tools = (!opts.limits.external_tools).then_some(no_tools);
    if let Some(job) = &args.resume {
        let env = ApplyEnv {
            data_dir: &dirs.data,
            progress: &progress,
            cancel: &cancel,
            trash: &*trash,
            tools,
        };
        return resume(args, job, &source, &env, &db, s);
    }
    let apply = args.apply || (!args.dry_run && !s.safety.dry_run_default);

    std::fs::create_dir_all(&dirs.cache).map_err(|e| {
        fail(format!(
            "cannot create the cache folder {}: {e}",
            dirs.cache.display()
        ))
    })?;
    let cache = Cache::open(&dirs.cache.join(CACHE_FILE)).map_err(fail)?;
    let net = s.net_config();
    let http = PoliteHttp::new(UreqHttp::new(&net), &net);
    let base = args
        .database_url
        .clone()
        .unwrap_or_else(|| DATABASE_BASE.into());
    let store = DatStore::new(&http, &cache, base);
    let dats = (!opts.offline && opts.download_dats).then_some(&store);
    let keys = SourceKeys::from_env(|name| std::env::var(name).ok());
    let screenscraper = online_wanted(s, &keys, opts.offline)
        .then(|| {
            let dev = keys.screenscraper_dev.clone()?;
            let cfg = SsConfig {
                base: args
                    .screenscraper_url
                    .clone()
                    .unwrap_or_else(|| SCREENSCRAPER_BASE.into()),
                ..SsConfig::default()
            };
            Some(ScreenScraperSource::new(
                &http,
                &cache,
                dev,
                keys.screenscraper_user.clone(),
                cfg,
            ))
        })
        .flatten();
    let lookup = screenscraper
        .as_ref()
        .map(|source| ScreenScraperLookup::new(source, &db));
    let env = OrganizeEnv {
        db: &db,
        profiles: &profiles,
        cache: &cache,
        dats,
        probe: &OsMetaProbe,
        online: lookup
            .as_ref()
            .map(|l| l as &dyn coverly_core::organize::evidence::OnlineLookup),
        now: SystemTime::now(),
    };

    let found = survey(&source, &opts, &env, &progress, &cancel).map_err(organize_failure)?;
    let mut decisions = assign_decisions(&found, &assigned)?;
    let mut plan = plan_from_survey(&found, &opts, &decisions, &env).map_err(organize_failure)?;
    if args.include_review {
        let settled: Vec<usize> = decisions.iter().map(|d| d.item).collect();
        let extra: Vec<Decision> = plan
            .items
            .iter()
            .filter(|i| i.action == OrganizeAction::Review && !settled.contains(&i.id))
            .filter(|i| i.id < found.entries.len())
            .map(|i| Decision {
                item: i.id,
                choice: Choice::Accept,
            })
            .collect();
        if !extra.is_empty() {
            decisions.extend(extra);
            plan = plan_from_survey(&found, &opts, &decisions, &env).map_err(organize_failure)?;
        }
    }
    warn_ignored_assigns(&plan, &assigned, &decisions);
    if !apply {
        return emit(args, Mode::DryRun, &plan, None, &db, &opts);
    }
    std::fs::create_dir_all(&dirs.data).map_err(|e| {
        fail(format!(
            "cannot create the data folder {}: {e}",
            dirs.data.display()
        ))
    })?;
    let env = ApplyEnv {
        data_dir: &dirs.data,
        progress: &progress,
        cancel: &cancel,
        trash: &*trash,
        tools,
    };
    let report =
        apply_organize(&plan, &env).map_err(|e| apply_failure(e, Some(&plan.target_root)))?;
    finish(args, Mode::Apply, &plan, &report, &db, &opts)
}

/// `--resume <job>`: continues the job from its saved plan, which must
/// be a plan of this source.
fn resume(
    args: &OrganizeArgs,
    job: &str,
    source: &Path,
    env: &ApplyEnv,
    db: &SystemDb,
    settings: &Settings,
) -> Result<(), Failure> {
    let plan = load_plan(env.data_dir, job).map_err(|e| apply_failure(e, None))?;
    if plan.source_root != source {
        return Err(Failure::usage(format!(
            "job {job} organises {}, not {}",
            plan.source_root.display(),
            source.display()
        )));
    }
    let report = resume_organize(job, env).map_err(|e| apply_failure(e, None))?;
    let opts = OrganizeOptions::from_settings(settings, Some(&plan.layout));
    finish(args, Mode::Resume, &plan, &report, db, &opts)
}

/// Prints the report of an apply or resume; a paused job is exit 5 with
/// the command that continues it.
fn finish(
    args: &OrganizeArgs,
    mode: Mode,
    plan: &OrganizePlan,
    report: &OrganizeReport,
    db: &SystemDb,
    opts: &OrganizeOptions,
) -> Result<(), Failure> {
    emit(args, mode, plan, Some(report), db, opts)?;
    if report.paused.is_none() {
        return Ok(());
    }
    let kind = report.pause_reason.map(|k| k.as_str()).unwrap_or("volume");
    let message = match &report.job {
        Some(job) => format!(
            "paused: {kind}; resume with --resume {job} (or undo with `coverly undo --job {job}`)"
        ),
        None => format!("paused: {kind}; nothing was written. Run the same command later"),
    };
    Err(Failure { code: 5, message })
}

/// An `--assign` on an item that stays where it is (a duplicate, a BIOS…)
/// changes nothing: say so instead of dropping it silently.
fn warn_ignored_assigns(
    plan: &OrganizePlan,
    assigned: &[(PathBuf, String)],
    decisions: &[Decision],
) {
    for ((file, system), d) in assigned.iter().zip(decisions) {
        let Some(item) = plan.items.get(d.item) else {
            continue;
        };
        // Duplicates and collisions are settled after the decisions, so
        // the outcome is what tells.
        if item.action == OrganizeAction::Auto && item.system_id.as_deref() == Some(system) {
            continue;
        }
        let why = match &item.action {
            OrganizeAction::Left { reason } => {
                format!(": it stays where it is ({})", left_text(reason))
            }
            OrganizeAction::SetAside { keeper, .. } => {
                format!(": it is set aside as a copy of {}", slash(keeper))
            }
            _ => String::new(),
        };
        eprintln!(
            "{}",
            table::terminal_safe(&format!(
                "warning: --assign {} ignored{why}",
                file.display()
            ))
        );
    }
}

/// One [`Choice::Assign`] per `--assign`, matched against the primary
/// file of each survey entry (exactly, else ignoring case when that is
/// unambiguous).
fn assign_decisions(
    found: &Survey,
    assigned: &[(PathBuf, String)],
) -> Result<Vec<Decision>, Failure> {
    let mut out = Vec::with_capacity(assigned.len());
    for (file, system) in assigned {
        let wanted = found.source_root.join(file);
        let primary = |i: usize| found.entries[i].sources.first();
        let exact = (0..found.entries.len()).find(|&i| primary(i) == Some(&wanted));
        let item = exact.or_else(|| {
            let lower = wanted.to_string_lossy().to_lowercase();
            let mut hits = (0..found.entries.len()).filter(|&i| {
                primary(i).is_some_and(|p| p.to_string_lossy().to_lowercase() == lower)
            });
            match (hits.next(), hits.next()) {
                (Some(i), None) => Some(i),
                _ => None,
            }
        });
        let Some(item) = item else {
            return Err(Failure::usage(format!(
                "--assign: {} is not in the plan",
                file.display()
            )));
        };
        out.push(Decision {
            item,
            choice: Choice::Assign {
                system: system.clone(),
            },
        });
    }
    Ok(out)
}

/// Writes the report (table or JSON) to `--output` or stdout.
fn emit(
    args: &OrganizeArgs,
    mode: Mode,
    plan: &OrganizePlan,
    report: Option<&OrganizeReport>,
    db: &SystemDb,
    opts: &OrganizeOptions,
) -> Result<(), Failure> {
    let text = match args.report {
        ReportArg::Json => {
            let v = serde_json::json!({
                "mode": mode.name(),
                "plan": plan,
                "job": report,
            });
            let mut s = serde_json::to_string_pretty(&v).map_err(fail)?;
            s.push('\n');
            s
        }
        ReportArg::Table => {
            let mut text = render_table(plan, db, opts, report.is_none());
            if let Some(report) = report {
                text.push_str(&render_report(plan, report));
            }
            text
        }
    };
    match &args.output {
        Some(path) => write_file(path, &text),
        None => print_stdout(&text),
    }
}

/// `path` with `/` separators, whatever the platform.
fn slash(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(table::one_line(&s.to_string_lossy())),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn mode_name(mode: TransferMode) -> &'static str {
    match mode {
        TransferMode::Move => "move",
        TransferMode::Copy => "copy",
    }
}

fn confidence_label(item: &OrganizeItem, opts: &OrganizeOptions) -> &'static str {
    if item.method == Some(IdLevel::User) || item.confidence + 1e-4 >= opts.thresholds.auto {
        "High"
    } else if item.confidence + 1e-4 >= opts.thresholds.review {
        "Medium"
    } else {
        "—"
    }
}

/// One sentence for a piece of evidence (the core has no UI strings).
pub(crate) fn why_text(e: &Evidence) -> String {
    let system = &e.system;
    let detail = e.detail.trim();
    match e.kind {
        IdLevel::Hash => match detail.split_once(' ') {
            Some((crc, name)) => format!("CRC {crc} in the {system} catalogue: {name}"),
            None if detail.is_empty() => format!("found in the {system} catalogue"),
            None => format!("{detail} in the {system} catalogue"),
        },
        IdLevel::Header if detail.is_empty() => format!("the header is {system}"),
        IdLevel::Header => format!("the header is {system} ({detail})"),
        IdLevel::Extension => {
            let ext = detail.trim_start_matches('.').trim_end_matches('!');
            if detail.ends_with('!') {
                format!(".{ext} extension ({system}), but the contents do not look like it")
            } else {
                format!(".{ext} extension ({system})")
            }
        }
        IdLevel::Name if detail.is_empty() => format!("the name suggests {system}"),
        IdLevel::Name => format!("the name resembles {detail} ({system})"),
        IdLevel::Online => format!("found online: {detail} ({system})"),
        IdLevel::User => "Chosen by you".to_string(),
    }
}

/// Why `reason` keeps a file where it is, on one line: names read from
/// an archive or a cue sheet cannot start a forged plan line.
pub(crate) fn left_text(reason: &LeftReason) -> String {
    table::one_line(&left_reason_text(reason))
}

fn left_reason_text(reason: &LeftReason) -> String {
    match reason {
        LeftReason::PartialDownload => "download not finished".into(),
        LeftReason::RecentlyModified => "modified moments ago, may still be written".into(),
        LeftReason::CloudOnly => "in the cloud, not on this disk".into(),
        LeftReason::NotARom { what } => match what {
            NotARom::WebPage => "not a ROM (web page)".into(),
            NotARom::Text => "not a ROM (text)".into(),
            NotARom::Patch => "not a ROM (a patch: apply it to the original ROM)".into(),
            NotARom::Installer => "not a ROM (installer)".into(),
            NotARom::Model3d => "not a ROM (3D model)".into(),
        },
        LeftReason::Bios => "BIOS or firmware".into(),
        LeftReason::DamagedArchive => "damaged archive".into(),
        LeftReason::UnsupportedArchive { format, need } => match (format, need) {
            (Some(format), Some(tool)) => {
                format!("to open {format} archives, install {}", tool_name(*tool))
            }
            (_, Some(tool)) => format!("to open this archive, install {}", tool_name(*tool)),
            (Some(format), None) => format!("unsupported archive format ({format})"),
            (None, None) => "unsupported archive format".into(),
        },
        LeftReason::EncryptedArchive => "password protected".into(),
        LeftReason::ArchiveNeedsMemory { mib } => {
            format!("needs {mib} MB of memory to decompress")
        }
        LeftReason::ArchiveTooBig { .. } => "too large to open safely".into(),
        LeftReason::MultiVolumeArchive { parts } => {
            format!("multi-part archive ({parts} parts); join them first")
        }
        LeftReason::UnsafeArchiveEntry { name, .. } => {
            format!("contains an unsafe path ({name}); not extracted")
        }
        LeftReason::SetUnsafeName { name } => {
            format!("a game file has a name the card can't store: {name}")
        }
        LeftReason::PathTooLong => "the path would be too long for Windows".into(),
        LeftReason::NestedArchive => "archive inside another archive; extract it first".into(),
        LeftReason::RepackArcade => "arcade sets must be .zip; repack it".into(),
        LeftReason::FolderFull { folder } => {
            format!("the folder {folder} can't hold more files on FAT32")
        }
        LeftReason::UnsafeTarget => "the target folder is a link; nothing is written there".into(),
        LeftReason::MultiGameArchive { count } => format!("archive with {count} games"),
        LeftReason::UnsupportedSystem { name } => format!("{name} is not organised"),
        LeftReason::Unknown => "unknown system".into(),
        LeftReason::Duplicate { of } => format!("same as {}", slash(of)),
        LeftReason::AlreadyPresent { at } => format!("already in {}", slash(at)),
        LeftReason::InLibrary { path, relation } => format!(
            "already in your library as {} ({})",
            slash(path),
            relation_text(*relation)
        ),
        LeftReason::AlreadySorted { folder } => {
            format!("already in the {folder} system folder")
        }
        LeftReason::FolderGame { system, folder } => {
            format!("{system} game in its own folder: move the whole folder to {folder}")
        }
        LeftReason::Symlink => "link, not followed".into(),
        LeftReason::SetIncomplete { missing } => {
            format!("game files missing: {}", missing.join(", "))
        }
        LeftReason::SetOutsideFolder => "the game lists files outside its folder".into(),
        LeftReason::SharedTrack => "track used by several games".into(),
        LeftReason::AmbiguousCompanion => "matches several ROMs".into(),
        LeftReason::TooBigForFat => "larger than 4 GiB on a FAT32 target".into(),
        LeftReason::Unreadable => "cannot be read".into(),
        LeftReason::CollisionSet => "a file of the game is taken at the target".into(),
        LeftReason::KeyMaterial => "key or licence file: never read or moved".into(),
        LeftReason::EmulatorData { folder } => {
            format!("inside an emulator's data folder ({folder}): left as it is")
        }
    }
}

/// The program's own name: never translated.
fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Unrar => "unrar",
        Tool::SevenZip => "7-Zip",
        Tool::Bsdtar => "bsdtar",
    }
}

fn rename_text(r: &RenameReason) -> String {
    match r {
        RenameReason::Collision { name } => format!("renamed: {name} is taken"),
        RenameReason::SafeName { from } => {
            format!("renamed from {from}: characters the card cannot store")
        }
        RenameReason::PathLength => "renamed: the path was too long".into(),
        RenameReason::Catalogue { from } => format!("renamed from {from} to the catalogue name"),
    }
}

fn note_text(n: &ItemNote) -> Option<String> {
    Some(match n {
        ItemNote::MultiRomZip { count } => {
            format!("zip with {count} ROMs: most emulators load the first")
        }
        ItemNote::NoFolderOnDevice { folder } => {
            format!("the device has no {folder} folder yet")
        }
        ItemNote::ExtensionContradicted { ext, .. } => {
            format!(
                "the .{} extension says otherwise",
                ext.trim_start_matches('.')
            )
        }
        ItemNote::Encrypted => "encrypted archive: the header was not read".into(),
        ItemNote::ContainerMismatch { ext, actual } => format!(
            "it is a {actual} archive named .{}",
            ext.trim_start_matches('.')
        ),
        ItemNote::TwinSheets { .. } => {
            "has a .gdi and a .cue for the same tracks; some menus will list the game twice".into()
        }
        ItemNote::NoStoredChecksum => "this format stores no checksum; the size is checked".into(),
        ItemNote::ArchiveNotFullyRead => {
            "couldn't be read in full during the dry run; please review it".into()
        }
        ItemNote::ArchiveKeptLeftovers { count, .. } => {
            format!("the archive stays: it has {count} files that are not extracted")
        }
        ItemNote::SheetNotReadYet => "the .cue sheet is checked while extracting".into(),
        ItemNote::MultiRomExtract { count } => format!("contains {count} ROMs"),
        ItemNote::Update { of } => format!("update for {of}"),
        ItemNote::Dlc { of } => format!("downloadable content for {of}"),
        ItemNote::EncryptedContent { emulator } => {
            format!("encrypted copy: {emulator} only opens decrypted copies")
        }
        ItemNote::InstallFirst { emulator } => {
            format!("{emulator} installs it; it does not open from the folder")
        }
        ItemNote::FolderSuffix { name, layout } => {
            format!("renamed to {name} so {layout} shows it")
        }
        // Listed once in the warnings.
        ItemNote::CatalogueMissing { .. } => return None,
    })
}

fn warning_text(w: &PlanWarning) -> String {
    match w {
        PlanWarning::CatalogueMissing { system } => format!(
            "no {system} catalogue: decided by header, extension or name (run without --offline to download it)"
        ),
        PlanWarning::TargetLayoutMismatch { profile } => {
            format!("the target already looks like the {profile} layout (pass --layout {profile} to keep it)")
        }
        PlanWarning::MoveUnavailable => {
            "the source cannot be written: files will be copied, not moved".into()
        }
        PlanWarning::SortedOnly => {
            "nothing loose: every ROM already sits in a system folder".into()
        }
        PlanWarning::NoSpace { needed, free } => format!(
            "not enough space on the target: {needed} bytes needed, {free} bytes free"
        ),
    }
}

fn candidates_text(item: &OrganizeItem) -> String {
    let mut all: Vec<(String, f32)> = Vec::new();
    if let Some(s) = &item.system_id {
        all.push((s.clone(), item.confidence));
    }
    for (s, w) in &item.alternatives {
        if !all.iter().any(|(id, _)| id == s) {
            all.push((s.clone(), *w));
        }
    }
    all.iter()
        .map(|(s, w)| format!("{s} {w:.2}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The first source and, for sets, how many files travel with it.
fn source_label(item: &OrganizeItem) -> String {
    let first = item.sources.first().map(|p| slash(p)).unwrap_or_default();
    match item.sources.len() {
        0 | 1 => first,
        n => format!("{first} (+{} files)", n - 1),
    }
}

fn item_extras(item: &OrganizeItem) -> String {
    let mut extras: Vec<String> = Vec::new();
    if let Some(r) = &item.rename {
        extras.push(rename_text(r));
    }
    extras.extend(item.notes.iter().filter_map(note_text));
    if !item.companions.is_empty() {
        extras.push(format!("with {} companion file(s)", item.companions.len()));
    }
    if extras.is_empty() {
        String::new()
    } else {
        format!("  [{}]", extras.join("; "))
    }
}

/// The system folder an item lands in (`ROMS/PS`): its target's folder
/// cut after the component named like the item's folder (a per-game
/// subfolder stays out), else the target's folder.
fn system_dir(item: &OrganizeItem) -> String {
    let Some(parent) = item.target.as_deref().and_then(Path::parent) else {
        return String::new();
    };
    let Some(folder) = item.folder.as_deref() else {
        return slash(parent);
    };
    let mut prefix = PathBuf::new();
    for c in parent.components() {
        prefix.push(c);
        if c.as_os_str() == folder {
            return slash(&prefix);
        }
    }
    slash(parent)
}

/// `Game.gba → ROMS/GBA/Game.gba`, or for an extraction `Game.zip
/// extract 12 files 405.7 MB → ROMS/PS/` (the folder of its first file;
/// a generated playlist is not counted).
fn action_label(item: &OrganizeItem) -> String {
    match &item.transfer {
        Transfer::AsIs => format!(
            "{} → {}",
            source_label(item),
            item.target.as_deref().map(slash).unwrap_or_default()
        ),
        Transfer::Extract { entries, bytes, .. } => {
            let dir = item
                .target
                .as_deref()
                .and_then(Path::parent)
                .map(slash)
                .unwrap_or_default();
            let files = match entries.len() {
                1 => "1 file".to_string(),
                n => format!("{n} files"),
            };
            format!(
                "{}  extract {files} {} → {dir}/",
                source_label(item),
                table::human_bytes(*bytes)
            )
        }
    }
}

/// The quarantine folder of a plan with set-aside items: their `dest`
/// without the item's own path, relative to the source root (or absolute:
/// the library's quarantine, for a pile inside a system folder).
fn quarantine_of(plan: &OrganizePlan) -> PathBuf {
    plan.items
        .iter()
        .find_map(|i| match (&i.action, i.sources.first()) {
            (OrganizeAction::SetAside { dest, .. }, Some(primary)) if dest.ends_with(primary) => {
                let mut base = dest.clone();
                primary.components().all(|_| base.pop()).then_some(base)
            }
            _ => None,
        })
        .unwrap_or_else(|| PathBuf::from(coverly_core::dedupe::DEFAULT_QUARANTINE_NAME))
}

/// A set-aside path as the table shows it: relative ones with `/`,
/// absolute ones whole.
fn aside_path(path: &Path) -> String {
    if path.is_absolute() {
        table::one_line(&path.display().to_string())
    } else {
        slash(path)
    }
}

/// Games the plan extracts and their bytes, when it extracts any.
fn extract_totals(plan: &OrganizePlan) -> Option<(usize, u64)> {
    let (games, bytes) = plan
        .items
        .iter()
        .filter(|i| i.action == OrganizeAction::Auto)
        .filter_map(|i| match &i.transfer {
            Transfer::Extract { bytes, .. } => Some(*bytes),
            Transfer::AsIs => None,
        })
        .fold((0, 0u64), |(n, b), bytes| (n + 1, b.saturating_add(bytes)));
    (games > 0).then_some((games, bytes))
}

/// Says why an archive that needs a program was not opened although the
/// program may be installed: external programs are off.
fn tools_off_hint(reason: &LeftReason, opts: &OrganizeOptions) -> &'static str {
    match reason {
        LeftReason::UnsupportedArchive { need: Some(_), .. } if !opts.limits.external_tools => {
            " (external programs are off)"
        }
        _ => "",
    }
}

/// What happened to an applied plan: counts, skipped and failed items,
/// how to undo it and how to open the result.
fn render_report(plan: &OrganizePlan, report: &OrganizeReport) -> String {
    let mut out = String::new();
    let job = report
        .job
        .as_deref()
        .map(|j| format!(" (job {j})"))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "\nOrganised: moved {}, copied {}, skipped {}, failed {}{job}",
        report.moved, report.copied, report.skipped, report.failed
    );
    if report.extracted > 0 {
        let _ = writeln!(out, "Extracted: {}", report.extracted);
    }
    if report.set_aside > 0 {
        let place = if plan.set_aside_trash {
            "sent to the Trash".to_string()
        } else {
            format!(
                "in {}",
                plan.source_root.join(quarantine_of(plan)).display()
            )
        };
        let _ = writeln!(out, "Duplicates set aside: {} ({place})", report.set_aside);
    }
    let label = |o: &ItemOutcome| {
        plan.items
            .get(o.item)
            .map(source_label)
            .unwrap_or_else(|| format!("item {}", o.item))
    };
    for o in &report.items {
        let line = match &o.status {
            ItemStatus::Skipped { reason } => format!("skipped: {}", skip_text(*reason)),
            ItemStatus::Failed { kind, detail } => format!("failed: {} {detail}", kind.as_str()),
            ItemStatus::CopiedNotRemoved => {
                "copied, but the source could not be removed".to_string()
            }
            ItemStatus::ExtractedKept => "extracted; the archive stays where it was".to_string(),
            ItemStatus::Moved
            | ItemStatus::Copied
            | ItemStatus::Extracted
            | ItemStatus::Quarantined
            | ItemStatus::Trashed => continue,
        };
        let _ = writeln!(out, "  {}  {line}", label(o));
    }
    if let Some(j) = &report.job {
        if report.paused.is_some() {
            let _ = writeln!(
                out,
                "{} item(s) not done yet: continue with --resume {j}",
                report.resumable_items.len()
            );
        }
        let _ = writeln!(out, "Undo with: coverly undo --job {j}");
    }
    if report.paused.is_none() && report.moved + report.copied + report.extracted > 0 {
        let _ = writeln!(
            out,
            "Open it with: coverly fill {} --profile {}",
            shell_word(&plan.target_root.display().to_string()),
            plan.layout
        );
    }
    table::terminal_safe(&out)
}

/// `s` quoted for a POSIX shell when it holds anything but plain path
/// characters.
fn shell_word(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:,@".contains(c));
    if plain {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn skip_text(r: SkipReason) -> &'static str {
    match r {
        SkipReason::ChangedSincePlan => "changed since the plan",
        SkipReason::SourceMissing => "no longer there",
        SkipReason::AlreadyPresent => "the same file is already at the target",
        SkipReason::CollisionSet => "a file of the game is taken at the target",
        SkipReason::NotDecided => "to review: no system chosen",
        SkipReason::Cancelled => "cancelled",
        SkipReason::ToolGone => "the archive tool the plan used is gone",
        SkipReason::TrashUnavailable => "no Trash on its volume: it stays where it was",
    }
}

fn render_table(
    plan: &OrganizePlan,
    db: &SystemDb,
    opts: &OrganizeOptions,
    dry_run: bool,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Organise {} → {} ({}, {})",
        plan.source_root.display(),
        plan.target_root.display(),
        plan.layout,
        mode_name(plan.mode)
    );
    let c = &plan.counts;
    let _ = writeln!(
        out,
        "ROMs: {} · Systems: {} · To review: {} · Staying put: {}",
        c.roms, c.systems, c.review, c.left
    );
    if c.set_aside + c.in_library > 0 {
        let _ = writeln!(
            out,
            "Duplicates set aside: {} · Already in your library: {}",
            c.set_aside, c.in_library
        );
    }

    // Moving items, grouped by their system folder.
    let mut groups: BTreeMap<String, Vec<&OrganizeItem>> = BTreeMap::new();
    for item in &plan.items {
        if item.action != OrganizeAction::Auto {
            continue;
        }
        groups.entry(system_dir(item)).or_default().push(item);
    }
    for (folder, items) in &groups {
        let system = items
            .first()
            .and_then(|i| i.system_id.as_deref())
            .and_then(|id| db.get(id))
            .map(|s| format!("  {}", s.name))
            .unwrap_or_default();
        let _ = writeln!(out, "\n{folder}/  ({}){system}", items.len());
        for item in items {
            let why = item.why.first().map(why_text).unwrap_or_default();
            let _ = writeln!(
                out,
                "  {}  {}  {why}{}",
                action_label(item),
                confidence_label(item, opts),
                item_extras(item)
            );
        }
    }

    let review: Vec<&OrganizeItem> = plan
        .items
        .iter()
        .filter(|i| i.action == OrganizeAction::Review)
        .collect();
    if !review.is_empty() {
        let _ = writeln!(
            out,
            "\nTo review ({}): settle with --assign FILE=SYSTEM_ID or --include-review",
            review.len()
        );
        for item in review {
            let why = item
                .why
                .first()
                .map(|e| format!("  {}", why_text(e)))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "  {}  candidates {}{why}",
                source_label(item),
                candidates_text(item)
            );
        }
    }

    let set_aside: Vec<&OrganizeItem> = plan.items.iter().filter(|i| i.is_set_aside()).collect();
    if !set_aside.is_empty() {
        let place = if plan.set_aside_trash {
            "to the Trash".to_string()
        } else {
            format!("to {}/", aside_path(&quarantine_of(plan)))
        };
        let _ = writeln!(out, "\nDuplicates set aside ({}), {place}", set_aside.len());
        for item in set_aside {
            if let OrganizeAction::SetAside {
                keeper,
                relation,
                reason,
                dest,
            } = &item.action
            {
                let to = if plan.set_aside_trash {
                    "Trash".to_string()
                } else {
                    aside_path(dest)
                };
                let _ = writeln!(
                    out,
                    "  {}  {}  → {to}  [stays: {}, {}]",
                    source_label(item),
                    relation_text(*relation),
                    slash(keeper),
                    table::one_line(&reason_text(reason))
                );
            }
        }
    }

    let left: Vec<(&OrganizeItem, &LeftReason)> = plan
        .items
        .iter()
        .filter_map(|i| match &i.action {
            OrganizeAction::Left { reason } => Some((i, reason)),
            _ => None,
        })
        .collect();
    if !left.is_empty() {
        let _ = writeln!(out, "\nStaying where they are ({})", left.len());
        for (item, reason) in left {
            let _ = writeln!(
                out,
                "  {}  {}{}",
                source_label(item),
                left_text(reason),
                tools_off_hint(reason, opts)
            );
        }
    }
    if !plan.sorted_folders.is_empty() {
        let _ = writeln!(out, "\nAlready sorted by system");
        for (folder, system) in &plan.sorted_folders {
            let _ = writeln!(out, "  {}/  {system}", slash(folder));
        }
    }
    if c.other_files > 0 {
        let _ = writeln!(out, "\nOther files: {}", c.other_files);
    }
    if !plan.deep_folders.is_empty() {
        let _ = writeln!(out, "\nToo deep, not looked into");
        for folder in &plan.deep_folders {
            let _ = writeln!(out, "  {}/", slash(folder));
        }
    }
    if !plan.warnings.is_empty() {
        out.push('\n');
        for w in &plan.warnings {
            let _ = writeln!(out, "warning: {}", warning_text(w));
        }
    }
    if let Some((games, bytes)) = extract_totals(plan) {
        let _ = writeln!(
            out,
            "\nExtracting: {games} {} ({})",
            if games == 1 { "game" } else { "games" },
            table::human_bytes(bytes)
        );
    }
    if dry_run {
        let _ = writeln!(
            out,
            "\nDry run: nothing was written. Re-run with --apply to organise {} files.",
            c.auto
        );
    }
    table::terminal_safe(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_archives_and_paths_stay_on_one_line() {
        use coverly_core::archive::names::UnsafeWhy;
        let forged = "a\nROMS/PS/  (1)  forged\r";
        let reasons = [
            LeftReason::UnsafeArchiveEntry {
                name: forged.into(),
                why: UnsafeWhy::Illegal,
            },
            LeftReason::SetUnsafeName {
                name: forged.into(),
            },
            LeftReason::SetIncomplete {
                missing: vec![forged.into()],
            },
            LeftReason::FolderFull {
                folder: forged.into(),
            },
            LeftReason::UnsupportedSystem {
                name: forged.into(),
            },
            LeftReason::AlreadySorted {
                folder: forged.into(),
            },
            LeftReason::FolderGame {
                system: "doom".into(),
                folder: forged.into(),
            },
        ];
        for r in &reasons {
            let text = left_text(r);
            assert!(!text.contains('\n') && !text.contains('\r'), "{text:?}");
        }
        let path = PathBuf::from("ROMS").join(forged);
        assert!(!slash(&path).contains('\n'), "{:?}", slash(&path));
    }

    #[test]
    fn assign_splits_on_the_last_equals() {
        let (file, system) = parse_assign("a=b.gb=GBC").unwrap();
        assert_eq!(file, PathBuf::from("a=b.gb"));
        assert_eq!(system, "gbc");
    }

    #[test]
    fn assign_without_equals_or_a_side_is_usage() {
        for bad in ["Odd.gb", "=gb", "Odd.gb=", "Odd.gb= "] {
            assert_eq!(parse_assign(bad).unwrap_err().code, 2, "{bad}");
        }
    }

    #[test]
    fn why_texts_read_as_sentences() {
        let hash = Evidence::new(IdLevel::Hash, "gba", 1.0, "1234ABCD Game (USA)");
        assert_eq!(
            why_text(&hash),
            "CRC 1234ABCD in the gba catalogue: Game (USA)"
        );
        let ext = Evidence::new(IdLevel::Extension, "gb", 0.6, ".gb!");
        assert!(why_text(&ext).starts_with(".gb extension (gb), but"));
        let user = Evidence::new(IdLevel::User, "gbc", 1.0, "");
        assert_eq!(why_text(&user), "Chosen by you");
    }

    #[test]
    fn progress_is_printed_sparingly() {
        let at = |phase, done, total| OrganizeProgress { phase, done, total };
        let due = |p| ProgressGate::default().due(p, Instant::now());
        assert!(due(at(OrganizePhase::Walk, 0, 0)));
        assert!(due(at(OrganizePhase::Hash, 0, 500)));
        assert!(!due(at(OrganizePhase::Hash, 7, 500)));
        assert!(due(at(OrganizePhase::Hash, 25, 500)));
        assert!(due(at(OrganizePhase::Hash, 500, 500)));
    }

    #[test]
    fn progress_text_names_the_item_and_its_files() {
        let p = OrganizeProgress {
            phase: OrganizePhase::Extract {
                item: 3,
                bytes_done: 5,
                bytes_total: 10,
                entries_done: 4,
                entries_total: 12,
            },
            done: 18,
            total: 40,
        };
        assert_eq!(
            progress_text(p).as_deref(),
            Some("extract item 19/40 files 4/12")
        );
        let walk = OrganizeProgress {
            phase: OrganizePhase::Walk,
            done: 0,
            total: 0,
        };
        assert_eq!(progress_text(walk).as_deref(), Some("walk"));
        let hash = OrganizeProgress {
            phase: OrganizePhase::Hash,
            done: 40,
            total: 631,
        };
        assert_eq!(progress_text(hash).as_deref(), Some("hash 40/631"));
    }

    #[test]
    fn extract_lines_follow_entries_not_bytes() {
        let gate = ProgressGate::default();
        let t0 = Instant::now();
        let x = |b, e| OrganizeProgress {
            phase: OrganizePhase::Extract {
                item: 1,
                bytes_done: b,
                bytes_total: 100,
                entries_done: e,
                entries_total: 3,
            },
            done: 0,
            total: 1,
        };
        assert!(gate.due(x(0, 0), t0), "first event");
        assert!(
            !gate.due(x(10, 0), t0 + Duration::from_millis(100)),
            "byte tick"
        );
        assert!(
            gate.due(x(20, 1), t0 + Duration::from_millis(1100)),
            "entry change"
        );
        assert!(
            !gate.due(x(30, 2), t0 + Duration::from_millis(1200)),
            "within a second"
        );
        assert!(
            gate.due(x(100, 3), t0 + Duration::from_millis(1300)),
            "last event"
        );
    }

    /// The core's real tail: the last chunk of the last entry (all bytes,
    /// one entry short), the all-staged edge, then the post-commit report.
    /// Only the all-staged line is new; the others repeat or skip the 1 s.
    #[test]
    fn extract_tail_prints_each_count_once() {
        let gate = ProgressGate::default();
        let t0 = Instant::now();
        let x = |item, b, e| OrganizeProgress {
            phase: OrganizePhase::Extract {
                item,
                bytes_done: b,
                bytes_total: 100,
                entries_done: e,
                entries_total: 3,
            },
            done: 0,
            total: 2,
        };
        assert!(gate.due(x(1, 0, 0), t0), "first event");
        assert!(gate.due(x(1, 60, 2), t0 + Duration::from_millis(1100)));
        assert!(
            !gate.due(x(1, 100, 2), t0 + Duration::from_millis(1150)),
            "last chunk, same count, within a second"
        );
        assert!(
            gate.due(x(1, 100, 3), t0 + Duration::from_millis(1160)),
            "all staged"
        );
        assert!(
            !gate.due(x(1, 100, 3), t0 + Duration::from_millis(1400)),
            "post-commit report repeats the line"
        );
        assert!(
            gate.due(x(2, 0, 0), t0 + Duration::from_millis(1500)),
            "next item"
        );
    }

    #[test]
    fn online_lookup_needs_the_source_enabled_an_account_and_the_network() {
        use coverly_core::credentials::{DevCredentials, Secret, SourceKeys, UserCredentials};
        let secret = || Secret::new("x");
        let keys = SourceKeys {
            steamgriddb: None,
            screenscraper_user: Some(UserCredentials {
                user: secret(),
                password: secret(),
            }),
            screenscraper_dev: Some(DevCredentials {
                id: secret(),
                password: secret(),
            }),
        };
        let mut s = coverly_core::settings::Settings::default();
        // Off out of the box.
        assert!(!online_wanted(&s, &keys, false));
        for e in &mut s.sources.order {
            if e.id == "screenscraper" {
                e.enabled = true;
            }
        }
        assert!(online_wanted(&s, &keys, false));
        assert!(!online_wanted(&s, &keys, true));
        let no_user = SourceKeys {
            screenscraper_user: None,
            ..keys.clone()
        };
        assert!(!online_wanted(&s, &no_user, false));
        let no_dev = SourceKeys {
            screenscraper_dev: None,
            ..keys
        };
        assert!(!online_wanted(&s, &no_dev, false));
    }

    #[test]
    fn shell_words_are_quoted_only_when_needed() {
        assert_eq!(shell_word("/Volumes/SD"), "/Volumes/SD");
        assert_eq!(shell_word("/tmp/my roms"), "'/tmp/my roms'");
        assert_eq!(shell_word("/tmp/it's"), "'/tmp/it'\\''s'");
    }

    #[test]
    fn archive_reasons_and_notes_follow_the_spec_copy() {
        let unsupported = |format: Option<&str>, need| LeftReason::UnsupportedArchive {
            format: format.map(str::to_string),
            need,
        };
        assert_eq!(
            left_text(&unsupported(Some("RAR"), Some(Tool::SevenZip))),
            "to open RAR archives, install 7-Zip"
        );
        assert_eq!(
            left_text(&unsupported(None, Some(Tool::Unrar))),
            "to open this archive, install unrar"
        );
        assert_eq!(
            left_text(&unsupported(Some("lzh"), None)),
            "unsupported archive format (lzh)"
        );
        assert_eq!(
            left_text(&unsupported(None, None)),
            "unsupported archive format"
        );
        assert_eq!(
            left_text(&LeftReason::ArchiveNeedsMemory { mib: 1536 }),
            "needs 1536 MB of memory to decompress"
        );
        assert_eq!(
            left_text(&LeftReason::MultiVolumeArchive { parts: 3 }),
            "multi-part archive (3 parts); join them first"
        );
        assert_eq!(
            left_text(&LeftReason::FolderGame {
                system: "doom".into(),
                folder: "prboom".into(),
            }),
            "doom game in its own folder: move the whole folder to prboom"
        );
        for ext in ["zip", ".zip"] {
            let note = ItemNote::ContainerMismatch {
                ext: ext.into(),
                actual: "7z".into(),
            };
            assert_eq!(
                note_text(&note).as_deref(),
                Some("it is a 7z archive named .zip")
            );
        }
        let kept = ItemNote::ArchiveKeptLeftovers {
            count: 2,
            examples: vec!["Game.sav".into()],
        };
        assert_eq!(
            note_text(&kept).as_deref(),
            Some("the archive stays: it has 2 files that are not extracted")
        );
    }

    #[test]
    fn slash_joins_with_forward_slashes() {
        assert_eq!(slash(Path::new("ROMS").join("GBA").as_path()), "ROMS/GBA");
    }
    #[test]
    fn left_text_new_reasons() {
        let cases = [
            (
                LeftReason::KeyMaterial,
                "key or licence file: never read or moved",
            ),
            (
                LeftReason::EmulatorData {
                    folder: "ux0".into(),
                },
                "inside an emulator's data folder (ux0): left as it is",
            ),
            (
                LeftReason::NotARom {
                    what: NotARom::Installer,
                },
                "not a ROM (installer)",
            ),
            (
                LeftReason::NotARom {
                    what: NotARom::Model3d,
                },
                "not a ROM (3D model)",
            ),
        ];
        for (reason, text) in cases {
            assert_eq!(left_text(&reason), text);
        }
    }

    #[test]
    fn note_text_new_notes() {
        let cases = [
            (
                ItemNote::Update {
                    of: "Mario Kart 8".into(),
                },
                "update for Mario Kart 8",
            ),
            (
                ItemNote::Dlc {
                    of: "0100152000022000".into(),
                },
                "downloadable content for 0100152000022000",
            ),
            (
                ItemNote::EncryptedContent {
                    emulator: "Azahar".into(),
                },
                "encrypted copy: Azahar only opens decrypted copies",
            ),
            (
                ItemNote::InstallFirst {
                    emulator: "RPCS3".into(),
                },
                "RPCS3 installs it; it does not open from the folder",
            ),
            (
                ItemNote::FolderSuffix {
                    name: "Game.ps3".into(),
                    layout: "ES-DE".into(),
                },
                "renamed to Game.ps3 so ES-DE shows it",
            ),
        ];
        for (note, text) in cases {
            assert_eq!(note_text(&note).as_deref(), Some(text));
        }
    }
}
