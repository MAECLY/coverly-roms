import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import { useDedupeStore } from '@/stores/dedupeStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { stubLayout } from '@/test/layout';

async function library() {
  setBackend(
    createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: null,
      pauseAfter: null,
    }),
  );
  resetLibraryStore();
  useDedupeStore.getState().reset();
  useDedupeStore.setState({ counts: {} });
  useNavStore.setState({ screen: 'main', duplicatesRoot: null });
  await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  render(
    <I18nProvider locale="en">
      <App />
    </I18nProvider>,
  );
  return useLibraryStore.getState().view!;
}

describe('Toolbar', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('toolbar_shows_find_then_count', async () => {
    const view = await library();
    const find = await screen.findByRole('button', {
      name: 'Find duplicates',
    });
    await userEvent.click(find);
    expect(useNavStore.getState()).toMatchObject({
      screen: 'duplicates',
      duplicatesRoot: view.root,
    });
    await waitFor(() => expect(useDedupeStore.getState().status).toBe('ready'));
    const extras = useDedupeStore.getState().plan!.totals.extras;
    expect(extras).toBeGreaterThan(0);
    await userEvent.click(screen.getByRole('button', { name: 'Library' }));
    expect(
      await screen.findByRole('button', { name: `Duplicates ${extras}` }),
    ).toBeInTheDocument();
    expect(useDedupeStore.getState().counts[view.root]).toBe(extras);
  });

  it('a_count_for_another_library_is_not_shown', async () => {
    await library();
    useDedupeStore.setState({ counts: { '/Volumes/Other': 4 } });
    expect(
      await screen.findByRole('button', { name: 'Find duplicates' }),
    ).toBeInTheDocument();
  });
});
