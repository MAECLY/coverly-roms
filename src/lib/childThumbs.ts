import type { ArtImage } from '@/ipc/types';

/** Most child icons read at once (addon views D15). */
export const THUMB_READS = 4;

/**
 * The icons an Organizar plan's updates, DLC and demos carry in the clear
 * (addon views D15): one read per item, at most {@link THUMB_READS} in
 * flight, the rest waiting in order. `close()` (the plan went away) drops
 * the waiting reads, ignores the answers still coming and revokes every
 * image URL; the queue is then empty and usable again (a remount).
 */
export interface ChildThumbs {
  /** The item's icon as an object URL, null when it has none. */
  get(itemId: number): Promise<string | null>;
  close(): void;
}

export function createChildThumbs(
  read: (itemId: number) => Promise<ArtImage>,
  limit: number = THUMB_READS,
): ChildThumbs {
  let session = 0;
  let active = 0;
  let cache = new Map<number, Promise<string | null>>();
  let urls: string[] = [];
  let waiting: { start(): void; drop(): void }[] = [];

  const pump = () => {
    while (active < limit && waiting.length > 0) waiting.shift()!.start();
  };

  const load = (itemId: number): Promise<string | null> => {
    const mine = session;
    return new Promise<string | null>((resolve) => {
      waiting.push({
        drop: () => resolve(null),
        start: () => {
          active += 1;
          read(itemId)
            .then((img) => {
              if (mine !== session) return resolve(null);
              const url = URL.createObjectURL(
                new Blob([img.bytes], { type: img.mime }),
              );
              urls.push(url);
              resolve(url);
            })
            // No art in the clear (or the pile changed): no thumbnail.
            .catch(() => resolve(null))
            .finally(() => {
              // A read of a closed plan freed its own count already.
              if (mine !== session) return;
              active -= 1;
              pump();
            });
        },
      });
      pump();
    });
  };

  return {
    get(itemId) {
      let p = cache.get(itemId);
      if (!p) {
        p = load(itemId);
        cache.set(itemId, p);
      }
      return p;
    },
    close() {
      session += 1;
      active = 0;
      const dropped = waiting;
      waiting = [];
      dropped.forEach((w) => w.drop());
      urls.forEach((u) => URL.revokeObjectURL(u));
      urls = [];
      cache = new Map();
    },
  };
}
