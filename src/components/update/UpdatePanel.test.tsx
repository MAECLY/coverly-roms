import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Root } from '@/Root';
import { setBackend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import type { MockUpdateScenario } from '@/ipc/mock/mockUpdates';
import type { JobView } from '@/ipc/types';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { resetSettingsStore } from '@/stores/settingsStore';
import { resetUpdateStore } from '@/stores/updateStore';
import { MemoryStorage } from '@/test/storage';

let backend: MockBackend;
function start(update: MockUpdateScenario, checkOnStart = false) {
  const storage = new MemoryStorage();
  backend = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage,
    update,
  });
  setBackend(backend);
  return backend.updateSettings({
    general: { check_updates_on_launch: checkOnStart },
  });
}
async function about() {
  render(<Root />);
  await screen.findByRole('button', { name: 'Settings' });
  act(() => useNavStore.getState().openSettings('about'));
  return screen.findByRole('tabpanel');
}

describe('updates UI', () => {
  beforeEach(() => {
    resetUpdateStore();
    resetSettingsStore();
    resetLibraryStore();
    useJobStore.getState().reset();
    useNavStore.getState().closeSettings();
  });
  afterEach(() => setBackend(null));

  it('About: check → available → download progress → restart', async () => {
    await start('available');
    const panel = await about();
    expect(within(panel).getByText('Not checked yet.')).toBeInTheDocument();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Check for updates' }),
    );
    expect(
      await within(panel).findByText(/Version \d+\.\d+\.0 is available\./),
    ).toBeInTheDocument();
    expect(
      within(panel).getByText('Faster scans, two new layouts and fixes.'),
    ).toBeInTheDocument();
    const notes = within(panel).getByRole('button', { name: "What's new" });
    expect(notes).toHaveAttribute('aria-expanded', 'true');
    await userEvent.click(notes);
    expect(notes).toHaveAttribute('aria-expanded', 'false');
    expect(
      within(panel).queryByText('Faster scans, two new layouts and fixes.'),
    ).not.toBeInTheDocument();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Download and install' }),
    );
    expect(
      await within(panel).findByRole('button', { name: 'Restart now' }),
    ).toBeEnabled();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Restart now' }),
    );
    expect(backend.updateLog).toEqual([
      'check:manual',
      'download',
      'install',
      'restart',
    ]);
  });

  it('shows offline and bad-signature errors with a retry', async () => {
    await start('offline');
    const panel = await about();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Check for updates' }),
    );
    expect(
      await within(panel).findByText(/Could not reach the update server/),
    ).toBeInTheDocument();
    expect(
      within(panel).getByRole('button', { name: 'Try again' }),
    ).toBeInTheDocument();
  });

  it('a bad signature says nothing was installed', async () => {
    await start('bad-signature');
    const panel = await about();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Check for updates' }),
    );
    await userEvent.click(
      await within(panel).findByRole('button', {
        name: 'Download and install',
      }),
    );
    expect(
      await within(panel).findByText(
        /failed its signature check and was NOT installed/,
      ),
    ).toBeInTheDocument();
  });

  it('restart is disabled while a fill job runs', async () => {
    await start('available');
    const panel = await about();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Check for updates' }),
    );
    await userEvent.click(
      await within(panel).findByRole('button', {
        name: 'Download and install',
      }),
    );
    await within(panel).findByRole('button', { name: 'Restart now' });
    act(() =>
      useJobStore.setState({
        job: { id: 1, state: 'applying' } as unknown as JobView,
      }),
    );
    expect(
      within(panel).getByRole('button', { name: 'Restart now' }),
    ).toBeDisabled();
    expect(
      within(panel).getByText('Restart after the fill job finishes.'),
    ).toBeInTheDocument();
  });

  it('startup check on: banner with the version; off: no request', async () => {
    await start('available', true);
    render(<Root />);
    expect(
      await screen.findByText(/Coverly ROMs \d+\.\d+\.0 is available\./),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Later' }));
    expect(screen.queryByText(/is available\./)).not.toBeInTheDocument();
  });

  it('no banner and no request when the setting is off', async () => {
    await start('available', false);
    render(<Root />);
    await screen.findByRole('button', { name: 'Settings' });
    await waitFor(() => expect(backend.updateLog).toEqual([]));
    expect(screen.queryByText(/is available\./)).not.toBeInTheDocument();
  });

  it('no banner when the startup check fails', async () => {
    await start('offline', true);
    render(<Root />);
    await waitFor(() => expect(backend.updateLog).toEqual(['check:startup']));
    expect(screen.queryByText(/update server/)).not.toBeInTheDocument();
  });

  it('without a key only the Releases link is offered, not Try again', async () => {
    await start('no-key');
    const panel = await about();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Check for updates' }),
    );
    const updates = (
      await within(panel).findByText(/This build cannot update itself/)
    ).closest('.update-panel') as HTMLElement;
    expect(
      within(updates).getByRole('button', { name: 'Releases' }),
    ).toBeInTheDocument();
    expect(
      within(updates).queryByRole('button', { name: 'Try again' }),
    ).not.toBeInTheDocument();
    expect(
      within(updates).queryByRole('button', { name: 'Check for updates' }),
    ).not.toBeInTheDocument();
  });

  it('a failed restart leaves Restarting, shows why and offers Restart again', async () => {
    await start('available');
    const panel = await about();
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Check for updates' }),
    );
    await userEvent.click(
      await within(panel).findByRole('button', {
        name: 'Download and install',
      }),
    );
    let fail: (e: unknown) => void = () => undefined;
    vi.spyOn(backend, 'restartApp').mockImplementationOnce(
      () =>
        new Promise<void>((_, reject) => {
          fail = reject;
        }),
    );
    await userEvent.click(
      await within(panel).findByRole('button', { name: 'Restart now' }),
    );
    expect(
      await within(panel).findByText('Restarting Coverly ROMs…'),
    ).toBeInTheDocument();
    expect(
      within(panel).queryByRole('button', { name: 'Restart now' }),
    ).not.toBeInTheDocument();
    act(() => fail(new BackendError('job-running')));
    expect(await within(panel).findByRole('alert')).toBeInTheDocument();
    expect(
      within(panel).getByText(/Version \d+\.\d+\.0 is installed\./),
    ).toBeInTheDocument();
    const again = within(panel).getByRole('button', { name: 'Restart now' });
    expect(again).toBeEnabled();
    await userEvent.click(again);
    await waitFor(() => expect(backend.updateLog).toContain('restart'));
    expect(within(panel).queryByRole('alert')).not.toBeInTheDocument();
  });

  it('General has a Check now button with the status', async () => {
    await start('up-to-date');
    render(<Root />);
    await screen.findByRole('button', { name: 'Settings' });
    act(() => useNavStore.getState().openSettings('general'));
    const panel = await screen.findByRole('tabpanel');
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Check for updates' }),
    );
    expect(await within(panel).findByText(/is up to date/)).toBeInTheDocument();
  });

  it('Spanish strings', async () => {
    await start('available');
    await backend.updateSettings({ general: { language: 'es' } });
    render(<Root />);
    await screen.findByRole('button', { name: 'Configuración' });
    act(() => useNavStore.getState().openSettings('about'));
    const panel = await screen.findByRole('tabpanel');
    await userEvent.click(
      within(panel).getByRole('button', { name: 'Buscar actualizaciones' }),
    );
    expect(
      await within(panel).findByText(
        /La versión \d+\.\d+\.0 está disponible\./,
      ),
    ).toBeInTheDocument();
  });
});
