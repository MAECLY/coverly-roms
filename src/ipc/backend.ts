/**
 * The one door from the UI to a backend: the Tauri app, or — in a plain
 * browser (`pnpm dev`) — the deterministic mock.
 */
import { hasTauriIpc } from './appInfo';
import type {
  ArtImage,
  IdentityView,
  LibrarySettings,
  LibraryView,
  Media,
  RecentLibrary,
  ScanProgress,
} from './types';

export interface Backend {
  readonly kind: 'tauri' | 'mock';
  openLibrary(path: string, profileId?: string): Promise<LibraryView>;
  onScanProgress(cb: (p: ScanProgress) => void): Promise<() => void>;
  recentLibraries(): Promise<RecentLibrary[]>;
  forgetRecentLibrary(path: string): Promise<RecentLibrary[]>;
  librarySettings(): Promise<LibrarySettings>;
  setLibrarySettings(s: LibrarySettings): Promise<LibrarySettings>;
  readArt(generation: number, entryId: number, media: Media): Promise<ArtImage>;
  identifyEntry(generation: number, entryId: number): Promise<IdentityView>;
  revealEntry(generation: number, entryId: number): Promise<void>;
  pickFolder(): Promise<string | null>;
  onFolderDrop(cb: (path: string) => void): Promise<() => void>;
}

let current: Backend | null = null;
let pending: Promise<Backend> | null = null;

async function createBackend(): Promise<Backend> {
  if (hasTauriIpc()) {
    const { createTauriBackend } = await import('./tauriBackend');
    return createTauriBackend();
  }
  const { createMockBackend } = await import('./mock/mockBackend');
  return createMockBackend();
}

/**
 * The Tauri backend inside the app, the mock in a plain browser. One
 * instance per page: concurrent first calls share the same promise (the
 * mock is stateful).
 */
export function getBackend(): Promise<Backend> {
  if (current) return Promise.resolve(current);
  if (pending) return pending;
  const created: Promise<Backend> = createBackend().then((backend) => {
    // A `setBackend` call while this was loading wins over it.
    if (pending !== created) return current ?? backend;
    current = backend;
    pending = null;
    return backend;
  });
  pending = created;
  return created;
}

/** Tests (and only tests) install a backend; `null` resets the choice. */
export function setBackend(backend: Backend | null): void {
  current = backend;
  pending = null;
}
