//! `coverly organize`: identify the ROMs of a loose pile, propose one
//! folder per system in the chosen layout, and show the plan (dry run).
//! Applying the plan is `--apply` (organise jobs, undone by `coverly undo`).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::{self, IsTerminal};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use clap::{Args, ValueEnum};
use coverly_core::addons::{AddonFormat, AddonKind, HeaderFit, LinkVia, UnlinkedWhy};
use coverly_core::archive::{FoundTool, Tool};
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::catalog::{SwitchCatalog, TitleNames};
use coverly_core::consoles::{FixedFolder, PartKind};
use coverly_core::credentials::{DevCredentials, SourceKeys};
use coverly_core::identify::dat::{DatStore, DATABASE_BASE};
use coverly_core::identify::evidence::{Evidence, IdLevel};
use coverly_core::identify::sniff::NotARom;
use coverly_core::net::{PoliteHttp, UreqHttp};
use coverly_core::organize::online::ScreenScraperLookup;
use coverly_core::organize::{
    apply_organize, gate, large_transfer, large_transfer_threshold, load_plan, plan_from_survey,
    resume_organize, survey, ApplyEnv, ApplyError, Choice, Decision, EmulatorChoice, EmulatorId,
    GameLink, GameRole, HugeItem, ItemKind, ItemNote, ItemOutcome, ItemStatus, LargeConfirm,
    LargeTransfer, LeftReason, NameMode, NotSupportedWhy, OrganizeAction, OrganizeEnv,
    OrganizeError, OrganizeItem, OrganizeOptions, OrganizePhase, OrganizePlan, OrganizeProgress,
    OrganizeReport, OsMetaProbe, PlacementTo, PlacementView, PlanWarning, RenameReason, SkipReason,
    Survey, Transfer, TransferMode, TranslateNote,
};
use coverly_core::paths::AppDirs;
use coverly_core::profiles::custom::all_profiles;
use coverly_core::profiles::Profile;
use coverly_core::settings::{OrganizeDedupeMode, Settings};
use coverly_core::sources::screenscraper::{ScreenScraperSource, SsConfig, SCREENSCRAPER_BASE};
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::process_trash;

use crate::catalog::{catalog_http, ui_locale};
use crate::dedupe::{reason_text, relation_text};
use crate::fill::write_file;
use crate::keys::{cli_store, resolve_keys, KeyPlan};
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
            "game_folders", "no_game_folders", "emulator", "emulator_data",
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
    /// Give each modern game its own folder for this run, with its
    /// updates in `Updates/` and its DLC in `DLC/` (default: the game
    /// folders setting of the layout, else the layout's own value).
    #[arg(long, conflicts_with = "no_game_folders")]
    game_folders: bool,
    /// Keep bases, updates and DLC loose in the system folder for this
    /// run, whatever the layout or the game folders setting say.
    #[arg(long)]
    no_game_folders: bool,
    /// Lay SYSTEM out for EMULATOR in this run (`ps4=shadps4`,
    /// `3ds=azahar`, `snes=retroarch`), or `SYSTEM=none` for the
    /// library's own layout; a system not named uses the layout's
    /// default. Updates and DLC that an emulator installs stay in the
    /// library; nothing is ever extracted, decrypted or installed.
    /// Repeatable.
    #[arg(long = "emulator", value_name = "SYSTEM=EMULATOR")]
    emulator: Vec<String>,
    /// The emulator's own data folder (absolute), where its mods,
    /// cheats and patches go (`duckstation=/path/to/duckstation`).
    /// Without it they stay where they are and the plan says where
    /// they belong. Repeatable.
    #[arg(long = "emulator-data", value_name = "EMULATOR=DIR")]
    emulator_data: Vec<String>,
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
    /// Confirm an apply that writes 10 GB or more to the target (copies,
    /// moves across volumes, extractions); without it such a run stops
    /// before writing anything.
    #[arg(long, requires = "apply")]
    yes_large: bool,
    /// Leave every item that writes 10 GB or more on its own (or extracts
    /// a folder game) where it is, without asking; the rest still needs
    /// confirming when it reaches 10 GB.
    #[arg(long, requires = "apply", conflicts_with = "yes_large")]
    skip_large: bool,
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
    #[arg(long, hide = true)]
    titledb_url: Option<String>,
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
        | OrganizeError::UnknownItem(_)
        | OrganizeError::EmulatorData { .. } => 2,
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

/// An apply writing 10 GB or more without `--yes-large` (systems batch 3
/// D23): refused like the no-space refusal, before anything is written.
fn large_failure(t: &LargeTransfer) -> Failure {
    fail(format!(
        "would write {} bytes (≥ 10 GB) to {}; re-run with --yes-large to confirm; nothing was written",
        t.bytes, t.volume
    ))
}

/// An answer to a per-item question (adjustments spec D18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Yes,
    All,
    No,
}

impl Answer {
    /// `y`/`yes`, `a`/`all`, `n`/`no`, any case; anything else is asked
    /// again.
    fn parse(line: &str) -> Option<Answer> {
        match line.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => Some(Answer::Yes),
            "a" | "all" => Some(Answer::All),
            "n" | "no" => Some(Answer::No),
            _ => None,
        }
    }
}

/// Where the per-item questions are asked (injected, so tests script the
/// answers on every OS without a pty).
pub trait Prompt {
    /// Writes `question` to stderr and reads the answer; `None` on EOF or
    /// Ctrl-C. An answer that is not one of the choices is asked again.
    fn ask(&mut self, question: &str) -> Option<Answer>;
}

/// The terminal: the question on stderr (stdout keeps the report), the
/// answer from stdin.
struct TtyPrompt;

impl Prompt for TtyPrompt {
    fn ask(&mut self, question: &str) -> Option<Answer> {
        use std::io::{BufRead, Write};
        loop {
            {
                let mut err = io::stderr().lock();
                let _ = write!(err, "{} ", table::terminal_safe(question));
                let _ = err.flush();
            }
            let mut line = String::new();
            match io::stdin().lock().read_line(&mut line) {
                Ok(0) | Err(_) => {
                    eprintln!();
                    return None;
                }
                Ok(_) => {
                    if let Some(a) = Answer::parse(&line) {
                        return Some(a);
                    }
                }
            }
        }
    }
}

/// Test builds only: `y,a,n…` answers the questions in order (then EOF);
/// the questions still go to stderr.
const TEST_PROMPT_ENV: &str = "COVERLY_TEST_PROMPT";

struct ScriptedPrompt {
    answers: std::collections::VecDeque<String>,
}

impl Prompt for ScriptedPrompt {
    fn ask(&mut self, question: &str) -> Option<Answer> {
        loop {
            eprintln!("{}", table::terminal_safe(question));
            if let Some(a) = Answer::parse(&self.answers.pop_front()?) {
                return Some(a);
            }
        }
    }
}

/// The prompt of this run: only when stdin and stderr are both terminals
/// (spec D18), else `None` (scripts keep today's refusal).
fn terminal_prompt() -> Option<Box<dyn Prompt>> {
    if let Some(script) = coverly_core::test_hook(TEST_PROMPT_ENV) {
        let answers = script
            .split(',')
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .map(String::from)
            .collect();
        return Some(Box::new(ScriptedPrompt { answers }));
    }
    (io::stdin().is_terminal() && io::stderr().is_terminal())
        .then(|| Box::new(TtyPrompt) as Box<dyn Prompt>)
}

/// What `sum` more bytes leave free on a target with `free` bytes, with
/// the apply's 1 % margin: `Ok(left)` or `Err(missing)`.
fn room_after(free: u64, sum: u64) -> Result<u64, u64> {
    let needed = sum.saturating_add(sum.div_ceil(100));
    free.checked_sub(needed).ok_or_else(|| needed - free)
}

/// The question for one huge item (spec D18).
fn item_question(t: &LargeTransfer, h: &HugeItem, used: u64) -> (String, bool) {
    let verb = if h.extract {
        "Extract"
    } else if t.copies {
        "Copy"
    } else {
        "Move"
    };
    let name = table::one_line(&h.name);
    let size = table::human_bytes(h.bytes);
    let volume = table::one_line(&t.volume);
    match t.free.map(|f| room_after(f, used.saturating_add(h.bytes))) {
        Some(Ok(left)) => (
            format!(
                "{verb} \"{name}\" ({size}; {} free on {volume} after this)? [y]es / [a]ll / [n]o:",
                table::human_bytes(left)
            ),
            true,
        ),
        Some(Err(missing)) => (
            format!(
                "{verb} \"{name}\" ({size}; does not fit on {volume}: {} short)? [n]o:",
                table::human_bytes(missing)
            ),
            false,
        ),
        None => (
            format!("{verb} \"{name}\" ({size})? [y]es / [a]ll / [n]o:"),
            true,
        ),
    }
}

/// The aggregate question for what the non-huge items write (D16b).
fn rest_question(t: &LargeTransfer, used: u64) -> String {
    let what = if t.huge.is_empty() {
        format!(
            "Write {} to {}",
            table::human_bytes(t.rest),
            table::one_line(&t.volume)
        )
    } else {
        format!(
            "Write {} more to {}",
            table::human_bytes(t.rest),
            table::one_line(&t.volume)
        )
    };
    match t.free.map(|f| room_after(f, used.saturating_add(t.rest))) {
        Some(Ok(left)) => format!(
            "{what} ({} free after this)? [y]es / [n]o:",
            table::human_bytes(left)
        ),
        Some(Err(missing)) => format!(
            "{what} (does not fit: {} short)? [y]es / [n]o:",
            table::human_bytes(missing)
        ),
        None => format!("{what}? [y]es / [n]o:"),
    }
}

