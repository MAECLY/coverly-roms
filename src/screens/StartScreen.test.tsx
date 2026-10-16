import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import { Root } from '@/Root';
import { useLocaleStore } from '@/stores/localeStore';
import { resetLibraryStore } from '@/stores/libraryStore';

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
  });
  afterEach(() => setBackend(null));

  it('lists recent libraries with their ROM counts', async () => {
    renderApp();
    const small = await screen.findByRole('button', { name: /^Small library/ });
    expect(within(small).getByText('240 ROMs')).toBeInTheDocument();
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

  it('edits the ignored folders', async () => {
    renderApp();
    await userEvent.click(
      screen.getByRole('button', { name: 'Library settings' }),
    );
    const box = await screen.findByLabelText('Ignored folders');
    await waitFor(() =>
      expect(box).toHaveValue('bios\nsaves\nstates\nShortcuts'),
    );
    await userEvent.clear(box);
    await userEvent.type(box, 'bios{enter}Saves{enter}bios');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument(),
    );
    expect((await backend.librarySettings()).ignored_dirs).toEqual([
      'bios',
      'Saves',
    ]);
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

  it('settings_dialog_traps_and_restores_focus', async () => {
    renderApp();
    const trigger = screen.getByRole('button', { name: 'Library settings' });
    await userEvent.click(trigger);
    const dialog = await screen.findByRole('dialog');
    const box = await screen.findByLabelText('Ignored folders');
    await waitFor(() => expect(box).not.toBeDisabled());
    await waitFor(() =>
      expect(dialog).toContainElement(document.activeElement as HTMLElement),
    );
    for (let i = 0; i < 6; i += 1) {
      await userEvent.tab();
      expect(dialog).toContainElement(document.activeElement as HTMLElement);
    }
    await userEvent.tab({ shift: true });
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
    await userEvent.keyboard('{Escape}');
    await waitFor(() =>
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument(),
    );
    expect(trigger).toHaveFocus();
  });

  it('switches the language', async () => {
    render(<Root />);
    await userEvent.selectOptions(screen.getByLabelText('Language'), 'es');
    expect(
      screen.getByRole('button', { name: 'Abrir carpeta…' }),
    ).toBeInTheDocument();
    useLocaleStore.getState().setChoice('auto');
  });
});
