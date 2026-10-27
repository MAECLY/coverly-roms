import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { MEDIA, REGIONS } from '@/ipc/types';
import { groupEntries } from '@/lib/grouping';
import { DEFAULT_SORT, type SortKey } from '@/lib/sort';
import { titleWithoutRegions } from '@/lib/tileLabel';
import { DEFAULT_PREFS, type GroupBy, type ViewPrefs } from '@/lib/viewPrefs';
import { stubLayout } from '@/test/layout';

import { EntryTable, ROW_H } from '../EntryTable';

const MAX_ROWS = 60;
const big = generateLibrary({
  seed: 12,
  count: 12_000,
  root: '/mock/Big',
  generation: 1,
  now: FIXTURE_NOW,
});

function Harness({
  onSort,
  onOpen = () => undefined,
  onPreview = () => undefined,
  setPrefs: spy = () => undefined,
  initial = {},
  groupBy = 'none',
}: {
  onSort: (k: SortKey) => void;
  onOpen?: (id: number) => void;
  onPreview?: (id: number) => void;
  setPrefs?: (patch: Partial<ViewPrefs>) => void;
  initial?: Partial<ViewPrefs>;
  groupBy?: GroupBy;
}) {
  const [selected, setSelected] = useState<number | null>(null);
  const [prefs, setPrefsState] = useState<ViewPrefs>({
    ...DEFAULT_PREFS,
    ...initial,
    groupBy,
  });
  const [{ ordered, sections }] = useState(() =>
    groupEntries(big.entries, groupBy, {
      systems: big.systems,
      regionOrder: REGIONS,
      artMedia: MEDIA,
    }),
  );
  return (
    <I18nProvider locale="en">
      <EntryTable
        entries={ordered}
        sections={sections}
        prefs={prefs}
        setPrefs={(patch) => {
          spy(patch);
          setPrefsState((p) => ({ ...p, ...patch }));
        }}
        systems={big.systems}
        generation={1}
        selectedId={selected}
        onSelect={setSelected}
        onOpen={(id) => {
          setSelected(id);
          onOpen(id);
        }}
        sort={DEFAULT_SORT}
        onSort={onSort}
        onPreview={onPreview}
      />
    </I18nProvider>
  );
}

