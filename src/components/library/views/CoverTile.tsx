import {
  useContext,
  type CSSProperties,
  type JSX,
  type ReactNode,
} from 'react';

import { SystemIcon } from '@/components/system/SystemIcon';
import { SystemLabel } from '@/components/system/SystemLabel';
import { Badge } from '@/components/ui/badge';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { EntryView, Media, SystemView } from '@/ipc/types';
import {
  fitSummary,
  speaksContent,
  spokenSummary,
  summaryParts,
  summaryText,
  type SummaryPart,
} from '@/lib/addonSummary';
import { metrics as coverMetrics } from '@/lib/coverSizes';
import { formatSize } from '@/lib/format';
import type { Game } from '@/lib/games';
import { REGION_CODE, titleWithoutRegions } from '@/lib/tileLabel';
import {
  SHELF_EDGE_X,
  SHELF_EDGE_Y,
  besideIconSize,
  besideIcons,
  besideWidth,
  shelfDlc,
  shelfStack,
} from '@/lib/shelves';
import type { CoverSize, ShelfAddons } from '@/lib/viewPrefs';

import { ArtStatusIcon } from '../ArtStatusIcon';
import { ArtThumb } from '../ArtThumb';
import { ChildThumb } from '../game/ChildThumb';

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
  /** The game this tile stands for: its add-ons show under the cover. */
  game?: Game | undefined;
  /** The library has add-on content: M, L and XL get the summary line. */
  contentLine?: boolean;
  /** A child of the game is new since the last open (D18). */
  isNew?: boolean | undefined;
  /** Estantes (§8.7): the game's box, its DLC as edges behind it or as
   * icons beside it; `fan` spreads the edges on hover and selection
   * (off under reduced motion, D17). */
  shelf?: { mode: ShelfAddons; fan: boolean } | undefined;
}

/** Mono 11px: the advance of one character, in px (§8.2 chip). */
const CHIP_CHAR_PX = 6.6;
/** The UI face at 12px: a generous advance of one character, in px. */
const LABEL_CHAR_PX = 6.4;
/** Between the shelf label and the warning chip. */
const LABEL_GAP_PX = 4;
/** The chip's horizontal padding, both sides. */
const CHIP_PAD_PX = 12;
/** «Nuevo» beside the chip: its padding plus the gap before it. */
const NEW_TAG_PAD_PX = 14;

/**
 * The add-on chip: `+1 DLC · v1.10`, or the warning `↑ falta v1.3.0` with
 * its word. `aria-hidden` (the tile's phrase says it all) and not a tab
 * stop; a click on it opens the panel at Versión (`data-open-version`).
 */
