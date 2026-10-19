import { useContext, type CSSProperties, type JSX } from 'react';

import { SystemIcon } from '@/components/system/SystemIcon';
import { SystemLabel } from '@/components/system/SystemLabel';
import { Badge } from '@/components/ui/badge';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { EntryView, Media, SystemView } from '@/ipc/types';
import { COVER_SIZES } from '@/lib/coverSizes';
import { formatSize } from '@/lib/format';
import { REGION_CODE, titleWithoutRegions } from '@/lib/tileLabel';
import type { CoverSize } from '@/lib/viewPrefs';

import { ArtStatusIcon } from '../ArtStatusIcon';
import { ArtThumb } from '../ArtThumb';

/**
 * Where a tile names its system (spec §A.6):
 * - `auto`: the size stop decides (a corner badge at XS and S, the meta
 *   line above that), as in Covers;
 * - `badge`: always a 20px titled corner badge (Shelves not grouped by
 *   system), with the meta line, if any, keeping only the regions;
 * - `none`: no system at all (Shelves grouped by system: the shelf header
 *   already names it).
 */
export type TileSystemMark = 'auto' | 'badge' | 'none';

export interface CoverTileProps {
  entry: EntryView;
  system: SystemView | undefined;
  generation: number;
  art: Media;
  size: CoverSize;
  selected: boolean;
  systemMark?: TileSystemMark;
  /** 1-based column in its row, for grids whose rows are virtualised. */
  colIndex?: number;
  style?: CSSProperties;
}

/** One cover tile (a `gridcell`): the art box of the size stop, its art
 *  marks, the name and, sizes permitting, the system and regions. */
export function CoverTile({
  entry,
  system,
  generation,
  art,
  size,
  selected,
  systemMark = 'auto',
  colIndex,
  style,
}: CoverTileProps): JSX.Element {
  const t = useT();
  const locale = useContext(LocaleContext);
  const metrics = COVER_SIZES[size];
  const hasLine = metrics.meta !== 'badge';
  const badge = systemMark === 'badge' || (systemMark === 'auto' && !hasLine);
  const labelInLine = systemMark === 'auto' && hasLine;
  const systemName = system?.name ?? '';
  return (
    <div
      id={`entry-${entry.id}`}
      role="gridcell"
      aria-selected={selected}
      aria-colindex={colIndex}
      data-entry-id={entry.id}
      className={`cover-tile cover-tile--${size}`}
      style={
        {
          width: metrics.tileW,
          height: metrics.tileH,
          '--cover-art-h': `${metrics.artH}px`,
          ...style,
        } as CSSProperties
      }
      title={entry.name}
    >
      {metrics.nameLines === 0 && (
        // No name line: the cell is still named by the game first, then
        // the system badge and the art marks.
        <span className="sr-only">{entry.name}</span>
      )}
      <div className="cover-tile__art">
        <ArtThumb
          generation={generation}
          entry={entry}
          media={art}
          // An empty slot with no name below shows the name.
          placeholder={
            metrics.nameLines === 0
              ? titleWithoutRegions(entry.name)
              : undefined
          }
        />
        {badge && (
          <span className="cover-tile__badge">
            <SystemIcon
              systemId={system?.system_id ?? null}
              size={20}
              title={systemName}
            />
          </span>
        )}
        <span className="cover-tile__marks">
          <ArtStatusIcon media="cover" mark={entry.art.cover} />
          <ArtStatusIcon media="screenshot" mark={entry.art.screenshot} />
          <ArtStatusIcon media="title" mark={entry.art.title} />
        </span>
      </div>
      {metrics.nameLines > 0 && (
        <span className="cover-tile__name">
          {titleWithoutRegions(entry.name)}
        </span>
      )}
      {hasLine && (
        <span className="cover-tile__info">
          <span className="cover-tile__system">
            {labelInLine && (
              <SystemLabel
                systemId={system?.system_id ?? null}
                name={systemName}
                size={16}
              />
            )}
          </span>
          {entry.regions.slice(0, 2).map((r) => (
            <Badge
              key={r}
              variant="outline"
              className="cover-tile__region"
              title={t(`regions.${r}`)}
            >
              {r === 'world' ? t('regions.world') : REGION_CODE[r]}
            </Badge>
          ))}
          {entry.regions.length > 2 && (
            <Badge
              variant="outline"
              className="cover-tile__region"
              title={entry.regions
                .slice(2)
                .map((r) => t(`regions.${r}`))
                .join(', ')}
            >
              +{entry.regions.length - 2}
            </Badge>
          )}
        </span>
      )}
      {metrics.meta === 'line+size' && (
        <span className="cover-tile__size">
          {formatSize(entry.size, locale)}
        </span>
      )}
    </div>
  );
}
