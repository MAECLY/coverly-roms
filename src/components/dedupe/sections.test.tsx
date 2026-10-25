import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { ConflictGroup } from '@/components/dedupe/ConflictGroup';
import { DedupeOptions } from '@/components/dedupe/DedupeOptions';
import { DedupeResult } from '@/components/dedupe/DedupeResult';
import { SystemSection } from '@/components/dedupe/SystemSection';
import { UnchangedList } from '@/components/dedupe/UnchangedList';
import { TooltipProvider } from '@/components/ui/tooltip';
import { I18nProvider } from '@/i18n/I18nProvider';
import type {
  DedupePlanView,
  DedupeReportView,
  DupGroupView,
} from '@/ipc/dedupeTypes';
import { dedupeFixture } from '@/ipc/mock/dedupeFixture';

function wrap(ui: ReactNode) {
  return render(
    <I18nProvider locale="es">
      <TooltipProvider delayDuration={0}>{ui}</TooltipProvider>
    </I18nProvider>,
  );
}

function bySystem(plan: DedupePlanView, system: string): DupGroupView[] {
  return plan.groups.filter(
    (g) => g.system === system && g.action.kind !== 'report-only',
  );
}

function section(
  plan: DedupePlanView,
  system: string,
  name: string,
  extra: Partial<Parameters<typeof SystemSection>[0]> = {},
) {
  return (
    <SystemSection
      system={system}
      name={name}
      groups={bySystem(plan, system)}
      quarantine={plan.quarantine}
      {...extra}
    />
  );
}

describe('SystemSection', () => {
  it('system_section_trigger_is_a_sentence_with_bytes_in_the_size_column', () => {
    const plan = dedupeFixture();
    wrap(section(plan, 'SMS', 'Master System'));
    const trigger = screen.getByRole('button', {
      name: 'Master System: 23 juegos, 23 copias',
    });
    // The system id is kept as a title, never in the sentence (R2).
    expect(trigger).toHaveAttribute('title', 'SMS');
    expect(trigger).not.toHaveTextContent('·');
    // Bytes sit apart, in the size column, and describe the trigger.
    const size = document.querySelector('.dup-system__size');
    expect(size).toHaveTextContent('7,4 MB');
    expect(trigger).not.toContainElement(size as HTMLElement);
    expect(trigger).toHaveAccessibleDescription('7,4 MB');
  });

  it('a game you asked to leave alone drops out of the system sentence and bytes', () => {
    const plan = dedupeFixture();
    const groups = bySystem(plan, 'SMS');
    const one = groups.find(
      (g) =>
        g.action.kind === 'act' &&
        g.members.filter((m) => m.id !== g.keeper).length === 1,
    )!;
    wrap(section(plan, 'SMS', 'Master System', { skipped: new Set([one.id]) }));
    expect(
      screen.getByRole('button', {
        name: 'Master System: 22 juegos, 22 copias',
      }),
    ).toBeInTheDocument();
    expect(document.querySelector('.dup-system__size')).not.toHaveTextContent(
      '7,4 MB',
    );
  });

  it('systems_with_conflicts_open_by_default', () => {
    const plan = dedupeFixture();
    wrap(
      <>
        {section(plan, 'GB', 'Game Boy')}
        {section(plan, 'GBA', 'Game Boy Advance')}
      </>,
    );
    const gb = screen.getByRole('button', { name: /^Game Boy:/ });
    expect(gb).toHaveAttribute('aria-expanded', 'true');
    expect(
      screen.getByText('1 con partidas distintas', { exact: false }),
    ).toBeInTheDocument();
    // GBA has 57 groups and no conflict: it starts closed.
    const gba = screen.getByRole('button', { name: /^Game Boy Advance:/ });
    expect(gba).toHaveAttribute('aria-expanded', 'false');
  });

  it('closed_system_mounts_no_rows', async () => {
    const plan = dedupeFixture();
    wrap(section(plan, 'GBA', 'Game Boy Advance'));
    expect(document.querySelectorAll('[data-dup-group]')).toHaveLength(0);
    await userEvent.click(
      screen.getByRole('button', { name: /^Game Boy Advance:/ }),
    );
    expect(
      document.querySelectorAll('[data-dup-group]').length,
    ).toBeGreaterThan(0);
  });

  it('system_section_windows_over_200_groups', async () => {
    const plan = dedupeFixture();
    const base = plan.groups.find((g) => g.title === 'GB Game 01')!;
    const groups: DupGroupView[] = Array.from({ length: 250 }, (_, i) => ({
      ...base,
      id: 10_000 + i,
      title: `Juego ${String(i).padStart(3, '0')}`,
    }));
    wrap(
      <SystemSection
        system="GB"
        name="Game Boy"
        groups={groups}
        quarantine={plan.quarantine}
        defaultOpen
      />,
    );
    const rendered = () =>
      [...document.querySelectorAll<HTMLElement>('[data-dup-group]')].map(
        (el) => Number(el.dataset.dupGroup),
      );
    const first = rendered();
    expect(first.length).toBeGreaterThan(0);
    expect(first.length).toBeLessThan(40);
    // The window holds the data in order.
    expect(first).toEqual(first.map((_, i) => 10_000 + i));

    // Tab from the last group in the window reaches the next one in the
    // data, which mounts as focus arrives at the edge.
    const lastId = first[first.length - 1]!;
    const last = document.querySelector<HTMLElement>(
      `[data-dup-group="${lastId}"]`,
    )!;
    const skip = within(last).getByRole('checkbox');
    act(() => skip.focus());
    await userEvent.tab();
    const now = (document.activeElement as HTMLElement).closest<HTMLElement>(
      '[data-dup-group]',
    );
    expect(Number(now?.dataset.dupGroup)).toBe(lastId + 1);
    expect(rendered().length).toBeLessThan(40);
  });

  it('renders conflicts as conflict groups and offers the system menu', async () => {
    const plan = dedupeFixture();
    const onSkipSystem = vi.fn();
    wrap(section(plan, 'GB', 'Game Boy', { onSkipSystem }));
    const conflict = screen
      .getAllByRole('group')
      .find((g) => g.getAttribute('data-conflict') !== null);
    expect(conflict).toBeDefined();
    expect(within(conflict!).queryAllByRole('radio')).toHaveLength(0);
    const menu = screen.getByRole('button', {
      name: 'Más acciones en Game Boy',
    });
    await userEvent.click(menu);
    await userEvent.click(
      await screen.findByRole('menuitem', { name: 'No tocar Game Boy' }),
    );
    expect(onSkipSystem).toHaveBeenCalledWith('GB');
  });
});

