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
import { MOCK_MIXED } from '@/ipc/mock/mockOrganize';
import { useDedupeStore } from '@/stores/dedupeStore';
import { resetLibraryStore } from '@/stores/libraryStore';
import { resetNavStore, useNavStore } from '@/stores/navStore';
import { useOrganizeStore } from '@/stores/organizeStore';
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

const ORG_CHANGED =
  'The organise settings changed. Scan again to use them for what you have not chosen here.';

describe('Organise screen settings link', () => {
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
    useOrganizeStore.getState().reset();
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    useOrganizeStore.getState().reset();
    restore();
    vi.restoreAllMocks();
    setBackend(null);
  });

  const organizeReady = () =>
    waitFor(() => expect(useOrganizeStore.getState().status).toBe('ready'));

  /** Opens Organise on the mixed pile and makes one decision. */
  async function openOrganize() {
    useNavStore.getState().openOrganize(MOCK_MIXED);
    render(<Root />);
    await waitFor(() =>
      expect(useSettingsStore.getState().status).toBe('ready'),
    );
    await organizeReady();
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'System for mystery.rom' }),
      'gba',
    );
    await organizeReady();
    expect(Object.keys(useOrganizeStore.getState().decisions)).toHaveLength(1);
    return screen.getByRole('button', { name: 'Organise settings' });
  }

  async function toOrganizeSettings(link: HTMLElement) {
    await userEvent.click(link);
    const tabs = await screen.findByRole('tablist', {
      name: 'Settings sections',
    });
    return within(tabs).getByRole('tab', { name: 'Organise' });
  }

  async function backToOrganize(saves: number) {
    await waitFor(() => expect(useSettingsStore.getState().saved).toBe(saves));
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    await screen.findByRole('heading', {
      level: 1,
      name: 'Organise by system',
    });
  }

  it('opens the organise tab and comes back with the decisions', async () => {
    const link = await openOrganize();
    const { plan, decisions } = useOrganizeStore.getState();
    const tab = await toOrganizeSettings(link);
    expect(tab).toHaveAttribute('aria-selected', 'true');
    await backToOrganize(0);
    expect(useOrganizeStore.getState().plan).toBe(plan);
    expect(useOrganizeStore.getState().decisions).toBe(decisions);
    expect(useOrganizeStore.getState().status).toBe('ready');
    expect(
      screen.getByRole('button', { name: 'Organise settings' }),
    ).toHaveFocus();
    expect(screen.queryByText(ORG_CHANGED)).not.toBeInTheDocument();
  });

  it('a changed names setting shows the alert and Scan again keeps the decisions', async () => {
    const link = await openOrganize();
    await toOrganizeSettings(link);
    const names = screen.getByRole('combobox', { name: 'Names' });
    const other =
      (names as HTMLSelectElement).value === 'keep' ? 'catalogue' : 'keep';
    await userEvent.selectOptions(names, other);
    await backToOrganize(1);
    const alert = screen
      .getByText(ORG_CHANGED)
      .closest<HTMLElement>('[role="status"]');
    expect(alert).not.toBeNull();
    const plan = vi.spyOn(backend, 'organizePlan');
    await userEvent.click(
      within(alert!).getByRole('button', { name: 'Scan again' }),
    );
    expect(plan.mock.calls[0]![0]).toMatchObject({ rescan: true });
    await organizeReady();
    expect(screen.queryByText(ORG_CHANGED)).not.toBeInTheDocument();
    expect(screen.getByText('Your choices kept: 1')).toBeInTheDocument();
    expect(Object.keys(useOrganizeStore.getState().decisions)).toHaveLength(1);
  });

  it('keeps the organise alert after a second visit to Settings with no change', async () => {
    const link = await openOrganize();
    await toOrganizeSettings(link);
    const names = screen.getByRole('combobox', { name: 'Names' });
    const other =
      (names as HTMLSelectElement).value === 'keep' ? 'catalogue' : 'keep';
    await userEvent.selectOptions(names, other);
    await backToOrganize(1);
    expect(screen.getByText(ORG_CHANGED)).toBeInTheDocument();
    await toOrganizeSettings(
      screen.getByRole('button', { name: 'Organise settings' }),
    );
    await backToOrganize(1);
    expect(screen.getByText(ORG_CHANGED)).toBeInTheDocument();
  });

  /** Holds every `organizePlan` call until `release()` runs. */
  function holdPlans() {
    const original = backend.organizePlan.bind(backend);
    let open!: () => void;
    const gate = new Promise<void>((resolve) => {
      open = resolve;
    });
    const spy = vi
      .spyOn(backend, 'organizePlan')
      .mockImplementation(async (...args) => {
        await gate;
        return original(...args);
      });
    return { spy, release: () => open() };
  }

  async function changeNames() {
    const names = screen.getByRole('combobox', { name: 'Names' });
    const other =
      (names as HTMLSelectElement).value === 'keep' ? 'catalogue' : 'keep';
    await userEvent.selectOptions(names, other);
  }

  it('a change made while a scan runs shows the alert once that scan is shown', async () => {
    const link = await openOrganize();
    const { spy, release } = holdPlans();
    await userEvent.click(screen.getByRole('button', { name: 'Scan again' }));
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(useOrganizeStore.getState().status).toBe('planning');
    // The scan in flight already read the settings: a change now is newer.
    await toOrganizeSettings(link);
    await changeNames();
    await waitFor(() => expect(useSettingsStore.getState().saved).toBe(1));
    release();
    await organizeReady();
    await backToOrganize(1);
    expect(screen.getByText(ORG_CHANGED)).toBeInTheDocument();
  });

  it('a cancelled scan keeps the alert of the plan on screen', async () => {
    const link = await openOrganize();
    await toOrganizeSettings(link);
    await changeNames();
    await backToOrganize(1);
    const alert = screen
      .getByText(ORG_CHANGED)
      .closest<HTMLElement>('[role="status"]');
    const { spy } = holdPlans();
    await userEvent.click(
      within(alert!).getByRole('button', { name: 'Scan again' }),
    );
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(screen.queryByText(ORG_CHANGED)).not.toBeInTheDocument();
    await useOrganizeStore.getState().cancel();
    await organizeReady();
    // The plan on screen still has the old settings.
    expect(await screen.findByText(ORG_CHANGED)).toBeInTheDocument();
  });

  it('the organise link is disabled while applying', async () => {
    const link = await openOrganize();
    expect(link).toBeEnabled();
    useOrganizeStore.setState({ status: 'applying' });
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: 'Organise settings' }),
      ).toBeDisabled(),
    );
  });
});
