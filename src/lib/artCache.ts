/**
 * Object URLs of art images, shared by the grid and the drawer: at most
 * ART_CACHE_SIZE live URLs (least recently used are revoked), one request
 * per image at a time, failures never cached.
 */
import { getBackend } from '@/ipc/backend';
import type { Media } from '@/ipc/types';

export const ART_CACHE_SIZE = 400;

const urls = new Map<string, string>();
const inflight = new Map<string, Promise<string>>();

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

export function loadArtUrl(
  generation: number,
  id: number,
  media: Media,
): Promise<string> {
  const key = artKey(generation, id, media);
  const hit = peekArtUrl(key);
  if (hit) return Promise.resolve(hit);
  const pending = inflight.get(key);
  if (pending) return pending;
  const request = (async () => {
    const backend = await getBackend();
    const image = await backend.readArt(generation, id, media);
    const url = URL.createObjectURL(
      new Blob([image.bytes], { type: image.mime }),
    );
    urls.set(key, url);
    evict();
    return url;
  })().finally(() => inflight.delete(key));
  inflight.set(key, request);
  return request;
}

export function clearArtCache(): void {
  for (const url of urls.values()) URL.revokeObjectURL(url);
  urls.clear();
  inflight.clear();
}