describe('ConflictGroup', () => {
  it('conflict_group_has_no_radio_and_lists_saves_with_open_folder', async () => {
    const g = dedupeFixture().groups.find((x) => x.title === 'Pokemon Red')!;
    const onReveal = vi.fn();
    wrap(<ConflictGroup group={g} onReveal={onReveal} />);
    expect(screen.queryAllByRole('radio')).toHaveLength(0);
    const group = screen.getByRole('group', { name: 'Pokemon Red' });
    // An icon and a word, never colour alone.
    expect(within(group).getByText('Partidas distintas')).toBeInTheDocument();
    expect(
      within(group).getByText('No se tocará nada en este juego.', {
        exact: false,
      }),
    ).toBeInTheDocument();
    const saves = [
      ...document.querySelectorAll('.dup-conflict__save .dup-row__name'),
    ].map((el) => el.textContent);
    expect(saves).toEqual([
      'Partida distinta: GB/Pokemon Red.sav',
      'Partida distinta: GB/Pokemon Red (1).sav',
    ]);
    const open = screen.getAllByRole('button', { name: /^Abrir carpeta/ });
    expect(open).toHaveLength(2);
    await userEvent.click(open[1]!);
    expect(onReveal).toHaveBeenCalledWith('GB/Pokemon Red (1).sav');
    // Both copies are named, so you know which file each save belongs to.
    expect(screen.getByText('Pokemon Red (1).gb')).toBeInTheDocument();
  });
});

