import type { LibraryView, Media } from '@/ipc/types';

export interface AuditRow {
  folder: string;
  /** Canonical system id, for the row's icon; null for the total row and
   *  for a folder Coverly did not recognise. */
  system_id: string | null;
  name: string;
  roms: number;
  /** ROMs missing each media the layout can store. */
  missing: Partial<Record<Media, number>>;
  /** ROMs missing none of them. */
  complete: number;
}

export interface AuditSummary {
  media: Media[];
  systems: AuditRow[];
  total: AuditRow;
}

/** The audit per system of an open library (spec §8 Reports). */
export function auditSummary(view: LibraryView): AuditSummary {
  const media = [...view.profile.media];
  const zero = () =>
    Object.fromEntries(media.map((m) => [m, 0])) as Partial<
      Record<Media, number>
    >;
  const rows: AuditRow[] = view.systems.map((s) => ({
    folder: s.folder,
    system_id: s.system_id,
    name: s.name,
    roms: 0,
    missing: zero(),
    complete: 0,
  }));
  const total: AuditRow = {
    folder: '',
    system_id: null,
    name: '',
    roms: 0,
    missing: zero(),
    complete: 0,
  };
  for (const e of view.entries) {
    const row = rows[e.system];
    if (!row) continue;
    let complete = true;
    for (const m of media) {
      if (e.art[m] === 'missing') {
        row.missing[m] = (row.missing[m] ?? 0) + 1;
        total.missing[m] = (total.missing[m] ?? 0) + 1;
        complete = false;
      }
    }
    row.roms += 1;
    total.roms += 1;
    if (complete) {
      row.complete += 1;
      total.complete += 1;
    }
  }
  return { media, systems: rows.filter((r) => r.roms > 0), total };
}
