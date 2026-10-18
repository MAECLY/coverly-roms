import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import {
  FIXTURE_NOW,
  generateLibrary,
  HOSTILE_NAMES,
} from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import { MEDIA, REGIONS, type EntryView } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';
import { groupEntries } from '@/lib/grouping';
import { titleWithoutRegions } from '@/lib/tileLabel';
import { DEFAULT_PREFS, type GroupBy } from '@/lib/viewPrefs';
import { stubLayout } from '@/test/layout';

import { CoverGrid } from '../CoverGrid';
import { CoversView } from './CoversView';
import { QuickLook } from './QuickLook';

/** The performance budget: tiles mounted at once. */
const MAX_TILES = 120;

const big = generateLibrary({
  seed: 12,
  count: 12_000,
  root: '/mock/Big',
  generation: 1,
  now: FIXTURE_NOW,
});

function grouped(entries: readonly EntryView[], groupBy: GroupBy) {
  return groupEntries(entries, groupBy, {
    systems: big.systems,
    regionOrder: REGIONS,
    artMedia: MEDIA,
  });
}

function Harness({
  entries: input,
  groupBy = 'none',
}: {
  entries: readonly EntryView[];
  groupBy?: GroupBy;
}) {
  const [selected, setSelected] = useState<number | null>(null);
  const [opened, setOpened] = useState<number | null>(null);
  const [previewId, setPreviewId] = useState<number | null>(null);
  const { ordered: entries, sections } = grouped(input, groupBy);
  return (
    <I18nProvider locale="en">
      <button type="button">before</button>
      <CoversView
        entries={entries}
        sections={sections}
        prefs={{ ...DEFAULT_PREFS, groupBy }}
        onPreview={setPreviewId}
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
      <button type="button">after</button>
      <QuickLook
        entries={entries}
        systems={big.systems}
        generation={1}
        media="cover"
        previewId={previewId}
        onSelect={(id) => {
          setSelected(id);
          setPreviewId(id);
        }}
        onClose={() => setPreviewId(null)}
      />
    </I18nProvider>
  );
}

