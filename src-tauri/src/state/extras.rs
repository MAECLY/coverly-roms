//! Extras writes in the app (part D D8, D10, D22): one journaled job per
//! panel action, behind the one-writer guard, on the listing a revision
//! names; the entry's counts refreshed after each write or undo.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Instant, SystemTime};

use coverly_core::extras::index::{IndexState, Listing};
use coverly_core::extras::ops::{
    add_files, create_category, expand_sources, forget, move_to, plan_add, remove_category, rename,
    rename_category, set_tags, trash, AddLimits, AddProgress, AddSource, ExtrasCtx, ExtrasError,
    ExtrasNote, ExtrasOutcome,
};
use coverly_core::extras::CategoryRef;
use coverly_core::organize::apply::user_available;
use coverly_core::scan::{recount_fixed, ScanOptions};
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::TrashBackend;
use coverly_core::write::{job_targets, list_jobs, same_volume, undo_job_with, JobKind};

use super::{absolute_root, check_data_dir, lock, AppState};
use crate::error::IpcError;
use crate::jobs::view::EntriesUpdatedEvent;
use crate::jobs::{FillConfig, OwnedManualWrite};
use crate::library::extras::{
    game_dir, read_listing, read_only, view, AddChoice, AddEnd, AddFinishedEvent, AddPlan,
    AddPlanView, AddPlans, AddProgressEvent, AddStarted, DropEvent, DropPosition, ExtrasAction,
    ExtrasAddEvents, ExtrasCache, ExtrasUndone, ExtrasWriteView, ExtrasWritten, ListedExtras,
    Picker, BACKGROUND_BYTES, BACKGROUND_FILES,
};
use crate::library::refresh::refresh_entries;
use crate::library::view::FolderCountsView;
use crate::library::Session;

fn absolute(path: &Path) -> Result<PathBuf, IpcError> {
    std::path::absolute(path).map_err(|e| IpcError::Io(e.to_string()))
}

/// Runs `action` on `listing` in the context `ctx`.
fn run(
    ctx: &ExtrasCtx,
    listing: &Listing,
    action: ExtrasAction,
) -> Result<ExtrasOutcome, ExtrasError> {
    let idx = |files: &[u32]| files.iter().map(|&i| i as usize).collect::<Vec<_>>();
    match action {
        ExtrasAction::Rename { index, stem } => rename(ctx, listing, index as usize, &stem),
        ExtrasAction::Move { files, to } => move_to(ctx, listing, &idx(&files), &to),
        ExtrasAction::Trash { files } => trash(ctx, listing, &idx(&files)),
        ExtrasAction::SetTags { index, tags } => set_tags(ctx, listing, index as usize, &tags),
        ExtrasAction::Forget { missing } => forget(ctx, listing, missing as usize),
        ExtrasAction::CategoryCreate { name } => create_category(ctx, listing, &name),
        ExtrasAction::CategoryRename { from, to } => rename_category(ctx, listing, &from, &to),
        ExtrasAction::CategoryRemove { name } => remove_category(ctx, listing, &name),
    }
}

impl AppState {
    /// Runs `action` on entry `id`'s listing `rev` (blocking; part D D10,
    /// D22). Refused while a job gathers, applies or is paused or another
    /// write holds the library ([`IpcError::JobRunning`]), for an older
    /// revision or files changed on disk ([`IpcError::ExtrasChanged`]),
    /// for an index from a newer Coverly ([`IpcError::ExtrasNewer`]) and
    /// for an entry that is no game folder or a folder the app cannot
    /// write ([`IpcError::WriteDenied`]); nothing is written then.
    pub fn extras_write(
        &self,
        generation: u64,
        id: u32,
        rev: u64,
        action: ExtrasAction,
    ) -> Result<ExtrasWritten, IpcError> {
        if self.jobs.paused() {
            return Err(IpcError::JobRunning);
        }
        let (_writing, session, cfg) = self.writable_entry(generation, id)?;
        let Target {
            listed,
            dir,
            root,
            game,
        } = self.extras_target(generation, id, rev, &session)?;
        let ctx = ExtrasCtx {
            library_root: &root,
            game_dir: &game,
            data_dir: &cfg.data_dir,
            trash: self.trash.as_ref(),
            now: SystemTime::now(),
            same_volume,
            available: user_available,
        };
        let outcome = run(&ctx, &listed.listing, action)?;
        if let Some(job) = &outcome.job {
            log::info!("extras job {job} for entry {id}");
        }
        let stored = lock(&self.extras).store(generation, id, Some(dir.clone()), outcome.listing);
        let updated = self.refresh_counts(generation, id, &dir);
        let system = &session.entries[id as usize].system_id;
        Ok(ExtrasWritten {
            view: ExtrasWriteView {
                job: outcome.job,
                // The write is done: the guard still held is this write's.
                listing: view(id, system, &stored, false, self.trash.as_ref()),
                notes: outcome.notes,
            },
            updated,
        })
    }

