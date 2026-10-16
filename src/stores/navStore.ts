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

interface NavState {
  screen: 'main' | 'settings';
  section: SettingsSection;
  openSettings(section?: SettingsSection): void;
  closeSettings(): void;
  setSection(section: SettingsSection): void;
}

export const useNavStore = create<NavState>()((set) => ({
  screen: 'main',
  section: 'general',
  openSettings: (section) =>
    set((s) => ({ screen: 'settings', section: section ?? s.section })),
  closeSettings: () => set({ screen: 'main' }),
  setSection: (section) => set({ section }),
}));
