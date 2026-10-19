import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import { MEDIA, REGIONS, type EntryView } from '@/ipc/types';
import { COVER_GAP, COVER_SIZES } from '@/lib/coverSizes';
import { EMPTY_FILTERS } from '@/lib/filters';
import { groupEntries } from '@/lib/grouping';
import { DEFAULT_PREFS, type CoverSize, type GroupBy } from '@/lib/viewPrefs';
import { useFilterStore } from '@/stores/filterStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

import { ShelvesView } from './ShelvesView';

const big = generateLibrary({
  seed: 12,
  count: 12_000,
  root: '/mock/Big',
  generation: 1,
  now: FIXTURE_NOW,
});

const ofSystem = (system: number, n: number) =>
  big.entries.filter((e) => e.system === system).slice(0, n);

function Harness({
  entries: input,
  groupBy = 'none',
  coverSize = 'm',
  initial = null,
  onOpen = () => {},
  onPreview = () => {},
}: {
  entries: readonly EntryView[];
  groupBy?: GroupBy;
  coverSize?: CoverSize;
  initial?: number | null;
  onOpen?: (id: number) => void;
  onPreview?: (id: number) => void;
}) {
  const [selected, setSelected] = useState<number | null>(initial);
  const { ordered: entries, sections } = groupEntries(input, groupBy, {
    systems: big.systems,
    regionOrder: REGIONS,
    artMedia: MEDIA,
  });
  return (
    <I18nProvider locale="en">
      <ShelvesView
        entries={entries}
        sections={sections}
        prefs={{ ...DEFAULT_PREFS, view: 'shelves', groupBy, coverSize }}
        onPreview={onPreview}
        systems={big.systems}
        generation={1}
        selectedId={selected}
        onSelect={setSelected}
        onOpen={(id) => {
          setSelected(id);
          onOpen(id);
        }}
      />
      <button type="button">after</button>
    </I18nProvider>
  );
}

const grid = () => screen.getByRole('grid', { name: 'Shelves' });
const active = () =>
  document.getElementById(grid().getAttribute('aria-activedescendant') ?? '');
const stripOf = (i: number) =>
  shelfEl(i).querySelector<HTMLElement>('.shelf__strip')!;
const shelfEl = (i: number) =>
  grid().querySelector<HTMLElement>(`[data-shelf-index="${i}"]`)!;

