import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Root } from '@/Root';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
} from '@/ipc/mock/mockBackend';
import { useDedupeStore } from '@/stores/dedupeStore';
import { resetLibraryStore } from '@/stores/libraryStore';
import { resetNavStore, useNavStore } from '@/stores/navStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

const ROOT = MOCK_LIBRARIES[0].path;
const CHANGED = 'The duplicates settings changed. Look again to use them.';

let backend: MockBackend;
let restore: () => void;

async function openDuplicates() {
  useNavStore.getState().openDuplicates(ROOT);
  render(<Root />);
  await waitFor(() => expect(useSettingsStore.getState().status).toBe('ready'));
  await waitFor(() => expect(useDedupeStore.getState().status).toBe('ready'));
  return screen.getByRole('button', { name: 'Duplicates settings' });
}

async function toSettings(link: HTMLElement) {
  await userEvent.click(link);
  const tabs = await screen.findByRole('tablist', {
    name: 'Settings sections',
  });
  return within(tabs).getByRole('tab', { name: 'Duplicates' });
}

/** Waits for a save to land, then goes Back to the screen. */
async function back(saves: number) {
  await waitFor(() => expect(useSettingsStore.getState().saved).toBe(saves));
  await userEvent.click(screen.getByRole('button', { name: 'Back' }));
  await screen.findByRole('heading', { level: 1, name: 'Duplicates' });
}

describe('Duplicates screen settings link', () => {
  beforeEach(() => {
    backend = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: new MemoryStorage(),
      pauseAfter: null,
    });
    setBackend(backend);
    resetSettingsStore();
    resetLibraryStore();
    resetNavStore();
    useDedupeStore.getState().reset();
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    useDedupeStore.getState().reset();
    restore();
    vi.restoreAllMocks();
    setBackend(null);
  });

  it('opens the duplicates tab and comes back with the plan', async () => {
    const link = await openDuplicates();
    const plan = useDedupeStore.getState().plan;
    const tab = await toSettings(link);
    expect(tab).toHaveAttribute('aria-selected', 'true');
    await back(0);
    expect(useDedupeStore.getState().plan).toBe(plan);
    expect(useDedupeStore.getState().status).toBe('ready');
    expect(
      screen.getByRole('button', { name: 'Duplicates settings' }),
    ).toHaveFocus();
    expect(screen.queryByText(CHANGED)).not.toBeInTheDocument();
  });

  it('a changed keeper rule shows the alert and Look again re-plans', async () => {
    const link = await openDuplicates();
    await toSettings(link);
    await userEvent.click(
      screen.getByRole('button', { name: /^Advanced options/ }),
    );
    const rules = screen.getByRole('list', { name: 'Which copy stays' });
    const [down] = within(rules).getAllByRole('button', {
      name: /^Move .* down$/,
    });
    await userEvent.click(down!);
    await back(1);
    const alert = screen
      .getByText(CHANGED)
      .closest<HTMLElement>('[role="status"]');
    expect(alert).not.toBeNull();
    const plan = vi.spyOn(backend, 'dedupePlan');
    await userEvent.click(
      within(alert!).getByRole('button', { name: 'Look again' }),
    );
    expect(plan).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(CHANGED)).not.toBeInTheDocument();
    await waitFor(() => expect(useDedupeStore.getState().status).toBe('ready'));
    expect(screen.queryByText(CHANGED)).not.toBeInTheDocument();
  });

  it('keeps the alert after a second visit to Settings with no change', async () => {
    const link = await openDuplicates();
    await toSettings(link);
    await userEvent.click(
      screen.getByRole('button', { name: /^Advanced options/ }),
    );
    const rules = screen.getByRole('list', { name: 'Which copy stays' });
    const [down] = within(rules).getAllByRole('button', {
      name: /^Move .* down$/,
    });
    await userEvent.click(down!);
    await back(1);
    expect(screen.getByText(CHANGED)).toBeInTheDocument();
    // The plan on screen still reads the old order: opening Settings again
    // must not take the changed order as the new baseline.
    await toSettings(
      screen.getByRole('button', { name: 'Duplicates settings' }),
    );
    await back(1);
    expect(screen.getByText(CHANGED)).toBeInTheDocument();
  });

  it('a changed default action shows no alert', async () => {
    const link = await openDuplicates();
    await toSettings(link);
    await userEvent.click(
      screen.getByRole('radio', { name: 'Send to the Trash' }),
    );
    await back(1);
    expect(useSettingsStore.getState().settings!.dedupe.action).toBe('trash');
    expect(screen.queryByText(CHANGED)).not.toBeInTheDocument();
  });

  it('the link is disabled while applying', async () => {
    const link = await openDuplicates();
    expect(link).toBeEnabled();
    useDedupeStore.setState({ status: 'applying' });
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: 'Duplicates settings' }),
      ).toBeDisabled(),
    );
  });
});
