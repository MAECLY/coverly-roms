import { useVirtualizer } from '@tanstack/react-virtual';
import {
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';

import { Button } from '@/components/ui/button';
import { useActiveRange } from '@/hooks/useActiveRange';
import { useElementSize } from '@/hooks/useElementSize';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import {
  MEDIA,
  REGIONS,
  type EntryView,
  type Media,
  type SystemView,
} from '@/ipc/types';
import { COVER_GAP, COVER_SIZES } from '@/lib/coverSizes';
import { groupEntries, sectionLabel, type Section } from '@/lib/grouping';
import { nearestByLeft } from '@/lib/keyboardNav';
import type { CoverSize } from '@/lib/viewPrefs';
import {
  SHELF_LIMIT,
  seeAllTarget,
  shelfHeight,
  shelfOfIndex,
} from '@/lib/shelves';
import { cn } from '@/lib/utils';
import { useFilterStore } from '@/stores/filterStore';
import { useViewStore } from '@/stores/viewStore';

import { CoverTile, type TileSystemMark } from './CoverTile';
import { SECTION_ROW_H, SectionHeaderRow } from './SectionHeaderRow';
import { entryIdOf, type ViewProps } from './types';
import { useViewKeys } from './useViewKeys';

const GAP = COVER_GAP;

/** Narrow to a shelf's games and show them in Covers. */
function openSeeAll(section: Section, systems: readonly SystemView[]): void {
  const target = seeAllTarget(section, systems);
  const view = useViewStore.getState();
  const filters = useFilterStore.getState();
  if (target.scope) view.setScope(target.scope);
  if (target.art) filters.setArt(target.art);
  if (target.region) {
    const region = target.region;
    useFilterStore.setState((s) => ({
      filters: { ...s.filters, regions: [region] },
    }));
  }
  view.setView('covers');
}

interface ShelfProps {
  section: Section;
  shelfIndex: number;
  entries: readonly EntryView[];
  systems: readonly SystemView[];
  generation: number;
  art: Media;
  size: CoverSize;
  systemMark: TileSystemMark;
  /** The highlighted game, or null while a "See all" cell is active. */
  selectedId: number | null;
  /** The cell to keep rendered and in view, -1 for none. */
  activeCell: number;
  width: number;
  seeAllId: string;
  savedLeft: number;
  onScrolled(key: string, left: number): void;
  style: CSSProperties;
}

/** One shelf: the shelf-rule header with its "See all", over a strip of
 *  tiles that scrolls sideways on its own and is virtualised. */
function Shelf({
  section,
  shelfIndex,
  entries,
  systems,
  generation,
  art,
  size,
  systemMark,
  selectedId,
  activeCell,
  width,
  seeAllId,
  savedLeft,
  onScrolled,
  style,
}: ShelfProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const metrics = COVER_SIZES[size];
  const cellW = metrics.tileW + GAP;
  const shown = Math.min(section.count, SHELF_LIMIT);
  const more = section.count > SHELF_LIMIT;
  const cells = shown + (more ? 1 : 0);
  const count = section.count.toLocaleString(locale);
  const group = sectionLabel(section, t);
  const stripRef = useRef<HTMLDivElement | null>(null);
  const rangeExtractor = useActiveRange(activeCell, null);
  const getItemKey = useCallback(
    (i: number) =>
      i < shown ? (entries[section.start + i]?.id ?? i) : 'see-all',
    [entries, section.start, shown],
  );
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    horizontal: true,
    count: cells,
    getScrollElement: () => stripRef.current,
    estimateSize: () => cellW,
    paddingStart: GAP,
    paddingEnd: GAP,
    overscan: 2,
    getItemKey,
    rangeExtractor,
    initialOffset: savedLeft,
    initialRect: { width, height: metrics.tileH + 2 * GAP },
  });

  // A shelf scrolled back into view comes back where it was left.
  useLayoutEffect(() => {
    const el = stripRef.current;
    if (el && savedLeft > 0) el.scrollLeft = savedLeft;
    // Only on mount: later scrolls are the user's.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (activeCell >= 0)
      virtualizer.scrollToIndex(activeCell, { align: 'auto' });
  }, [activeCell, virtualizer]);

  return (
    <div
      role="rowgroup"
      className="shelf"
      data-shelf-index={shelfIndex}
      data-shelf-key={section.key}
      style={style}
    >
      <SectionHeaderRow
        section={section}
        colSpan={Math.max(1, cells)}
        rowIndex={shelfIndex * 2}
        style={{ position: 'relative', height: SECTION_ROW_H }}
      >
        <Button
          variant="link"
          size="sm"
          className="shelf__see-all"
          aria-label={t('layouts.seeAllIn', { n: count, group })}
          onClick={() => openSeeAll(section, systems)}
        >
          {t('layouts.seeAll')}
        </Button>
      </SectionHeaderRow>
      <div
        ref={stripRef}
        role="row"
        aria-rowindex={shelfIndex * 2 + 2}
        className="shelf__strip"
        style={{ height: metrics.tileH + 2 * GAP }}
        onScroll={(ev) => onScrolled(section.key, ev.currentTarget.scrollLeft)}
      >
        <div
          className="shelf__track"
          style={{ width: virtualizer.getTotalSize() }}
        >
          {virtualizer.getVirtualItems().map((item) => {
            const at: CSSProperties = {
              position: 'absolute',
              top: GAP,
              left: 0,
              transform: `translateX(${item.start}px)`,
            };
            if (item.index >= shown) {
              return (
                <div
                  key={item.key}
                  id={seeAllId}
                  role="gridcell"
                  aria-colindex={item.index + 1}
                  aria-selected={activeCell === item.index}
                  aria-label={t('layouts.seeAllCount', { n: count })}
                  data-see-all="true"
                  className={cn(
                    'shelf__more',
                    (size === 'xs' || size === 's') && 'shelf__more--small',
                  )}
                  style={{
                    ...at,
                    width: metrics.tileW,
                    height: metrics.tileH,
                  }}
                >
                  <span aria-hidden="true" className="shelf__more-count">
                    {count}
                  </span>
                  <span aria-hidden="true" className="shelf__more-label">
                    {t('layouts.seeAll')}
                  </span>
                </div>
              );
            }
            const entry = entries[section.start + item.index];
            if (!entry) return null;
            return (
              <CoverTile
                key={item.key}
                entry={entry}
                system={systems[entry.system]}
                generation={generation}
                art={art}
                size={size}
                selected={entry.id === selectedId}
                systemMark={systemMark}
                colIndex={item.index + 1}
                style={at}
              />
            );
          })}
        </div>
      </div>
    </div>
  );
}

