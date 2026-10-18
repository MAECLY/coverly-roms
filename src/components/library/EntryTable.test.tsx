import { fireEvent, render, screen, within } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { DEFAULT_SORT, type SortKey } from '@/lib/sort';
import { stubLayout } from '@/test/layout';

import { EntryTable, ROW_H } from './EntryTable';

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
}: {
  onSort: (k: SortKey) => void;
  onOpen?: (id: number) => void;
}) {
  const [selected, setSelected] = useState<number | null>(null);
  return (
    <I18nProvider locale="en">
      <EntryTable
        entries={big.entries}
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
      />
    </I18nProvider>
  );
}

describe('EntryTable', () => {
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
  });

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
});
