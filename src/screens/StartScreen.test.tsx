import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_DOWNLOADS,
  type MockBackend,
} from '@/ipc/mock/mockBackend';
import { Root } from '@/Root';
import { resetLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { resetSettingsStore } from '@/stores/settingsStore';

let backend: MockBackend;

function renderApp(locale: 'en' | 'es' = 'en') {
  return render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
}

describe('Start screen', () => {
  beforeEach(() => {
    backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    setBackend(backend);
    resetLibraryStore();
    resetSettingsStore();
    useNavStore.getState().closeSettings();
  });
  afterEach(() => {
    useNavStore.getState().closeSettings();
    useNavStore.getState().closeOrganize();
    setBackend(null);
  });

  it('lists recent libraries with their ROM counts', async () => {
    renderApp();
    const small = await screen.findByRole('button', { name: /^Small library/ });
    expect(within(small).getByText('240 ROMs')).toBeInTheDocument();
  });

  it('formats recent ROM counts for the locale', async () => {
    renderApp('es');
    const fixture = await screen.findByRole('button', {
      name: /^Fixture SD \(12k\)/,
    });
    expect(within(fixture).getByText('12.000 ROMs')).toBeInTheDocument();
  });

  it('ejected_recent_is_disabled', async () => {
    renderApp();
    const ejected = await screen.findByRole('button', { name: /^EJECTED/ });
    expect(ejected).toBeDisabled();
    expect(within(ejected).getByText('Not connected')).toBeInTheDocument();
  });

  it('opens a recent library', async () => {
    renderApp();
    await userEvent.click(
      await screen.findByRole('button', { name: /^Small library/ }),
    );
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Small library' }),
    ).toBeInTheDocument();
  });

  it('opens the folder picked in the dialog', async () => {
    renderApp();
    await userEvent.click(screen.getByRole('button', { name: 'Open folder…' }));
    expect(
      await screen.findByRole('heading', {
        level: 1,
        name: 'Fixture SD (12k)',
      }),
    ).toBeInTheDocument();
  });

  it('opens a dropped folder', async () => {
    renderApp();
    await screen.findByRole('button', { name: /^Small library/ });
    const drop = new Event('drop', { cancelable: true });
    Object.defineProperty(drop, 'dataTransfer', {
      value: { files: [], getData: () => 'Small library' },
    });
    window.dispatchEvent(drop);
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Small library' }),
    ).toBeInTheDocument();
  });

  it('shows_a_translated_error_when_opening_fails', async () => {
    backend.pickFolder = () => Promise.resolve('/mock/Broken');
    renderApp('es');
    await userEvent.click(
      screen.getByRole('button', { name: 'Abrir carpeta…' }),
    );
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Esta carpeta no existe: /mock/Broken',
    );
  });

  it('forgets a recent library', async () => {
    renderApp();
    await userEvent.click(
      await screen.findByRole('button', { name: 'Forget Empty library' }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole('button', { name: /^Empty library/ }),
      ).not.toBeInTheDocument(),
    );
  });

  it('names the icon-only forget button in a tooltip on keyboard focus', async () => {
    renderApp();
    const forget = await screen.findByRole('button', {
      name: 'Forget Empty library',
    });
    act(() => forget.focus());
    expect(await screen.findByRole('tooltip')).toHaveTextContent(
      'Forget Empty library',
    );
  });

  it('opens the Settings screen', async () => {
    renderApp();
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(
      screen.getByRole('tabpanel', { name: 'General' }),
    ).toBeInTheDocument();
  });

  it('reports_a_failing_folder_picker', async () => {
    backend.pickFolder = () => Promise.reject(new Error('dialog denied'));
    renderApp();
    await userEvent.click(screen.getByRole('button', { name: 'Open folder…' }));
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Open folder…' }),
    ).not.toBeDisabled();
  });

  it('lists connected drives and opens one', async () => {
    render(<Root />);
    const section = await screen.findByRole('region', {
      name: 'Connected drives',
    });
    await userEvent.click(
      await within(section).findByRole('button', {
        name: 'Open Small library',
      }),
    );
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Small library' }),
    ).toBeInTheDocument();
  });

  it('says so when no drive is connected', async () => {
    backend.mountedVolumes = () => Promise.resolve([]);
    renderApp('es');
    const section = await screen.findByRole('region', {
      name: 'Unidades conectadas',
    });
    expect(
      await within(section).findByText(
        'No se encontraron unidades ni tarjetas SD.',
      ),
    ).toBeInTheDocument();
  });

  it('reloads the drives when the window regains focus', async () => {
    let calls = 0;
    backend.mountedVolumes = () => {
      calls += 1;
      return Promise.resolve(
        calls === 1 ? [] : [{ path: '/mock/Small library', name: 'Card' }],
      );
    };
    renderApp();
    const section = await screen.findByRole('region', {
      name: 'Connected drives',
    });
    await within(section).findByText('No drives or SD cards found.');
    window.dispatchEvent(new Event('focus'));
    expect(
      await within(section).findByRole('button', { name: 'Open Card' }),
    ).toBeInTheDocument();
  });

  it('offers to organise a plain pile on no-profile', async () => {
    backend.pickFolder = () => Promise.resolve(MOCK_DOWNLOADS);
    const probe = vi.spyOn(backend, 'probeFolder');
    renderApp();
    await userEvent.click(screen.getByRole('button', { name: 'Open folder…' }));
    expect(
      await screen.findByRole('heading', {
        name: "This folder doesn't follow a known layout",
      }),
    ).toBeInTheDocument();
    expect(probe).toHaveBeenCalledWith(MOCK_DOWNLOADS);
    expect(probe).toHaveBeenCalledTimes(1);
    expect(
      screen.getByText(/It holds 20 files that look like ROMs/),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Organise this folder…' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Choose another folder' }),
    ).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('keeps the error text when nothing looks like a ROM', async () => {
    backend.pickFolder = () => Promise.resolve(MOCK_DOWNLOADS);
    backend.probeFolder = () =>
      Promise.resolve({
        rom_like: 0,
        partial_downloads: 0,
        cloud_only: 0,
        sorted_folders: [],
        writable: true,
        broad_root: false,
        truncated: false,
      });
    renderApp('es');
    await userEvent.click(
      screen.getByRole('button', { name: 'Abrir carpeta…' }),
    );
    expect(await screen.findByRole('alert')).toHaveTextContent(MOCK_DOWNLOADS);
    expect(
      screen.queryByRole('button', { name: 'Organizar esta carpeta…' }),
    ).not.toBeInTheDocument();
  });

  it('keeps the error text when the probe fails', async () => {
    backend.pickFolder = () => Promise.resolve(MOCK_DOWNLOADS);
    backend.probeFolder = () => Promise.reject(new Error('denied'));
    renderApp();
    expect(
      await screen.findByRole('button', { name: 'Open folder…' }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Open folder…' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'No known layout was found',
    );
  });

  it('opens a folder sorted by system with the selected layout', async () => {
    backend.pickFolder = () => Promise.resolve(MOCK_DOWNLOADS);
    backend.probeFolder = () =>
      Promise.resolve({
        rom_like: 0,
        partial_downloads: 0,
        cloud_only: 0,
        sorted_folders: ['GB', 'GBA'],
        writable: true,
        broad_root: false,
        truncated: false,
      });
    const openLibrary = vi.spyOn(backend, 'openLibrary');
    renderApp();
    await userEvent.click(screen.getByRole('button', { name: 'Open folder…' }));
    const button = await screen.findByRole('button', {
      name: 'Open as organised by system',
    });
    await screen.findByRole('option', { name: 'muOS' });
    await userEvent.selectOptions(screen.getByLabelText('Layout'), 'muos');
    await userEvent.click(button);
    await waitFor(() =>
      expect(openLibrary).toHaveBeenLastCalledWith(MOCK_DOWNLOADS, 'muos'),
    );
  });

  it('organise button opens the organise screen with the source', async () => {
    backend.pickFolder = () => Promise.resolve(MOCK_DOWNLOADS);
    renderApp();
    await userEvent.click(screen.getByRole('button', { name: 'Open folder…' }));
    await userEvent.click(
      await screen.findByRole('button', { name: 'Organise this folder…' }),
    );
    expect(
      await screen.findByRole('heading', {
        level: 1,
        name: 'Organise by system',
      }),
    ).toBeInTheDocument();
    expect(useNavStore.getState().organizeSource).toBe(MOCK_DOWNLOADS);
  });

  it('choose another folder opens the picker again', async () => {
    const picks = [MOCK_DOWNLOADS, '/mock/Small library'];
    backend.pickFolder = () => Promise.resolve(picks.shift() ?? null);
    renderApp();
    await userEvent.click(screen.getByRole('button', { name: 'Open folder…' }));
    await userEvent.click(
      await screen.findByRole('button', { name: 'Choose another folder' }),
    );
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Small library' }),
    ).toBeInTheDocument();
  });

  it('switches the language', async () => {
    render(<Root />);
    const select = screen.getByRole('combobox', { name: 'Language' });
    await waitFor(() => expect(select).not.toBeDisabled());
    expect(select).toHaveTextContent('System language');
    await userEvent.click(select);
    await userEvent.click(screen.getByRole('option', { name: 'Español' }));
    expect(
      await screen.findByRole('combobox', { name: 'Idioma' }),
    ).toHaveTextContent('Español');
    expect(
      await screen.findByRole('button', { name: 'Abrir carpeta…' }),
    ).toBeInTheDocument();
    expect((await backend.getSettings()).settings.general.language).toBe('es');
  });
});
