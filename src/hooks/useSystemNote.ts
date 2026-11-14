import { useSyncExternalStore } from 'react';

import { getBackend } from '@/ipc/backend';

/** The note a shelf header carries after its system's name. */
export type SystemNote = 'noEmulator' | 'pcOnly';

interface Notes {
  /** `systems.toml` `no_emulator` (systems batch 3 D22). */
  noEmulator: ReadonlySet<string>;
  /** `systems.toml` `pc_only`: runs only on a PC (Type X). */
  pcOnly: ReadonlySet<string>;
}

/**
 * The systems whose shelf carries a note, read once from the catalogue
 * and shared by every shelf header. Empty until the backend answers; a
 * failed read is retried by the next header that mounts.
 */
let notes: Notes = { noEmulator: new Set(), pcOnly: new Set() };
let loading: Promise<void> | null = null;
const listeners = new Set<() => void>();

function load(): void {
  if (loading) return;
  loading = getBackend()
    .then((b) => b.listSystems())
    .then(
      (systems) => {
        const list = Array.isArray(systems) ? systems : [];
        const next: Notes = {
          noEmulator: new Set(
            list.filter((s) => s.no_emulator).map((s) => s.id),
          ),
          pcOnly: new Set(
            list.filter((s) => s.pc_only === true).map((s) => s.id),
          ),
        };
        const empty = (n: Notes) => n.noEmulator.size + n.pcOnly.size === 0;
        if (empty(next) && empty(notes)) return;
        notes = next;
        listeners.forEach((l) => l());
      },
      () => {
        // A failed read is not the answer: the next subscriber asks again.
        loading = null;
      },
    );
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  load();
  return () => {
    listeners.delete(listener);
  };
}

const snapshot = () => notes;

/** The shelf note of a system (`noEmulator` wins); null for none. */
export function useSystemNote(
  systemId: string | null | undefined,
): SystemNote | null {
  const n = useSyncExternalStore(subscribe, snapshot, snapshot);
  if (!systemId) return null;
  if (n.noEmulator.has(systemId)) return 'noEmulator';
  if (n.pcOnly.has(systemId)) return 'pcOnly';
  return null;
}

/** Tests only: start from a known catalogue (`null`: load it again). */
export function setSystemNotes(
  next: { noEmulator: readonly string[]; pcOnly: readonly string[] } | null,
): void {
  notes = {
    noEmulator: new Set(next?.noEmulator ?? []),
    pcOnly: new Set(next?.pcOnly ?? []),
  };
  loading = next === null ? null : Promise.resolve();
  listeners.forEach((l) => l());
}
