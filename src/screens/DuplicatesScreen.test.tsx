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
import { fileName, formatBytes } from '@/lib/dedupe';
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

  it('the result reveals the duplicates folder in the words of each OS', async () => {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue(
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36',
    );
    renderScreen();
    await ready();
    const quarantine = plan().quarantine;
    await userEvent.click(
      screen.getByRole('button', { name: 'Apartar 89 archivos' }),
    );
    await screen.findByText(/89 archivos apartados/);
    await userEvent.click(
      screen.getByRole('button', {
        name: 'Mostrar _Duplicados en el Explorador',
      }),
    );
    await waitFor(() => expect(b.revealedPaths).toEqual([quarantine]));
  });

  it('a copy reveals its own file from the shelf', async () => {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue(
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15',
    );
    renderScreen();
    await ready();
    const g = plan().groups.find((x) => x.title === 'Ecco the Dolphin')!;
    const keeper = g.members.find((m) => m.id === g.keeper)!;
    await userEvent.click(
      screen.getByRole('button', {
        name: 'Mostrar Ecco the Dolphin.sms en Finder',
      }),
    );
    await waitFor(() => expect(b.revealedPaths).toEqual([keeper.unit.primary]));
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

  it('undo_does_not_count_the_game_list_it_restored_as_a_copy', async () => {
    // Undo restores an edited gamelist.xml byte for byte and lists it in
    // `restored`: it is not a copy coming back.
    const list = `${ROOT}/ROMS/SMS/gamelist.xml`;
    tweakPlans(b, (view) => {
      const extra = view.groups[0]!.members.find(
        (m) => m.id !== view.groups[0]!.keeper,
      )!;
      extra.gamelist = { gamelist: list, path: './extra.sms' };
    });
    const real = b.organizeUndo.bind(b);
    vi.spyOn(b, 'organizeUndo').mockImplementation(async (job, later) => {
      const view = await real(job, later);
      view.report[0]!.restored.push(list);
      return view;
    });
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
    // The resume finishes the job; its report covers the whole job, the
    // 5 set aside before the pause too (Rust's `finish_started`).
    expect(
      await screen.findByText(
        '89 archivos apartados en _Duplicados/. Deshacer está a un clic.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Continuar' })).toBeNull();
  });

  it('a job resumed from the jobs panel ends on its result, and undo counts the copies', async () => {
    // The jobs panel resumes by journal: the screen opens with no plan.
    b = mock({ dedupePauseAfter: 5 });
    setBackend(b);
    renderScreen();
    await ready();
    await userEvent.click(
      screen.getByRole('button', { name: 'Apartar 89 archivos' }),
    );
    await screen.findByRole('button', { name: 'Continuar' });
    const job = useDedupeStore.getState().job!.job!;
    await useDedupeStore.getState().resumeJob(job, ROOT);
    expect(useDedupeStore.getState().plan).toBeNull();
    expect(
      await screen.findByText(
        '89 archivos apartados en _Duplicados/. Deshacer está a un clic.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByText(/Fuera de la biblioteca:/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Deshacer' }));
    const dialog = await screen.findByRole('alertdialog', {
      name: '¿Devolver 89 archivos a su sitio?',
    });
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Devolver 89 archivos' }),
    );
    // The whole journal comes back, the Ecco cover not counted as a copy.
    expect(
      await screen.findByText('89 archivos devueltos a su sitio.'),
    ).toBeInTheDocument();
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

describe('DuplicatesScreen keys', () => {
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

  const MAC =
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15';
  const game = (title: string) => screen.getByRole('group', { name: title });
  const keeperRadio = (title: string) =>
    within(game(title))
      .getAllByRole('radio')
      .find((r) => r.getAttribute('data-state') === 'checked')!;
  const focusedGame = () =>
    (document.activeElement as HTMLElement | null)
      ?.closest('[data-dup-group]')
      ?.querySelector('h3')?.textContent ?? null;
  /** A key on whatever holds focus; `false` when it was prevented. */
  const press = (init: KeyboardEventInit) =>
    fireEvent.keyDown(document.activeElement ?? document.body, init);

  it('j_and_k_move_between_games_across_open_systems', async () => {
    renderScreen();
    await ready();
    // GB (open) holds a conflict and four games, GBA is closed, PS is open.
    act(() => keeperRadio('GB Game 03').focus());
    press({ key: 'j', code: 'KeyJ' });
    expect(focusedGame()).toBe('GB Game 04');
    expect(document.activeElement).toBe(keeperRadio('GB Game 04'));
    // The closed Game Boy Advance section is skipped.
    press({ key: 'j', code: 'KeyJ' });
    expect(focusedGame()).toBe('Crash Bash');
    press({ key: 'k', code: 'KeyK' });
    expect(focusedGame()).toBe('GB Game 04');
    // A non-Latin layout still has J and K.
    press({ key: 'л', code: 'KeyK' });
    expect(focusedGame()).toBe('GB Game 03');
    // A conflict has no radio: the group itself takes focus.
    press({ key: 'k', code: 'KeyK' });
    press({ key: 'k', code: 'KeyK' });
    press({ key: 'k', code: 'KeyK' });
    expect(focusedGame()).toBe('Pokemon Red');
    expect(document.activeElement).toBe(game('Pokemon Red'));
    // Nothing before the first game.
    press({ key: 'k', code: 'KeyK' });
    expect(focusedGame()).toBe('Pokemon Red');
  });

  it('j_k_need_no_modifier_and_stay_in_the_list', async () => {
    renderScreen();
    await ready();
    act(() => keeperRadio('GB Game 03').focus());
    press({ key: 'J', code: 'KeyJ', shiftKey: true });
    press({ key: 'j', code: 'KeyJ', metaKey: true });
    expect(focusedGame()).toBe('GB Game 03');
    // Outside the groups list (the action bar) J does nothing.
    const apply = screen.getByRole('button', { name: 'Apartar 89 archivos' });
    act(() => apply.focus());
    press({ key: 'j', code: 'KeyJ' });
    expect(document.activeElement).toBe(apply);
  });

  it('digits_keep_the_nth_copy_by_physical_key', async () => {
    const spy = vi.spyOn(b, 'dedupeSetKeeper');
    renderScreen();
    await ready();
    const g = plan().groups.find((x) => x.title === 'Crash Bash')!;
    act(() => keeperRadio('Crash Bash').focus());
    // AZERTY: Shift+é is the 2 key.
    press({ key: '2', code: 'Digit2', shiftKey: true });
    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith(
        expect.anything(),
        g.id,
        g.members[1]!.id,
      ),
    );
    expect(keeperRadio('Crash Bash')).toHaveAttribute(
      'value',
      String(g.members[1]!.id),
    );
    // Every radio advertises its number.
    const radios = within(game('Crash Bash')).getAllByRole('radio');
    expect(
      radios.map((r) => r.getAttribute('aria-keyshortcuts')).sort(),
    ).toEqual(['1', '2', '3']);
  });

  it('digits_beyond_the_copies_and_zero_do_nothing', async () => {
    const spy = vi.spyOn(b, 'dedupeSetKeeper');
    renderScreen();
    await ready();
    act(() => keeperRadio('Ecco the Dolphin').focus());
    press({ key: '9', code: 'Digit9' });
    press({ key: '0', code: 'Digit0' });
    press({ key: '3', code: 'Numpad3' });
    press({ key: '2', code: 'Digit2', ctrlKey: true });
    await new Promise((r) => setTimeout(r, 20));
    expect(spy).not.toHaveBeenCalled();
  });

  it('keys_are_ignored_while_typing', async () => {
    const spy = vi.spyOn(b, 'dedupeSetKeeper');
    renderScreen();
    await ready();
    const input = document.createElement('input');
    input.type = 'text';
    game('Ecco the Dolphin').appendChild(input);
    act(() => input.focus());
    press({ key: '2', code: 'Digit2' });
    press({ key: 'j', code: 'KeyJ' });
    await new Promise((r) => setTimeout(r, 20));
    expect(spy).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(input);
    input.remove();
  });

  it('the_reveal_chord_shows_the_focused_copy_and_never_reloads', async () => {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue(MAC);
    const replans = vi.spyOn(b, 'dedupePlan');
    renderScreen();
    await ready();
    const calls = replans.mock.calls.length;
    const g = plan().groups.find((x) => x.title === 'Ecco the Dolphin')!;
    const extra = g.members.find((m) => m.id !== g.keeper)!;
    const row = game('Ecco the Dolphin').querySelector<HTMLElement>(
      `[data-member="${extra.id}"]`,
    )!;
    act(() => within(row).getByRole('radio').focus());
    // ⌥R types «®» on macOS: the chord is matched by code.
    const passed = press({
      key: '®',
      code: 'KeyR',
      metaKey: true,
      altKey: true,
    });
    expect(passed).toBe(false);
    await waitFor(() => expect(b.revealedPaths).toEqual([extra.unit.primary]));
    expect(replans.mock.calls.length).toBe(calls);
    // Reveal buttons advertise the chord.
    expect(
      within(row).getByRole('button', { name: /en Finder$/ }),
    ).toHaveAttribute('aria-keyshortcuts', 'Meta+Alt+R');
  });

  it('cmd_enter_takes_the_apply_path_with_its_confirmation', async () => {
    tweakPlans(b, (v) => {
      v.totals = { ...v.totals, extras: 600 };
    });
    const apply = vi.spyOn(b, 'dedupeApply');
    renderScreen();
    await ready();
    expect(
      screen.getByRole('button', { name: 'Apartar 600 archivos' }),
    ).toHaveAttribute('aria-keyshortcuts', 'Meta+Enter Control+Enter');
    act(() => keeperRadio('Ecco the Dolphin').focus());
    expect(press({ key: 'Enter', code: 'Enter', metaKey: true })).toBe(false);
    const dialog = await screen.findByRole('alertdialog', {
      name: '¿Apartar 600 archivos?',
    });
    // Under the dialog the chord does nothing (its own buttons decide).
    fireEvent.keyDown(dialog, { key: 'Enter', code: 'Enter', metaKey: true });
    await new Promise((r) => setTimeout(r, 20));
    expect(apply).not.toHaveBeenCalled();
    expect(useDedupeStore.getState().status).toBe('ready');
  });

  it('ctrl_enter_applies_directly_when_nothing_needs_asking_and_only_once', async () => {
    const apply = vi.spyOn(b, 'dedupeApply');
    renderScreen({ libraryBytes: 1e12 });
    await ready();
    act(() => keeperRadio('Ecco the Dolphin').focus());
    press({ key: 'Enter', code: 'Enter', ctrlKey: true });
    expect(screen.queryByRole('alertdialog')).toBeNull();
    // Applying: the proposal and its chord are gone.
    fireEvent.keyDown(window, { key: 'Enter', code: 'Enter', ctrlKey: true });
    await waitFor(() => expect(useDedupeStore.getState().status).toBe('done'));
    expect(apply).toHaveBeenCalledTimes(1);
  });

  it('the legend lists the new keys', async () => {
    renderScreen();
    await ready();
    const legend = screen.getByRole('list', { name: 'Atajos de teclado' });
    for (const text of [
      'Juego siguiente o anterior',
      'Quedarme con la copia n',
      'Mostrar la copia en la carpeta',
      'Apartar',
    ])
      expect(within(legend).getByText(text)).toBeInTheDocument();
  });

  it('the_apply_key_in_the_legend_says_what_the_button_does', async () => {
    renderScreen();
    await ready();
    await userEvent.click(
      screen.getByRole('radio', { name: 'Enviar a la Papelera' }),
    );
    const legend = screen.getByRole('list', { name: 'Atajos de teclado' });
    expect(
      within(legend).getByText('Enviar a la Papelera'),
    ).toBeInTheDocument();
    expect(within(legend).queryByText('Apartar')).toBeNull();
  });

  it('the_reveal_chord_works_in_a_conflict_too', async () => {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue(MAC);
    renderScreen();
    await ready();
    const g = plan().groups.find((x) => x.title === 'Pokemon Red')!;
    const second = g.members[1]!;
    const conflict = game('Pokemon Red');
    const reveal = within(conflict).getAllByRole('button', {
      name: /en Finder$/,
    });
    // Every copy's reveal button advertises the chord.
    for (const r of reveal)
      expect(r).toHaveAttribute('aria-keyshortcuts', 'Meta+Alt+R');
    act(() => reveal[1]!.focus());
    const passed = press({
      key: '®',
      code: 'KeyR',
      metaKey: true,
      altKey: true,
    });
    expect(passed).toBe(false);
    await waitFor(() => expect(b.revealedPaths).toEqual([second.unit.primary]));
    // Focus on the group itself: its first copy.
    act(() => conflict.focus());
    press({ key: '®', code: 'KeyR', metaKey: true, altKey: true });
    await waitFor(() =>
      expect(b.revealedPaths).toEqual([
        second.unit.primary,
        g.members[0]!.unit.primary,
      ]),
    );
  });

  it('the_set_aside_scene_is_decoration_beside_the_progress_text', async () => {
    renderScreen();
    await ready();
    const p = plan();
    vi.spyOn(b, 'dedupeCancelApply').mockResolvedValue(undefined);
    act(() =>
      useDedupeStore.setState({
        status: 'applying',
        progress: { plan_id: p.plan_id, phase: 'apply', done: 2, total: 40 },
      }),
    );
    expect(screen.getByRole('status')).toHaveTextContent('Apartando 2 de 40…');
    const scene = document.querySelector('svg.dup-scene')!;
    expect(scene).toHaveAttribute('aria-hidden', 'true');
    expect(scene.textContent).toContain(fileName(p.quarantine));
    expect(scene).not.toHaveAttribute('data-stopped');
    await userEvent.click(screen.getByRole('button', { name: 'Cancelar' }));
    expect(scene).toHaveAttribute('data-stopped', 'true');
  });
});
