import { create } from 'zustand';

import type { TransferMode } from '@/ipc/types';

export type SettingsSection =
  | 'general'
  | 'library'
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
  'art',
  'matching',
  'sources',
  'network',
  'safety',
  'advanced',
  'profiles',
  'about',
];

export type Screen = 'main' | 'settings' | 'organize';

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

interface NavState {
  screen: Screen;
  section: SettingsSection;
  /** The folder the organise screen was opened for. */
  organizeSource: string | null;
  organizeLaunch: OrganizeLaunch | null;
  openSettings(section?: SettingsSection): void;
  closeSettings(): void;
  setSection(section: SettingsSection): void;
  openOrganize(source: string, launch?: OrganizeLaunch): void;
  closeOrganize(): void;
}

export const useNavStore = create<NavState>()((set) => ({
  screen: 'main',
  section: 'general',
  organizeSource: null,
  organizeLaunch: null,
  openSettings: (section) =>
    set((s) => ({ screen: 'settings', section: section ?? s.section })),
  closeSettings: () => set({ screen: 'main' }),
  setSection: (section) => set({ section }),
  openOrganize: (source, launch) =>
    set({
      screen: 'organize',
      organizeSource: source,
      organizeLaunch: launch ?? null,
    }),
  closeOrganize: () =>
    set({ screen: 'main', organizeSource: null, organizeLaunch: null }),
}));
