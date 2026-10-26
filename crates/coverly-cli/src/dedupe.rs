//! `coverly dedupe`: find the same game stored more than once inside one
//! system and show which copy stays and where the others would go
//! (duplicates spec §14). A dry run unless `--apply`, whatever the "dry
//! run by default" setting says: then the extra copies are set aside in
//! the quarantine (`_Duplicados/` inside the library, or `--quarantine
//! DIR`) or sent to the Trash (`--trash`), journaled so `coverly undo`
//! puts them back. A paused job (exit 5) continues with `--resume JOB`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clap::{Args, ValueEnum};
use coverly_core::archive::{Limits, ToolsOnDemand};
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::dedupe::apply::{
    apply_dedupe, load_dedupe_plan, resume_dedupe, DedupeApplyEnv, DedupeApplyNote,
    DedupeItemStatus, DedupeReport, DedupeSkip, DestLimit,
};
use coverly_core::dedupe::archive::ArchiveEntryListing;
use coverly_core::dedupe::disc::ReaderDiscProbe;
use coverly_core::dedupe::keys::DiskBytes;
use coverly_core::dedupe::DedupeEnv;
use coverly_core::dedupe::{
    plan_dedupe, DedupeDecisions, DedupeError, DedupeOptions, DedupePhase, DedupePlan,
    DedupeProgress, DupGroup, GroupAction, KeepReason, Member, QuarantineChoice, Relation,
    ReportWhy, TravelKind,
};
use coverly_core::identify::dat::{DatStore, DATABASE_BASE};
use coverly_core::net::{PoliteHttp, UreqHttp};
use coverly_core::organize::apply::ApplyError;
use coverly_core::paths::AppDirs;
use coverly_core::scan::scan_library;
use coverly_core::settings::{DedupeLevel, ExternalTools, KeepRule, RuleToggle};
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::process_trash;

use crate::profile::{check_root, choose_profile};
use crate::{print_stdout, table, Failure};

#[derive(Debug, Args)]
pub struct DedupeArgs {
    /// Library root (e.g. the SD card), the folder holding `ROMS`, `Roms`…
    root: PathBuf,
    /// Layout profile id (see `coverly profiles`), or `auto` to detect it
    /// (default: the `default_profile` setting, else `auto`).
    #[arg(long, value_name = "ID|auto")]
    profile: Option<String>,
    /// Only these systems: ids or folder names, comma-separated (see
    /// `coverly systems`).
    #[arg(long, value_name = "SYSTEMS", value_delimiter = ',')]
    system: Vec<String>,
    /// Levels to look for, comma-separated (default: the duplicates
    /// setting). `version` acts only with --one-game-one-rom.
    #[arg(long, value_enum, value_delimiter = ',')]
    level: Vec<LevelArg>,
    /// Also set aside other versions of a game (1G1R).
    #[arg(long)]
    one_game_one_rom: bool,
    /// Keeper rules in the order they are weighed, comma-separated; the
    /// rules not listed are off (has_save, played, in_library, verified,
    /// retail, region, revision, container, cover, catalogue_name).
    #[arg(long, value_name = "RULES", value_delimiter = ',')]
    keep_rules: Vec<String>,
    /// This copy stays, whatever the rules say. Repeatable.
    #[arg(long = "keep", value_name = "PATH")]
    keep: Vec<PathBuf>,
    /// Forget the keepers you chose in earlier runs.
    #[arg(long)]
    reset_choices: bool,
    /// Cached catalogues only: no download.
    #[arg(long)]
    offline: bool,
    /// Never run unrar, 7-Zip or bsdtar: RAR archives (and 7z coders the
    /// built-in reader lacks) are not read inside.
    #[arg(long)]
    no_external_tools: bool,
    /// Show the plan only (the default, whatever the "dry run by default"
    /// setting says).
    #[arg(long, conflicts_with = "apply")]
    dry_run: bool,
    /// Set the extra copies aside: into the quarantine (`_Duplicados/`
    /// inside the library unless --quarantine), or the Trash with --trash.
    /// Undo with `coverly undo`.
    #[arg(long)]
    apply: bool,
    /// Set the extra copies aside in this folder instead (anywhere but
    /// inside a system folder).
    #[arg(long, value_name = "DIR", conflicts_with = "trash")]
    quarantine: Option<PathBuf>,
    /// Send the extra copies to the Trash instead of the quarantine.
    #[arg(long)]
    trash: bool,
    /// Continue a paused or interrupted duplicates job from its saved plan
    /// (the options that shape a plan are refused).
    #[arg(
        long,
        value_name = "JOB_ID",
        conflicts_with_all = [
            "dry_run", "quarantine", "trash", "system", "level", "one_game_one_rom",
            "keep_rules", "keep", "reset_choices",
        ]
    )]
    resume: Option<String>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = ReportArg::Table)]
    report: ReportArg,
    /// Write the report to this file instead of stdout.
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,
    /// Data folder (job journals; default: the backup location setting,
    /// else the app data folder).
    #[arg(long, value_name = "DIR")]
    data_dir: Option<PathBuf>,
    /// Cache folder (catalogues, content keys, remembered choices).
    #[arg(long, value_name = "DIR")]
    cache_dir: Option<PathBuf>,
    #[arg(long, hide = true)]
    database_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum LevelArg {
    Exact,
    Format,
    Version,
}

