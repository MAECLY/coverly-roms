import { LibraryScreen } from './screens/LibraryScreen';
import { StartScreen } from './screens/StartScreen';
import { useLibraryStore } from './stores/libraryStore';

export default function App() {
  const status = useLibraryStore((s) => s.status);
  return status === 'ready' ? <LibraryScreen /> : <StartScreen />;
}
