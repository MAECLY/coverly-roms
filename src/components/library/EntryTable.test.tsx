import { fireEvent, render, screen, within } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { DEFAULT_SORT, type SortKey } from '@/lib/sort';
import { stubLayout } from '@/test/layout';

import { EntryTable } from './EntryTable';

const MAX_ROWS = 60;
const big = generateLibrary({
  seed: 12,
  count: 12_000,
  root: '/mock/Big',
  generation: 1,
  now: FIXTURE_NOW,
});

function Harness({ onSort }: { onSort: (k: SortKey) => void }) {
  const [selected, setSelected] = useState<number | null>(null);
  return (
    <I18nProvider locale="en">
      <EntryTable
        entries={big.entries}
        systems={big.systems}
        generation={1}
        selectedId={selected}
        onSelect={setSelected}
        onOpen={setSelected}
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
});
