import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { OrganizePlan, OrganizeProgressEvent } from '@/ipc/types';
import { autoItem, discItem, planOf } from '@/test/organizeFixtures';

import { ApplyProgress } from './ApplyProgress';

type ExtractEvent = Extract<OrganizeProgressEvent, { phase: 'extract' }>;

/** Two disc games with a cartridge move between them (D6). */
const PLAN: OrganizePlan = planOf([
  discItem(3),
  autoItem(4, 'Tetris.gb', 'gb', 'ROMS/GB'),
  discItem(7),
]);

const extract = (extra: Partial<ExtractEvent> = {}): ExtractEvent => ({
  plan_id: 1,
  phase: 'extract',
  done: 2,
  total: 3,
  item: 7,
  bytes_done: 300,
  bytes_total: 1200,
  entries_done: 4,
  entries_total: 12,
  ...extra,
});

const systemName = (id: string) => (id === 'psx' ? 'PlayStation' : id);

function renderProgress(
  progress: OrganizeProgressEvent | null,
  plan: OrganizePlan | null = PLAN,
  onCancel = vi.fn(),
) {
  const ui = (p: OrganizeProgressEvent | null) => (
    <I18nProvider locale="es">
      <ApplyProgress
        plan={plan}
        progress={p}
        systemName={systemName}
        onCancel={onCancel}
      />
    </I18nProvider>
  );
  const r = render(ui(progress));
  return {
    ...r,
    onCancel,
    update: (p: OrganizeProgressEvent | null) => r.rerender(ui(p)),
  };
}

/** The innermost element whose whole text is `text` (values sit in mono spans). */
const byText = (text: string) =>
  screen.getByText(
    (_, el) =>
      el !== null &&
      el.textContent === text &&
      Array.from(el.children).every((c) => c.textContent !== text),
  );
const queryByText = (text: string) =>
  screen.queryByText(
    (_, el) =>
      el !== null &&
      el.textContent === text &&
      Array.from(el.children).every((c) => c.textContent !== text),
  );

let now = 0;
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('ApplyProgress while extracting', () => {
  it('says which game goes where, in one status sentence', () => {
    renderProgress(extract());
    const status = screen.getByRole('status');
    expect(status).toHaveTextContent(
      'Extrayendo Next Tetris, The.zip en /pile/ROMS/PS',
    );
    expect(screen.getAllByRole('status')).toHaveLength(1);
    // Name and folder in mono; the folder keeps its full path as a tooltip.
    const folder = within(status).getByTitle('/pile/ROMS/PS');
    expect(folder).toHaveClass('mono');
  });

  it('keeps «Juego n de N» apart from «Pista k de M»', () => {
    renderProgress(extract());
    expect(byText('Juego 2 de 2')).toBeInTheDocument();
    // The track being written: entries_done + 1.
    expect(byText('Pista 5 de 12')).toBeInTheDocument();
    expect(screen.queryByText(/de \d+ archivos…/)).not.toBeInTheDocument();
  });

  it('fills the bar by bytes and reads the amount as its value', () => {
    renderProgress(extract());
    const bar = screen.getByRole('progressbar');
    expect(bar).toHaveAttribute('aria-valuenow', '300');
    expect(bar).toHaveAttribute('aria-valuemax', '1200');
    expect(bar).toHaveAttribute('aria-valuetext', '300 B de 1,2 kB');
    expect(byText('300 B de 1,2 kB')).toBeInTheDocument();
  });

  it('draws the stage for the game and its system', () => {
    const { container } = renderProgress(extract());
    expect(
      screen.getByRole('img', {
        name: 'Extracción de Next Tetris, The.zip hacia la carpeta de PlayStation',
      }),
    ).toBeInTheDocument();
    expect(container.querySelector('[data-system-icon="psx"]')).not.toBeNull();
    expect(container.querySelector('.ws-box')).not.toBeNull();
  });

  it('lists the tracks behind a disclosure when the card is narrow', async () => {
    renderProgress(extract());
    const show = screen.getByRole('button', { name: 'Ver las 12 pistas' });
    expect(show).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(show);
    const list = screen.getByRole('list', {
      name: 'Pistas de Next Tetris, The.zip',
    });
    expect(within(list).getAllByRole('listitem')).toHaveLength(12);
  });

  it('measures before it gives a time, then shows speed and time left', () => {
    vi.useFakeTimers();
    now = 0;
    vi.spyOn(performance, 'now').mockImplementation(() => now);
    const total = 6_000_000_000;
    const { update } = renderProgress(
      extract({ bytes_done: 0, bytes_total: total }),
    );
    expect(screen.getByText('Calculando el tiempo…')).toBeInTheDocument();
    for (const done of [20e6, 40e6, 60e6]) {
      act(() => {
        now += 1000;
        vi.advanceTimersByTime(1000);
      });
      update(extract({ bytes_done: done, bytes_total: total }));
    }
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(byText('20 MB/s')).toBeInTheDocument();
    // (6 GB − 60 MB) at 20 MB/s ≈ 297 s.
    expect(byText('Quedan unos 5 minutos')).toBeInTheDocument();
    expect(screen.queryByText('Calculando el tiempo…')).not.toBeInTheDocument();
  });

  it('says what cancelling means under the button, and tells the button', () => {
    renderProgress(extract());
    const note = screen.getByText(
      /^Si cancelas, se borra lo extraído de este juego/,
    );
    expect(note).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Cancelar' }),
    ).toHaveAccessibleDescription(note.textContent ?? '');
  });

  it('shows no amount for an archive with nothing to decode', () => {
    renderProgress(extract({ bytes_done: 0, bytes_total: 0 }));
    expect(queryByText('0 B de 0 B')).not.toBeInTheDocument();
    expect(screen.getByRole('progressbar')).not.toHaveAttribute(
      'aria-valuenow',
    );
  });
});

