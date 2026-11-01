import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Root } from '@/Root';
import { translate } from '@/i18n/core';
import { setBackend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import { DEFAULT_REGION_ORDER } from '@/lib/fill';
import { resetLibraryStore } from '@/stores/libraryStore';
import {
  resetNavStore,
  useNavStore,
  type SettingsSection,
} from '@/stores/navStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

import { settingId } from './Field';
import { foldCount, type SettingsFold } from './registry';

let backend: MockBackend;
let restore: () => void;

/** The fold trigger's name: its count follows the registry. */
const advanced = (fold: SettingsFold) =>
  `Advanced options (${foldCount(fold)})`;

async function open(section: SettingsSection) {
  render(<Root />);
  await screen.findByRole('button', { name: 'Settings' });
  act(() => useNavStore.getState().openSettings(section));
  return screen.findByRole('tabpanel');
}

const saved = async () => (await backend.getSettings()).settings;

describe('settings sections', () => {
  beforeEach(() => {
    backend = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: new MemoryStorage(),
    });
    setBackend(backend);
    resetSettingsStore();
    resetLibraryStore();
    // A previous test leaves the settings screen open.
    resetNavStore();
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    vi.restoreAllMocks();
    setBackend(null);
  });

  it('about: the no-telemetry line names the update server too', async () => {
    const panel = await open('about');
    expect(
      within(panel).getByText(
        'No telemetry: Coverly ROMs only contacts the art sources you enable and, unless you turn it off, the update server.',
      ),
    ).toBeInTheDocument();
  });

  it('art: media, single slot, format, aspect, quality, overwrite and a per-profile size', async () => {
    await open('art');
    await userEvent.click(
      screen.getByRole('checkbox', { name: 'Title screen' }),
    );
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Image for single-slot layouts' }),
      'screenshot',
    );
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Output format' }),
      'jpg',
    );
    await userEvent.click(
      screen.getByRole('switch', { name: 'Keep the aspect ratio' }),
    );
    const quality = screen.getByRole('textbox', { name: 'JPEG quality' });
    await userEvent.clear(quality);
    await userEvent.type(quality, '75{enter}');
    await userEvent.click(
      screen.getByRole('radio', { name: 'Ask every time' }),
    );
    const width = screen.getByRole('textbox', {
      name: 'Onion OS: maximum width',
    });
    const height = screen.getByRole('textbox', {
      name: 'Onion OS: maximum height',
    });
    await userEvent.type(width, '300');
    await userEvent.type(height, '420{enter}');
    const s = await saved();
    expect(s.art).toMatchObject({
      media: ['cover', 'screenshot'],
      single_slot_media: 'screenshot',
      output_format: 'jpg',
      keep_aspect: false,
      jpeg_quality: 75,
      overwrite: 'ask',
    });
    expect(s.art.max_size['onion']).toEqual({ width: 300, height: 420 });
    // Emptying both removes the override (a `null` in the merge patch).
    await userEvent.clear(width);
    await userEvent.clear(height);
    await userEvent.tab();
    expect((await saved()).art.max_size['onion']).toBeUndefined();
  });

  it('art: a size that is not a number is refused and a reset empties the row', async () => {
    await open('art');
    const width = screen.getByRole('textbox', {
      name: 'Onion OS: maximum width',
    });
    const height = screen.getByRole('textbox', {
      name: 'Onion OS: maximum height',
    });
    await userEvent.type(width, 'abc');
    await userEvent.type(height, '420{enter}');
    expect(
      await screen.findByText('This value is not valid.'),
    ).toBeInTheDocument();
    expect((await saved()).art.max_size['onion']).toBeUndefined();
    await userEvent.clear(width);
    await userEvent.type(width, '300{enter}');
    expect((await saved()).art.max_size['onion']).toEqual({
      width: 300,
      height: 420,
    });
    expect(
      screen.queryByText('This value is not valid.'),
    ).not.toBeInTheDocument();
    await act(() => useSettingsStore.getState().reset());
    expect(width).toHaveValue('');
    expect(height).toHaveValue('');
  });

  it('matching: strategy, region order, languages, thresholds and toggles', async () => {
    await open('matching');
    await userEvent.click(
      screen.getByRole('radio', { name: 'Source priority' }),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Move USA up' }));
    const langs = screen.getByRole('textbox', { name: 'Preferred languages' });
    await userEvent.clear(langs);
    await userEvent.type(langs, 'es, en');
    await userEvent.tab();
    const auto = screen.getByRole('textbox', {
      name: 'Auto-accept threshold (%)',
    });
    await userEvent.clear(auto);
    await userEvent.type(auto, '95{enter}');
    await userEvent.click(
      screen.getByRole('switch', { name: 'Fuzzy name matching' }),
    );
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'How ROMs are identified' }),
      'name-only',
    );
    const s = await saved();
    expect(s.matching).toMatchObject({
      strategy: 'priority',
      regions: ['rom', 'usa', 'world', 'europe', 'japan'],
      languages: ['es', 'en'],
      auto_threshold: 0.95,
      fuzzy: false,
      match_method: 'name-only',
    });
  });

  it('matching: announces a move, removes a region and adds it back from the list', async () => {
    await open('matching');
    await userEvent.click(screen.getByRole('button', { name: 'Move USA up' }));
    expect(screen.getByText('USA: position 2 of 5')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Remove Japan' }));
    await waitFor(async () =>
      expect((await saved()).matching.regions).not.toContain('japan'),
    );
    await userEvent.click(screen.getByRole('combobox', { name: 'Add region' }));
    await userEvent.click(await screen.findByRole('option', { name: 'Japan' }));
    await waitFor(async () =>
      expect((await saved()).matching.regions.at(-1)).toBe('japan'),
    );
    // The picker goes back to its placeholder for the next region.
    expect(
      screen.getByRole('combobox', { name: 'Add region' }),
    ).toHaveTextContent('Add region');
  });

  it('matching: hints and errors are linked to their field', async () => {
    await open('matching');
    const langs = screen.getByRole('textbox', { name: 'Preferred languages' });
    expect(langs).toHaveAccessibleDescription(
      'Two-letter codes, comma-separated (en, es, fr…). Used to break ties.',
    );
    const review = screen.getByRole('textbox', {
      name: 'Review threshold (%)',
    });
    expect(review).not.toHaveAttribute('aria-invalid');
    await userEvent.clear(review);
    await userEvent.type(review, 'abc{enter}');
    expect(review).toHaveAttribute('aria-invalid', 'true');
    expect(review).toHaveAccessibleDescription('This value is not valid.');
  });

  it('matching: a review threshold above the auto one is an error next to the field', async () => {
    await open('matching');
    const review = screen.getByRole('textbox', {
      name: 'Review threshold (%)',
    });
    await userEvent.clear(review);
    await userEvent.type(review, '99{enter}');
    expect(
      await screen.findByText(
        'This must not be higher than the auto-accept threshold.',
      ),
    ).toBeInTheDocument();
    expect(review).toHaveValue('99');
    expect((await saved()).matching.review_threshold).toBe(0.65);
  });

  it('sources: SteamGridDB turns on once a key is stored; the key never comes back', async () => {
    const panel = await open('sources');
    const sgdb = within(panel).getByRole('switch', { name: /SteamGridDB/ });
    expect(sgdb).toBeDisabled();
    expect(
      within(panel).getByText('Store an API key below to turn it on'),
    ).toBeInTheDocument();
    const key = within(panel).getByLabelText('SteamGridDB API key');
    expect(key).toHaveAttribute('type', 'password');
    await userEvent.type(key, 'sk-SECRET');
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Save SteamGridDB API key' }),
    );
    expect(
      await within(panel).findByText('Saved in the system keychain.'),
    ).toBeInTheDocument();
    expect(key).toHaveValue('');
    expect(sgdb).toBeEnabled();
    expect(
      within(panel).queryByText('Store an API key below to turn it on'),
    ).not.toBeInTheDocument();
    await userEvent.click(sgdb);
    expect(
      (await saved()).sources.order.find((e) => e.id === 'steamgriddb')!
        .enabled,
    ).toBe(true);
    expect(document.body.innerHTML).not.toContain('sk-SECRET');
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Remove SteamGridDB API key' }),
    );
    expect((await backend.credentialStatus()).set['steamgriddb-api-key']).toBe(
      false,
    );
  });

  it('sources: the typed key can be shown, and is hidden again once saved', async () => {
    const panel = await open('sources');
    const key = within(panel).getByLabelText('SteamGridDB API key');
    const show = within(panel).getByRole('button', {
      name: 'Show SteamGridDB API key',
    });
    expect(show).toBeDisabled();
    await userEvent.type(key, 'sk-PEEK');
    await userEvent.click(show);
    expect(show).toHaveAttribute('aria-pressed', 'true');
    expect(key).toHaveAttribute('type', 'text');
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Save SteamGridDB API key' }),
    );
    await within(panel).findByText('Saved in the system keychain.');
    expect(key).toHaveAttribute('type', 'password');
    expect(show).toHaveAttribute('aria-pressed', 'false');
    expect(document.body.innerHTML).not.toContain('sk-PEEK');
  });

  it('sources: screenscraper says not available in this build', async () => {
    const panel = await open('sources');
    expect(
      within(panel).getByRole('switch', { name: /ScreenScraper/ }),
    ).toBeDisabled();
    expect(
      within(panel).getByText('Not available in this build'),
    ).toBeInTheDocument();
    expect(
      within(panel).queryByRole('button', {
        name: 'Test the ScreenScraper password',
      }),
    ).not.toBeInTheDocument();
  });

  it('sources: Test reports ok, invalid and missing without showing the key', async () => {
    const panel = await open('sources');
    const test = within(panel).getByRole('button', {
      name: 'Test the SteamGridDB API key',
    });
    expect(test).toBeDisabled();
    const key = within(panel).getByLabelText('SteamGridDB API key');
    await userEvent.type(key, 'bad-key');
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Save SteamGridDB API key' }),
    );
    await within(panel).findByText('Saved in the system keychain.');
    await userEvent.click(test);
    expect(await within(panel).findByRole('status')).toHaveTextContent(
      'The key was refused.',
    );
    await userEvent.type(key, 'sk-good');
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Save SteamGridDB API key' }),
    );
    // A new key forgets the previous result.
    expect(within(panel).queryByRole('status')).not.toBeInTheDocument();
    await userEvent.click(test);
    expect(await within(panel).findByRole('status')).toHaveTextContent(
      'The key works.',
    );
    expect(document.body.innerHTML).not.toContain('sk-good');
    expect(document.body.innerHTML).not.toContain('bad-key');
  });

  it('sources: Test says the key is missing when it was removed elsewhere', async () => {
    await backend.setCredential('steamgriddb-api-key', 'sk');
    const panel = await open('sources');
    const test = within(panel).getByRole('button', {
      name: 'Test the SteamGridDB API key',
    });
    await waitFor(() => expect(test).toBeEnabled());
    await backend.clearCredential('steamgriddb-api-key');
    await userEvent.click(test);
    expect(await within(panel).findByRole('status')).toHaveTextContent(
      'No key is stored.',
    );
  });

  it('sources: an enabled source without its key can still be turned off', async () => {
    await backend.setCredential('steamgriddb-api-key', 'sk');
    await backend.updateSettings({
      sources: {
        order: [
          { id: 'libretro', enabled: true },
          { id: 'steamgriddb', enabled: true },
          { id: 'screenscraper', enabled: false },
        ],
      },
    });
    await backend.clearCredential('steamgriddb-api-key');
    const panel = await open('sources');
    const sgdb = within(panel).getByRole('switch', { name: /SteamGridDB/ });
    expect(sgdb).toBeChecked();
    expect(sgdb).toBeEnabled();
    await userEvent.click(sgdb);
    expect(
      (await saved()).sources.order.find((e) => e.id === 'steamgriddb')!
        .enabled,
    ).toBe(false);
    // Once off, it cannot be turned on again without a key.
    await waitFor(() => expect(sgdb).toBeDisabled());
  });

  it('sources: the image from the game files says what it is, needs no key and can be moved', async () => {
    await backend.updateSettings({ general: { language: 'es' } });
    render(<Root />);
    await screen.findByRole('button', { name: 'Configuración' });
    act(() => useNavStore.getState().openSettings('sources'));
    const panel = await screen.findByRole('tabpanel');
    const row = await within(panel).findByRole('switch', {
      name: 'Imagen incluida en el juego',
    });
    await waitFor(() =>
      expect(row).toHaveAccessibleDescription(
        'Usa el icono que traen algunos juegos (PS3, PS Vita, Wii U, 3DS, Xbox 360) cuando no hay carátula en otra fuente',
      ),
    );
    expect(row).toBeEnabled();
    // No key field: only SteamGridDB and ScreenScraper have credentials.
    expect(
      within(panel).queryByLabelText(/Imagen incluida en el juego/, {
        selector: 'input:not([role="switch"])',
      }),
    ).toBeNull();
    const order = (await saved()).sources.order.map((e) => e.id);
    const at = order.indexOf('embedded');
    expect(at).toBeGreaterThan(0);
    await userEvent.click(
      within(panel).getByRole('button', {
        name: 'Subir Imagen incluida en el juego',
      }),
    );
    await waitFor(async () =>
      expect((await saved()).sources.order[at - 1]!.id).toBe('embedded'),
    );
    expect(
      within(panel).getByText(
        `Imagen incluida en el juego: posición ${at} de ${order.length}`,
      ),
    ).toBeInTheDocument();
  });

  it('sources: speaks Spanish', async () => {
    await backend.updateSettings({ general: { language: 'es' } });
    render(<Root />);
    await screen.findByRole('button', { name: 'Configuración' });
    act(() => useNavStore.getState().openSettings('sources'));
    const panel = await screen.findByRole('tabpanel');
    expect(
      await within(panel).findByText('No disponible en esta versión'),
    ).toBeInTheDocument();
    expect(
      within(panel).getByRole('button', {
        name: 'Probar Clave de API de SteamGridDB',
      }),
    ).toBeInTheDocument();
  });

  it('network: invalid timeout shows an error, keeps the typed value and other fields still save', async () => {
    await open('network');
    const timeout = screen.getByRole('textbox', { name: 'Timeout (seconds)' });
    await userEvent.clear(timeout);
    await userEvent.type(timeout, '1{enter}');
    expect(
      await screen.findByText('Use a value from 5 to 300.'),
    ).toBeInTheDocument();
    expect(timeout).toHaveValue('1');
    const retries = screen.getByRole('textbox', { name: 'Retries' });
    await userEvent.clear(retries);
    await userEvent.type(retries, '5{enter}');
    expect((await saved()).network).toMatchObject({
      retries: 5,
      timeout_secs: 30,
    });
    await userEvent.click(screen.getByRole('radio', { name: 'Manual' }));
    expect(
      await screen.findByText('This cannot be empty.'),
    ).toBeInTheDocument();
    const url = screen.getByRole('textbox', { name: 'Proxy URL' });
    await userEvent.type(url, 'http://proxy.local:3128');
    await userEvent.tab();
    await userEvent.click(screen.getByRole('radio', { name: 'Manual' }));
    expect((await saved()).network).toMatchObject({
      proxy: 'manual',
      proxy_url: 'http://proxy.local:3128',
    });
  });

  it('network: a Manual proxy refused for its empty URL is applied once a URL is typed', async () => {
    await open('network');
    await userEvent.click(screen.getByRole('radio', { name: 'Manual' }));
    expect(
      await screen.findByText('This cannot be empty.'),
    ).toBeInTheDocument();
    const url = screen.getByRole('textbox', { name: 'Proxy URL' });
    await userEvent.type(url, 'http://proxy.local:3128');
    await userEvent.tab();
    expect((await saved()).network).toMatchObject({
      proxy: 'manual',
      proxy_url: 'http://proxy.local:3128',
    });
    expect(screen.getByRole('radio', { name: 'Manual' })).toBeChecked();
  });

  it('network: a URL typed with another proxy choice keeps that choice', async () => {
    await open('network');
    const url = screen.getByRole('textbox', { name: 'Proxy URL' });
    await userEvent.type(url, 'http://proxy.local:3128');
    await userEvent.tab();
    expect((await saved()).network).toMatchObject({
      proxy: 'system',
      proxy_url: 'http://proxy.local:3128',
    });
  });

  it('network: clears the cache after a confirmation', async () => {
    await open('network');
    expect(await screen.findByText(/In use: 48/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Clear cache…' }));
    await userEvent.click(
      within(screen.getByRole('alertdialog')).getByRole('button', {
        name: 'Clear cache',
      }),
    );
    expect(await screen.findByText(/Freed 48/)).toBeInTheDocument();
    expect(await backend.cacheUsage()).toBe(0);
  });

  it('organize: the organise defaults show and save', async () => {
    const group = await open('organize');
    const layout = within(group).getByRole('combobox', {
      name: 'Default layout',
    });
    await waitFor(() =>
      expect(
        within(layout).getByRole('option', { name: 'ES-DE' }),
      ).toBeInTheDocument(),
    );
    expect(layout).toHaveValue('anbernic-stock');
    const nest = within(group).getByRole('switch', {
      name: "Put the system folders inside the layout's ROMs folder",
    });
    expect(nest).toBeChecked();
    const mode = within(group).getByRole('combobox', { name: 'Mode' });
    expect(mode).toHaveValue('auto');
    await userEvent.selectOptions(layout, 'esde');
    await userEvent.click(nest);
    await userEvent.selectOptions(mode, 'copy');
    await userEvent.selectOptions(
      within(group).getByRole('combobox', { name: 'Names' }),
      'catalogue',
    );
    await userEvent.click(
      within(group).getByRole('switch', {
        name: 'Download game catalogues to identify ROMs by their contents',
      }),
    );
    await userEvent.selectOptions(
      within(group).getByRole('combobox', { name: 'When organising' }),
      'report',
    );
    await waitFor(async () =>
      expect((await saved()).organize).toMatchObject({
        default_layout: 'esde',
        nest_in_roms_root: false,
        mode: 'copy',
        names: 'catalogue',
        download_dats: false,
      }),
    );
    await waitFor(async () =>
      expect((await saved()).dedupe.organize_step).toBe('report'),
    );
  });

  it('organize: the common options show in order, the RAR programs folded', async () => {
    const group = await open('organize');
    // Hidden fold content is out of the accessibility tree.
    const controls = [
      ...within(group).getAllByRole('combobox'),
      ...within(group).getAllByRole('switch'),
    ]
      .sort((a, b) =>
        a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING
          ? -1
          : 1,
      )
      .map((c) => c.id);
    expect(controls).toEqual([
      settingId('organize.mode'),
      settingId('organize.default_layout'),
      settingId('organize.names'),
      settingId('organize.nest_in_roms_root'),
      settingId('organize.download_dats'),
      settingId('dedupe.organize_step'),
    ]);
    const hint = within(group).getByText(
      'Thresholds and lists are edited in Advanced (settings file).',
    );
    expect(hint).not.toBeVisible();
    const fold = within(group).getByRole('button', {
      name: advanced('organize-advanced'),
    });
    expect(fold).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(fold);
    expect(fold).toHaveAttribute('aria-expanded', 'true');
    expect(hint).toBeVisible();
    expect(
      within(group).getByRole('switch', { name: 'External programs for RAR' }),
    ).toBeVisible();
  });

  it('library: shows no organise control', async () => {
    const panel = await open('library');
    expect(
      within(panel).queryByRole('combobox', { name: 'Mode' }),
    ).not.toBeInTheDocument();
    expect(
      within(panel).queryByRole('combobox', { name: 'When organising' }),
    ).not.toBeInTheDocument();
  });

  it('duplicates: the duplicates tab shows and saves', async () => {
    const panel = await open('duplicates');
    await userEvent.click(
      within(panel).getByRole('switch', { name: 'One ROM per game' }),
    );
    await userEvent.click(
      within(panel).getByRole('radio', { name: 'Send to the Trash' }),
    );
    expect(
      within(panel).getByRole('textbox', { name: 'Quarantine folder' }),
    ).toHaveAttribute('placeholder', '<library>/_Duplicados');
    await userEvent.click(
      within(panel).getByRole('button', {
        name: advanced('duplicates-advanced'),
      }),
    );
    const version = within(panel).getByRole('switch', {
      name: 'Other versions',
    });
    expect(version).toBeChecked();
    await userEvent.click(version);
    const exclude = within(panel).getByRole('textbox', {
      name: 'Systems to leave out',
    });
    await userEvent.clear(exclude);
    await userEvent.type(exclude, 'ports, msu');
    await userEvent.tab();
    await waitFor(async () =>
      expect((await saved()).dedupe).toMatchObject({
        levels: ['exact', 'format'],
        one_game_one_rom: true,
        action: 'trash',
        exclude_systems: ['ports', 'msu'],
      }),
    );
  });

  it('duplicates: the common options show, the rest folded', async () => {
    const panel = await open('duplicates');
    expect(
      within(panel).getByRole('radio', { name: 'Set aside in a folder' }),
    ).toBeVisible();
    expect(
      within(panel).getByRole('switch', { name: 'One ROM per game' }),
    ).toHaveAccessibleDescription('Keeps one version of each game.');
    // Hidden fold content is out of the accessibility tree.
    expect(
      within(panel).queryByRole('switch', { name: 'Other versions' }),
    ).not.toBeInTheDocument();
    expect(
      within(panel).queryByRole('textbox', { name: 'Systems to leave out' }),
    ).not.toBeInTheDocument();
    expect(
      within(panel).queryByRole('group', { name: 'Duplicates' }),
    ).not.toBeInTheDocument();
  });

  it('duplicates: choose sits beside the quarantine folder, before its help', async () => {
    const panel = await open('duplicates');
    const input = within(panel).getByRole('textbox', {
      name: 'Quarantine folder',
    });
    const choose = within(panel).getByRole('button', { name: 'Choose…' });
    const more = within(panel).getAllByRole('button', {
      name: 'More information',
    })[0]!;
    // Same row as its field (spec layout), not after the hint and link.
    expect(choose.parentElement).toBe(input.parentElement);
    expect(
      choose.compareDocumentPosition(more) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(input).toHaveAccessibleDescription(
      'Empty: _Duplicados inside the library, on its own drive.',
    );
  });

  it('duplicates: the fold shows what applies when', async () => {
    const panel = await open('duplicates');
    const note = within(panel).getByText(
      'The duplicates screen starts with these options; the ones below apply when you look again.',
    );
    expect(note).not.toBeVisible();
    await userEvent.click(
      within(panel).getByRole('button', {
        name: advanced('duplicates-advanced'),
      }),
    );
    expect(note).toBeVisible();
    expect(
      within(panel).getByRole('list', { name: 'Which copy stays' }),
    ).toHaveAttribute('id', 'setting-dedupe-keep-rules');
    const order = within(panel).getByRole('group', {
      name: 'Format order per profile',
    });
    expect(order).toHaveAttribute('id', 'setting-dedupe-container-order');
    expect(order).toBeVisible();
    expect(
      within(panel).getByText(
        'What happens when organising is set in Organise.',
      ),
    ).toBeVisible();
  });

  it('duplicates: shows the Matching region order and links to it', async () => {
    const panel = await open('duplicates');
    await userEvent.click(
      within(panel).getByRole('button', {
        name: advanced('duplicates-advanced'),
      }),
    );
    const names = DEFAULT_REGION_ORDER.filter((r) => r !== 'rom')
      .map((r) => translate('en', `regions.${r}`))
      .join(', ');
    expect(names).toBe('World, USA, Europe, Japan');
    expect(
      within(panel).getByText(`Regions, in order: ${names}`),
    ).toBeVisible();
    expect(
      within(panel).getByText('It also decides which art is chosen.'),
    ).toBeVisible();
    expect(
      within(panel).getByRole('switch', { name: 'Use: Preferred region' }),
    ).toHaveAccessibleDescription(
      `Regions, in order: ${names} It also decides which art is chosen.`,
    );
    expect(
      within(panel).queryByRole('button', {
        name: 'Uses the Matching region order',
      }),
    ).not.toBeInTheDocument();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Change in Matching' }),
    );
    expect(useNavStore.getState().section).toBe('matching');
    const list = await screen.findByRole('list', { name: 'Region order' });
    expect(list).toHaveAttribute('id', 'setting-matching-regions');
    await waitFor(() => expect(list).toHaveFocus());
    expect((await saved()).matching.regions).toEqual([...DEFAULT_REGION_ORDER]);
  });

  it('duplicates: only the ROM region says the rule decides nothing', async () => {
    const panel = await open('duplicates');
    await act(() =>
      useSettingsStore.getState().update({ matching: { regions: ['rom'] } }),
    );
    expect((await saved()).matching.regions).toEqual(['rom']);
    await userEvent.click(
      within(panel).getByRole('button', {
        name: advanced('duplicates-advanced'),
      }),
    );
    expect(
      within(panel).getByText(
        "Only the ROM's region: this rule decides nothing.",
      ),
    ).toBeVisible();
    expect(
      within(panel).getByRole('switch', { name: 'Use: Preferred region' }),
    ).toHaveAccessibleDescription(
      "Only the ROM's region: this rule decides nothing. It also decides which art is chosen.",
    );
    expect(
      within(panel).queryByText(/^Regions, in order:/),
    ).not.toBeInTheDocument();
  });

  it('duplicates: Go to Organise lands on When organising', async () => {
    const panel = await open('duplicates');
    await userEvent.click(
      within(panel).getByRole('button', {
        name: advanced('duplicates-advanced'),
      }),
    );
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Go to Organise' }),
    );
    expect(useNavStore.getState().section).toBe('organize');
    await waitFor(() =>
      expect(
        screen.getByRole('combobox', { name: 'When organising' }),
      ).toHaveFocus(),
    );
  });

  it('duplicates: an error inside the fold opens it, one outside does not', async () => {
    const panel = await open('duplicates');
    const fold = within(panel).getByRole('button', {
      name: advanced('duplicates-advanced'),
    });
    await act(() =>
      useSettingsStore.getState().update({ dedupe: { quarantine_dir: 'rel' } }),
    );
    expect(fold).toHaveAttribute('aria-expanded', 'false');
    await act(() =>
      useSettingsStore
        .getState()
        .update({ dedupe: { exclude_systems: ['ports/msu'] } }),
    );
    await waitFor(() => expect(fold).toHaveAttribute('aria-expanded', 'true'));
    expect(
      within(panel).getByRole('textbox', { name: 'Systems to leave out' }),
    ).toBeInvalid();
    expect((await saved()).dedupe.exclude_systems).not.toContain('ports/msu');
  });

  it('duplicates: the keeper rules and the format order read their short hint', async () => {
    const panel = await open('duplicates');
    await userEvent.click(
      within(panel).getByRole('button', {
        name: advanced('duplicates-advanced'),
      }),
    );
    expect(
      within(panel).getByRole('list', { name: 'Which copy stays' }),
    ).toHaveAccessibleDescription('Use the arrows to reorder them.');
    expect(
      within(panel).getByRole('group', { name: 'Format order per profile' }),
    ).toHaveAccessibleDescription('Comma-separated, preferred first.');
  });

  it.each([
    ['setting-dedupe-keep-rules', 'list', 'Which copy stays'],
    ['setting-dedupe-container-order', 'group', 'Format order per profile'],
  ] as const)(
    'duplicates: a link to %s opens the fold and lands there',
    async (focus, role, name) => {
      await open('library');
      act(() => useNavStore.getState().openSettings('duplicates', { focus }));
      const target = await screen.findByRole(role, { name });
      await waitFor(() => expect(target).toHaveFocus());
      expect(useNavStore.getState().openFolds).toContain('duplicates-advanced');
      expect(target).toHaveAttribute('data-arrived', 'true');
    },
  );

  it('library: shows no duplicates control', async () => {
    const panel = await open('library');
    expect(
      within(panel).queryByRole('switch', { name: 'One ROM per game' }),
    ).not.toBeInTheDocument();
    expect(
      within(panel).queryByRole('textbox', { name: 'Quarantine folder' }),
    ).not.toBeInTheDocument();
    expect(panel.querySelector('[name="dedupe-level-exact"]')).toBeNull();
  });

  it('library: the scan options show, the ignored lists folded, then the recents', async () => {
    const panel = await open('library');
    const fold = within(panel).getByRole('button', {
      name: advanced('library-advanced'),
    });
    expect(fold).toHaveAttribute('aria-expanded', 'false');
    expect(
      within(panel).queryByRole('textbox', { name: 'Ignored folders' }),
    ).not.toBeInTheDocument();
    expect(
      within(panel).queryByRole('textbox', { name: 'Ignored extensions' }),
    ).not.toBeInTheDocument();
    const order = [
      within(panel).getByRole('combobox', { name: 'Default layout profile' }),
      within(panel).getByRole('switch', { name: 'Include hidden files' }),
      within(panel).getByRole('switch', { name: 'Follow symbolic links' }),
      fold,
      within(panel).getByRole('heading', { name: 'Recent libraries' }),
    ];
    order.reduce((prev, next) => {
      expect(
        prev.compareDocumentPosition(next) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      return next;
    });
    await userEvent.click(fold);
    expect(
      within(panel).getByRole('textbox', { name: 'Ignored extensions' }),
    ).toHaveAccessibleDescription('Comma-separated, without dots.');
    const [, extsMore] = within(panel).getAllByRole('button', {
      name: 'More information',
    });
    if (!extsMore) throw new Error('no second More information toggle');
    await userEvent.click(extsMore);
    expect(
      within(panel).getByText(
        'Files with these extensions are never taken as ROMs.',
      ),
    ).toBeVisible();
  });

  it('library: the advanced fold opens on an ignored-folder error', async () => {
    const panel = await open('library');
    const fold = within(panel).getByRole('button', {
      name: advanced('library-advanced'),
    });
    expect(fold).toHaveAttribute('aria-expanded', 'false');
    act(() =>
      useSettingsStore.setState({
        errors: {
          'library.ignored_dirs': {
            field: 'library.ignored_dirs',
            code: 'invalid',
            min: null,
            max: null,
          },
        },
      }),
    );
    await waitFor(() => expect(fold).toHaveAttribute('aria-expanded', 'true'));
    expect(useNavStore.getState().openFolds).toContain('library-advanced');
    expect(
      within(panel).getByRole('textbox', { name: 'Ignored folders' }),
    ).toBeInvalid();
  });

  it('library: follow symlinks has a short hint and more information', async () => {
    const panel = await open('library');
    const sw = within(panel).getByRole('switch', {
      name: 'Follow symbolic links',
    });
    expect(sw).toHaveAccessibleDescription(
      'Linked folders are read as if they were inside.',
    );
    const [more] = within(panel).getAllByRole('button', {
      name: 'More information',
    });
    if (!more) throw new Error('no More information toggle');
    expect(more).toHaveAttribute('aria-expanded', 'false');
    const details =
      'Art is never written outside the library folder: ROMs in a linked folder that points elsewhere are listed, but their art is skipped with an error.';
    expect(within(panel).getByText(details)).not.toBeVisible();
    await userEvent.click(more);
    expect(more).toHaveAttribute('aria-expanded', 'true');
    expect(within(panel).getByText(details)).toBeVisible();
  });

  it('duplicates: settings_reorder_rules_announces', async () => {
    const panel = await open('duplicates');
    await userEvent.click(
      within(panel).getByRole('button', {
        name: advanced('duplicates-advanced'),
      }),
    );
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Move Has a cover up' }),
    );
    expect(
      within(panel).getByText('Has a cover: position 8 of 10'),
    ).toBeInTheDocument();
    await waitFor(async () =>
      expect(
        (await saved()).dedupe.keep_rules.map((r) => r.rule).slice(6, 9),
      ).toEqual(['revision', 'cover', 'container']),
    );
    await userEvent.click(
      within(panel).getByRole('switch', { name: 'Use: Has a cover' }),
    );
    await waitFor(async () =>
      expect(
        (await saved()).dedupe.keep_rules.find((r) => r.rule === 'cover')?.on,
      ).toBe(false),
    );
  });

  it('duplicates: settings_rejects_quarantine_inside_a_system_folder', async () => {
    const panel = await open('duplicates');
    const dir = within(panel).getByRole('textbox', {
      name: 'Quarantine folder',
    });
    await userEvent.type(dir, '/Volumes/SD/ROMS/GB/dup');
    await userEvent.tab();
    expect(dir).toHaveAccessibleDescription(/can't be inside/);
    expect((await saved()).dedupe.quarantine_dir).toBeNull();
    await userEvent.clear(dir);
    await userEvent.type(dir, '/Users/demo/Duplicados');
    await userEvent.tab();
    await waitFor(async () =>
      expect((await saved()).dedupe.quarantine_dir).toBe(
        '/Users/demo/Duplicados',
      ),
    );
  });

  it('organize: external_tools_toggle_saves_off', async () => {
    const group = await open('organize');
    await userEvent.click(
      within(group).getByRole('button', {
        name: advanced('organize-advanced'),
      }),
    );
    const tools = within(group).getByRole('switch', {
      name: 'External programs for RAR',
    });
    expect(tools).toBeChecked();
    await userEvent.click(tools);
    await waitFor(async () =>
      expect((await saved()).organize.external_tools).toBe('off'),
    );
    expect(tools).not.toBeChecked();
    await userEvent.click(tools);
    await waitFor(async () =>
      expect((await saved()).organize.external_tools).toBe('auto'),
    );
  });

  it('organize: found_tools_listed_in_mono', async () => {
    backend = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: new MemoryStorage(),
      externalTools: [
        { tool: 'seven-zip', version: '24.08', path: '/opt/homebrew/bin/7zz' },
        { tool: 'bsdtar', version: '3.5.3', path: '/usr/bin/bsdtar' },
      ],
    });
    setBackend(backend);
    const group = await open('organize');
    await userEvent.click(
      within(group).getByRole('button', {
        name: advanced('organize-advanced'),
      }),
    );
    const tools = within(group).getByRole('switch', {
      name: 'External programs for RAR',
    });
    expect(tools).toHaveAccessibleDescription(
      'Found: 7-Zip 24.08 (/opt/homebrew/bin/7zz), bsdtar 3.5.3 (/usr/bin/bsdtar)',
    );
    const list = within(group).getByText(
      '7-Zip 24.08 (/opt/homebrew/bin/7zz), bsdtar 3.5.3 (/usr/bin/bsdtar)',
    );
    expect(list).toHaveClass('font-mono');
    // Paths are long unbroken tokens (`C:\Program Files\…`):
    // they wrap anywhere instead of pushing the panel wider.
    expect(list).toHaveClass('[overflow-wrap:anywhere]');
    expect(
      within(group).queryByText('None. Install unrar or 7-Zip to open RAR.'),
    ).not.toBeInTheDocument();
  });

  it('organize: no_tools_shows_install_hint', async () => {
    const group = await open('organize');
    await userEvent.click(
      within(group).getByRole('button', {
        name: advanced('organize-advanced'),
      }),
    );
    expect(
      within(group).getByRole('switch', { name: 'External programs for RAR' }),
    ).toHaveAccessibleDescription('None. Install unrar or 7-Zip to open RAR.');
    expect(within(group).queryByText(/^Found:/)).not.toBeInTheDocument();
  });

  it('safety: backups are locked on; the rest saves', async () => {
    await open('safety');
    expect(
      screen.getByRole('switch', {
        name: 'Back up files before replacing them',
      }),
    ).toBeDisabled();
    await userEvent.click(
      screen.getByRole('switch', {
        name: 'Simulate before writing (dry run first)',
      }),
    );
    await userEvent.click(
      screen.getByRole('switch', {
        name: 'Delete macOS ._ files on removable drives',
      }),
    );
    const depth = screen.getByRole('textbox', { name: 'Jobs kept for undo' });
    await userEvent.clear(depth);
    await userEvent.type(depth, '20{enter}');
    const folder = screen.getByRole('textbox', { name: 'Backup folder' });
    await userEvent.type(folder, 'relative/path');
    await userEvent.tab();
    expect(
      await screen.findByText(
        'Use a full path (starting with / or a drive letter).',
      ),
    ).toBeInTheDocument();
    expect((await saved()).safety).toMatchObject({
      dry_run_default: false,
      clean_apple_double: false,
      undo_depth: 20,
      backup_dir: null,
    });
  });

  it('advanced: log level, folders and reset with confirmation', async () => {
    await open('advanced');
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Log level' }),
      'debug',
    );
    await userEvent.click(
      screen.getByRole('button', { name: 'Open logs folder' }),
    );
    await userEvent.click(
      screen.getByRole('button', { name: 'Open settings folder' }),
    );
    expect(backend.opened).toEqual(['folder:logs', 'folder:config']);
    expect((await saved()).advanced.log_level).toBe('debug');
    await userEvent.click(
      screen.getByRole('button', { name: 'Reset all settings…' }),
    );
    const dialog = screen.getByRole('alertdialog', { name: /reset/i });
    // A destructive confirmation starts on the safe action.
    expect(
      within(dialog).getByRole('button', { name: 'Cancel' }),
    ).toHaveFocus();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Cancel' }),
    );
    expect((await saved()).advanced.log_level).toBe('debug');
    await userEvent.click(
      screen.getByRole('button', { name: 'Reset all settings…' }),
    );
    await userEvent.click(
      within(screen.getByRole('alertdialog')).getByRole('button', {
        name: 'Reset settings',
      }),
    );
    expect((await saved()).advanced.log_level).toBe('info');
  });

  it('safety and advanced: folders that cannot be picked or opened show the error', async () => {
    vi.spyOn(backend, 'pickFolder').mockRejectedValue(
      new BackendError('io', 'picker broke'),
    );
    vi.spyOn(backend, 'openLogsFolder').mockRejectedValue(
      new BackendError('io', 'no opener'),
    );
    await open('safety');
    await userEvent.click(screen.getByRole('button', { name: 'Choose…' }));
    expect(await screen.findByText(/picker broke/)).toBeInTheDocument();
    expect((await saved()).safety.backup_dir).toBeNull();
    act(() => useNavStore.getState().setSection('advanced'));
    await userEvent.click(
      await screen.findByRole('button', { name: 'Open logs folder' }),
    );
    expect(await screen.findByText(/no opener/)).toBeInTheDocument();
  });
});
