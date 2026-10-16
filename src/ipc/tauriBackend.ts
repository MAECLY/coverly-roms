import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';

import type { Backend } from './backend';
import { BackendError, toBackendError } from './errors';
import { sniffImageMime } from './sniff';
import type {
  IdentityView,
  LibrarySettings,
  LibraryView,
  RecentLibrary,
  ScanProgress,
} from './types';

export const SCAN_PROGRESS_EVENT = 'library-scan-progress';

async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toBackendError(e);
  }
}

export function createTauriBackend(): Backend {
  return {
    kind: 'tauri',
    openLibrary: (path, profileId) =>
      call<LibraryView>('open_library', { path, profileId: profileId ?? null }),
    onScanProgress: (cb) =>
      listen<ScanProgress>(SCAN_PROGRESS_EVENT, (e) => cb(e.payload)),
    recentLibraries: () => call<RecentLibrary[]>('recent_libraries'),
    forgetRecentLibrary: (path) =>
      call<RecentLibrary[]>('forget_recent_library', { path }),
    librarySettings: () => call<LibrarySettings>('library_settings'),
    setLibrarySettings: (s) =>
      call<LibrarySettings>('set_library_settings', {
        ignoredDirs: s.ignored_dirs,
      }),
    async readArt(generation, entryId, media) {
      const buffer = await call<ArrayBuffer>('read_art', {
        generation,
        entryId,
        media,
      });
      const bytes = new Uint8Array(buffer);
      const mime = sniffImageMime(bytes);
      if (!mime) throw new BackendError('art-unreadable');
      return { bytes, mime };
    },
    identifyEntry: (generation, entryId) =>
      call<IdentityView>('identify_entry', { generation, entryId }),
    revealEntry: (generation, entryId) =>
      call<void>('reveal_entry', { generation, entryId }),
    async pickFolder() {
      const picked = await open({ directory: true, multiple: false });
      return typeof picked === 'string' ? picked : null;
    },
    onFolderDrop: (cb) =>
      getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type !== 'drop') return;
        const first = event.payload.paths[0];
        if (first) cb(first);
      }),
  };
}
