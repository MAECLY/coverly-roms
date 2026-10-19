//! `coverly dedupe`: find the same game stored more than once inside one
//! system and show which copy stays and where the others would go
//! (duplicates spec §14). Always a dry run for now: setting copies aside
//! arrives with the archive work.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use clap::{Args, ValueEnum};
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::dedupe::keys::DiskBytes;
use coverly_core::dedupe::source::{NoDiscProbe, ZipListing};
use coverly_core::dedupe::DedupeEnv;
use coverly_core::dedupe::{
    plan_dedupe, DedupeOptions, DedupePhase, DedupePlan, DedupeProgress, DupGroup, GroupAction,
    KeepReason, Member, Relation, ReportWhy, TravelKind,
};
use coverly_core::identify::dat::{DatStore, DATABASE_BASE};
use coverly_core::net::{PoliteHttp, UreqHttp};
use coverly_core::paths::AppDirs;
use coverly_core::scan::scan_library;
use coverly_core::settings::{DedupeLevel, KeepRule, RuleToggle};
use coverly_core::systems::SystemDb;

use crate::profile::{check_root, choose_profile};
use crate::{print_stdout, table, Failure};

/// Until setting aside lands (plan Task 27), these flags are refused.
const NOT_YET: &str = "available after the archive work lands";

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
    /// Show the plan only (the default, whatever the "dry run by default"
    /// setting says).
    #[arg(long, conflicts_with = "apply")]
    dry_run: bool,
    /// Set the extra copies aside (not available yet).
    #[arg(long)]
    apply: bool,
    /// Set the extra copies aside in this folder (not available yet).
    #[arg(long, value_name = "DIR", conflicts_with = "trash")]
    quarantine: Option<PathBuf>,
    /// Send the extra copies to the Trash (not available yet).
    #[arg(long)]
    trash: bool,
    /// Continue an interrupted duplicates job (not available yet).
    #[arg(long, value_name = "JOB_ID")]
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

pub fn run(args: &DedupeArgs) -> Result<(), Failure> {
    if args.apply || args.trash || args.quarantine.is_some() || args.resume.is_some() {
        let flag = if args.apply {
            "--apply"
        } else if args.trash {
            "--trash"
        } else if args.quarantine.is_some() {
            "--quarantine"
        } else {
            "--resume"
        };
        return Err(Failure::usage(format!(
            "{flag} is {NOT_YET}; `coverly dedupe` only shows the plan for now"
        )));
    }
    check_root(&args.root)?;
    // Absolute, never canonical: the plan's paths match what you typed.
    let root = std::path::absolute(&args.root)?;
    let cli = crate::settings::load_cli_settings();
    let s = &cli.settings;
    let db = SystemDb::builtin();

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

    let mut chosen = choose_profile(&root, args.profile.as_deref(), &cli)?;
    chosen.profile = s.art_profile(&chosen.profile);
    let profile = &chosen.profile;
    opts.forced_keepers = args
        .keep
        .iter()
        .map(|p| keep_path(p, &root, &profile.roms_root))
        .collect::<io::Result<_>>()?;

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
    let env = DedupeEnv {
        cache: &cache,
        db: &db,
        profile,
        settings: s,
        listing: &ZipListing,
        discs: &NoDiscProbe,
        dats,
        bytes: &DiskBytes,
        progress: &progress,
        cancel: &cancel,
    };
    let plan = plan_dedupe(&scan, &opts, &env).map_err(fail)?;
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

    let text = match args.report {
        ReportArg::Table => table::terminal_safe(&render_table(&plan, &db)),
        ReportArg::Json => {
            let mut buf = serde_json::to_string_pretty(&plan).map_err(io::Error::from)?;
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

/// `79.6 MB`, decimal units.
fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit])
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

fn relation_text(r: Relation) -> &'static str {
    match r {
        Relation::Exact => "exact copy",
        Relation::Format => "other format",
        Relation::Version => "other version",
    }
}

/// Why the keeper stays, in one line.
fn reason_text(reason: &KeepReason) -> String {
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
                    human(m.size),
                    relation_text(m.relation).into(),
                ]);
            }
            for s in saves {
                notes.push(format!(
                    "           save {} ({})",
                    file_name(&s.path),
                    human(s.size)
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
                        human(m.size),
                        reason_text(&g.reason),
                    ]);
                    continue;
                }
                let mut what = relation_text(m.relation).to_string();
                if let Some(dest) = m.dest.as_ref().filter(|_| acting) {
                    let _ = write!(what, "  → {}", dest_text(plan, dest));
                }
                let mark = if acting { "    →" } else { "    ·" };
                rows.push(vec![mark.into(), name, human(m.size), what]);
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
fn render_table(plan: &DedupePlan, db: &SystemDb) -> String {
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
            human(bytes),
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
        "{} across {}, {} bytes. Dry run: nothing moved.",
        plural(t.extras, "extra copy", "extra copies"),
        plural(acting, "game", "games"),
        thousands(t.bytes)
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
    if plan.same_volume && t.extras > 0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_and_human_sizes() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(32_768), "32,768");
        assert_eq!(thousands(1_529_567_750), "1,529,567,750");
        assert_eq!(human(512), "512 B");
        assert_eq!(human(79_600_000), "79.6 MB");
        assert_eq!(human(1_900_000_000), "1.9 GB");
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
