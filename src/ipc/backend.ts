/**
 * The one door from the UI to a backend: the Tauri app, or — in a plain
 * browser (`pnpm dev`) — the deterministic mock. `getBackend`/`setBackend`
 * are added in Task 7, once the mock exists.
 */
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