    /// Undoes Extras job `job` of entry `id`'s game (the toast's
    /// «Deshacer», part D D10) with the existing undo: files changed since
    /// stay as they are, archives come back from the Trash. Only an Extras
    /// job of the open library that wrote in this game folder; refused
    /// like a write while anything holds the library.
    pub fn extras_undo(
        &self,
        generation: u64,
        id: u32,
        job: &str,
    ) -> Result<ExtrasUndone, IpcError> {
        if self.jobs.paused() {
            return Err(IpcError::JobRunning);
        }
        let (_writing, session, cfg) = self.writable_entry(generation, id)?;
        let dir = game_dir(&session, id)?.ok_or(IpcError::NothingToUndo)?;
        let root = absolute_root(&session)?;
        let game = absolute(&dir)?;
        let ours = list_jobs(&cfg.data_dir)?
            .iter()
            .any(|j| j.job == job && !j.undone && j.kind == JobKind::Extras && j.root == root)
            && job_targets(&cfg.data_dir, job)?
                .iter()
                .any(|t| t.starts_with(&game));
        if !ours {
            return Err(IpcError::NothingToUndo);
        }
        let report = undo_job_with(&cfg.data_dir, job, self.trash.as_ref())?;
        log::info!("undid extras job {job} for entry {id}");
        let listing = self.relist(generation, &session, id)?;
        let updated = self.refresh_counts(generation, id, &dir);
        Ok(ExtrasUndone {
            job: report.job,
            removed: report.removed.len(),
            restored: report.restored.len(),
            kept: report.skipped_modified.len() + report.skipped_occupied.len(),
            listing,
            updated,
        })
    }

    /// Lists entry `id` again under a new revision (the write is done).
    fn relist(
        &self,
        generation: u64,
        session: &Session,
        id: u32,
    ) -> Result<crate::library::extras::ExtrasView, IpcError> {
        let (dir, listing) = read_listing(session, id)?;
        let stored = lock(&self.extras).store(generation, id, dir, listing);
        let system = &session.entries[id as usize].system_id;
        Ok(view(id, system, &stored, false, self.trash.as_ref()))
    }

    /// Counts game folder `dir`'s fixed folders again and keeps them in
    /// the session (part D D22): the `library-entries-updated` payload for
    /// entry `id`, or `None` once another library is open.
    fn refresh_counts(&self, generation: u64, id: u32, dir: &Path) -> Option<EntriesUpdatedEvent> {
        refresh_counts_in(
            &self.session,
            &self.settings().scan_options(),
            generation,
            id,
            dir,
        )
    }

    /// The listing `rev` of entry `id` and its game folder, once the
    /// write guard is held: refused for an older revision
    /// ([`IpcError::ExtrasChanged`]), an index from a newer Coverly
    /// ([`IpcError::ExtrasNewer`]), an entry that is no game folder or a
    /// folder the app cannot write ([`IpcError::WriteDenied`]).
    fn extras_target(
        &self,
        generation: u64,
        id: u32,
        rev: u64,
        session: &Session,
    ) -> Result<Target, IpcError> {
        let listed = lock(&self.extras).get(generation, id, rev)?;
        let Some(dir) = listed.game_dir.clone() else {
            return Err(IpcError::WriteDenied(
                session.entries[id as usize].rom.path.display().to_string(),
            ));
        };
        if matches!(listed.listing.index, IndexState::Newer { .. }) {
            return Err(IpcError::ExtrasNewer);
        }
        if read_only(&dir) {
            return Err(IpcError::WriteDenied(dir.display().to_string()));
        }
        let root = absolute_root(session)?;
        let game = absolute(&dir)?;
        Ok(Target {
            listed,
            dir,
            root,
            game,
        })
    }

    // ---- Adds from a picker or an OS drop (D11, D13, P-5) ----