/**
 * Shelves (⌘6): one shelf per group (by system unless another grouping is
 * chosen), each a sideways strip of at most 60 covers ending in "See all".
 * ←/→ move along a shelf; ↑/↓ go to the game in the next shelf whose left
 * edge is nearest on screen, since every shelf keeps its own scroll.
 */
export function ShelvesView({
  entries: input,
  sections: inputSections,
  prefs,
  systems,
  generation,
  selectedId,
  onSelect,
  onOpen,
  onPreview,
}: ViewProps) {
  const t = useT();
  const baseId = useId();
  const stop = prefs.coverSize;
  const groupBy = prefs.groupBy === 'none' ? 'system' : prefs.groupBy;
  // Shelves are always grouped: ungrouped is shown by system (spec §B.5).
  const { entries, sections } = useMemo(() => {
    const grouped =
      prefs.groupBy === 'none'
        ? groupEntries(input, 'system', {
            systems,
            regionOrder: REGIONS,
            artMedia: MEDIA,
          })
        : { ordered: input, sections: inputSections };
    return {
      entries: grouped.ordered,
      sections: grouped.sections.filter((s) => s.count > 0),
    };
  }, [input, inputSections, prefs.groupBy, systems]);
  const systemMark: TileSystemMark = groupBy === 'system' ? 'none' : 'badge';

  const [ref, size, element] = useElementSize<HTMLDivElement>();
  const shelfH = shelfHeight(stop);
  const pageShelves = Math.max(1, Math.floor(size.height / shelfH));
  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  const selectedIndex =
    selectedId === null ? -1 : (indexOf.get(selectedId) ?? -1);
  const activeShelf =
    selectedIndex < 0 ? -1 : shelfOfIndex(sections, selectedIndex);
  const activePos =
    activeShelf < 0 ? -1 : selectedIndex - sections[activeShelf]!.start;

  // A "See all" cell is not a game: it is active only until the selection
  // moves on.
  const [seeAll, setSeeAll] = useState<{
    shelf: number;
    forId: number | null;
  } | null>(null);
  const seeAllShelf =
    seeAll && seeAll.forId === selectedId && seeAll.shelf < sections.length
      ? seeAll.shelf
      : null;
  const cursorShelf = seeAllShelf ?? activeShelf;
  const seeAllId = (shelf: number) => `${baseId}-all-${shelf}`;

  const scrollLefts = useRef(new Map<string, number>());
  const onScrolled = useCallback((key: string, left: number) => {
    scrollLefts.current.set(key, left);
  }, []);

  const rangeExtractor = useActiveRange(cursorShelf, null);
  const getItemKey = useCallback(
    (i: number) => `${sections[i]?.key ?? i}:${stop}`,
    [sections, stop],
  );
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: sections.length,
    getScrollElement: () => element.current,
    estimateSize: () => shelfH,
    overscan: 1,
    getItemKey,
    rangeExtractor,
    initialRect: size,
  });

  const keys = useViewKeys({
    entries,
    sections,
    selectedIndex,
    onSelect,
    onOpen,
    onPreview,
    scrollToIndex: (i) => {
      const shelf = shelfOfIndex(sections, i);
      if (shelf >= 0) virtualizer.scrollToIndex(shelf, { align: 'auto' });
    },
    move: (key) => {
      const first = sections[0];
      if (!first) return null;
      if (activeShelf < 0) {
        return ['ArrowRight', 'ArrowLeft', 'Home', 'End'].includes(key)
          ? first.start
          : null;
      }
      const section = sections[activeShelf]!;
      const shown = Math.min(section.count, SHELF_LIMIT);
      const pos = Math.min(activePos, shown - 1);
      switch (key) {
        case 'ArrowRight':
          return pos + 1 < shown ? section.start + pos + 1 : null;
        case 'ArrowLeft':
          return pos > 0 ? section.start + pos - 1 : null;
        case 'Home':
          return section.start;
        case 'End':
          return section.start + shown - 1;
        default:
          return null;
      }
    },
  });

  function selectEntry(index: number) {
    const entry = entries[index];
    setSeeAll(null);
    if (entry) onSelect(entry.id);
  }

  /** ↑/↓ (and Page keys): onto shelf `to`, at the cell whose left edge is
   *  nearest the current one on screen, or the same position if the shelf
   *  is not rendered. */
  function moveToShelf(to: number, nearest: boolean) {
    const target = sections[to];
    if (!target) return;
    const current = document.getElementById(
      seeAllShelf !== null ? seeAllId(seeAllShelf) : `entry-${selectedId}`,
    );
    const shelfEl = element.current?.querySelector(
      `[data-shelf-index="${to}"]`,
    );
    const candidates = shelfEl
      ? [...shelfEl.querySelectorAll<HTMLElement>('[role="gridcell"]')]
      : [];
    if (nearest && current && candidates.length > 0) {
      const left = current.getBoundingClientRect().left;
      const hit =
        candidates[
          nearestByLeft(
            left,
            candidates.map((c) => c.getBoundingClientRect().left),
          )
        ];
      if (hit?.dataset.seeAll) {
        setSeeAll({ shelf: to, forId: selectedId });
        virtualizer.scrollToIndex(to, { align: 'auto' });
        return;
      }
      const id = entryIdOf(hit ?? null);
      if (id !== null) {
        setSeeAll(null);
        onSelect(id);
        return;
      }
    }
    const shown = Math.min(target.count, SHELF_LIMIT);
    const pos = seeAllShelf !== null ? shown - 1 : Math.max(0, activePos);
    selectEntry(target.start + Math.min(pos, shown - 1));
  }

  function onKeyDown(ev: KeyboardEvent<HTMLDivElement>) {
    if (ev.target !== ev.currentTarget || ev.key === 'Tab') return;
    const plain = !ev.altKey && !ev.ctrlKey && !ev.metaKey;
    const vertical: Readonly<Record<string, number>> = {
      ArrowDown: 1,
      ArrowUp: -1,
      PageDown: pageShelves,
      PageUp: -pageShelves,
    };
    const step = vertical[ev.key];
    if (plain && step !== undefined) {
      ev.preventDefault();
      if (cursorShelf < 0) {
        if (sections[0]) selectEntry(sections[0].start);
        return;
      }
      const to = Math.min(sections.length - 1, Math.max(0, cursorShelf + step));
      if (to !== cursorShelf) moveToShelf(to, Math.abs(step) === 1);
      return;
    }
    if (plain && seeAllShelf !== null) {
      const section = sections[seeAllShelf]!;
      const shown = Math.min(section.count, SHELF_LIMIT);
      switch (ev.key) {
        case 'Enter':
          ev.preventDefault();
          openSeeAll(section, systems);
          return;
        case 'ArrowLeft':
          ev.preventDefault();
          selectEntry(section.start + shown - 1);
          return;
        case 'Home':
          ev.preventDefault();
          selectEntry(section.start);
          return;
        case 'ArrowRight':
        case 'End':
        case ' ':
          // The last cell; nothing to preview.
          ev.preventDefault();
          return;
      }
    }
    if (plain && activeShelf >= 0 && seeAllShelf === null) {
      const section = sections[activeShelf]!;
      const shown = Math.min(section.count, SHELF_LIMIT);
      const last = activePos >= shown - 1;
      if (
        section.count > SHELF_LIMIT &&
        (ev.key === 'End' || (ev.key === 'ArrowRight' && last))
      ) {
        ev.preventDefault();
        setSeeAll({ shelf: activeShelf, forId: selectedId });
        return;
      }
    }
    if (seeAllShelf !== null) setSeeAll(null);
    keys(ev);
  }

  function onClick(ev: MouseEvent<HTMLDivElement>) {
    // The "See all" cell: the grid handles its click, as it does a tile's.
    const more =
      ev.target instanceof Element
        ? ev.target.closest('[data-see-all]')?.closest('[data-shelf-index]')
        : null;
    const shelf = sections[Number(more?.getAttribute('data-shelf-index'))];
    if (more && shelf) {
      openSeeAll(shelf, systems);
      return;
    }
    const id = entryIdOf(ev.target);
    if (id !== null) {
      setSeeAll(null);
      onOpen(id);
    }
  }

  const maxCells = sections.reduce(
    (m, s) =>
      Math.max(
        m,
        Math.min(s.count, SHELF_LIMIT) + (s.count > SHELF_LIMIT ? 1 : 0),
      ),
    1,
  );
  const activeVisible =
    activeShelf >= 0 && activePos >= 0 && activePos < SHELF_LIMIT;
  const activeDescendant =
    seeAllShelf !== null
      ? seeAllId(seeAllShelf)
      : activeVisible && selectedId !== null
        ? `entry-${selectedId}`
        : undefined;

  return (
    <div
      ref={ref}
      role="grid"
      aria-label={t('layouts.shelves')}
      aria-rowcount={sections.length * 2}
      aria-colcount={maxCells}
      aria-activedescendant={activeDescendant}
      tabIndex={0}
      className="shelves"
      onKeyDown={onKeyDown}
      onClick={onClick}
    >
      <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
        {virtualizer.getVirtualItems().map((item) => {
          const section = sections[item.index];
          if (!section) return null;
          const activeCell =
            seeAllShelf === item.index
              ? Math.min(section.count, SHELF_LIMIT)
              : item.index === activeShelf && activeVisible
                ? activePos
                : -1;
          return (
            <Shelf
              key={item.key}
              section={section}
              shelfIndex={item.index}
              entries={entries}
              systems={systems}
              generation={generation}
              art={prefs.art}
              size={stop}
              systemMark={systemMark}
              selectedId={seeAllShelf === null ? selectedId : null}
              activeCell={activeCell}
              width={size.width}
              seeAllId={seeAllId(item.index)}
              savedLeft={scrollLefts.current.get(section.key) ?? 0}
              onScrolled={onScrolled}
              style={{
                transform: `translateY(${item.start}px)`,
                height: shelfH,
              }}
            />
          );
        })}
      </div>
    </div>
  );
}
