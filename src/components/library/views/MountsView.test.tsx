import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { MEDIA, REGIONS, type EntryView, type Media } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';
import { groupEntries } from '@/lib/grouping';
import { titleWithoutRegions } from '@/lib/tileLabel';
import { DEFAULT_PREFS, type GroupBy } from '@/lib/viewPrefs';
import { stubLayout } from '@/test/layout';

import { MountsView, MOUNT_ROW_H } from './MountsView';

const big = generateLibrary({
  seed: 12,
  count: 12_000,
  root: '/mock/Big',
  generation: 1,
  now: FIXTURE_NOW,
});

function Harness({
  entries: input,
  groupBy = 'none',
  onOpen = () => {},
  onOpenArt = () => {},
}: {
  entries: readonly EntryView[];
  groupBy?: GroupBy;
  onOpen?: (id: number) => void;
  onOpenArt?: (id: number, media: Media) => void;
}) {
  const [selected, setSelected] = useState<number | null>(null);
  const { ordered: entries, sections } = groupEntries(input, groupBy, {
    systems: big.systems,
    regionOrder: REGIONS,
    artMedia: MEDIA,
  });
  return (
    <I18nProvider locale="en">
      <button type="button">before</button>
      <MountsView
        entries={entries}
        sections={sections}
        prefs={{ ...DEFAULT_PREFS, view: 'mounts', groupBy }}
        onPreview={() => {}}
        systems={big.systems}
        generation={1}
        selectedId={selected}
        onSelect={setSelected}
        onOpen={(id) => {
          setSelected(id);
          onOpen(id);
        }}
        onOpenArt={(id, media) => {
          setSelected(id);
          onOpenArt(id, media);
        }}
      />
      <button type="button">after</button>
    </I18nProvider>
  );
}

