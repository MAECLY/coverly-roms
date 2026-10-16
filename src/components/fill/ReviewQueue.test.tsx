import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import { mockClass, mockFailure } from '@/ipc/mock/mockJobs';
import type { JobItemView } from '@/ipc/types';
import { DEFAULT_REGION_ORDER } from '@/lib/fill';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

async function planned(locale: 'en' | 'es' = 'en') {
  const backend = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    pauseAfter: null,
  });
  setBackend(backend);
  resetLibraryStore();
  useJobStore.getState().reset();
  await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
  const view = useLibraryStore.getState().view!;
  await useJobStore.getState().start(view.generation, {
    entry_ids: null,
    media: ['cover'],
    strategy: 'rom-region',
    regions: [...DEFAULT_REGION_ORDER],
    overwrite: 'never',
  });
  await waitFor(() =>
    expect(useJobStore.getState().job?.state).toBe('planned'),
  );
  const review = view.entries.filter(
    (e) => e.art.cover === 'missing' && mockClass(e.id, 'cover') === 'review',
  );
  return { backend, view, review };
}

function openQueue(name = 'Review doubtful matches') {
  act(() => useViewStore.getState().setOverlay('review'));
  return screen.findByRole('dialog', { name });
}

describe('Review queue', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('shows the current art and the candidates side by side', async () => {
    const { review } = await planned();
    const dialog = await openQueue();
    const first = review[0]!;
    expect(
      await within(dialog).findByRole('heading', {
        level: 3,
        name: first.name,
      }),
    ).toBeInTheDocument();
    expect(within(dialog).getByText('Current art')).toBeInTheDocument();
    const group = within(dialog).getByRole('group', { name: 'Candidates' });
    const buttons = within(group).getAllByRole('button');
    expect(buttons.length).toBeGreaterThanOrEqual(2);
    expect(buttons[0]).toHaveAttribute('aria-pressed', 'true');
    expect(within(group).getByText(/^1\. libretro · /)).toBeInTheDocument();
    expect(
      within(dialog).getByText(`1 of ${review.length} · 0 decided`),
    ).toBeInTheDocument();
  });

  it('opens from the wizard summary', async () => {
    const { review } = await planned();
    act(() => useViewStore.getState().setOverlay('wizard'));
    const wizard = await screen.findByRole('dialog', {
      name: 'Fill missing art',
    });
    await userEvent.click(
      within(wizard).getByRole('button', { name: `Review (${review.length})` }),
    );
    expect(
      await screen.findByRole('dialog', { name: 'Review doubtful matches' }),
    ).toBeInTheDocument();
  });

  it('is keyboard driven: digits pick, Enter accepts, S skips, A accepts the rest', async () => {
    const { backend, review } = await planned();
    const spy = vi.spyOn(backend, 'decideFill');
    const dialog = await openQueue();
    await within(dialog).findByRole('heading', {
      level: 3,
      name: review[0]!.name,
    });
    const items = await backend.fillJobItems(
      useJobStore.getState().job!.id,
      'review',
    );
    fireEvent.keyDown(window, { key: '2' });
    const group = within(dialog).getByRole('group', { name: 'Candidates' });
    expect(within(group).getAllByRole('button')[1]).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    fireEvent.keyDown(window, { key: 'Enter' });
    await waitFor(() =>
      expect(spy).toHaveBeenLastCalledWith(expect.any(Number), [
        { choice: 'pick', candidate: 1, index: items[0]!.index },
      ]),
    );
    expect(
      await within(dialog).findByRole('heading', {
        level: 3,
        name: review[1]!.name,
      }),
    ).toBeInTheDocument();
    fireEvent.keyDown(window, { key: 's' });
    await waitFor(() =>
      expect(spy).toHaveBeenLastCalledWith(expect.any(Number), [
        { choice: 'skip', index: items[1]!.index },
      ]),
    );
    fireEvent.keyDown(window, { key: 'a' });
    await waitFor(() =>
      expect(useJobStore.getState().job?.decided).toBe(review.length),
    );
  });

  it('Enter on a focused Skip button skips, not accepts', async () => {
    const { backend, review } = await planned();
    const spy = vi.spyOn(backend, 'decideFill');
    const dialog = await openQueue();
    await within(dialog).findByRole('heading', {
      level: 3,
      name: review[0]!.name,
    });
    const items = await backend.fillJobItems(
      useJobStore.getState().job!.id,
      'review',
    );
    within(dialog).getByRole('button', { name: 'Skip' }).focus();
    await userEvent.keyboard('{Enter}');
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy).toHaveBeenLastCalledWith(expect.any(Number), [
      { choice: 'skip', index: items[0]!.index },
    ]);
  });

  it('Enter on a focused Done button closes without deciding', async () => {
    const { backend, review } = await planned();
    const spy = vi.spyOn(backend, 'decideFill');
    const dialog = await openQueue();
    await within(dialog).findByRole('heading', {
      level: 3,
      name: review[0]!.name,
    });
    within(dialog).getByRole('button', { name: 'Done' }).focus();
    await userEvent.keyboard('{Enter}');
    await waitFor(() => expect(useViewStore.getState().overlay).toBe('wizard'));
    expect(spy).not.toHaveBeenCalled();
  });

  it('Enter on a focused candidate selects it without accepting', async () => {
    const { backend, review } = await planned();
    const spy = vi.spyOn(backend, 'decideFill');
    const dialog = await openQueue();
    await within(dialog).findByRole('heading', {
      level: 3,
      name: review[0]!.name,
    });
    const group = within(dialog).getByRole('group', { name: 'Candidates' });
    const second = within(group).getAllByRole('button')[1]!;
    second.focus();
    await userEvent.keyboard('{Enter}');
    expect(second).toHaveAttribute('aria-pressed', 'true');
    expect(
      within(dialog).getByRole('heading', { level: 3, name: review[0]!.name }),
    ).toBeInTheDocument();
    expect(spy).not.toHaveBeenCalled();
  });

  it('ignores auto-repeated decision keys', async () => {
    const { backend, review } = await planned();
    const spy = vi.spyOn(backend, 'decideFill');
    const dialog = await openQueue();
    await within(dialog).findByRole('heading', {
      level: 3,
      name: review[0]!.name,
    });
    fireEvent.keyDown(window, { key: 'Enter', repeat: true });
    fireEvent.keyDown(window, { key: 's', repeat: true });
    fireEvent.keyDown(window, { key: 'a', repeat: true });
    await act(async () => {});
    expect(spy).not.toHaveBeenCalled();
  });

  it('leaves the up and down arrows to scroll the dialog', async () => {
    const { review } = await planned();
    const dialog = await openQueue();
    await within(dialog).findByRole('heading', {
      level: 3,
      name: review[0]!.name,
    });
    expect(fireEvent.keyDown(window, { key: 'ArrowDown' })).toBe(true);
    expect(fireEvent.keyDown(window, { key: 'ArrowUp' })).toBe(true);
    expect(
      within(dialog).getByText(`1 of ${review.length} · 0 decided`),
    ).toBeInTheDocument();
  });

  it('says there is nothing to review when there is no job', async () => {
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
    resetLibraryStore();
    useJobStore.getState().reset();
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
    render(
      <I18nProvider locale="en">
        <App />
      </I18nProvider>,
    );
    const dialog = await openQueue();
    expect(
      within(dialog).getByText('There is no fill job.'),
    ).toBeInTheDocument();
    expect(within(dialog).queryByText('Loading…')).toBeNull();
  });

  it('a double Enter decides one item, not two', async () => {
    const { backend, review } = await planned();
    const spy = vi.spyOn(backend, 'decideFill');
    const dialog = await openQueue();
    await within(dialog).findByRole('heading', {
      level: 3,
      name: review[0]!.name,
    });
    fireEvent.keyDown(window, { key: 'Enter' });
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(
      await within(dialog).findByRole('heading', {
        level: 3,
        name: review[1]!.name,
      }),
    ).toBeInTheDocument();
    await waitFor(() => expect(useJobStore.getState().pending).toBe(false));
    expect(spy).toHaveBeenCalledTimes(1);
    expect(
      within(dialog).getByText(`2 of ${review.length} · 1 decided`),
    ).toBeInTheDocument();
  });

  it('decisions feed the run', async () => {
    const { view, review } = await planned();
    const dialog = await openQueue();
    await within(dialog).findByRole('heading', {
      level: 3,
      name: review[0]!.name,
    });
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Accept all remaining' }),
    );
    await waitFor(() =>
      expect(useJobStore.getState().job?.decided).toBe(review.length),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Done' }));
    const wizard = await screen.findByRole('dialog', {
      name: 'Fill missing art',
    });
    const auto = view.entries.filter(
      (e) => e.art.cover === 'missing' && mockClass(e.id, 'cover') === 'auto',
    );
    await userEvent.click(
      within(wizard).getByRole('button', {
        name: `Run (${auto.length + review.length})`,
      }),
    );
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const written = auto.filter((e) => mockFailure(e.id) === null).length;
    expect(useJobStore.getState().job?.result?.filled).toBe(
      written + review.length,
    );
  });

  it('renders_hostile_names_as_text', async () => {
    const { backend } = await planned();
    const hostile = '<img src=x onerror=alert(1)>';
    const item: JobItemView = {
      index: 0,
      entry_id: null,
      folder: 'SFC',
      name: hostile,
      media: 'cover',
      action: 'review',
      confidence: 0.8,
      note: 'unreleased-variant',
      none_reason: null,
      candidates: [
        {
          index: 0,
          source: 'libretro',
          name: `${hostile} (USA) (Beta)`,
          regions: ['usa'],
          kind: 'title-exact',
          score: 95,
          confidence: 0.95,
        },
      ],
      replaces_existing: false,
      decision: null,
      outcome: null,
    };
    vi.spyOn(backend, 'fillJobItems').mockResolvedValue([item]);
    const dialog = await openQueue();
    expect(
      await within(dialog).findByRole('heading', { level: 3, name: hostile }),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(`${hostile} (USA) (Beta)`),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(/beta, prototype, hack or unlicensed/),
    ).toBeInTheDocument();
    expect(document.querySelector('img[src="x"]')).toBeNull();
    expect(document.querySelector('[onerror]')).toBeNull();
  });

  it('speaks Spanish', async () => {
    await planned('es');
    const dialog = await openQueue('Revisar coincidencias dudosas');
    expect(
      within(dialog).getByRole('button', { name: 'Aceptar el resto' }),
    ).toBeInTheDocument();
  });
});
