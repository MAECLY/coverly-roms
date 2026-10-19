import { useArtUrl } from '@/hooks/useArtUrl';
import { useT } from '@/i18n/useT';
import type { EntryView, Media } from '@/ipc/types';

export function ArtThumb({
  generation,
  entry,
  media,
  className = '',
  placeholder,
}: {
  generation: number;
  entry: EntryView;
  media: Media;
  className?: string;
  /** Text for the empty slot instead of "No cover" (a tile with no name line). */
  placeholder?: string | undefined;
}) {
  const t = useT();
  const mark = entry.art[media];
  const url = useArtUrl(generation, entry.id, media, mark === 'present');
  if (url) {
    return (
      <img
        src={url}
        alt=""
        draggable={false}
        className={`art-thumb ${className}`}
      />
    );
  }
  return (
    <div
      className={`art-thumb art-placeholder ${className}`}
      aria-hidden="true"
    >
      {mark === 'present'
        ? ''
        : (placeholder ??
          t(mark === 'missing' ? `art.none.${media}` : 'art.mark.unsupported'))}
    </div>
  );
}
