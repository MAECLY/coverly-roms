import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { DupGroup } from '@/components/dedupe/DupGroup';
import { TooltipProvider } from '@/components/ui/tooltip';
import { I18nProvider } from '@/i18n/I18nProvider';
import type { Locale } from '@/i18n/core';
import type { DupGroupView, MemberView } from '@/ipc/dedupeTypes';
import { dedupeFixture } from '@/ipc/mock/dedupeFixture';

const QUARANTINE = '/Volumes/SD/_Duplicados';

function fixtureGroup(title: string): DupGroupView {
  const g = dedupeFixture().groups.find((x) => x.title === title);
  if (!g) throw new Error(`no fixture group ${title}`);
  return g;
}

function member(
  id: number,
  path: string,
  extra: Partial<MemberView> = {},
): MemberView {
  return {
    id,
    unit: {
      primary: path,
      files: [path],
      kind: 'single',
      mtime_ns: 1,
      in_library: true,
    },
    relation: 'exact',
    size: 1024,
    travels: [],
    stays: [],
    dest: id === 0 ? null : `${QUARANTINE}/${path}`,
    catalogue_name: null,
    serials: [],
    confidence: 1,
    ...extra,
  };
}

function groupOf(
  members: MemberView[],
  extra: Partial<DupGroupView> = {},
): DupGroupView {
  return {
    id: 77,
    system: 'GB',
    title: 'Zelda',
    keeper: 0,
    reason: { kind: 'tie' },
    runner_up: null,
    members,
    action: { kind: 'act' },
    tie: false,
    choice_dropped: false,
    ...extra,
  };
}

interface Opts {
  locale?: Locale;
  onKeeperChange?: (group: number, keeper: number) => void;
  onSkipChange?: (group: number, skipped: boolean) => void;
}