describe('DedupeOptions', () => {
  function options(extra: Partial<Parameters<typeof DedupeOptions>[0]> = {}) {
    const plan = dedupeFixture();
    const props = {
      plan,
      options: {
        levels: ['exact', 'format', 'version'] as const,
        one_game_one_rom: false,
      },
      action: 'quarantine' as const,
      onOptionsChange: vi.fn(),
      onActionChange: vi.fn(),
      onChangeFolder: vi.fn(),
      ...extra,
    };
    wrap(
      <DedupeOptions
        {...props}
        options={{ ...props.options, levels: [...props.options.levels] }}
      />,
    );
    return props;
  }

  it('shows each level with its count', () => {
    options();
    expect(
      screen.getByRole('button', { name: /Copias exactas 85/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: /Otros formatos 4/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: /Otras versiones 37 solo informe/ }),
    ).toBeInTheDocument();
  });

  it('option_change_shows_the_delta', async () => {
    const p = options();
    await userEvent.click(
      screen.getByRole('checkbox', {
        name: 'Apartar también las otras versiones (una ROM por juego)',
      }),
    );
    expect(p.onOptionsChange).toHaveBeenCalledWith({
      levels: ['exact', 'format', 'version'],
      one_game_one_rom: true,
    });
    const delta = screen.getByText('+37 archivos');
    expect(delta.closest('[data-option="oneGameOneRom"]')).not.toBeNull();
    expect(
      screen
        .getAllByRole('status')
        .some((s) =>
          s.textContent?.includes('+37 archivos. Se apartarían 126 archivos.'),
        ),
    ).toBe(true);
  });

  it('turning a level off shows a negative delta by the levels', async () => {
    options({
      options: { levels: ['exact', 'format'], one_game_one_rom: false },
    });
    await userEvent.click(
      screen.getByRole('button', { name: /Otros formatos/ }),
    );
    const delta = screen.getByText('−4 archivos');
    expect(delta.closest('[data-option="levels"]')).not.toBeNull();
  });

  it('trash_option_shows_its_caveats', async () => {
    const p = options();
    expect(
      screen.queryByText(/Deshacer los devuelve desde la Papelera/),
    ).toBeNull();
    await userEvent.click(
      screen.getByRole('radio', { name: 'Enviar a la Papelera' }),
    );
    expect(p.onActionChange).toHaveBeenCalledWith('trash');
  });

  it('trash caveats say undo brings them back and name the card Trash', () => {
    options({ action: 'trash' });
    expect(
      screen.getByText(/Deshacer los devuelve desde la Papelera/),
    ).toBeInTheDocument();
    // The fixture library is a removable card.
    expect(
      screen.getByText(/la Papelera está en la propia tarjeta/),
    ).toBeInTheDocument();
  });

  it('names the quarantine folder in mono and offers to change it', async () => {
    const p = options();
    expect(screen.getByText('_Duplicados/')).toHaveClass('dup-mono');
    await userEvent.click(screen.getByRole('button', { name: 'Cambiar…' }));
    expect(p.onChangeFolder).toHaveBeenCalled();
    expect(screen.getByText('Siguiente juego')).toBeInTheDocument();
  });
});

describe('UnchangedList', () => {
  it('unchanged_list_shows_reasons', async () => {
    const plan = dedupeFixture();
    const skippedId = plan.groups.find((g) => g.title === 'SMS Game 01')!.id;
    wrap(
      <UnchangedList
        groups={plan.groups}
        skipped={new Set([skippedId])}
        systemName={(id) => (id === 'GB' ? 'Game Boy' : id)}
      />,
    );
    const trigger = screen.getByRole('button', { name: /^Sin cambios \(38\)/ });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(trigger);
    const tetris = screen.getByText('Tetris').closest('li')!;
    expect(
      within(tetris).getByText('Otras versiones, solo informe'),
    ).toBeInTheDocument();
    expect(within(tetris).getByText('Game Boy')).toBeInTheDocument();
    const sms = screen.getByText('SMS Game 01').closest('li')!;
    expect(within(sms).getByText('Has pedido no tocarlo')).toBeInTheDocument();
    // Conflicts have their own block.
    expect(screen.queryByText('Pokemon Red')).toBeNull();
  });
});

