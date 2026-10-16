import { useArtUrl } from '@/hooks/useArtUrl';
import { useT } from '@/i18n/useT';
import type { EntryView, Media } from '@/ipc/types';

export function ArtThumb({
  generation,
  entry,
  media,
  className = '',
}: {
  generation: number;
  entry: EntryView;
  media: Media;
  className?: string;
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
        : t(mark === 'missing' ? `art.none.${media}` : 'art.mark.unsupported')}
    </div>
  );
}
