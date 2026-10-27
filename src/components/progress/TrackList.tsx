import {
  memo,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';

import { SlotMark } from '@/components/ui/slot-mark';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { TrackRow } from '@/lib/extractProgress';
import { formatDecimalSize } from '@/lib/format';
import { cn, focusRing } from '@/lib/utils';
import './progress.css';

export interface TrackListProps {
  /** The current item's entries in archive order (`extractView().rows`). */
  rows: TrackRow[];
  noun: 'track' | 'file';
  /** The archive's name, for the list's accessible name. */
  name: string;
  /** Behind a disclosure, collapsed at first (the list wraps below the stage). */
  collapsible?: boolean;
}

/** How long a user's own scroll of the list holds off the follow. */
const USER_HOLD_MS = 5000;
/** The end of a name (` (Track 18).bin`) never gives way to the column;
 * the head before it takes the one ellipsis (middle truncation in CSS). */
const TAIL = 15;

interface RowProps {
  row: TrackRow;
  stateText: string;
  locale: string;
}

/**
 * Memoised by the row's own fields: a tick re-renders the two rows it moves,
 * and the next game's rows (same index, maybe the same state) still repaint.
 */
const Row = memo(
  function Row({ row, stateText, locale }: RowProps) {
    const done = row.state === 'done';
    // By code point, so an astral character never straddles the two spans.
    const chars = Array.from(row.name);
    const cut = Math.max(0, chars.length - TAIL);
    return (
      <li
        data-index={row.index}
        aria-current={row.state === 'current' ? 'step' : undefined}
        className={cn(
          'grid h-7 grid-cols-[auto_minmax(0,1fr)_auto_1rem] items-center gap-2 px-2 text-sm',
          row.state === 'waiting' ? 'text-cr-text-secondary' : 'text-cr-text',
        )}
      >
        <SlotMark
          state={done ? 'applied' : 'missing'}
          size={16}
          className={cn('tl-mark', row.state === 'current' && 'ws-march')}
        />
        <span className="mono min-w-0 overflow-hidden whitespace-nowrap">
          <bdi title={row.full} className="flex min-w-0">
            <span className="tl-head truncate">
              {chars.slice(0, cut).join('')}
            </span>
            <span className="tl-tail shrink-0 whitespace-pre">
              {chars.slice(cut).join('')}
            </span>
          </bdi>
        </span>
        <span className="mono text-xs tabular-nums text-cr-text-secondary">
          {formatDecimalSize(row.size, locale)}
        </span>
        <span aria-hidden className="text-center text-cr-slot-applied">
          {done ? '✓' : ''}
        </span>
        <span className="sr-only">{stateText}</span>
      </li>
    );
  },
  (a, b) =>
    a.row.index === b.row.index &&
    a.row.state === b.row.state &&
    a.row.full === b.row.full &&
    a.row.name === b.row.name &&
    a.row.size === b.row.size &&
    a.row.checked === b.row.checked &&
    a.stateText === b.stateText &&
    a.locale === b.locale,
);

/**
 * The tracks of the game being extracted (spec §6.3), ticking ✓ as each
 * file is checked. Not a live region: the readout's counter carries the
 * progress. It follows the current row with its own `scrollTop` (never
 * `scrollIntoView`, which would also scroll the page), unless the user
 * scrolled it in the last 5 s; smooth only when motion is welcome (CSS).
 */
export function TrackList({
  rows,
  noun,
  name,
  collapsible = false,
}: TrackListProps) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const id = useId();
  const [opened, setOpen] = useState(false);
  // Not collapsible (a wide window) always shows the list, even if it was
  // collapsed while narrow.
  const open = !collapsible || opened;
  const listRef = useRef<HTMLOListElement>(null);
  const userAt = useRef(Number.NEGATIVE_INFINITY);
  const current = rows.findIndex((r) => r.state === 'current');

  const shown = rows.length > 0;

  // The user's own hand on the list (wheel, touch, scrollbar, keys) holds
  // off the follow; plain scroll events cannot tell it from ours.
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const mark = () => {
      userAt.current = Date.now();
    };
    const kinds = ['wheel', 'touchstart', 'pointerdown', 'keydown'] as const;
    kinds.forEach((k) => list.addEventListener(k, mark, { passive: true }));
    return () => kinds.forEach((k) => list.removeEventListener(k, mark));
  }, [shown]);

  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list || !open || current < 0) return;
    if (Date.now() - userAt.current < USER_HOLD_MS) return;
    const rowEl = list.children[current] as HTMLElement | undefined;
    if (!rowEl) return;
    list.scrollTop = Math.max(0, rowEl.offsetTop - list.clientHeight / 2);
  }, [current, open]);

  if (rows.length === 0) return null;

  const stateText = (r: TrackRow) =>
    r.state === 'done'
      ? t(
          r.checked === 'crc'
            ? 'organize.progress.crcChecked'
            : 'organize.progress.sizeChecked',
        )
      : t(
          r.state === 'current'
            ? 'organize.progress.writing'
            : 'organize.progress.waiting',
        );
  const tracks = noun === 'track';
  const count = new Intl.NumberFormat(locale).format(rows.length);

  return (
    <div className="grid min-w-0 gap-1">
      {collapsible && (
        <button
          type="button"
          aria-expanded={open}
          aria-controls={id}
          onClick={() => setOpen((v) => !v)}
          className={cn(
            'justify-self-start rounded-sm text-sm text-cr-text-secondary underline-offset-2 hover:text-cr-text hover:underline',
            focusRing,
          )}
        >
          {opened
            ? t(
                tracks
                  ? 'organize.progress.hideTracks'
                  : 'organize.progress.hideFiles',
              )
            : tr(
                tracks
                  ? 'organize.progress.showTracks'
                  : 'organize.progress.showFiles',
                { count },
              )}
        </button>
      )}
      <ol
        id={id}
        ref={listRef}
        hidden={!open}
        // A scrolling region must be reachable from the keyboard (WCAG 2.1.1).
        // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
        tabIndex={0}
        aria-label={t(
          tracks
            ? 'organize.progress.tracksLabel'
            : 'organize.progress.filesLabel',
          { name },
        )}
        className={cn(
          'tl-list relative max-h-[calc(8*1.75rem)] overflow-y-auto rounded-md border border-cr-border',
          focusRing,
        )}
      >
        {open &&
          rows.map((r) => (
            <Row
              key={r.index}
              row={r}
              stateText={stateText(r)}
              locale={locale}
            />
          ))}
      </ol>
    </div>
  );
}
