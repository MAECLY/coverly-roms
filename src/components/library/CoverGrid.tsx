import { useVirtualizer } from '@tanstack/react-virtual';
import { useMemo, type KeyboardEvent, type MouseEvent } from 'react';

import { useElementSize } from '@/hooks/useElementSize';
import { useT } from '@/i18n/useT';
import type { EntryView, SystemView } from '@/ipc/types';
import { nextIndex } from '@/lib/keyboardNav';

import { ArtStatusIcon } from './ArtStatusIcon';
import { ArtThumb } from './ArtThumb';

export const TILE_W = 168;
export const TILE_H = 264;
export const GAP = 12;

export interface EntryListProps {
  entries: readonly EntryView[];
  systems: readonly SystemView[];
  generation: number;
  selectedId: number | null;
  /** The keyboard moved the selection. */
  onSelect(id: number): void;
  /** Click or Enter: select and show details. */
  onOpen(id: number): void;
}

/** The entry id of the item an event happened in, if any. */
// eslint-disable-next-line react-refresh/only-export-components -- shared with EntryTable (plan interface)
export function entryIdOf(target: EventTarget | null): number | null {
  const el =
    target instanceof Element ? target.closest('[data-entry-id]') : null;
  const raw = el?.getAttribute('data-entry-id');
  return raw == null ? null : Number(raw);
}

export function CoverGrid({
  entries,
  systems,
  generation,
  selectedId,
  onSelect,
  onOpen,
}: EntryListProps) {
  const t = useT();
  const [ref, size, element] = useElementSize<HTMLDivElement>();
  const columns = Math.max(1, Math.floor((size.width - GAP) / (TILE_W + GAP)));
  const rows = Math.ceil(entries.length / columns);
  const pageRows = Math.max(1, Math.floor(size.height / (TILE_H + GAP)));
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: rows,
    getScrollElement: () => element.current,
    estimateSize: () => TILE_H + GAP,
    overscan: 2,
    initialRect: size,
  });
  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  const selectedIndex =
    selectedId === null ? -1 : (indexOf.get(selectedId) ?? -1);

  function onKeyDown(ev: KeyboardEvent<HTMLDivElement>) {
    if (ev.key === 'Enter') {
      if (selectedId !== null) {
        ev.preventDefault();
        onOpen(selectedId);
      }
      return;
    }
    const next = nextIndex(
      ev.key,
      selectedIndex,
      entries.length,
      columns,
      pageRows,
    );
    const entry = next === null ? undefined : entries[next];
    if (next === null || !entry) return;
    ev.preventDefault();
    onSelect(entry.id);
    virtualizer.scrollToIndex(Math.floor(next / columns), { align: 'auto' });
  }

  function onClick(ev: MouseEvent<HTMLDivElement>) {
    const id = entryIdOf(ev.target);
    if (id !== null) onOpen(id);
  }

  return (
    <div
      ref={ref}
      role="grid"
      aria-label={t('library.grid.label')}
      aria-rowcount={rows}
      aria-colcount={columns}
      aria-activedescendant={
        selectedId === null ? undefined : `entry-${selectedId}`
      }
      tabIndex={0}
      className="cover-grid"
      onKeyDown={onKeyDown}
      onClick={onClick}
    >
      <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
        {virtualizer.getVirtualItems().map((row) => (
          <div
            key={row.key}
            role="row"
            aria-rowindex={row.index + 1}
            className="cover-row"
            style={{
              transform: `translateY(${row.start}px)`,
              height: TILE_H,
              gap: GAP,
              paddingLeft: GAP,
            }}
          >
            {entries
              .slice(row.index * columns, (row.index + 1) * columns)
              .map((entry) => (
                <div
                  key={entry.id}
                  id={`entry-${entry.id}`}
                  role="gridcell"
                  aria-selected={entry.id === selectedId}
                  data-entry-id={entry.id}
                  className="cover-tile"
                  style={{ width: TILE_W }}
                  title={entry.name}
                >
                  <ArtThumb
                    generation={generation}
                    entry={entry}
                    media="cover"
                  />
                  <span className="cover-tile__name">{entry.name}</span>
                  <span className="cover-tile__meta">
                    {systems[entry.system]?.folder ?? ''} · {entry.ext}
                  </span>
                  <span className="cover-tile__marks">
                    <ArtStatusIcon media="cover" mark={entry.art.cover} />
                    <ArtStatusIcon
                      media="screenshot"
                      mark={entry.art.screenshot}
                    />
                    <ArtStatusIcon media="title" mark={entry.art.title} />
                  </span>
                </div>
              ))}
          </div>
        ))}
      </div>
    </div>
  );
}
