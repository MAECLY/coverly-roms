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
import type { CatalogStatus } from '@/ipc/types';
import { PLANNED } from '@/lib/integrations';
import { resetLibraryStore } from '@/stores/libraryStore';
import { resetNavStore, useNavStore } from '@/stores/navStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

import { resetIntegrationCardMemory } from './IntegrationCard';
import { foldCount } from './registry';

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

async function open(
  opts: { status?: CatalogStatus; settingsButton?: string } = {},
) {
  if (opts.status)
    vi.spyOn(backend, 'catalogStatus').mockResolvedValue(opts.status);
  render(<Root />);
  await screen.findByRole('button', {
    name: opts.settingsButton ?? 'Settings',
  });
  act(() => useNavStore.getState().openSettings('integrations'));
  const panel = await screen.findByRole('tabpanel');
  // The cards come with the first view.
  await within(panel).findByRole('heading', { level: 3, name: /SteamGridDB/ });
  return panel;
}

/** A card's article, by its service name. */
const card = (name: RegExp | string) =>
  screen
    .getByRole('heading', { level: 3, name })
    .closest('article') as HTMLElement;

beforeEach(() => {
  make();
  resetSettingsStore();
  resetLibraryStore();
  resetNavStore();
  resetIntegrationCardMemory();
  restore = stubLayout(1280, 800);
});
afterEach(() => {
  restore();
  vi.restoreAllMocks();
  setBackend(null);
});