function renderGroup(group: DupGroupView, opts: Opts = {}) {
  return render(
    <I18nProvider locale={opts.locale ?? 'es'}>
      <TooltipProvider delayDuration={0}>
        <DupGroup
          group={group}
          quarantine={QUARANTINE}
          profileName="Anbernic (stock)"
          onKeeperChange={opts.onKeeperChange}
          onSkipChange={opts.onSkipChange}
        />
      </TooltipProvider>
    </I18nProvider>,
  );
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('DupGroup', () => {
  it('renders one radiogroup labelled by the game title with exactly one checked radio', () => {
    renderGroup(fixtureGroup('Ecco the Dolphin'));
    const rg = screen.getByRole('radiogroup', { name: 'Ecco the Dolphin' });
    const radios = within(rg).getAllByRole('radio');
    expect(radios).toHaveLength(2);
    const checked = radios.filter(
      (r) => r.getAttribute('aria-checked') === 'true',
    );
    expect(checked).toHaveLength(1);
    expect(checked[0]).toHaveAccessibleName('Ecco the Dolphin.sms');
    // The why line describes the keeper's radio.
    expect(checked[0]).toHaveAccessibleDescription(
      'Se queda: tiene portada; la otra no.',
    );
    expect(within(rg).getByText('Se queda')).toBeInTheDocument();
    // The tray row says where the copy goes and what travels with it.
    expect(
      screen.getByText('→ _Duplicados/SMS/Adventures of Dolphins.sms'),
    ).toBeInTheDocument();
    expect(screen.getByText('+ 1 portada')).toBeInTheDocument();
  });

  it('tray_radio_has_a_visible_keep_label', () => {
    renderGroup(fixtureGroup('Crash Bash'));
    const keepers = screen.getAllByRole('radio', {
      name: /^Quedarme con esta /,
    });
    expect(keepers.map((r) => r.getAttribute('aria-checked'))).toEqual([
      'false',
      'false',
    ]);
    expect(keepers[0]).toHaveAccessibleName('Quedarme con esta Crash Bash.bin');
    // Visible text, not only an accessible name.
    expect(screen.getAllByText('Quedarme con esta')).toHaveLength(2);
    // The shared save stays with the PBP.
    expect(
      screen.getByText(
        'La partida Crash Bash.srm se queda con Crash Bash.PBP.',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        'Se queda: formato preferido en Anbernic (stock) (PBP); la otra es BIN.',
      ),
    ).toBeInTheDocument();
  });

  it('changing_keeper_announces_and_offers_undo', () => {
    vi.useFakeTimers();
    const onKeeperChange = vi.fn();
    const g = fixtureGroup('Ecco the Dolphin');
    renderGroup(g, { onKeeperChange });

    fireEvent.click(
      screen.getByRole('radio', {
        name: 'Quedarme con esta Adventures of Dolphins.sms',
      }),
    );
    expect(onKeeperChange).toHaveBeenLastCalledWith(g.id, 1);
    expect(screen.getByRole('status')).toHaveTextContent(
      'Ahora se queda Adventures of Dolphins.sms; Ecco the Dolphin.sms pasa a apartarse.',
    );
    const checked = screen
      .getAllByRole('radio')
      .find((r) => r.getAttribute('aria-checked') === 'true');
    expect(checked).toHaveAccessibleName('Adventures of Dolphins.sms');
    expect(checked).toHaveAccessibleDescription('Se queda: lo has elegido tú.');
    // The old keeper is now in the tray with a destination.
    expect(
      screen.getByText('→ _Duplicados/SMS/Ecco the Dolphin.sms'),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Deshacer' }));
    expect(onKeeperChange).toHaveBeenLastCalledWith(g.id, 0);
    expect(
      screen.getByRole('radio', { name: 'Ecco the Dolphin.sms' }),
    ).toHaveAttribute('aria-checked', 'true');
    expect(
      screen.getByText('Se queda: tiene portada; la otra no.'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Deshacer' })).toBeNull();

    // The undo offer lasts 10 s.
    fireEvent.click(
      screen.getByRole('radio', {
        name: 'Quedarme con esta Adventures of Dolphins.sms',
      }),
    );
    act(() => vi.advanceTimersByTime(9_900));
    expect(
      screen.getByRole('button', { name: 'Deshacer' }),
    ).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(200));
    expect(screen.queryByRole('button', { name: 'Deshacer' })).toBeNull();
  });

  it('arrow keys move within the group, Tab leaves it, X toggles "No tocar este juego"', async () => {
    const user = userEvent.setup();
    const onKeeperChange = vi.fn();
    const onSkipChange = vi.fn();
    const g = fixtureGroup('Crash Bash');
    renderGroup(g, { onKeeperChange, onSkipChange });

    screen.getByRole('radio', { name: 'Crash Bash.PBP' }).focus();
    await user.keyboard('{ArrowDown}');
    expect(onKeeperChange).toHaveBeenLastCalledWith(g.id, 1);
    expect(document.activeElement).toHaveAccessibleName('Crash Bash.bin');
    // Arrows walk the copies in a stable order, whichever sits on the shelf.
    await user.keyboard('{ArrowDown}');
    expect(onKeeperChange).toHaveBeenLastCalledWith(g.id, 2);
    expect(document.activeElement).toHaveAccessibleName('Crash Bash 1.bin');
    await user.keyboard('{ArrowDown}');
    expect(onKeeperChange).toHaveBeenLastCalledWith(g.id, 0);
    await user.keyboard('{ArrowUp}');
    expect(onKeeperChange).toHaveBeenLastCalledWith(g.id, 2);

    await user.tab();
    const skip = screen.getByRole('checkbox', { name: 'No tocar este juego' });
    expect(document.activeElement).toBe(skip);
    expect(screen.getAllByRole('radio')).not.toContain(document.activeElement);

    screen.getByRole('radio', { name: 'Crash Bash 1.bin' }).focus();
    await user.keyboard('x');
    expect(onSkipChange).toHaveBeenLastCalledWith(g.id, true);
    expect(skip).toHaveAttribute('aria-checked', 'true');
    await user.keyboard('X');
    expect(onSkipChange).toHaveBeenLastCalledWith(g.id, false);
    expect(skip).toHaveAttribute('aria-checked', 'false');
  });

  it('level 2 without 1G1R shows "Se queda también (solo informe)" and no radios', () => {
    renderGroup(fixtureGroup('Tetris'));
    expect(screen.queryAllByRole('radio')).toHaveLength(0);
    expect(
      screen.getByText('Se queda también (solo informe)'),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^→ /)).toBeNull();
    expect(screen.queryByRole('checkbox')).toBeNull();
    expect(screen.getByText('Tetris (Japan).gb')).toBeInTheDocument();
  });

  it('shows confidence only in the badge tooltip; below 1.00 shows "Sin confirmar"', async () => {
    const { unmount } = renderGroup(fixtureGroup('Crash Bash'));
    // The weakest link names the group; the number is not on the page.
    const badge = screen.getAllByText('Otro formato')[0]!.closest('span')!;
    expect(screen.queryByText(/0\.97/)).toBeNull();
    act(() => badge.focus());
    expect(await screen.findByRole('tooltip')).toHaveTextContent(
      'Confianza: 0.97',
    );
    unmount();

    renderGroup(
      groupOf([
        member(0, 'GB/Zelda.gb'),
        member(1, 'GB/Zelda (1).gb', { confidence: 0.9 }),
      ]),
    );
    expect(screen.getAllByText('Sin confirmar').length).toBeGreaterThan(0);
    expect(screen.queryByText('Copia exacta')).toBeNull();
    expect(screen.queryByText(/0\.9/)).toBeNull();
  });

  it('a former keeper takes the relation of the copy that now stays', () => {
    renderGroup(fixtureGroup('Crash Bash'));
    fireEvent.click(
      screen.getByRole('radio', { name: 'Quedarme con esta Crash Bash.bin' }),
    );
    // The PBP is another format of the BIN that stays, not an exact copy
    // (relations are to the keeper; the plan measured them to the PBP).
    const pbp = screen.getByText('Crash Bash.PBP').closest('.dup-row')!;
    expect(within(pbp as HTMLElement).getByText('Otro formato')).toBeVisible();
  });

  it('a picked-away keeper with an absolute path keeps its system folder', () => {
    // As the core sends them: absolute primaries, dest under the quarantine.
    renderGroup(
      groupOf([
        member(0, '/Volumes/SD/ROMS/GB/Zelda.gb', { dest: null }),
        member(1, '/Volumes/SD/ROMS/GB/Zelda (1).gb', {
          dest: `${QUARANTINE}/GB/Zelda (1).gb`,
        }),
      ]),
    );
    expect(
      screen.getByText('→ _Duplicados/GB/Zelda (1).gb'),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole('radio', { name: 'Quedarme con esta Zelda (1).gb' }),
    );
    expect(screen.getByText('→ _Duplicados/GB/Zelda.gb')).toBeInTheDocument();
  });

  it('save that leaves with a picked keeper warns inline', () => {
    renderGroup(
      groupOf(
        [
          member(0, 'GB/Zelda.gb', {
            travels: [{ from: 'GB/Zelda.srm', kind: { kind: 'save' } }],
          }),
          member(1, 'GB/Zelda (1).gb'),
        ],
        { reason: { kind: 'has-save', path: 'GB/Zelda.srm' } },
      ),
    );
    expect(screen.queryByText(/Su partida/)).toBeNull();
    fireEvent.click(
      screen.getByRole('radio', { name: 'Quedarme con esta Zelda (1).gb' }),
    );
    expect(
      screen.getByText(
        'Su partida Zelda.srm se irá con ella a _Duplicados; el juego que se queda no la verá.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByText('+ 1 partida')).toBeInTheDocument();
  });

  it('art leaving shows the cover warning', () => {
    renderGroup(
      groupOf([
        member(0, 'GB/Zelda.gb'),
        member(1, 'GB/Zelda (1).gb', {
          travels: [
            {
              from: 'GB/Imgs/Zelda (1).png',
              kind: { kind: 'art', media: 'cover' },
            },
          ],
          art_leaves: { media: ['cover'] },
        }),
      ]),
    );
    expect(
      screen.getByText(
        'La portada se va con esta copia; Zelda.gb se quedará sin portada.',
      ),
    ).toBeInTheDocument();
  });

  it('reduced motion: no transition on the shelf cap', () => {
    const motion = (reduce: boolean) =>
      vi.spyOn(window, 'matchMedia').mockImplementation(
        (query: string) =>
          ({
            matches: reduce && query.includes('prefers-reduced-motion'),
            media: query,
            onchange: null,
            addListener: () => undefined,
            removeListener: () => undefined,
            addEventListener: () => undefined,
            removeEventListener: () => undefined,
            dispatchEvent: () => false,
          }) as MediaQueryList,
      );
    const root = () =>
      screen
        .getByRole('heading', { name: /Ecco the Dolphin/ })
        .closest('[data-dup-group]') as HTMLElement;

    motion(false);
    const { unmount } = renderGroup(fixtureGroup('Ecco the Dolphin'));
    expect(root().style.getPropertyValue('--dup-cap-duration')).toBe(
      'var(--cr-duration-base)',
    );
    unmount();
    vi.restoreAllMocks();

    motion(true);
    renderGroup(fixtureGroup('Ecco the Dolphin'));
    expect(root().style.getPropertyValue('--dup-cap-duration')).toBe('0ms');
  });

  it('hostile file names render as text', () => {
    const evil = 'GB/<img src=x onerror=alert(1)>.gb';
    renderGroup(
      groupOf([member(0, 'GB/Zelda.gb'), member(1, evil)], {
        title: '<b>Zelda</b>',
      }),
    );
    expect(document.querySelector('img')).toBeNull();
    expect(document.querySelector('b')).toBeNull();
    expect(
      screen.getByText('<img src=x onerror=alert(1)>.gb'),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('radiogroup', { name: '<b>Zelda</b>' }),
    ).toBeInTheDocument();
  });

  it('names the catalogue entry when the keeper file is called otherwise', () => {
    renderGroup(
      groupOf([
        member(0, 'GB/zelda.gb', {
          catalogue_name: "Legend of Zelda, The - Link's Awakening (USA)",
        }),
        member(1, 'GB/Zelda (1).gb'),
      ]),
      { locale: 'en' },
    );
    expect(
      screen.getByText(
        "The catalogue calls it Legend of Zelda, The - Link's Awakening (USA)",
      ),
    ).toBeInTheDocument();
  });
});