/// Confirms what `plan` writes past the threshold (spec D16, D16b, D18):
/// each huge item is asked on its own (`answer_all` answers them all:
/// `All` for `--yes-large`, `No` for `--skip-large`), declined ones are
/// left through [`Choice::Decline`] and the plan is made again with
/// `replan` (an item that became huge through it is asked in turn); then,
/// only when what the other items write reaches the threshold, one
/// question for that rest. Without a prompt, anything still
/// unconfirmed is refused exactly as before (`--yes-large` hint); EOF at a
/// question refuses too. Returns the plan to apply, which the shared gate
/// accepts.
fn confirm_large(
    plan: OrganizePlan,
    t: LargeTransfer,
    answer_all: Option<Answer>,
    mut prompt: Option<Box<dyn Prompt>>,
    replan: &dyn Fn(&[Decision]) -> Result<OrganizePlan, Failure>,
) -> Result<OrganizePlan, Failure> {
    if answer_all == Some(Answer::All) {
        return Ok(plan);
    }
    // A run that cannot ask and was told nothing: today's refusal.
    if answer_all.is_none() && prompt.is_none() {
        return Err(large_failure(&t));
    }
    // Declining an item can make another one huge (the copy a duplicate
    // set aside becomes the keeper): the plan is made again and every item
    // not answered yet is asked, until no answer declines anything new.
    let mut answers = Answers::default();
    let mut plan = plan;
    let mut left = Some(t);
    while let Some(t) = &left {
        let Some(round) = answer_items(t, answer_all, &mut prompt, &answers) else {
            return Err(large_failure(t));
        };
        answers.all |= round.all;
        answers.accepted.extend(round.accepted);
        if round.declined.is_empty() {
            break;
        }
        answers.declined.extend(round.declined);
        plan = replan(&answers.declined)?;
        left = large_transfer(&plan);
    }
    let Answers {
        accepted,
        declined: _,
        all,
    } = answers;
    let Some(left) = left else {
        return Ok(plan);
    };
    if !all && left.rest >= large_transfer_threshold() {
        let used: u64 = left
            .huge
            .iter()
            .map(|h| h.bytes)
            .fold(0, u64::saturating_add);
        let Some(p) = prompt.as_deref_mut() else {
            return Err(rest_failure(&left));
        };
        match p.ask(&rest_question(&left, used)) {
            Some(Answer::Yes | Answer::All) => {}
            None | Some(Answer::No) => return Err(large_failure(&left)),
        }
    }
    let confirm = LargeConfirm {
        bytes: left.bytes,
        items: accepted,
    };
    gate(&plan, Some(&confirm)).map_err(|t| large_failure(&t))?;
    Ok(plan)
}

/// The answers to the per-item questions of [`confirm_large`].
#[derive(Debug, Default)]
struct Answers {
    /// Item ids accepted.
    accepted: Vec<usize>,
    /// One [`Choice::Decline`] per item declined.
    declined: Vec<Decision>,
    /// «All» was answered: the rest needs no question.
    all: bool,
}

/// Asks each huge item of `t` not in `before` (the answers of earlier
/// rounds) in turn, or answers them all with `answer_all` (or with
/// «yes» once «all» was answered); an item that does not fit takes only
/// «no». `None` when the prompt ends (EOF) or there is none to ask.
fn answer_items(
    t: &LargeTransfer,
    answer_all: Option<Answer>,
    prompt: &mut Option<Box<dyn Prompt>>,
    before: &Answers,
) -> Option<Answers> {
    let mut out = Answers::default();
    // Bytes already counted against the free space: the rest, then each
    // item accepted (in an earlier round too).
    let mut used = t
        .huge
        .iter()
        .filter(|h| before.accepted.contains(&h.item))
        .map(|h| h.bytes)
        .fold(t.rest, u64::saturating_add);
    let answered = |item: usize| {
        before.accepted.contains(&item) || before.declined.iter().any(|d| d.item == item)
    };
    for h in t.huge.iter().filter(|h| !answered(h.item)) {
        let answer = match (answer_all, out.all || before.all) {
            (Some(a), _) => a,
            (None, true) => Answer::Yes,
            (None, false) => {
                let (question, fits) = item_question(t, h, used);
                let p = prompt.as_deref_mut()?;
                loop {
                    match p.ask(&question) {
                        None => return None,
                        Some(Answer::No) => break Answer::No,
                        Some(_) if !fits => continue,
                        Some(a) => break a,
                    }
                }
            }
        };
        match answer {
            Answer::No => out.declined.push(Decision {
                item: h.item,
                choice: Choice::Decline,
            }),
            Answer::All => {
                out.all = true;
                out.accepted.push(h.item);
                used = used.saturating_add(h.bytes);
            }
            Answer::Yes => {
                out.accepted.push(h.item);
                used = used.saturating_add(h.bytes);
            }
        }
    }
    Some(out)
}

/// `--skip-large` without a terminal when the rest alone still reaches
/// the threshold: nothing confirms it.
fn rest_failure(t: &LargeTransfer) -> Failure {
    fail(format!(
        "the other items would still write {} bytes (≥ 10 GB) to {}; run in a terminal to confirm, or use --yes-large instead of --skip-large; nothing was written",
        t.rest, t.volume
    ))
}

