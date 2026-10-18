import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockOptions,
} from '@/ipc/mock/mockBackend';
import { BackendError } from '@/ipc/errors';
import { mockClass, mockFailure } from '@/ipc/mock/mockJobs';
import { DEFAULT_REGION_ORDER } from '@/lib/fill';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { useOrganizeStore } from '@/stores/organizeStore';
import type { ItemErrorKind } from '@/ipc/types';
import { stubLayout } from '@/test/layout';

import en from '../../../locales/en/ui.json';
import es from '../../../locales/es/ui.json';

async function running(opts: MockOptions = {}, locale: 'en' | 'es' = 'en') {
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
  await act(() => useJobStore.getState().run());
  return { backend, view };
}

async function openPanel(name = 'Jobs') {
  await userEvent.click(
    screen.getByRole('button', { name: new RegExp(`^${name}`) }),
  );
  return screen.findByRole('dialog', { name });
}

describe('Jobs panel', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('shows_the_pause_reason_and_resumes', async () => {
    await running({ pauseAfter: 3 });
    await waitFor(() =>
      expect(useJobStore.getState().job?.state).toBe('paused'),
    );
    const dialog = await openPanel();
    const banner = within(dialog).getByRole('alert');
    expect(banner).toHaveTextContent(
      'Paused: the art server is limiting requests.',
    );
    await userEvent.click(
      within(banner).getByRole('button', { name: 'Resume' }),
    );
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    expect(await within(dialog).findByText('Finished')).toBeInTheDocument();
  });

  it('lists per-item errors with their reasons', async () => {
    const { view } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const dialog = await openPanel();
    const failing = view.entries.filter(
      (e) =>
        e.art.cover === 'missing' &&
        mockClass(e.id, 'cover') === 'auto' &&
        mockFailure(e.id) !== null,
    );
    expect(failing.length).toBeGreaterThan(0);
    const list = await within(dialog).findByRole('list', {
      name: `Problems (${failing.length})`,
    });
    expect(within(list).getAllByRole('listitem')).toHaveLength(failing.length);
    const first = failing[0]!;
    const reasons: Partial<Record<ItemErrorKind, string>> = {
      'no-art': 'No art found',
      network: 'Network error',
      decode: 'Not a usable image',
      'name-invalid': 'Name not valid on SD cards',
    };
    const reason = reasons[mockFailure(first.id)!]!;
    expect(
      within(list).getAllByText(new RegExp(reason)).length,
    ).toBeGreaterThan(0);
  });

  it('undo asks for confirmation and restores the library', async () => {
    const { view } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const missing = view.entries.filter(
      (e) => e.art.cover === 'missing',
    ).length;
    const dialog = await openPanel();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Undo last job' }),
    );
    expect(within(dialog).getByText(/Undo the last job\?/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Undo' }));
    expect(await within(dialog).findByText(/^Undone: /)).toBeInTheDocument();
    expect(
      await screen.findByText(`${missing} without cover`),
    ).toBeInTheDocument();
    expect(within(dialog).getByText('Undone')).toBeInTheDocument();
  });

  it('undo_reports_kept_files', async () => {
    const { backend } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    vi.spyOn(backend, 'undoLastJob').mockResolvedValue({
      job: 'mock-0001',
      removed: 2,
      restored: 1,
      kept: 1,
      entries: [],
    });
    const dialog = await openPanel();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Undo last job' }),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Undo' }));
    expect(
      await within(dialog).findByText(
        'Undone: 2 removed, 1 restored, 1 kept because you changed them.',
      ),
    ).toBeInTheDocument();
  });

  it('cancels a running job', async () => {
    await running({ delayMs: 40 });
    const dialog = await openPanel();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Cancel' }),
    );
    await waitFor(() =>
      expect(useJobStore.getState().job?.state).toBe('cancelled'),
    );
    expect(await within(dialog).findByText('Cancelled')).toBeInTheDocument();
  });

  it('keeps a history of jobs', async () => {
    await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const dialog = await openPanel();
    const history = await within(dialog).findByRole('list', {
      name: 'History',
    });
    expect(within(history).getAllByRole('listitem')).toHaveLength(1);
    expect(within(history).getByText('Complete')).toBeInTheDocument();
  });

  it('says one file, not one files, in the history', async () => {
    const { backend } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const [real] = await backend.jobHistory();
    vi.spyOn(backend, 'jobHistory').mockResolvedValue([
      { ...real!, job: 'one', entries: 1 },
      { ...real!, job: 'two', entries: 2 },
    ]);
    const dialog = await openPanel();
    const history = await within(dialog).findByRole('list', {
      name: 'History',
    });
    await waitFor(() =>
      expect(within(history).getAllByRole('listitem')).toHaveLength(2),
    );
    expect(within(history).getByText('1 file')).toBeInTheDocument();
    expect(within(history).getByText('2 files')).toBeInTheDocument();
  });

  it('survives history entries with unreadable dates', async () => {
    const { backend } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const [real] = await backend.jobHistory();
    vi.spyOn(backend, 'jobHistory').mockResolvedValue([
      { ...real!, job: 'bad-1', created_at: '' },
      { ...real!, job: 'bad-2', created_at: 'not a date \u202e' },
      real!,
    ]);
    const dialog = await openPanel();
    const history = await within(dialog).findByRole('list', {
      name: 'History',
    });
    await waitFor(() =>
      expect(within(history).getAllByRole('listitem')).toHaveLength(3),
    );
    expect(within(history).getAllByText('Unknown date')).toHaveLength(2);
  });

  it('labels organise jobs and resumes an interrupted one', async () => {
    const { backend } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const [real] = await backend.jobHistory();
    vi.spyOn(backend, 'jobHistory').mockResolvedValue([
      {
        ...real!,
        job: 'organize-0002',
        root: '/Users/demo/Downloads/roms',
        kind: 'organize',
        complete: false,
        undone: false,
      },
      { ...real!, job: 'organize-0001', kind: 'organize', complete: true },
      real!,
    ]);
    const resume = vi
      .spyOn(backend, 'organizeResume')
      .mockRejectedValue(new BackendError('no-plan', 'organize-0002'));
    const dialog = await openPanel();
    const history = await within(dialog).findByRole('list', {
      name: 'History',
    });
    await waitFor(() =>
      expect(within(history).getAllByRole('listitem')).toHaveLength(3),
    );
    const [interrupted, organised, fill] =
      within(history).getAllByRole('listitem');
    expect(within(organised!).getByText('Organisation')).toBeInTheDocument();
    expect(within(fill!).getByText('Covers')).toBeInTheDocument();
    expect(
      within(organised!).queryByRole('button', { name: 'Resume' }),
    ).not.toBeInTheDocument();
    expect(within(interrupted!).getByText('Interrupted')).toBeInTheDocument();
    await userEvent.click(
      within(interrupted!).getByRole('button', { name: 'Resume' }),
    );
    await waitFor(() => expect(resume).toHaveBeenCalledWith('organize-0002'));
    expect(useNavStore.getState()).toMatchObject({
      screen: 'organize',
      organizeSource: '/Users/demo/Downloads/roms',
    });
    expect(useOrganizeStore.getState().source).toBe(
      '/Users/demo/Downloads/roms',
    );
    act(() => {
      useOrganizeStore.getState().reset();
      useNavStore.getState().closeOrganize();
    });
  });

  it('does_not_offer_undo_last_job_for_an_organise_job', async () => {
    // Like Rust: the library's undo is for fill jobs; an organise job is
    // undone from the organise screen (it checks later jobs first).
    const { backend } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const [real] = await backend.jobHistory();
    vi.spyOn(backend, 'jobHistory').mockResolvedValue([
      { ...real!, job: 'organize-0001', kind: 'organize', complete: true },
      { ...real!, undone: true },
    ]);
    const dialog = await openPanel();
    const history = await within(dialog).findByRole('list', {
      name: 'History',
    });
    await waitFor(() =>
      expect(within(history).getAllByRole('listitem')).toHaveLength(2),
    );
    expect(
      within(dialog).getByRole('button', { name: 'Undo last job' }),
    ).toBeDisabled();
  });

  it('undoes_an_organise_job_from_its_history_row_after_confirming', async () => {
    // Organise spec §4.3: undo is also available later from the job history.
    const { backend, view } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const [real] = await backend.jobHistory();
    const organised = {
      ...real!,
      job: 'organize-0001',
      kind: 'organize' as const,
      complete: true,
      undone: false,
    };
    const history = vi
      .spyOn(backend, 'jobHistory')
      .mockResolvedValue([organised, { ...real!, undone: true }]);
    const undo = vi.spyOn(backend, 'organizeUndo').mockResolvedValue({
      report: [
        {
          job: 'organize-0001',
          removed: [],
          restored: ['/a.gba', '/b.gba'],
          skipped_modified: ['/c.gba'],
          skipped_occupied: [],
          dirs_removed: [],
        },
      ],
      later_jobs: [],
    });
    const dialog = await openPanel();
    const list = await within(dialog).findByRole('list', { name: 'History' });
    await waitFor(() =>
      expect(within(list).getAllByRole('listitem')).toHaveLength(2),
    );
    const [row, fill] = within(list).getAllByRole('listitem');
    expect(
      within(fill!).queryByRole('button', { name: /^Undo/ }),
    ).not.toBeInTheDocument();
    const opener = within(row!).getByRole('button', {
      name: /^Undo the organisation of /,
    });
    await userEvent.click(opener);
    expect(undo).not.toHaveBeenCalled();
    // The confirmation interrupts, and focus starts on the safe action.
    let confirm = screen.getByRole('alertdialog', {
      name: /^Undo the organisation of .+\?$/,
    });
    expect(within(confirm).getByRole('button', { name: 'Keep' })).toHaveFocus();
    // Keeping it changes nothing and gives focus back to the row.
    await userEvent.click(
      within(confirm).getByRole('button', { name: 'Keep' }),
    );
    expect(undo).not.toHaveBeenCalled();
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    expect(opener).toHaveFocus();
    await userEvent.click(opener);
    confirm = screen.getByRole('alertdialog', {
      name: /^Undo the organisation of .+\?$/,
    });
    expect(confirm).toHaveAccessibleDescription(
      /files go back where they were/,
    );
    history.mockResolvedValue([
      { ...organised, undone: true },
      { ...real!, undone: true },
    ]);
    await userEvent.click(
      within(confirm).getByRole('button', { name: /^Undo \d+ files$/ }),
    );
    await waitFor(() =>
      expect(undo).toHaveBeenCalledWith('organize-0001', false),
    );
    expect(
      await within(dialog).findByText(
        'Restored: 2 · Left alone because they changed: 1',
      ),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(within(list).getAllByText('Undone')).toHaveLength(2),
    );
    // The open library is rescanned on close: its ROMs moved.
    const open = vi.spyOn(useLibraryStore.getState(), 'open');
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Close' }),
    );
    expect(open).toHaveBeenCalledWith(view.root, view.profile.id);
  });

  it('lists_the_later_jobs_and_undoes_all_in_order_from_the_history', async () => {
    const { backend } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const [real] = await backend.jobHistory();
    vi.spyOn(backend, 'jobHistory').mockResolvedValue([
      { ...real!, job: 'fill-0009', undone: false },
      {
        ...real!,
        job: 'organize-0001',
        kind: 'organize',
        complete: true,
        undone: false,
      },
    ]);
    const undo = vi
      .spyOn(backend, 'organizeUndo')
      .mockRejectedValueOnce(new BackendError('later-jobs', 'fill-0009'))
      .mockResolvedValueOnce({ report: [], later_jobs: [] });
    const dialog = await openPanel();
    const list = await within(dialog).findByRole('list', { name: 'History' });
    await waitFor(() =>
      expect(within(list).getAllByRole('listitem')).toHaveLength(2),
    );
    const row = within(list).getAllByRole('listitem')[1]!;
    await userEvent.click(
      within(row).getByRole('button', { name: /^Undo the organisation of / }),
    );
    await userEvent.click(
      within(screen.getByRole('alertdialog')).getByRole('button', {
        name: /^Undo \d+ files$/,
      }),
    );
    expect(
      await within(dialog).findByText(/^These must be undone first: Covers /),
    ).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Undo all in order' }),
    );
    await waitFor(() =>
      expect(undo).toHaveBeenLastCalledWith('organize-0001', true),
    );
  });

  it('moves focus into the undo confirmation and back', async () => {
    await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const dialog = await openPanel();
    const undo = within(dialog).getByRole('button', { name: 'Undo last job' });
    await userEvent.click(undo);
    expect(within(dialog).getByRole('button', { name: 'Keep' })).toHaveFocus();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Keep' }));
    expect(
      within(dialog).getByRole('button', { name: 'Undo last job' }),
    ).toHaveFocus();
  });

  it('keeps focus inside the dialog after undoing', async () => {
    await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const dialog = await openPanel();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Undo last job' }),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Undo' }));
    await within(dialog).findByText(/^Undone: /);
    await waitFor(() =>
      expect(dialog.contains(document.activeElement)).toBe(true),
    );
  });

  it('lists recorded item errors of a failed job', async () => {
    const { backend } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const job = useJobStore.getState().job!;
    const [item] = await backend.fillJobItems(job.id, 'failed');
    vi.spyOn(backend, 'fillJobItems').mockResolvedValue([
      { ...item!, index: 9001, name: 'Known.zip' },
      {
        ...item!,
        index: 9002,
        name: 'Unknown.zip',
        outcome: { status: 'failed', error: null, message: null },
      },
    ]);
    act(() =>
      useJobStore.setState({
        job: { ...job, state: 'failed', error: 'boom', result: null },
      }),
    );
    const dialog = await openPanel();
    const list = await within(dialog).findByRole('list', {
      name: 'Problems (2)',
    });
    const rows = within(list).getAllByRole('listitem');
    expect(rows[1]).toHaveTextContent('Unknown problem');
    expect(rows[1]).not.toHaveTextContent('Disk error');
  });

  it('speaks Spanish', async () => {
    await running({}, 'es');
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const dialog = await openPanel('Trabajos');
    expect(
      within(dialog).getByRole('button', {
        name: 'Deshacer el último trabajo',
      }),
    ).toBeInTheDocument();
  });
});

// Typecheck fails here when an ItemErrorKind is added without a label.
const KINDS: Record<ItemErrorKind, true> = {
  'no-art': true,
  network: true,
  decode: true,
  'write-denied': true,
  'disk-full': true,
  'name-invalid': true,
  'outside-root': true,
  'volume-gone': true,
  io: true,
};

describe('Item error labels', () => {
  it('both locales name every item error kind', () => {
    const kinds = Object.keys(KINDS).sort();
    expect(Object.keys(en.jobs.error).sort()).toEqual(kinds);
    expect(Object.keys(es.jobs.error).sort()).toEqual(kinds);
  });
});