impl From<LevelArg> for DedupeLevel {
    fn from(l: LevelArg) -> DedupeLevel {
        match l {
            LevelArg::Exact => DedupeLevel::Exact,
            LevelArg::Format => DedupeLevel::Format,
            LevelArg::Version => DedupeLevel::Version,
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

/// The rule spelled `name` (`has_save`, `cover`…), as in the settings.
fn parse_rule(name: &str) -> Option<KeepRule> {
    let name = name.trim().to_ascii_lowercase().replace('-', "_");
    serde_json::from_value(serde_json::Value::String(name)).ok()
}

fn rule_names() -> String {
    KeepRule::ALL
        .iter()
        .filter_map(|r| serde_json::to_value(r).ok())
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `--keep-rules`: the listed rules on, in order, then the others off.
fn keep_rules(names: &[String]) -> Result<Vec<RuleToggle>, Failure> {
    let mut out: Vec<RuleToggle> = Vec::with_capacity(KeepRule::ALL.len());
    for name in names.iter().filter(|n| !n.trim().is_empty()) {
        let rule = parse_rule(name).ok_or_else(|| {
            Failure::usage(format!(
                "unknown keep rule `{name}`; expected some of: {}",
                rule_names()
            ))
        })?;
        if out.iter().any(|t| t.rule == rule) {
            return Err(Failure::usage(format!(
                "keep rule `{name}` is listed twice"
            )));
        }
        out.push(RuleToggle { rule, on: true });
    }
    for rule in KeepRule::ALL {
        if !out.iter().any(|t| t.rule == rule) {
            out.push(RuleToggle { rule, on: false });
        }
    }
    Ok(out)
}

/// `--system`: system ids, or folder names the system list knows.
fn system_ids(names: &[String], db: &SystemDb) -> Result<Option<Vec<String>>, Failure> {
    if names.is_empty() {
        return Ok(None);
    }
    let mut ids = Vec::new();
    for name in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        let lower = name.to_ascii_lowercase();
        let id = db
            .get(&lower)
            .or_else(|| db.resolve_folder(name))
            .map(|s| s.id.clone())
            .ok_or_else(|| {
                Failure::usage(format!(
                    "unknown system `{name}` in --system (see `coverly systems`)"
                ))
            })?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(Some(ids))
}

/// A `--keep` path as the plan spells paths: absolute, tried from the
/// working directory, then the library root, then its ROMs folder.
fn keep_path(path: &Path, root: &Path, roms_root: &str) -> io::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let from_cwd = std::path::absolute(path)?;
    let candidates = [
        from_cwd.clone(),
        root.join(path),
        root.join(roms_root).join(path),
    ];
    Ok(candidates
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or(from_cwd))
}

/// Prints `fingerprints 40/631` lines to stderr, only when it is a
/// terminal: phase starts, ends and every 25th step.
fn progress_line(p: &DedupeProgress) {
    if !(p.done == 0 || p.done == p.total || p.done.is_multiple_of(25)) {
        return;
    }
    let phase = match p.phase {
        DedupePhase::Listing => "listing",
        DedupePhase::Sizes => "sizes",
        DedupePhase::Fingerprints => "fingerprints",
        DedupePhase::Serials => "serials",
        DedupePhase::Catalogues => "catalogues",
        _ => "planning",
    };
    if p.total == 0 {
        eprintln!("{phase}");
    } else {
        eprintln!("{phase} {}/{}", p.done, p.total);
    }
}

/// How the report was made.
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

/// The app folders, created, and the cache opened in them.
fn open_dirs(
    args: &DedupeArgs,
    s: &coverly_core::settings::Settings,
) -> Result<(AppDirs, Cache), Failure> {
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
    Ok((dirs, cache))
}

/// A quarantine inside a system folder is bad usage (exit 2); anything
/// else the planner refuses is an error (exit 1).
fn plan_failure(e: DedupeError) -> Failure {
    match e {
        DedupeError::QuarantineInSystemFolder(_) => Failure::usage(e.to_string()),
        e => fail(e),
    }
}

/// Not enough space names both numbers; a missing plan or a journal
/// error is a job error (exit 1).
fn apply_failure(e: ApplyError, plan: Option<&DedupePlan>) -> Failure {
    match (&e, plan) {
        (ApplyError::NoSpace { needed, free }, Some(plan)) => fail(format!(
            "not enough space in {}: {needed} bytes needed, {free} free; nothing was written",
            plan.quarantine.display()
        )),
        (ApplyError::Dedupe(DedupeError::QuarantineInSystemFolder(_)), _) => {
            Failure::usage(e.to_string())
        }
        _ => fail(e),
    }
}

/// Prints `setting aside 3/10` lines to stderr, only when it is a
/// terminal: the start, the end and every 25th copy.
fn apply_progress_line(done: usize, total: usize) {
    if done == 0 || done == total || done.is_multiple_of(25) {
        eprintln!("setting aside {done}/{total}");
    }
}

pub fn run(args: &DedupeArgs) -> Result<(), Failure> {
    check_root(&args.root)?;
    // Absolute, never canonical: the plan's paths match what you typed.
    let root = std::path::absolute(&args.root)?;
    let cli = crate::settings::load_cli_settings();
    let s = &cli.settings;
    let db = SystemDb::builtin();
    if let Some(job) = &args.resume {
        return resume(args, job, &root, s, &db);
    }

    let mut opts = DedupeOptions::from_settings(&s.dedupe);
    if !args.level.is_empty() {
        let mut levels: Vec<DedupeLevel> = Vec::new();
        for l in args.level.iter().map(|l| DedupeLevel::from(*l)) {
            if !levels.contains(&l) {
                levels.push(l);
            }
        }
        opts.levels = levels;
    }
    if args.one_game_one_rom {
        opts.one_game_one_rom = true;
    }
    if !args.keep_rules.is_empty() {
        opts.rules = keep_rules(&args.keep_rules)?;
    }
    opts.systems = system_ids(&args.system, &db)?;
    opts.offline = args.offline;
    opts.reset_choices = args.reset_choices;
    if args.trash {
        opts.quarantine = QuarantineChoice::Trash;
    } else if let Some(dir) = &args.quarantine {
        opts.quarantine = QuarantineChoice::Folder {
            path: std::path::absolute(dir)?,
        };
    }

    let mut chosen = choose_profile(&root, args.profile.as_deref(), &cli)?;
    chosen.profile = s.art_profile(&chosen.profile);
    let profile = &chosen.profile;
    opts.forced_keepers = args
        .keep
        .iter()
        .map(|p| keep_path(p, &root, &profile.roms_root))
        .collect::<io::Result<_>>()?;

    let (dirs, cache) = open_dirs(args, s)?;
    let net = s.net_config();
    let http = PoliteHttp::new(UreqHttp::new(&net), &net);
    let base = args
        .database_url
        .clone()
        .unwrap_or_else(|| DATABASE_BASE.into());
    let store = DatStore::new(&http, &cache, base);
    let dats = (!opts.offline).then_some(&store);

    let tty = io::stderr().is_terminal();
    let progress = move |p: DedupeProgress| {
        if tty {
            progress_line(&p);
        }
    };
    let cancel = AtomicBool::new(false);
    let scan = scan_library(&root, profile, &db, &s.scan_options());
    for w in &scan.warnings {
        eprintln!(
            "{}",
            table::terminal_safe(&format!("warning: {}: {}", w.path.display(), w.message))
        );
    }
    // External tools are found on first need, never inside the library
    // (which holds the default quarantine) or a custom quarantine.
    let limits = Limits {
        external_tools: !args.no_external_tools && s.organize.external_tools == ExternalTools::Auto,
        ..Limits::default()
    };
    let mut avoid: Vec<&Path> = vec![&root];
    if let QuarantineChoice::Folder { path } = &opts.quarantine {
        avoid.push(path);
    }
    // One discovery for the entry listing and the disc probe.
    let tools = Arc::new(ToolsOnDemand::new(&avoid));
    let listing = ArchiveEntryListing::with_tools_on_demand(limits.clone(), Arc::clone(&tools));
    let discs = ReaderDiscProbe::new(ArchiveEntryListing::with_tools_on_demand(limits, tools));
    let env = DedupeEnv {
        cache: &cache,
        db: &db,
        profile,
        settings: s,
        listing: &listing,
        discs: &discs,
        dats,
        bytes: &DiskBytes,
        progress: &progress,
        cancel: &cancel,
    };
    let plan = plan_dedupe(&scan, &opts, &env).map_err(plan_failure)?;
    for path in &opts.forced_keepers {
        let found = plan
            .groups
            .iter()
            .flat_map(|g| &g.members)
            .any(|m| m.unit.primary == *path || m.unit.files.contains(path));
        if !found {
            eprintln!(
                "{}",
                table::terminal_safe(&format!(
                    "warning: --keep {} is in no duplicates group",
                    path.display()
                ))
            );
        }
    }

    if !args.apply {
        return emit(args, Mode::DryRun, &plan, None, &db);
    }
    std::fs::create_dir_all(&dirs.data).map_err(|e| {
        fail(format!(
            "cannot create the data folder {}: {e}",
            dirs.data.display()
        ))
    })?;
    let trash = process_trash();
    let apply_progress = move |done: usize, total: usize| {
        if tty {
            apply_progress_line(done, total);
        }
    };
    let env = DedupeApplyEnv {
        data_dir: &dirs.data,
        trash: &*trash,
        cache: &cache,
        progress: &apply_progress,
        cancel: &cancel,
    };
    // `--keep` is already in the plan: no other decision to pass.
    let report = apply_dedupe(&plan, &DedupeDecisions::default(), &env)
        .map_err(|e| apply_failure(e, Some(&plan)))?;
    finish(args, Mode::Apply, &plan, &report, &db)
}

/// `--resume <job>`: continues the job from its saved plan, which must be
/// a plan of this library.
fn resume(
    args: &DedupeArgs,
    job: &str,
    root: &Path,
    s: &coverly_core::settings::Settings,
    db: &SystemDb,
) -> Result<(), Failure> {
    let (dirs, cache) = open_dirs(args, s)?;
    let plan = load_dedupe_plan(&dirs.data, job).map_err(|e| apply_failure(e, None))?;
    if plan.root != root {
        return Err(Failure::usage(format!(
            "job {job} sets aside copies of {}, not {}",
            plan.root.display(),
            root.display()
        )));
    }
    let tty = io::stderr().is_terminal();
    let progress = move |done: usize, total: usize| {
        if tty {
            apply_progress_line(done, total);
        }
    };
    let cancel = AtomicBool::new(false);
    let trash = process_trash();
    let env = DedupeApplyEnv {
        data_dir: &dirs.data,
        trash: &*trash,
        cache: &cache,
        progress: &progress,
        cancel: &cancel,
    };
    let report = resume_dedupe(job, &env).map_err(|e| apply_failure(e, Some(&plan)))?;
    finish(args, Mode::Resume, &plan, &report, db)
}

/// Prints the report of an apply or resume; a paused job is exit 5 with
/// the command that continues it.
fn finish(
    args: &DedupeArgs,
    mode: Mode,
    plan: &DedupePlan,
    report: &DedupeReport,
    db: &SystemDb,
) -> Result<(), Failure> {
    emit(args, mode, plan, Some(report), db)?;
    let Some(kind) = report.paused else {
        return Ok(());
    };
    let message = match &report.job {
        Some(job) => format!(
            "paused: {}; resume with --resume {job} (or undo with `coverly undo --job {job}`)",
            kind.as_str()
        ),
        None => format!(
            "paused: {}; nothing was written. Run the same command later",
            kind.as_str()
        ),
    };
    Err(Failure { code: 5, message })
}

/// The dry run prints the plan (JSON: the plan itself, `version: 1`); an
/// apply or resume adds what it did (JSON: `{ mode, plan, job }`).
fn emit(
    args: &DedupeArgs,
    mode: Mode,
    plan: &DedupePlan,
    report: Option<&DedupeReport>,
    db: &SystemDb,
) -> Result<(), Failure> {
    let text = match (args.report, report) {
        (ReportArg::Table, report) => {
            let mut text = render_table(plan, db, report.is_none());
            if let Some(report) = report {
                text.push_str(&render_report(plan, report));
            }
            table::terminal_safe(&text)
        }
        (ReportArg::Json, None) => {
            let mut buf = serde_json::to_string_pretty(plan).map_err(io::Error::from)?;
            buf.push('\n');
            buf
        }
        (ReportArg::Json, Some(report)) => {
            let v = serde_json::json!({ "mode": mode.name(), "plan": plan, "job": report });
            let mut buf = serde_json::to_string_pretty(&v).map_err(io::Error::from)?;
            buf.push('\n');
            buf
        }
    };
    match &args.output {
        Some(path) => {
            let mut file = BufWriter::new(File::create(path).map_err(|e| Failure {
                code: 1,
                message: format!("cannot write {}: {e}", path.display()),
            })?);
            file.write_all(text.as_bytes())?;
            file.flush()?;
            Ok(())
        }
        None => print_stdout(&text),
    }
}

// --- Table report ---

/// `1,529,567,750`.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A value's wire name (`world`, `headerless`, `beta`), from
/// `serde_json::to_value`.
fn wire(value: serde_json::Result<serde_json::Value>) -> String {
    match value {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::from("?"),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub(crate) fn relation_text(r: Relation) -> &'static str {
    match r {
        Relation::Exact => "exact copy",
        Relation::Format => "other format",
        Relation::Version => "other version",
    }
}

/// Why the keeper stays, in one line.
pub(crate) fn reason_text(reason: &KeepReason) -> String {
    match reason {
        KeepReason::UserChoice => "your pick".into(),
        KeepReason::HasSave { path } => format!("has a save ({})", file_name(path)),
        KeepReason::Played { playcount } => {
            format!("played ({})", plural(*playcount as usize, "time", "times"))
        }
        KeepReason::InLibrary => "already in the library".into(),
        KeepReason::Verified { kept, other } => format!(
            "better dump ({}); the other is {}",
            wire(serde_json::to_value(kept)),
            wire(serde_json::to_value(other))
        ),
        KeepReason::Retail { other } => {
            format!("retail; the other is {}", wire(serde_json::to_value(other)))
        }
        KeepReason::Region { kept, other } => format!(
            "preferred region ({}); the other is {}",
            wire(serde_json::to_value(kept)),
            wire(serde_json::to_value(other))
        ),
        KeepReason::Revision { kept, other } => {
            format!("newer revision ({kept}); the other is {other}")
        }
        KeepReason::Container { kept, profile, .. } => format!(
            "preferred format on {profile} ({})",
            kept.to_ascii_uppercase()
        ),
        KeepReason::HasCover => "has a cover".into(),
        KeepReason::CatalogueName { name } => format!("catalogue name ({name})"),
        KeepReason::Tie => "tie: the oldest file stays".into(),
        _ => "chosen by the rules".into(),
    }
}

/// Why a group only gets reported.
fn report_text(why: &ReportWhy) -> String {
    match why {
        ReportWhy::LevelTwoOff => {
            "report only: other versions (--one-game-one-rom sets them aside)".into()
        }
        ReportWhy::Unconfirmed => "report only: only a CRC matched".into(),
        ReportWhy::HeaderDiffers => "report only: the headers differ".into(),
        ReportWhy::BuildDiffers => "report only: same serial, another build".into(),
        ReportWhy::PartialSet => "report only: a disc set misses tracks or discs".into(),
        ReportWhy::SerialConflict => "report only: the serials disagree".into(),
        ReportWhy::Modified => "report only: a modified program (hack, translation)".into(),
        ReportWhy::ArcadeNoDat => "report only: arcade sets with no catalogue".into(),
        ReportWhy::BigGroup { members } => {
            format!("report only: too many copies to act on ({members})")
        }
        ReportWhy::UserSkipped => "report only: skipped".into(),
        ReportWhy::ChdMetadataDiffers => "report only: same CHD data, other metadata".into(),
        _ => "report only".into(),
    }
}

/// The system folder of a group: the keeper's first folder below the ROMs
/// folder, else the system id in capitals.
fn group_folder(plan: &DedupePlan, g: &DupGroup) -> String {
    g.members
        .get(g.keeper)
        .and_then(|m| m.unit.primary.strip_prefix(&plan.roms_dir).ok())
        .and_then(|rel| rel.components().next())
        .filter(|_| g.members.len() > 1)
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_else(|| g.system.to_ascii_uppercase())
}

/// Where a copy goes, relative to the quarantine's parent when it is
/// there (`_Duplicados/PS/Crash Bash.bin`).
fn dest_text(plan: &DedupePlan, dest: &Path) -> String {
    plan.quarantine
        .parent()
        .and_then(|parent| dest.strip_prefix(parent).ok())
        .unwrap_or(dest)
        .display()
        .to_string()
}

/// Member path as shown: relative to its system folder.
fn member_name(plan: &DedupePlan, folder: &str, m: &Member) -> String {
    let p = &m.unit.primary;
    p.strip_prefix(plan.roms_dir.join(folder))
        .unwrap_or(p)
        .display()
        .to_string()
}

fn render_group(out: &mut String, plan: &DedupePlan, folder: &str, g: &DupGroup) {
    let relation = Relation::weakest(
        g.members
            .iter()
            .filter(|m| m.id != g.keeper)
            .map(|m| m.relation),
    )
    .unwrap_or(Relation::Exact);
    let confidence = g
        .members
        .iter()
        .filter(|m| m.id != g.keeper)
        .map(|m| m.confidence)
        .fold(1.0f32, f32::min);
    let mut serials: Vec<&str> = g
        .members
        .iter()
        .flat_map(|m| m.serials.iter().map(String::as_str))
        .collect();
    serials.sort_unstable();
    serials.dedup();
    let mut head = format!("  {}", g.title);
    if !serials.is_empty() {
        let _ = write!(head, "  {}", serials.join(", "));
    }
    let _ = write!(head, "  {}  {confidence:.2}", relation_text(relation));
    if g.tie {
        head.push_str("  (tie)");
    }
    let _ = writeln!(out, "{head}");
    if g.choice_dropped {
        let _ = writeln!(
            out,
            "    your earlier pick no longer holds: another copy has a save now"
        );
    }

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    match &g.action {
        GroupAction::Conflict { saves } => {
            let _ = writeln!(
                out,
                "    conflict: the copies have different saves; nothing moves"
            );
            for m in &g.members {
                rows.push(vec![
                    "    ·".into(),
                    member_name(plan, folder, m),
                    table::human_bytes(m.size),
                    relation_text(m.relation).into(),
                ]);
            }
            for s in saves {
                notes.push(format!(
                    "           save {} ({})",
                    file_name(&s.path),
                    table::human_bytes(s.size)
                ));
            }
        }
        action => {
            if let GroupAction::ReportOnly { why } = action {
                let _ = writeln!(out, "    {}", report_text(why));
            }
            let acting = *action == GroupAction::Act;
            let keeper_name = g
                .members
                .get(g.keeper)
                .map(|m| member_name(plan, folder, m))
                .unwrap_or_default();
            for m in &g.members {
                let name = member_name(plan, folder, m);
                if m.id == g.keeper {
                    rows.push(vec![
                        "    stays".into(),
                        name,
                        table::human_bytes(m.size),
                        reason_text(&g.reason),
                    ]);
                    continue;
                }
                let mut what = relation_text(m.relation).to_string();
                if let Some(dest) = m.dest.as_ref().filter(|_| acting) {
                    let _ = write!(what, "  → {}", dest_text(plan, dest));
                }
                let mark = if acting { "    →" } else { "    ·" };
                rows.push(vec![mark.into(), name, table::human_bytes(m.size), what]);
                if acting {
                    for t in &m.travels {
                        let kind = match t.kind {
                            TravelKind::Save => "save",
                            TravelKind::State => "state",
                            TravelKind::Art(_) => "art",
                            _ => "file",
                        };
                        notes.push(format!(
                            "           {kind} {} goes with {}",
                            file_name(&t.from),
                            member_name(plan, folder, m)
                        ));
                    }
                    if !m.art_leaves.is_empty() {
                        notes.push(format!(
                            "           {keeper_name} is left without its {}",
                            m.art_leaves
                                .media
                                .iter()
                                .map(|x| crate::media_name(*x))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                }
                for stay in &m.stays {
                    let with = g
                        .members
                        .get(stay.with)
                        .map(|w| member_name(plan, folder, w))
                        .unwrap_or_else(|| keeper_name.clone());
                    notes.push(format!(
                        "           save {} stays with {with}",
                        file_name(&stay.path)
                    ));
                }
            }
        }
    }
    out.push_str(&table::render(&rows));
    for n in notes {
        let _ = writeln!(out, "{n}");
    }
}

/// The plain-text report of spec §14: one block per system, one per
/// group, then the totals.
fn render_table(plan: &DedupePlan, db: &SystemDb, dry_run: bool) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "library: {}", plan.root.display());
    let _ = writeln!(out, "profile: {}", plan.profile_id);
    let _ = writeln!(out, "quarantine: {}", plan.quarantine.display());
    out.push('\n');

    // Folder → groups, in folder order.
    let mut by_folder: BTreeMap<String, Vec<&DupGroup>> = BTreeMap::new();
    for g in &plan.groups {
        by_folder.entry(group_folder(plan, g)).or_default().push(g);
    }
    for (folder, groups) in &by_folder {
        let system = &groups[0].system;
        let name = db.get(system).map_or(system.as_str(), |s| s.name.as_str());
        let (copies, bytes) = groups.iter().filter(|g| g.action == GroupAction::Act).fold(
            (0usize, 0u64),
            |(c, b), g| {
                let extras = g.members.iter().filter(|m| m.id != g.keeper);
                (
                    c + extras.clone().count(),
                    b + extras.map(|m| m.size).sum::<u64>(),
                )
            },
        );
        let rows = vec![vec![
            folder.clone(),
            name.to_string(),
            plural(groups.len(), "game", "games"),
            plural(copies, "copy", "copies"),
            table::human_bytes(bytes),
        ]];
        out.push_str(&table::render(&rows));
        for g in groups {
            render_group(&mut out, plan, folder, g);
        }
        out.push('\n');
    }

    let t = &plan.totals;
    let acting: usize = t.by_system.values().map(|(g, _, _)| *g).sum();
    let reported = plan
        .groups
        .iter()
        .filter(|g| matches!(g.action, GroupAction::ReportOnly { .. }))
        .count();
    let _ = writeln!(
        out,
        "{} across {}, {} bytes.{}",
        plural(t.extras, "extra copy", "extra copies"),
        plural(acting, "game", "games"),
        thousands(t.bytes),
        if dry_run {
            " Dry run: nothing moved."
        } else {
            ""
        }
    );
    if reported > 0 {
        let _ = writeln!(
            out,
            "{} only reported.",
            plural(reported, "group", "groups")
        );
    }
    if t.conflicts > 0 {
        let _ = writeln!(
            out,
            "{} with different saves: nothing moves until you choose.",
            plural(t.conflicts, "group", "groups")
        );
    }
    if dry_run && plan.same_volume && t.extras > 0 && !plan.trash {
        let _ = writeln!(
            out,
            "The quarantine is on the same volume: setting copies aside frees no space there."
        );
    }
    if !plan.no_catalogue.is_empty() {
        let _ = writeln!(
            out,
            "No catalogue (no other versions): {}.",
            plan.no_catalogue.join(", ")
        );
    }
    if plan.chd_unread > 0 {
        let _ = writeln!(
            out,
            "Other formats: {} not read inside.",
            plural(plan.chd_unread, "CHD", "CHDs")
        );
    }
    if !plan.off_card_saves.is_empty() {
        let _ = writeln!(
            out,
            "Saves may also live off the card for: {}.",
            plan.off_card_saves.join(", ")
        );
    }
    for p in &plan.unreadable {
        let _ = writeln!(out, "unreadable: {}", p.display());
    }
    out
}

// --- Apply report ---

/// Why a copy stayed where it was, in a few words.
fn skip_text(reason: &DedupeSkip) -> String {
    match reason {
        DedupeSkip::KeeperChanged => "the copy that stays changed since the plan".into(),
        DedupeSkip::ChangedSincePlan => "changed since the plan".into(),
        DedupeSkip::SourceMissing => "no longer there".into(),
        DedupeSkip::DestinationLimit { why } => match why {
            DestLimit::Fat32Size => "4 GiB or more: too big for FAT32 there".into(),
            DestLimit::BadName => "a name the destination refuses".into(),
            DestLimit::PathTooLong => "the path is too long there".into(),
            DestLimit::CaseCollision => "a name that only differs by case is there".into(),
        },
        DedupeSkip::TrashUnavailable => "its volume has no Trash".into(),
        DedupeSkip::Conflict => "the copies have different saves".into(),
        DedupeSkip::ReportOnly => "only reported".into(),
        DedupeSkip::Excluded => "left out".into(),
        DedupeSkip::Cancelled => "cancelled".into(),
        DedupeSkip::Taken => "a file of the set is in the quarantine already".into(),
        _ => "left alone".into(),
    }
}

/// The copy `(group, member)` of the plan as the table names it, else the
/// first of its files.
fn outcome_name(plan: &DedupePlan, group: u64, member: usize, files: &[PathBuf]) -> String {
    plan.groups
        .iter()
        .find(|g| g.id == group)
        .and_then(|g| {
            let m = g.members.iter().find(|m| m.id == member)?;
            Some(member_name(plan, &group_folder(plan, g), m))
        })
        .or_else(|| files.first().map(|p| p.display().to_string()))
        .unwrap_or_else(|| format!("group {group}, copy {member}"))
}

/// What an apply or resume did: one row per copy it touched (the
/// conflicts, reported and left-out groups are in the table above), then
/// the totals, the job and how to undo it.
fn render_report(plan: &DedupePlan, report: &DedupeReport) -> String {
    let mut out = String::from("\n");
    let mut rows: Vec<Vec<String>> = Vec::new();
    let (mut quarantined, mut trashed) = (0usize, 0usize);
    for o in &report.outcomes {
        let name = outcome_name(plan, o.group, o.member, &o.files);
        match &o.status {
            DedupeItemStatus::Quarantined { to } => {
                quarantined += 1;
                rows.push(vec![
                    "set aside".into(),
                    name,
                    format!("→ {}", dest_text(plan, to)),
                ]);
            }
            DedupeItemStatus::Trashed { .. } => {
                trashed += 1;
                rows.push(vec!["to the Trash".into(), name, String::new()]);
            }
            DedupeItemStatus::Skipped {
                reason: DedupeSkip::Conflict | DedupeSkip::ReportOnly | DedupeSkip::Excluded,
            } => {}
            DedupeItemStatus::Skipped { reason } => {
                rows.push(vec!["left".into(), name, skip_text(reason)]);
            }
            DedupeItemStatus::Failed { kind, detail } => {
                rows.push(vec![
                    "failed".into(),
                    name,
                    format!("{}: {}", kind.as_str(), table::one_line(detail)),
                ]);
            }
        }
    }
    out.push_str(&table::render(&rows));
    for note in &report.notes {
        match note {
            DedupeApplyNote::GamelistNotUpdated { system } => {
                let _ = writeln!(
                    out,
                    "The game list of {system} was not updated: it is outside the folder Coverly writes into."
                );
            }
            DedupeApplyNote::ArtLeaves { keeper } => {
                let _ = writeln!(
                    out,
                    "{} is left without some art.",
                    table::one_line(&keeper.display().to_string())
                );
            }
            _ => {}
        }
    }
    let done = quarantined + trashed;
    if done == 0 {
        let _ = writeln!(out, "Nothing was set aside.");
    } else {
        let mut line = format!("Set aside {}", plural(done, "copy", "copies"));
        if quarantined > 0 && trashed > 0 {
            let _ = write!(
                line,
                " ({quarantined} in the quarantine, {trashed} in the Trash)"
            );
        } else if trashed > 0 {
            line.push_str(" in the Trash");
        } else {
            let _ = write!(line, " in {}", plan.quarantine.display());
        }
        let _ = writeln!(
            out,
            "{line}: {} bytes left the ROMs folder.",
            thousands(report.bytes_out)
        );
        if quarantined > 0 && plan.same_volume {
            let _ = writeln!(
                out,
                "The quarantine is on the same volume: the card is as full as before."
            );
        }
    }
    if let Some(job) = &report.job {
        if report.paused.is_some() && !report.resumable.is_empty() {
            let _ = writeln!(
                out,
                "{} not done yet: continue with --resume {job}.",
                plural(report.resumable.len(), "copy", "copies")
            );
        }
        let _ = writeln!(out, "Job {job}: undo with `coverly undo --job {job}`.");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_and_human_sizes() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(32_768), "32,768");
        assert_eq!(thousands(1_529_567_750), "1,529,567,750");
        assert_eq!(table::human_bytes(512), "512 B");
        assert_eq!(table::human_bytes(79_600_000), "79.6 MB");
        assert_eq!(table::human_bytes(1_900_000_000), "1.9 GB");
    }

    #[test]
    fn keep_rules_listed_on_then_the_rest_off() {
        let rules = keep_rules(&["cover".into(), "has-save".into()]).unwrap();
        assert_eq!(rules.len(), KeepRule::ALL.len());
        assert_eq!(
            rules[0],
            RuleToggle {
                rule: KeepRule::Cover,
                on: true
            }
        );
        assert_eq!(
            rules[1],
            RuleToggle {
                rule: KeepRule::HasSave,
                on: true
            }
        );
        assert!(rules[2..].iter().all(|t| !t.on));
        assert_eq!(keep_rules(&["shiny".into()]).unwrap_err().code, 2);
        assert_eq!(
            keep_rules(&["cover".into(), "cover".into()])
                .unwrap_err()
                .code,
            2
        );
    }
}