    /// Records an OS drop (the window's drag-drop handler, D11): its
    /// paths stay here under a token; the answer is what `extras://drop`
    /// tells the webview (names, sizes, the position).
    pub fn extras_drop(&self, paths: Vec<PathBuf>, position: DropPosition) -> DropEvent {
        lock(&self.drops).record(paths, position, Instant::now())
    }

    /// The game folder of entry `id`, for an add; an entry that is no
    /// game folder is refused before any dialog opens.
    fn add_entry(&self, generation: u64, id: u32) -> Result<Arc<Session>, IpcError> {
        let session = self.known_entry(generation, id)?;
        if game_dir(&session, id)?.is_none() {
            return Err(IpcError::WriteDenied(
                session.entries[id as usize].rom.path.display().to_string(),
            ));
        }
        Ok(session)
    }

    /// Opens `picker` for entry `id` (the command runs it on a blocking
    /// worker, D13) and plans the picked files for the add dialog;
    /// `None` when the user cancels. Reads only, never a key file.
    pub fn extras_add_pick(
        &self,
        generation: u64,
        id: u32,
        picker: &dyn Picker,
    ) -> Result<Option<AddPlanView>, IpcError> {
        self.add_entry(generation, id)?;
        let Some(paths) = picker.pick_files() else {
            return Ok(None);
        };
        self.session(generation)?;
        Ok(Some(self.plan_paths(generation, id, &paths)))
    }

    /// Plans the files of drop `token` for the add dialog (a drop on no
    /// category, D11). The token is spent.
    pub fn extras_add_plan(
        &self,
        generation: u64,
        id: u32,
        token: u64,
    ) -> Result<AddPlanView, IpcError> {
        self.add_entry(generation, id)?;
        let paths = self.drop_paths(token)?;
        Ok(self.plan_paths(generation, id, &paths))
    }

    fn drop_paths(&self, token: u64) -> Result<Vec<PathBuf>, IpcError> {
        lock(&self.drops)
            .take(token, Instant::now())
            .ok_or_else(|| IpcError::ExtrasRefused {
                why: crate::error::extras_refusal::DROP_EXPIRED.into(),
            })
    }

    fn plan_paths(&self, generation: u64, id: u32, paths: &[PathBuf]) -> AddPlanView {
        let files = plan_add(paths);
        let (_, keys, hidden) = expand_sources(paths);
        let mut adds = lock(&self.adds);
        let plan = AddPlan {
            id: adds.next_id(),
            generation,
            entry: id,
            files,
            keys,
            hidden,
        };
        adds.keep(plan).view()
    }

    /// Copies the files of plan `plan` the dialog chose, each to its
    /// category, on listing `rev` (D13). Small adds are done at once;
    /// above [`BACKGROUND_FILES`] files or [`BACKGROUND_BYTES`] bytes the
    /// copy runs on a worker that holds the write guard, reports
    /// `progress` and ends with `finished`. A plan is committed once.
    pub fn extras_add_commit(
        &self,
        generation: u64,
        id: u32,
        rev: u64,
        plan: u64,
        choices: &[AddChoice],
        events: Arc<dyn ExtrasAddEvents>,
    ) -> Result<AddStarted, IpcError> {
        let no_plan = || IpcError::BadRequest(format!("no add plan {plan}"));
        let kept = lock(&self.adds)
            .get(plan, generation, id)
            .ok_or_else(no_plan)?;
        let mut sources = Vec::with_capacity(choices.len());
        let mut bytes = 0u64;
        for c in choices {
            let file = kept.files.get(c.index as usize).ok_or_else(|| {
                IpcError::BadRequest(format!("no file {} in plan {plan}", c.index))
            })?;
            bytes = bytes.saturating_add(file.size);
            sources.push(AddSource {
                path: file.path.clone(),
                category: c.category.clone(),
            });
        }
        let (guard, session, cfg) = self.add_guard(generation, id)?;
        let target = self.extras_target(generation, id, rev, &session)?;
        let taken = lock(&self.adds)
            .take(plan, generation, id)
            .ok_or_else(no_plan)?;
        let mut skipped = Vec::new();
        if kept.keys > 0 {
            skipped.push(ExtrasNote::KeysSkipped { n: kept.keys });
        }
        if kept.hidden > 0 {
            skipped.push(ExtrasNote::HiddenSkipped { n: kept.hidden });
        }
        let n = sources.len();
        self.start_add(
            guard,
            AddWork {
                generation,
                id,
                add: plan,
                plan: Some(taken),
                system: session.entries[id as usize].system_id.clone(),
                target,
                data_dir: cfg.data_dir.clone(),
                sources,
                skipped,
                trash: Arc::clone(&self.trash),
                extras: Arc::clone(&self.extras),
                session: Arc::clone(&self.session),
                opts: self.settings().scan_options(),
                events,
            },
            n,
            bytes,
        )
    }

