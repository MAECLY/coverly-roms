import { useEffect, useState } from 'react';

import { getBackend } from '@/ipc/backend';
import { BackendError, toBackendError } from '@/ipc/errors';
import { sniffImageMime } from '@/ipc/sniff';
import {
  isMissingUrl,
  loadUrl,
  markMissingUrl,
  peekArtUrl,
} from '@/lib/artCache';

/** The cache key of a Switch title's eShop icon (adjustments D8). */
export const catalogIconKey = (tid: string) => `catalog-icon:${tid}`;

/**
 * The 16-hex Switch title id of a file name's `[0100…]` token (spec §6),
 * upper case; `null` without one.
 */
export function switchTitleIdOf(name: string): string | null {
  const m = /\[(01[0-9a-f]{14})\]/i.exec(name);
  return m ? m[1]!.toUpperCase() : null;
}

/**
 * The object URL of title `tid`'s cached eShop icon, fetched on demand
 * through `catalogIcon` (never a remote URL in the webview, D8); null while
 * loading, disabled, or when the title has none. `art-missing` is
 * remembered until the art cache is cleared, so a scroll never asks twice.
 */
export function useCatalogIconUrl(
  tid: string | null,
  enabled: boolean,
): string | null {
  const active = enabled && tid !== null;
  const key = tid === null ? '' : catalogIconKey(tid);
  const cached = active ? peekArtUrl(key) : null;
  const missing = active && !cached && isMissingUrl(key);
  const [loaded, setLoaded] = useState<{
    key: string;
    url: string | null;
  } | null>(null);
  // A loaded URL that is no longer cached was evicted and revoked: load again.
  const evicted = !cached && loaded?.key === key && loaded.url !== null;
  useEffect(() => {
    if (!active || cached || missing || tid === null) return;
    let alive = true;
    loadUrl(key, async () => {
      const image = await (await getBackend()).catalogIcon(tid);
      const bytes = new Uint8Array(image.bytes);
      const mime = sniffImageMime(bytes);
      if (!mime) throw new BackendError('art-unreadable');
      return new Blob([bytes], { type: mime });
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
  }, [active, cached, missing, evicted, key, tid]);
  return active ? cached : null;
}
