import { useEffect, useRef } from 'react';

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
  // Back from Settings: focus returns to the Settings button that opened it.
  const shown = useRef(screen);
  useEffect(() => {
    if (shown.current === 'settings' && screen === 'main')
      document.querySelector<HTMLElement>('[data-settings-opener]')?.focus();
    shown.current = screen;
  }, [screen]);
  if (screen === 'settings') return <SettingsScreen />;
  if (screen === 'organize') return <OrganizeScreen />;
  return ready ? <LibraryScreen /> : <StartScreen />;
}
