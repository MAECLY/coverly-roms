import { useSyncExternalStore } from 'react';

import { getBackend } from '@/ipc/backend';

/**
 * The systems no emulator opens (`systems.toml` `no_emulator`, systems
 * batch 3 D22), read once from the catalogue and shared by every shelf
 * header. Empty until the backend answers; a failed read is retried by
 * the next header that mounts.
 */
let ids: ReadonlySet<string> = new Set();
let loading: Promise<void> | null = null;
const listeners = new Set<() => void>();

function load(): void {
  if (loading) return;
  loading = getBackend()
    .then((b) => b.listSystems())
    .then(
      (systems) => {
        const next = new Set(
          (Array.isArray(systems) ? systems : [])
            .filter((s) => s.no_emulator)
            .map((s) => s.id),
        );
        if (next.size === 0 && ids.size === 0) return;
        ids = next;
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

const snapshot = () => ids;

/** Whether no emulator opens this system; false for none or unknown. */
export function useNoEmulator(systemId: string | null | undefined): boolean {
  const set = useSyncExternalStore(subscribe, snapshot, snapshot);
  return !!systemId && set.has(systemId);
}

/** Tests only: start from a known catalogue (`null`: load it again). */
export function setNoEmulatorIds(next: readonly string[] | null): void {
  ids = new Set(next ?? []);
  loading = next === null ? null : Promise.resolve();
  listeners.forEach((l) => l());
}
