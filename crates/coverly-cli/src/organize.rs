//! `coverly organize`: identify the ROMs of a loose pile, propose one
//! folder per system in the chosen layout, and show the plan (dry run).
//! Applying the plan is `--apply` (organise jobs, undone by `coverly undo`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, IsTerminal};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use clap::{Args, ValueEnum};
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::identify::dat::{DatStore, DATABASE_BASE};
use coverly_core::identify::evidence::{Evidence, IdLevel};
use coverly_core::identify::sniff::NotARom;
use coverly_core::net::{PoliteHttp, UreqHttp};
use coverly_core::organize::{
    plan_from_survey, survey, Choice, Decision, ItemNote, LeftReason, NameMode, OrganizeAction,
    OrganizeEnv, OrganizeError, OrganizeItem, OrganizeOptions, OrganizePhase, OrganizePlan,
    OrganizeProgress, OsMetaProbe, PlanWarning, RenameReason, Survey, TransferMode,
};
use coverly_core::paths::AppDirs;
use coverly_core::profiles::custom::all_profiles;
use coverly_core::profiles::Profile;
use coverly_core::systems::SystemDb;

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
    /// Continue an interrupted organise job.
    #[arg(long, value_name = "JOB_ID", conflicts_with_all = ["apply", "dry_run"])]
    resume: Option<String>,
    /// Move the items to review with their best candidate system.
    #[arg(long)]
    include_review: bool,
    /// Cached catalogues only: no download, no online lookup.
    #[arg(long)]
    offline: bool,
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

/// Prints `identify 40/631` lines to stderr, only when it is a terminal.
fn progress_line(p: OrganizeProgress) {
    let phase = match p.phase {
        OrganizePhase::Walk => "walk",
        OrganizePhase::Identify => "identify",
        OrganizePhase::Dats => "catalogues",
        OrganizePhase::Hash => "hash",
        OrganizePhase::Apply => "apply",
    };
    eprintln!("{phase} {}/{}", p.done, p.total);
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
    opts.offline = args.offline;
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
    if args.resume.is_some() {
        return Err(Failure::usage(
            "--resume is not available in this build yet",
        ));
    }
    let apply = args.apply || (!args.dry_run && !s.safety.dry_run_default);
    if apply {
        return Err(Failure::usage(
            "--apply is not available in this build yet; run without it for the plan",
        ));
    }

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
    let dats = (!opts.offline && opts.download_dats).then_some(&store);
    let env = OrganizeEnv {
        db: &db,
        profiles: &profiles,
        cache: &cache,
        dats,
        probe: &OsMetaProbe,
        online: None,
        now: SystemTime::now(),
    };

    let tty = io::stderr().is_terminal();
    let progress = move |p: OrganizeProgress| {
        if tty {
            progress_line(p);
        }
    };
    let cancel = AtomicBool::new(false);
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
    emit(args, &plan, &db, &opts)
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
    plan: &OrganizePlan,
    db: &SystemDb,
    opts: &OrganizeOptions,
) -> Result<(), Failure> {
    let text = match args.report {
        ReportArg::Json => {
            let v = serde_json::json!({
                "mode": "dry-run",
                "plan": plan,
                "job": serde_json::Value::Null,
            });
            let mut s = serde_json::to_string_pretty(&v).map_err(fail)?;
            s.push('\n');
            s
        }
        ReportArg::Table => render_table(plan, db, opts),
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
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
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

/// Why `reason` keeps a file where it is.
pub(crate) fn left_text(reason: &LeftReason) -> String {
    match reason {
        LeftReason::PartialDownload => "download not finished".into(),
        LeftReason::RecentlyModified => "modified moments ago, may still be written".into(),
        LeftReason::CloudOnly => "in the cloud, not on this disk".into(),
        LeftReason::NotARom { what } => match what {
            NotARom::WebPage => "not a ROM (web page)".into(),
            NotARom::Text => "not a ROM (text)".into(),
        },
        LeftReason::Bios => "BIOS or firmware".into(),
        LeftReason::DamagedArchive => "damaged archive".into(),
        LeftReason::UnsupportedArchive => "unsupported archive format".into(),
        LeftReason::MultiGameArchive { count } => format!("archive with {count} games"),
        LeftReason::UnsupportedSystem { name } => format!("{name} is not organised"),
        LeftReason::Unknown => "unknown system".into(),
        LeftReason::Duplicate { of } => format!("same as {}", slash(of)),
        LeftReason::AlreadyPresent { at } => format!("already in {}", slash(at)),
        LeftReason::AlreadySorted { folder } => {
            format!("already in the {folder} system folder")
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
        ItemNote::ExtensionContradicted { ext } => {
            format!(
                "the .{} extension says otherwise",
                ext.trim_start_matches('.')
            )
        }
        ItemNote::Encrypted => "encrypted archive: the header was not read".into(),
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

fn render_table(plan: &OrganizePlan, db: &SystemDb, opts: &OrganizeOptions) -> String {
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

    // Moving items, grouped by their system folder.
    let mut groups: BTreeMap<String, Vec<&OrganizeItem>> = BTreeMap::new();
    for item in &plan.items {
        if item.action != OrganizeAction::Auto {
            continue;
        }
        let folder = item
            .target
            .as_deref()
            .and_then(Path::parent)
            .map(slash)
            .unwrap_or_default();
        groups.entry(folder).or_default().push(item);
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
            let target = item.target.as_deref().map(slash).unwrap_or_default();
            let why = item.why.first().map(why_text).unwrap_or_default();
            let _ = writeln!(
                out,
                "  {} → {}  {}  {why}{}",
                source_label(item),
                target,
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
            let _ = writeln!(out, "  {}  {}", source_label(item), left_text(reason));
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
    let _ = writeln!(
        out,
        "\nDry run: nothing was written. Re-run with --apply to organise {} files.",
        c.auto
    );
    table::terminal_safe(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn slash_joins_with_forward_slashes() {
        assert_eq!(slash(Path::new("ROMS").join("GBA").as_path()), "ROMS/GBA");
    }
}
