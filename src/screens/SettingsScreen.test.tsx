import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Root } from '@/Root';
import { AdvancedFold } from '@/components/settings/Field';
import { setBackend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
} from '@/ipc/mock/mockBackend';
import { DEFAULT_REGION_ORDER } from '@/lib/fill';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { resetNavStore, useNavStore } from '@/stores/navStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

import { SECTION_COMPONENTS } from './SettingsScreen';

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
    resetNavStore();
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
      'Organise',
      'Duplicates',
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

  it('moves focus into the screen and back to the Settings button', async () => {
    await start();
    screen.getByRole('button', { name: 'Settings' }).focus();
    await userEvent.keyboard('{Enter}');
    expect(screen.getByRole('heading', { name: 'Settings' })).toHaveFocus();
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(screen.getByRole('button', { name: 'Settings' })).toHaveFocus();
  });

  it('lands on a setting and marks it', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      await start();
      act(() =>
        useNavStore
          .getState()
          .openSettings('library', { focus: 'setting-library-include-hidden' }),
      );
      const hidden = await screen.findByRole('switch', {
        name: 'Include hidden files',
      });
      await waitFor(() => expect(hidden).toHaveFocus());
      expect(
        screen.getByRole('heading', { name: 'Settings' }),
      ).not.toHaveFocus();
      const row = hidden.closest('[data-setting-row]');
      expect(row).toHaveAttribute('data-arrived', 'true');
      expect(useNavStore.getState().focusSetting).toBeNull();
      act(() => vi.advanceTimersByTime(1200));
      expect(row).not.toHaveAttribute('data-arrived');
    } finally {
      vi.useRealTimers();
    }
  });

  it('opens the fold that holds the target', async () => {
    await start();
    act(() =>
      useNavStore
        .getState()
        .openSettings('matching', { focus: 'setting-matching-regions' }),
    );
    const list = await screen.findByRole('list', { name: 'Region order' });
    await waitFor(() => expect(list).toHaveFocus());
    expect(list).toHaveAttribute('id', 'setting-matching-regions');
    expect(useNavStore.getState().focusSetting).toBeNull();
  });

  it('focuses the tab when the target is missing', async () => {
    await start();
    act(() =>
      useNavStore.getState().openSettings('art', { focus: 'setting-nope' }),
    );
    const tab = await screen.findByRole('tab', { name: 'Art' });
    await waitFor(() => expect(tab).toHaveFocus());
    expect(useNavStore.getState().focusSetting).toBeNull();
  });

  it('lands on the ignored folders inside the closed library fold', async () => {
    await start();
    act(() =>
      useNavStore
        .getState()
        .openSettings('library', { focus: 'setting-library-ignored-dirs' }),
    );
    const box = await screen.findByRole('textbox', { name: 'Ignored folders' });
    await waitFor(() => expect(box).toHaveFocus());
    expect(useNavStore.getState().openFolds).toEqual(['library-advanced']);
    expect(
      screen.getByRole('button', { name: 'Advanced options (2)' }),
    ).toHaveAttribute('aria-expanded', 'true');
    expect(box.closest('[data-setting-row]')).toHaveAttribute(
      'data-arrived',
      'true',
    );
  });

  it('opens a closed advanced fold before landing inside it', async () => {
    const original = SECTION_COMPONENTS.art;
    SECTION_COMPONENTS.art = () => (
      <AdvancedFold id="art-test-advanced" count={1} errorPaths={[]}>
        <div data-setting-row="">
          <label htmlFor="setting-test-folded">Folded</label>
          <input id="setting-test-folded" />
        </div>
      </AdvancedFold>
    );
    try {
      await start();
      act(() =>
        useNavStore
          .getState()
          .openSettings('art', { focus: 'setting-test-folded' }),
      );
      const input = await screen.findByLabelText('Folded');
      await waitFor(() => expect(input).toHaveFocus());
      expect(useNavStore.getState().openFolds).toEqual(['art-test-advanced']);
      expect(
        screen.getByRole('button', { name: 'Advanced options (1)' }),
      ).toHaveAttribute('aria-expanded', 'true');
      expect(input.closest('[data-setting-row]')).toHaveAttribute(
        'data-arrived',
        'true',
      );
      expect(useNavStore.getState().focusSetting).toBeNull();
    } finally {
      SECTION_COMPONENTS.art = original;
    }
  });

  it('keeps the mark for its full time when the same setting is reached again', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      await start();
      const land = () =>
        act(() =>
          useNavStore.getState().openSettings('library', {
            focus: 'setting-library-include-hidden',
          }),
        );
      land();
      const hidden = await screen.findByRole('switch', {
        name: 'Include hidden files',
      });
      await waitFor(() => expect(hidden).toHaveFocus());
      const row = hidden.closest('[data-setting-row]');
      act(() => vi.advanceTimersByTime(800));
      land();
      await waitFor(() =>
        expect(useNavStore.getState().focusSetting).toBeNull(),
      );
      act(() => vi.advanceTimersByTime(800));
      expect(row).toHaveAttribute('data-arrived', 'true');
      act(() => vi.advanceTimersByTime(400));
      expect(row).not.toHaveAttribute('data-arrived');
    } finally {
      vi.useRealTimers();
    }
  });

  it('returns focus to the library toolbar after Back', async () => {
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
    await start();
    await userEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(screen.getByRole('button', { name: 'Settings' })).toHaveFocus();
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
    await userEvent.keyboard('{ArrowDown}');
    expect(screen.getByRole('tab', { name: 'Organise' })).toHaveFocus();
    await userEvent.keyboard('{ArrowUp}{ArrowUp}{ArrowUp}');
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
      await screen.findByRole('switch', { name: 'Include hidden files' }),
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
    await userEvent.click(
      await screen.findByRole('button', { name: 'Advanced options (2)' }),
    );
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

  it('shows a default profile it does not know instead of auto', async () => {
    await start();
    act(() => useNavStore.getState().openSettings('library'));
    const select = await screen.findByRole('combobox', {
      name: 'Default layout profile',
    });
    act(() => {
      const cur = useSettingsStore.getState().settings!;
      useSettingsStore.setState({
        settings: {
          ...cur,
          library: { ...cur.library, default_profile: 'gone-profile' },
        },
      });
    });
    expect(select).toHaveDisplayValue('gone-profile');
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

  it('says when a setting could not be saved', async () => {
    await start();
    act(() => useNavStore.getState().openSettings('general'));
    const real = backend.updateSettings.bind(backend);
    backend.updateSettings = () =>
      Promise.reject(new BackendError('settings', 'read-only folder'));
    await userEvent.click(await screen.findByRole('radio', { name: 'Dark' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The settings could not be saved: read-only folder',
    );
    backend.updateSettings = real;
    await userEvent.click(screen.getByRole('radio', { name: 'Light' }));
    await waitFor(() =>
      expect(screen.queryByRole('alert')).not.toBeInTheDocument(),
    );
  });

  it('ignores a key test that ends after the key was saved or removed', async () => {
    await start();
    act(() => useNavStore.getState().openSettings('sources'));
    await backend.setCredential('steamgriddb-api-key', 'sk-old');
    const pending: ((v: 'ok' | 'invalid') => void)[] = [];
    backend.testSourceKey = () =>
      new Promise((resolve) => pending.push(resolve));
    const field = await screen.findByLabelText('SteamGridDB API key');
    const testButton = await screen.findByRole('button', {
      name: 'Test the SteamGridDB API key',
    });
    await waitFor(() => expect(testButton).toBeEnabled());
    // Test the old key, then save a new one before the answer comes.
    await userEvent.click(testButton);
    await userEvent.type(field, 'sk-new');
    await userEvent.click(
      screen.getByRole('button', { name: 'Save SteamGridDB API key' }),
    );
    await act(async () => pending.shift()!('invalid'));
    expect(screen.queryByText('The key was refused.')).not.toBeInTheDocument();
    // Test again, then remove the key before the answer comes.
    await waitFor(() => expect(testButton).toBeEnabled());
    await userEvent.click(testButton);
    await userEvent.click(
      screen.getByRole('button', { name: 'Remove SteamGridDB API key' }),
    );
    await act(async () => pending.shift()!('ok'));
    expect(screen.queryByText('The key works.')).not.toBeInTheDocument();
    expect(testButton).not.toHaveTextContent('Testing…');
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
