import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Root } from '@/Root';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  type MockBackend,
  type MockOptions,
} from '@/ipc/mock/mockBackend';
import { resetLibraryStore } from '@/stores/libraryStore';
import { resetNavStore, useNavStore } from '@/stores/navStore';
import { resetSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

let backend: MockBackend;
let restore: () => void;

function make(opts: Partial<MockOptions> = {}) {
  backend = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: new MemoryStorage(),
    integrations: null,
    keychain: null,
    platform: 'macos',
    ...opts,
  });
  setBackend(backend);
}

async function open(settingsButton = 'Settings') {
  render(<Root />);
  await screen.findByRole('button', { name: settingsButton });
  act(() => useNavStore.getState().openSettings('sources'));
  const panel = await screen.findByRole('tabpanel');
  // The hints come with the sources info.
  await within(panel).findAllByText(
    /^(Not available in this build|No key|No account|No disponible en esta versión|Sin clave|Sin cuenta)$/,
  );
  return panel;
}

/** A source's row in the order list, by its switch's name. */
const row = (panel: HTMLElement, name: RegExp | string) =>
  within(panel).getByRole('switch', { name }).closest('li') as HTMLElement;

const saved = async () => (await backend.getSettings()).settings;

beforeEach(() => {
  make();
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

describe('sources: the order only', () => {
  it('has no key fields, no Accounts block and no Switch catalogue', async () => {
    const panel = await open();
    expect(panel.querySelector('input[type="password"]')).toBeNull();
    expect(
      within(panel).queryByRole('heading', { name: 'Accounts' }),
    ).not.toBeInTheDocument();
    expect(within(panel).queryByLabelText(/API key/)).toBeNull();
    expect(
      within(panel).queryByRole('switch', { name: /catalogue/i }),
    ).not.toBeInTheDocument();
    expect(
      within(panel).queryByTestId('switch-catalog-status'),
    ).not.toBeInTheDocument();
    expect(panel.querySelector('[id^="setting-sources-switch"]')).toBeNull();
  });

  it('only SteamGridDB and ScreenScraper link to their card in Integrations', async () => {
    const panel = await open();
    const links = within(panel).getAllByRole('button', {
      name: 'Set up in Integrations',
    });
    expect(links).toHaveLength(2);
    expect(
      within(row(panel, /SteamGridDB/)).getByRole('button', {
        name: 'Set up in Integrations',
      }),
    ).toBeInTheDocument();
    expect(
      within(row(panel, /ScreenScraper/)).getByRole('button', {
        name: 'Set up in Integrations',
      }),
    ).toBeInTheDocument();
    for (const name of [/libretro/, /Image from the game files/])
      expect(
        within(row(panel, name)).queryByRole('button', {
          name: 'Set up in Integrations',
        }),
      ).not.toBeInTheDocument();
  });

  it('tells the two «Set up in Integrations» buttons apart by their source', async () => {
    const panel = await open();
    const [first, second] = within(panel).getAllByRole('button', {
      name: 'Set up in Integrations',
    });
    const names = [first, second].map((b) =>
      within(b!.closest('li') as HTMLElement)
        .getByRole('switch')
        .getAttribute('id'),
    );
    expect(first).toHaveAccessibleDescription(
      names[0] === 'source-steamgriddb' ? 'SteamGridDB' : 'ScreenScraper',
    );
    expect(second).toHaveAccessibleDescription(
      names[1] === 'source-steamgriddb' ? 'SteamGridDB' : 'ScreenScraper',
    );
  });

  it('«Set up in Integrations» opens the card and focuses its header', async () => {
    const panel = await open();
    await userEvent.click(
      within(row(panel, /SteamGridDB/)).getByRole('button', {
        name: 'Set up in Integrations',
      }),
    );
    expect(useNavStore.getState().section).toBe('integrations');
    await waitFor(() =>
      expect(document.getElementById('integration-steamgriddb')).toHaveFocus(),
    );
  });

  it('says «No key» for SteamGridDB, with its switch off until a key is stored', async () => {
    const panel = await open();
    const sgdb = within(panel).getByRole('switch', { name: /SteamGridDB/ });
    expect(sgdb).toBeDisabled();
    expect(sgdb).toHaveAccessibleDescription('No key');
    expect(
      within(panel).queryByText('Store an API key below to turn it on'),
    ).not.toBeInTheDocument();
  });

  it('SteamGridDB turns on once a key is stored', async () => {
    await backend.setCredential('steamgriddb-api-key', 'sk-SECRET');
    const panel = await open();
    const sgdb = within(panel).getByRole('switch', { name: /SteamGridDB/ });
    await waitFor(() => expect(sgdb).toBeEnabled());
    expect(within(panel).queryByText('No key')).not.toBeInTheDocument();
    await userEvent.click(sgdb);
    expect(
      (await saved()).sources.order.find((e) => e.id === 'steamgriddb')!
        .enabled,
    ).toBe(true);
    expect(document.body.innerHTML).not.toContain('sk-SECRET');
  });

  it('an enabled source without its key can still be turned off', async () => {
    await backend.updateSettings({
      sources: {
        order: [
          { id: 'libretro', enabled: true },
          { id: 'steamgriddb', enabled: true },
          { id: 'screenscraper', enabled: false },
        ],
      },
    });
    const panel = await open();
    const sgdb = within(panel).getByRole('switch', { name: /SteamGridDB/ });
    expect(sgdb).toBeChecked();
    expect(sgdb).toBeEnabled();
    await userEvent.click(sgdb);
    expect(
      (await saved()).sources.order.find((e) => e.id === 'steamgriddb')!
        .enabled,
    ).toBe(false);
    await waitFor(() => expect(sgdb).toBeDisabled());
  });

  it('ScreenScraper says not available in an official build', async () => {
    const panel = await open();
    const ss = within(panel).getByRole('switch', { name: /ScreenScraper/ });
    expect(ss).toBeDisabled();
    expect(ss).toHaveAccessibleDescription('Not available in this build');
  });

  it('ScreenScraper says «No account» in a build that can use it', async () => {
    make({ integrations: 'dev' });
    await backend.forgetIntegration('screenscraper');
    await backend.updateSettings({
      sources: {
        order: [
          { id: 'libretro', enabled: true },
          { id: 'steamgriddb', enabled: false },
          { id: 'screenscraper', enabled: false },
        ],
      },
    });
    const panel = await open();
    const ss = within(panel).getByRole('switch', { name: /ScreenScraper/ });
    await waitFor(() => expect(ss).toHaveAccessibleDescription('No account'));
    expect(ss).toBeDisabled();
  });

  it('speaks Spanish', async () => {
    await backend.updateSettings({ general: { language: 'es' } });
    const panel = await open('Configuración');
    expect(
      within(panel).getByRole('switch', { name: /SteamGridDB/ }),
    ).toHaveAccessibleDescription('Sin clave');
    expect(
      within(panel).getAllByRole('button', {
        name: 'Configurar en Integraciones',
      }),
    ).toHaveLength(2);
    expect(
      within(panel).getByText('No disponible en esta versión'),
    ).toBeInTheDocument();
  });
});
