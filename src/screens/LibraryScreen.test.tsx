import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import { applyFilters, EMPTY_FILTERS } from '@/lib/filters';
import { dateBucket } from '@/lib/scope';
import {
  DEFAULT_PREFS,
  prefsFor,
  readPrefsFile,
  VIEW_IDS,
  type ViewId,
} from '@/lib/viewPrefs';
import { useFilterStore } from '@/stores/filterStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { useOrganizeStore } from '@/stores/organizeStore';
import { useSelectionStore } from '@/stores/selectionStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

const SMALL = MOCK_LIBRARIES[1].path;
const EMPTY = MOCK_LIBRARIES[2].path;

async function openLibrary(path: string = SMALL, locale: 'en' | 'es' = 'en') {
  setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
  resetLibraryStore();
  await useLibraryStore.getState().open(path);
  render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
  return useLibraryStore.getState().view!;
}

describe('Library screen', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    useNavStore.getState().closeSettings();
    setBackend(null);
  });

  it('shows the library name and counts', async () => {
    const view = await openLibrary();
    expect(
      screen.getByRole('heading', { level: 1, name: 'Small library' }),
    ).toBeInTheDocument();
    expect(screen.getByText('240 of 240 ROMs')).toBeInTheDocument();
    const missing = view.entries.filter(
      (e) => e.art.cover === 'missing',
    ).length;
    expect(screen.getByText(`${missing} without cover`)).toBeInTheDocument();
    expect(
      screen.getByRole('grid', { name: 'Covers view' }),
    ).toBeInTheDocument();
  });

  it('switches to a system from the Systems view', async () => {
    const view = await openLibrary();
    const nav = screen.getByRole('navigation', { name: 'Views' });
    await userEvent.click(within(nav).getByRole('tab', { name: 'Systems' }));
    const gba = view.systems.find((s) => s.folder === 'GBA')!;
    await userEvent.click(within(nav).getByText(gba.name));
    expect(
      await screen.findByText(`${gba.roms} of 240 ROMs`),
    ).toBeInTheDocument();
  });

  it('browses a folder from the Folders view', async () => {
    const view = await openLibrary();
    const nav = screen.getByRole('navigation', { name: 'Views' });
    await userEvent.click(within(nav).getByRole('tab', { name: 'Folders' }));
    await userEvent.click(within(nav).getByText('SFC'));
    const inSfc = view.entries.filter((e) =>
      e.dir.startsWith('ROMS/SFC'),
    ).length;
    expect(await screen.findByText(`${inSfc} of 240 ROMs`)).toBeInTheDocument();
  });

  it('groups by date in the Date view', async () => {
    const view = await openLibrary();
    const now = Math.floor(Date.now() / 1000);
    const nav = screen.getByRole('navigation', { name: 'Views' });
    await userEvent.click(within(nav).getByRole('tab', { name: 'Date' }));
    await userEvent.click(within(nav).getByText('Older'));
    const older = view.entries.filter(
      (e) => dateBucket(e.mtime, now) === 'older',
    ).length;
    expect(await screen.findByText(`${older} of 240 ROMs`)).toBeInTheDocument();
  });

  it('searches accent- and case-insensitively', async () => {
    await openLibrary();
    await userEvent.type(
      screen.getByRole('searchbox', { name: 'Search by name' }),
      'POKEMON edicion oro',
    );
    expect(await screen.findByText('1 of 240 ROMs')).toBeInTheDocument();
  });

  it('combines filters and reflects them in counts', async () => {
    const view = await openLibrary();
    await userEvent.click(screen.getByRole('button', { name: 'Filters' }));
    const panel = screen.getByRole('region', { name: 'Filters' });
    await userEvent.click(
      within(panel).getByRole('radio', { name: /^Missing cover/ }),
    );
    await userEvent.click(
      within(panel).getByRole('checkbox', { name: /^Super Nintendo/ }),
    );
    const snes = view.systems.find((s) => s.folder === 'SFC')!.index;
    const expected = applyFilters(
      view.entries,
      useLibraryStore.getState().keys,
      {
        ...EMPTY_FILTERS,
        art: 'missing-cover',
        systems: [snes],
      },
    ).length;
    expect(
      await screen.findByText(`${expected} of 240 ROMs`),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Filters (2)' }),
    ).toBeInTheDocument();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Clear filters' }),
    );
    expect(await screen.findByText('240 of 240 ROMs')).toBeInTheDocument();
  });

  it('moves focus to the new view, named after it, only after a switch', async () => {
    await openLibrary();
    expect(document.activeElement).toBe(document.body);
    const layout = screen.getByRole('radiogroup', { name: 'View' });
    await userEvent.click(
      within(layout).getByRole('radio', { name: 'Details' }),
    );
    expect(document.activeElement).toBe(
      screen.getByRole('grid', { name: 'Details view' }),
    );
    fireEvent.keyDown(document.activeElement!, { key: '4', ctrlKey: true });
    const gallery = await screen.findByRole('group', { name: 'Gallery view' });
    expect(gallery).toContainElement(document.activeElement as HTMLElement);
    fireEvent.keyDown(document.activeElement!, { key: '6', metaKey: true });
    expect(document.activeElement).toBe(
      await screen.findByRole('grid', { name: 'Shelves view' }),
    );
    act(() => useViewStore.getState().setView('covers'));
  });

  it('opens each library with the view it was left in', async () => {
    const views = useViewStore.getState();
    views.loadFor(SMALL);
    views.setView('details');
    views.loadFor('/elsewhere');
    views.setView('covers');
    await openLibrary();
    expect(useViewStore.getState().root).toBe(SMALL);
    expect(
      screen.getByRole('grid', { name: 'Details view' }),
    ).toBeInTheDocument();
    expect(screen.queryByRole('grid', { name: 'Covers view' })).toBeNull();
    // Opening a library is not a switch: the focus is left where it was.
    expect(document.activeElement).toBe(document.body);
    act(() => useViewStore.getState().setView('covers'));
  });

  it('previews the active game with Space and keeps the drawer on Escape', async () => {
    const user = userEvent.setup();
    const view = await openLibrary();
    const grid = screen.getByRole('grid', { name: 'Covers view' });
    grid.focus();
    await user.keyboard('{ArrowRight}{Enter}');
    const first = useViewStore.getState().drawerOpen;
    expect(first).toBe(true);
    grid.focus();
    await user.keyboard(' ');
    const entry =
      view.entries[
        Number(grid.getAttribute('aria-activedescendant')!.slice(6))
      ]!;
    expect(
      screen.getByRole('dialog', { name: entry.name }),
    ).toBeInTheDocument();
    // Each step moves both the preview and the selection on.
    await user.keyboard('{ArrowRight}{ArrowRight}');
    const id = Number(grid.getAttribute('aria-activedescendant')!.slice(6));
    expect(id).not.toBe(entry.id);
    const stepped = view.entries[id]!;
    expect(
      screen.getByRole('dialog', { name: stepped.name }),
    ).toHaveTextContent(/^.*3 of /);
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(useViewStore.getState().drawerOpen).toBe(true);
    expect(grid).toHaveFocus();
    act(() => useViewStore.getState().closeDrawer());
  });

  it('enables Fill missing art', async () => {
    await openLibrary();
    expect(
      screen.getByRole('button', { name: 'Fill missing art' }),
    ).toBeEnabled();
  });

  it('focuses the search with the slash key', async () => {
    await openLibrary();
    fireEvent.keyDown(document.body, { key: '/' });
    expect(
      screen.getByRole('searchbox', { name: 'Search by name' }),
    ).toHaveFocus();
  });

  it('keeps a decimal size as typed and filters by it', async () => {
    const view = await openLibrary();
    await userEvent.click(screen.getByRole('button', { name: 'Filters' }));
    const panel = screen.getByRole('region', { name: 'Filters' });
    const min = within(panel).getByRole('spinbutton', { name: 'Min' });
    // Each partial text the browser reports while typing must survive.
    for (const text of ['1', '1.0', '1.05']) {
      fireEvent.change(min, { target: { value: text } });
      expect(min).toHaveDisplayValue(text);
    }
    expect(useFilterStore.getState().filters.sizeMin).toBe(
      Math.round(1.05 * 1024 * 1024),
    );
    const expected = applyFilters(
      view.entries,
      useLibraryStore.getState().keys,
      { ...EMPTY_FILTERS, sizeMin: Math.round(1.05 * 1024 * 1024) },
    ).length;
    expect(
      await screen.findByText(`${expected} of 240 ROMs`),
    ).toBeInTheDocument();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Clear filters' }),
    );
    expect(min).toHaveValue(null);
  });

  it('focuses the search with slash from a filter checkbox', async () => {
    await openLibrary();
    await userEvent.click(screen.getByRole('button', { name: 'Filters' }));
    const panel = screen.getByRole('region', { name: 'Filters' });
    const box = within(panel).getAllByRole('checkbox')[0]!;
    box.focus();
    fireEvent.keyDown(box, { key: '/' });
    expect(
      screen.getByRole('searchbox', { name: 'Search by name' }),
    ).toHaveFocus();
  });

  it('opens the Settings screen from the toolbar and comes back', async () => {
    await openLibrary();
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(
      screen.getByRole('tabpanel', { name: 'General' }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(
      screen.getByRole('heading', { level: 1, name: 'Small library' }),
    ).toBeInTheDocument();
  });

  it('shows unfiltered library totals in every sidebar view', async () => {
    const view = await openLibrary();
    await userEvent.type(
      screen.getByRole('searchbox', { name: 'Search by name' }),
      'zzzzqqq',
    );
    const nav = screen.getByRole('navigation', { name: 'Views' });
    await userEvent.click(within(nav).getByRole('tab', { name: 'Systems' }));
    const gba = view.systems.find((s) => s.folder === 'GBA')!;
    const total = view.entries.filter((e) => e.system === gba.index).length;
    const item = within(nav).getByText(gba.name).closest('button')!;
    expect(within(item).getByText(String(total))).toBeInTheDocument();
  });

  it('moves between sidebar tabs with the arrow keys', async () => {
    await openLibrary();
    const nav = screen.getByRole('navigation', { name: 'Views' });
    const folders = within(nav).getByRole('tab', { name: 'Folders' });
    await userEvent.click(folders);
    await userEvent.keyboard('{ArrowRight}');
    const systems = within(nav).getByRole('tab', { name: 'Systems' });
    expect(systems).toHaveFocus();
    expect(systems).toHaveAttribute('aria-selected', 'true');
    expect(systems).toHaveAttribute('tabindex', '0');
    expect(folders).toHaveAttribute('tabindex', '-1');
    await userEvent.keyboard('{ArrowLeft}{ArrowLeft}');
    expect(within(nav).getByRole('tab', { name: 'All' })).toHaveFocus();
  });

  it('formats sidebar and facet counts in the app locale', async () => {
    await openLibrary(MOCK_LIBRARIES[0].path, 'es');
    const nav = screen.getByRole('navigation', { name: 'Vistas' });
    await userEvent.click(within(nav).getByRole('tab', { name: 'Todo' }));
    expect(within(nav).getByText('12.000')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Filtros' }));
    const panel = screen.getByRole('region', { name: 'Filtros' });
    expect(within(panel).getAllByText('12.000').length).toBeGreaterThan(0);
  });

  it('names every form field so autofill and devtools can identify it', async () => {
    await openLibrary();
    await userEvent.click(screen.getByRole('button', { name: 'Filters' }));
    const fields = document.querySelectorAll('input, select, textarea');
    expect(fields.length).toBeGreaterThan(5);
    const unnamed = [...fields].filter(
      (f) => !f.getAttribute('name') && !f.getAttribute('id'),
    );
    expect(unnamed.map((f) => f.outerHTML)).toEqual([]);
  });

  it('offers to clear when nothing matches', async () => {
    await openLibrary();
    await userEvent.type(
      screen.getByRole('searchbox', { name: 'Search by name' }),
      'zzzzqqq',
    );
    expect(
      await screen.findByText('No ROMs match the search and filters.'),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', { name: 'Clear filters' }),
    );
    expect(await screen.findByText('240 of 240 ROMs')).toBeInTheDocument();
  });

  it('empty_library_explains_itself', async () => {
    await openLibrary(EMPTY);
    expect(
      screen.getByText('No ROMs found in this folder.'),
    ).toBeInTheDocument();
  });

  it('speaks Spanish', async () => {
    await openLibrary(SMALL, 'es');
    expect(screen.getByText('240 de 240 ROMs')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Sistemas' })).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Rellenar arte que falta' }),
    ).toBeEnabled();
  });

  it('closes back to the start screen', async () => {
    await openLibrary();
    await userEvent.click(
      screen.getByRole('button', { name: 'Close library' }),
    );
    expect(
      screen.getByRole('button', { name: 'Open folder…' }),
    ).toBeInTheDocument();
  });

  it('offers to organise loose files only when the scan saw some', async () => {
    const view = await openLibrary();
    expect(
      screen.queryByRole('button', { name: 'Organise loose files…' }),
    ).not.toBeInTheDocument();
    act(() => useLibraryStore.setState({ view: { ...view, loose_files: 3 } }));
    await userEvent.click(
      screen.getByRole('button', { name: 'Organise loose files…' }),
    );
    expect(useNavStore.getState()).toMatchObject({
      screen: 'organize',
      organizeSource: view.root,
      organizeLaunch: {
        layout: view.profile.id,
        mode: 'move',
        locked: true,
        flat: view.profile.fitted,
      },
    });
    act(() => {
      useOrganizeStore.getState().reset();
      useNavStore.getState().closeOrganize();
    });
  });
});

