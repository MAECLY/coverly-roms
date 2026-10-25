import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
  type MockOptions,
} from '@/ipc/mock/mockBackend';
import type { DedupePlanView, OrganizeUndoView } from '@/ipc/types';
import { formatBytes } from '@/lib/dedupe';
import { useDedupeStore, type DedupeLaunch } from '@/stores/dedupeStore';
import { useNavStore } from '@/stores/navStore';

import { DuplicatesScreen } from './DuplicatesScreen';

const ROOT = MOCK_LIBRARIES[0].path;

function mock(options: MockOptions = {}): MockBackend {
  return createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: null,
    pauseAfter: null,
    ...options,
  });
}

/** Rewrites every plan the mock answers. */
function tweakPlans(b: MockBackend, f: (view: DedupePlanView) => void) {
  for (const method of ['dedupePlan', 'dedupeRegroup'] as const) {
    const real = b[method].bind(b) as (
      ...args: unknown[]
    ) => Promise<DedupePlanView>;
    vi.spyOn(b, method).mockImplementation(async (...args: unknown[]) => {
      const view = await real(...args);
      f(view);
      return view;
    });
  }
}

function renderScreen(launch?: DedupeLaunch) {
  if (launch) useDedupeStore.setState({ launch });
  useNavStore.getState().openDuplicates(ROOT);
  return render(
    <I18nProvider locale="es">
      <DuplicatesScreen launch={launch} />
    </I18nProvider>,
  );
}

const ready = () =>
  waitFor(() => expect(useDedupeStore.getState().status).toBe('ready'));
const plan = () => useDedupeStore.getState().plan!;

