import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import {
  FIXTURE_NOW,
  generateLibrary,
  HOSTILE_NAMES,
} from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import type { EntryView } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';
import { stubLayout } from '@/test/layout';

import { CoverGrid } from './CoverGrid';

/** The performance budget: tiles mounted at once. */
const MAX_TILES = 120;

const big = generateLibrary({
  seed: 12,
  count: 12_000,
  root: '/mock/Big',
  generation: 1,
  now: FIXTURE_NOW,
});

function Harness({ entries }: { entries: readonly EntryView[] }) {
  const [selected, setSelected] = useState<number | null>(null);
  const [opened, setOpened] = useState<number | null>(null);
  return (
    <I18nProvider locale="en">
      <CoverGrid
        entries={entries}
        systems={big.systems}
        generation={1}
        selectedId={selected}
        onSelect={setSelected}
        onOpen={(id) => {
          setSelected(id);
          setOpened(id);
        }}
      />
      <output data-testid="opened">{opened ?? ''}</output>
    </I18nProvider>
  );
}

describe('CoverGrid', () => {
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

  it('mounts only the visible tiles of a 12k library', () => {
    render(<Harness entries={big.entries} />);
    const cells = screen.getAllByRole('gridcell');
    expect(cells.length).toBeGreaterThan(0);
    expect(cells.length).toBeLessThanOrEqual(MAX_TILES);
  });

  it('moves the selection with the keyboard and opens with Enter', () => {
    render(<Harness entries={big.entries} />);
    const grid = screen.getByRole('grid');
    const columns = Number(grid.getAttribute('aria-colcount'));
    expect(columns).toBe(7);
    grid.focus();
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${big.entries[0]!.id}`,
    );
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${big.entries[1 + columns]!.id}`,
    );
    fireEvent.keyDown(grid, { key: 'End' });
    const last = big.entries.at(-1)!.id;
    expect(grid).toHaveAttribute('aria-activedescendant', `entry-${last}`);
    expect(screen.getAllByRole('gridcell').length).toBeLessThanOrEqual(
      MAX_TILES,
    );
    fireEvent.keyDown(grid, { key: 'Enter' });
    expect(screen.getByTestId('opened')).toHaveTextContent(String(last));
  });

  it('ignores keys that bubble up from inside the grid', () => {
    render(<Harness entries={big.entries} />);
    const grid = screen.getByRole('grid');
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    const tile = screen.getAllByRole('gridcell')[3]!;
    fireEvent.keyDown(tile, { key: 'ArrowRight' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${big.entries[0]!.id}`,
    );
    expect(fireEvent.keyDown(tile, { key: 'Enter' })).toBe(true);
    expect(screen.getByTestId('opened')).toHaveTextContent('');
  });

  it('opens a tile on click', () => {
    render(<Harness entries={big.entries} />);
    fireEvent.click(screen.getAllByRole('gridcell')[2]!);
    expect(screen.getByTestId('opened')).toHaveTextContent(
      String(big.entries[2]!.id),
    );
  });

  it('loads cover art lazily for present covers only', async () => {
    // Open the same fixture in the mock so generation 1 serves art.
    const backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    setBackend(backend);
    expect((await backend.openLibrary(MOCK_LIBRARIES[0].path)).generation).toBe(
      1,
    );
    const entries = big.entries.slice(0, 14);
    const { container } = render(<Harness entries={entries} />);
    const present = entries.filter((e) => e.art.cover === 'present').length;
    await waitFor(() =>
      expect(container.querySelectorAll('img')).toHaveLength(present),
    );
  });

  it('shows the title, system and region tags under each cover', () => {
    const entry = big.entries.find((e) => e.regions.join() === 'usa,europe')!;
    const { container } = render(<Harness entries={[entry]} />);
    const tile = container.querySelector('.cover-tile')!;
    expect(tile).toHaveAttribute('title', entry.name);
    expect(tile.querySelector('.cover-tile__name')).toHaveTextContent(
      entry.name.replace(' (USA, Europe)', ''),
    );
    expect(tile.querySelector('.cover-tile__system')).toHaveTextContent(
      big.systems[entry.system]!.name,
    );
    const tags = [...tile.querySelectorAll('.cover-tile__region')];
    expect(tags.map((el) => el.textContent)).toEqual(['US', 'EU']);
    expect(tags.map((el) => el.getAttribute('title'))).toEqual([
      'USA',
      'Europe',
    ]);
  });

  it('shows no region tag when the name has none', () => {
    const entry = big.entries.find((e) => e.regions.length === 0)!;
    const { container } = render(<Harness entries={[entry]} />);
    expect(container.querySelector('.cover-tile__region')).toBeNull();
    expect(container.querySelector('.cover-tile__name')).toHaveTextContent(
      entry.name,
    );
  });

  it('keeps every tile inside its row', () => {
    const { container } = render(<Harness entries={big.entries} />);
    const row = container.querySelector<HTMLElement>('.cover-row')!;
    const tile = container.querySelector<HTMLElement>('.cover-tile')!;
    expect(parseFloat(row.style.height)).toBe(
      parseFloat(tile.style.height) + 12,
    );
  });

  it('renders_hostile_names_as_text', () => {
    const small = generateLibrary({
      seed: 7,
      count: 240,
      root: '/m',
      generation: 1,
      now: FIXTURE_NOW,
    });
    const hostile = small.entries.filter((e) =>
      (HOSTILE_NAMES as readonly string[]).includes(e.name),
    );
    const { container } = render(<Harness entries={hostile} />);
    expect(
      screen.getByText('<img src=x onerror=alert(1)>'),
    ).toBeInTheDocument();
    expect(container.querySelector('img[src="x"]')).toBeNull();
    expect(container.querySelector('[onerror]')).toBeNull();
  });
});
