import { useT } from '@/i18n/useT';
import type { ArtMark, Media } from '@/ipc/types';

const GLYPH: Record<ArtMark, string> = {
  present: '●',
  missing: '○',
  unsupported: '–',
};

export function ArtStatusIcon({
  media,
  mark,
}: {
  media: Media;
  mark: ArtMark;
}) {
  const t = useT();
  const label = t('art.status', {
    media: t(`art.media.${media}`),
    state: t(`art.mark.${mark}`),
  });
  return (
    <span
      role="img"
      aria-label={label}
      title={label}
      className={`art-mark art-mark--${mark}`}
    >
      {GLYPH[mark]}
    </span>
  );
}
