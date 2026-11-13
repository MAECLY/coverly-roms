import { useEffect, useState } from 'react';

import { getBackend } from '@/ipc/backend';
import { toBackendError } from '@/ipc/errors';
import type { ChildMedia } from '@/ipc/types';
import {
  isMissingUrl,
  loadUrl,
  markMissingUrl,
  peekArtUrl,
} from '@/lib/artCache';
import { childArtKey } from '@/lib/childArt';

/**
 * The object URL of an update's or DLC's art slot, through the shared art
 * cache; asked for by entry id and child index, never by path (D3). Null
 * while loading, when the slot has no file, or when reading failed.
 */
export function useChildArtUrl(
  generation: number,
  entryId: number,
  childIndex: number,
  slot: ChildMedia,
  enabled: boolean,
): string | null {
  const key = childArtKey(generation, entryId, childIndex, slot);
  const cached = enabled ? peekArtUrl(key) : null;
  const missing = enabled && !cached && isMissingUrl(key);
  const [loaded, setLoaded] = useState<{
    key: string;
    url: string | null;
  } | null>(null);
  // A loaded URL that is no longer cached was evicted and revoked: load again.
  const evicted = !cached && loaded?.key === key && loaded.url !== null;
  useEffect(() => {
    if (!enabled || cached || missing) return;
    let alive = true;
    loadUrl(key, async () => {
      const image = await (
        await getBackend()
      ).readChildArtBytes(generation, entryId, childIndex, slot);
      return new Blob([image.bytes], { type: image.mime });
    }).then(
      (url) => alive && setLoaded({ key, url }),
      (e: unknown) => {
        if (toBackendError(e).code === 'art-missing') markMissingUrl(key);
        if (alive) setLoaded({ key, url: null });
      },
    );
    return () => {
      alive = false;
    };
  }, [
    enabled,
    cached,
    missing,
    evicted,
    key,
    generation,
    entryId,
    childIndex,
    slot,
  ]);
  return enabled ? cached : null;
}