const VIEW_NAMES: Record<ViewId, string> = {
  covers: 'Covers',
  details: 'Details',
  columns: 'Columns',
  gallery: 'Gallery',
  mounts: 'Art slots',
  shelves: 'Shelves',
};
/** The role of each view's root (spec §B.8). */
const VIEW_ROLES: Record<ViewId, 'grid' | 'group'> = {
  covers: 'grid',
  details: 'grid',
  columns: 'group',
  gallery: 'group',
  mounts: 'grid',
  shelves: 'grid',
};

function viewRoot(id: ViewId): HTMLElement {
  return screen.getByRole(VIEW_ROLES[id], { name: `${VIEW_NAMES[id]} view` });
}

function mountApp() {
  return render(
    <I18nProvider locale="en">
      <App />
    </I18nProvider>,
  );
}

/** Every library touched here back to the default prefs. */
function resetPrefs() {
  act(() => {
    for (const root of [EMPTY, SMALL]) {
      useViewStore.getState().loadFor(root);
      useViewStore.getState().setPrefs({ ...DEFAULT_PREFS });
    }
    useViewStore.getState().reset();
    useSelectionStore.getState().select(null);
  });
}

function press(target: Element, key: string, mod: 'meta' | 'ctrl') {
  return fireEvent.keyDown(target, {
    key,
    metaKey: mod === 'meta',
    ctrlKey: mod === 'ctrl',
  });
}

