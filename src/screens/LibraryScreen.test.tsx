import { fireEvent, render, screen, within } from '@testing-library/react';
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
      screen.getByRole('grid', { name: 'ROM covers' }),
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

  it('toggles between grid and table', async () => {
    await openLibrary();
    await userEvent.click(screen.getByRole('button', { name: 'List' }));
    expect(screen.getByRole('grid', { name: 'ROM list' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Grid' }));
    expect(
      screen.getByRole('grid', { name: 'ROM covers' }),
    ).toBeInTheDocument();
  });

  it('keeps Fill missing art disabled until M4', async () => {
    await openLibrary();
    expect(
      screen.getByRole('button', { name: 'Fill missing art' }),
    ).toBeDisabled();
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

  it('leaves focus in the settings dialog on Ctrl+F', async () => {
    await openLibrary();
    await userEvent.click(
      screen.getByRole('button', { name: 'Library settings' }),
    );
    const dialog = screen.getByRole('dialog');
    fireEvent.keyDown(dialog, { key: 'f', ctrlKey: true });
    expect(
      screen.getByRole('searchbox', { name: 'Search by name' }),
    ).not.toHaveFocus();
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
    ).toBeDisabled();
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
});
