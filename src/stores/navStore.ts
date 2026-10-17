import { create } from 'zustand';

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

interface NavState {
  screen: Screen;
  section: SettingsSection;
  /** The folder the organise screen was opened for. */
  organizeSource: string | null;
  openSettings(section?: SettingsSection): void;
  closeSettings(): void;
  setSection(section: SettingsSection): void;
  openOrganize(source: string): void;
  closeOrganize(): void;
}

export const useNavStore = create<NavState>()((set) => ({
  screen: 'main',
  section: 'general',
  organizeSource: null,
  openSettings: (section) =>
    set((s) => ({ screen: 'settings', section: section ?? s.section })),
  closeSettings: () => set({ screen: 'main' }),
  setSection: (section) => set({ section }),
  openOrganize: (source) => set({ screen: 'organize', organizeSource: source }),
  closeOrganize: () => set({ screen: 'main', organizeSource: null }),
}));
