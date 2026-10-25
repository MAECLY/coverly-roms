/** The app settings (spec §9) as the UI sees them: loaded once, updated
 * per field with the backend's validation errors kept per field. */
import { create } from 'zustand';

import { getBackend } from '@/ipc/backend';
import { toBackendError, type BackendError } from '@/ipc/errors';
import type {
  FieldError,
  FoundTool,
  Settings,
  SettingsNotice,
  SettingsPatch,
} from '@/ipc/types';
import { leafPaths } from '@/lib/settingsRules';
import { useLibraryStore } from '@/stores/libraryStore';

const LEGACY_LOCALE = 'coverly.locale';

interface SettingsState {
  status: 'idle' | 'loading' | 'ready' | 'error';
  settings: Settings | null;
  notice: SettingsNotice | null;
  backup: string | null;
  fixed: string[];
  /** The external archive tools found on this machine (preference
   * order), whatever `organize.external_tools` says. */
  externalToolsFound: FoundTool[];
  errors: Record<string, FieldError>;
  reopen: boolean;
  saved: number;
  error: BackendError | null;
  /** The last save or reset that failed; cleared by the next one that works. */
  saveError: BackendError | null;
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
  externalToolsFound: [],
  errors: {},
  reopen: false,
  saved: 0,
  error: null,
  saveError: null,
};

/** Each save is numbered; an answer older than one applied is not shown. */
let saveSeq = 0;
let appliedSeq = 0;

/** A scan setting changed only matters to a library already open. */
function libraryOpen(): boolean {
  return useLibraryStore.getState().status === 'ready';
}

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
        externalToolsFound: view.external_tools_found,
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
      set({
        settings: view.settings,
        externalToolsFound: view.external_tools_found,
      });
    } catch {
      // Keep what is shown; the next update reports any problem.
    }
  },
  async update(patch) {
    const mine = (saveSeq += 1);
    let out;
    try {
      out = await (await getBackend()).updateSettings(patch);
    } catch (e) {
      set({ saveError: toBackendError(e) });
      return [];
    }
    const paths = leafPaths(patch);
    const stale = mine < appliedSeq;
    if (!stale && out.errors.length === 0) appliedSeq = mine;
    set((s) => {
      const errors = { ...s.errors };
      for (const key of Object.keys(errors)) {
        if (paths.some((p) => key === p || key.startsWith(`${p}.`)))
          delete errors[key];
      }
      for (const e of out.errors) errors[e.field] = e;
      if (out.errors.length) return { errors };
      return {
        errors,
        ...(stale ? {} : { settings: out.settings }),
        reopen: s.reopen || (out.reopen && libraryOpen()),
        notice: null,
        saveError: null,
        saved: s.saved + 1,
      };
    });
    return out.errors;
  },
  async reset() {
    const mine = (saveSeq += 1);
    let view;
    try {
      view = await (await getBackend()).resetSettings();
    } catch (e) {
      set({ saveError: toBackendError(e) });
      return;
    }
    if (mine < appliedSeq) return;
    appliedSeq = mine;
    set({
      settings: view.settings,
      externalToolsFound: view.external_tools_found,
      notice: null,
      errors: {},
      saveError: null,
      reopen: libraryOpen(),
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

// A library opened (or reopened) is scanned with the current settings.
useLibraryStore.subscribe((now, before) => {
  if (
    now.status === 'ready' &&
    now.view?.generation !== before.view?.generation
  )
    useSettingsStore.setState({ reopen: false });
});

/** Tests only. */
export function resetSettingsStore(): void {
  saveSeq = 0;
  appliedSeq = 0;
  useSettingsStore.setState({ ...INITIAL });
}