describe('integrations tab', () => {
  it('shows one card per working service under its group, in page order', async () => {
    const panel = await open();
    expect(
      within(panel)
        .getAllByRole('heading', { level: 2 })
        .map((h) => h.textContent),
    ).toEqual(['Covers and art', 'Catalogues', 'Updates', 'Coming later']);
    expect(
      within(panel)
        .getAllByRole('heading', { level: 3 })
        .map((h) => h.textContent?.replace(/,.*$/, '')),
    ).toEqual([
      'SteamGridDB',
      'ScreenScraper',
      'Switch catalogue',
      'Coverly updates',
    ]);
    expect(
      within(panel).queryByRole('heading', { name: 'Names and metadata' }),
    ).not.toBeInTheDocument();
    expect(
      within(panel).queryByRole('heading', { name: 'Achievements' }),
    ).not.toBeInTheDocument();
  });

  it('a view that failed to load gives way to the next one', async () => {
    const view = backend.integrationsView.bind(backend);
    vi.spyOn(backend, 'integrationsView')
      .mockRejectedValueOnce({ code: 'io', detail: 'boom' })
      .mockImplementation(view);
    render(<Root />);
    await screen.findByRole('button', { name: 'Settings' });
    act(() => useNavStore.getState().openSettings('integrations'));
    const panel = await screen.findByRole('tabpanel');
    await within(panel).findByRole('alert');
    // A setting the cards show changes: the view is read again.
    await act(() =>
      useSettingsStore
        .getState()
        .update({ catalogs: { switch: { enabled: true } } }),
    );
    await within(panel).findByRole('heading', {
      level: 3,
      name: /SteamGridDB/,
    });
    expect(within(panel).queryByText(/boom/)).not.toBeInTheDocument();
  });

  it('opening the tab sends nothing: no connection test', async () => {
    const test = vi.spyOn(backend, 'testIntegration');
    const legacy = vi.spyOn(backend, 'testSourceKey');
    await open();
    expect(test).not.toHaveBeenCalled();
    expect(legacy).not.toHaveBeenCalled();
  });

  it('lists the planned services with what they will need, and only «See the plan»', async () => {
    await open();
    const list = document.getElementById('integrations-planned')!;
    expect(list.tagName).toBe('UL');
    expect(list).toHaveAttribute('tabindex', '-1');
    const items = within(list).getAllByRole('listitem');
    expect(items).toHaveLength(PLANNED.length);
    expect(items.map((li) => li.querySelector('strong')?.textContent)).toEqual(
      PLANNED.map((p) => p.name),
    );
    expect(items[0]).toHaveTextContent('client ID and secret');
    expect(items[0]).toHaveTextContent('Art, metadata and external ids');
    expect(
      items.find((li) => li.textContent?.startsWith('Wikidata')),
    ).toHaveTextContent('no account');
    const section = list.closest('section')!;
    expect(section.querySelectorAll('input')).toHaveLength(0);
    expect(within(section).queryAllByRole('switch')).toHaveLength(0);
    const buttons = within(section).getAllByRole('button');
    expect(buttons.map((b) => b.textContent)).toEqual(['See the plan']);
    await userEvent.click(buttons[0]!);
    expect(backend.opened).toEqual(['link:roadmap']);
  });

  it('links to Network, Sources and Acknowledgements', async () => {
    const panel = await open();
    // Its own landmark name: the tab panel is already «Integrations».
    expect(
      within(panel).getByRole('navigation', { name: 'Related tabs' }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', { name: 'Proxy and limits in Network' }),
    );
    expect(useNavStore.getState().section).toBe('network');
    act(() => useNavStore.getState().openSettings('integrations'));
    await userEvent.click(
      await screen.findByRole('button', { name: 'Source order in Sources' }),
    );
    expect(useNavStore.getState().section).toBe('sources');
    act(() => useNavStore.getState().openSettings('integrations'));
    await userEvent.click(
      await screen.findByRole('button', {
        name: 'Credits in Acknowledgements',
      }),
    );
    expect(useNavStore.getState().section).toBe('credits');
  });

  describe('empty state', () => {
    it('says where to start, opens every card and lands on SteamGridDB', async () => {
      make({ integrations: 'empty' });
      await open();
      expect(
        screen.getByText(/To start, get a free SteamGridDB key/),
      ).toBeInTheDocument();
      expect(
        screen.queryByText(/ScreenScraper, with an account/),
      ).not.toBeInTheDocument();
      for (const name of [
        /SteamGridDB/,
        /ScreenScraper/,
        /Switch catalogue/,
        /Coverly updates/,
      ])
        await waitFor(() =>
          expect(within(card(name)).getAllByRole('button')[0]).toHaveAttribute(
            'aria-expanded',
            'true',
          ),
        );
      await userEvent.click(
        screen.getByRole('button', { name: 'Set up SteamGridDB' }),
      );
      await waitFor(() =>
        expect(
          document.getElementById('integration-steamgriddb'),
        ).toHaveFocus(),
      );
    });

    it('names ScreenScraper only in a build that can use it', async () => {
      make({ integrations: 'dev' });
      // The account is stored, so nothing is «empty»: forget it first.
      await backend.forgetIntegration('screenscraper');
      await open();
      expect(
        screen.getByText(/ScreenScraper, with an account/),
      ).toBeInTheDocument();
    });

    it('is not shown once a key is stored', async () => {
      make({ integrations: 'saved' });
      await open();
      expect(
        screen.queryByText(/To start, get a free SteamGridDB key/),
      ).not.toBeInTheDocument();
      expect(
        within(card(/SteamGridDB/)).getAllByRole('button')[0],
      ).toHaveAttribute('aria-expanded', 'false');
    });
  });

  it('a keychain that is not there: one alert for this OS, fields off, switches on', async () => {
    make({ keychain: 'unavailable', integrations: 'saved' });
    await open();
    const alerts = screen
      .getAllByRole('alert')
      .filter((a) =>
        /login keychain cannot be found/.test(a.textContent ?? ''),
      );
    expect(alerts).toHaveLength(1);
    expect(screen.queryByText(/To start, get a free/)).not.toBeInTheDocument();
    const sgdb = card(/SteamGridDB/);
    expect(within(sgdb).getByLabelText('API key')).toBeDisabled();
    expect(within(sgdb).getByRole('switch', { name: 'On' })).toBeEnabled();
  });

  it('a deep link opens the card and focuses its header', async () => {
    make({ integrations: 'saved' });
    render(<Root />);
    await screen.findByRole('button', { name: 'Settings' });
    act(() =>
      useNavStore
        .getState()
        .openSettings('integrations', { focus: 'integration-screenscraper' }),
    );
    const header = await screen.findByRole('button', {
      name: /^ScreenScraper, /,
    });
    await waitFor(() => expect(header).toHaveFocus());
    expect(header).toHaveAttribute('aria-expanded', 'true');
  });

  it('a deep link into the folded fallback opens the card and the fold', async () => {
    make({ integrations: 'saved' });
    // Off, the address is disabled and cannot take focus.
    await backend.updateSettings({ catalogs: { switch: { enabled: true } } });
    render(<Root />);
    await screen.findByRole('button', { name: 'Settings' });
    act(() =>
      useNavStore.getState().openSettings('integrations', {
        focus: 'setting-integrations-switch-catalog-fallback-url',
      }),
    );
    const url = await screen.findByRole('textbox', {
      name: 'Fallback address',
    });
    await waitFor(() => expect(url).toHaveFocus());
    expect(useNavStore.getState().openFolds).toEqual(
      expect.arrayContaining([
        'integration-switch-catalog',
        'integrations-switch-advanced',
      ]),
    );
  });

  it('gives SteamGridDB its place in the Sources order', async () => {
    make({ integrations: 'saved' });
    await open();
    const sgdb = card(/SteamGridDB/);
    await userEvent.click(within(sgdb).getAllByRole('button')[0]!);
    const order = within(sgdb).getByRole('button', {
      name: /^Sources: position \d of \d$/,
    });
    await userEvent.click(order);
    expect(useNavStore.getState().section).toBe('sources');
  });

  describe('updates card', () => {
    it('shows the channel and links to General and the releases', async () => {
      await open();
      const updates = card(/Coverly updates/);
      expect(within(updates).getByText('Channel: Stable')).toBeInTheDocument();
      expect(within(updates).queryByRole('switch')).not.toBeInTheDocument();
      expect(
        within(updates).queryByRole('button', {
          name: 'Credit in Acknowledgements',
        }),
      ).not.toBeInTheDocument();
      await userEvent.click(
        within(updates).getByRole('button', { name: 'See releases' }),
      );
      expect(backend.opened).toEqual(['link:releases']);
      await userEvent.click(
        within(updates).getByRole('button', { name: 'Change in General' }),
      );
      expect(useNavStore.getState().section).toBe('general');
      await waitFor(() =>
        expect(
          document.getElementById('setting-general-update-channel'),
        ).toHaveFocus(),
      );
    });

    it('its tag says whether Coverly checks on launch', async () => {
      await open();
      expect(
        within(card(/Coverly updates/)).getAllByRole('button')[0],
      ).toHaveAccessibleName('Coverly updates, On launch');
    });

    it('its tag says Manual once checking on launch is off', async () => {
      await backend.updateSettings({
        general: { check_updates_on_launch: false },
      });
      await open();
      expect(
        within(card(/Coverly updates/)).getAllByRole('button')[0],
      ).toHaveAccessibleName('Coverly updates, Manual');
    });
  });
});

/* The Switch catalogue moved here from Sources (D23): the same behaviour,
 * inside its card. */

const ON = { catalogs: { switch: { enabled: true } } } as const;

/** The catalogue's status line (a live region). */
const statusLine = () =>
  screen.findByTestId('switch-catalog-status', {}, { timeout: 2000 });
const catalogCard = () => card(/Switch catalogue/);
const catalogSwitch = () =>
  within(catalogCard()).getByRole('switch', { name: 'On' });
const iconsSwitch = () => screen.getByRole('switch', { name: 'Switch icons' });
const fallbackSwitch = () =>
  screen.getByRole('switch', { name: 'Online fallback' });
const openFold = async () =>
  userEvent.click(
    screen.getByRole('button', {
      name: `Advanced options (${foldCount('integrations-switch-advanced')})`,
    }),
  );

describe('integrations: Switch catalogue card', () => {
  it('off: says so, the tag is Off, and icons, fallback and the buttons wait for it', async () => {
    await open();
    await waitFor(async () =>
      expect(await statusLine()).toHaveTextContent('Off'),
    );
    expect(
      within(catalogCard()).getAllByRole('button')[0],
    ).toHaveAccessibleName('Switch catalogue, Off');
    expect(catalogSwitch()).not.toBeChecked();
    expect(catalogSwitch()).toHaveAttribute(
      'id',
      'setting-integrations-switch-catalog',
    );
    // The size it downloads is a number: data face; turning it off deletes it.
    const body = catalogCard();
    expect(within(body).getByText('90 MB').closest('.mono')).not.toBeNull();
    expect(
      within(body).getByText(/deletes the downloaded catalogue/),
    ).toBeInTheDocument();
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

  it('keeps the ids search lands on', async () => {
    await open();
    for (const id of [
      'setting-integrations-switch-catalog',
      'setting-integrations-switch-catalog-icons',
      'setting-integrations-switch-catalog-fallback',
      'setting-integrations-switch-catalog-fallback-url',
    ])
      expect(document.getElementById(id), id).not.toBeNull();
  });

  it('turning it on saves the setting, downloads the catalogue and the tag says Ready', async () => {
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
    await waitFor(() =>
      expect(
        within(catalogCard()).getAllByRole('button')[0],
      ).toHaveAccessibleName('Switch catalogue, Ready'),
    );
    expect(iconsSwitch()).toBeEnabled();
    expect(
      screen.getByRole('button', { name: 'Check for update' }),
    ).toBeEnabled();
  });

  it.each<[CatalogStatus, string, string]>([
    [{ state: 'pending' }, 'Catalogue download pending', 'Pending'],
    [
      { state: 'ready', count: 9_876, updated: '2026-10-01', fallback: true },
      '9,876 titles from the United States list, updated Oct 1, 2026',
      'Ready',
    ],
    [
      { state: 'failed-old', updated: '2026-09-14' },
      'Could not update; using the data from Sep 14, 2026.',
      'Error',
    ],
    [
      { state: 'rejected' },
      'The catalogue holds data Coverly does not accept; the previous one is kept.',
      'Error',
    ],
  ])('status %j', async (status, text, tag) => {
    await backend.updateSettings(ON);
    await open({ status });
    await waitFor(async () =>
      expect(await statusLine()).toHaveTextContent(text),
    );
    await waitFor(() =>
      expect(
        within(catalogCard()).getAllByRole('button')[0],
      ).toHaveAccessibleName(`Switch catalogue, ${tag}`),
    );
  });

  it('speaks Spanish, with the counts in the data face', async () => {
    await backend.updateSettings({ ...ON, general: { language: 'es' } });
    await open({
      status: {
        state: 'ready',
        count: 17_342,
        updated: '2026-10-30',
        fallback: false,
      },
      settingsButton: 'Configuración',
    });
    const line = await statusLine();
    await waitFor(() =>
      expect(line.textContent).toMatch(
        /^17\.342 títulos, actualizado el 30 oct 2026$/,
      ),
    );
    await userEvent.click(
      within(card(/Catálogo de Switch/)).getAllByRole('button')[0]!,
    );
    expect(
      within(card(/Catálogo de Switch/)).getByRole('switch', {
        name: 'Activado',
      }),
    ).toBeChecked();
  });

  it('«Delete catalogue» deletes it and shows what is left', async () => {
    await backend.updateSettings(ON);
    await backend.catalogUpdate();
    const remove = vi.spyOn(backend, 'catalogRemove');
    await open();
    await userEvent.click(within(catalogCard()).getAllByRole('button')[0]!);
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
  });

  it('icons and the fallback save once the catalogue is on', async () => {
    await backend.updateSettings(ON);
    await open();
    await userEvent.click(within(catalogCard()).getAllByRole('button')[0]!);
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
    await userEvent.click(within(catalogCard()).getAllByRole('button')[0]!);
    await openFold();
    const url = screen.getByRole('textbox', { name: 'Fallback address' });
    expect(url).toHaveValue('https://api.nlib.cc');
    expect(url).toHaveClass('font-mono');
    await userEvent.clear(url);
    await userEvent.type(url, 'http://api.nlib.cc');
    await userEvent.tab();
    await waitFor(() =>
      expect(url).toHaveAccessibleDescription(
        expect.stringContaining('It has to start with https://'),
      ),
    );
    expect(
      (await backend.getSettings()).settings.catalogs.switch.fallback_url,
    ).toBe('https://api.nlib.cc');
  });
});
