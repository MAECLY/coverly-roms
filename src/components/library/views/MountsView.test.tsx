import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { MOCK_ADDONS } from '@/ipc/mock/addonViewsFixture';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import {
  MEDIA,
  REGIONS,
  type EntryView,
  type LibraryView,
  type Media,
} from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';
import { buildGames } from '@/lib/games';
import { groupEntries } from '@/lib/grouping';
import { titleWithoutRegions } from '@/lib/tileLabel';
import {
  DEFAULT_PREFS,
  readPrefsFile,
  sanitisePrefs,
  type GroupBy,
} from '@/lib/viewPrefs';
import { useViewStore } from '@/stores/viewStore';
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
    const grid = screen.getByRole('grid', { name: 'Art slots view' });
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

  it('draws a missing slot dashed and names it as its placeholder reads', () => {
    const entry = big.entries.find((e) => e.art.cover === 'missing')!;
    render(<Harness entries={[entry]} />);
    const cell = screen.getByRole('gridcell', { name: 'No cover' });
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

describe('MountsView: art slots of updates and DLC (addon views §8.6)', () => {
  let lib: LibraryView;
  let restore: () => void;
  beforeEach(async () => {
    // Tall enough for every row of the add-ons library to render.
    restore = stubLayout(1280, 4000);
    const backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    lib = await backend.openLibrary(MOCK_ADDONS);
    setBackend(backend);
    let n = 0;
    vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:${++n}`);
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
    useViewStore.getState().setPrefs({ childSlots: true });
  });
  afterEach(() => {
    vi.restoreAllMocks();
    restore();
    clearArtCache();
    setBackend(null);
  });

  /** Lantern Harbor's v1.10 carries an icon the user found. */
  const withUpdateArt = (e: EntryView): EntryView =>
    e.name === 'Lantern Harbor (USA)'
      ? {
          ...e,
          children: e.children!.map((c) =>
            c.version === '1.10'
              ? {
                  ...c,
                  art: { icon: true, banner: false, icon_origin: 'user' },
                }
              : c,
          ),
        }
      : e;

  function AddonHarness({
    edit = (e) => e,
    onOpenChild = () => {},
  }: {
    edit?: (e: EntryView) => EntryView;
    onOpenChild?: (id: number, index: number) => void;
  }) {
    const { games, byEntry } = buildGames(lib.entries.map(edit), {});
    const entries = games.map((g) => g.entry);
    const prefs = useViewStore((s) => s.prefs);
    const [selected, setSelected] = useState<number | null>(null);
    return (
      <I18nProvider locale="es">
        <MountsView
          entries={entries}
          sections={[]}
          prefs={{ ...prefs, view: 'mounts', groupBy: 'none' }}
          onPreview={() => {}}
          systems={lib.systems}
          generation={lib.generation}
          selectedId={selected}
          onSelect={setSelected}
          onOpen={setSelected}
          content={{
            byEntry,
            line: true,
            fresh: new Set(),
            onOpenChild: (id, index) => {
              setSelected(id);
              onOpenChild(id, index);
            },
          }}
        />
      </I18nProvider>
    );
  }

  const childRows = () =>
    screen
      .getAllByRole('row')
      .filter((r) => r.hasAttribute('data-child-index'));

  it('adds a row for each DLC and each update with art of its own', () => {
    render(<AddonHarness edit={withUpdateArt} />);
    expect(childRows().map((r) => r.getAttribute('aria-label'))).toEqual([
      'Actualización v1.10, icono tuyo, banner falta',
      'DLC Lighthouse Keeper Pack, icono incluido en el paquete, banner falta',
      'DLC Encore Pack, usa el del juego base',
      'DLC Night Drive Pack, usa el del juego base',
      'DLC Synthwave Pack, usa el del juego base',
      'DLC Reef Pack, usa el del juego base',
    ]);
    // v1.05 has no art of its own: no row.
    expect(screen.queryByText('Actualización v1.05')).toBeNull();
  });

  it('tags a filled slot with its origin and dashes a missing one «falta»', () => {
    render(<AddonHarness />);
    const [dlc] = childRows();
    const icon = within(dlc!).getByRole('gridcell', {
      name: 'Icono: incluido en el paquete',
    });
    expect(icon).toHaveAttribute('data-slot-state', 'filled');
    expect(icon).toHaveTextContent('paquete');
    const banner = within(dlc!).getByRole('gridcell', {
      name: 'Banner: falta',
    });
    expect(banner).toHaveAttribute('data-slot-state', 'missing');
    expect(banner.querySelector('.art-placeholder')).toHaveTextContent('falta');
  });

  it('dims the base art of a DLC without art of its own', () => {
    render(<AddonHarness />);
    const row = childRows().find((r) =>
      r.getAttribute('aria-label')!.startsWith('DLC Reef Pack'),
    )!;
    const cells = within(row).getAllByRole('gridcell');
    expect(cells[1]).toHaveAttribute('data-slot-state', 'inherit');
    expect(cells[1]).toHaveAccessibleName('Icono: usa el del juego base');
    expect(row).toHaveTextContent('usa el del juego base');
  });

  it('counts its rows on the toggle, which hides them and is saved', async () => {
    const user = userEvent.setup();
    render(<AddonHarness edit={withUpdateArt} />);
    const toggle = screen.getByRole('button', {
      name: 'Huecos del contenido (6)',
    });
    expect(toggle).toHaveAttribute('aria-pressed', 'true');
    expect(childRows()).toHaveLength(6);
    await user.click(toggle);
    expect(toggle).toHaveAttribute('aria-pressed', 'false');
    expect(childRows()).toHaveLength(0);
    expect(useViewStore.getState().prefs.childSlots).toBe(false);
    const saved = readPrefsFile(localStorage);
    expect(sanitisePrefs(saved.last).childSlots).toBe(false);
  });

  it('has no toggle in a library without add-ons', () => {
    render(<Harness entries={big.entries.slice(0, 20)} />);
    expect(screen.queryByRole('button', { name: /Content slots/ })).toBeNull();
  });

  it('↓ walks into the child rows and Enter opens the panel at the child', () => {
    const onOpenChild = vi.fn();
    render(<AddonHarness onOpenChild={onOpenChild} />);
    const lantern = lib.entries.find((e) => e.name === 'Lantern Harbor (USA)')!;
    const dlc = lantern.children!.find((c) => c.role === 'dlc')!;
    const grid = screen.getByRole('grid');
    grid.focus();
    fireEvent.keyDown(grid, { key: 'Home' });
    while (grid.getAttribute('aria-activedescendant') !== `entry-${lantern.id}`)
      fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${lantern.id}-k${dlc.index}-c1`,
    );
    fireEvent.keyDown(grid, { key: 'Enter' });
    expect(onOpenChild).toHaveBeenCalledWith(lantern.id, dlc.index);
    // ↑ goes back to the game's own row, same column.
    fireEvent.keyDown(grid, { key: 'ArrowUp' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${lantern.id}-c1`,
    );
  });

  it('a modifier or Tab alone keeps the keyboard on the child row', () => {
    render(<AddonHarness />);
    const lantern = lib.entries.find((e) => e.name === 'Lantern Harbor (USA)')!;
    const dlc = lantern.children!.find((c) => c.role === 'dlc')!;
    const grid = screen.getByRole('grid');
    grid.focus();
    fireEvent.keyDown(grid, { key: 'Home' });
    while (grid.getAttribute('aria-activedescendant') !== `entry-${lantern.id}`)
      fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    const onChild = `entry-${lantern.id}-k${dlc.index}`;
    expect(grid).toHaveAttribute('aria-activedescendant', onChild);
    for (const key of ['Shift', 'Control', 'Alt', 'Meta', 'Tab', 'Escape'])
      fireEvent.keyDown(grid, { key });
    expect(grid).toHaveAttribute('aria-activedescendant', onChild);
    // A real move still leaves the child rows.
    fireEvent.keyDown(grid, { key: 'Home' });
    expect(grid.getAttribute('aria-activedescendant')).not.toContain('-k');
  });

  it('↑ from the next game lands on the last child row of the one above', () => {
    render(<AddonHarness />);
    const games = buildGames(lib.entries, {}).games;
    const at = games.findIndex((g) => g.entry.name === 'Lantern Harbor (USA)');
    const lantern = games[at]!;
    const next = games[at + 1]!;
    const dlc = lantern.dlc.at(-1)!;
    const grid = screen.getByRole('grid');
    grid.focus();
    fireEvent.keyDown(grid, { key: 'Home' });
    while (grid.getAttribute('aria-activedescendant') !== `entry-${next.id}`)
      fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'ArrowUp' });
    expect(grid).toHaveAttribute(
      'aria-activedescendant',
      `entry-${lantern.id}-k${dlc.index}`,
    );
  });

  it('clicking a child slot opens the panel at that child', async () => {
    const user = userEvent.setup();
    const onOpenChild = vi.fn();
    render(<AddonHarness onOpenChild={onOpenChild} />);
    const lantern = lib.entries.find((e) => e.name === 'Lantern Harbor (USA)')!;
    const dlc = lantern.children!.find((c) => c.role === 'dlc')!;
    await user.click(
      within(childRows()[0]!).getByRole('gridcell', { name: 'Banner: falta' }),
    );
    expect(onOpenChild).toHaveBeenCalledWith(lantern.id, dlc.index);
  });
});
