import { LibraryScreen } from './screens/LibraryScreen';
import { SettingsScreen } from './screens/SettingsScreen';
import { StartScreen } from './screens/StartScreen';
import { useLibraryStore } from './stores/libraryStore';
import { useNavStore } from './stores/navStore';

export default function App() {
  const status = useLibraryStore((s) => s.status);
  const screen = useNavStore((s) => s.screen);
  if (screen === 'settings') return <SettingsScreen />;
  return status === 'ready' ? <LibraryScreen /> : <StartScreen />;
}
