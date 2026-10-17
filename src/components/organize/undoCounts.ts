import type { OrganizeUndoView } from '@/ipc/types';

/** Files an organise undo put back and left alone, counting every organise
 * job it undid ("Undo all in order" also undoes later fill jobs, whose art
 * is not a restored ROM). Null when nothing was undone. */
export function undoCounts(
  undo: OrganizeUndoView | null,
): { restored: number; modified: number } | null {
  if (!undo?.report.length) return null;
  const fills = new Set(
    undo.later_jobs.filter((j) => j.kind === 'fill').map((j) => j.job),
  );
  return undo.report
    .filter((r) => !fills.has(r.job))
    .reduce(
      (sum, r) => ({
        restored: sum.restored + r.restored.length + r.removed.length,
        modified:
          sum.modified + r.skipped_modified.length + r.skipped_occupied.length,
      }),
      { restored: 0, modified: 0 },
    );
}