describe('DuplicatesScreen', () => {
  let b: MockBackend;
  beforeEach(() => {
    b = mock();
    setBackend(b);
    useDedupeStore.getState().reset();
  });
  afterEach(() => {
    useDedupeStore.getState().reset();
    useNavStore.getState().closeDuplicates();
    vi.restoreAllMocks();
    setBackend(null);
  });

  it('shows the summary sentence, the dry-run line and the shelves', async () => {
    renderScreen();
    await ready();
    expect(
      screen.getByText('89 copias de más en 88 juegos.'),
    ).toBeInTheDocument();
    expect(
      screen.getByText('Es una propuesta: todavía no se ha movido nada.'),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('group', { name: 'Ecco the Dolphin' }),
    ).toBeInTheDocument();
    // Report-only groups wait under "Sin cambios".
    expect(screen.getByText(/^Sin cambios \(\d+\)$/)).toBeInTheDocument();
  });

  it('sticky_bar_counts_files_and_bytes_out_of_roms', async () => {
    renderScreen();
    await ready();
    const bar = screen.getByRole('region', { name: 'Apartar 89 archivos' });
    expect(
      within(bar).getByRole('button', { name: 'Apartar 89 archivos' }),
    ).toBeEnabled();
    const bytes = formatBytes(plan().totals.bytes, 'es');
    const freed = bar.querySelector('.dup-bar__freed')!;
    expect(freed).toHaveTextContent(`Fuera de ROMS/: ${bytes}`);
    // Only the number is set in mono.
    expect(within(bar).getByText(bytes)).toHaveClass('dup-mono');
  });

  it('primary_disabled_with_reason_when_zero', async () => {
    renderScreen();
    await ready();
    for (const name of [
      /^Copias exactas/,
      /^Otros formatos/,
      /^Otras versiones/,
    ])
      await userEvent.click(screen.getByRole('button', { name }));
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: 'No hay nada que apartar' }),
      ).toBeDisabled(),
    );
    expect(
      screen.getByText('Con estas opciones ninguna copia sale de su sitio.'),
    ).toBeInTheDocument();
    // The options stay, so the levels can come back.
    expect(
      screen.getByRole('button', { name: /^Copias exactas/ }),
    ).toBeEnabled();
  });

  it('cmd_r_replans', async () => {
    renderScreen();
    await ready();
    const first = plan().plan_id;
    await userEvent.keyboard('{Meta>}r{/Meta}');
    await waitFor(() => expect(plan().plan_id).toBeGreaterThan(first));
    await ready();
  });

  it('picking another copy sends the pick back', async () => {
    const spy = vi.spyOn(b, 'dedupeSetKeeper');
    renderScreen();
    await ready();
    const ecco = screen.getByRole('group', { name: 'Ecco the Dolphin' });
    await userEvent.click(
      within(ecco).getByRole('radio', { name: /Quedarme con esta/ }),
    );
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(
      await within(ecco).findByText('Se queda: lo has elegido tú.'),
    ).toBeInTheDocument();
  });

  it('bulk_confirm_above_500_files', async () => {
    tweakPlans(b, (v) => {
      v.totals = { ...v.totals, extras: 600 };
    });
    renderScreen();
    await ready();
    await userEvent.click(
      screen.getByRole('button', { name: 'Apartar 600 archivos' }),
    );
    const dialog = await screen.findByRole('alertdialog', {
      name: '¿Apartar 600 archivos?',
    });
    expect(within(dialog).getByText(/de ellos en GBA\//)).toBeInTheDocument();
    expect(
      within(dialog).getByText(/Irán a _Duplicados\//),
    ).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Apartar 600 archivos' }),
    );
    await waitFor(() =>
      expect(useDedupeStore.getState().status).not.toBe('ready'),
    );
  });

  it('asks above a quarter of the library bytes and on another volume', async () => {
    tweakPlans(b, (v) => {
      v.same_volume = false;
    });
    renderScreen({ libraryBytes: null });
    await ready();
    await userEvent.click(
      screen.getByRole('button', { name: 'Apartar 89 archivos' }),
    );
    const dialog = await screen.findByRole('alertdialog');
    expect(within(dialog).getByText(/está en otro disco/)).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Cancelar' }),
    );
    expect(useDedupeStore.getState().status).toBe('ready');
  });

  it('an ordinary plan applies without asking', async () => {
    renderScreen({ libraryBytes: 1e12 });
    await ready();
    await userEvent.click(
      screen.getByRole('button', { name: 'Apartar 89 archivos' }),
    );
    expect(screen.queryByRole('alertdialog')).toBeNull();
    await waitFor(() => expect(useDedupeStore.getState().status).toBe('done'));
  });

  it('trash_confirm_names_count_bytes_and_says_undo_brings_them_back', async () => {
    renderScreen();
    await ready();
    await userEvent.click(
      screen.getByRole('radio', { name: 'Enviar a la Papelera' }),
    );
    const apply = await screen.findByRole('button', {
      name: 'Enviar 89 archivos a la Papelera',
    });
    await userEvent.click(apply);
    const dialog = await screen.findByRole('alertdialog', {
      name: '¿Enviar 89 archivos a la Papelera?',
    });
    const bytes = formatBytes(plan().totals.bytes, 'es');
    expect(
      within(dialog).getByText(
        `Son ${bytes}. Deshacer los devuelve desde la Papelera mientras no la vacíes.`,
      ),
    ).toBeInTheDocument();
    // The fixture library is a card: its Trash is on the card.
    expect(
      within(dialog).getByText(/la Papelera está en la propia tarjeta/),
    ).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole('button', {
        name: 'Enviar 89 archivos a la Papelera',
      }),
    );
    await waitFor(() => expect(useDedupeStore.getState().status).toBe('done'));
    expect(
      screen.getByText('89 archivos enviados a la Papelera.'),
    ).toBeInTheDocument();
  });

  it('trash_option_disabled_when_the_volume_has_no_trash', async () => {
    b.dedupeScenario({ trashAvailable: false });
    renderScreen();
    await ready();
    expect(
      screen.getByRole('radio', { name: 'Enviar a la Papelera' }),
    ).toBeDisabled();
    expect(
      screen.getByText(
        'Este disco no tiene Papelera: las copias solo pueden apartarse.',
      ),
    ).toBeInTheDocument();
  });

  it('same_volume_done_state_says_the_card_is_as_full', async () => {
    renderScreen();
    await ready();
    await userEvent.click(
      screen.getByRole('button', { name: 'Apartar 89 archivos' }),
    );
    expect(
      await screen.findByText(
        '89 archivos apartados en _Duplicados/. Deshacer está a un clic.',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/La tarjeta sigue igual de llena hasta que vacíes/),
    ).toBeInTheDocument();
  });

  it('undo from the result puts the files back', async () => {
    renderScreen();
    await ready();
    await userEvent.click(
      screen.getByRole('button', { name: 'Apartar 89 archivos' }),
    );
    await screen.findByText(/89 archivos apartados/);
    await userEvent.click(screen.getByRole('button', { name: 'Deshacer' }));
    const dialog = await screen.findByRole('alertdialog', {
      name: '¿Devolver 89 archivos a su sitio?',
    });
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Devolver 89 archivos' }),
    );
    expect(
      await screen.findByText(/devueltos a su sitio\./),
    ).toBeInTheDocument();
  });

  it('undo_counts_the_copies_not_the_art_that_came_with_them', async () => {
    // The fixture's Ecco copy carries its cover: 89 copies set aside
    // must read as 89 back, not 90.
    renderScreen();
    await ready();
    await applyThenUndo();
    expect(
      await screen.findByText('89 archivos devueltos a su sitio.'),
    ).toBeInTheDocument();
  });

  /** Applies the plan on screen and confirms its undo. */
  async function applyThenUndo() {
    await userEvent.click(
      screen.getByRole('button', { name: 'Apartar 89 archivos' }),
    );
    await screen.findByText(/89 archivos apartados/);
    await userEvent.click(screen.getByRole('button', { name: 'Deshacer' }));
    const dialog = await screen.findByRole('alertdialog', {
      name: '¿Devolver 89 archivos a su sitio?',
    });
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Devolver 89 archivos' }),
    );
  }

  it('undo_says_which_copies_did_not_come_back', async () => {
    renderScreen();
    await ready();
    vi.spyOn(b, 'organizeUndo').mockImplementation(
      async (job): Promise<OrganizeUndoView> => ({
        report: [
          {
            job,
            removed: [],
            restored: [`${ROOT}/ROMS/GB/Tetris (1).gb`],
            skipped_modified: [],
            skipped_occupied: [`${ROOT}/ROMS/GB/Zelda (1).gb`],
            dirs_removed: [],
            archive_lost: [`${ROOT}/ROMS/GBA/Metroid (1).zip`],
          },
        ],
        later_jobs: [],
      }),
    );
    await applyThenUndo();
    // Only what came back is counted as back...
    expect(
      await screen.findByText('1 archivo devuelto a su sitio.'),
    ).toBeInTheDocument();
    // ...and what did not is named, never passed over in silence.
    expect(
      screen.getByText(
        `No estaba en la Papelera: ${ROOT}/ROMS/GBA/Metroid (1).zip`,
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/1 archivo no ha vuelto a su sitio/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(`${ROOT}/ROMS/GB/Zelda (1).gb`),
    ).toBeInTheDocument();
  });

  it('undo_after_a_later_job_offers_undo_all_in_order', async () => {
    renderScreen();
    await ready();
    const real = b.organizeUndo.bind(b);
    const spy = vi
      .spyOn(b, 'organizeUndo')
      .mockImplementation(async (job, includeLater) => {
        if (!includeLater) throw new BackendError('later-jobs', 'job-later');
        return real(job, false);
      });
    await applyThenUndo();
    expect(
      await screen.findByText('Primero hay que deshacer: job-later'),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', { name: 'Deshacer todo en orden' }),
    );
    await waitFor(() =>
      expect(spy).toHaveBeenLastCalledWith(expect.any(String), true),
    );
    expect(
      await screen.findByText(/devueltos a su sitio\./),
    ).toBeInTheDocument();
  });

  it('leaving while planning stops the plan', async () => {
    b = mock({ delayMs: 30 });
    setBackend(b);
    const cancel = vi.spyOn(b, 'dedupeCancel');
    renderScreen();
    await waitFor(() =>
      expect(useDedupeStore.getState().progress).not.toBeNull(),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Biblioteca' }));
    await waitFor(() => expect(cancel).toHaveBeenCalled());
    expect(useNavStore.getState().screen).toBe('main');
  });

  it('a paused apply offers to resume', async () => {
    b = mock({ dedupePauseAfter: 5 });
    setBackend(b);
    renderScreen();
    await ready();
    await userEvent.click(
      screen.getByRole('button', { name: 'Apartar 89 archivos' }),
    );
    await userEvent.click(
      await screen.findByRole('button', { name: 'Continuar' }),
    );
    // The resume sets aside what the pause left (84 of 89).
    expect(
      await screen.findByText(
        '84 archivos apartados en _Duplicados/. Deshacer está a un clic.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Continuar' })).toBeNull();
  });

  it('export_report_writes_json', async () => {
    renderScreen();
    await ready();
    await userEvent.click(
      screen.getByRole('button', { name: 'Exportar informe' }),
    );
    expect(
      await screen.findByText('Informe guardado: coverly-duplicates-plan.json'),
    ).toBeInTheDocument();
    const saved = JSON.parse(b.exports.at(-1)!.text) as DedupePlanView;
    expect(saved.groups.length).toBe(plan().groups.length);
  });

  it('off_card_saves_note_once_per_system', async () => {
    tweakPlans(b, (v) => {
      v.off_card_saves = ['GB', 'GB', 'PS'];
    });
    renderScreen();
    await ready();
    expect(screen.getAllByText(/Si juegas a/)).toHaveLength(2);
  });

  it('chd_unread_note', async () => {
    tweakPlans(b, (v) => {
      v.chd_unread = 24;
    });
    renderScreen();
    await ready();
    expect(
      screen.getByText('Otros formatos: 24 CHD sin leer por dentro.'),
    ).toBeInTheDocument();
  });

  it('empty_state_lists_systems_without_catalogue', async () => {
    b.dedupeScenario({ empty: true });
    renderScreen({ systems: 55 });
    await ready();
    expect(
      screen.getByText('No hay duplicados en 55 sistemas.'),
    ).toBeInTheDocument();
    expect(screen.getByText('Este estante está en orden.')).toBeInTheDocument();
    expect(
      screen.getByText(
        'Otras versiones: sin catálogo para 3 sistemas (NDS, PSP, N64).',
      ),
    ).toBeInTheDocument();
  });

  it('error_state_says_nothing_moved', async () => {
    b.dedupeScenario({ failPlan: 'volume-gone' });
    renderScreen();
    await waitFor(() => expect(useDedupeStore.getState().status).toBe('error'));
    const alert = screen.getByRole('alert');
    expect(
      within(alert).getByText('No se ha movido nada.'),
    ).toBeInTheDocument();
    b.dedupeScenario({ failPlan: null });
    await userEvent.click(
      within(alert).getByRole('button', { name: 'Volver a intentarlo' }),
    );
    await ready();
  });

  it('the conflicts alert counts them and shows the first', async () => {
    renderScreen();
    await ready();
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    expect(
      screen.getByText(
        '1 juego tiene copias con partidas distintas. No se tocará nada en él.',
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Ver' }));
    expect(scroll).toHaveBeenCalled();
  });

  it('back returns to the library', async () => {
    renderScreen();
    await ready();
    await userEvent.click(screen.getByRole('button', { name: 'Biblioteca' }));
    expect(useNavStore.getState().screen).toBe('main');
    expect(useDedupeStore.getState().status).toBe('idle');
  });
});
