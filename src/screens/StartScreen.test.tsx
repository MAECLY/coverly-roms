import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
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

  it('switches the language', async () => {
    render(<Root />);
    const select = screen.getByLabelText('Language');
    await waitFor(() => expect(select).not.toBeDisabled());
    await userEvent.selectOptions(select, 'es');
    expect(
      await screen.findByRole('button', { name: 'Abrir carpeta…' }),
    ).toBeInTheDocument();
    expect((await backend.getSettings()).settings.general.language).toBe('es');
  });
});