    /// Copies the paths Rust recorded for drop `token` (and only those)
    /// into `category` on listing `rev` (a drop on a category, D11): a
    /// dropped folder file by file, keys and hidden files skipped. Runs
    /// like [`Self::extras_add_commit`]. The token is spent.
    pub fn extras_add_dropped(
        &self,
        generation: u64,
        id: u32,
        rev: u64,
        token: u64,
        category: CategoryRef,
        events: Arc<dyn ExtrasAddEvents>,
    ) -> Result<AddStarted, IpcError> {
        let (guard, session, cfg) = self.add_guard(generation, id)?;
        let target = self.extras_target(generation, id, rev, &session)?;
        let paths = self.drop_paths(token)?;
        let (files, _, _) = expand_sources(&paths);
        let bytes = files
            .iter()
            .filter_map(|p| std::fs::symlink_metadata(p).ok())
            .map(|m| m.len())
            .fold(0u64, u64::saturating_add);
        let sources = paths
            .into_iter()
            .map(|path| AddSource {
                path,
                category: category.clone(),
            })
            .collect();
        let add = lock(&self.adds).next_id();
        self.start_add(
            guard,
            AddWork {
                generation,
                id,
                add,
                plan: None,
                system: session.entries[id as usize].system_id.clone(),
                target,
                data_dir: cfg.data_dir.clone(),
                sources,
                skipped: Vec::new(),
                trash: Arc::clone(&self.trash),
                extras: Arc::clone(&self.extras),
                session: Arc::clone(&self.session),
                opts: self.settings().scan_options(),
                events,
            },
            files.len(),
            bytes,
        )
    }

    /// Asks background add `add` to stop after its current file (D13):
    /// what was copied stays as the undoable job. False when no such add
    /// runs.
    pub fn extras_add_cancel(&self, add: u64) -> bool {
        lock(&self.adds).cancel(add)
    }

    /// The write guard for an add, owned so a background copy can keep it.
    fn add_guard(
        &self,
        generation: u64,
        id: u32,
    ) -> Result<(OwnedManualWrite, Arc<Session>, Arc<FillConfig>), IpcError> {
        if self.jobs.paused() {
            return Err(IpcError::JobRunning);
        }
        let guard = self.jobs.begin_manual_owned()?;
        let session = self.known_entry(generation, id)?;
        let cfg = self.fill_config();
        check_data_dir(&cfg, &session)?;
        Ok((guard, session, cfg))
    }

    fn start_add(
        &self,
        guard: OwnedManualWrite,
        work: AddWork,
        n: usize,
        bytes: u64,
    ) -> Result<AddStarted, IpcError> {
        if n <= BACKGROUND_FILES && bytes <= BACKGROUND_BYTES {
            let mut started = false;
            let copied = work.copy(&mut |_| {
                started = true;
                ControlFlow::Continue(())
            });
            if copied.is_err() && !started {
                work.keep_plan(&self.adds);
            }
            drop(guard);
            return copied.map(|write| AddStarted::Done { write });
        }
        let add = work.add;
        let adds = Arc::clone(&self.adds);
        let cancel = lock(&adds).start(add);
        let spawned = {
            let adds = Arc::clone(&adds);
            std::thread::Builder::new()
                .name("extras-add".into())
                .spawn(move || work.background(guard, &cancel, &adds))
        };
        if let Err(e) = spawned {
            lock(&adds).end(add);
            return Err(IpcError::Io(e.to_string()));
        }
        log::info!("extras add {add}: {n} files in the background");
        Ok(AddStarted::Background {
            add,
            n: u32::try_from(n).unwrap_or(u32::MAX),
            total: bytes,
        })
    }
}

/// [`AppState::extras_target`]'s answer.
struct Target {
    listed: Arc<ListedExtras>,
    /// The game folder as scanned.
    dir: PathBuf,
    root: PathBuf,
    /// The game folder, absolute.
    game: PathBuf,
}

