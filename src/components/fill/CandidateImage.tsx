import { useT } from '@/i18n/useT';
import { useImageUrl, type ImageSource } from '@/hooks/useImageUrl';

/** A candidate image; `label` (a hostile name) is only ever an attribute or text. */
export function CandidateImage({
  source,
  label,
  className = '',
}: {
  source: ImageSource;
  label: string;
  className?: string;
}) {
  const t = useT();
  const { url, failed } = useImageUrl(source);
  if (url) {
    return (
      <img
        src={url}
        alt={label}
        draggable={false}
        className={`art-thumb ${className}`}
      />
    );
  }
  return (
    <div
      role="img"
      aria-label={label}
      className={`art-thumb art-placeholder ${className}`}
    >
      {failed ? t('fill.imageFailed') : ''}
    </div>
  );
}