describe('ApplyProgress checking', () => {
  it('switches the sentence and drops the time once every entry is staged', () => {
    renderProgress(extract({ entries_done: 12, bytes_done: 1200 }));
    expect(screen.getByRole('status')).toHaveTextContent(
      'Comprobando los archivos de Next Tetris, The.zip…',
    );
    expect(byText('Pista 12 de 12')).toBeInTheDocument();
    expect(screen.queryByText('Calculando el tiempo…')).not.toBeInTheDocument();
  });
});

describe('ApplyProgress cancelling', () => {
  it('disables the button, says so and calls cancel once', async () => {
    const { onCancel } = renderProgress(extract());
    await userEvent.click(screen.getByRole('button', { name: 'Cancelar' }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    const button = screen.getByRole('button', { name: 'Cancelando…' });
    expect(button).toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent(
      'Cancelando: se deshace lo extraído de Next Tetris, The.zip…',
    );
    expect(
      screen.queryByText(/^Si cancelas, se borra/),
    ).not.toBeInTheDocument();
    expect(screen.queryByText('Calculando el tiempo…')).not.toBeInTheDocument();
  });

  it('offers Cancel again when the cancel request fails', async () => {
    const onCancel = vi.fn(() => Promise.resolve(false));
    renderProgress(extract(), PLAN, onCancel);
    await userEvent.click(screen.getByRole('button', { name: 'Cancelar' }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    const again = await screen.findByRole('button', { name: 'Cancelar' });
    expect(again).toBeEnabled();
    expect(screen.getByRole('status')).toHaveTextContent(
      'Extrayendo Next Tetris, The.zip en /pile/ROMS/PS',
    );
  });

  it('stays cancelling once the request went through', async () => {
    const onCancel = vi.fn(() => Promise.resolve(true));
    renderProgress(extract(), PLAN, onCancel);
    await userEvent.click(screen.getByRole('button', { name: 'Cancelar' }));
    await act(async () => {});
    expect(screen.getByRole('button', { name: 'Cancelando…' })).toBeDisabled();
  });
});

describe('ApplyProgress moving', () => {
  it('keeps the move sentence and draws a stack of sleeves', () => {
    const { container } = renderProgress({
      plan_id: 1,
      phase: 'apply',
      done: 4,
      total: 16,
    });
    expect(screen.getByRole('status')).toHaveTextContent('Moviendo 4 de 16…');
    expect(
      screen.getByRole('img', { name: 'Moviendo a /pile' }),
    ).toBeInTheDocument();
    expect(container.querySelector('.ws-box')).toBeNull();
    expect(container.querySelector('[data-system-icon]')).toBeNull();
    const bar = screen.getByRole('progressbar');
    expect(bar).toHaveAttribute('aria-valuenow', '4');
    expect(bar).toHaveAttribute('aria-valuemax', '16');
    expect(screen.queryByText('Calculando el tiempo…')).not.toBeInTheDocument();
    expect(screen.queryByRole('list')).not.toBeInTheDocument();
  });

  it('labels the stage plainly when a resumed job has no plan', () => {
    renderProgress({ plan_id: 1, phase: 'apply', done: 4, total: 16 }, null);
    expect(
      screen.getByRole('img', { name: 'Progreso de la organización' }),
    ).toBeInTheDocument();
  });

  it('starts with an open bar before the first event', () => {
    renderProgress(null);
    expect(screen.getByRole('status')).toHaveTextContent('Empezando…');
    expect(screen.getByRole('progressbar')).not.toHaveAttribute(
      'aria-valuenow',
    );
  });
});

describe('ApplyProgress without a plan (a resumed job)', () => {
  it('names the item by id and counts tracks from the event only', () => {
    renderProgress(extract(), null);
    expect(screen.getByRole('status')).toHaveTextContent('Extrayendo #7…');
    expect(queryByText('Juego 2 de 2')).not.toBeInTheDocument();
    expect(screen.queryByText(/^Juego /)).not.toBeInTheDocument();
    // Without the plan nothing says it is a disc: entries are files.
    expect(byText('Archivo 5 de 12')).toBeInTheDocument();
    expect(screen.queryByRole('list')).not.toBeInTheDocument();
  });
});
