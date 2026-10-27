import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { RowState, TrackRow } from '@/lib/extractProgress';

import { TrackList, type TrackListProps } from './TrackList';

const LONG = 'Tetris Plus (Europe) (En,Fr,De,Es,It) (Disc 1) (Track 18).bin';

/** 12 rows: 4 done, one current, 7 waiting; row 2 has no stored CRC. */
function makeRows(done = 4, total = 12): TrackRow[] {
  return Array.from({ length: total }, (_, k) => {
    const state: RowState =
      k < done ? 'done' : k === done ? 'current' : 'waiting';
    const name =
      k === 6
        ? LONG
        : `Tetris Plus (Track ${String(k + 1).padStart(2, '0')}).bin`;
    return {
      index: k,
      name,
      full: `Tetris Plus/${name}`,
      size: 25_000_000 + k,
      checked: k === 2 ? 'size' : 'crc',
      state,
    };
  });
}

const BASE: TrackListProps = {
  rows: makeRows(),
  noun: 'track',
  name: 'Tetris Plus.zip',
};

function renderList(props: Partial<TrackListProps> = {}) {
  const ui = (p: Partial<TrackListProps>) => (
    <I18nProvider locale="es">
      <TrackList {...BASE} {...p} />
    </I18nProvider>
  );
  const r = render(ui(props));
  return {
    ...r,
    update: (next: Partial<TrackListProps>) =>
      r.rerender(ui({ ...props, ...next })),
  };
}

const scrollIntoView = Element.prototype.scrollIntoView;

