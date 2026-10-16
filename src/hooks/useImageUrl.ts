import { useEffect, useState } from 'react';

import { getBackend } from '@/ipc/backend';
import type { Media } from '@/ipc/types';

/** A candidate image: of a plan item, or of the drawer's candidate list. */
export type ImageSource =
  | { kind: 'job'; job: number; index: number; candidate: number }
  | {
      kind: 'entry';
      generation: number;
      entryId: number;
      media: Media;
      candidate: number;
    };

export function sourceKey(s: ImageSource): string {
  return s.kind === 'job'
    ? `job:${s.job}:${s.index}:${s.candidate}`
    : `entry:${s.generation}:${s.entryId}:${s.media}:${s.candidate}`;
}

/** A blob URL of the candidate image (revoked when it changes or unmounts). */
export function useImageUrl(source: ImageSource): {
  url: string | null;
  failed: boolean;
} {
  const key = sourceKey(source);
  const [state, setState] = useState<{
    key: string;
    url: string | null;
    failed: boolean;
  } | null>(null);
  useEffect(() => {
    let alive = true;
    let url: string | null = null;
    void (async () => {
      try {
        const b = await getBackend();
        const img =
          source.kind === 'job'
            ? await b.candidateArt(source.job, source.index, source.candidate)
            : await b.entryCandidateArt(
                source.generation,
                source.entryId,
                source.media,
                source.candidate,
              );
        url = URL.createObjectURL(new Blob([img.bytes], { type: img.mime }));
        if (alive) setState({ key, url, failed: false });
        else URL.revokeObjectURL(url);
      } catch {
        if (alive) setState({ key, url: null, failed: true });
      }
    })();
    return () => {
      alive = false;
      if (url) URL.revokeObjectURL(url);
    };
    // `key` identifies `source` completely.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return state?.key === key
    ? { url: state.url, failed: state.failed }
    : { url: null, failed: false };
}
