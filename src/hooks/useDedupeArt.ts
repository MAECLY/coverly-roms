import { useEffect, useState } from 'react';

import { getBackend } from '@/ipc/backend';
import type { MediaView } from '@/ipc/dedupeTypes';
import { toBackendError } from '@/ipc/errors';
import { sniffImageMime } from '@/ipc/sniff';
import {
  dedupeArtKey,
  isMissingUrl,
  loadUrl,
  markMissingUrl,
  peekArtUrl,
} from '@/lib/artCache';

export type DedupeArtState =
  | { status: 'loading' }
  | { status: 'ready'; url: string }
  | { status: 'missing' };

const LOADING: DedupeArtState = { status: 'loading' };
const MISSING: DedupeArtState = { status: 'missing' };

/** Real art is sniffed; the browser mock serves SVG, which the sniffer refuses. */
function artMime(bytes: Uint8Array): string {
  const sniffed = sniffImageMime(bytes);
  if (sniffed) return sniffed;
  const head = new TextDecoder().decode(bytes.subarray(0, 64)).trimStart();
  return head.startsWith('<svg') || head.startsWith('<?xml')
    ? 'image/svg+xml'
    : 'application/octet-stream';
}

/**
 * A duplicate group's cover (`member` null) or a copy's own art, through the
 * shared object-URL cache. A group without art is remembered as missing until
 * the cache is cleared, so scrolling never asks twice. `planId` undefined or
 * `enabled` false stays `loading` without a request.
 */
export function useDedupeArt(
  planId: number | undefined,
  group: number,
  member: number | null,
  media: MediaView,
  enabled: boolean,
): DedupeArtState {
  const active = enabled && planId !== undefined;
  const key =
    planId === undefined ? '' : dedupeArtKey(planId, group, member, media);
  const cached = active ? peekArtUrl(key) : null;
  const missing = active && !cached && isMissingUrl(key);
  const [loaded, setLoaded] = useState<{
    key: string;
    url: string | null;
  } | null>(null);
  // A loaded URL that is no longer cached was evicted and revoked: load again.
  const evicted = !cached && loaded?.key === key && loaded.url !== null;
  useEffect(() => {
    if (!active || cached || missing || planId === undefined) return;
    let alive = true;
    loadUrl(key, async () => {
      const backend = await getBackend();
      const bytes = new Uint8Array(
        await backend.dedupeArt(planId, group, member, media),
      );
      return new Blob([bytes], { type: artMime(bytes) });
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
  }, [active, cached, missing, evicted, key, planId, group, member, media]);
  if (!active) return LOADING;
  if (cached) return { status: 'ready', url: cached };
  // `art-missing` is remembered; any other failure shows the slot this time.
  if (missing || (loaded?.key === key && loaded.url === null)) return MISSING;
  return LOADING;
}
