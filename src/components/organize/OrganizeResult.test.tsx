import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import type { JobHistoryItem, OrganizeJobView } from '@/ipc/types';

import { OrganizeResult } from './OrganizeResult';

function view(patch: Partial<OrganizeJobView> = {}): OrganizeJobView {
  return {
    job: 'organize-0001',
    plan_id: 1,
    state: 'done',
    progress: null,
    target_root: '/Volumes/SD',
    layout: 'anbernic-stock',
    error: null,
    report: {
      job: 'organize-0001',
      moved: 1,
      copied: 1,
      skipped: 1,
      failed: 1,
      paused: null,
      pause_reason: null,
      resumable_items: [],
      items: [
        { item: 0, status: 'moved', targets: ['/Volumes/SD/ROMS/GBA/a.gba'] },
        {
          item: 1,
          status: 'copied-not-removed',
          targets: ['/Volumes/SD/ROMS/GB/b.gb'],
        },
        {
          item: 2,
          status: 'skipped',
          reason: 'changed-since-plan',
          targets: [],
        },
        {
          item: 3,
          status: 'failed',
          kind: 'write-denied',
          detail: 'denied',
          targets: [],
        },
      ],
    },
    ...patch,
  };
}

function show(
  job: OrganizeJobView,
  error: BackendError | null = null,
  handlers = { onOpen: vi.fn(), onUndo: vi.fn(), onResume: vi.fn() },
) {
  render(
    <I18nProvider locale="en">
      <OrganizeResult
        job={job}
        plan={null}
        undo={null}
        error={error}
        {...handlers}
      />
    </I18nProvider>,
  );
  return handlers;
}

describe('OrganizeResult', () => {
  let b: MockBackend;
  beforeEach(() => {
    b = createMockBackend({ delayMs: 0, now: FIXTURE_NOW, storage: null });
    setBackend(b);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    setBackend(null);
  });

  it('labels every outcome and counts copies that kept their original', async () => {
    show(view());
    expect(
      screen.getByText('Moved: 1 · Copied: 1 · Skipped: 1 · Errors: 1'),
    ).toBeInTheDocument();
    expect(
      screen.getByText('Copied without removing the original: 1'),
    ).toBeInTheDocument();
    // The per-file outcomes wait behind a closed disclosure.
    const details = screen.getByRole('button', {
      name: 'What happened to each file',
    });
    expect(details).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText('a.gba · Moved')).not.toBeInTheDocument();
    await userEvent.click(details);
    expect(details).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('a.gba · Moved')).toBeInTheDocument();
    expect(
      screen.getByText("b.gb · Copied; the original couldn't be removed"),
    ).toBeInTheDocument();
    expect(
      screen.getByText('#2 · Skipped: changed since the proposal'),
    ).toBeInTheDocument();
    expect(screen.getByText('#3 · Error: Write denied')).toBeInTheDocument();
  });

  it('a stopped job says what stays and still offers open and undo', async () => {
    const h = show(view({ state: 'cancelled' }));
    expect(
      screen.getByText(
        'Stopped. What was already organised stays; Undo puts it back.',
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Undo 2 files' }));
    expect(h.onUndo).toHaveBeenCalledWith(false);
    await userEvent.click(screen.getByRole('button', { name: 'Open library' }));
    expect(h.onOpen).toHaveBeenCalled();
  });

  it('a failed job without a journal offers neither open nor undo', () => {
    show(view({ state: 'failed', job: null, report: null }));
    expect(
      screen.queryByRole('button', { name: 'Open library' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /^Undo( \d+ files)?$/ }),
    ).not.toBeInTheDocument();
  });

  it('an undo of several jobs counts the files of every organise job', () => {
    const job = (id: string, restored: number, modified: number) => ({
      job: id,
      removed: [],
      restored: Array.from({ length: restored }, (_, i) => `/r/${id}/${i}`),
      skipped_modified: Array.from({ length: modified }, (_, i) => `/m/${i}`),
      skipped_occupied: [],
      dirs_removed: [],
    });
    render(
      <I18nProvider locale="en">
        <OrganizeResult
          job={view()}
          plan={null}
          undo={{
            report: [
              job('fill-0004', 9, 9),
              job('organize-0002', 2, 1),
              job('organize-0001', 3, 0),
            ],
            later_jobs: [
              {
                job: 'fill-0004',
                root: '/Volumes/SD',
                created_at: '2026-10-17T10:40:00Z',
                entries: 18,
                complete: true,
                undone: true,
                kind: 'fill',
                undoable: true,
              },
              {
                job: 'organize-0002',
                root: '/Volumes/SD',
                created_at: '2026-10-17T10:32:00Z',
                entries: 3,
                complete: true,
                undone: true,
                kind: 'organize',
                undoable: true,
              },
            ],
          }}
          error={null}
          onOpen={vi.fn()}
          onUndo={vi.fn()}
          onResume={vi.fn()}
        />
      </I18nProvider>,
    );
    expect(screen.getByRole('status')).toHaveTextContent(
      'Restored: 5 · Left alone because they changed: 1',
    );
  });

  it('names the later jobs by kind and date, and undoes them all on request', async () => {
    const later: JobHistoryItem = {
      job: 'fill-0003',
      root: '/Volumes/SD',
      created_at: '2026-10-17T10:32:00Z',
      entries: 4,
      complete: true,
      undone: false,
      kind: 'fill',
      undoable: true,
    };
    vi.spyOn(b, 'jobHistory').mockResolvedValue([later]);
    const h = show(view(), new BackendError('later-jobs', 'fill-0003, gone'));
    expect(
      await screen.findByText(/^These must be undone first: Covers .+, gone$/),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', { name: 'Undo all in order' }),
    );
    expect(h.onUndo).toHaveBeenCalledWith(true);
  });
});
