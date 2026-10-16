import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { getBackend, setBackend } from '@/ipc/backend';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockOptions,
} from '@/ipc/mock/mockBackend';
import { mockClass, mockFailure } from '@/ipc/mock/mockJobs';
import { useFilterStore } from '@/stores/filterStore';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { stubLayout } from '@/test/layout';

import { FillWizard } from './FillWizard';

async function setup(opts: MockOptions = {}, locale: 'en' | 'es' = 'en') {
  const backend = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    pauseAfter: null,
    ...opts,
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
  return { backend, view: useLibraryStore.getState().view! };
}

async function toPreview(dialog: HTMLElement, scope: RegExp) {
  await userEvent.click(within(dialog).getByRole('radio', { name: scope }));
  for (let i = 0; i < 3; i += 1) {
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
  }
  await userEvent.click(
    within(dialog).getByRole('button', { name: 'Preview (dry run)' }),
  );
}

describe('Fill wizard', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('walks the wizard to a dry-run summary and runs it', async () => {
    const { view } = await setup();
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await userEvent.click(
      within(dialog).getByRole('radio', { name: /Whole library/ }),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    expect(
      within(dialog).getByRole('checkbox', { name: 'Cover' }),
    ).toBeChecked();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    expect(
      within(dialog).getByRole('radio', { name: /ROM region/ }),
    ).toBeChecked();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Move USA up' }),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    expect(
      within(dialog).getByRole('radio', { name: 'Only missing art' }),
    ).toBeChecked();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Preview (dry run)' }),
    );
    const table = await within(dialog).findByRole('table', { name: 'Dry run' });
    const missing = view.entries.filter((e) => e.art.cover === 'missing');
    const auto = missing.filter((e) => mockClass(e.id, 'cover') === 'auto');
    expect(
      within(table).getByRole('row', {
        name: new RegExp(`^Total ${auto.length} `),
      }),
    ).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole('button', { name: `Run (${auto.length})` }),
    );
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const written = auto.filter((e) => mockFailure(e.id) === null).length;
    expect(
      await screen.findByText(`${missing.length - written} without cover`),
    ).toBeInTheDocument();
  });

  it('run_counts_accepted_review_items_but_not_skipped_ones', async () => {
    const { view } = await setup();
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await toPreview(dialog, /Whole library/);
    await within(dialog).findByRole('table', { name: 'Dry run' });
    const missing = view.entries.filter((e) => e.art.cover === 'missing');
    const auto = missing.filter(
      (e) => mockClass(e.id, 'cover') === 'auto',
    ).length;
    const job = useJobStore.getState().job!;
    const review = await (await getBackend()).fillJobItems(job.id, 'review');
    expect(review.length).toBeGreaterThan(2);
    await act(() =>
      useJobStore.getState().decide([
        { index: review[0]!.index, choice: 'accept' },
        { index: review[1]!.index, choice: 'skip' },
        { index: review[2]!.index, choice: 'pick', candidate: 1 },
      ]),
    );
    expect(
      await within(dialog).findByRole('button', { name: `Run (${auto + 2})` }),
    ).toBeInTheDocument();
  });

  it('sends the visible entries for the current filter', async () => {
    const { backend, view } = await setup();
    const spy = vi.spyOn(backend, 'startFill');
    act(() => useFilterStore.getState().setArt('missing-cover'));
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await toPreview(dialog, /Current filter/);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    const ids = [...(spy.mock.calls[0]![1].entry_ids ?? [])].sort(
      (a, b) => a - b,
    );
    expect(ids).toEqual(
      view.entries.filter((e) => e.art.cover === 'missing').map((e) => e.id),
    );
  });

  it('run_button_is_disabled_while_running', async () => {
    await setup({ delayMs: 40 });
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await toPreview(dialog, /Whole library/);
    expect(within(dialog).getByRole('status')).toHaveTextContent(
      'Looking for art',
    );
    expect(
      within(dialog).queryByRole('button', { name: /^Run/ }),
    ).not.toBeInTheDocument();
    const run = await within(dialog).findByRole(
      'button',
      { name: /^Run/ },
      { timeout: 5000 },
    );
    expect(run).toBeEnabled();
  });

  it('warns that Batocera gamelists are not updated', () => {
    const view = generateLibrary({
      seed: 3,
      count: 12,
      root: '/mock/bato',
      generation: 1,
      now: FIXTURE_NOW,
    });
    render(
      <I18nProvider locale="en">
        <FillWizard
          view={{ ...view, profile: { ...view.profile, id: 'batocera' } }}
          visible={view.entries}
          selectedId={null}
          onClose={() => undefined}
        />
      </I18nProvider>,
    );
    expect(screen.getByText(/gamelist\.xml/)).toBeInTheDocument();
  });

  it('speaks Spanish', async () => {
    await setup({}, 'es');
    await userEvent.click(
      screen.getByRole('button', { name: 'Rellenar arte que falta' }),
    );
    const dialog = screen.getByRole('dialog', {
      name: 'Rellenar arte que falta',
    });
    expect(
      within(dialog).getByRole('button', { name: 'Siguiente' }),
    ).toBeInTheDocument();
    expect(within(dialog).getByText('Qué rellenar')).toBeInTheDocument();
  });
});
