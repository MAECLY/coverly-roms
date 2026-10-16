import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
} from '@/ipc/mock/mockBackend';
import type { EntryView } from '@/ipc/types';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useSelectionStore } from '@/stores/selectionStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

let backend: MockBackend;

async function setup(locale: 'en' | 'es' = 'en') {
  backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
  setBackend(backend);
  resetLibraryStore();
  await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
  return useLibraryStore.getState().view!;
}

function show(entry: EntryView) {
  act(() => {
    useSelectionStore.getState().select(entry.id);
    useViewStore.getState().openDrawer();
  });
  return screen.getByRole('complementary', { name: entry.name });
}

describe('Detail drawer', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('opens from a click on a tile with file facts', async () => {
    const view = await setup();
    const tile = screen.getAllByRole('gridcell')[0]!;
    await userEvent.click(tile);
    const first = view.entries.find((e) => `entry-${e.id}` === tile.id)!;
    const drawer = screen.getByRole('complementary', { name: first.name });
    expect(within(drawer).getByText(first.rel_path)).toBeInTheDocument();
    expect(within(drawer).getByText(`.${first.ext}`)).toBeInTheDocument();
    expect(
      within(drawer).getByText(view.systems[first.system]!.name),
    ).toBeInTheDocument();
  });

  it('shows a placeholder for missing art and loads present art', async () => {
    const view = await setup();
    const entry = view.entries.find(
      (e) => e.art.cover === 'missing' && e.art.screenshot === 'present',
    )!;
    const drawer = show(entry);
    expect(within(drawer).getByText('No cover')).toBeInTheDocument();
    expect(
      await within(drawer).findAllByRole('presentation', { hidden: true }),
    ).not.toHaveLength(0);
  });

  it('identifies on demand', async () => {
    const view = await setup();
    const drawer = show(view.entries[0]!);
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Identify' }),
    );
    expect(await within(drawer).findByText('Content hash')).toBeInTheDocument();
    expect(within(drawer).getByText('100%')).toBeInTheDocument();
  });

  it('says when identification was offline', async () => {
    const view = await setup();
    const drawer = show(view.entries[2]!);
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Identify' }),
    );
    expect(
      await within(drawer).findByText(
        'Offline: identified by its file name only.',
      ),
    ).toBeInTheDocument();
    expect(
      within(drawer).getByText('Not found in the databases'),
    ).toBeInTheDocument();
  });

  it('reveals the file; searching by title stays for later', async () => {
    const view = await setup();
    const entry = view.entries[5]!;
    const drawer = show(entry);
    expect(
      within(drawer).getByRole('button', { name: 'Search…' }),
    ).toBeDisabled();
    expect(
      within(drawer).getByRole('button', { name: 'Change…' }),
    ).toBeEnabled();
    expect(
      within(drawer).getByRole('button', { name: 'From file…' }),
    ).toBeEnabled();
    expect(
      within(drawer).getByRole('button', { name: 'Remove' }),
    ).toHaveProperty('disabled', entry.art.cover !== 'present');
    await userEvent.click(
      within(drawer).getByRole('button', {
        name: /Reveal in Finder|Show in Explorer|Show in folder/,
      }),
    );
    expect(backend.revealed).toEqual([entry.id]);
  });

  it('drawer_renders_hostile_name_as_text', async () => {
    const view = await setup();
    const hostile = view.entries.find(
      (e) => e.name === '<img src=x onerror=alert(1)>',
    )!;
    const drawer = show(hostile);
    expect(
      within(drawer).getByRole('heading', { name: hostile.name }),
    ).toBeInTheDocument();
    expect(document.querySelector('img[src="x"]')).toBeNull();
  });

  it('closes with Escape and with the close button', async () => {
    const view = await setup();
    show(view.entries[0]!);
    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
    const drawer = show(view.entries[0]!);
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Close details' }),
    );
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
  });

  it('follows the keyboard selection', async () => {
    const view = await setup();
    show(view.entries[0]!);
    const grid = screen.getByRole('grid', { name: 'ROM covers' });
    fireEvent.keyDown(grid, { key: 'ArrowRight' });
    expect(
      screen.getByRole('complementary', { name: view.entries[1]!.name }),
    ).toBeInTheDocument();
  });

  it('speaks Spanish', async () => {
    const view = await setup('es');
    const drawer = show(view.entries[0]!);
    expect(
      within(drawer).getByRole('button', { name: 'Identificar' }),
    ).toBeInTheDocument();
    expect(
      within(drawer).getByRole('button', { name: 'Cambiar…' }),
    ).toBeEnabled();
  });
});
