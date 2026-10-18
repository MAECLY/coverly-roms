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
import { useSelectionStore } from '@/stores/selectionStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

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
    resetSettingsStore();
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
    const regions = within(dialog).getByRole('list', { name: 'Region order' });
    const before = within(regions)
      .getAllByRole('listitem')
      .findIndex((li) => li.textContent?.includes('USA'));
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Move USA up' }),
    );
    // The reorder is announced: the button moves away from under focus.
    expect(within(dialog).getByRole('status')).toHaveTextContent(
      `USA: position ${before} of `,
    );
    expect(
      within(regions)
        .getAllByRole('listitem')
        [before - 1]?.textContent?.includes('USA'),
    ).toBe(true);
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

  it('shows the system icon beside each folder in the dry-run summary', async () => {
    const { view } = await setup();
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await toPreview(dialog, /Whole library/);
    const table = await within(dialog).findByRole('table', { name: 'Dry run' });
    const heads = within(table)
      .getAllByRole('rowheader')
      .filter((th) => th.textContent !== 'Total');
    expect(heads.length).toBeGreaterThan(0);
    for (const th of heads) {
      // A fallback mount carries text of its own; read the label's name.
      const folder = th.querySelector('.system-label__name')?.textContent;
      const sys = view.systems.find((s) => s.folder === folder);
      expect(sys).toBeDefined();
      expect(
        th.querySelector(`[data-system-icon="${sys!.system_id ?? 'unknown'}"]`),
      ).toHaveAttribute('aria-hidden', 'true');
    }
  });

  it('explains popularity and says when SteamGridDB votes cannot be used', async () => {
    await setup();
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await userEvent.click(
      within(dialog).getByRole('radio', { name: /Whole library/ }),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    expect(
      within(dialog).queryByText(/SteamGridDB is not ready/),
    ).not.toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole('radio', { name: /^Popularity \(votes\)/ }),
    );
    // The hint describes the choice; it is not part of its name.
    expect(
      within(dialog).getByRole('radio', { name: 'Popularity (votes)' }),
    ).toHaveAccessibleDescription(/most SteamGridDB votes/);
    expect(
      await within(dialog).findByText(/SteamGridDB is not ready/),
    ).toBeInTheDocument();
  });

  it('keeps the scope disabled when it has nothing to fill', async () => {
    await setup();
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    expect(
      within(dialog).getByRole('radiogroup', { name: 'What to fill' }),
    ).toBeInTheDocument();
    const selected = within(dialog).getByRole('radio', {
      name: /Selected ROM/,
    });
    expect(selected).toBeDisabled();
    await userEvent.click(selected);
    expect(selected).not.toBeChecked();
  });

  it('closes on Escape', async () => {
    await setup();
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    screen.getByRole('dialog', { name: 'Fill missing art' });
    await userEvent.keyboard('{Escape}');
    await waitFor(() =>
      expect(
        screen.queryByRole('dialog', { name: 'Fill missing art' }),
      ).not.toBeInTheDocument(),
    );
  });

  it('does not warn about votes once SteamGridDB is enabled with a key', async () => {
    const { backend } = await setup();
    await backend.setCredential('steamgriddb-api-key', 'sk');
    await act(() =>
      useSettingsStore.getState().update({
        sources: {
          order: [
            { id: 'libretro', enabled: true },
            { id: 'steamgriddb', enabled: true },
            { id: 'screenscraper', enabled: false },
          ],
        },
      }),
    );
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await userEvent.click(
      within(dialog).getByRole('radio', { name: /Whole library/ }),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    await userEvent.click(
      within(dialog).getByRole('radio', { name: /^Popularity \(votes\)/ }),
    );
    // Give the readiness check time to answer before asserting it stays quiet.
    await waitFor(() =>
      expect(
        within(dialog).getByText(/most SteamGridDB votes/),
      ).toBeInTheDocument(),
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(
      within(dialog).queryByText(/SteamGridDB is not ready/),
    ).not.toBeInTheDocument();
  });

  it('explains popularity in Spanish', async () => {
    await setup({}, 'es');
    await userEvent.click(
      screen.getByRole('button', { name: 'Rellenar arte que falta' }),
    );
    const dialog = screen.getByRole('dialog', {
      name: 'Rellenar arte que falta',
    });
    await userEvent.click(
      within(dialog).getByRole('radio', { name: /Toda la biblioteca/ }),
    );
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Siguiente' }),
    );
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Siguiente' }),
    );
    await userEvent.click(
      within(dialog).getByRole('radio', { name: /^Popularidad \(votos\)/ }),
    );
    expect(
      await within(dialog).findByText(/SteamGridDB no está listo/),
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

    // Changing a decision keeps the decided count but moves the Run count.
    await act(() =>
      useJobStore
        .getState()
        .decide([{ index: review[0]!.index, choice: 'skip' }]),
    );
    expect(
      await within(dialog).findByRole('button', { name: `Run (${auto + 1})` }),
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

  it('notes that gamelist layouts update gamelist.xml with a backup', () => {
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
          view={{
            ...view,
            profile: { ...view.profile, id: 'batocera', gamelist: true },
          }}
          visible={view.entries}
          selectedId={null}
          onClose={() => undefined}
        />
      </I18nProvider>,
    );
    expect(
      screen.getByText(/gamelist\.xml in each system folder is updated too/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/does not update yet/)).not.toBeInTheDocument();
  });

  it('starts from the settings and asks for the overwrite choice when the setting is "ask"', async () => {
    await setup({ storage: new MemoryStorage() });
    await useSettingsStore.getState().load();
    await useSettingsStore.getState().update({
      art: { overwrite: 'ask', media: ['cover'] },
      matching: { strategy: 'priority' },
    });
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
    expect(
      within(dialog).getByRole('checkbox', { name: 'Title screen' }),
    ).not.toBeChecked();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    expect(
      within(dialog).getByRole('radio', { name: /Source priority/ }),
    ).toBeChecked();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    expect(
      within(dialog).getByRole('radio', { name: 'Only missing art' }),
    ).not.toBeChecked();
    expect(
      within(dialog).getByRole('radio', { name: /Also replace existing art/ }),
    ).not.toBeChecked();
    const preview = within(dialog).getByRole('button', {
      name: 'Preview (dry run)',
    });
    expect(preview).toBeDisabled();
    await userEvent.click(
      within(dialog).getByRole('radio', { name: 'Only missing art' }),
    );
    expect(preview).toBeEnabled();
  });

  it('runs a plan with nothing to review by itself when dry-run-first is off', async () => {
    const { view } = await setup({ storage: new MemoryStorage() });
    await useSettingsStore.getState().load();
    await useSettingsStore.getState().update({
      safety: { dry_run_default: false },
    });
    const entry = view.entries.find(
      (e) => e.art.cover === 'missing' && mockClass(e.id, 'cover') === 'auto',
    )!;
    act(() => useSelectionStore.getState().select(entry.id));
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await toPreview(dialog, /Selected ROM/);
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
  });

  it('keeps the dry run waiting when dry-run-first is on', async () => {
    const { view } = await setup({ storage: new MemoryStorage() });
    await useSettingsStore.getState().load();
    const entry = view.entries.find(
      (e) => e.art.cover === 'missing' && mockClass(e.id, 'cover') === 'auto',
    )!;
    act(() => useSelectionStore.getState().select(entry.id));
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await toPreview(dialog, /Selected ROM/);
    expect(
      await within(dialog).findByRole('button', { name: /^Run \(/ }),
    ).toBeInTheDocument();
    expect(useJobStore.getState().job?.state).toBe('planned');
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