describe('DedupeResult', () => {
  function report(
    plan: DedupePlanView,
    extra: Partial<DedupeReportView> = {},
  ): DedupeReportView {
    const outcomes: DedupeReportView['outcomes'] = [];
    for (const g of plan.groups) {
      if (g.action.kind !== 'act') continue;
      for (const m of g.members)
        if (m.id !== g.keeper)
          outcomes.push({
            group: g.id,
            member: m.id,
            status: 'quarantined',
            to: m.dest ?? '',
            files: [],
          });
    }
    return {
      job: 'job-1',
      outcomes,
      bytes_out: plan.totals.bytes,
      art_left: [],
      paused: null,
      resumable: [],
      notes: [],
      ...extra,
    };
  }

  function result(plan: DedupePlanView, r: DedupeReportView) {
    const props = {
      onOpenFolder: vi.fn(),
      onUndo: vi.fn(),
      onBack: vi.fn(),
      onFindCovers: vi.fn(),
    };
    wrap(
      <DedupeResult plan={plan} report={r} action="quarantine" {...props} />,
    );
    return props;
  }

  it('each result is a region named by its own headline', () => {
    const plan = dedupeFixture();
    const r = report(plan);
    const noop = () => {};
    wrap(
      <>
        <DedupeResult
          plan={plan}
          report={r}
          action="quarantine"
          onOpenFolder={noop}
          onUndo={noop}
          onBack={noop}
        />
        <DedupeResult
          plan={plan}
          report={r}
          action="trash"
          onOpenFolder={noop}
          onUndo={noop}
          onBack={noop}
        />
      </>,
    );
    const [a, b] = [...document.querySelectorAll('section.dup-result')];
    expect(a!.getAttribute('aria-labelledby')).not.toBe(
      b!.getAttribute('aria-labelledby'),
    );
    expect(a).toHaveAccessibleName(/apartados en _Duplicados/);
    expect(b).not.toHaveAccessibleName(/apartados en _Duplicados/);
  });

  it('result_same_volume_says_the_card_is_as_full', () => {
    const plan = dedupeFixture();
    result(plan, report(plan));
    expect(
      screen.getByText(
        '89 archivos apartados en _Duplicados/. Deshacer está a un clic.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByText(/^Fuera de ROMS\//)).toBeInTheDocument();
    expect(
      screen.getByText(
        'La tarjeta sigue igual de llena hasta que vacíes _Duplicados/ o lo muevas a tu ordenador.',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Abrir _Duplicados/' }),
    ).toBeInTheDocument();
  });

  it('no card note when the quarantine is on another volume', () => {
    const plan = { ...dedupeFixture(), same_volume: false };
    result(plan, report(plan));
    expect(screen.queryByText(/sigue igual de llena/)).toBeNull();
  });

  it('result_partial_turns_warning_with_reasons', () => {
    const plan = dedupeFixture();
    const r = report(plan);
    const [a, b] = r.outcomes;
    r.outcomes[0] = {
      group: a!.group,
      member: a!.member,
      files: [],
      status: 'skipped',
      reason: { kind: 'keeper-changed' },
    };
    r.outcomes[1] = {
      group: b!.group,
      member: b!.member,
      files: [],
      status: 'skipped',
      reason: { kind: 'destination-limit', why: 'fat32-size' },
    };
    result(plan, r);
    const alert = screen
      .getAllByRole('status')
      .find((a) => a.getAttribute('data-variant') === 'warning')!;
    expect(alert).toHaveTextContent('Apartados: 87 · Sin tocar: 2');
    expect(alert).toHaveTextContent(
      'El archivo que se queda ha cambiado; no se ha movido nada de este juego.',
    );
    expect(alert).toHaveTextContent('más de 4 GB en FAT32');
    expect(alert).toHaveTextContent('No se ha movido nada más.');
    expect(alert).toHaveTextContent('Adventures of Dolphins.sms');
  });

  it('copies the plan never meant to move keep the result a success', () => {
    const plan = dedupeFixture();
    const r = report(plan);
    for (const kind of ['report-only', 'excluded', 'conflict'] as const)
      r.outcomes.push({
        group: 1,
        member: 1,
        files: [],
        status: 'skipped',
        reason: { kind },
      });
    result(plan, r);
    expect(
      screen.getByText(
        '89 archivos apartados en _Duplicados/. Deshacer está a un clic.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Sin tocar/)).toBeNull();
  });

  it('result_offers_find_covers_when_art_left', async () => {
    const plan = dedupeFixture();
    const p = result(
      plan,
      report(plan, { art_left: ['SMS/Imgs/Adventures of Dolphins.png'] }),
    );
    await userEvent.click(
      screen.getByRole('button', { name: 'Buscar portada para 1 juego' }),
    );
    expect(p.onFindCovers).toHaveBeenCalled();
  });

  it('undo asks first and names the count', async () => {
    const plan = dedupeFixture();
    const p = result(plan, report(plan));
    await userEvent.click(screen.getByRole('button', { name: 'Deshacer' }));
    expect(
      await screen.findByText('¿Devolver 89 archivos a su sitio?'),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole('button', { name: 'Devolver 89 archivos' }),
    );
    expect(p.onUndo).toHaveBeenCalled();
  });
});