afterEach(() => {
  Element.prototype.scrollIntoView = scrollIntoView;
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('TrackList', () => {
  it('names the list after the game and is not a live region', () => {
    const { container } = renderList();
    const list = screen.getByRole('list', {
      name: 'Pistas de Tetris Plus.zip',
    });
    expect(list.tagName).toBe('OL');
    expect(within(list).getAllByRole('listitem')).toHaveLength(12);
    expect(container.querySelector('[aria-live]')).toBeNull();
    expect(container.querySelector('[role="status"], [role="log"]')).toBeNull();
  });

  it('says «Archivos de» for files', () => {
    renderList({ noun: 'file' });
    expect(
      screen.getByRole('list', { name: 'Archivos de Tetris Plus.zip' }),
    ).toBeInTheDocument();
  });

  it('ticks done rows and states every row in hidden text', () => {
    renderList();
    const items = within(screen.getByRole('list')).getAllByRole('listitem');
    // Done, with a stored CRC.
    expect(items[0]).toHaveTextContent('✓');
    expect(within(items[0]!).getByText('CRC comprobado')).toHaveClass(
      'sr-only',
    );
    // Done, no stored CRC: size and copy.
    expect(items[2]).toHaveTextContent('✓');
    expect(
      within(items[2]!).getByText('Tamaño y copia comprobados'),
    ).toHaveClass('sr-only');
    // Current and waiting: no tick.
    expect(items[4]).not.toHaveTextContent('✓');
    expect(within(items[4]!).getByText('Escribiendo')).toHaveClass('sr-only');
    expect(items[4]).toHaveAttribute('aria-current', 'step');
    expect(items[5]).not.toHaveTextContent('✓');
    expect(within(items[5]!).getByText('En espera')).toHaveClass('sr-only');
  });

  it('draws the slot for each state, marching only on the current row', () => {
    const { container } = renderList();
    const marks = container.querySelectorAll('li svg');
    expect(marks[0]).toHaveAttribute('data-state', 'applied');
    expect(marks[4]).toHaveAttribute('data-state', 'missing');
    expect(marks[5]).toHaveAttribute('data-state', 'missing');
    expect(container.querySelectorAll('.ws-march')).toHaveLength(1);
    expect(marks[4]).toHaveClass('ws-march');
  });

  it('isolates names in bdi with the full path as title, truncated in the middle', () => {
    renderList();
    const items = within(screen.getByRole('list')).getAllByRole('listitem');
    items.forEach((li, k) => {
      const bdi = li.querySelector('bdi');
      expect(bdi).not.toBeNull();
      expect(bdi).toHaveAttribute('title', BASE.rows[k]!.full);
    });
    // One ellipsis, drawn by the browser on the head only where the column
    // is too narrow; the text itself stays whole for readers.
    const long = items[6]!.querySelector('bdi')!;
    expect(long.textContent).toBe(LONG);
    expect(long.textContent).not.toContain('…');
  });

  it('keeps the end of a name visible when the column is narrower than the text', () => {
    renderList();
    const items = within(screen.getByRole('list')).getAllByRole('listitem');
    items.forEach((li) => {
      const head = li.querySelector('bdi > .tl-head');
      const tail = li.querySelector('bdi > .tl-tail');
      expect(head).toHaveClass('truncate');
      expect(tail).toHaveClass('shrink-0');
    });
    const short = items[0]!.querySelector('bdi')!;
    expect(short.querySelector('.tl-tail')!.textContent).toBe(
      ' (Track 01).bin',
    );
    expect(short.textContent).toBe('Tetris Plus (Track 01).bin');
    const long = items[6]!.querySelector('bdi')!;
    expect(long.querySelector('.tl-tail')!.textContent).toBe(' (Track 18).bin');
  });

  it('never splits a character between the head and the tail of a name', () => {
    // An astral character (two UTF-16 units) straddling the 15th code unit
    // from the end: a code-unit split would leave a lone surrogate per span.
    const name = 'Tetris Plus 🎮(Track 01).bin';
    const rows = makeRows(0, 1).map((r) => ({ ...r, name, full: name }));
    renderList({ rows });
    const bdi = within(screen.getByRole('list'))
      .getAllByRole('listitem')[0]!
      .querySelector('bdi')!;
    const lone =
      /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/;
    for (const part of ['.tl-head', '.tl-tail']) {
      expect(bdi.querySelector(part)!.textContent).not.toMatch(lone);
    }
    expect(bdi.textContent).toBe(name);
  });

  it('shows sizes in decimal units', () => {
    renderList();
    const items = within(screen.getByRole('list')).getAllByRole('listitem');
    expect(items[0]).toHaveTextContent('25 MB');
  });

  it('renders nothing without rows', () => {
    const { container } = renderList({ rows: [] });
    expect(container).toBeEmptyDOMElement();
  });

  it('collapses behind a disclosure button that controls the list', () => {
    renderList({ collapsible: true });
    const button = screen.getByRole('button', { name: 'Ver las 12 pistas' });
    expect(button).toHaveAttribute('aria-expanded', 'false');
    const list = document.getElementById(button.getAttribute('aria-controls')!);
    expect(list?.tagName).toBe('OL');
    expect(list).not.toBeVisible();
    // A closed list keeps no rows: each tick would repaint them for nothing.
    expect(list!.children).toHaveLength(0);
    fireEvent.click(button);
    expect(button).toHaveAttribute('aria-expanded', 'true');
    expect(list).toBeVisible();
    expect(list!.children).toHaveLength(12);
    // Open, the button offers what a press does next.
    expect(button).toHaveAccessibleName('Ocultar las pistas');
    fireEvent.click(button);
    expect(button).toHaveAttribute('aria-expanded', 'false');
    expect(button).toHaveAccessibleName('Ver las 12 pistas');
  });

  it('says «Ver los 12 archivos» for files', () => {
    renderList({ collapsible: true, noun: 'file' });
    expect(
      screen.getByRole('button', { name: 'Ver los 12 archivos' }),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole('button', { name: 'Ver los 12 archivos' }),
    );
    expect(
      screen.getByRole('button', { name: 'Ocultar los archivos' }),
    ).toBeInTheDocument();
  });

  it('keeps the current row in view with its own scrollTop, never scrollIntoView', () => {
    const spy = vi.fn();
    Element.prototype.scrollIntoView = spy;
    vi.spyOn(HTMLElement.prototype, 'offsetTop', 'get').mockImplementation(
      function (this: HTMLElement) {
        const k = Number(this.dataset.index ?? 0);
        return k * 28;
      },
    );
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(224);
    const { update } = renderList({ rows: makeRows(0) });
    const list = screen.getByRole('list');
    for (let d = 1; d <= 10; d++) update({ rows: makeRows(d) });
    expect(spy).not.toHaveBeenCalled();
    // Row 10 is current: 10 × 28 − 224 / 2.
    expect(list.scrollTop).toBe(168);
  });

  it('leaves the list alone for 5 s after the user scrolls it', () => {
    vi.useFakeTimers();
    vi.spyOn(HTMLElement.prototype, 'offsetTop', 'get').mockImplementation(
      function (this: HTMLElement) {
        return Number(this.dataset.index ?? 0) * 28;
      },
    );
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(224);
    const { update } = renderList({ rows: makeRows(6) });
    const list = screen.getByRole('list');
    expect(list.scrollTop).toBe(56);
    fireEvent.wheel(list);
    list.scrollTop = 0;
    update({ rows: makeRows(8) });
    expect(list.scrollTop).toBe(0);
    act(() => {
      vi.advanceTimersByTime(5001);
    });
    update({ rows: makeRows(9) });
    expect(list.scrollTop).toBe(140);
  });

  it("shows the next game's names on rows whose index and state repeat", () => {
    // Game 1 last seen mid-way; game 2 starts: row 11 stays «waiting».
    const { update } = renderList({ rows: makeRows(10) });
    const next = makeRows(0).map((r) => ({
      ...r,
      name: `Bust-A-Move (Track ${r.index + 1}).bin`,
      full: `Bust-A-Move/Bust-A-Move (Track ${r.index + 1}).bin`,
      size: 1_000_000,
      checked: 'size' as const,
    }));
    update({ rows: next, name: 'Bust-A-Move.zip' });
    const items = within(
      screen.getByRole('list', { name: 'Pistas de Bust-A-Move.zip' }),
    ).getAllByRole('listitem');
    expect(items[11]!.querySelector('bdi')).toHaveAttribute(
      'title',
      'Bust-A-Move/Bust-A-Move (Track 12).bin',
    );
    expect(items[11]).toHaveTextContent('1 MB');
  });

  it('shows the list when it stops being collapsible while collapsed', () => {
    const { update } = renderList({ collapsible: true });
    expect(screen.getByRole('list', { hidden: true })).not.toBeVisible();
    update({ collapsible: false });
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.getByRole('list')).toBeVisible();
  });

  it('re-renders only the rows whose state changed', () => {
    // Memoised rows keep their DOM node identity and content across ticks.
    const { update } = renderList();
    const before = within(screen.getByRole('list')).getAllByRole('listitem');
    update({ rows: makeRows(5) });
    const after = within(screen.getByRole('list')).getAllByRole('listitem');
    after.forEach((li, k) => expect(li).toBe(before[k]));
    expect(after[4]).toHaveTextContent('✓');
    expect(after[5]).toHaveAttribute('aria-current', 'step');
  });
});