/// The Switch catalogue this run uses (plan P-1, P-2): `None` while the
/// setting is off, under `--offline` and for a pile without a Switch
/// item. Otherwise it is downloaded or refreshed when due (a first
/// download is capped at 30 s, after which the plan goes on without it),
/// with `ncas.json` only when an item has NCA ids but no title id, and the
/// nlib fallback asked only when it is on.
fn switch_catalog(
    args: &OrganizeArgs,
    s: &Settings,
    found: &Survey,
    data: &Path,
    cancel: &AtomicBool,
) -> Option<SwitchCatalog> {
    let sw = &s.catalogs.switch;
    if args.offline || !sw.enabled || !found.has_switch() {
        return None;
    }
    let mut catalog = SwitchCatalog::open(data).ok()?;
    if let Some(base) = &args.titledb_url {
        catalog = catalog.with_base(base.clone());
    }
    let http = catalog_http(s);
    let now = SystemTime::now();
    let deadline = Some(Instant::now() + Duration::from_secs(30));
    let locale = ui_locale(s);
    let quiet = |_: u64, _: Option<u64>| {};
    catalog.ensure(&http, sw, locale, now, cancel, deadline, &quiet);
    if found.wants_ncas() {
        catalog.ensure_ncas(&http, sw, now, cancel, deadline, &quiet);
    }
    if sw.fallback_on() {
        let ids = found.switch_title_ids();
        catalog.ensure_nlib(&http, sw, locale, &ids, now, cancel, deadline);
    }
    Some(catalog)
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
        if let OrganizePhase::Folder {
            bytes_done,
            bytes_total,
            ..
        } = p.phase
        {
            // A folder copy prints its start and its end only.
            return bytes_done == 0 || bytes_done >= bytes_total;
        }
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
        OrganizePhase::Folder {
            bytes_done,
            bytes_total,
            ..
        } => {
            return Some(format!(
                "copy folder item {}/{} bytes {bytes_done}/{bytes_total}",
                p.done + 1,
                p.total
            ));
        }
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
    let Some(profile) = profiles.iter().find(|p| p.id == opts.layout) else {
        return Err(Failure::usage(format!(
            "unknown layout `{}`; expected one of: {}",
            opts.layout,
            profile_ids(&cli.custom)
        )));
    };
    opts.game_folders = Some(game_folders_for(args, profile, s));
    let db = SystemDb::builtin();
    for e in &args.emulator {
        let (system, choice) = parse_emulator(e, &db)?;
        opts.emulators.insert(system, choice);
    }
    for e in &args.emulator_data {
        let (emulator, dir) = parse_emulator_data(e)?;
        opts.emulator_data.insert(emulator, dir);
    }
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
    // The account is only needed when the lookup can run at all.
    let ss_on = !opts.offline
        && DevCredentials::from_build().is_some()
        && s.sources
            .order
            .iter()
            .any(|e| e.id == "screenscraper" && e.enabled);
    let plan = KeyPlan {
        steamgriddb: false,
        screenscraper: ss_on,
        default_endpoints: args.screenscraper_url.is_none(),
    };
    let keychain = cli_store();
    let keys = resolve_keys(
        |name| std::env::var(name).ok(),
        keychain.as_deref(),
        plan,
        &mut |m| eprintln!("note: {m}"),
    );
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
        titles: None,
        now: SystemTime::now(),
    };

    let found = survey(&source, &opts, &env, &progress, &cancel).map_err(organize_failure)?;
    // The optional Switch catalogue (plan P-2): fetched or refreshed only
    // when it is on, the run is online and the pile has a Switch item.
    let catalog = switch_catalog(args, s, &found, &dirs.data, &cancel);
    let env = OrganizeEnv {
        titles: catalog.as_ref().map(|c| c as &dyn TitleNames),
        ..env
    };
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
    if let Some(t) = large_transfer(&plan) {
        let replan = |extra: &[Decision]| {
            let mut all: Vec<Decision> = decisions
                .iter()
                .filter(|d| !extra.iter().any(|e| e.item == d.item))
                .cloned()
                .collect();
            all.extend_from_slice(extra);
            plan_from_survey(&found, &opts, &all, &env).map_err(organize_failure)
        };
        let answer_all = if args.yes_large {
            Some(Answer::All)
        } else if args.skip_large {
            Some(Answer::No)
        } else {
            None
        };
        plan = confirm_large(plan, t, answer_all, terminal_prompt(), &replan)?;
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
                format!(": it stays where it is ({})", left_text_for(item, reason))
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

/// The systems some emulator lays out (`--emulator`), sorted.
fn emulator_systems(db: &SystemDb) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = EmulatorId::ALL
        .iter()
        .flat_map(|e| e.systems().iter().copied())
        .filter(|s| db.get(s).is_some())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Every emulator id, comma-separated (`shadps4, rpcs3, …`).
fn emulator_ids() -> String {
    EmulatorId::ALL
        .iter()
        .map(|e| e.id())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `--emulator SYSTEM=EMULATOR|none` (mods spec D24): the system id and
/// the run's choice; an unknown system or emulator, or an emulator that
/// does not run the system, is a usage error naming the valid ids.
pub(crate) fn parse_emulator(s: &str, db: &SystemDb) -> Result<(String, EmulatorChoice), Failure> {
    let Some((system, emulator)) = s
        .split_once('=')
        .map(|(a, b)| (a.trim().to_ascii_lowercase(), b.trim()))
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
    else {
        return Err(Failure::usage(format!(
            "--emulator expects SYSTEM=EMULATOR (or SYSTEM=none), got `{s}`"
        )));
    };
    let systems = emulator_systems(db);
    if !systems.contains(&system.as_str()) {
        return Err(Failure::usage(format!(
            "unknown system `{system}` in --emulator; expected one of: {}",
            systems.join(", ")
        )));
    }
    if emulator.eq_ignore_ascii_case("none") {
        return Ok((system, EmulatorChoice::None));
    }
    let Some(id) = EmulatorId::from_id(emulator) else {
        return Err(Failure::usage(format!(
            "unknown emulator `{emulator}` in --emulator; expected one of: {}, none",
            emulator_ids()
        )));
    };
    if !id.systems().contains(&system.as_str()) {
        let runs: Vec<&str> = EmulatorId::for_system(&system)
            .into_iter()
            .map(EmulatorId::id)
            .chain(["none"])
            .collect();
        return Err(Failure::usage(format!(
            "{id} does not run {system} (--emulator {system}={id}); use one of: {}",
            runs.join(", ")
        )));
    }
    Ok((system, EmulatorChoice::Emulator(id)))
}

/// `--emulator-data EMULATOR=DIR` (mods spec D24): a known emulator and
/// an absolute folder (split on the first `=`: a path may hold one).
pub(crate) fn parse_emulator_data(s: &str) -> Result<(EmulatorId, PathBuf), Failure> {
    let Some((emulator, dir)) = s
        .split_once('=')
        .map(|(a, b)| (a.trim(), b))
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
    else {
        return Err(Failure::usage(format!(
            "--emulator-data expects EMULATOR=DIR, got `{s}`"
        )));
    };
    let Some(id) = EmulatorId::from_id(emulator) else {
        return Err(Failure::usage(format!(
            "unknown emulator `{emulator}` in --emulator-data; expected one of: {}",
            emulator_ids()
        )));
    };
    let dir = PathBuf::from(dir);
    if !dir.is_absolute() {
        return Err(Failure::usage(format!(
            "--emulator-data needs an absolute folder: `{}`",
            dir.display()
        )));
    }
    Ok((id, dir))
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

/// Whether this run builds game folders (game folders spec D23): the
/// flag, else the user's setting for the layout, else the layout's value.
fn game_folders_for(args: &OrganizeArgs, profile: &Profile, settings: &Settings) -> bool {
    if args.game_folders {
        true
    } else if args.no_game_folders {
        false
    } else {
        profile.uses_game_folders(&settings.organize)
    }
}

/// The version column of an update or DLC row (`v1.1.0`; D11 display
/// form, D23), empty for every other item.
fn version_column(item: &OrganizeItem) -> String {
    match &item.game {
        Some(GameLink {
            role: GameRole::Update | GameRole::Dlc,
            version: Some(v),
            ..
        }) if !v.trim().is_empty() => format!("  v{}", v.trim()),
        _ => String::new(),
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
#[cfg(test)]
pub(crate) fn left_text(reason: &LeftReason) -> String {
    table::one_line(&left_reason_text(reason, None))
}

/// [`left_text`] for one item: a reason that names a command line gets
/// the item's system in it.
pub(crate) fn left_text_for(item: &OrganizeItem, reason: &LeftReason) -> String {
    table::one_line(&left_reason_text(reason, item.system_id.as_deref()))
}

fn left_reason_text(reason: &LeftReason, system: Option<&str>) -> String {
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
            NotARom::PcGameData => "not a ROM (PC game data)".into(),
            NotARom::ConsoleSystem => "not a ROM (console system file)".into(),
            NotARom::EmulatorCache => "not a ROM (emulator cache)".into(),
            NotARom::EmulatorConfig => "not a ROM (emulator settings)".into(),
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
        LeftReason::KeyMaterial => "key or licence file: never read, left here".into(),
        LeftReason::LicenceOf { of } => format!(
            "licence of {}: goes with it, never read",
            of.file_name().unwrap_or(of.as_os_str()).to_string_lossy()
        ),
        LeftReason::EmulatorData { folder } => {
            format!("inside an emulator's data folder ({folder}): left as it is")
        }
        LeftReason::GameOverlay { of } => {
            format!("mods or patch folder of {of}: left where it is")
        }
        LeftReason::GamePart {
            part: PartKind::Backup,
            ..
        } => "encrypted Vita backup; not moved on its own".into(),
        LeftReason::GamePart { .. } => "part of a game; not moved on its own".into(),
        LeftReason::NotConfirmed => "not confirmed; left where it is".into(),
        LeftReason::GameFolderContent { folder, of } => format!(
            "kept in place: the {} folder of {of} stays for now",
            fixed_folder_name(*folder)
        ),
        LeftReason::AddonUnlinked { kind, format, why } => {
            let what = format!("{}, {}", addon_kind_name(*kind), addon_format_name(*format));
            match why {
                UnlinkedWhy::NoMatch | UnlinkedWhy::NoKey => {
                    format!("its game is unknown: stays here ({what})")
                }
                UnlinkedWhy::Several { count } => format!("fits {count} games: stays here"),
                UnlinkedWhy::Many { count } => {
                    format!("holds patches for {count} games: stays here")
                }
                UnlinkedWhy::Budget => {
                    format!("its game could not be checked in time: stays here ({what})")
                }
                UnlinkedWhy::Partial => format!("only part of a mod: stays here ({what})"),
            }
        }
        LeftReason::AddonNameTaken { .. } => {
            "a file with that name is already in its folder".into()
        }
        LeftReason::AddonNoPlace { kind } => format!(
            "this layout has no folder for its game's {}: stays here",
            match kind {
                AddonKind::Cheat => "cheats",
                AddonKind::Patch | AddonKind::Mod => "mods",
            }
        ),
        LeftReason::AddonForEmulator { emulator, path } => format!(
            "goes in {emulator}'s folder: {} (pass --emulator-data)",
            slash(path)
        ),
        LeftReason::NotSupported { emulator, why } => format!(
            "{emulator} does not use it like this: {}",
            not_supported_text(*why)
        ),
        LeftReason::PatchNotApplied { emulator } => {
            let id = EmulatorId::ALL
                .into_iter()
                .find(|e| e.display_name() == emulator.as_str())
                .map_or_else(|| emulator.to_lowercase(), |e| e.id().to_string());
            format!(
                "{emulator} would apply it when you play: pass --emulator {}={id}",
                system.unwrap_or("<system>")
            )
        }
    }
}

/// What an addon is, in a left reason (the core has no UI strings).
fn addon_kind_name(kind: AddonKind) -> &'static str {
    match kind {
        AddonKind::Patch => "Translation patch",
        AddonKind::Cheat => "Cheats",
        AddonKind::Mod => "Mod",
    }
}

/// An addon format's short name: product and format names, never
/// translated.
fn addon_format_name(format: AddonFormat) -> &'static str {
    use AddonFormat as F;
    match format {
        F::Bps => "BPS",
        F::Ups => "UPS",
        F::Ips => "IPS",
        F::Xdelta => "xdelta",
        F::Ppf => "PPF",
        F::Aps => "APS",
        F::RetroArchCht => "RetroArch .cht",
        F::FceuxCht => "FCEUX .cht",
        F::DuckStationCht => "DuckStation .cht",
        F::Pnach => "PCSX2 .pnach",
        F::DolphinIni => "Dolphin .ini",
        F::PpssppIni => "PPSSPP .ini",
        F::CtrCheats => "3DS cheats",
        F::SwitchCheats => "Switch cheats",
        F::Pchtxt => "IPSwitch .pchtxt",
        F::Rpcs3Patch => "RPCS3 patch",
        F::Shadps4Cheat => "shadPS4 cheats",
        F::Shadps4Patch => "shadPS4 patch",
        F::XeniaPatch => "Xenia patch",
        F::Vita3kPatch => "Vita3K patch",
        F::MelonDsCheats => "melonDS cheats",
        F::MgbaCheats => "mGBA cheats",
        F::DesmumeCheats => "DeSmuME cheats",
        F::CodeText => "cheat codes",
        F::TexturePack => "texture pack",
        F::DolphinGraphicMod => "Dolphin graphics mod",
        F::Riivolution => "Riivolution",
        F::LayeredFs => "LayeredFS mod",
        F::SwitchMod => "Switch mod",
        F::ExefsPatches => "ExeFS patches",
        F::CemuGraphicPack => "Cemu graphic pack",
        F::VitaRepatch => "rePatch",
        F::MesenHdPack => "Mesen HD pack",
        F::Msu1 => "MSU-1",
        F::XeniaPlugin => "Xenia plugin",
        F::N64HiresTextures => "N64 hi-res textures",
        F::Shadps4Mods => "shadPS4 mods",
    }
}

/// What an addon row is: its kind, or the format family when that says
/// more (texture packs, emulator patches).
fn addon_label(kind: AddonKind, format: AddonFormat) -> &'static str {
    use AddonFormat as F;
    match format {
        F::TexturePack | F::N64HiresTextures => "Texture pack",
        F::Rpcs3Patch
        | F::Shadps4Patch
        | F::XeniaPatch
        | F::Vita3kPatch
        | F::Pchtxt
        | F::ExefsPatches => "Emulator patches",
        _ => addon_kind_name(kind),
    }
}

/// How an addon was linked to its game (spec §10).
fn via_text(via: LinkVia, key: &str) -> String {
    match via {
        LinkVia::SourceCrc => "linked by the source ROM's CRC32".into(),
        LinkVia::Dat => "linked by CRC32 and the DAT".into(),
        LinkVia::Name => "linked by its name".into(),
        LinkVia::Readme => "linked by its readme".into(),
        LinkVia::Key => format!("linked by {}", table::one_line(key)),
        LinkVia::Folder => "it was already in the game's folder".into(),
    }
}

/// An addon's base, when it moves in this plan into the same system
/// folder: the addon is then printed under it. `by_id` indexes the
/// plan's items by id (built once per render: a pile can hold tens of
/// thousands of items).
fn nested_base<'a>(
    by_id: &HashMap<usize, &'a OrganizeItem>,
    item: &OrganizeItem,
) -> Option<&'a OrganizeItem> {
    let link = item.game.as_ref()?;
    if !matches!(link.role, GameRole::Mod | GameRole::Cheat) {
        return None;
    }
    let base = *by_id.get(&link.of?)?;
    // An addon bound for the emulator's own data folder (an absolute
    // target) still reads under its game; `addon_dest` prints the path.
    let elsewhere = item.target.as_deref().is_some_and(Path::is_absolute);
    (base.action == OrganizeAction::Auto && (elsewhere || system_dir(base) == system_dir(item)))
        .then_some(base)
}

