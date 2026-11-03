import { useEffect, useRef } from 'react';

import { DuplicatesScreen } from './screens/DuplicatesScreen';
import { LibraryScreen } from './screens/LibraryScreen';
import { OrganizeScreen } from './screens/OrganizeScreen';
import { SettingsScreen } from './screens/SettingsScreen';
import { StartScreen } from './screens/StartScreen';
import { useJobStore } from './stores/jobStore';
import { useLibraryStore } from './stores/libraryStore';
import { useNavStore } from './stores/navStore';

export default function App() {
  const status = useLibraryStore((s) => s.status);
  const screen = useNavStore((s) => s.screen);
  const ready = status === 'ready';
  // Job events for as long as a library is open, whatever screen shows:
  // art a job writes while Settings is open still reaches the grid.
  useEffect(() => {
    if (!ready) return;
    let alive = true;
    let off: (() => void) | undefined;
    void useJobStore
      .getState()
      .connect()
      .then(
        (release) => {
          if (alive) off = release;
          else release();
        },
        () => undefined,
      );
    return () => {
      alive = false;
      off?.();
    };
  }, [ready]);
  // Back from Settings: focus returns to the control that opened it, on
  // whichever screen that was. A screen can have several openers (the
  // organise options and a «Configurar» notice), so the one clicked is
  // remembered by its place among them; the screen mounts again in the
  // same order. Links inside Settings (to another tab or card) are
  // openers too, but they never replace the one that left the screen.
  const shown = useRef(screen);
  const opener = useRef(0);
  useEffect(() => {
    const note = (ev: Event) => {
      if (useNavStore.getState().screen === 'settings') return;
      const el =
        ev.target instanceof Element
          ? ev.target.closest('[data-settings-opener]')
          : null;
      if (!el) return;
      const all = [...document.querySelectorAll('[data-settings-opener]')];
      opener.current = Math.max(0, all.indexOf(el));
    };
    document.addEventListener('click', note, true);
    return () => document.removeEventListener('click', note, true);
  }, []);
  useEffect(() => {
    if (shown.current === 'settings' && screen !== 'settings') {
      const all = document.querySelectorAll<HTMLElement>(
        '[data-settings-opener]',
      );
      (all[opener.current] ?? all[0])?.focus();
      opener.current = 0;
    }
    shown.current = screen;
  }, [screen]);
  if (screen === 'settings') return <SettingsScreen />;
  if (screen === 'organize') return <OrganizeScreen />;
  if (screen === 'duplicates') return <DuplicatesScreen />;
  return ready ? <LibraryScreen /> : <StartScreen />;
}