/// One add, with everything it needs off the state (it may run on a
/// worker after the command answered).
struct AddWork {
    generation: u64,
    id: u32,
    add: u64,
    /// The dialog's plan, kept again when the add is refused before it
    /// copies anything (D13): the user fixes the reason and adds again.
    plan: Option<Arc<AddPlan>>,
    system: String,
    target: Target,
    data_dir: PathBuf,
    sources: Vec<AddSource>,
    /// Files the plan already left out (keys, hidden).
    skipped: Vec<ExtrasNote>,
    trash: Arc<dyn TrashBackend>,
    extras: Arc<Mutex<ExtrasCache>>,
    session: Arc<Mutex<Option<Arc<Session>>>>,
    opts: ScanOptions,
    events: Arc<dyn ExtrasAddEvents>,
}

impl AddWork {
    /// Copies the sources in one job and keeps the new listing; the
    /// entry's new counts go to `events`.
    fn copy(
        &self,
        progress: &mut dyn FnMut(AddProgress) -> ControlFlow<()>,
    ) -> Result<ExtrasWriteView, IpcError> {
        let t = &self.target;
        let ctx = ExtrasCtx {
            library_root: &t.root,
            game_dir: &t.game,
            data_dir: &self.data_dir,
            trash: self.trash.as_ref(),
            now: SystemTime::now(),
            same_volume,
            available: user_available,
        };
        let mut outcome = add_files(
            &ctx,
            &t.listed.listing,
            &self.sources,
            AddLimits::default(),
            progress,
        )?;
        outcome.notes.extend(self.skipped.iter().cloned());
        if let Some(job) = &outcome.job {
            log::info!("extras add job {job} for entry {}", self.id);
        }
        let stored = lock(&self.extras).store(
            self.generation,
            self.id,
            Some(t.dir.clone()),
            outcome.listing,
        );
        if let Some(event) =
            refresh_counts_in(&self.session, &self.opts, self.generation, self.id, &t.dir)
        {
            self.events.entries_updated(event);
        }
        Ok(ExtrasWriteView {
            job: outcome.job,
            listing: view(self.id, &self.system, &stored, false, self.trash.as_ref()),
            notes: outcome.notes,
        })
    }

    /// Keeps the dialog's plan again, after a refusal that copied nothing.
    fn keep_plan(&self, adds: &Mutex<AddPlans>) {
        if let Some(plan) = &self.plan {
            lock(adds).restore(Arc::clone(plan));
        }
    }

    /// [`Self::copy`] on a worker: progress before the first file and
    /// after each, stopped between files once `cancel` is set; the guard
    /// is released and the add forgotten before `finished` is told, also
    /// after a panic (the add then ends failed, never copying forever).
    fn background(self, guard: OwnedManualWrite, cancel: &AtomicBool, adds: &Mutex<AddPlans>) {
        let (entry_id, add) = (self.id, self.add);
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let events = Arc::clone(&self.events);
        let mut started = false;
        let copied = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.copy(&mut |p| {
                started = true;
                events.progress(AddProgressEvent {
                    entry_id,
                    add,
                    i: count(p.i),
                    n: count(p.n),
                    done: p.done,
                    total: p.total,
                });
                if cancel.load(Ordering::SeqCst) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
        }))
        .unwrap_or_else(|_| Err(IpcError::Io("the extras add stopped unexpectedly".into())));
        let end = match copied {
            Ok(write) => AddEnd::Done { write },
            Err(error) => {
                log::warn!("extras add {add} failed: {error}");
                if !started {
                    self.keep_plan(adds);
                }
                AddEnd::Failed { error }
            }
        };
        drop(guard);
        lock(adds).end(add);
        events.finished(AddFinishedEvent { entry_id, add, end });
    }
}

/// Counts game folder `dir`'s fixed folders again and keeps them in the
/// session `slot` (part D D22): the `library-entries-updated` payload for
/// entry `id`, or `None` once another library is open.
fn refresh_counts_in(
    slot: &Mutex<Option<Arc<Session>>>,
    opts: &ScanOptions,
    generation: u64,
    id: u32,
    dir: &Path,
) -> Option<EntriesUpdatedEvent> {
    let counts = recount_fixed(dir, opts);
    let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
    let current = Arc::clone(slot.as_ref().filter(|s| s.generation == generation)?);
    let (mut next, mut entries) = refresh_entries(&current, &[id], &SystemDb::builtin());
    next.entries.get_mut(id as usize)?.rom.fixed_counts = Some(counts);
    for e in &mut entries {
        e.addons = Some(FolderCountsView::from(counts));
    }
    *slot = Some(Arc::new(next));
    Some(EntriesUpdatedEvent {
        generation,
        entries,
    })
}
