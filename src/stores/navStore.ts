import { create } from 'zustand';

import type { TransferMode } from '@/ipc/types';

export type SettingsSection =
  | 'general'
  | 'library'
  | 'organize'
  | 'duplicates'
  | 'art'
  | 'matching'
  | 'sources'
  | 'network'
  | 'safety'
  | 'advanced'
  | 'profiles'
  | 'about';
export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  'general',
  'library',
  'organize',
  'duplicates',
  'profiles',
  'art',
  'matching',
  'sources',
  'network',
  'safety',
  'advanced',
  'about',
];

export type SettingsGroup = 'collection' | 'covers' | 'app';
/** Visual groups of the tab list (D3); future tabs (collections, devices,
 * saves, RetroAchievements) join 'collection'. */
export const SETTINGS_GROUPS: Readonly<
  Record<SettingsSection, SettingsGroup | null>
> = {
  general: null,
  library: 'collection',
  organize: 'collection',
  duplicates: 'collection',
  profiles: 'collection',
  art: 'covers',
  matching: 'covers',
  sources: 'covers',
  network: 'app',
  safety: 'app',
  advanced: 'app',
  about: null,
};

export type Screen = 'main' | 'settings' | 'organize' | 'duplicates';

/** How the organise screen starts: preset options (organising the loose
 * files of an open library keeps its layout, locked, and moves). */
export interface OrganizeLaunch {
  layout?: string;
  mode?: TransferMode;
  /** The library root is its ROMs folder: no `ROMS/` inside it. */
  flat?: boolean;
  /** Layout and target are the library's: they cannot change. */
  locked?: boolean;
}

export interface OpenSettingsOptions {
  /** DOM id of the control to land on (settingId(path) or a list anchor). */
  focus?: string;
}

interface NavState {
  screen: Screen;
  section: SettingsSection;
  /** The folder the organise screen was opened for. */
  organizeSource: string | null;
  organizeLaunch: OrganizeLaunch | null;
  /** The library the duplicates screen was opened for. */
  duplicatesRoot: string | null;
  /** The screen Settings was opened from: where Back returns. */
  settingsFrom: Exclude<Screen, 'settings'> | null;
  /** Advanced folds open in Settings (this session only). */
  openFolds: readonly string[];
  /** DOM id of the control Settings should land on when it opens. */
  focusSetting: string | null;
  openSettings(section?: SettingsSection, opts?: OpenSettingsOptions): void;
  closeSettings(): void;
  setSection(section: SettingsSection): void;
  setFold(id: string, open: boolean): void;
  clearFocusSetting(): void;
  openOrganize(source: string, launch?: OrganizeLaunch): void;
  closeOrganize(): void;
  openDuplicates(root: string): void;
  closeDuplicates(): void;
}

const INITIAL = {
  screen: 'main' as Screen,
  section: 'general' as SettingsSection,
  organizeSource: null as string | null,
  organizeLaunch: null as OrganizeLaunch | null,
  duplicatesRoot: null as string | null,
  settingsFrom: null as Exclude<Screen, 'settings'> | null,
  openFolds: [] as readonly string[],
  focusSetting: null as string | null,
};

const known = (s: SettingsSection | undefined): s is SettingsSection =>
  s !== undefined && SETTINGS_SECTIONS.includes(s);

export const useNavStore = create<NavState>()((set) => ({
  ...INITIAL,
  openSettings: (section, opts) =>
    set((s) => ({
      screen: 'settings',
      section: known(section) ? section : s.section,
      // From inside Settings (a search result, a cross-tab link) Back
      // still returns to the screen that opened Settings.
      settingsFrom: s.screen === 'settings' ? s.settingsFrom : s.screen,
      focusSetting: opts?.focus ?? null,
    })),
  closeSettings: () =>
    set((s) => {
      const from = s.settingsFrom;
      const back: Screen =
        (from === 'duplicates' && !s.duplicatesRoot) ||
        (from === 'organize' && !s.organizeSource)
          ? 'main'
          : (from ?? 'main');
      return { screen: back, settingsFrom: null, focusSetting: null };
    }),
  setSection: (section) => {
    if (known(section)) set({ section });
  },
  setFold: (id, open) =>
    set((s) => {
      const has = s.openFolds.includes(id);
      if (open === has) return s;
      return {
        openFolds: open
          ? [...s.openFolds, id]
          : s.openFolds.filter((f) => f !== id),
      };
    }),
  clearFocusSetting: () => set({ focusSetting: null }),
  openOrganize: (source, launch) =>
    set({
      screen: 'organize',
      organizeSource: source,
      organizeLaunch: launch ?? null,
    }),
  closeOrganize: () =>
    set({ screen: 'main', organizeSource: null, organizeLaunch: null }),
  openDuplicates: (root) => set({ screen: 'duplicates', duplicatesRoot: root }),
  closeDuplicates: () => set({ screen: 'main', duplicatesRoot: null }),
}));

/** Tests: back to the start screen with nothing remembered. */
export function resetNavStore(): void {
  useNavStore.setState({ ...INITIAL });
}