function AddonChip({ parts }: { parts: readonly SummaryPart[] }) {
  const t = useT();
  const warning = parts.some((p) => p.kind === 'missing');
  return (
    <span
      className={`addon-chip${warning ? ' addon-chip--warning' : ''}`}
      aria-hidden="true"
      data-open-version=""
    >
      {parts.map((p, i) => (
        <span key={p.kind} className="addon-chip__group">
          {i > 0 && <span className="addon-chip__sep">·</span>}
          <span
            className={`addon-chip__part${
              p.kind === 'missing' ? ' addon-chip__part--warning' : ''
            }`}
          >
            {summaryText(p, t)}
          </span>
        </span>
      ))}
    </span>
  );
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
  game,
  contentLine = false,
  isNew = false,
  shelf,
}: CoverTileProps): JSX.Element {
  const t = useT();
  const locale = useContext(LocaleContext);
  // The library flag alone: every tile of a row keeps the row's height,
  // with or without a game behind it.
  const metrics = coverMetrics(size, contentLine);
  const hasLine = metrics.meta !== 'badge';
  const badge = systemMark === 'badge' || (systemMark === 'auto' && !hasLine);
  const labelInLine = systemMark === 'auto' && hasLine;
  const systemName = system?.name ?? '';
  const parts = game ? summaryParts(game) : [];
  // Whatever the chip or the dot shows is also said (§8.9).
  const dlc = shelf ? shelfDlc(game) : 0;
  const stack = shelf?.mode === 'behind' ? shelfStack(dlc) : null;
  const edges = stack?.edges ?? 0;
  const beside = shelf?.mode === 'beside' && dlc > 0;
  const extraW = beside ? besideWidth(size) : 0;
  const phrase =
    game && (parts.length > 0 || speaksContent(game, isNew))
      ? spokenSummary(game, systemName, t, undefined, {
          isNew,
          locale,
          addons: dlc > 0,
        })
      : null;
  const missing = parts.some((p) => p.kind === 'missing');
  const demoOf = game?.isDemo
    ? game.entry.demo?.of_name !== undefined
      ? t('content.demoOf', {
          name: titleWithoutRegions(game.entry.demo.of_name),
        })
      : t('content.kind.demo')
    : null;
  const newTag = isNew ? t('content.new') : null;
  const fitted =
    metrics.content === 'line'
      ? fitSummary(
          parts,
          t,
          Math.floor(
            (metrics.artW -
              CHIP_PAD_PX -
              (newTag ? newTag.length * CHIP_CHAR_PX + NEW_TAG_PAD_PX : 0)) /
              CHIP_CHAR_PX,
          ),
        )
      : parts;
  const hideText = phrase !== null;
  // Estantes: «Con 3 complementos» in words, and the warning beside it
  // when the newest update is missing; the warning never drops (§8.7).
  const missingPart = parts.find((p) => p.kind === 'missing');
  const shelfLabel =
    dlc > 0 && demoOf === null
      ? dlc === 1
        ? t('content.shelves.withOne')
        : t('content.shelves.with', {
            n: new Intl.NumberFormat(locale).format(dlc),
          })
      : null;
  const warningText = missingPart ? summaryText(missingPart, t) : null;
  const lineW = metrics.artW + extraW;
  const labelFits =
    shelfLabel !== null &&
    (warningText === null ||
      shelfLabel.length * LABEL_CHAR_PX +
        LABEL_GAP_PX +
        warningText.length * CHIP_CHAR_PX +
        CHIP_PAD_PX <=
        lineW);
  const artInner: ReactNode = (
    <>
      <ArtThumb
        generation={generation}
        entry={entry}
        media={art}
        systemId={system?.system_id}
        // An empty slot with no name below shows the name.
        placeholder={
          metrics.nameLines === 0 ? titleWithoutRegions(entry.name) : undefined
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
      {metrics.content === 'dot' && (parts.length > 0 || isNew) && (
        // XS and S: a dot in the top-right corner, an arrow (a shape,
        // not only a colour) when the newest update is missing.
        <span
          className={`cover-tile__dot${
            missing ? ' cover-tile__dot--warning' : ''
          }`}
          aria-hidden="true"
          data-open-version=""
        >
          {missing ? '↑' : ''}
        </span>
      )}
    </>
  );
  let artBox: ReactNode;
  if (edges > 0 && stack) {
    // The DLC stand behind the box as edges, each one further right and
    // up; the cover gives up that room so the tile keeps its size.
    const style = {
      '--stack-x': `${edges * SHELF_EDGE_X}px`,
      '--stack-y': `${edges * SHELF_EDGE_Y}px`,
      '--cover-art-h': `${metrics.artH - edges * SHELF_EDGE_Y}px`,
    } as CSSProperties;
    artBox = (
      <div className="cover-tile__art cover-tile__art--stack" style={style}>
        <span
          className={`shelf-stack${shelf?.fan ? ' shelf-stack--fan' : ''}`}
          aria-hidden="true"
        >
          {Array.from({ length: edges }, (_, i) => (
            <span
              key={i}
              className={`shelf-edge shelf-edge--${i + 1}`}
              style={{ '--i': i + 1 } as CSSProperties}
            />
          ))}
        </span>
        {stack.more > 0 && (
          <span className="shelf-edge__more" aria-hidden="true">
            +{new Intl.NumberFormat(locale).format(stack.more)}
          </span>
        )}
        <div className="shelf-case">{artInner}</div>
      </div>
    );
  } else if (beside && game) {
    // «Al lado»: the DLC's own icons in a column beside the box.
    const icon = besideIconSize(size);
    const { shown, more } = besideIcons(size, dlc, metrics.artH);
    artBox = (
      <div className="cover-tile__art cover-tile__art--beside">
        <div className="shelf-case" style={{ width: metrics.artW }}>
          {artInner}
        </div>
        <span className="shelf-beside" aria-hidden="true">
          {game.dlc.slice(0, shown).map((child) => (
            <ChildThumb
              key={child.index}
              generation={generation}
              entry={game.entry}
              child={child}
              systemId={system?.system_id}
              size={icon}
            />
          ))}
          {more > 0 && (
            <span className={`shelf-beside__more shelf-beside__more--${icon}`}>
              +{new Intl.NumberFormat(locale).format(more)}
            </span>
          )}
        </span>
      </div>
    );
  } else {
    artBox = <div className="cover-tile__art">{artInner}</div>;
  }
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
          width: metrics.tileW + extraW,
          height: metrics.tileH,
          '--cover-art-h': `${metrics.artH}px`,
          ...style,
        } as CSSProperties
      }
      title={phrase ?? entry.name}
    >
      {phrase !== null ? (
        // The whole state in words (§8.9), inside the cell rather than as
        // an aria-label, so the visible name stays part of the name.
        <span className="sr-only">{phrase}</span>
      ) : (
        metrics.nameLines === 0 && (
          // No name line: the cell is still named by the game first, then
          // the system badge and the art marks.
          <span className="sr-only">{entry.name}</span>
        )
      )}
      {artBox}
      {metrics.nameLines > 0 && (
        <span className="cover-tile__name" aria-hidden={hideText || undefined}>
          {titleWithoutRegions(entry.name)}
        </span>
      )}
      {hasLine && (
        <span className="cover-tile__info" aria-hidden={hideText || undefined}>
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
      {metrics.content === 'line' && (
        <span className="cover-tile__content" aria-hidden="true">
          {demoOf !== null ? (
            <span className="cover-tile__demo">{demoOf}</span>
          ) : shelfLabel !== null ? (
            <>
              {labelFits && <span className="shelf-label">{shelfLabel}</span>}
              {missingPart && <AddonChip parts={[missingPart]} />}
            </>
          ) : (
            fitted.length > 0 && <AddonChip parts={fitted} />
          )}
          {newTag !== null && <span className="addon-new">{newTag}</span>}
        </span>
      )}
      {metrics.meta === 'line+size' && (
        <span className="cover-tile__size" aria-hidden={hideText || undefined}>
          {formatSize(entry.size, locale)}
        </span>
      )}
    </div>
  );
}
