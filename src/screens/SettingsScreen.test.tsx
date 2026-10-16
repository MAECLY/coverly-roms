import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { Root } from '@/Root';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
} from '@/ipc/mock/mockBackend';
import { DEFAULT_REGION_ORDER } from '@/lib/fill';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { resetSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

let backend: MockBackend;
let storage: MemoryStorage;
let restore: () => void;

async function start() {
  render(<Root />);
  await screen.findByRole('button', { name: 'Settings' });
}

describe('Settings screen', () => {
  beforeEach(() => {
    storage = new MemoryStorage();
    backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW, storage });
    setBackend(backend);
    resetSettingsStore();
    resetLibraryStore();
    useNavStore.getState().closeSettings();
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
    document.documentElement.removeAttribute('data-theme');
  });

  it('opens from the start screen, shows every section and goes back', async () => {
    await start();
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    const tabs = screen.getByRole('tablist', { name: 'Settings sections' });
    expect(
      within(tabs)
        .getAllByRole('tab')
        .map((t) => t.textContent),
    ).toEqual([
      'General',
      'Library',
      'Art',
      'Matching',
      'Sources',
      'Network',
      'Safety',
      'Advanced',
      'Profiles',
      'About',
    ]);
    expect(
      screen.getByRole('tabpanel', { name: 'General' }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(
      screen.getByRole('button', { name: 'Open folder…' }),
    ).toBeInTheDocument();
  });

  it('moves between sections with the arrow keys', async () => {
    await start();
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await userEvent.click(screen.getByRole('tab', { name: 'General' }));
    await userEvent.keyboard('{ArrowDown}');
    const library = screen.getByRole('tab', { name: 'Library' });
    expect(library).toHaveFocus();
    expect(library).toHaveAttribute('aria-selected', 'true');
    expect(
      screen.getByRole('tabpanel', { name: 'Library' }),
    ).toBeInTheDocument();
    await userEvent.keyboard('{ArrowUp}{ArrowUp}');
    expect(screen.getByRole('tab', { name: 'About' })).toHaveFocus();
    await userEvent.keyboard('{Home}');
    expect(screen.getByRole('tab', { name: 'General' })).toHaveFocus();
  });

  it('switches the theme for the whole app and keeps it after a reload', async () => {
    await start();
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await userEvent.click(screen.getByRole('radio', { name: 'Dark' }));
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(
      (
        await createMockBackend({
          delayMs: 0,
          now: FIXTURE_NOW,
          storage,
        }).getSettings()
      ).settings.general.theme,
    ).toBe('dark');
  });

  it('switches the language live', async () => {
    await start();
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Language' }),
      'es',
    );
    expect(
      await screen.findByRole('heading', { name: 'Configuración' }),
    ).toBeInTheDocument();
    expect(document.documentElement.lang).toBe('es');
  });

  it('offers to reopen after a scan setting changes', async () => {
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
    const before = useLibraryStore.getState().view!.generation;
    await start();
    act(() => useNavStore.getState().openSettings('library'));
    await userEvent.click(
      await screen.findByRole('checkbox', { name: 'Include hidden files' }),
    );
    expect(
      await screen.findByText('Reopen the library to apply these changes.'),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Reopen now' }));
    expect(useLibraryStore.getState().view!.generation).toBeGreaterThan(before);
    expect(
      screen.queryByText('Reopen the library to apply these changes.'),
    ).not.toBeInTheDocument();
  });

  it('edits the ignored folders and the default profile', async () => {
    await start();
    act(() => useNavStore.getState().openSettings('library'));
    const box = await screen.findByRole('textbox', { name: 'Ignored folders' });
    await userEvent.clear(box);
    await userEvent.type(box, 'bios{enter}Junk');
    await userEvent.tab();
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Default layout profile' }),
      'onion',
    );
    const s = (await backend.getSettings()).settings;
    expect(s.library.ignored_dirs).toEqual(['bios', 'Junk']);
    expect(s.library.default_profile).toBe('onion');
  });

  it('shows a notice when the stored settings were unreadable', async () => {
    storage.setItem('coverly.mock.settings', '{broken');
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW, storage }));
    await start();
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(screen.getByRole('status')).toHaveTextContent('could not be read');
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByText(/could not be read/)).not.toBeInTheDocument();
  });

  it('keeps the art a job writes while settings are open', async () => {
    backend = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage,
      pauseAfter: null,
    });
    setBackend(backend);
    useJobStore.getState().reset();
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
    const missing = () =>
      useLibraryStore
        .getState()
        .view!.entries.filter((e) => e.art.cover === 'missing').length;
    const before = missing();
    await start();
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await screen.findByRole('tablist', { name: 'Settings sections' });
    const view = useLibraryStore.getState().view!;
    await act(() =>
      useJobStore.getState().start(view.generation, {
        entry_ids: null,
        media: ['cover'],
        strategy: 'rom-region',
        regions: [...DEFAULT_REGION_ORDER],
        overwrite: 'never',
      }),
    );
    await waitFor(() =>
      expect(useJobStore.getState().job?.state).toBe('planned'),
    );
    await act(() => useJobStore.getState().run());
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    await waitFor(() => expect(missing()).toBeLessThan(before));
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(missing()).toBeLessThan(before);
  });

  it('shows About with the licence and opens fixed links only', async () => {
    await start();
    act(() => useNavStore.getState().openSettings('about'));
    expect(await screen.findByText(/AGPL-3\.0-only/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Licence' }));
    await userEvent.click(screen.getByRole('button', { name: 'Source code' }));
    expect(backend.opened).toEqual(['link:license', 'link:repo']);
  });
});
