import { useChildArtUrl } from '@/hooks/useChildArtUrl';
import { useT } from '@/i18n/useT';
import type { ChildView, EntryView } from '@/ipc/types';
import { childArtState } from '@/lib/childArt';
import { cn } from '@/lib/utils';

import { ArtThumb } from '../ArtThumb';

/**
 * An update's or DLC's art at 48 or 24 px (addon views §8.3, §8.4, §8.8):
 * its own icon, the base game's art dimmed when its packages carry none in
 * the clear (Switch), or the dashed missing slot («sin arte» at 48 px).
 * Decorative: whoever shows it says the child's name and art state in
 * words.
 */
export function ChildThumb({
  generation,
  entry,
  child,
  systemId,
  size = 48,
  load = true,
  className,
}: {
  generation: number;
  /** The game's entry: the handle of its children, and its art. */
  entry: EntryView;
  child: ChildView;
  systemId: string | null | undefined;
  size?: 48 | 24 | undefined;
  /** False holds back reading its own art (a selection still moving). */
  load?: boolean | undefined;
  className?: string | undefined;
}) {
  const t = useT();
  const state = childArtState(child, systemId);
  const url = useChildArtUrl(
    generation,
    entry.id,
    child.index,
    state.kind === 'own' ? state.slot : 'icon',
    state.kind === 'own' && load,
  );
  return (
    <span
      aria-hidden="true"
      data-art={state.kind}
      className={cn('child-thumb', `child-thumb--${size}`, className)}
    >
      {state.kind === 'own' ? (
        url ? (
          <img
            src={url}
            alt=""
            draggable={false}
            decoding="async"
            className="child-thumb__img"
          />
        ) : (
          <span className="art-placeholder child-thumb__slot" />
        )
      ) : state.kind === 'base' ? (
        <ArtThumb
          generation={generation}
          entry={entry}
          media="cover"
          placeholder=""
          systemId={systemId}
          className="child-thumb__inherit"
        />
      ) : (
        <span className="art-placeholder child-thumb__slot child-thumb__slot--missing">
          {size === 48 ? t('content.noArt') : ''}
        </span>
      )}
    </span>
  );
}
