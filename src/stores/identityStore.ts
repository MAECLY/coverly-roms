import { create } from 'zustand';

import { getBackend } from '@/ipc/backend';
import { toBackendError, type BackendError } from '@/ipc/errors';
import type { IdentityView } from '@/ipc/types';

export type IdentityState =
  | { status: 'loading' }
  | { status: 'done'; identity: IdentityView }
  | { status: 'error'; error: BackendError };

export const identityKey = (generation: number, id: number) =>
  `${generation}:${id}`;

interface IdentityStore {
  byKey: Record<string, IdentityState>;
  identify(generation: number, id: number): Promise<void>;
  reset(): void;
}

export const useIdentityStore = create<IdentityStore>()((set) => {
  const put = (key: string, state: IdentityState) =>
    set((s) => ({ byKey: { ...s.byKey, [key]: state } }));
  return {
    byKey: {},
    async identify(generation, id) {
      const key = identityKey(generation, id);
      put(key, { status: 'loading' });
      try {
        const backend = await getBackend();
        put(key, {
          status: 'done',
          identity: await backend.identifyEntry(generation, id),
        });
      } catch (e) {
        put(key, { status: 'error', error: toBackendError(e) });
      }
    },
    reset: () => set({ byKey: {} }),
  };
});
