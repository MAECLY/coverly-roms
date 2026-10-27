/**
 * Object URLs of art images, shared by the grid and the drawer: at most
 * ART_CACHE_SIZE live URLs (least recently used are revoked), one request
 * per image at a time, failures never cached.
 */
import { getBackend } from '@/ipc/backend';
import type { MediaView } from '@/ipc/dedupeTypes';
import { MEDIA, type Media } from '@/ipc/types';

export const ART_CACHE_SIZE = 400;

const urls = new Map<string, string>();
const inflight = new Map<string, Promise<string>>();
/** Bumped by clearArtCache so loads started before a clear never land. */
let epoch = 0;

export function artKey(generation: number, id: number, media: Media): string {
  return `${generation}:${id}:${media}`;
}

/** The cached URL (marking it recently used), or null. */
export function peekArtUrl(key: string): string | null {
  const url = urls.get(key);
  if (url === undefined) return null;
  urls.delete(key);
  urls.set(key, url);
  return url;
}

function evict(): void {
  while (urls.size > ART_CACHE_SIZE) {
    const oldest = urls.keys().next().value;
    if (oldest === undefined) return;
    const url = urls.get(oldest);
    urls.delete(oldest);
    if (url) URL.revokeObjectURL(url);
  }
}

/** One object URL per `key`, through the same LRU and revoke-on-evict as art. */
export function loadUrl(
  key: string,
  fetch: () => Promise<Blob>,
): Promise<string> {
  const hit = peekArtUrl(key);
  if (hit) return Promise.resolve(hit);
  const pending = inflight.get(key);
  if (pending) return pending;
  const started = epoch;
  const request = (async () => {
    const url = URL.createObjectURL(await fetch());
    if (started !== epoch) {
      URL.revokeObjectURL(url);
      throw new Error('art cache cleared while loading');
    }
    urls.set(key, url);
    evict();
    return url;
  })().finally(() => {
    if (inflight.get(key) === request) inflight.delete(key);
  });
  inflight.set(key, request);
  return request;
}

export function loadArtUrl(
  generation: number,
  id: number,
  media: Media,
): Promise<string> {
  return loadUrl(artKey(generation, id, media), async () => {
    const image = await (await getBackend()).readArt(generation, id, media);
    return new Blob([image.bytes], { type: image.mime });
  });
}

export const dedupeArtKey = (
  plan: number,
  group: number,
  member: number | null,
  media: MediaView,
) => `dedupe:${plan}:${group}:${member ?? 'g'}:${media}`;

/** Keys the backend answered `art-missing` for, so a scroll never re-asks. */
const missing = new Set<string>();

export function markMissingUrl(key: string): void {
  missing.add(key);
}

export function isMissingUrl(key: string): boolean {
  return missing.has(key);
}

export function clearArtCache(): void {
  epoch += 1;
  for (const url of urls.values()) URL.revokeObjectURL(url);
  urls.clear();
  inflight.clear();
  missing.clear();
}

/** Forgets (and revokes) the images of `ids`: their art was rewritten. */
export function dropArtUrls(generation: number, ids: readonly number[]): void {
  for (const id of ids) {
    for (const media of MEDIA) {
      const key = artKey(generation, id, media);
      const url = urls.get(key);
      if (url !== undefined) {
        URL.revokeObjectURL(url);
        urls.delete(key);
      }
      inflight.delete(key);
    }
  }
}
