import { useArtUrl } from '@/hooks/useArtUrl';
import { switchTitleIdOf, useCatalogIconUrl } from '@/hooks/useCatalogIconUrl';
import { useT } from '@/i18n/useT';
import type { EntryView, Media } from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

export function ArtThumb({
  generation,
  entry,
  media,
  className = '',
  placeholder,
  systemId,
}: {
  generation: number;
  entry: EntryView;
  media: Media;
  className?: string;
  /** Text for the empty slot instead of "No cover" (a tile with no name line). */
  placeholder?: string | undefined;
  /** The entry's system: a Switch game with no cover may show its cached
   * eShop icon while «Iconos de Switch» is on (adjustments D8). */
  systemId?: string | null | undefined;
}) {
  const t = useT();
  const mark = entry.art[media];
  const url = useArtUrl(generation, entry.id, media, mark === 'present');
  const iconsOn = useSettingsStore((s) => {
    const sw = s.settings?.catalogs.switch;
    return !!sw && sw.enabled && sw.icons;
  });
  // Only in place of a missing cover, never over the library's own art.
  const tid =
    media === 'cover' && mark === 'missing' && systemId === 'switch'
      ? switchTitleIdOf(entry.rel_path)
      : null;
  const icon = useCatalogIconUrl(tid, iconsOn);
  const src = url ?? icon;
  if (src) {
    return (
      <img
        src={src}
        alt=""
        draggable={false}
        loading="lazy"
        decoding="async"
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
