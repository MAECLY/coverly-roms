import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
} from '@/ipc/mock/mockBackend';
import { BackendError } from '@/ipc/errors';
import { auditSummary } from '@/lib/audit';
import { DEFAULT_REGION_ORDER } from '@/lib/fill';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { stubLayout } from '@/test/layout';

let backend: MockBackend;

async function setup(locale: 'en' | 'es' = 'en') {
  backend = createMockBackend({
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
  return useLibraryStore.getState().view!;
}

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

describe('Reports panel', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('summarises the audit per system and exports it', async () => {
    const view = await setup();
    await userEvent.click(screen.getByRole('button', { name: 'Reports' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reports' });
    expect(dialog).toHaveAccessibleDescription(
      'Missing art per system, and exports of the audit and the last fill job.',
    );
    const table = within(dialog).getByRole('table', { name: 'Art audit' });
    const first = auditSummary(view).systems[0]!;
    expect(
      within(table).getByRole('row', {
        name: new RegExp(`^${escapeRe(first.name)} ${first.roms} `),
      }),
    ).toBeInTheDocument();
    expect(
      within(table).getByRole('row', {
        name: new RegExp(`^Total ${view.entries.length} `),
      }),
    ).toBeInTheDocument();
    expect(within(dialog).getByText('No job has run yet.')).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Export audit (CSV)' }),
    );
    expect(
      await within(dialog).findByText('Saved as coverly-audit.csv.'),
    ).toBeInTheDocument();
    expect(backend.exports[0]?.name).toBe('coverly-audit.csv');
  });

  it('shows the system icon beside each audit row', async () => {
    const view = await setup();
    await userEvent.click(screen.getByRole('button', { name: 'Reports' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reports' });
    const table = within(dialog).getByRole('table', { name: 'Art audit' });
    const first = auditSummary(view).systems[0]!;
    const row = within(table).getByRole('row', {
      name: new RegExp(`^${escapeRe(first.name)} ${first.roms} `),
    });
    const icon = row.querySelector(
      `th [data-system-icon="${first.system_id ?? 'unknown'}"]`,
    );
    expect(icon).toHaveAttribute('aria-hidden', 'true');
    expect(within(row).getByText(first.name)).toBeInTheDocument();
    const total = within(table).getByRole('row', { name: /^Total / });
    expect(total.querySelector('[data-system-icon]')).toBeNull();
  });

  it('exports the last job report', async () => {
    const view = await setup();
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
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    await userEvent.click(screen.getByRole('button', { name: 'Reports' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reports' });
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Export report (JSON)' }),
    );
    const id = useJobStore.getState().job!.id;
    expect(
      await within(dialog).findByText(`Saved as coverly-job-${id}.json.`),
    ).toBeInTheDocument();
  });

  it('shows a failed job report export', async () => {
    const view = await setup();
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
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    vi.spyOn(backend, 'exportJobReport').mockRejectedValue(
      new BackendError('io', 'disk gone'),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Reports' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reports' });
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Export report (CSV)' }),
    );
    expect(await within(dialog).findByRole('alert')).toHaveTextContent(
      'disk gone',
    );
  });

  it('closes on Escape and returns focus to the Reports button', async () => {
    await setup();
    const opener = screen.getByRole('button', { name: 'Reports' });
    await userEvent.click(opener);
    await screen.findByRole('dialog', { name: 'Reports' });
    await userEvent.keyboard('{Escape}');

    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Reports' })).toHaveFocus(),
    );
  });

  it('speaks Spanish', async () => {
    await setup('es');
    await userEvent.click(screen.getByRole('button', { name: 'Informes' }));
    const dialog = await screen.findByRole('dialog', { name: 'Informes' });
    expect(
      within(dialog).getByRole('button', { name: 'Exportar auditoría (CSV)' }),
    ).toBeInTheDocument();
  });
});