describe('Library views', () => {
  let restore: () => void;
  let storage: MemoryStorage;
  beforeEach(() => {
    restore = stubLayout(1440, 900);
    storage = new MemoryStorage();
    vi.stubGlobal('localStorage', storage);
  });
  afterEach(() => {
    resetPrefs();
    vi.unstubAllGlobals();
    restore();
    setBackend(null);
  });

  it('switches through all six views from the segments', async () => {
    await openLibrary();
    const switcher = screen.getByRole('radiogroup', { name: 'View' });
    const radios = within(switcher).getAllByRole('radio');
    expect(radios.map((r) => r.getAttribute('aria-label'))).toEqual(
      VIEW_IDS.map((id) => VIEW_NAMES[id]),
    );
    const statuses = screen.queryAllByRole('status').length;
    // Covers first is not a switch; start from Details so every view is one.
    for (const id of [...VIEW_IDS.slice(1), 'covers' as const]) {
      await userEvent.click(
        within(switcher).getByRole('radio', { name: VIEW_NAMES[id] }),
      );
      expect(useViewStore.getState().prefs.view).toBe(id);
      expect(
        within(switcher).getByRole('radio', { name: VIEW_NAMES[id] }),
      ).toBeChecked();
      const root = viewRoot(id);
      expect(root).toContainElement(document.activeElement as HTMLElement);
      // The view's own name is the announcement: no extra live region.
      expect(screen.queryAllByRole('status')).toHaveLength(statuses);
    }
  });

  it.each(['meta', 'ctrl'] as const)(
    'switches views with %s+1–6, also from the search box',
    async (mod) => {
      await openLibrary();
      const search = screen.getByRole('searchbox', { name: 'Search by name' });
      search.focus();
      press(search, '3', mod);
      expect(
        await screen.findByRole('group', { name: 'Columns view' }),
      ).toBeInTheDocument();
      for (const [i, id] of VIEW_IDS.entries()) {
        press(document.activeElement ?? document.body, String(i + 1), mod);
        expect(useViewStore.getState().prefs.view).toBe(id);
        expect(viewRoot(id)).toBeInTheDocument();
      }
    },
  );

  it('leaves the view alone while Fill is open', async () => {
    await openLibrary();
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    press(document.activeElement ?? document.body, '3', 'ctrl');
    expect(useViewStore.getState().prefs.view).toBe('covers');
    act(() => useViewStore.getState().closeOverlay());
  });

  it('does not resize covers with Ctrl+= in Details', async () => {
    await openLibrary();
    press(document.body, '2', 'ctrl');
    const grid = viewRoot('details');
    const size = useViewStore.getState().prefs.coverSize;
    const ev = new KeyboardEvent('keydown', {
      key: '=',
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    });
    act(() => {
      grid.dispatchEvent(ev);
    });
    expect(ev.defaultPrevented).toBe(false);
    expect(useViewStore.getState().prefs.coverSize).toBe(size);
  });

  it('keeps the selection active across views', async () => {
    const view = await openLibrary();
    const covers = viewRoot('covers');
    const tiles = covers.querySelectorAll<HTMLElement>('[id^="entry-"]');
    const tile = tiles[2]!;
    await userEvent.click(tile);
    const id = useSelectionStore.getState().selectedId;
    expect(id).not.toBeNull();
    expect(tile.id).toBe(`entry-${id}`);
    const switcher = screen.getByRole('radiogroup', { name: 'View' });

    await userEvent.click(
      within(switcher).getByRole('radio', { name: 'Details' }),
    );
    const details = viewRoot('details');
    expect(details).toHaveAttribute('aria-activedescendant', `entry-${id}`);
    const row = details.querySelector(`[id="entry-${id}"]`)!;
    expect(row).toHaveAttribute('role', 'row');
    expect(row).toHaveAttribute('aria-selected', 'true');

    await userEvent.click(
      within(switcher).getByRole('radio', { name: 'Columns' }),
    );
    const games = within(viewRoot('columns')).getByRole('listbox', {
      name: /Games/,
    });
    expect(games).toHaveAttribute('aria-activedescendant', `entry-${id}`);
    expect(games.querySelector(`[id="entry-${id}"]`)).toHaveAttribute(
      'aria-selected',
      'true',
    );

    await userEvent.click(
      within(switcher).getByRole('radio', { name: 'Gallery' }),
    );
    const strip = within(viewRoot('gallery')).getByRole('listbox', {
      name: 'Filmstrip',
    });
    expect(strip).toHaveAttribute('aria-activedescendant', `entry-${id}`);
    const entry = view.entries.find((e) => e.id === id)!;
    expect(
      within(strip).getByRole('option', { name: entry.name }),
    ).toHaveAttribute('aria-selected', 'true');
    expect(useSelectionStore.getState().selectedId).toBe(id);
  });

  it('restores the view after a remount', async () => {
    await openLibrary();
    await userEvent.click(
      within(screen.getByRole('radiogroup', { name: 'View' })).getByRole(
        'radio',
        { name: 'Columns' },
      ),
    );
    // Saved for this library, where a fresh start reads it back.
    expect(prefsFor(readPrefsFile(storage), SMALL).view).toBe('columns');
    cleanup();
    mountApp();
    expect(viewRoot('columns')).toBeInTheDocument();
  });

  it('keeps separate views for two libraries', async () => {
    await openLibrary();
    press(document.body, '3', 'ctrl');
    expect(viewRoot('columns')).toBeInTheDocument();
    await act(() => useLibraryStore.getState().open(EMPTY));
    expect(useViewStore.getState().root).toBe(EMPTY);
    act(() => useViewStore.getState().setView('shelves'));
    await act(() => useLibraryStore.getState().open(SMALL));
    expect(useViewStore.getState().root).toBe(SMALL);
    expect(useViewStore.getState().prefs.view).toBe('columns');
    expect(viewRoot('columns')).toBeInTheDocument();
    const file = readPrefsFile(storage);
    expect(prefsFor(file, EMPTY).view).toBe('shelves');
    expect(prefsFor(file, SMALL).view).toBe('columns');
  });

  it.each(['covers', 'details', 'shelves'] as const)(
    'shows system icons in the section headers of %s when grouped by system',
    async (id) => {
      const view = await openLibrary();
      act(() =>
        useViewStore.getState().setPrefs({ view: id, groupBy: 'system' }),
      );
      const headers = within(viewRoot(id)).getAllByRole('rowheader');
      expect(headers.length).toBeGreaterThan(0);
      const known = new Set(view.systems.map((s) => s.system_id ?? 'unknown'));
      for (const header of headers) {
        const icon = header.querySelector('[data-system-icon]');
        expect(icon).not.toBeNull();
        expect(known.has(icon!.getAttribute('data-system-icon')!)).toBe(true);
      }
    },
  );

  it('collapses the switcher into a usable menu when narrow', async () => {
    restore();
    restore = stubLayout(1000, 800);
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: query.includes('max-width: 1180px'),
      media: query,
      onchange: null,
      addListener: () => undefined,
      removeListener: () => undefined,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      dispatchEvent: () => false,
    }));
    await openLibrary();
    const trigger = screen.getByRole('button', { name: 'View, Covers' });
    await userEvent.click(trigger);
    const menu = await screen.findByRole('menu');
    expect(within(menu).getAllByRole('menuitemradio')).toHaveLength(6);
    await userEvent.click(
      within(menu).getByRole('menuitemradio', { name: /Gallery/ }),
    );
    expect(viewRoot('gallery')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'View, Gallery' }),
    ).toBeInTheDocument();
  });

  it('keeps compact rows in Details across a remount', async () => {
    await openLibrary();
    press(document.body, '2', 'ctrl');
    await userEvent.click(screen.getByRole('button', { name: 'View options' }));
    const dialog = await screen.findByRole('dialog', { name: 'View options' });
    await userEvent.click(
      within(dialog).getByRole('radio', { name: 'Compact' }),
    );
    await userEvent.keyboard('{Escape}');
    expect(prefsFor(readPrefsFile(storage), SMALL).density).toBe('compact');
    cleanup();
    mountApp();
    const rows = () =>
      viewRoot('details').querySelectorAll('[role="row"][data-entry-id]');
    expect(rows()[0]).toHaveClass('entry-row--compact');
  });

  it('shows system icons in the sidebar, the Details system cell and the drawer', async () => {
    const view = await openLibrary();
    const nav = screen.getByRole('navigation', { name: 'Views' });
    await userEvent.click(within(nav).getByRole('tab', { name: 'Systems' }));
    const gba = view.systems.find((s) => s.folder === 'GBA')!;
    const item = within(nav).getByText(gba.name).closest('button')!;
    expect(
      item.querySelector(`[data-system-icon="${gba.system_id}"]`),
    ).toHaveAttribute('aria-hidden', 'true');

    act(() => useViewStore.getState().setView('details'));
    const row = viewRoot('details').querySelector<HTMLElement>(
      '[role="row"][data-entry-id]',
    )!;
    const entry = view.entries.find(
      (e) => String(e.id) === row.dataset.entryId,
    )!;
    const system = view.systems[entry.system]!;
    expect(
      row.querySelector(
        `[data-system-icon="${system.system_id ?? 'unknown'}"]`,
      ),
    ).not.toBeNull();

    act(() => {
      useSelectionStore.getState().select(entry.id);
      useViewStore.getState().openDrawer();
    });
    const drawer = await screen.findByRole('complementary', {
      name: entry.name,
    });
    expect(
      drawer.querySelector(
        `[data-system-icon="${system.system_id ?? 'unknown'}"]`,
      ),
    ).not.toBeNull();
  });
});