describe('MountsView', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
  });
  afterEach(() => {
    restore();
    clearArtCache();
    setBackend(null);
  });

  it('is a four-column grid with fixed 112px rows', () => {
    render(<Harness entries={big.entries.slice(0, 20)} />);
    const grid = screen.getByRole('grid', { name: 'Art slots' });
    expect(grid).toHaveAttribute('aria-colcount', '4');
    const row = screen.getAllByRole('row')[0]!;
    expect(row.style.height).toBe(`${MOUNT_ROW_H}px`);
    expect(MOUNT_ROW_H).toBe(112);
  });

  it('renders under 30 rows of a 12k library at 800px', () => {
    render(<Harness entries={big.entries} />);
    const rows = screen.getAllByRole('row');
    expect(rows.length).toBeGreaterThan(5);
    expect(rows.length).toBeLessThan(30);
  });

  it('puts the name cell first, then cover, title and screenshot', () => {
    const entry = big.entries[0]!;
    render(<Harness entries={[entry]} />);
    const cells = within(screen.getAllByRole('row')[0]!).getAllByRole(
      'gridcell',
    );
    expect(cells).toHaveLength(4);
    expect(cells[0]).toHaveTextContent(titleWithoutRegions(entry.name));
    expect(cells.slice(1).map((c) => c.getAttribute('data-media'))).toEqual([
      'cover',
      'title',
      'screenshot',
    ]);
  });

  it('draws a missing slot dashed and names it "No Cover"', () => {
    const entry = big.entries.find((e) => e.art.cover === 'missing')!;
    render(<Harness entries={[entry]} />);
    const cell = screen.getByRole('gridcell', { name: 'No Cover' });
    expect(cell).toHaveAttribute('data-slot-state', 'missing');
  });

  it('names a slot with art "<media>: has art"', () => {
    const entry = big.entries.find((e) => e.art.cover === 'present')!;
    render(<Harness entries={[entry]} />);
    const cell = screen.getByRole('gridcell', { name: 'Cover: has art' });
    expect(cell).toHaveAttribute('data-slot-state', 'filled');
  });

  it('→ → then Enter opens the art workspace on the title slot', () => {
    const onOpenArt = vi.fn();
    const onOpen = vi.fn();
    render(
      <Harness
        entries={big.entries.slice(0, 20)}
        onOpen={onOpen}
        onOpenArt={onOpenArt}
      />,
    );
    const grid = screen.getByRole('grid');
    grid.focus();
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    const first = big.entries[0]!;
    expect(grid).toHaveAttribute('aria-activedescendant', `entry-${first.id}`);
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${first.id}-c2`,
    );
    fireEvent.keyDown(grid, { key: 'Enter' });
    expect(onOpenArt).toHaveBeenCalledWith(first.id, 'title');
    expect(onOpen).not.toHaveBeenCalled();
  });

  it('Enter on the name cell opens the entry', () => {
    const onOpen = vi.fn();
    const onOpenArt = vi.fn();
    render(
      <Harness
        entries={big.entries.slice(0, 20)}
        onOpen={onOpen}
        onOpenArt={onOpenArt}
      />,
    );
    const grid = screen.getByRole('grid');
    grid.focus();
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'Enter' });
    expect(onOpen).toHaveBeenCalledWith(big.entries[0]!.id);
    expect(onOpenArt).not.toHaveBeenCalled();
  });

  it('↓ keeps the column; ← at the name cell stays; Home/End go to the ends', () => {
    const entries = big.entries.slice(0, 20);
    render(<Harness entries={entries} />);
    const grid = screen.getByRole('grid');
    grid.focus();
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${entries[1]!.id}-c3`,
    );
    fireEvent.keyDown(grid, { key: 'End' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${entries[19]!.id}-c3`,
    );
    fireEvent.keyDown(grid, { key: 'Home' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${entries[0]!.id}-c3`,
    );
    for (let i = 0; i < 4; i++) fireEvent.keyDown(grid, { key: 'ArrowLeft' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${entries[0]!.id}`,
    );
  });

  it('a modified ← or → never moves to another row', () => {
    const entries = big.entries.slice(0, 20);
    render(<Harness entries={entries} />);
    const grid = screen.getByRole('grid');
    grid.focus();
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    for (const mod of ['metaKey', 'ctrlKey', 'altKey'] as const) {
      fireEvent.keyDown(grid, { key: 'ArrowRight', [mod]: true });
      fireEvent.keyDown(grid, { key: 'ArrowLeft', [mod]: true });
    }
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${entries[0]!.id}`,
    );
  });

  it('clicking a slot opens the art workspace on it', async () => {
    const user = userEvent.setup();
    const onOpenArt = vi.fn();
    const entry = big.entries[0]!;
    render(<Harness entries={[entry]} onOpenArt={onOpenArt} />);
    const cells = within(screen.getAllByRole('row')[0]!).getAllByRole(
      'gridcell',
    );
    await user.click(cells[3]!);
    expect(onOpenArt).toHaveBeenCalledWith(entry.id, 'screenshot');
  });

  it('renders group headers spanning all four cells', () => {
    render(<Harness entries={big.entries.slice(0, 200)} groupBy="system" />);
    const header = screen.getAllByRole('rowheader')[0]!;
    expect(header).toHaveAttribute('aria-colspan', '4');
  });

  it('keeps the active cell rendered after scrolling far away', () => {
    render(<Harness entries={big.entries} groupBy="system" />);
    const grid = screen.getByRole('grid');
    grid.focus();
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    const active = grid.getAttribute('aria-activedescendant')!;
    expect(document.getElementById(active)).not.toBeNull();
    Object.defineProperty(grid, 'scrollTop', {
      configurable: true,
      value: 1_000_000,
    });
    fireEvent.scroll(grid);
    expect(grid.getAttribute('aria-activedescendant')).toBe(active);
    expect(document.getElementById(active)).not.toBeNull();
  });

  it('Tab leaves the grid', async () => {
    const user = userEvent.setup();
    render(<Harness entries={big.entries.slice(0, 50)} />);
    const grid = screen.getByRole('grid');
    grid.focus();
    await user.keyboard('{ArrowDown}{ArrowRight}');
    await user.tab();
    expect(screen.getByRole('button', { name: 'after' })).toHaveFocus();
  });
});