/// Where an addon goes, seen from its game: `Mods/`, `beside it`, or the
/// whole path when it lands elsewhere (an emulator's data folder).
fn addon_dest(item: &OrganizeItem, base: &OrganizeItem) -> String {
    let Some(target) = item.target.as_deref() else {
        return String::new();
    };
    if target.is_absolute() {
        return table::one_line(&target.display().to_string());
    }
    let parent = target.parent().unwrap_or(Path::new(""));
    let game_dir = base
        .game
        .as_ref()
        .and_then(|g| g.folder.clone())
        .or_else(|| {
            base.target
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
        })
        .unwrap_or_default();
    match parent.strip_prefix(&game_dir) {
        Ok(rel) if rel.as_os_str().is_empty() => "beside it".into(),
        Ok(rel) => format!("{}/", slash(rel)),
        Err(_) => format!("{}/", slash(parent)),
    }
}

/// `    + Mod  0004000000055D00/ (folder)  → Mods/  (linked by …)`.
fn addon_row(item: &OrganizeItem, base: &OrganizeItem) -> String {
    let (label, via) = item
        .game
        .as_ref()
        .and_then(|g| g.addon.as_ref())
        .map(|a| (addon_label(a.kind, a.format), via_text(a.via, &a.key_text)))
        .unwrap_or_else(|| {
            let kind = match item.game.as_ref().map(|g| g.role) {
                Some(GameRole::Cheat) => AddonKind::Cheat,
                _ => AddonKind::Mod,
            };
            (addon_kind_name(kind), "linked to its game".into())
        });
    format!(
        "    + {label}  {}  → {}  ({via}){}",
        source_label(item),
        addon_dest(item, base),
        item_extras_with(item, true)
    )
}

/// What a translator says about one placement (mods spec D14, D24).
fn translate_note_text(note: &TranslateNote) -> String {
    match note {
        TranslateNote::InstallInEmulator { emulator } => format!(
            "{} installs it; it does not open from the folder",
            emulator.display_name()
        ),
        TranslateNote::ExtractFirst => {
            "this package does not open as is: it has to be extracted first".into()
        }
        TranslateNote::NotSupported { emulator, why } => format!(
            "{} does not use it like this: {}",
            emulator.display_name(),
            not_supported_text(*why)
        ),
        TranslateNote::HeaderMismatch { needs } => {
            note_text(&ItemNote::HeaderMismatch { needs: *needs })
                .unwrap_or_else(|| "the patch fits the ROM as it is".into())
        }
        TranslateNote::PointEmulatorHere { emulator, folder } => format!(
            "point {} at this folder: {}",
            emulator.display_name(),
            slash(folder)
        ),
        TranslateNote::PatchNotApplied { emulator } => {
            format!("{} would apply it when you play", emulator.display_name())
        }
    }
}

/// The layouts the plan made for an emulator (mods spec D16), one block
/// per emulator and system.
fn translated_text(plan: &OrganizePlan) -> String {
    type Block<'a> = ((EmulatorId, &'a str), Vec<&'a PlacementView>);
    let mut blocks: Vec<Block> = Vec::new();
    for view in &plan.translated {
        let key = (view.emulator, view.system.as_str());
        match blocks.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.extend(&view.placements),
            None => blocks.push((key, view.placements.iter().collect())),
        }
    }
    let mut out = String::new();
    for ((emulator, system), placements) in blocks {
        let _ = writeln!(out, "\nLaid out for {} ({system})", emulator.display_name());
        for pl in placements {
            let to = match &pl.to {
                PlacementTo::System { path } => slash(path),
                PlacementTo::EmulatorData { path } => {
                    format!("{}'s folder: {}", emulator.display_name(), slash(path))
                }
                PlacementTo::Library => "stays in the library".into(),
            };
            let note = pl
                .note
                .as_ref()
                .map(|n| format!("  [{}]", translate_note_text(n)))
                .unwrap_or_default();
            let _ = writeln!(out, "  {} → {to}{note}", slash(&pl.from));
        }
    }
    out
}

/// Why an emulator does not use an item as the library keeps it.
fn not_supported_text(why: NotSupportedWhy) -> &'static str {
    match why {
        NotSupportedWhy::Rpcs3Mods => "no mods folder",
        NotSupportedWhy::VitaRepatch => "it does not read rePatch",
        NotSupportedWhy::SeveralPatches => "several patches for one ROM; only one is applied",
        NotSupportedWhy::OlderUpdate => "only the newest update is used",
        NotSupportedWhy::ConfigExists => "a settings file with that name already exists",
    }
}

