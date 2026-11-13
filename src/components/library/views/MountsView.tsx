import { useVirtualizer } from '@tanstack/react-virtual';
import {
  useCallback,
  useContext,
  useId,
  useMemo,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';

import { SystemLabel } from '@/components/system/SystemLabel';
import { SlotMark, type SlotState } from '@/components/ui/slot-mark';
import { Toggle } from '@/components/ui/toggle';
import { useActiveRange, stickyHeaderAt } from '@/hooks/useActiveRange';
import { useChildArtUrl } from '@/hooks/useChildArtUrl';
import { useElementSize } from '@/hooks/useElementSize';
import { LocaleContext } from '@/i18n/core';
import { useT, type TFunction } from '@/i18n/useT';
import type {
  ArtMark,
  ChildMedia,
  ChildOrigin,
  ChildView,
  EntryView,
  Media,
} from '@/ipc/types';
import { childCaption, ORIGIN_KEY, trayChildren } from '@/lib/childArt';
import { formatSize } from '@/lib/format';
import type { Game } from '@/lib/games';
import { nextIndex } from '@/lib/keyboardNav';
import {
  buildRowModel,
  coveringSections,
  moveVertical,
  type Row,
} from '@/lib/rowModel';
import { titleWithoutRegions } from '@/lib/tileLabel';
import { cn } from '@/lib/utils';
import { useViewStore } from '@/stores/viewStore';

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

/** A child row under its game (addon views §8.6): shorter, two slots. */
export const CHILD_ROW_H = 88;

/** An update's or DLC's slots, after its name cell. */
const CHILD_MEDIA: readonly ChildMedia[] = ['icon', 'banner'];

/** Name + icon + banner. */
const CHILD_COLS = 1 + CHILD_MEDIA.length;

/** Keys that never move the selection, so the keyboard stays on a child
 *  row: modifiers pressed on their own, Tab out of the view, Escape,
 *  Space (preview) and Enter. */
const KEEPS_SUB: ReadonlySet<string> = new Set([
  'Shift',
  'Control',
  'Alt',
  'AltGraph',
  'Meta',
  'CapsLock',
  'Tab',
  'Escape',
  ' ',
  'Enter',
]);

/** A game's row, a header, or one of a game's updates or DLC under it. */
type MountRow =
  | Row
  | {
      kind: 'child';
      /** The game's index in `entries`. */
      index: number;
      game: Game;
      child: ChildView;
      /** Its place among the game's child rows. */
      pos: number;
      last: boolean;
    };

/** How one child slot reads: its own art (with where it came from), the
 *  base game's dimmed, or missing. */
type ChildSlotState =
  | { kind: 'own'; origin: ChildOrigin | undefined }
  | { kind: 'inherit' }
  | { kind: 'missing' };

const CHILD_SLOT_STATE: Record<ChildSlotState['kind'], string> = {
  own: 'filled',
  inherit: 'inherit',
  missing: 'missing',
};

/** A child with no art at all shows the base game's in both slots (§8.6);
 *  otherwise each slot is its own or missing. */
function childSlotState(child: ChildView, slot: ChildMedia): ChildSlotState {
  if (!child.art.icon && !child.art.banner) return { kind: 'inherit' };
  return child.art[slot]
    ? { kind: 'own', origin: child.art[`${slot}_origin`] }
    : { kind: 'missing' };
}

/** «incluido en el paquete», «de ScreenScraper», «tuyo», «falta». */
function slotStateText(state: ChildSlotState, t: TFunction): string {
  switch (state.kind) {
    case 'inherit':
      return t('content.slot.inherits');
    case 'missing':
      return t('content.slot.missing');
    case 'own': {
      const origin = state.origin;
      if (origin === undefined) return t('layouts.slotHas');
      const name = t(`content.origin.${ORIGIN_KEY[origin]}`);
      return origin === 'screenscraper' || origin === 'steamgriddb'
        ? t('content.mounts.from', { source: name })
        : name;
    }
  }
}

/** A child row's name: «DLC Isla del faro», «Actualización v1.2». */
function childRowName(child: ChildView, game: Game, t: TFunction): string {
  const caption = childCaption(child, game, t);
  return child.role === 'dlc'
    ? t('content.mounts.dlc', { name: caption })
    : caption;
}

/** «DLC Isla del faro, icono incluido en el paquete, banner falta». */
function childRowLabel(child: ChildView, game: Game, t: TFunction): string {
  const name = childRowName(child, game, t);
  if (!child.art.icon && !child.art.banner)
    return `${name}, ${t('content.slot.inherits')}`;
  return [
    name,
    ...CHILD_MEDIA.map((slot) =>
      t(`content.mounts.${slot}`, {
        state: slotStateText(childSlotState(child, slot), t),
      }),
    ),
  ].join(', ');
}

/** The id of cell `col` of child row `childIndex` of `entryId`. */
function childCellId(entryId: number, childIndex: number, col: number): string {
  const row = `entry-${entryId}-k${childIndex}`;
  return col === 0 ? row : `${row}-c${col}`;
}

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
  content,
}: ViewProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const setPrefs = useViewStore((s) => s.setPrefs);
  const hintId = useId();
  const [ref, size, element] = useElementSize<HTMLDivElement>();
  // The active cell column; the row is the selection.
  const [col, setCol] = useState(0);
  // The child row of the selected game the keyboard is on, if any; it
  // belongs to that game, so another selection drops it.
  const [sub, setSub] = useState<{ id: number; pos: number } | null>(null);
  const pageRows = Math.max(1, Math.floor(size.height / MOUNT_ROW_H));
  const withHeaders = prefs.groupBy !== 'none';
  const model = useMemo(
    () =>
      buildRowModel(coveringSections(sections, entries.length), 1, withHeaders),
    [sections, entries.length, withHeaders],
  );
  // Each game's child rows (§8.6): its updates with art of their own and
  // every DLC. Counted whether shown or not, for the toggle.
  const byEntry = content?.byEntry;
  const kidsOf = useMemo(
    () =>
      entries.map((e) => {
        const game = byEntry?.get(e.id);
        return game && game.entry.id === e.id ? trayChildren(game) : [];
      }),
    [entries, byEntry],
  );
  const slotCount = useMemo(
    () => kidsOf.reduce((n, kids) => n + kids.length, 0),
    [kidsOf],
  );
  const showKids = slotCount > 0 && prefs.childSlots;
  const { rows, rowOfIndex, headerRows } = useMemo(() => {
    if (!showKids || !byEntry) return model;
    const out: MountRow[] = [];
    const ofIndex = new Int32Array(model.rowOfIndex.length).fill(-1);
    const headers: number[] = [];
    for (const row of model.rows) {
      if (row.kind === 'header') {
        headers.push(out.length);
        out.push(row);
        continue;
      }
      ofIndex.fill(out.length, row.start, row.end);
      out.push(row);
      const kids = kidsOf[row.start] ?? [];
      const game = byEntry.get(entries[row.start]?.id ?? -1);
      if (!game) continue;
      kids.forEach((child, pos) =>
        out.push({
          kind: 'child',
          index: row.start,
          game,
          child,
          pos,
          last: pos === kids.length - 1,
        }),
      );
    }
    return { rows: out, rowOfIndex: ofIndex, headerRows: headers };
  }, [model, showKids, byEntry, kidsOf, entries]);
  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  const selectedIndex =
    selectedId === null ? -1 : (indexOf.get(selectedId) ?? -1);
  const selectedKids = showKids ? (kidsOf[selectedIndex] ?? []) : [];
  const activeSub =
    sub !== null && sub.id === selectedId && sub.pos < selectedKids.length
      ? sub.pos
      : null;
  const activeChild = activeSub === null ? null : selectedKids[activeSub];
  // A child row has fewer cells: the column is kept and clamped.
  const activeCol = activeSub === null ? col : Math.min(col, CHILD_COLS - 1);
  const gameRow = selectedIndex < 0 ? -1 : (rowOfIndex[selectedIndex] ?? -1);
  const activeRow =
    gameRow < 0 ? -1 : activeSub === null ? gameRow : gameRow + 1 + activeSub;
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
        : row.kind === 'child'
          ? `c:${row.game.id}:${row.child.index}`
          : `r:${entries[row.start]?.id ?? row.start}`;
    },
    [rows, entries],
  );
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => element.current,
    estimateSize: (i) => {
      const kind = rows[i]?.kind;
      return kind === 'header'
        ? SECTION_ROW_H
        : kind === 'child'
          ? CHILD_ROW_H
          : MOUNT_ROW_H;
    },
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
      // ←/→ move between the cells of the active row (onKeyDown); with a
      // modifier they reach here and must not move to another row.
      if (selectedIndex >= 0 && (key === 'ArrowLeft' || key === 'ArrowRight')) {
        return null;
      }
      // One item a row: ↑/↓/Home/End are list moves.
      return nextIndex(key, selectedIndex, entries.length, 1, pageRows);
    },
  });

  /** Onto child row `pos` of the game at `index` (`null`: its own row). */
  function enterSub(index: number, pos: number | null) {
    const id = entries[index]?.id;
    if (id === undefined) return;
    setSub(pos === null ? null : { id, pos });
    if (id !== selectedId) onSelect(id);
    const at = rowOfIndex[index] ?? -1;
    if (at >= 0)
      virtualizer.scrollToIndex(at + (pos === null ? 0 : 1 + pos), {
        align: 'auto',
      });
  }

  /** ↑/↓ through the child rows; false leaves the key to the list. */
  function moveSub(key: string): boolean {
    if (key === 'ArrowDown') {
      if (activeSub === null) {
        if (selectedKids.length === 0) return false;
        enterSub(selectedIndex, 0);
        return true;
      }
      if (activeSub + 1 < selectedKids.length) {
        enterSub(selectedIndex, activeSub + 1);
        return true;
      }
      // The last child row of the last game: stay.
      if (selectedIndex === entries.length - 1) return true;
      setSub(null);
      return false;
    }
    if (activeSub !== null) {
      enterSub(selectedIndex, activeSub === 0 ? null : activeSub - 1);
      return true;
    }
    const above = selectedIndex - 1;
    const kids = showKids ? (kidsOf[above] ?? []) : [];
    if (above < 0 || kids.length === 0) return false;
    enterSub(above, kids.length - 1);
    return true;
  }

  function onKeyDown(ev: KeyboardEvent<HTMLDivElement>) {
    const own = ev.target === ev.currentTarget && !ev.nativeEvent.isComposing;
    const entry = entries[selectedIndex];
    if (own && entry && !ev.altKey && !ev.metaKey && !ev.ctrlKey) {
      if (ev.key === 'ArrowLeft' || ev.key === 'ArrowRight') {
        ev.preventDefault();
        const step = ev.key === 'ArrowRight' ? 1 : -1;
        const last = (activeSub === null ? COLS : CHILD_COLS) - 1;
        setCol(Math.min(last, Math.max(0, activeCol + step)));
        return;
      }
      if (
        (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') &&
        !ev.shiftKey &&
        moveSub(ev.key)
      ) {
        ev.preventDefault();
        return;
      }
      if (activeChild && ev.key === 'Enter') {
        ev.preventDefault();
        if (content?.onOpenChild)
          content.onOpenChild(entry.id, activeChild.index);
        else onOpen(entry.id);
        return;
      }
      const media = MOUNT_MEDIA[col - 1];
      if (ev.key === 'Enter' && media) {
        ev.preventDefault();
        if (onOpenArt) onOpenArt(entry.id, media);
        else onOpen(entry.id);
        return;
      }
    }
    // Any other move leaves the child rows for a game's own row; a key
    // that moves nothing (a modifier, Tab out, Space, Enter) keeps it.
    if (activeSub !== null && !KEEPS_SUB.has(ev.key)) setSub(null);
    viewKeys(ev);
  }

  function onClick(ev: MouseEvent<HTMLDivElement>) {
    const id = entryIdOf(ev.target);
    if (id === null) return;
    const cell =
      ev.target instanceof Element ? ev.target.closest('[data-col]') : null;
    const c = Number(cell?.getAttribute('data-col') ?? 0);
    setCol(c);
    const childRow =
      ev.target instanceof Element
        ? ev.target.closest('[data-child-index]')
        : null;
    if (childRow) {
      const pos = Number(childRow.getAttribute('data-child-pos'));
      const index = Number(childRow.getAttribute('data-child-index'));
      setSub({ id, pos });
      if (content?.onOpenChild) content.onOpenChild(id, index);
      else onOpen(id);
      return;
    }
    setSub(null);
    const media = MOUNT_MEDIA[c - 1];
    if (media && onOpenArt) onOpenArt(id, media);
    else onOpen(id);
  }

  const count = new Intl.NumberFormat(locale).format(slotCount);

  return (
    <div className="mounts-shell">
      {slotCount > 0 && (
        <div className="mounts-bar">
          <Toggle
            variant="outline"
            size="sm"
            pressed={prefs.childSlots}
            onPressedChange={(on) => setPrefs({ childSlots: on })}
            aria-label={t('content.mounts.toggle', { n: count })}
            aria-describedby={hintId}
          >
            {t('content.mounts.toggleLabel')}
            <span aria-hidden="true" className="mono tabular-nums">
              ({count})
            </span>
          </Toggle>
          <span id={hintId} className="mounts-bar__hint">
            {t('content.mounts.hint')}
          </span>
        </div>
      )}
      <div
        ref={ref}
        role="grid"
        aria-label={t('layouts.announce', { view: t('layouts.mounts') })}
        data-view-root=""
        aria-rowcount={rows.length}
        aria-colcount={COLS}
        aria-activedescendant={
          selectedIndex < 0 || selectedId === null
            ? undefined
            : activeChild
              ? childCellId(selectedId, activeChild.index, activeCol)
              : cellId(selectedId, col)
        }
        tabIndex={0}
        className="mounts"
        onKeyDown={onKeyDown}
        onClick={onClick}
      >
        <div
          style={{ height: virtualizer.getTotalSize(), position: 'relative' }}
        >
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
            if (row.kind === 'child') {
              const selected =
                row.game.id === selectedId && activeSub === row.pos;
              return (
                <ChildSlotRow
                  key={item.key}
                  row={row}
                  rowIndex={item.index}
                  top={item.start}
                  generation={generation}
                  systemId={systems[row.game.entry.system]?.system_id ?? null}
                  activeCol={selected ? activeCol : null}
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
                  aria-selected={selected && activeSub === null && col === 0}
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
                        ? t(`art.none.${media}`)
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
                      aria-selected={
                        selected && activeSub === null && col === c
                      }
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
    </div>
  );
}

/**
 * One update's or DLC's row under its game (addon views §8.6): an L-shaped
 * hairline back to the game, its Icon and Banner slots (each tagged with
 * where its art came from, or dashed «falta»), then its name. A DLC with
 * no art of its own shows the base game's dimmed. The row's name says it
 * all in words.
 */
function ChildSlotRow({
  row,
  rowIndex,
  top,
  generation,
  systemId,
  activeCol,
}: {
  row: Extract<MountRow, { kind: 'child' }>;
  rowIndex: number;
  top: number;
  generation: number;
  systemId: string | null;
  /** The active cell, when the keyboard is on this row. */
  activeCol: number | null;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const { game, child } = row;
  const id = game.id;
  const inherits = !child.art.icon && !child.art.banner;
  return (
    <div
      role="row"
      aria-rowindex={rowIndex + 1}
      aria-label={childRowLabel(child, game, t)}
      data-child-index={child.index}
      data-child-pos={row.pos}
      data-last={row.last ? 'true' : undefined}
      className="mount-row mount-row--child"
      style={{ transform: `translateY(${top}px)`, height: CHILD_ROW_H }}
    >
      <div
        id={childCellId(id, child.index, 0)}
        role="gridcell"
        aria-colindex={1}
        aria-selected={activeCol === 0}
        data-entry-id={id}
        data-col={0}
        className="mount-row__name"
        title={child.name}
      >
        <span className="mount-row__title">{childRowName(child, game, t)}</span>
        <span className="mount-row__meta">
          <span className="mount-row__size">
            {formatSize(child.size, locale)}
          </span>
          {inherits && (
            <span className="mount-row__fact">
              {t('content.slot.inherits')}
            </span>
          )}
        </span>
      </div>
      {CHILD_MEDIA.map((slot, i) => {
        const c = i + 1;
        const state = childSlotState(child, slot);
        return (
          <ChildSlotCell
            key={slot}
            id={childCellId(id, child.index, c)}
            col={c}
            entryId={id}
            selected={activeCol === c}
            slot={slot}
            state={state}
            generation={generation}
            game={game}
            child={child}
            systemId={systemId}
          />
        );
      })}
    </div>
  );
}

function ChildSlotCell({
  id,
  col,
  entryId,
  selected,
  slot,
  state,
  generation,
  game,
  child,
  systemId,
}: {
  id: string;
  col: number;
  entryId: number;
  selected: boolean;
  slot: ChildMedia;
  state: ChildSlotState;
  generation: number;
  game: Game;
  child: ChildView;
  systemId: string | null;
}) {
  const t = useT();
  const url = useChildArtUrl(
    generation,
    entryId,
    child.index,
    slot,
    state.kind === 'own',
  );
  const stateText = slotStateText(state, t);
  const label = t('content.mounts.slotLabel', {
    slot: t(`content.slot.${slot}`),
    state: stateText,
  });
  const base = game.entry;
  return (
    <div
      id={id}
      role="gridcell"
      aria-colindex={col + 1}
      aria-selected={selected}
      aria-label={label}
      title={label}
      data-entry-id={entryId}
      data-col={col}
      data-slot={slot}
      data-slot-state={CHILD_SLOT_STATE[state.kind]}
      className={cn(
        'mount-slot',
        'mount-child-slot',
        `mount-child-slot--${slot}`,
      )}
    >
      <span className="mount-child-slot__frame">
        {state.kind === 'own' ? (
          url ? (
            <img
              src={url}
              alt=""
              draggable={false}
              decoding="async"
              className="mount-child-slot__img"
            />
          ) : (
            <span className="art-placeholder" />
          )
        ) : state.kind === 'inherit' ? (
          <ArtThumb
            generation={generation}
            entry={base}
            media={
              slot === 'banner' && base.art.title === 'present'
                ? 'title'
                : 'cover'
            }
            placeholder=""
            systemId={systemId}
            className="mount-child-slot__inherit"
          />
        ) : (
          <span className="art-placeholder">{t('content.slot.missing')}</span>
        )}
      </span>
      {/* Always there, so both slots of a row line up. */}
      <span aria-hidden="true" className="mount-child-slot__tag">
        {state.kind === 'own' && state.origin !== undefined
          ? t(`content.mounts.tag.${ORIGIN_KEY[state.origin]}`)
          : ''}
      </span>
    </div>
  );
}