describe('CoversView', () => {
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

  it('shows the system icon beside the name', () => {
    render(<Harness entries={big.entries} />);
    const grid = screen.getByRole('grid');
    grid.focus();
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    const first = big.entries[0]!;
    const tile = document.getElementById(`entry-${first.id}`)!;
    expect(tile).toHaveAttribute('aria-selected', 'true');
    const system = big.systems[first.system]!;
    const label = tile.querySelector('.cover-tile__system')!;
    expect(label).toHaveTextContent(system.name);
    const icon = label.querySelector(
      `[data-system-icon="${system.system_id}"]`,
    );
    expect(icon).not.toBeNull();
    expect(icon).toHaveAttribute('aria-hidden', 'true');
  });

  it('keeps CoverGrid as an alias of CoversView', () => {
    expect(CoverGrid).toBe(CoversView);
  });

  it('renders section headers as row > rowheader[aria-colspan]', () => {
    render(<Harness entries={big.entries} groupBy="system" />);
    const grid = screen.getByRole('grid');
    const { sections } = grouped(big.entries, 'system');
    const first = sections[0]!;
    const header = screen.getAllByRole('rowheader')[0]!;
    expect(header).toHaveAttribute(
      'aria-colspan',
      grid.getAttribute('aria-colcount'),
    );
    expect(header).toHaveAccessibleName(
      `${first.label}, ${first.count.toLocaleString('en')} games`,
    );
    expect(header.parentElement).toHaveAttribute('role', 'row');
    expect(
      header.querySelector(`[data-system-icon="${first.systemId}"]`),
    ).not.toBeNull();
    // Header rows count in the totals.
    const rows = Number(grid.getAttribute('aria-rowcount'));
    expect(rows).toBe(
      sections.length +
        sections.reduce((n, s) => n + Math.ceil(s.count / 7), 0),
    );
  });

  it('mounts only the visible tiles of a grouped 12k library', () => {
    render(<Harness entries={big.entries} groupBy="system" />);
    expect(screen.getAllByRole('gridcell').length).toBeLessThanOrEqual(
      MAX_TILES,
    );
  });

  it('keeps the active tile rendered after scrolling away', () => {
    render(<Harness entries={big.entries} groupBy="system" />);
    const grid = screen.getByRole('grid');
    grid.focus();
    for (let i = 0; i < 6; i++) fireEvent.keyDown(grid, { key: 'ArrowRight' });
    const active = grid.getAttribute('aria-activedescendant')!;
    expect(document.getElementById(active)).not.toBeNull();
    Object.defineProperty(grid, 'scrollTop', {
      configurable: true,
      value: 1_000_000,
    });
    fireEvent.scroll(grid);
    expect(grid.getAttribute('aria-activedescendant')).toBe(active);
    expect(document.getElementById(active)).not.toBeNull();
    expect(screen.getAllByRole('gridcell').length).toBeLessThanOrEqual(
      MAX_TILES,
    );
  });

  it('ArrowDown at a partial last row of a section lands in the next section', () => {
    const a = big.entries.filter((e) => e.system === 0).slice(0, 10);
    const b = big.entries.filter((e) => e.system === 1).slice(0, 10);
    const { ordered } = grouped([...a, ...b], 'system');
    render(<Harness entries={[...a, ...b]} groupBy="system" />);
    const grid = screen.getByRole('grid');
    expect(grid).toHaveAttribute('aria-colcount', '7');
    grid.focus();
    // Index 8: second column of the section's partial last row.
    for (let i = 0; i < 9; i++) fireEvent.keyDown(grid, { key: 'ArrowRight' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${ordered[8]!.id}`,
    );
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${ordered[11]!.id}`,
    );
    // Up again: back to the same column of the partial row.
    fireEvent.keyDown(grid, { key: 'ArrowUp' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${ordered[8]!.id}`,
    );
    // Column 4 of the full row above clamps into the partial row.
    fireEvent.keyDown(grid, { key: 'Home' });
    for (let i = 0; i < 4; i++) fireEvent.keyDown(grid, { key: 'ArrowRight' });
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${ordered[9]!.id}`,
    );
  });
});

describe('CoversView keyboard model', () => {
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

  const titleOf = (e: EntryView) => titleWithoutRegions(e.name).toLowerCase();

  it('typing letters selects the first name that starts with them', async () => {
    const user = userEvent.setup();
    render(<Harness entries={big.entries} />);
    const grid = screen.getByRole('grid');
    grid.focus();
    // A two-letter prefix whose first match is well into the library.
    const prefix = titleOf(big.entries[500]!).slice(0, 2);
    await user.keyboard(prefix);
    const want = big.entries.find((e) => titleOf(e).startsWith(prefix))!;
    expect(big.entries.indexOf(want)).toBeGreaterThan(0);
    expect(grid).toHaveAttribute('aria-activedescendant', `entry-${want.id}`);
    expect(document.getElementById(`entry-${want.id}`)).not.toBeNull();
  });

  it('pressing the same letter again goes to the next match', async () => {
    const user = userEvent.setup();
    render(<Harness entries={big.entries} />);
    const grid = screen.getByRole('grid');
    grid.focus();
    await user.keyboard('tt');
    const matches = big.entries.filter((e) => titleOf(e).startsWith('t'));
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${matches[1]!.id}`,
    );
  });

  it('Space opens a quick look and Escape gives focus back', async () => {
    const user = userEvent.setup();
    render(<Harness entries={big.entries} />);
    const grid = screen.getByRole('grid');
    grid.focus();
    await user.keyboard('{ArrowRight}');
    await user.keyboard(' ');
    expect(
      screen.getByRole('dialog', { name: big.entries[0]!.name }),
    ).toBeInTheDocument();
    await user.keyboard('{ArrowRight}');
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${big.entries[1]!.id}`,
    );
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).toBeNull();
    await waitFor(() => expect(grid).toHaveFocus());
  });

  it('Alt+ArrowDown and Alt+ArrowUp jump between groups', () => {
    const { ordered, sections } = grouped(big.entries, 'system');
    render(<Harness entries={big.entries} groupBy="system" />);
    const grid = screen.getByRole('grid');
    grid.focus();
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    fireEvent.keyDown(grid, { key: 'ArrowDown', altKey: true });
    const second = ordered[sections[1]!.start]!;
    expect(grid).toHaveAttribute('aria-activedescendant', `entry-${second.id}`);
    expect(document.getElementById(`entry-${second.id}`)).not.toBeNull();
    fireEvent.keyDown(grid, { key: 'ArrowUp', altKey: true });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${ordered[0]!.id}`,
    );
  });

  it('Tab leaves the grid and Shift+Tab comes back', async () => {
    const user = userEvent.setup();
    render(<Harness entries={big.entries} />);
    const grid = screen.getByRole('grid');
    grid.focus();
    await user.keyboard('{ArrowRight}');
    await user.tab();
    expect(document.activeElement).not.toBe(grid);
    expect(screen.getByRole('button', { name: 'after' })).toHaveFocus();
    await user.tab({ shift: true });
    expect(grid).toHaveFocus();
  });

  it('Space mid-word is part of the typed name, not a quick look', async () => {
    const user = userEvent.setup();
    render(<Harness entries={big.entries} />);
    const grid = screen.getByRole('grid');
    grid.focus();
    const spaced = big.entries.find((e) => / ./.test(titleOf(e)))!;
    const title = titleOf(spaced);
    const prefix = title.slice(0, title.indexOf(' ') + 2);
    await user.keyboard(prefix);
    expect(screen.queryByRole('dialog')).toBeNull();
    const want = big.entries.find((e) => titleOf(e).startsWith(prefix))!;
    expect(grid).toHaveAttribute('aria-activedescendant', `entry-${want.id}`);
  });

  it('keeps the entry stepped to in Quick look scrolled into view', async () => {
    const user = userEvent.setup();
    const scrollTo = vi.fn();
    Object.defineProperty(HTMLElement.prototype, 'scrollTo', {
      configurable: true,
      value: scrollTo,
    });
    Object.defineProperty(HTMLElement.prototype, 'scrollHeight', {
      configurable: true,
      get: () => 1_000_000,
    });
    try {
      render(<Harness entries={big.entries} />);
      const grid = screen.getByRole('grid');
      grid.focus();
      await user.keyboard('{ArrowRight} ');
      scrollTo.mockClear();
      // Far past the first screen of tiles, with no key reaching the grid.
      for (let i = 0; i < 60; i++) await user.keyboard('{ArrowRight}');
      expect(grid).toHaveAttribute(
        'aria-activedescendant',
        `entry-${big.entries[60]!.id}`,
      );
      const top = (scrollTo.mock.lastCall?.[0] as { top: number }).top;
      expect(top).toBeGreaterThan(0);
    } finally {
      Reflect.deleteProperty(HTMLElement.prototype, 'scrollTo');
      Reflect.deleteProperty(HTMLElement.prototype, 'scrollHeight');
    }
  });
});

describe('CoversView row offsets', () => {
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

  it('re-lays rows out when the sections change but the row count does not', () => {
    const a = big.entries.filter((e) => e.system === 0);
    const b = big.entries.filter((e) => e.system === 1);
    const before = [...a.slice(0, 10), ...b.slice(0, 3)];
    const after = [...a.slice(0, 3), ...b.slice(0, 10)];
    const { rerender } = render(<Harness entries={before} groupBy="system" />);
    // H 7 3 H 3 → H 3 H 7 3: five rows both times, at new offsets.
    rerender(<Harness entries={after} groupBy="system" />);
    const { ordered } = grouped(after, 'system');
    const rowOf = (i: number) =>
      document.getElementById(`entry-${ordered[i]!.id}`)!.parentElement!;
    // Header 32 + a 3-tile row 308 + header 32.
    expect(rowOf(3).style.transform).toBe('translateY(372px)');
    expect(rowOf(10).style.transform).toBe('translateY(680px)');
  });
});