/// A fixed folder's name as on disk: English, never translated.
fn fixed_folder_name(folder: FixedFolder) -> &'static str {
    match folder {
        FixedFolder::Updates => "Updates",
        FixedFolder::Dlc => "DLC",
        FixedFolder::Mods => "Mods",
        FixedFolder::Cheats => "Cheats",
        FixedFolder::Extras => "Extras",
        FixedFolder::Saves => "Saves",
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
        RenameReason::GameFolder { from } => format!("renamed for its game folder (was {from})"),
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
        ItemNote::ExtractFirst => {
            "this package does not open as is: it has to be extracted first".into()
        }
        ItemNote::NoEmulator => "no emulator opens it in this form".into(),
        ItemNote::ProEnhanced => "enhanced for PS5 Pro".into(),
        ItemNote::DlcFolder { layout } => {
            format!("goes to the DLC folder, where {layout} looks for downloadable content")
        }
        ItemNote::Homebrew => "homebrew".into(),
        ItemNote::Vita7z => "Vita3K only installs .zip, .vpk or .vci".into(),
        ItemNote::MissingDisc { n } => format!("disc {n} is missing"),
        ItemNote::DiscSet => "looks like a multi-disc game".into(),
        ItemNote::EncryptedName => {
            "encrypted; the name cannot be read without decrypting it".into()
        }
        ItemNote::ArchiveGames { count } => format!("the archive holds {count} games"),
        ItemNote::Ps4OrVita => "param.sfo must be read to tell PS4 from Vita".into(),
        ItemNote::ArchiveCaseCollision => {
            "two names in the archive differ only in case; extracting them would collide".into()
        }
        ItemNote::DemoOf { of } => format!("demo of {of}"),
        ItemNote::BaseMissing => "its game is not here or in the library".into(),
        ItemNote::ModOf { of } => format!("mod for {of}"),
        ItemNote::PatchOf { of } => format!("patch for {of}"),
        ItemNote::CheatOf { of } => format!("cheats for {of}"),
        ItemNote::HeaderMismatch { needs } => match needs {
            HeaderFit::NeedsHeaderless => "this patch expects the ROM without a header".into(),
            HeaderFit::NeedsHeadered => "this patch expects the ROM with a header".into(),
            HeaderFit::Matches => return None,
        },
        ItemNote::PointEmulatorHere { emulator, folder } => {
            format!("point {emulator} at this folder: {folder}")
        }
        ItemNote::RegionLetterUnknown { letter } => {
            format!("region letter “{letter}” not identified; no region is added to the name")
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
        PlanWarning::LargeTransfer { bytes, .. } => format!(
            "{} will be written to the target; you will be asked to confirm",
            table::human_bytes(*bytes)
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
    source_label_of(&item.sources, item.kind)
}

/// `Zelda/ (folder)` for a folder game, `Game.cue (+2 files)` for a set.
fn source_label_of(sources: &[PathBuf], kind: ItemKind) -> String {
    let first = sources.first().map(|p| slash(p)).unwrap_or_default();
    if kind == ItemKind::Folder {
        return format!("{first}/ (folder)");
    }
    match sources.len() {
        0 | 1 => first,
        n => format!("{first} (+{} files)", n - 1),
    }
}

fn item_extras(item: &OrganizeItem) -> String {
    item_extras_with(item, false)
}

/// [`item_extras`]; `nested`: an addon row printed under its game, whose
/// own line already says what it is and whose game it is.
fn item_extras_with(item: &OrganizeItem, nested: bool) -> String {
    let mut extras: Vec<String> = Vec::new();
    let addon = item.game.as_ref().and_then(|g| g.addon.as_ref());
    if let (Some(a), false) = (addon, nested) {
        extras.push(format!(
            "{}, {}",
            addon_label(a.kind, a.format),
            via_text(a.via, &a.key_text)
        ));
    }
    if let Some(r) = &item.rename {
        extras.push(rename_text(r));
    }
    extras.extend(
        item.notes
            .iter()
            .filter(|n| {
                !nested
                    || !matches!(
                        n,
                        ItemNote::ModOf { .. }
                            | ItemNote::PatchOf { .. }
                            | ItemNote::CheatOf { .. }
                    )
            })
            .filter_map(note_text),
    );
    if !item.companions.is_empty() {
        extras.push(format!("with {} companion file(s)", item.companions.len()));
    }
    for licence in item.companions.iter().filter(|c| c.licence) {
        let name = licence
            .source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| slash(&licence.source));
        extras.push(format!("licence {name} goes with it"));
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
    out.push_str(&licences_left_text(report));
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

/// One line per licence whose rename failed (game folders spec D17): it
/// stays beside the pile although the plan said it went with its game.
fn licences_left_text(report: &OrganizeReport) -> String {
    let mut out = String::new();
    for path in report.items.iter().flat_map(|o| &o.licences_left) {
        let name = path.file_name().unwrap_or(path.as_os_str());
        let _ = writeln!(
            out,
            "  {}  licence stayed where it was: only a rename moves it",
            name.to_string_lossy()
        );
    }
    out
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

/// The items that really stay where they are: every left item except a
/// licence that moves with its game (mods/cheats spec D22), which its
/// game's row names instead.
fn staying(plan: &OrganizePlan) -> Vec<(&OrganizeItem, &LeftReason)> {
    let moving_licences: Vec<&Path> = plan
        .items
        .iter()
        .filter(|i| (i.action == OrganizeAction::Auto && i.target.is_some()) || i.is_set_aside())
        .flat_map(|i| &i.companions)
        .filter(|c| c.licence)
        .map(|c| c.source.as_path())
        .collect();
    plan.items
        .iter()
        .filter_map(|i| match &i.action {
            OrganizeAction::Left { reason } => Some((i, reason)),
            _ => None,
        })
        .filter(|(i, _)| {
            i.sources
                .first()
                .is_none_or(|s| !moving_licences.contains(&s.as_path()))
        })
        .collect()
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
    // Counted as listed below: a licence that goes with its game is not
    // staying put (D22), although the plan counts it as left.
    let left = staying(plan);
    let _ = writeln!(
        out,
        "ROMs: {} · Systems: {} · To review: {} · Staying put: {}",
        c.roms,
        c.systems,
        c.review,
        left.len()
    );
    if c.set_aside + c.in_library > 0 {
        let _ = writeln!(
            out,
            "Duplicates set aside: {} · Already in your library: {}",
            c.set_aside, c.in_library
        );
    }

    // Moving items, grouped by their system folder.
    let by_id: HashMap<usize, &OrganizeItem> = plan.items.iter().map(|i| (i.id, i)).collect();
    let mut groups: BTreeMap<String, Vec<&OrganizeItem>> = BTreeMap::new();
    for item in &plan.items {
        if item.action != OrganizeAction::Auto {
            continue;
        }
        // An addon is grouped with its game, wherever its own target is.
        let folder = nested_base(&by_id, item).map_or_else(|| system_dir(item), system_dir);
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
        // Addons printed under their base, in plan order.
        let mut addons_of: HashMap<usize, Vec<&OrganizeItem>> = HashMap::new();
        for item in items {
            if let Some(base) = nested_base(&by_id, item) {
                addons_of.entry(base.id).or_default().push(item);
            }
        }
        for item in items {
            if nested_base(&by_id, item).is_some() {
                continue;
            }
            let why = item.why.first().map(why_text).unwrap_or_default();
            let _ = writeln!(
                out,
                "  {}  {}{}  {why}{}",
                action_label(item),
                confidence_label(item, opts),
                version_column(item),
                item_extras(item)
            );
            for addon in addons_of.get(&item.id).into_iter().flatten() {
                let _ = writeln!(out, "{}", addon_row(addon, item));
            }
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

    out.push_str(&translated_text(plan));

    if !left.is_empty() {
        let _ = writeln!(out, "\nStaying where they are ({})", left.len());
        for (item, reason) in left {
            let _ = writeln!(
                out,
                "  {}  {}{}",
                source_label(item),
                left_text_for(item, reason),
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

    fn transfer(free: Option<u64>, huge: Vec<HugeItem>, rest: u64) -> LargeTransfer {
        LargeTransfer {
            bytes: rest + huge.iter().map(|h| h.bytes).sum::<u64>(),
            items: huge.len() + 1,
            copies: true,
            free,
            volume: "TinyExt".into(),
            huge,
            rest,
        }
    }

    fn huge(item: usize, name: &str, bytes: u64, extract: bool) -> HugeItem {
        HugeItem {
            item,
            name: name.into(),
            system: "ps4".into(),
            bytes,
            extract,
            followers: Vec::new(),
        }
    }

    #[test]
    fn answers_parse_short_and_long_forms_only() {
        assert_eq!(Answer::parse(" Y\n"), Some(Answer::Yes));
        assert_eq!(Answer::parse("all"), Some(Answer::All));
        assert_eq!(Answer::parse("No"), Some(Answer::No));
        assert_eq!(Answer::parse("si"), None);
        assert_eq!(Answer::parse(""), None);
    }

    #[test]
    fn item_questions_show_size_and_what_is_left() {
        let h = huge(3, "Bloodborne.zip", 40_000_000_000, true);
        let t = transfer(Some(100_000_000_000), vec![h.clone()], 0);
        let (q, fits) = item_question(&t, &h, 20_000_000_000);
        assert!(fits);
        // 60 GB + 1 % margin out of 100 GB.
        assert_eq!(
            q,
            "Extract \"Bloodborne.zip\" (40.0 GB; 39.4 GB free on TinyExt after this)? [y]es / [a]ll / [n]o:"
        );
        let (q, fits) = item_question(&t, &h, 70_000_000_000);
        assert!(!fits);
        assert_eq!(
            q,
            "Extract \"Bloodborne.zip\" (40.0 GB; does not fit on TinyExt: 11.1 GB short)? [n]o:"
        );
        let copy = huge(4, "Game\nX.iso", 12_000_000_000, false);
        let t = transfer(None, vec![copy.clone()], 0);
        assert_eq!(
            item_question(&t, &copy, 0).0,
            "Copy \"Game\u{FFFD}X.iso\" (12.0 GB)? [y]es / [a]ll / [n]o:"
        );
    }

    /// Answers in order, `None` once they run out (EOF).
    struct Script(Vec<Answer>, Vec<String>);

    impl Prompt for Script {
        fn ask(&mut self, question: &str) -> Option<Answer> {
            self.1.push(question.to_string());
            (!self.0.is_empty()).then(|| self.0.remove(0))
        }
    }

    #[test]
    fn an_item_that_does_not_fit_accepts_only_no() {
        let t = transfer(Some(10), vec![huge(7, "Big.iso", 50, false)], 0);
        // «Yes» and «all» are asked again; then EOF.
        let mut p: Option<Box<dyn Prompt>> =
            Some(Box::new(Script(vec![Answer::Yes, Answer::All], Vec::new())));
        assert!(answer_items(&t, None, &mut p, &Answers::default()).is_none());
        let mut p: Option<Box<dyn Prompt>> =
            Some(Box::new(Script(vec![Answer::Yes, Answer::No], Vec::new())));
        let got = answer_items(&t, None, &mut p, &Answers::default()).unwrap();
        assert!(got.accepted.is_empty());
        assert_eq!(got.declined.len(), 1);
        assert_eq!(got.declined[0].item, 7);
        assert_eq!(got.declined[0].choice, Choice::Decline);
    }

    #[test]
    fn yes_to_all_answers_the_later_items_without_asking() {
        let t = transfer(
            None,
            vec![
                huge(1, "A", 9, false),
                huge(2, "B", 9, false),
                huge(3, "C", 9, false),
            ],
            0,
        );
        let mut p: Option<Box<dyn Prompt>> =
            Some(Box::new(Script(vec![Answer::No, Answer::All], Vec::new())));
        let got = answer_items(&t, None, &mut p, &Answers::default()).unwrap();
        assert_eq!(got.accepted, vec![2, 3]);
        assert!(got.all);
        // `--skip-large` asks nothing, even without a prompt.
        let got = answer_items(&t, Some(Answer::No), &mut None, &Answers::default()).unwrap();
        assert_eq!(got.declined.len(), 3);
        assert!(answer_items(&t, None, &mut None, &Answers::default()).is_none());
    }

    #[test]
    fn a_later_round_asks_only_the_items_not_answered_yet() {
        let t = transfer(
            Some(100),
            vec![
                huge(1, "A", 30, false),
                huge(2, "B", 30, false),
                huge(3, "C", 30, false),
            ],
            0,
        );
        let before = Answers {
            accepted: vec![1],
            declined: vec![Decision {
                item: 2,
                choice: Choice::Decline,
            }],
            all: false,
        };
        // One answer only: a second question would read EOF (`None`).
        let mut p: Option<Box<dyn Prompt>> = Some(Box::new(Script(vec![Answer::Yes], Vec::new())));
        let got = answer_items(&t, None, &mut p, &before).unwrap();
        assert_eq!(got.accepted, vec![3]);
        assert!(got.declined.is_empty());
        // C is weighed after A: 60 B + 1 % of 100 B leaves 39 B; with B
        // too it would not fit and take only «no».
        let c = &t.huge[2];
        assert!(
            item_question(&t, c, 30).0.contains("39 B free"),
            "{}",
            item_question(&t, c, 30).0
        );
        let mut p: Option<Box<dyn Prompt>> = None;
        // «All» in an earlier round answers the new items without asking.
        let before = Answers {
            all: true,
            ..before
        };
        let got = answer_items(&t, None, &mut p, &before).unwrap();
        assert_eq!(got.accepted, vec![3]);
    }

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
    fn folder_games_read_as_folders() {
        assert_eq!(
            source_label_of(&[PathBuf::from("Zelda")], ItemKind::Folder),
            "Zelda/ (folder)"
        );
        assert_eq!(
            source_label_of(
                &[PathBuf::from("a.cue"), PathBuf::from("a.bin")],
                ItemKind::Set
            ),
            "a.cue (+1 files)"
        );
        assert_eq!(
            source_label_of(&[PathBuf::from("a.gb")], ItemKind::File),
            "a.gb"
        );
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
    fn a_folder_copy_prints_its_start_and_end_only() {
        let gate = ProgressGate::default();
        let folder = |bytes_done| OrganizeProgress {
            phase: OrganizePhase::Folder {
                item: 2,
                bytes_done,
                bytes_total: 900,
            },
            done: 1,
            total: 4,
        };
        let now = Instant::now();
        assert!(gate.due(folder(0), now));
        assert!(!gate.due(folder(450), now));
        assert!(gate.due(folder(900), now));
        assert_eq!(
            progress_text(folder(450)).as_deref(),
            Some("copy folder item 2/4 bytes 450/900")
        );
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
                "key or licence file: never read, left here",
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
            (
                LeftReason::NotARom {
                    what: NotARom::PcGameData,
                },
                "not a ROM (PC game data)",
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

    #[test]
    fn adjustment_texts() {
        for (reason, text) in [
            (
                LeftReason::GamePart {
                    part: PartKind::ExeFs,
                    system: "switch".into(),
                    title_id: None,
                },
                "part of a game; not moved on its own",
            ),
            (
                LeftReason::GamePart {
                    part: PartKind::Backup,
                    system: "psvita".into(),
                    title_id: Some("PCSE00120".into()),
                },
                "encrypted Vita backup; not moved on its own",
            ),
            (LeftReason::NotConfirmed, "not confirmed; left where it is"),
        ] {
            assert_eq!(left_text(&reason), text);
        }
        for (note, text) in [
            (ItemNote::Homebrew, "homebrew"),
            (ItemNote::Vita7z, "Vita3K only installs .zip, .vpk or .vci"),
            (ItemNote::MissingDisc { n: 2 }, "disc 2 is missing"),
            (ItemNote::DiscSet, "looks like a multi-disc game"),
            (
                ItemNote::EncryptedName,
                "encrypted; the name cannot be read without decrypting it",
            ),
        ] {
            assert_eq!(note_text(&note).as_deref(), Some(text));
        }
    }

    #[test]
    fn current_generation_texts() {
        assert_eq!(
            left_text(&LeftReason::GameOverlay {
                of: "Bloodborne".into()
            }),
            "mods or patch folder of Bloodborne: left where it is"
        );
        assert!(left_text(&LeftReason::NotARom {
            what: NotARom::ConsoleSystem
        })
        .contains("console system file"));
        let notes = [
            (
                ItemNote::ExtractFirst,
                "this package does not open as is: it has to be extracted first",
            ),
            (ItemNote::NoEmulator, "no emulator opens it in this form"),
            (ItemNote::ProEnhanced, "enhanced for PS5 Pro"),
            (
                ItemNote::DlcFolder {
                    layout: "Batocera".into(),
                },
                "goes to the DLC folder, where Batocera looks for downloadable content",
            ),
        ];
        for (note, text) in notes {
            assert_eq!(note_text(&note).as_deref(), Some(text));
        }
        assert_eq!(
            warning_text(&PlanWarning::LargeTransfer {
                bytes: 12_000_000_000,
                items: 2
            }),
            "12.0 GB will be written to the target; you will be asked to confirm"
        );
    }

    fn test_item(id: usize, name: &str) -> OrganizeItem {
        OrganizeItem {
            id,
            sources: vec![name.into()],
            kind: ItemKind::File,
            size: 1,
            fps: Vec::new(),
            target: None,
            set_targets: Vec::new(),
            system_id: None,
            folder: None,
            confidence: 0.95,
            method: None,
            why: Vec::new(),
            alternatives: Vec::new(),
            rename: None,
            notes: Vec::new(),
            companions: Vec::new(),
            catalogue_name: None,
            transfer: Transfer::AsIs,
            disc: None,
            title_id: None,
            content_id: None,
            playlist: None,
            title_source: None,
            game: None,
            action: OrganizeAction::Auto,
        }
    }

    fn plan_with_items(items: Vec<OrganizeItem>) -> OrganizePlan {
        OrganizePlan {
            source_root: PathBuf::from("pile"),
            target_root: PathBuf::from("lib"),
            roms_dir: PathBuf::from("lib"),
            layout: "coverly-library".into(),
            mode: TransferMode::Move,
            names: NameMode::Keep,
            items,
            counts: Default::default(),
            sorted_folders: Vec::new(),
            deep_folders: Vec::new(),
            other_files: Vec::new(),
            warnings: Vec::new(),
            broad_root: false,
            fat32: false,
            move_allowed: true,
            target_detected: None,
            created_at: String::new(),
            duplicates: Vec::new(),
            set_aside_trash: false,
            updates: 0,
            integration_notices: Vec::new(),
            target_anchor: None,
            translated: Vec::new(),
            emulator_roots: Vec::new(),
        }
    }

    #[test]
    fn a_licence_that_moves_with_its_game_is_not_staying() {
        use coverly_core::organize::Companion;
        let p = std::path::Path::new;
        let mut game = test_item(0, "Resistance.pkg");
        game.action = OrganizeAction::Auto;
        game.target = Some(p("PS3").join("Resistance").join("Resistance.pkg"));
        game.companions.push(Companion {
            source: p("BLUS30001.rap").into(),
            target: p("PS3").join("Resistance").join("BLUS30001.rap"),
            licence: true,
        });
        let mut rap = test_item(1, "BLUS30001.rap");
        rap.action = OrganizeAction::Left {
            reason: LeftReason::LicenceOf {
                of: p("Resistance.pkg").into(),
            },
        };
        let mut key = test_item(2, "prod.keys");
        key.action = OrganizeAction::Left {
            reason: LeftReason::KeyMaterial,
        };
        let mut plan = plan_with_items(vec![game, rap, key]);
        // As the core counts them: both left items.
        plan.counts.left = 2;
        let names: Vec<String> = staying(&plan)
            .iter()
            .map(|(i, _)| source_label(i))
            .collect();
        assert_eq!(names, vec!["prod.keys".to_string()]);
        let table = render_table(
            &plan,
            &SystemDb::builtin(),
            &OrganizeOptions::default(),
            true,
        );
        assert!(
            table.contains("licence BLUS30001.rap goes with it"),
            "{table}"
        );
        assert!(table.contains("Staying where they are (1)"), "{table}");
        // The summary line agrees with the list below it.
        assert!(table.contains("Staying put: 1"), "{table}");
        assert!(!table.contains("licence of Resistance.pkg"), "{table}");
    }

    #[test]
    fn a_licence_whose_game_stays_is_still_listed_as_staying() {
        use coverly_core::organize::Companion;
        let p = std::path::Path::new;
        let mut game = test_item(0, "Resistance.pkg");
        game.action = OrganizeAction::Review;
        game.companions.push(Companion {
            source: p("BLUS30001.rap").into(),
            target: p("PS3").join("Resistance").join("BLUS30001.rap"),
            licence: true,
        });
        let mut rap = test_item(1, "BLUS30001.rap");
        rap.action = OrganizeAction::Left {
            reason: LeftReason::LicenceOf {
                of: p("Resistance.pkg").into(),
            },
        };
        let plan = plan_with_items(vec![game, rap]);
        let names: Vec<String> = staying(&plan)
            .iter()
            .map(|(i, _)| source_label(i))
            .collect();
        assert_eq!(names, vec!["BLUS30001.rap".to_string()]);
    }

    #[test]
    fn a_licence_left_behind_is_named_in_the_report() {
        let report = OrganizeReport {
            items: vec![ItemOutcome {
                item: 0,
                status: ItemStatus::Moved,
                targets: vec![std::path::Path::new("lib").join("Resistance.pkg")],
                licences_left: vec![std::path::Path::new("pile").join("BLUS30001.rap")],
            }],
            ..OrganizeReport::default()
        };
        assert_eq!(
            licences_left_text(&report),
            "  BLUS30001.rap  licence stayed where it was: only a rename moves it\n"
        );
        assert_eq!(licences_left_text(&OrganizeReport::default()), "");
    }

    #[test]
    fn game_folder_texts() {
        use coverly_core::consoles::FixedFolder;
        assert_eq!(
            left_text(&LeftReason::LicenceOf {
                of: std::path::Path::new("ps3").join("Resistance.pkg"),
            }),
            "licence of Resistance.pkg: goes with it, never read"
        );
        assert_eq!(
            left_text(&LeftReason::GameFolderContent {
                folder: FixedFolder::Mods,
                of: "Pokemon Y".into(),
            }),
            "kept in place: the Mods folder of Pokemon Y stays for now"
        );
        assert_eq!(
            left_text(&LeftReason::GameFolderContent {
                folder: FixedFolder::Dlc,
                of: "Pokemon Y".into(),
            }),
            "kept in place: the DLC folder of Pokemon Y stays for now"
        );
        assert_eq!(
            left_text(&LeftReason::NotARom {
                what: NotARom::EmulatorCache
            }),
            "not a ROM (emulator cache)"
        );
        assert_eq!(
            left_text(&LeftReason::NotARom {
                what: NotARom::EmulatorConfig
            }),
            "not a ROM (emulator settings)"
        );
        assert_eq!(
            note_text(&ItemNote::DemoOf {
                of: "Pokemon Y".into()
            })
            .as_deref(),
            Some("demo of Pokemon Y")
        );
        assert_eq!(
            note_text(&ItemNote::BaseMissing).as_deref(),
            Some("its game is not here or in the library")
        );
        assert_eq!(
            rename_text(&RenameReason::GameFolder {
                from: "pky.3ds".into()
            }),
            "renamed for its game folder (was pky.3ds)"
        );
    }

    #[test]
    fn addon_and_emulator_texts() {
        use coverly_core::addons::{AddonFormat, AddonKind, HeaderFit, UnlinkedWhy};
        use coverly_core::organize::NotSupportedWhy;
        let unlinked = |why| LeftReason::AddonUnlinked {
            kind: AddonKind::Patch,
            format: AddonFormat::Ips,
            why,
        };
        assert_eq!(
            left_text(&unlinked(UnlinkedWhy::NoMatch)),
            "its game is unknown: stays here (Translation patch, IPS)"
        );
        assert_eq!(
            left_text(&unlinked(UnlinkedWhy::NoKey)),
            "its game is unknown: stays here (Translation patch, IPS)"
        );
        assert_eq!(
            left_text(&unlinked(UnlinkedWhy::Several { count: 2 })),
            "fits 2 games: stays here"
        );
        assert_eq!(
            left_text(&LeftReason::AddonUnlinked {
                kind: AddonKind::Cheat,
                format: AddonFormat::Rpcs3Patch,
                why: UnlinkedWhy::Many { count: 400 },
            }),
            "holds patches for 400 games: stays here"
        );
        assert_eq!(
            left_text(&unlinked(UnlinkedWhy::Budget)),
            "its game could not be checked in time: stays here (Translation patch, IPS)"
        );
        assert_eq!(
            left_text(&LeftReason::AddonUnlinked {
                kind: AddonKind::Mod,
                format: AddonFormat::Msu1,
                why: UnlinkedWhy::Partial,
            }),
            "only part of a mod: stays here (Mod, MSU-1)"
        );
        assert_eq!(
            left_text(&LeftReason::AddonNameTaken {
                at: Path::new("PS1").join("Game").join("Cheats"),
            }),
            "a file with that name is already in its folder"
        );
        assert_eq!(
            left_text(&LeftReason::AddonNoPlace {
                kind: AddonKind::Mod
            }),
            "this layout has no folder for its game's mods: stays here"
        );
        assert_eq!(
            left_text(&LeftReason::AddonNoPlace {
                kind: AddonKind::Cheat
            }),
            "this layout has no folder for its game's cheats: stays here"
        );
        assert_eq!(
            left_text(&LeftReason::AddonForEmulator {
                emulator: "DuckStation".into(),
                path: Path::new("cheats").join("SLUS-00594.cht"),
            }),
            "goes in DuckStation's folder: cheats/SLUS-00594.cht (pass --emulator-data)"
        );
        assert_eq!(
            left_text(&LeftReason::NotSupported {
                emulator: "RPCS3".into(),
                why: NotSupportedWhy::Rpcs3Mods,
            }),
            "RPCS3 does not use it like this: no mods folder"
        );
        assert_eq!(
            left_text(&LeftReason::NotSupported {
                emulator: "Vita3K".into(),
                why: NotSupportedWhy::VitaRepatch,
            }),
            "Vita3K does not use it like this: it does not read rePatch"
        );
        let patch = LeftReason::PatchNotApplied {
            emulator: "RetroArch".into(),
        };
        let mut item = test_item(0, "Chrono Trigger.ips");
        item.system_id = Some("snes".into());
        assert_eq!(
            left_text_for(&item, &patch),
            "RetroArch would apply it when you play: pass --emulator snes=retroarch"
        );
        assert_eq!(
            left_text(&patch),
            "RetroArch would apply it when you play: pass --emulator <system>=retroarch"
        );
        assert_eq!(
            note_text(&ItemNote::ModOf { of: "Zelda".into() }).as_deref(),
            Some("mod for Zelda")
        );
        assert_eq!(
            note_text(&ItemNote::PatchOf { of: "Zelda".into() }).as_deref(),
            Some("patch for Zelda")
        );
        assert_eq!(
            note_text(&ItemNote::CheatOf { of: "Zelda".into() }).as_deref(),
            Some("cheats for Zelda")
        );
        assert_eq!(
            note_text(&ItemNote::HeaderMismatch {
                needs: HeaderFit::NeedsHeaderless
            })
            .as_deref(),
            Some("this patch expects the ROM without a header")
        );
        assert_eq!(
            note_text(&ItemNote::HeaderMismatch {
                needs: HeaderFit::NeedsHeadered
            })
            .as_deref(),
            Some("this patch expects the ROM with a header")
        );
        assert_eq!(
            note_text(&ItemNote::HeaderMismatch {
                needs: HeaderFit::Matches
            }),
            None
        );
        assert_eq!(
            note_text(&ItemNote::PointEmulatorHere {
                emulator: "shadPS4".into(),
                folder: "addcont".into(),
            })
            .as_deref(),
            Some("point shadPS4 at this folder: addcont")
        );
        assert_eq!(
            note_text(&ItemNote::RegionLetterUnknown { letter: 'T' }).as_deref(),
            Some("region letter “T” not identified; no region is added to the name")
        );
        assert_eq!(
            addon_format_name(AddonFormat::CemuGraphicPack),
            "Cemu graphic pack"
        );
    }

    #[test]
    fn emulator_flags_parse_system_and_emulator() {
        use coverly_core::organize::EmulatorChoice;
        let db = SystemDb::builtin();
        assert_eq!(
            parse_emulator("ps4=shadps4", &db).unwrap(),
            (
                "ps4".to_string(),
                EmulatorChoice::Emulator(EmulatorId::Shadps4)
            )
        );
        assert_eq!(
            parse_emulator("PS4=None", &db).unwrap(),
            ("ps4".to_string(), EmulatorChoice::None)
        );
        assert_eq!(
            parse_emulator("snes=RetroArch", &db).unwrap(),
            (
                "snes".to_string(),
                EmulatorChoice::Emulator(EmulatorId::RetroArch)
            )
        );
        let e = parse_emulator("nosuch=shadps4", &db).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(
            e.message.contains("unknown system `nosuch`"),
            "{}",
            e.message
        );
        assert!(e.message.contains("ps4"), "{}", e.message);
        assert!(e.message.contains("3ds"), "{}", e.message);
        let e = parse_emulator("ps4=nosuch", &db).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(
            e.message.contains("unknown emulator `nosuch`"),
            "{}",
            e.message
        );
        assert!(e.message.contains("shadps4"), "{}", e.message);
        assert!(e.message.contains("none"), "{}", e.message);
        let e = parse_emulator("ps3=shadps4", &db).unwrap_err();
        assert_eq!(e.code, 2);
        assert_eq!(
            e.message,
            "shadps4 does not run ps3 (--emulator ps3=shadps4); use one of: rpcs3, none"
        );
        for bad in ["ps4", "=shadps4", "ps4="] {
            assert_eq!(parse_emulator(bad, &db).unwrap_err().code, 2, "{bad}");
        }
    }

    #[test]
    fn emulator_data_flags_need_a_known_emulator_and_an_absolute_path() {
        let abs = std::env::temp_dir().join("ds=data");
        let arg = format!("duckstation={}", abs.display());
        assert_eq!(
            parse_emulator_data(&arg).unwrap(),
            (EmulatorId::DuckStation, abs)
        );
        let e = parse_emulator_data("duckstation=rel/ds").unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.message.contains("absolute"), "{}", e.message);
        let e =
            parse_emulator_data(&format!("nosuch={}", std::env::temp_dir().display())).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(
            e.message.contains("unknown emulator `nosuch`"),
            "{}",
            e.message
        );
        assert!(!e.message.contains("none"), "{}", e.message);
        assert_eq!(parse_emulator_data("duckstation").unwrap_err().code, 2);
    }

    fn addon_item(
        id: usize,
        name: &str,
        role: GameRole,
        of: usize,
        target: PathBuf,
    ) -> OrganizeItem {
        use coverly_core::addons::{AddonLink, LinkVia};
        let mut it = test_item(id, name);
        it.system_id = Some("3ds".into());
        it.folder = Some("3DS".into());
        it.target = Some(target);
        let (kind, format) = match role {
            GameRole::Cheat => (AddonKind::Cheat, AddonFormat::CtrCheats),
            _ => (AddonKind::Mod, AddonFormat::LayeredFs),
        };
        it.notes.push(ItemNote::ModOf {
            of: "Pokemon Y".into(),
        });
        let mut link = GameLink::new(role);
        link.of = Some(of);
        link.addon = Some(AddonLink {
            kind,
            format,
            key_text: "0004000000055D00".into(),
            via: LinkVia::Key,
            header: None,
        });
        it.game = Some(link);
        it
    }

    #[test]
    fn addons_are_listed_under_their_base_with_kind_and_key() {
        let p = std::path::Path::new;
        let game = p("3DS").join("Pokemon Y (World)");
        let mut base = test_item(0, "Pokemon Y (World).cia");
        base.system_id = Some("3ds".into());
        base.folder = Some("3DS".into());
        base.target = Some(game.join("Pokemon Y (World).cia"));
        let mut cheat = addon_item(
            1,
            "0004000000055D00.txt",
            GameRole::Cheat,
            0,
            game.join("Cheats").join("0004000000055D00.txt"),
        );
        cheat.notes = vec![ItemNote::CheatOf {
            of: "Pokemon Y".into(),
        }];
        let mut m = addon_item(
            2,
            "0004000000055D00",
            GameRole::Mod,
            0,
            game.join("Mods").join("0004000000055D00"),
        );
        m.kind = ItemKind::Folder;
        // Listed before its base in the plan: still printed under it.
        let plan = plan_with_items(vec![m, base, cheat]);
        let table = render_table(
            &plan,
            &SystemDb::builtin(),
            &OrganizeOptions::default(),
            true,
        );
        let lines: Vec<&str> = table.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.contains("Pokemon Y (World).cia →"))
            .unwrap_or_else(|| panic!("{table}"));
        assert_eq!(
            lines[at + 1],
            "    + Mod  0004000000055D00/ (folder)  → Mods/  (linked by 0004000000055D00)",
            "{table}"
        );
        assert_eq!(
            lines[at + 2],
            "    + Cheats  0004000000055D00.txt  → Cheats/  (linked by 0004000000055D00)",
            "{table}"
        );
        assert!(!table.contains("mod for Pokemon Y"), "{table}");
        assert!(table.contains("3DS/  (3)"), "{table}");
    }

    #[test]
    fn an_addon_for_the_emulator_data_folder_stays_under_its_base() {
        let p = std::path::Path::new;
        let game = p("3DS").join("Pokemon Y (World)");
        let mut base = test_item(0, "Pokemon Y (World).cia");
        base.system_id = Some("3ds".into());
        base.folder = Some("3DS".into());
        base.target = Some(game.join("Pokemon Y (World).cia"));
        let data = std::env::temp_dir().join("azahar-data");
        let target = data.join("cheats").join("0004000000055D00.txt");
        let mut cheat = addon_item(
            1,
            "0004000000055D00.txt",
            GameRole::Cheat,
            0,
            target.clone(),
        );
        cheat.notes = vec![ItemNote::CheatOf {
            of: "Pokemon Y".into(),
        }];
        let plan = plan_with_items(vec![base, cheat]);
        let table = render_table(
            &plan,
            &SystemDb::builtin(),
            &OrganizeOptions::default(),
            true,
        );
        let lines: Vec<&str> = table.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.contains("Pokemon Y (World).cia →"))
            .unwrap_or_else(|| panic!("{table}"));
        assert_eq!(
            lines[at + 1],
            format!(
                "    + Cheats  0004000000055D00.txt  → {}  (linked by 0004000000055D00)",
                table::one_line(&target.display().to_string())
            ),
            "{table}"
        );
        // No group of its own named after the absolute folder.
        assert!(table.contains("3DS/  (2)"), "{table}");
        assert!(!table.contains("cheats/  (1)"), "{table}");
    }

    #[test]
    fn an_addon_whose_base_is_not_in_the_plan_keeps_its_own_row() {
        let p = std::path::Path::new;
        let mut cheat = addon_item(
            0,
            "SLUS-00594.cht",
            GameRole::Cheat,
            7,
            p("PS1").join("Game").join("Cheats").join("SLUS-00594.cht"),
        );
        cheat.game.as_mut().unwrap().of = None;
        cheat.notes = vec![ItemNote::CheatOf { of: "Game".into() }];
        let plan = plan_with_items(vec![cheat]);
        let table = render_table(
            &plan,
            &SystemDb::builtin(),
            &OrganizeOptions::default(),
            true,
        );
        let row = table
            .lines()
            .find(|l| l.contains("SLUS-00594.cht →"))
            .unwrap_or_else(|| panic!("{table}"));
        assert!(row.contains("cheats for Game"), "{row}");
        assert!(row.contains("Cheats, linked by 0004000000055D00"), "{row}");
    }

    #[test]
    fn addon_kinds_and_links_read_as_words() {
        use coverly_core::addons::LinkVia;
        assert_eq!(
            addon_label(AddonKind::Patch, AddonFormat::Bps),
            "Translation patch"
        );
        assert_eq!(
            addon_label(AddonKind::Mod, AddonFormat::TexturePack),
            "Texture pack"
        );
        assert_eq!(
            addon_label(AddonKind::Cheat, AddonFormat::Rpcs3Patch),
            "Emulator patches"
        );
        assert_eq!(addon_label(AddonKind::Cheat, AddonFormat::Pnach), "Cheats");
        assert_eq!(
            via_text(LinkVia::SourceCrc, "CRC32 1A2B3C4D"),
            "linked by the source ROM's CRC32"
        );
        assert_eq!(via_text(LinkVia::Dat, "x"), "linked by CRC32 and the DAT");
        assert_eq!(via_text(LinkVia::Name, "x"), "linked by its name");
        assert_eq!(via_text(LinkVia::Readme, "x"), "linked by its readme");
        assert_eq!(via_text(LinkVia::Key, "SLUS-00594"), "linked by SLUS-00594");
        assert_eq!(
            via_text(LinkVia::Folder, "x"),
            "it was already in the game's folder"
        );
    }

    #[test]
    fn translated_layouts_are_listed_per_emulator() {
        use coverly_core::organize::{
            NotSupportedWhy, PlacementTo, PlacementView, TranslateNote, TranslatedGameView,
        };
        let p = std::path::Path::new;
        let mut plan = plan_with_items(Vec::new());
        plan.translated.push(TranslatedGameView {
            system: "ps4".into(),
            base: None,
            emulator: EmulatorId::Shadps4,
            placements: vec![
                PlacementView {
                    from: p("CUSA00900").into(),
                    to: PlacementTo::System {
                        path: p("CUSA00900").into(),
                    },
                    note: None,
                },
                PlacementView {
                    from: p("CUSA00900-DLC-ABC").into(),
                    to: PlacementTo::System {
                        path: p("addcont").join("CUSA00900").join("CUSA00900-DLC-ABC"),
                    },
                    note: Some(TranslateNote::PointEmulatorHere {
                        emulator: EmulatorId::Shadps4,
                        folder: p("addcont").into(),
                    }),
                },
            ],
        });
        plan.translated.push(TranslatedGameView {
            system: "ps3".into(),
            base: None,
            emulator: EmulatorId::Rpcs3,
            placements: vec![
                PlacementView {
                    from: p("Update.pkg").into(),
                    to: PlacementTo::Library,
                    note: Some(TranslateNote::InstallInEmulator {
                        emulator: EmulatorId::Rpcs3,
                    }),
                },
                PlacementView {
                    from: p("Mod").into(),
                    to: PlacementTo::Library,
                    note: Some(TranslateNote::NotSupported {
                        emulator: EmulatorId::Rpcs3,
                        why: NotSupportedWhy::Rpcs3Mods,
                    }),
                },
            ],
        });
        plan.translated.push(TranslatedGameView {
            system: "psx".into(),
            base: None,
            emulator: EmulatorId::DuckStation,
            placements: vec![PlacementView {
                from: p("SLUS-00594.cht").into(),
                to: PlacementTo::EmulatorData {
                    path: p("cheats").join("SLUS-00594.cht"),
                },
                note: None,
            }],
        });
        let table = render_table(
            &plan,
            &SystemDb::builtin(),
            &OrganizeOptions::default(),
            true,
        );
        assert!(table.contains("\nLaid out for shadPS4 (ps4)\n"), "{table}");
        assert!(table.contains("  CUSA00900 → CUSA00900\n"), "{table}");
        assert!(
            table.contains(
                "  CUSA00900-DLC-ABC → addcont/CUSA00900/CUSA00900-DLC-ABC  [point shadPS4 at this folder: addcont]\n"
            ),
            "{table}"
        );
        assert!(table.contains("\nLaid out for RPCS3 (ps3)\n"), "{table}");
        assert!(
            table.contains(
                "  Update.pkg → stays in the library  [RPCS3 installs it; it does not open from the folder]\n"
            ),
            "{table}"
        );
        assert!(
            table.contains(
                "  Mod → stays in the library  [RPCS3 does not use it like this: no mods folder]\n"
            ),
            "{table}"
        );
        assert!(
            table.contains("  SLUS-00594.cht → DuckStation's folder: cheats/SLUS-00594.cht\n"),
            "{table}"
        );
        assert_eq!(
            translate_note_text(&TranslateNote::ExtractFirst),
            "this package does not open as is: it has to be extracted first"
        );
        assert_eq!(
            translate_note_text(&TranslateNote::PatchNotApplied {
                emulator: EmulatorId::RetroArch
            }),
            "RetroArch would apply it when you play"
        );
        assert_eq!(
            translate_note_text(&TranslateNote::HeaderMismatch {
                needs: HeaderFit::NeedsHeadered
            }),
            "this patch expects the ROM with a header"
        );
    }
}
