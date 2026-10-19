import { useVirtualizer } from '@tanstack/react-virtual';
import {
  useCallback,
  useContext,
  useMemo,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';

import { SystemLabel } from '@/components/system/SystemLabel';
import { SlotMark, type SlotState } from '@/components/ui/slot-mark';
import { useActiveRange, stickyHeaderAt } from '@/hooks/useActiveRange';
import { useElementSize } from '@/hooks/useElementSize';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { ArtMark, EntryView, Media } from '@/ipc/types';
import { formatSize } from '@/lib/format';
import { nextIndex } from '@/lib/keyboardNav';
import { buildRowModel, coveringSections, moveVertical } from '@/lib/rowModel';
import { titleWithoutRegions } from '@/lib/tileLabel';
import { cn } from '@/lib/utils';

import { ArtThumb } from '../ArtThumb';
import { SECTION_ROW_H, SectionHeaderRow } from './SectionHeaderRow';
import { entryIdOf, type ViewProps } from './types';
import { useViewKeys } from './useViewKeys';

/** Every Art slots row is this tall (spec §B.4.5, §B.7). */
export const MOUNT_ROW_H = 112;

/** The slot cells in reading order, after the name cell (column 0). */
const MOUNT_MEDIA: readonly Media[] = ['cover', 'title', 'screenshot'];

/** Name + three slots. */
const COLS = 1 + MOUNT_MEDIA.length;

/** A slot's brand state (§2). Coverly has no applied-this-run record for an
 *  entry yet, so art that is there is drawn solid. */
const SLOT_STATE: Record<ArtMark, SlotState> = {
  present: 'filled',
  missing: 'missing',
  unsupported: 'unsupported',
};

/** The id of cell `col` of `entryId`: the name cell keeps the plain entry id,
 *  as in every other view. */
function cellId(entryId: number, col: number): string {
  return col === 0 ? `entry-${entryId}` : `entry-${entryId}-c${col}`;
}

function fileName(entry: EntryView): string {
  const slash = entry.rel_path.lastIndexOf('/');
  return slash < 0 ? entry.rel_path : entry.rel_path.slice(slash + 1);
}

/**
 * Art slots (⌘5), the view built for Coverly's job: each row is an album
 * page holding one game's cover, title screen and screenshot slots side by
 * side, each drawn in its Slot state, with the game's name beside them.
 * ←/→ move between the cells of a row, ↑/↓ between rows in the same cell
 * column; Enter on a slot opens the art workspace on that media.
 */
export function MountsView({
  entries,
  sections,
  prefs,
  systems,
  generation,
  selectedId,
  onSelect,
  onOpen,
  onPreview,
  onOpenArt,
}: ViewProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const [ref, size, element] = useElementSize<HTMLDivElement>();
  // The active cell column; the row is the selection.
  const [col, setCol] = useState(0);
  const pageRows = Math.max(1, Math.floor(size.height / MOUNT_ROW_H));
  const withHeaders = prefs.groupBy !== 'none';
  const model = useMemo(
    () =>
      buildRowModel(coveringSections(sections, entries.length), 1, withHeaders),
    [sections, entries.length, withHeaders],
  );
  const { rows, rowOfIndex, headerRows } = model;
  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  const selectedIndex =
    selectedId === null ? -1 : (indexOf.get(selectedId) ?? -1);
  const activeRow = selectedIndex < 0 ? -1 : (rowOfIndex[selectedIndex] ?? -1);
  const rangeExtractor = useActiveRange(
    activeRow,
    activeRow < 0 ? null : stickyHeaderAt(headerRows, activeRow),
    headerRows,
  );
  // Row identity, not position (see CoversView).
  const getItemKey = useCallback(
    (i: number) => {
      const row = rows[i];
      if (!row) return i;
      return row.kind === 'header'
        ? `h:${row.section.key}`
        : `r:${entries[row.start]?.id ?? row.start}`;
    },
    [rows, entries],
  );
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => element.current,
    estimateSize: (i) =>
      rows[i]?.kind === 'header' ? SECTION_ROW_H : MOUNT_ROW_H,
    // Three thumbnails a row: one screen ahead and behind, no more (§B.7).
    overscan: pageRows,
    getItemKey,
    rangeExtractor,
    scrollPaddingStart: headerRows.length > 0 ? SECTION_ROW_H : 0,
    initialRect: size,
  });
  const stuck = stickyHeaderAt(headerRows, virtualizer.range?.startIndex ?? 0);

  const viewKeys = useViewKeys({
    entries,
    sections,
    selectedIndex,
    onSelect,
    onOpen,
    onPreview,
    scrollToIndex: (i) =>
      virtualizer.scrollToIndex(rowOfIndex[i] ?? 0, { align: 'auto' }),
    move: (key) => {
      if (selectedIndex >= 0 && key.startsWith('Page')) {
        return moveVertical(
          model,
          selectedIndex,
          key === 'PageDown' ? pageRows : -pageRows,
        );
      }
      // One item a row: ↑/↓/Home/End are list moves; ←/→ never reach here
      // once a row is active.
      return nextIndex(key, selectedIndex, entries.length, 1, pageRows);
    },
  });

  function onKeyDown(ev: KeyboardEvent<HTMLDivElement>) {
    const own = ev.target === ev.currentTarget && !ev.nativeEvent.isComposing;
    const entry = entries[selectedIndex];
    if (own && entry && !ev.altKey && !ev.metaKey && !ev.ctrlKey) {
      if (ev.key === 'ArrowLeft' || ev.key === 'ArrowRight') {
        ev.preventDefault();
        const step = ev.key === 'ArrowRight' ? 1 : -1;
        setCol((c) => Math.min(COLS - 1, Math.max(0, c + step)));
        return;
      }
      const media = MOUNT_MEDIA[col - 1];
      if (ev.key === 'Enter' && media) {
        ev.preventDefault();
        onOpenArt?.(entry.id, media);
        return;
      }
    }
    viewKeys(ev);
  }

  function onClick(ev: MouseEvent<HTMLDivElement>) {
    const id = entryIdOf(ev.target);
    if (id === null) return;
    const cell =
      ev.target instanceof Element ? ev.target.closest('[data-col]') : null;
    const c = Number(cell?.getAttribute('data-col') ?? 0);
    setCol(c);
    const media = MOUNT_MEDIA[c - 1];
    if (media && onOpenArt) onOpenArt(id, media);
    else onOpen(id);
  }

  return (
    <div
      ref={ref}
      role="grid"
      aria-label={t('layouts.mounts')}
      aria-rowcount={rows.length}
      aria-colcount={COLS}
      aria-activedescendant={
        selectedIndex < 0 || selectedId === null
          ? undefined
          : cellId(selectedId, col)
      }
      tabIndex={0}
      className="mounts"
      onKeyDown={onKeyDown}
      onClick={onClick}
    >
      <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
        {virtualizer.getVirtualItems().map((item) => {
          const row = rows[item.index];
          if (!row) return null;
          if (row.kind === 'header') {
            const sticky = item.index === stuck;
            return (
              <SectionHeaderRow
                key={item.key}
                section={row.section}
                colSpan={COLS}
                rowIndex={item.index}
                sticky={sticky}
                style={
                  sticky
                    ? { height: SECTION_ROW_H }
                    : {
                        transform: `translateY(${item.start}px)`,
                        height: SECTION_ROW_H,
                      }
                }
              />
            );
          }
          const entry = entries[row.start];
          if (!entry) return null;
          const selected = entry.id === selectedId;
          const system = systems[entry.system];
          return (
            <div
              key={item.key}
              role="row"
              aria-rowindex={item.index + 1}
              data-selected={selected ? 'true' : undefined}
              className="mount-row"
              style={{
                transform: `translateY(${item.start}px)`,
                height: MOUNT_ROW_H,
              }}
            >
              <div
                id={cellId(entry.id, 0)}
                role="gridcell"
                aria-colindex={1}
                aria-selected={selected && col === 0}
                data-entry-id={entry.id}
                data-col={0}
                className="mount-row__name"
                title={entry.name}
              >
                <span className="mount-row__title">
                  {titleWithoutRegions(entry.name)}
                </span>
                <span className="mount-row__meta">
                  <SystemLabel
                    systemId={system?.system_id ?? null}
                    name={system?.name ?? ''}
                    size={16}
                  />
                  {entry.regions.length > 0 && (
                    <span className="mount-row__fact">
                      {entry.regions.map((r) => t(`regions.${r}`)).join(', ')}
                    </span>
                  )}
                  <span className="mount-row__fact mount-row__size">
                    {formatSize(entry.size, locale)}
                  </span>
                </span>
                <span className="mount-row__file">{fileName(entry)}</span>
              </div>
              {MOUNT_MEDIA.map((media, i) => {
                const c = i + 1;
                const mark = entry.art[media];
                const mediaName = t(`art.media.${media}`);
                const label =
                  mark === 'present'
                    ? `${mediaName}: ${t('layouts.slotHas')}`
                    : mark === 'missing'
                      ? t('layouts.slotMissing', { media: mediaName })
                      : t('art.status', {
                          media: mediaName,
                          state: t('art.mark.unsupported'),
                        });
                return (
                  <div
                    key={media}
                    id={cellId(entry.id, c)}
                    role="gridcell"
                    aria-colindex={c + 1}
                    aria-selected={selected && col === c}
                    aria-label={label}
                    title={label}
                    data-entry-id={entry.id}
                    data-col={c}
                    data-media={media}
                    data-slot-state={SLOT_STATE[mark]}
                    className={cn(
                      'mount-slot',
                      media === 'cover'
                        ? 'mount-slot--cover'
                        : 'mount-slot--wide',
                    )}
                  >
                    {mark === 'unsupported' ? (
                      <span className="mount-slot__unsupported">
                        <SlotMark state="unsupported" size={24} />
                      </span>
                    ) : (
                      <ArtThumb
                        generation={generation}
                        entry={entry}
                        media={media}
                      />
                    )}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}
