/** The app settings (spec §9) as the UI sees them: loaded once, updated
 * per field with the backend's validation errors kept per field. */
import { create } from 'zustand';

import { getBackend } from '@/ipc/backend';
import { toBackendError, type BackendError } from '@/ipc/errors';
import type {
  FieldError,
  Settings,
  SettingsNotice,
  SettingsPatch,
} from '@/ipc/types';
import { leafPaths } from '@/lib/settingsRules';

const LEGACY_LOCALE = 'coverly.locale';

interface SettingsState {
  status: 'idle' | 'loading' | 'ready' | 'error';
  settings: Settings | null;
  notice: SettingsNotice | null;
  backup: string | null;
  fixed: string[];
  errors: Record<string, FieldError>;
  reopen: boolean;
  saved: number;
  error: BackendError | null;
  load(): Promise<void>;
  refresh(): Promise<void>;
  update(patch: SettingsPatch): Promise<FieldError[]>;
  reset(): Promise<void>;
  dismissNotice(): void;
  dismissReopen(): void;
  clearErrors(prefix: string): void;
}

const INITIAL = {
  status: 'idle' as const,
  settings: null,
  notice: null,
  backup: null,
  fixed: [],
  errors: {},
  reopen: false,
  saved: 0,
  error: null,
};

function takeLegacyLocale(): 'en' | 'es' | null {
  try {
    const v = localStorage.getItem(LEGACY_LOCALE);
    localStorage.removeItem(LEGACY_LOCALE);
    return v === 'en' || v === 'es' ? v : null;
  } catch {
    return null;
  }
}

export const useSettingsStore = create<SettingsState>()((set, get) => ({
  ...INITIAL,
  async load() {
    set({ status: 'loading', error: null });
    try {
      const b = await getBackend();
      const view = await b.getSettings();
      set({
        status: 'ready',
        settings: view.settings,
        notice: view.notice,
        backup: view.backup,
        fixed: view.fixed,
      });
      const legacy = takeLegacyLocale();
      if (legacy && view.settings.general.language === 'auto') {
        await get().update({ general: { language: legacy } });
      }
    } catch (e) {
      set({ status: 'error', error: toBackendError(e) });
    }
  },
  async refresh() {
    try {
      const view = await (await getBackend()).getSettings();
      set({ settings: view.settings });
    } catch {
      // Keep what is shown; the next update reports any problem.
    }
  },
  async update(patch) {
    const b = await getBackend();
    const out = await b.updateSettings(patch);
    const paths = leafPaths(patch);
    set((s) => {
      const errors = { ...s.errors };
      for (const key of Object.keys(errors)) {
        if (paths.some((p) => key === p || key.startsWith(`${p}.`)))
          delete errors[key];
      }
      for (const e of out.errors) errors[e.field] = e;
      return out.errors.length
        ? { errors }
        : {
            errors,
            settings: out.settings,
            reopen: s.reopen || out.reopen,
            notice: null,
            saved: s.saved + 1,
          };
    });
    return out.errors;
  },
  async reset() {
    const b = await getBackend();
    const view = await b.resetSettings();
    set({
      settings: view.settings,
      notice: null,
      errors: {},
      reopen: true,
      saved: get().saved + 1,
    });
  },
  dismissNotice: () => set({ notice: null }),
  dismissReopen: () => set({ reopen: false }),
  clearErrors: (prefix) =>
    set((s) => ({
      errors: Object.fromEntries(
        Object.entries(s.errors).filter(
          ([k]) => k !== prefix && !k.startsWith(`${prefix}.`),
        ),
      ),
    })),
}));

/** Tests only. */
export function resetSettingsStore(): void {
  useSettingsStore.setState({ ...INITIAL });
}
