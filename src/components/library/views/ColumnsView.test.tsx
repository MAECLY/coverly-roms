import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { EntryView } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';
import { DEFAULT_PREFS } from '@/lib/viewPrefs';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

import { ColumnsView, MILLER_ROW_H } from './ColumnsView';

const lib = generateLibrary({
  seed: 12,
  count: 600,
  root: '/mock/Lib',
  generation: 1,
  now: FIXTURE_NOW,
});

/** Every entry of `lib` moved into one system's top folder. */
function flat(system: number): EntryView[] {
  const dir = lib.systems[system]!.dir;
  return lib.entries.map((e) => ({
    ...e,
    system,
    dir,
    rel_path: `${dir}/${e.name}.${e.ext}`,
  }));
}

function Harness({
  entries,
  initial = null,
  onOpen = () => {},
}: {
  entries: readonly EntryView[];
  initial?: number | null;
  onOpen?: (id: number) => void;
}) {
  const [selected, setSelected] = useState<number | null>(initial);
  return (
    <I18nProvider locale="en">
      <button type="button">before</button>
      <ColumnsView
        entries={entries}
        sections={[{ key: 'all', label: '', start: 0, count: entries.length }]}
        prefs={{ ...DEFAULT_PREFS, view: 'columns' }}
        onPreview={() => {}}
        systems={lib.systems}
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

/** The first system of the fixture that has entries in a sub-folder. */
const withFolders = lib.systems.find((s) =>
  lib.entries.some((e) => e.system === s.index && e.dir !== s.dir),
)!;

describe('ColumnsView', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
    useViewStore.setState({ scope: { kind: 'all' } });
  });
  afterEach(() => {
    restore();
    clearArtCache();
    setBackend(null);
    useViewStore.setState({ scope: { kind: 'all' } });
  });

  it('shows Systems, Folders and Games lists and a Preview pane', () => {
    const first = lib.entries.find((e) => e.system === withFolders.index)!;
    render(<Harness entries={lib.entries} initial={first.id} />);
    expect(screen.getByRole('listbox', { name: 'Systems' })).toBeVisible();
    expect(screen.getByRole('listbox', { name: 'Folders' })).toBeVisible();
    expect(screen.getByRole('listbox', { name: 'Games' })).toBeVisible();
    expect(screen.getByRole('region', { name: 'Preview' })).toBeVisible();
    expect(screen.getByRole('group', { name: 'Columns view' })).toBeVisible();
  });

  it('lists the systems in sidebar order with an icon and a count', () => {
    render(<Harness entries={lib.entries} />);
    const options = within(
      screen.getByRole('listbox', { name: 'Systems' }),
    ).getAllByRole('option');
    const shown = lib.systems.filter((s) =>
      lib.entries.some((e) => e.system === s.index),
    );
    expect(options).toHaveLength(shown.length);
    expect(options[0]).toHaveTextContent(shown[0]!.name);
    const n = lib.entries.filter((e) => e.system === shown[0]!.index).length;
    expect(options[0]).toHaveTextContent(String(n));
    expect(options[0]!.querySelector('[data-system-icon]')).not.toBeNull();
    expect(options[0]!.style.height).toBe(`${MILLER_ROW_H}px`);
  });

  it('leaves the Folders pane out for a system with no sub-folders', () => {
    render(<Harness entries={flat(2)} />);
    expect(screen.queryByRole('listbox', { name: 'Folders' })).toBeNull();
    expect(screen.getByRole('listbox', { name: 'Games' })).toBeVisible();
  });

  it('walks the panes with → and ←', async () => {
    const user = userEvent.setup();
    render(<Harness entries={lib.entries} />);
    const systems = screen.getByRole('listbox', { name: 'Systems' });
    // Choose the system with sub-folders by clicking it.
    await user.click(
      within(systems).getByRole('option', {
        name: new RegExp(withFolders.name.replace(/[/()]/g, '\\$&')),
      }),
    );
    expect(systems).toHaveFocus();
    await user.keyboard('{ArrowRight}');
    const folders = screen.getByRole('listbox', { name: 'Folders' });
    expect(folders).toHaveFocus();
    await user.keyboard('{ArrowRight}');
    const games = screen.getByRole('listbox', { name: 'Games' });
    expect(games).toHaveFocus();
    // Going into the games selects the first one, as Finder does.
    const active = games.getAttribute('aria-activedescendant');
    expect(active).toMatch(/^entry-\d+$/);
    expect(document.getElementById(active!)).toHaveAttribute(
      'aria-selected',
      'true',
    );
    // → on a game goes nowhere.
    await user.keyboard('{ArrowRight}');
    expect(games).toHaveFocus();
    await user.keyboard('{ArrowLeft}');
    expect(folders).toHaveFocus();
    await user.keyboard('{ArrowLeft}');
    expect(systems).toHaveFocus();
  });

  it('↓ in the Systems pane changes the games shown', async () => {
    const user = userEvent.setup();
    render(<Harness entries={lib.entries} />);
    const systems = screen.getByRole('listbox', { name: 'Systems' });
    const games = screen.getByRole('listbox', { name: 'Games' });
    const before = within(games).getAllByRole('option')[0]!.id;
    systems.focus();
    await user.keyboard('{ArrowDown}');
    const chosen = document.getElementById(
      systems.getAttribute('aria-activedescendant')!,
    );
    expect(chosen).toHaveAttribute('aria-selected', 'true');
    // The same pane, now listing the next system's games.
    expect(screen.getByRole('listbox', { name: 'Games' })).toBe(games);
    expect(within(games).getAllByRole('option')[0]!.id).not.toBe(before);
  });

  it('Enter on a branch goes in, Enter on a game opens it', async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    render(<Harness entries={flat(2)} onOpen={onOpen} />);
    const systems = screen.getByRole('listbox', { name: 'Systems' });
    systems.focus();
    await user.keyboard('{Enter}');
    const games = screen.getByRole('listbox', { name: 'Games' });
    expect(games).toHaveFocus();
    expect(onOpen).not.toHaveBeenCalled();
    await user.keyboard('{ArrowDown}{Enter}');
    expect(onOpen).toHaveBeenCalledTimes(1);
    const id = Number(
      games.getAttribute('aria-activedescendant')!.replace('entry-', ''),
    );
    expect(onOpen).toHaveBeenCalledWith(id);
  });

  it('clicking a system does not change the sidebar scope', async () => {
    const user = userEvent.setup();
    render(<Harness entries={lib.entries} />);
    const options = within(
      screen.getByRole('listbox', { name: 'Systems' }),
    ).getAllByRole('option');
    await user.click(options[2]!);
    expect(options[2]).toHaveAttribute('aria-selected', 'true');
    expect(useViewStore.getState().scope).toEqual({ kind: 'all' });
  });

  it('hides the Systems pane when the scope is one system', () => {
    useViewStore.setState({ scope: { kind: 'system', system: 2 } });
    render(<Harness entries={flat(2)} />);
    expect(screen.queryByRole('listbox', { name: 'Systems' })).toBeNull();
    expect(screen.getByRole('listbox', { name: 'Games' })).toBeVisible();
  });

  it('opens on the selected game, its system and folder', () => {
    const deep = lib.entries.find(
      (e) => e.system === withFolders.index && e.dir !== withFolders.dir,
    )!;
    render(<Harness entries={lib.entries} initial={deep.id} />);
    const games = screen.getByRole('listbox', { name: 'Games' });
    expect(games).toHaveAttribute('aria-activedescendant', `entry-${deep.id}`);
    expect(document.getElementById(`entry-${deep.id}`)).not.toBeNull();
    // The Games pane is the view's tab stop.
    expect(games).toHaveAttribute('tabindex', '0');
    const systems = screen.getByRole('listbox', { name: 'Systems' });
    expect(systems).toHaveAttribute('tabindex', '-1');
    const sys = document.getElementById(
      systems.getAttribute('aria-activedescendant')!,
    );
    expect(sys).toHaveTextContent(withFolders.name);
    const folders = screen.getByRole('listbox', { name: 'Folders' });
    const dir = document.getElementById(
      folders.getAttribute('aria-activedescendant')!,
    );
    expect(dir).toHaveTextContent(deep.dir.split('/').pop()!);
    // The preview shows that game.
    expect(
      within(screen.getByRole('region', { name: 'Preview' })).getByRole(
        'heading',
        { name: deep.name },
      ),
    ).toBeVisible();
  });

  it('jumps by typed letters in the focused pane', async () => {
    const user = userEvent.setup();
    render(<Harness entries={lib.entries} />);
    const systems = screen.getByRole('listbox', { name: 'Systems' });
    systems.focus();
    await user.keyboard('ga');
    const chosen = document.getElementById(
      systems.getAttribute('aria-activedescendant')!,
    );
    expect(chosen?.textContent?.toLowerCase().startsWith('ga')).toBe(true);
  });

  it('renders about one screen of a 12k games pane, active row kept', () => {
    const big = generateLibrary({
      seed: 12,
      count: 12_000,
      root: '/mock/Big',
      generation: 1,
      now: FIXTURE_NOW,
    });
    const dir = big.systems[0]!.dir;
    const one = big.entries.map((e) => ({ ...e, system: 0, dir }));
    render(<Harness entries={one} initial={one[5]!.id} />);
    const games = screen.getByRole('listbox', { name: 'Games' });
    expect(within(games).getAllByRole('option').length).toBeLessThan(60);
    expect(MILLER_ROW_H).toBe(28);
    games.scrollTop = 1_000_000;
    fireEvent.scroll(games);
    expect(
      document.getElementById(games.getAttribute('aria-activedescendant')!),
    ).not.toBeNull();
  });

  it('is one tab stop: Tab leaves the view', async () => {
    const user = userEvent.setup();
    render(<Harness entries={lib.entries} />);
    await user.click(screen.getByRole('button', { name: 'before' }));
    await user.tab();
    expect(screen.getByRole('listbox', { name: 'Systems' })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole('button', { name: 'after' })).toHaveFocus();
  });

  it('hands focus from the view root to the active pane', () => {
    render(<Harness entries={lib.entries} />);
    screen.getByRole('group', { name: 'Columns view' }).focus();
    expect(screen.getByRole('listbox', { name: 'Systems' })).toHaveFocus();
  });
});
