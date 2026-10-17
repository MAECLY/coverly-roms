import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import type { EntryView } from '@/ipc/types';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useSelectionStore } from '@/stores/selectionStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

async function setup() {
  setBackend(
    createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: new MemoryStorage(),
    }),
  );
  resetLibraryStore();
  resetSettingsStore();
  useJobStore.getState().reset();
  await useSettingsStore.getState().load();
  await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  render(
    <I18nProvider locale="en">
      <App />
    </I18nProvider>,
  );
  return useLibraryStore.getState().view!;
}

function show(entry: EntryView) {
  act(() => {
    useSelectionStore.getState().select(entry.id);
    useViewStore.getState().openDrawer();
  });
  return screen.getByRole('complementary', { name: entry.name });
}

describe('drawer search by title', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('searches a typed title, writes the pick and shows the manual match', async () => {
    const view = await setup();
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    const drawer = show(entry);
    const box = within(drawer).getByRole('searchbox', {
      name: 'Title to search',
    });
    // Pre-filled with the game's name, without its tags.
    expect(box).toHaveValue(entry.name.replace(/\s*[([][^)\]]*[)\]]/g, ''));
    await userEvent.clear(box);
    await userEvent.type(box, 'Golden Sun{enter}');
    const group = await within(drawer).findByRole('group', {
      name: 'Search results',
    });
    await userEvent.click(
      within(group).getByRole('button', { name: /Golden Sun \(Europe\)/ }),
    );
    expect(
      await within(drawer).findByText(
        /^Art updated\. This ROM now matches “Golden Sun \(Europe\)”\./,
      ),
    ).toBeInTheDocument();
    expect(useLibraryStore.getState().view!.entries[entry.id]!.art.cover).toBe(
      'present',
    );
    expect(
      await within(drawer).findByText('Chosen by you'),
    ).toBeInTheDocument();
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Forget manual match' }),
    );
    expect(within(drawer).queryByText('Chosen by you')).not.toBeInTheDocument();
  });

  it('asks before replacing existing art', async () => {
    const view = await setup();
    const entry = view.entries.find((e) => e.art.cover === 'present')!;
    const drawer = show(entry);
    await userEvent.click(within(drawer).getByRole('tab', { name: 'Cover' }));
    await userEvent.type(
      within(drawer).getByRole('searchbox', { name: 'Title to search' }),
      '{selectall}Zelda{enter}',
    );
    const group = await within(drawer).findByRole('group', {
      name: 'Search results',
    });
    await userEvent.click(within(group).getAllByRole('button')[0]!);
    expect(
      within(drawer).getByText(/The current image is backed up/),
    ).toBeInTheDocument();
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Replace' }),
    );
    expect(await within(drawer).findByText(/Art updated/)).toBeInTheDocument();
  });

  it('search renders hostile names as text and refuses an empty title', async () => {
    const view = await setup();
    const entry = view.entries[0]!;
    const drawer = show(entry);
    const box = within(drawer).getByRole('searchbox', {
      name: 'Title to search',
    });
    await userEvent.clear(box);
    expect(
      within(drawer).getByRole('button', { name: 'Search' }),
    ).toBeDisabled();
    await userEvent.type(box, '<img src=x onerror=alert(1)>{enter}');
    await within(drawer).findByRole('group', { name: 'Search results' });
    expect(
      within(drawer).getAllByText(/<img src=x onerror=alert\(1\)> \(USA\)/)
        .length,
    ).toBeGreaterThan(0);
    expect(document.querySelectorAll('img[src="x"], [onerror]').length).toBe(0);
  });

  it('searches in place and clearing the title brings the candidates back', async () => {
    const view = await setup();
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    const drawer = show(entry);
    await within(drawer).findByRole('group', { name: 'Candidates' });
    const box = within(drawer).getByRole('searchbox', {
      name: 'Title to search',
    });
    await userEvent.clear(box);
    await userEvent.type(box, 'Zelda{enter}');
    const results = await within(drawer).findByRole('group', {
      name: 'Search results',
    });
    expect(within(results).getAllByText('Zelda (USA)').length).toBe(1);
    expect(
      within(drawer).queryByRole('group', { name: 'Candidates' }),
    ).toBeNull();
    await userEvent.clear(box);
    await within(drawer).findByRole('group', { name: 'Candidates' });
    expect(
      within(drawer).queryByRole('group', { name: 'Search results' }),
    ).toBeNull();

    await userEvent.type(box, 'Zelda{enter}');
    await within(drawer).findByRole('group', { name: 'Search results' });
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Back to candidates' }),
    );
    await within(drawer).findByRole('group', { name: 'Candidates' });
  });
});
