import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Root } from '@/Root';
import { setBackend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import { resetLibraryStore } from '@/stores/libraryStore';
import { useNavStore, type SettingsSection } from '@/stores/navStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

let backend: MockBackend;
let restore: () => void;

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
    useNavStore.getState().closeSettings();
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
