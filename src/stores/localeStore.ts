import { create } from 'zustand';

import type { Locale } from '@/i18n/core';

export type LocaleChoice = 'auto' | Locale;
const KEY = 'coverly.locale';

function readChoice(): LocaleChoice {
  try {
    const v = localStorage.getItem(KEY);
    return v === 'en' || v === 'es' ? v : 'auto';
  } catch {
    return 'auto';
  }
}

interface LocaleState {
  choice: LocaleChoice;
  setChoice(choice: LocaleChoice): void;
}

/** UI language until M5 settings exist (a per-viewer convenience). */
export const useLocaleStore = create<LocaleState>()((set) => ({
  choice: readChoice(),
  setChoice(choice) {
    try {
      localStorage.setItem(KEY, choice);
    } catch {
      // Storage may be blocked; the choice still applies to this session.
    }
    set({ choice });
  },
}));
