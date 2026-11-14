/** Listings and a fake backend for the player tests. */
import { vi } from 'vitest';

import type { Backend } from '@/ipc/backend';
import type {
  CategoryRef,
  ExtraFileView,
  ExtrasMedia,
  ExtrasView,
} from '@/ipc/types';

const SOUNDTRACK: CategoryRef = { kind: 'fixed', name: 'soundtrack' };
const ARTWORK: CategoryRef = { kind: 'fixed', name: 'artwork' };

function file(
  index: number,
  path: string,
  category: CategoryRef = SOUNDTRACK,
): ExtraFileView {
  const name = path.split('/').pop()!;
  return {
    index,
    category,
    path,
    name,
    kind: /\.(flac|wav)$/.test(name) ? 'audio' : 'image',
    provenance: 'user',
    format: null,
    tags: [],
    size: 1,
    link: false,
    playable: 'inline',
  };
}

export const FILES = [
  file(0, 'Soundtrack/01 Opening.flac'),
  file(1, 'Soundtrack/02 Boss.wav'),
  file(2, 'Artwork/cover.png', ARTWORK),
  file(3, 'Soundtrack/03 Ending.flac'),
];

export function listing(
  files: ExtraFileView[] = FILES,
  rev = 1,
  entry_id = 7,
): ExtrasView {
  return {
    entry_id,
    rev,
    game_folder: true,
    index: { state: 'ok' },
    writable: { kind: 'yes' },
    categories: [],
    files,
    missing: [],
    capped: false,
    system: 'ps3',
    manual_inside: false,
    trash: true,
  };
}

export const GAME = { generation: 3, title: 'Tidewalker' };

/** A backend that only grants media: `blob:<rev>-<index>`. */
export function mediaBackend() {
  const extrasMedia = vi.fn(
    async (
      _generation: number,
      _entryId: number,
      rev: number,
      index: number,
    ): Promise<ExtrasMedia> => ({
      url: `blob:track-${rev}-${index}`,
      mime: 'audio/wav',
      kind: 'audio',
    }),
  );
  return { backend: { extrasMedia } as unknown as Backend, extrasMedia };
}
