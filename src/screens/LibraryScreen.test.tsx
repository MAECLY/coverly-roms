import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import { applyFilters, EMPTY_FILTERS } from '@/lib/filters';
import { dateBucket } from '@/lib/scope';
import { useFilterStore } from '@/stores/filterStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { useOrganizeStore } from '@/stores/organizeStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

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

  it('toggles between covers and details', async () => {
    await openLibrary();
    const layout = screen.getByRole('radiogroup', { name: 'View' });
    expect(within(layout).getByRole('radio', { name: 'Covers' })).toBeChecked();
    await userEvent.click(
      within(layout).getByRole('radio', { name: 'Details' }),
    );
    expect(
      screen.getByRole('grid', { name: 'Details view' }),
    ).toBeInTheDocument();
    expect(
      within(layout).getByRole('radio', { name: 'Details' }),
    ).toBeChecked();
    await userEvent.click(
      within(layout).getByRole('radio', { name: 'Covers' }),
    );
    expect(
      screen.getByRole('grid', { name: 'Covers view' }),
    ).toBeInTheDocument();
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
