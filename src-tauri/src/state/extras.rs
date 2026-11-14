//! Extras writes in the app (part D D8, D10, D22): one journaled job per
//! panel action, behind the one-writer guard, on the listing a revision
//! names; the entry's counts refreshed after each write or undo.

use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError};
use std::time::SystemTime;

use coverly_core::extras::index::{IndexState, Listing};
use coverly_core::extras::ops::{
    create_category, forget, move_to, remove_category, rename, rename_category, set_tags, trash,
    ExtrasCtx, ExtrasError, ExtrasOutcome,
};
use coverly_core::organize::apply::user_available;
use coverly_core::scan::recount_fixed;
use coverly_core::systems::SystemDb;
use coverly_core::write::{job_targets, list_jobs, same_volume, undo_job_with, JobKind};

use super::{absolute_root, lock, AppState};
use crate::error::IpcError;
use crate::jobs::view::EntriesUpdatedEvent;
use crate::library::extras::{
    game_dir, read_listing, read_only, view, ExtrasAction, ExtrasUndone, ExtrasWriteView,
    ExtrasWritten,
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
        let root = absolute_root(&session)?;
        let game = absolute(&dir)?;
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
                listing: view(id, system, &stored, false),
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
        Ok(view(id, system, &stored, false))
    }

    /// Counts game folder `dir`'s fixed folders again and keeps them in
    /// the session (part D D22): the `library-entries-updated` payload for
    /// entry `id`, or `None` once another library is open.
    fn refresh_counts(&self, generation: u64, id: u32, dir: &Path) -> Option<EntriesUpdatedEvent> {
        let counts = recount_fixed(dir, &self.settings().scan_options());
        let mut slot = self.session.lock().unwrap_or_else(PoisonError::into_inner);
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
}
