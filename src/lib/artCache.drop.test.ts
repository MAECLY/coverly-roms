import { afterEach, describe, expect, it } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';

import {
  artKey,
  clearArtCache,
  dropArtUrls,
  loadArtUrl,
  peekArtUrl,
} from './artCache';

describe('dropping art urls', () => {
  afterEach(() => {
    clearArtCache();
    setBackend(null);
  });

  it('forgets the images of updated entries only', async () => {
    const b = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    setBackend(b);
    const view = await b.openLibrary(MOCK_LIBRARIES[1].path);
    const [a, c] = view.entries.filter((e) => e.art.cover === 'present');
    await loadArtUrl(view.generation, a!.id, 'cover');
    await loadArtUrl(view.generation, c!.id, 'cover');
    dropArtUrls(view.generation, [a!.id]);
    expect(peekArtUrl(artKey(view.generation, a!.id, 'cover'))).toBeNull();
    expect(peekArtUrl(artKey(view.generation, c!.id, 'cover'))).not.toBeNull();
  });
});