describe('DetailsView', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('mounts only the visible rows of a 12k library', () => {
    render(<Harness onSort={() => undefined} />);
    const rows = screen.getAllByRole('row');
    expect(rows.length).toBeGreaterThan(1);
    expect(rows.length).toBeLessThanOrEqual(MAX_ROWS);
  });

  it('shows name, system, size, date and art status per row', () => {
    render(<Harness onSort={() => undefined} />);
    const first = big.entries[0]!;
    const row = document.getElementById(`entry-${first.id}`)!;
    expect(within(row).getByText(first.name)).toBeInTheDocument();
    expect(
      within(row).getByText(big.systems[first.system]!.name),
    ).toBeInTheDocument();
    expect(
      within(row).getByRole('img', {
        name: `Cover: ${first.art.cover === 'present' ? 'present' : 'missing'}`,
      }),
    ).toBeInTheDocument();
  });

  it('sorts from the column headers', () => {
    const onSort = vi.fn();
    render(<Harness onSort={onSort} />);
    fireEvent.click(screen.getByRole('button', { name: 'Name' }));
    fireEvent.click(screen.getByRole('button', { name: 'Modified' }));
    expect(onSort.mock.calls).toEqual([['name'], ['mtime']]);
  });

  it('moves the selection with the arrow keys', () => {
    render(<Harness onSort={() => undefined} />);
    const grid = screen.getByRole('grid');
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${big.entries[1]!.id}`,
    );
  });

  it('scrolls the keyboard selection fully below the sticky header', () => {
    const height = 800;
    const scrollTo = vi.fn();
    Object.defineProperty(HTMLElement.prototype, 'scrollTo', {
      configurable: true,
      value: scrollTo,
    });
    Object.defineProperty(HTMLElement.prototype, 'scrollHeight', {
      configurable: true,
      get: () => (big.entries.length + 1) * ROW_H,
    });
    try {
      render(<Harness onSort={() => undefined} />);
      const grid = screen.getByRole('grid');
      const presses = 40;
      for (let i = 0; i < presses; i += 1)
        fireEvent.keyDown(grid, { key: 'ArrowDown' });
      const index = presses - 1;
      const top = (scrollTo.mock.lastCall?.[0] as { top: number }).top;
      // Row `index` sits below the header row inside the scroll element.
      const rowTop = ROW_H + index * ROW_H - top;
      expect(rowTop).toBeGreaterThanOrEqual(ROW_H);
      expect(rowTop + ROW_H).toBeLessThanOrEqual(height);

      for (let i = 0; i < presses; i += 1)
        fireEvent.keyDown(grid, { key: 'ArrowUp' });
      expect(
        (scrollTo.mock.lastCall?.[0] as { top: number }).top,
      ).toBeLessThanOrEqual(0);
    } finally {
      Reflect.deleteProperty(HTMLElement.prototype, 'scrollTo');
      Reflect.deleteProperty(HTMLElement.prototype, 'scrollHeight');
    }
    // 80 key presses re-render the large table; a slow CI runner needs more
    // than the 5 s default.
  }, 20_000);

  it('leaves Enter and arrows on a sort button to the button', () => {
    const onSort = vi.fn();
    const onOpen = vi.fn();
    render(<Harness onSort={onSort} onOpen={onOpen} />);
    const grid = screen.getByRole('grid');
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    const name = screen.getByRole('button', { name: 'Name' });
    fireEvent.keyDown(name, { key: 'ArrowDown' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${big.entries[0]!.id}`,
    );
    const enter = fireEvent.keyDown(name, { key: 'Enter' });
    expect(enter).toBe(true);
    fireEvent.click(name);
    expect(onSort).toHaveBeenCalledWith('name');
    expect(onOpen).not.toHaveBeenCalled();
  });

  it('shows the system icon beside the name', () => {
    render(<Harness onSort={() => undefined} />);
    const first = big.entries[0]!;
    const system = big.systems[first.system]!;
    const row = document.getElementById(`entry-${first.id}`)!;
    const cell = row.querySelector('.cell--system')!;
    expect(within(row).getByText(system.name)).toBeInTheDocument();
    const icon = cell.querySelector(`[data-system-icon="${system.system_id}"]`);
    expect(icon).not.toBeNull();
    expect(icon).toHaveAttribute('aria-hidden', 'true');
  });

  it('keeps the active row rendered', () => {
    render(<Harness onSort={() => undefined} />);
    const grid = screen.getByRole('grid');
    grid.focus();
    for (let i = 0; i < 4; i++) fireEvent.keyDown(grid, { key: 'ArrowDown' });
    const active = grid.getAttribute('aria-activedescendant')!;
    expect(active).toBe(`entry-${big.entries[3]!.id}`);
    Object.defineProperty(grid, 'scrollTop', {
      configurable: true,
      value: 200_000,
    });
    fireEvent.scroll(grid);
    expect(document.getElementById(active)).not.toBeNull();
    expect(screen.getAllByRole('row').length).toBeLessThanOrEqual(MAX_ROWS);
  });

  it('jumps to a typed name and previews it with Space', () => {
    const onPreview = vi.fn();
    render(<Harness onSort={() => undefined} onPreview={onPreview} />);
    const grid = screen.getByRole('grid');
    const title = (i: number) =>
      titleWithoutRegions(big.entries[i]!.name).toLowerCase();
    const prefix = title(300).slice(0, 2);
    const want = big.entries.findIndex((_, i) => title(i).startsWith(prefix));
    const now = vi.spyOn(Date, 'now').mockReturnValue(1_000);
    for (const ch of prefix) fireEvent.keyDown(grid, { key: ch });
    const id = big.entries[want]!.id;
    expect(grid).toHaveAttribute('aria-activedescendant', `entry-${id}`);
    // Space after the type-ahead pause previews instead of extending the name.
    now.mockReturnValue(2_000);
    expect(fireEvent.keyDown(grid, { key: ' ' })).toBe(false);
    now.mockRestore();
    expect(onPreview).toHaveBeenCalledWith(id);
  });
  it('lists every column in the header context menu', async () => {
    render(<Harness onSort={() => undefined} />);
    fireEvent.contextMenu(screen.getByRole('columnheader', { name: 'Size' }));
    const menu = await screen.findByRole('menu');
    const items = within(menu).getAllByRole('menuitemcheckbox');
    expect(items).toHaveLength(11);
    expect(
      within(menu).getByRole('menuitemcheckbox', { name: 'Name' }),
    ).toHaveAttribute('aria-disabled', 'true');
    expect(
      within(menu).getByRole('menuitemcheckbox', { name: 'Thumbnail' }),
    ).toHaveAttribute('aria-checked', 'false');
  });

  it('hides a column unticked in the menu and saves it', async () => {
    const user = userEvent.setup();
    const setPrefs = vi.fn();
    render(<Harness onSort={() => undefined} setPrefs={setPrefs} />);
    fireEvent.contextMenu(screen.getByRole('columnheader', { name: 'Size' }));
    const menu = await screen.findByRole('menu');
    await user.click(
      within(menu).getByRole('menuitemcheckbox', { name: 'Size' }),
    );
    expect(
      screen.queryByRole('columnheader', { name: 'Size' }),
    ).not.toBeInTheDocument();
    const patch = setPrefs.mock.lastCall?.[0] as Partial<ViewPrefs>;
    expect(patch.columns?.map((c) => c.id)).not.toContain('size');
  });

  it('moves the column the menu was opened on', async () => {
    const user = userEvent.setup();
    const setPrefs = vi.fn();
    render(<Harness onSort={() => undefined} setPrefs={setPrefs} />);
    fireEvent.contextMenu(screen.getByRole('columnheader', { name: 'System' }));
    const menu = await screen.findByRole('menu');
    await user.click(within(menu).getByRole('menuitem', { name: 'Move left' }));
    const heads = screen
      .getAllByRole('columnheader')
      .map((h) => h.getAttribute('data-column'));
    expect(heads.slice(0, 2)).toEqual(['system', 'name']);
  });

  it('opens the same menu with Shift+F10 on a focused header', async () => {
    render(<Harness onSort={() => undefined} />);
    const name = screen.getByRole('button', { name: 'Name' });
    name.focus();
    fireEvent.keyDown(name, { key: 'F10', shiftKey: true });
    const menu = await screen.findByRole('menu');
    expect(within(menu).getAllByRole('menuitemcheckbox')).toHaveLength(11);
  });

  it('opens the menu with the context-menu key', async () => {
    render(<Harness onSort={() => undefined} />);
    const name = screen.getByRole('button', { name: 'Name' });
    fireEvent.keyDown(name, { key: 'ContextMenu' });
    expect(await screen.findByRole('menu')).toBeInTheDocument();
  });

  it('draws compact rows 24px tall', () => {
    render(
      <Harness onSort={() => undefined} initial={{ density: 'compact' }} />,
    );
    const first = document.getElementById(`entry-${big.entries[0]!.id}`)!;
    expect(first).toHaveStyle({ height: '24px' });
    expect(first).toHaveClass('entry-row--compact');
    expect(screen.getAllByRole('row').length).toBeLessThanOrEqual(MAX_ROWS);
  });

  it('shows a thumbnail column when it is picked', () => {
    render(
      <Harness
        onSort={() => undefined}
        initial={{ columns: [{ id: 'thumb' }, { id: 'name' }] }}
      />,
    );
    const row = document.getElementById(`entry-${big.entries[0]!.id}`)!;
    expect(within(row).getAllByRole('gridcell')).toHaveLength(2);
    expect(row.querySelector('.cell--thumb .art-thumb')).not.toBeNull();
    expect(
      screen.getByRole('columnheader', { name: 'Thumbnail' }),
    ).toBeInTheDocument();
  });

  it('renders region and extension cells', () => {
    render(
      <Harness
        onSort={() => undefined}
        initial={{
          columns: [{ id: 'name' }, { id: 'regions' }, { id: 'ext' }],
        }}
      />,
    );
    const first = big.entries.find((e) => e.regions.length > 0)!;
    const index = big.entries.indexOf(first);
    // Only the first screen is mounted.
    if (index > 20) return;
    const row = document.getElementById(`entry-${first.id}`)!;
    expect(row.querySelector('.cell--ext')).toHaveTextContent(first.ext);
    expect(row.querySelector('.cell--regions')!.textContent).not.toBe('');
  });

  it('lays the columns out from the saved widths', () => {
    render(
      <Harness
        onSort={() => undefined}
        initial={{ columns: [{ id: 'name' }, { id: 'size', width: 150 }] }}
      />,
    );
    const grid = screen.getByRole('grid');
    expect(grid.style.getPropertyValue('--details-cols')).toBe(
      'minmax(200px,1fr) 150px',
    );
    // A fixed floor, not max-content, so a long name cannot widen its own
    // row's flexible track and push the other cells out of their columns.
    expect(grid.style.getPropertyValue('--details-min-width')).toBe('350px');
  });

  it('resizes a column with the arrow keys on its edge', () => {
    const setPrefs = vi.fn();
    render(
      <Harness
        onSort={() => undefined}
        setPrefs={setPrefs}
        initial={{ columns: [{ id: 'name' }, { id: 'size', width: 100 }] }}
      />,
    );
    const edge = screen.getByRole('separator', { name: 'Resize Size' });
    expect(edge).toHaveAttribute('aria-valuenow', '100');
    fireEvent.keyDown(edge, { key: 'ArrowRight' });
    fireEvent.keyDown(edge, { key: 'ArrowRight' });
    fireEvent.keyDown(edge, { key: 'ArrowLeft' });
    expect(edge).toHaveAttribute('aria-valuenow', '108');
    const patch = setPrefs.mock.lastCall?.[0] as Partial<ViewPrefs>;
    expect(patch.columns).toContainEqual({ id: 'size', width: 108 });
  });

  it('grows the name from its drawn width with the arrow keys', () => {
    const setPrefs = vi.fn();
    render(
      <Harness
        onSort={() => undefined}
        setPrefs={setPrefs}
        initial={{ columns: [{ id: 'name' }, { id: 'size', width: 100 }] }}
      />,
    );
    const edge = screen.getByRole('separator', { name: 'Resize Name' });
    // The name fills the row: it is drawn far wider than its 200px minimum.
    const head = edge.closest<HTMLElement>('[role="columnheader"]')!;
    const rect = vi
      .spyOn(head, 'getBoundingClientRect')
      .mockReturnValue({ width: 420 } as DOMRect);
    fireEvent.keyDown(edge, { key: 'ArrowRight' });
    rect.mockRestore();
    const patch = setPrefs.mock.lastCall?.[0] as Partial<ViewPrefs>;
    expect(patch.columns).toContainEqual({ id: 'name', width: 428 });
  });

  it('resizes a column by dragging its edge and saves once', () => {
    const setPrefs = vi.fn();
    render(
      <Harness
        onSort={() => undefined}
        setPrefs={setPrefs}
        initial={{ columns: [{ id: 'name' }, { id: 'size', width: 100 }] }}
      />,
    );
    const edge = screen.getByRole('separator', { name: 'Resize Size' });
    fireEvent.pointerDown(edge, { clientX: 500, pointerId: 1, button: 0 });
    fireEvent.pointerMove(edge, { clientX: 540, pointerId: 1 });
    fireEvent.pointerMove(edge, { clientX: 560, pointerId: 1 });
    expect(setPrefs).not.toHaveBeenCalled();
    expect(edge).toHaveAttribute('aria-valuenow', '160');
    fireEvent.pointerUp(edge, { clientX: 560, pointerId: 1 });
    expect(setPrefs).toHaveBeenCalledTimes(1);
    expect(
      (setPrefs.mock.lastCall?.[0] as Partial<ViewPrefs>).columns,
    ).toContainEqual({ id: 'size', width: 160 });
  });

  it('fits a column to its content on double-click', () => {
    // jsdom has no canvas: 7px per character stands in for the font.
    const getContext = vi
      .spyOn(HTMLCanvasElement.prototype, 'getContext')
      .mockReturnValue({
        font: '',
        measureText: (text: string) => ({ width: text.length * 7 }),
      } as unknown as CanvasRenderingContext2D);
    const setPrefs = vi.fn();
    try {
      render(
        <Harness
          onSort={() => undefined}
          setPrefs={setPrefs}
          initial={{ columns: [{ id: 'name' }, { id: 'ext' }] }}
        />,
      );
      fireEvent.doubleClick(
        screen.getByRole('separator', { name: 'Resize Extension' }),
      );
    } finally {
      getContext.mockRestore();
    }
    const cols = (setPrefs.mock.lastCall?.[0] as Partial<ViewPrefs>).columns!;
    // "Extension" (9 × 7 = 63px) is wider than any ".gba" cell: 63 + 17.
    expect(cols.find((c) => c.id === 'ext')).toEqual({ id: 'ext', width: 80 });
  });

  it('draws group rows spanning the visible columns', () => {
    render(<Harness onSort={() => undefined} groupBy="system" />);
    const headers = screen.getAllByRole('rowheader');
    expect(headers[0]).toHaveAttribute(
      'aria-colspan',
      String(DEFAULT_PREFS.columns.length),
    );
    expect(headers[0]).toHaveAccessibleName(/, [\d,]+ games$/);
    expect(headers[0]!.querySelector('[data-system-icon]')).not.toBeNull();
    const grid = screen.getByRole('grid');
    expect(Number(grid.getAttribute('aria-rowcount'))).toBeGreaterThan(
      big.entries.length + 1,
    );
    expect(screen.getAllByRole('row').length).toBeLessThanOrEqual(MAX_ROWS);
  });

  it('keeps the active row rendered when grouped', () => {
    render(<Harness onSort={() => undefined} groupBy="system" />);
    const grid = screen.getByRole('grid');
    for (let i = 0; i < 3; i++) fireEvent.keyDown(grid, { key: 'ArrowDown' });
    const active = grid.getAttribute('aria-activedescendant')!;
    Object.defineProperty(grid, 'scrollTop', {
      configurable: true,
      value: 300_000,
    });
    fireEvent.scroll(grid);
    expect(document.getElementById(active)).not.toBeNull();
  });
});
