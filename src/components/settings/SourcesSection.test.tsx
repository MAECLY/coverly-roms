import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Root } from '@/Root';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import type { CatalogStatus } from '@/ipc/types';
import { resetLibraryStore } from '@/stores/libraryStore';
import { resetNavStore, useNavStore } from '@/stores/navStore';
import { resetSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

import { foldCount } from './registry';

let backend: MockBackend;
let restore: () => void;

const ON = { catalogs: { switch: { enabled: true } } } as const;

async function open(status?: CatalogStatus, settingsButton = 'Settings') {
  if (status) vi.spyOn(backend, 'catalogStatus').mockResolvedValue(status);
  render(<Root />);
  await screen.findByRole('button', { name: settingsButton });
  act(() => useNavStore.getState().openSettings('sources'));
  return screen.findByRole('tabpanel');
}

/** The catalogue's status line (a live region). */
const statusLine = () =>
  screen.findByTestId('switch-catalog-status', {}, { timeout: 2000 });

const catalogSwitch = () =>
  screen.getByRole('switch', { name: 'Switch name catalogue' });
const iconsSwitch = () => screen.getByRole('switch', { name: 'Switch icons' });
const fallbackSwitch = () =>
  screen.getByRole('switch', { name: 'Online fallback' });
const openFold = async () =>
  userEvent.click(
    screen.getByRole('button', {
      name: `Advanced options (${foldCount('sources-advanced')})`,
    }),
  );

describe('sources: Switch name catalogue', () => {
  beforeEach(() => {
    backend = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: new MemoryStorage(),
    });
    setBackend(backend);
    resetSettingsStore();
    resetLibraryStore();
    resetNavStore();
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    vi.restoreAllMocks();
    setBackend(null);
  });

  it('off: says so, and icons, fallback and the buttons wait for it', async () => {
    const panel = await open();
    const group = within(panel).getByRole('group', {
      name: 'Switch catalogue',
    });
    await waitFor(async () =>
      expect(await statusLine()).toHaveTextContent('Off'),
    );
    expect(catalogSwitch()).not.toBeChecked();
    // The size it downloads is a number: data face.
    expect(within(group).getByText('90 MB').closest('.mono')).not.toBeNull();
    expect(iconsSwitch()).toBeDisabled();
    await openFold();
    expect(fallbackSwitch()).toBeDisabled();
    expect(
      screen.getByRole('textbox', { name: 'Fallback address' }),
    ).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Download now' })).toBeDisabled();
    expect(
      screen.getByRole('button', { name: 'Delete catalogue' }),
    ).toBeDisabled();
  });

  it('turning it on saves the setting and downloads the catalogue', async () => {
    const update = vi.spyOn(backend, 'catalogUpdate');
    await open();
    await userEvent.click(catalogSwitch());
    await waitFor(() => expect(update).toHaveBeenCalledWith('en'));
    expect((await backend.getSettings()).settings.catalogs.switch.enabled).toBe(
      true,
    );
    await waitFor(async () =>
      expect(await statusLine()).toHaveTextContent(
        '17,342 titles, updated Oct 30, 2026',
      ),
    );
    const line = await statusLine();
    expect(within(line).getByText('17,342').closest('.mono')).not.toBeNull();
    expect(
      within(line).getByText('Oct 30, 2026').closest('.mono'),
    ).not.toBeNull();
    expect(iconsSwitch()).toBeEnabled();
    expect(
      screen.getByRole('button', { name: 'Check for update' }),
    ).toBeEnabled();
  });

  it('downloading: bytes done of total in mono, with a progress bar, updated by events', async () => {
    await backend.updateSettings(ON);
    let push: ((s: CatalogStatus) => void) | null = null;
    vi.spyOn(backend, 'onCatalogProgress').mockImplementation((cb) => {
      push = cb;
      return Promise.resolve(() => undefined);
    });
    await open({ state: 'downloading', done: 45_200_000, total: 92_400_000 });
    const line = await statusLine();
    await waitFor(() =>
      expect(line).toHaveTextContent('Downloading 45.2 MB of 92.4 MB'),
    );
    expect(within(line).getByText('45.2 MB').closest('.mono')).not.toBeNull();
    expect(within(line).getByText('92.4 MB').closest('.mono')).not.toBeNull();
    expect(
      screen.getByRole('progressbar', { name: 'Switch name catalogue' }),
    ).toBeInTheDocument();
    // No second download while one runs.
    expect(screen.getByRole('button', { name: 'Download now' })).toBeDisabled();
    await waitFor(() => expect(push).not.toBeNull());
    act(() => push!({ state: 'downloading', done: 80_000_000, total: null }));
    expect(line).toHaveTextContent('Downloading 80 MB');
  });

  it('a download left running while on another tab still ends on return', async () => {
    await backend.updateSettings(ON);
    let push: ((s: CatalogStatus) => void) | null = null;
    vi.spyOn(backend, 'onCatalogProgress').mockImplementation((cb) => {
      push = cb;
      return Promise.resolve(() => undefined);
    });
    let finish: ((s: CatalogStatus) => void) | null = null;
    vi.spyOn(backend, 'catalogUpdate').mockImplementation(
      () => new Promise<CatalogStatus>((r) => (finish = r)),
    );
    await open({ state: 'pending' });
    await userEvent.click(
      await screen.findByRole('button', { name: 'Download now' }),
    );
    await waitFor(() => expect(finish).not.toBeNull());
    const now: CatalogStatus = {
      state: 'downloading',
      done: 10_000_000,
      total: 92_400_000,
    };
    act(() => push!(now));
    // The back end reports the running download to a status read.
    vi.mocked(backend.catalogStatus).mockResolvedValue(now);
    act(() => useNavStore.getState().openSettings('about'));
    act(() => useNavStore.getState().openSettings('sources'));
    await waitFor(async () =>
      expect(await statusLine()).toHaveTextContent(
        'Downloading 10 MB of 92.4 MB',
      ),
    );
    act(() =>
      finish!({
        state: 'ready',
        count: 17_342,
        updated: '2026-10-30',
        fallback: false,
      }),
    );
    await waitFor(async () =>
      expect(await statusLine()).toHaveTextContent(
        '17,342 titles, updated Oct 30, 2026',
      ),
    );
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Check for update' }),
    ).toBeEnabled();
  });

  it('a download this window did not start is read until it ends', async () => {
    await backend.updateSettings(ON);
    const read = vi.spyOn(backend, 'catalogStatus').mockResolvedValue({
      state: 'downloading',
      done: 30_000_000,
      total: 92_400_000,
    });
    await open();
    await waitFor(async () =>
      expect(await statusLine()).toHaveTextContent(
        'Downloading 30 MB of 92.4 MB',
      ),
    );
    read.mockResolvedValue({
      state: 'ready',
      count: 17_342,
      updated: '2026-10-30',
      fallback: false,
    });
    await waitFor(
      async () =>
        expect(await statusLine()).toHaveTextContent(
          '17,342 titles, updated Oct 30, 2026',
        ),
      { timeout: 3000 },
    );
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });

  it('a progress event that arrives after the download answered is ignored', async () => {
    await backend.updateSettings(ON);
    let push: ((s: CatalogStatus) => void) | null = null;
    vi.spyOn(backend, 'onCatalogProgress').mockImplementation((cb) => {
      push = cb;
      return Promise.resolve(() => undefined);
    });
    await open({ state: 'pending' });
    await userEvent.click(
      await screen.findByRole('button', { name: 'Download now' }),
    );
    const done = await screen.findByRole('button', {
      name: 'Check for update',
    });
    await waitFor(() => expect(done).toBeEnabled());
    act(() =>
      push!({ state: 'downloading', done: 92_400_000, total: 92_400_000 }),
    );
    expect(await statusLine()).toHaveTextContent(
      '17,342 titles, updated Oct 30, 2026',
    );
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    expect(done).toBeEnabled();
  });

  it.each<[CatalogStatus, string]>([
    [{ state: 'pending' }, 'Catalogue download pending'],
    [
      { state: 'ready', count: 9_876, updated: '2026-10-01', fallback: true },
      '9,876 titles from the United States list, updated Oct 1, 2026',
    ],
    [
      { state: 'failed-old', updated: '2026-09-14' },
      'Could not update; using the data from Sep 14, 2026.',
    ],
    [{ state: 'failed-none' }, 'Could not download; using file names.'],
    [
      { state: 'rejected' },
      'The catalogue holds data Coverly does not accept; the previous one is kept.',
    ],
    [
      { state: 'remove-failed' },
      'Could not delete it; close other Coverly windows.',
    ],
  ])('status %j', async (status, text) => {
    await backend.updateSettings(ON);
    await open(status);
    await waitFor(async () =>
      expect(await statusLine()).toHaveTextContent(text),
    );
  });

  it('speaks Spanish, with the date and the counts in the data face', async () => {
    await backend.updateSettings({ ...ON, general: { language: 'es' } });
    await open(
      { state: 'ready', count: 17_342, updated: '2026-10-30', fallback: false },
      'Configuración',
    );
    const line = await statusLine();
    await waitFor(() =>
      expect(line.textContent).toMatch(
        /^17\.342 títulos, actualizado el 30 oct 2026$/,
      ),
    );
    expect(
      screen.getByRole('switch', { name: 'Catálogo de nombres de Switch' }),
    ).toBeChecked();
    expect(
      screen.getByRole('button', { name: 'Buscar actualización' }),
    ).toBeEnabled();
  });

  it('«Delete catalogue» deletes it and shows what is left', async () => {
    await backend.updateSettings(ON);
    await backend.catalogUpdate();
    const remove = vi.spyOn(backend, 'catalogRemove');
    await open();
    const button = await screen.findByRole('button', {
      name: 'Delete catalogue',
    });
    await waitFor(() => expect(button).toBeEnabled());
    await userEvent.click(button);
    expect(remove).toHaveBeenCalledOnce();
    await waitFor(async () =>
      expect(await statusLine()).toHaveTextContent(
        'Catalogue download pending',
      ),
    );
    expect(screen.getByRole('button', { name: 'Download now' })).toBeEnabled();
  });

  it('icons and the fallback save once the catalogue is on', async () => {
    await backend.updateSettings(ON);
    await open();
    await userEvent.click(iconsSwitch());
    await openFold();
    await userEvent.click(fallbackSwitch());
    await waitFor(async () =>
      expect(
        (await backend.getSettings()).settings.catalogs.switch,
      ).toMatchObject({ icons: true, fallback: true }),
    );
  });

  it('a fallback address that is not https:// says it has to be', async () => {
    await backend.updateSettings(ON);
    await open();
    await openFold();
    const url = screen.getByRole('textbox', { name: 'Fallback address' });
    expect(url).toHaveValue('https://api.nlib.cc');
    await userEvent.clear(url);
    await userEvent.type(url, 'http://api.nlib.cc');
    await userEvent.tab();
    await waitFor(() =>
      expect(url).toHaveAccessibleDescription(
        expect.stringContaining('It has to start with https://'),
      ),
    );
    // The typed text stays; the saved one does not change.
    expect(url).toHaveValue('http://api.nlib.cc');
    expect(
      (await backend.getSettings()).settings.catalogs.switch.fallback_url,
    ).toBe('https://api.nlib.cc');

    await userEvent.clear(url);
    await userEvent.type(url, 'https://mirror.example.org');
    await userEvent.tab();
    await waitFor(async () =>
      expect(
        (await backend.getSettings()).settings.catalogs.switch.fallback_url,
      ).toBe('https://mirror.example.org'),
    );
    expect(url).not.toHaveAccessibleDescription(
      expect.stringContaining('https://'),
    );
  });
});
