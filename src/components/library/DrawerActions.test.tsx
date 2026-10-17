import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockOptions,
} from '@/ipc/mock/mockBackend';
import type { EntryView } from '@/ipc/types';
import { DEFAULT_SETTINGS } from '@/lib/settingsRules';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useSelectionStore } from '@/stores/selectionStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

async function setup(opts: MockOptions = {}) {
  setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW, ...opts }));
  resetLibraryStore();
  resetSettingsStore();
  useJobStore.getState().reset();
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

const cover = (id: number) =>
  useLibraryStore.getState().view!.entries[id]!.art.cover;

describe('Drawer art actions', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('fills an empty slot with one click and moves on to the next empty one', async () => {
    const view = await setup();
    const entry = view.entries.find(
      (e) =>
        e.art.cover === 'missing' &&
        e.art.screenshot === 'missing' &&
        e.art.title === 'present',
    )!;
    const drawer = show(entry);
    const group = await within(drawer).findByRole('group', {
      name: 'Candidates',
    });
    const buttons = within(group).getAllByRole('button');
    expect(buttons).toHaveLength(3);
    await userEvent.click(buttons[1]!);
    expect(
      await within(drawer).findByText(
        'Cover updated. Undo it from Jobs if needed. Now: Screenshot.',
      ),
    ).toHaveAttribute('role', 'status');
    expect(cover(entry.id)).toBe('present');
    expect(
      within(drawer).getByRole('tab', { name: 'Cover' }),
    ).toHaveAccessibleDescription('present');
    const shot = within(drawer).getByRole('tab', { name: 'Screenshot' });
    expect(shot).toHaveAttribute('aria-selected', 'true');
    expect(shot).toHaveFocus();

    // The next slot loads its own candidates: one more click fills it, and
    // with no empty slot left the list stays for comparison.
    const next = await within(drawer).findByRole('group', {
      name: 'Candidates',
    });
    const picked = within(next).getAllByRole('button')[0]!;
    await userEvent.click(picked);
    expect(
      await within(drawer).findByText(
        'Screenshot updated. Undo it from Jobs if needed.',
      ),
    ).toBeInTheDocument();
    expect(shot).toHaveAttribute('aria-selected', 'true');
    expect(within(picked).getByText('In use')).toBeInTheDocument();
    expect(picked).toHaveFocus();
    expect(
      within(drawer).queryByRole('button', { name: 'Replace' }),
    ).toBeNull();
  });

  it('drops the “In use” tag once the image is removed', async () => {
    const view = await setup();
    const entry = view.entries.find(
      (e) =>
        e.art.cover === 'missing' &&
        e.art.screenshot === 'present' &&
        e.art.title === 'present',
    )!;
    const drawer = show(entry);
    const group = await within(drawer).findByRole('group', {
      name: 'Candidates',
    });
    await userEvent.click(within(group).getAllByRole('button')[0]!);
    expect(await within(drawer).findByText('In use')).toBeInTheDocument();
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Remove' }),
    );
    await userEvent.click(
      within(
        screen.getByRole('alertdialog', { name: 'Remove this image?' }),
      ).getByRole('button', { name: 'Remove image' }),
    );
    expect(
      await within(drawer).findByText(/^Cover removed/),
    ).toBeInTheDocument();
    expect(cover(entry.id)).toBe('missing');
    expect(within(drawer).queryByText('In use')).toBeNull();
  });

  it('asks inline before replacing existing art, and can cancel', async () => {
    const view = await setup();
    const entry = view.entries.find(
      (e) => e.art.cover === 'missing' && e.art.screenshot === 'present',
    )!;
    const drawer = show(entry);
    await userEvent.click(
      within(drawer).getByRole('tab', { name: 'Screenshot' }),
    );
    const group = await within(drawer).findByRole('group', {
      name: 'Candidates',
    });
    const [first, second] = within(group).getAllByRole('button');
    await userEvent.click(first!);
    expect(first).toHaveAttribute('aria-pressed', 'true');
    const bar = within(drawer).getByRole('group', { name: /^Use “/ });
    expect(first!.parentElement).toContainElement(bar);
    expect(within(bar).getByText(/backed up/)).toBeInTheDocument();
    expect(within(bar).getByRole('button', { name: 'Replace' })).toHaveFocus();
    await userEvent.click(within(bar).getByRole('button', { name: 'Cancel' }));
    expect(
      within(drawer).queryByRole('button', { name: 'Replace' }),
    ).toBeNull();
    expect(first).toHaveFocus();

    await userEvent.click(second!);
    await userEvent.keyboard('{Escape}');
    expect(
      within(drawer).queryByRole('button', { name: 'Replace' }),
    ).toBeNull();
    expect(screen.getByRole('complementary')).toBeInTheDocument();

    await userEvent.click(second!);
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Replace' }),
    );
    expect(
      await within(drawer).findByText(
        'Screenshot updated. Undo it from Jobs if needed.',
      ),
    ).toBeInTheDocument();
    expect(second).toHaveFocus();
  });

  it('replaces without asking when the settings say to keep a backup', async () => {
    const view = await setup();
    act(() =>
      useSettingsStore.setState((s) => ({
        settings: {
          ...(s.settings ?? DEFAULT_SETTINGS),
          art: {
            ...(s.settings ?? DEFAULT_SETTINGS).art,
            overwrite: 'backup',
          },
        },
      })),
    );
    const entry = view.entries.find(
      (e) => e.art.cover === 'missing' && e.art.screenshot === 'present',
    )!;
    const drawer = show(entry);
    await userEvent.click(
      within(drawer).getByRole('tab', { name: 'Screenshot' }),
    );
    const group = await within(drawer).findByRole('group', {
      name: 'Candidates',
    });
    await userEvent.click(within(group).getAllByRole('button')[0]!);
    expect(
      await within(drawer).findByText(/^Screenshot updated/),
    ).toBeInTheDocument();
  });

  it('sets the art from a file and removes it after confirmation', async () => {
    const view = await setup();
    const entry = view.entries.find(
      (e) =>
        e.art.cover === 'missing' &&
        e.art.screenshot === 'present' &&
        e.art.title === 'present',
    )!;
    const drawer = show(entry);
    expect(
      within(drawer).getByRole('button', { name: 'Remove' }),
    ).toBeDisabled();
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'From file…' }),
    );
    expect(
      await within(drawer).findByText(/^Cover updated/),
    ).toBeInTheDocument();
    expect(cover(entry.id)).toBe('present');
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Remove' }),
    );
    // A destructive question: an alert dialog that says what happens and
    // starts on the safe answer.
    const ask = screen.getByRole('alertdialog', { name: 'Remove this image?' });
    expect(ask).toHaveAccessibleDescription(
      /backed up and Undo brings it back/,
    );
    expect(within(ask).getByRole('button', { name: 'Keep' })).toHaveFocus();
    await userEvent.click(within(ask).getByRole('button', { name: 'Keep' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(
      within(drawer).getByRole('button', { name: 'Remove' }),
    ).toHaveFocus();
    expect(cover(entry.id)).toBe('present');
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Remove' }),
    );
    // Escape answers "Keep" and leaves the drawer open.
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.getByRole('complementary')).toBeInTheDocument();
    expect(cover(entry.id)).toBe('present');
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'Remove' }),
    );
    await userEvent.click(
      within(screen.getByRole('alertdialog')).getByRole('button', {
        name: 'Remove image',
      }),
    );
    expect(
      await within(drawer).findByText(
        'Cover removed. Undo it from Jobs if needed.',
      ),
    ).toBeInTheDocument();
    expect(cover(entry.id)).toBe('missing');
    // Remove is disabled now, so the focus lands on the next header action
    // rather than on the page.
    expect(
      within(drawer).getByRole('button', { name: 'From file…' }),
    ).toHaveFocus();
  });

  it('does nothing when the file picker is cancelled', async () => {
    const view = await setup({ pick: 'cancel' });
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    const drawer = show(entry);
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'From file…' }),
    );
    expect(cover(entry.id)).toBe('missing');
    expect(within(drawer).queryByRole('alert')).not.toBeInTheDocument();
  });

  it('explains a file that is not an image', async () => {
    const view = await setup({ pick: 'not-image' });
    const drawer = show(view.entries[0]!);
    await userEvent.click(
      within(drawer).getByRole('button', { name: 'From file…' }),
    );
    expect(await within(drawer).findByRole('alert')).toHaveTextContent(
      'This file is not a usable image: not an image',
    );
  });

  it('disables art changes while a job runs', async () => {
    const view = await setup();
    act(() =>
      useJobStore.setState({
        job: {
          id: 1,
          generation: view.generation,
          state: 'applying',
          progress: null,
          paused: null,
          plan: null,
          decided: 0,
          skipped: 0,
          journal: null,
          result: null,
          error: null,
        },
      }),
    );
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    const drawer = show(entry);
    for (const name of ['From file…', 'Remove', 'Search']) {
      expect(within(drawer).getByRole('button', { name })).toBeDisabled();
    }
    expect(
      within(drawer).getByRole('searchbox', { name: 'Title to search' }),
    ).toBeDisabled();
    // Nothing is looked up while the job runs.
    await act(() => new Promise((r) => setTimeout(r, 300)));
    expect(within(drawer).queryByRole('group')).toBeNull();
    expect(within(drawer).queryByRole('alert')).toBeNull();
    expect(
      within(drawer).getByText(/A fill job is running/),
    ).toBeInTheDocument();
    // The held lookup goes out once the job ends.
    act(() => useJobStore.getState().reset());
    expect(
      await within(drawer).findByRole('group', { name: 'Candidates' }),
    ).toBeInTheDocument();
  });
});
