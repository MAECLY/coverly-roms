import { useEffect, useState } from 'react';

import type { Media } from '@/ipc/types';
import { artKey, loadArtUrl, peekArtUrl } from '@/lib/artCache';

/** The object URL of one art image once loaded; null while loading, missing or failed. */
export function useArtUrl(
  generation: number,
  id: number,
  media: Media,
  enabled: boolean,
): string | null {
  const key = artKey(generation, id, media);
  const cached = enabled ? peekArtUrl(key) : null;
  const [loaded, setLoaded] = useState<{
    key: string;
    url: string | null;
  } | null>(null);
  // A loaded URL that is no longer cached was evicted and revoked: load again.
  const evicted = !cached && loaded?.key === key && loaded.url !== null;
  useEffect(() => {
    if (!enabled || cached) return;
    let alive = true;
    loadArtUrl(generation, id, media).then(
      (url) => alive && setLoaded({ key, url }),
      () => alive && setLoaded({ key, url: null }),
    );
    return () => {
      alive = false;
    };
  }, [enabled, cached, evicted, key, generation, id, media]);
  if (!enabled) return null;
  return cached;
}