describe('ShelvesView', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1200, 800);
    useViewStore.setState({ scope: { kind: 'all' } });
    useViewStore.getState().setView('shelves');
    useFilterStore.setState({ filters: EMPTY_FILTERS });
  });
  afterEach(() => {
    vi.restoreAllMocks();
    restore();
  });

  it('shows ungrouped games one shelf per system, headed by its icon', () => {
    const a = ofSystem(0, 5);
    const b = ofSystem(1, 5);
    render(<Harness entries={[...b, ...a]} />);
    const headers = within(grid()).getAllByRole('rowheader');
    expect(headers).toHaveLength(2);
    // Sidebar order, whatever order the games came in.
    expect(headers[0]).toHaveAccessibleName(`${big.systems[0]!.name}, 5 games`);
    expect(headers[0]!.querySelector('[data-system-icon]')).not.toBeNull();
    expect(grid()).toHaveAttribute('aria-rowcount', '4');
  });

  it('draws no system on the tiles when shelved by system', () => {
    render(<Harness entries={ofSystem(0, 8)} groupBy="system" />);
    const cells = within(grid()).getAllByRole('gridcell');
    expect(cells.length).toBeGreaterThan(0);
    for (const cell of cells) {
      expect(cell.querySelector('[data-system-icon]')).toBeNull();
    }
  });

  it('badges each tile with its system when shelved another way', () => {
    render(<Harness entries={ofSystem(0, 8)} groupBy="letter" />);
    const cell = within(grid()).getAllByRole('gridcell')[0]!;
    const badge = cell.querySelector('.cover-tile__badge [data-system-icon]');
    expect(badge).not.toBeNull();
    expect(
      cell.querySelector('.cover-tile__system [data-system-icon]'),
    ).toBeNull();
  });

  it('ends a shelf of more than 60 with a "See all {n}" cell', () => {
    const games = ofSystem(0, 100);
    render(<Harness entries={games} initial={games[0]!.id} />);
    const strip = stripOf(0);
    // The cell is rendered once the strip reaches its end.
    act(() => {
      fireEvent.keyDown(grid(), { key: 'End' });
    });
    expect(
      within(strip).getByRole('gridcell', { name: 'See all 100' }),
    ).toBeInTheDocument();
    expect(active()).toHaveAccessibleName('See all 100');
  });

  it('has no "See all" cell on a shelf of 60 or fewer', () => {
    const games = ofSystem(0, 60);
    render(<Harness entries={games} initial={games[0]!.id} />);
    fireEvent.keyDown(grid(), { key: 'End' });
    expect(active()).toHaveAttribute('data-entry-id', String(games[59]!.id));
    expect(
      within(grid()).queryByRole('gridcell', { name: /^See all/ }),
    ).toBeNull();
  });

  it('Enter on "See all" scopes to the system and switches to Covers', () => {
    const games = ofSystem(2, 100);
    render(<Harness entries={games} initial={games[0]!.id} />);
    grid().focus();
    fireEvent.keyDown(grid(), { key: 'End' });
    fireEvent.keyDown(grid(), { key: 'Enter' });
    expect(useViewStore.getState().scope).toEqual({
      kind: 'system',
      system: 2,
    });
    expect(useViewStore.getState().prefs.view).toBe('covers');
  });

  it('the header "See all" does the same, and names its shelf', async () => {
    const user = userEvent.setup();
    render(<Harness entries={ofSystem(1, 10)} />);
    await user.click(
      screen.getByRole('button', {
        name: `See all 10 in ${big.systems[1]!.name}`,
      }),
    );
    expect(useViewStore.getState().scope).toEqual({
      kind: 'system',
      system: 1,
    });
    expect(useViewStore.getState().prefs.view).toBe('covers');
  });

  it('"See all" on an art shelf sets the art filter', async () => {
    const user = userEvent.setup();
    const games = big.entries
      .filter((e) => e.art.cover === 'missing')
      .slice(0, 5);
    render(<Harness entries={games} groupBy="art" />);
    await user.click(screen.getByRole('button', { name: /^See all 5 in/ }));
    expect(useFilterStore.getState().filters.art).toBe('missing-cover');
    expect(useViewStore.getState().scope).toEqual({ kind: 'all' });
    expect(useViewStore.getState().prefs.view).toBe('covers');
  });

  it('→ past the 60th game reaches "See all"; ← comes back', () => {
    const games = ofSystem(0, 100);
    render(<Harness entries={games} initial={games[59]!.id} />);
    fireEvent.keyDown(grid(), { key: 'ArrowRight' });
    expect(active()).toHaveAttribute('data-see-all', 'true');
    expect(active()).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(grid(), { key: 'ArrowLeft' });
    expect(active()).toHaveAttribute('data-entry-id', String(games[59]!.id));
  });

  it('moves along a shelf with ←/→ and Home', () => {
    const games = ofSystem(0, 10);
    render(<Harness entries={games} initial={games[3]!.id} />);
    fireEvent.keyDown(grid(), { key: 'ArrowRight' });
    expect(active()).toHaveAttribute('data-entry-id', String(games[4]!.id));
    fireEvent.keyDown(grid(), { key: 'ArrowLeft' });
    fireEvent.keyDown(grid(), { key: 'ArrowLeft' });
    expect(active()).toHaveAttribute('data-entry-id', String(games[2]!.id));
    fireEvent.keyDown(grid(), { key: 'Home' });
    expect(active()).toHaveAttribute('data-entry-id', String(games[0]!.id));
  });

  it('↓ lands on the game in the next shelf nearest on screen', () => {
    const a = ofSystem(0, 20);
    const b = ofSystem(1, 20);
    const cellW = COVER_SIZES.m.tileW + COVER_GAP;
    // Each cell's left edge is its translateX, minus its shelf's scroll:
    // the second shelf is scrolled two tiles further than the first.
    vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockImplementation(
      function (this: HTMLElement) {
        const x = Number(
          /translateX\((-?[\d.]+)px\)/.exec(this.style.transform)?.[1] ?? 0,
        );
        const shelf =
          this.closest('[data-shelf-index]')?.getAttribute('data-shelf-index');
        const left = x - (shelf === '1' ? 2 * cellW : 0);
        return {
          x: left,
          y: 0,
          top: 0,
          left,
          right: left + 100,
          bottom: 100,
          width: 100,
          height: 100,
          toJSON: () => ({}),
        } as DOMRect;
      },
    );
    render(<Harness entries={[...a, ...b]} initial={a[3]!.id} />);
    fireEvent.keyDown(grid(), { key: 'ArrowDown' });
    expect(active()).toHaveAttribute('data-entry-id', String(b[5]!.id));
    fireEvent.keyDown(grid(), { key: 'ArrowUp' });
    expect(active()).toHaveAttribute('data-entry-id', String(a[3]!.id));
    // ↑ on the first shelf stays put.
    fireEvent.keyDown(grid(), { key: 'ArrowUp' });
    expect(active()).toHaveAttribute('data-entry-id', String(a[3]!.id));
  });

  it('↓ onto a shelf that is not rendered keeps the position', () => {
    const a = ofSystem(0, 20);
    const b = ofSystem(1, 20);
    render(<Harness entries={[...a, ...b]} initial={a[3]!.id} />);
    // Every rect is at 0: the nearest is the first rendered cell.
    fireEvent.keyDown(grid(), { key: 'ArrowDown' });
    expect(active()).toHaveAttribute('data-entry-id', String(b[0]!.id));
  });

  it('renders under 10 shelves of under 20 tiles for 12,000 games', () => {
    render(<Harness entries={big.entries} initial={big.entries[0]!.id} />);
    const shelves = grid().querySelectorAll('[data-shelf-index]');
    expect(shelves.length).toBeGreaterThan(0);
    expect(shelves.length).toBeLessThan(10);
    for (const shelf of shelves) {
      const cells = within(shelf as HTMLElement).queryAllByRole('gridcell');
      expect(cells.length).toBeLessThan(20);
    }
  });

  it('keeps the active game rendered after scrolling away', () => {
    render(<Harness entries={big.entries} initial={big.entries[0]!.id} />);
    const id = grid().getAttribute('aria-activedescendant');
    expect(active()).not.toBeNull();
    Object.defineProperty(grid(), 'scrollTop', {
      configurable: true,
      value: 1_000_000,
    });
    fireEvent.scroll(grid());
    expect(grid().getAttribute('aria-activedescendant')).toBe(id);
    expect(active()).not.toBeNull();
  });

  it("keeps each shelf's own sideways scroll when it comes back", () => {
    render(<Harness entries={big.entries} />);
    const strip = stripOf(0);
    strip.scrollLeft = 540;
    fireEvent.scroll(strip);
    Object.defineProperty(grid(), 'scrollTop', {
      configurable: true,
      value: 1_000_000,
    });
    fireEvent.scroll(grid());
    expect(grid().querySelector('[data-shelf-index="0"]')).toBeNull();
    Object.defineProperty(grid(), 'scrollTop', {
      configurable: true,
      value: 0,
    });
    fireEvent.scroll(grid());
    expect(stripOf(0).scrollLeft).toBe(540);
  });

  it('Enter opens the game, Space previews it', () => {
    const onOpen = vi.fn();
    const onPreview = vi.fn();
    const games = ofSystem(0, 5);
    render(
      <Harness
        entries={games}
        initial={games[1]!.id}
        onOpen={onOpen}
        onPreview={onPreview}
      />,
    );
    fireEvent.keyDown(grid(), { key: ' ' });
    expect(onPreview).toHaveBeenCalledWith(games[1]!.id);
    fireEvent.keyDown(grid(), { key: 'Enter' });
    expect(onOpen).toHaveBeenCalledWith(games[1]!.id);
  });

  it('clicking a tile opens it', async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    const games = ofSystem(0, 5);
    render(<Harness entries={games} onOpen={onOpen} />);
    await user.click(document.getElementById(`entry-${games[2]!.id}`)!);
    expect(onOpen).toHaveBeenCalledWith(games[2]!.id);
  });

  it('Tab leaves the grid', async () => {
    const user = userEvent.setup();
    const games = ofSystem(0, 5);
    render(<Harness entries={games} initial={games[0]!.id} />);
    act(() => grid().focus());
    await user.tab();
    expect(grid()).not.toHaveFocus();
  });
});
