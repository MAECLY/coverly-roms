import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { SystemGroups } from '@/components/organize/SystemGroups';
import { I18nProvider } from '@/i18n/I18nProvider';
import { groupBySystem } from '@/lib/organize';
import type { OrganizeItem, SystemInfo, TransferMode } from '@/ipc/types';
import { autoItem, discItem, planOf } from '@/test/organizeFixtures';

const NAMES: Record<string, string> = {
  gba: 'Game Boy Advance',
  gb: 'Game Boy',
  psx: 'PlayStation',
};
const systemName = (id: string) => NAMES[id] ?? id;

function renderGroups(
  items: ReturnType<typeof autoItem>[],
  mode: TransferMode = 'move',
) {
  render(
    <I18nProvider locale="en">
      <SystemGroups
        groups={groupBySystem(planOf(items))}
        layoutName="Anbernic (stock)"
        systemName={systemName}
        mode={mode}
        profile={null}
      />
    </I18nProvider>,
  );
}

describe('SystemGroups', () => {
  it('shows the target folder, the row, the confidence and the why', () => {
    renderGroups([
      autoItem(0, 'Tetris Worlds (USA).zip', 'gba', 'ROMS/GBA', {
        why: [
          {
            kind: 'hash',
            system: 'gba',
            weight: 1,
            detail: '7b729804 Tetris Worlds (USA)',
          },
        ],
      }),
      autoItem(1, 'Tetris DX.zip', 'gb', 'ROMS/GB', {
        confidence: 0.9,
        method: 'header',
        why: [
          {
            kind: 'header',
            system: 'gb',
            weight: 0.9,
            detail: 'Nintendo logo, CGB 0x00',
          },
        ],
      }),
    ]);
    const gba = screen.getByRole('group', { name: /^ROMS\/GBA\/ \(1\)/ });
    expect(
      within(gba).getByRole('button', { name: /^ROMS\/GBA\/ \(1\)/ }),
    ).toHaveAttribute('aria-expanded', 'true');
    expect(within(gba).getByText('Game Boy Advance')).toBeInTheDocument();
    // The system's icon sits beside its name on the group trigger.
    expect(
      within(gba)
        .getByRole('button', { name: /^ROMS\/GBA\/ \(1\)/ })
        .querySelector('[data-system-icon="gba"][aria-hidden="true"]'),
    ).not.toBeNull();
    expect(
      within(gba).getByText(
        'Tetris Worlds (USA).zip → ROMS/GBA/Tetris Worlds (USA).zip',
      ),
    ).toBeInTheDocument();
    // The confidence is a labelled badge, not colour alone.
    expect(within(gba).getByText('High')).toHaveAttribute(
      'title',
      'Confidence',
    );
    expect(
      within(gba).getByText(
        'CRC 7B729804 in the Game Boy Advance catalogue: Tetris Worlds (USA)',
      ),
    ).toBeInTheDocument();
    const gb = screen.getByRole('group', { name: /^ROMS\/GB\/ \(1\)/ });
    expect(within(gb).getByText('Medium')).toBeInTheDocument();
    expect(
      within(gb).getByText('The header is Game Boy (Nintendo logo, CGB 0x00)'),
    ).toBeInTheDocument();
  });

  it('shows no confidence for a system the user chose', () => {
    renderGroups([
      autoItem(0, 'Tetris.zip', 'gb', 'ROMS/GB', {
        confidence: 1,
        method: 'user',
        why: [{ kind: 'user', system: 'gb', weight: 1, detail: '' }],
      }),
    ]);
    const gb = screen.getByRole('group', { name: /^ROMS\/GB\/ \(1\)/ });
    expect(within(gb).getByText('Chosen by you')).toBeInTheDocument();
    expect(within(gb).queryByTitle('Confidence')).not.toBeInTheDocument();
    expect(within(gb).queryByText('High')).not.toBeInTheDocument();
  });

  it('renders at most 200 rows per group and a show-more button', async () => {
    const items = Array.from({ length: 450 }, (_, i) =>
      autoItem(i, `Game ${String(i).padStart(3, '0')}.gba`, 'gba', 'ROMS/GBA'),
    );
    renderGroups(items);
    const group = screen.getByRole('group', { name: /^ROMS\/GBA\/ \(450\)/ });
    const toggle = within(group).getByRole('button', {
      name: /^ROMS\/GBA\/ \(450\)/,
    });
    // More than 200 rows overall: groups start closed and render no rows.
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(within(group).queryAllByRole('listitem')).toHaveLength(0);
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(within(group).getAllByRole('listitem')).toHaveLength(200);
    await userEvent.click(
      within(group).getByRole('button', { name: 'Show 250 more' }),
    );
    expect(within(group).getAllByRole('listitem')).toHaveLength(400);
    await userEvent.click(
      within(group).getByRole('button', { name: 'Show 50 more' }),
    );
    expect(within(group).getAllByRole('listitem')).toHaveLength(450);
    expect(
      within(group).queryByRole('button', { name: /more/ }),
    ).not.toBeInTheDocument();
    // Closing it takes the rows out of the DOM again.
    await userEvent.click(toggle);
    expect(within(group).queryAllByRole('listitem')).toHaveLength(0);
  });

  it('explains renames and notes', () => {
    renderGroups([
      autoItem(0, 'Tetris (1).gba', 'gba', 'ROMS/GBA', {
        rename: { kind: 'collision', name: 'Tetris (2).gba' },
        target: 'ROMS/GBA/Tetris (2).gba',
        notes: [
          { kind: 'multi-rom-zip', count: 3 },
          { kind: 'no-folder-on-device', folder: 'GBA' },
        ],
      }),
    ]);
    expect(
      screen.getByText(
        'Will be saved as Tetris (2).gba because a different file already exists',
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText('Holds 3 ROMs; the console will load only the first.'),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        'Anbernic (stock) has no folder for Game Boy Advance; GBA will be created.',
      ),
    ).toBeInTheDocument();
  });
});

describe('SystemGroups — a system the user chose', () => {
  const SYSTEMS: SystemInfo[] = [
    { id: 'gb', name: 'Game Boy', family: 'handheld', no_emulator: false },
    {
      id: 'gba',
      name: 'Game Boy Advance',
      family: 'handheld',
      no_emulator: false,
    },
  ];

  function renderChosen(onDecide: (ids: number[], c: unknown) => void) {
    const items = [
      autoItem(0, 'mystery.rom', 'gb', 'ROMS/GB', {
        method: 'user',
        why: [{ kind: 'user', system: 'gb', weight: 1, detail: '' }],
      }),
      autoItem(1, 'Tetris DX.zip', 'gb', 'ROMS/GB'),
    ];
    render(
      <I18nProvider locale="en">
        <SystemGroups
          groups={groupBySystem(planOf(items))}
          layoutName="Anbernic (stock)"
          systemName={systemName}
          systems={SYSTEMS}
          onDecide={onDecide}
        />
      </I18nProvider>,
    );
  }

  it('can be changed to another system or left in place', async () => {
    const calls: [number[], unknown][] = [];
    renderChosen((ids, c) => calls.push([ids, c]));
    const select = screen.getByRole('combobox', {
      name: 'System for mystery.rom',
    });
    expect(select).toHaveValue('gb');
    // The chosen system's icon adorns the select, outside its wrapper.
    const adornment = select.parentElement!.previousElementSibling;
    expect(adornment).toHaveAttribute('data-system-icon', 'gb');
    expect(adornment).toHaveAttribute('aria-hidden', 'true');
    expect(select.querySelector('[data-system-icon]')).toBeNull();
    // Only the user's own choice is changeable, not the ladder's.
    expect(
      screen.queryByRole('combobox', { name: 'System for Tetris DX.zip' }),
    ).toBeNull();
    await userEvent.selectOptions(select, 'Game Boy Advance');
    await userEvent.selectOptions(select, 'Leave in place');
    expect(calls).toEqual([
      [[0], { system: 'gba' }],
      [[0], 'leave'],
    ]);
  });
});

describe('SystemGroups — the full picker of a chosen system', () => {
  it('groups every system by family, sorted, with leave last', () => {
    const systems: SystemInfo[] = [
      {
        id: 'zxspectrum',
        name: 'ZX Spectrum',
        family: 'computer',
        no_emulator: false,
      },
      {
        id: 'ps2',
        name: 'Sony PlayStation 2',
        family: 'console',
        no_emulator: false,
      },
      { id: 'gb', name: 'Game Boy', family: 'handheld', no_emulator: false },
      {
        id: 'jaguar',
        name: 'Atari Jaguar',
        family: 'console',
        no_emulator: false,
      },
    ];
    const items = [
      autoItem(0, 'mystery.rom', 'gb', 'ROMS/GB', {
        method: 'user',
        why: [{ kind: 'user', system: 'gb', weight: 1, detail: '' }],
      }),
    ];
    render(
      <I18nProvider locale="es">
        <SystemGroups
          groups={groupBySystem(planOf(items))}
          layoutName="Anbernic (stock)"
          systemName={systemName}
          systems={systems}
          onDecide={() => {}}
        />
      </I18nProvider>,
    );
    const select = screen.getByRole('combobox');
    const groups = [...select.querySelectorAll('optgroup')];
    expect(groups.map((g) => g.label)).toEqual([
      'Consolas',
      'Portátiles',
      'Ordenadores',
    ]);
    expect(
      [...groups[0]!.querySelectorAll('option')].map((o) => o.textContent),
    ).toEqual(['Atari Jaguar', 'Sony PlayStation 2']);
    // «Dejar» stays the last option, outside every group.
    const last = select.lastElementChild!;
    expect(last.tagName).toBe('OPTION');
    expect(last).toHaveValue('__leave__');
    expect(select).toHaveValue('gb');
  });
});

describe('SystemGroups — actions and discs', () => {
  it('extract_badge_has_icon_and_word', () => {
    renderGroups([discItem(0)]);
    const badge = screen.getByText('Extract');
    // A petrol outline with the archive icon, beside the word.
    expect(badge).toHaveClass('border-primary');
    const icon = badge.querySelector('svg');
    expect(icon).not.toBeNull();
    expect(icon).toHaveAttribute('aria-hidden', 'true');
    expect(icon).toHaveClass('lucide-archive');
  });

  it('badge_never_colour_only', () => {
    renderGroups([autoItem(0, 'Tetris DX.zip', 'gb', 'ROMS/GB'), discItem(1)]);
    // Every row names its action in words; the colour only repeats it.
    const gb = screen.getByRole('group', { name: /^ROMS\/GB\/ \(1\)/ });
    expect(within(gb).getByText('Move')).toHaveTextContent(/^Move$/);
    const ps = screen.getByRole('group', { name: /^ROMS\/PS\/ \(1\)/ });
    expect(within(ps).getByText('Extract')).toHaveTextContent(/^Extract$/);
    expect(within(ps).queryByText('Move')).not.toBeInTheDocument();
  });

  it('names copy rows in copy mode', () => {
    renderGroups([autoItem(0, 'Tetris DX.zip', 'gb', 'ROMS/GB')], 'copy');
    expect(screen.getByText('Copy')).toBeInTheDocument();
    expect(screen.queryByText('Move')).not.toBeInTheDocument();
  });

  it('extract_detail_in_mono_with_exact_count', () => {
    renderGroups([discItem(0)]);
    const detail = screen.getByText('12 files · 419.4 MB');
    expect(detail).toHaveClass('font-mono');
  });

  it('extract_detail_says_one_file_in_the_singular', () => {
    renderGroups([discItem(0, {}, 0)]);
    expect(screen.getByText('1 file · 1.2 kB')).toHaveClass('font-mono');
    expect(screen.queryByText(/1 files/)).not.toBeInTheDocument();
  });

  it('disc_row_shows_serial', () => {
    renderGroups([discItem(0), autoItem(1, 'Tetris DX.zip', 'gb', 'ROMS/GB')]);
    const ps = screen.getByRole('group', { name: /^ROMS\/PS\/ \(1\)/ });
    expect(within(ps).getByText('SLUS-00862')).toHaveClass('font-mono');
    expect(
      within(ps).getByText(
        'PlayStation disc: SYSTEM.CNF BOOT SLUS_008.62 (SLUS-00862)',
      ),
    ).toBeInTheDocument();
    const gb = screen.getByRole('group', { name: /^ROMS\/GB\/ \(1\)/ });
    expect(within(gb).queryByText(/SLUS/)).not.toBeInTheDocument();
  });

  it("puts an extract row's notes in its detail", async () => {
    renderGroups([discItem(0)]);
    // Folded away until the row is expanded.
    expect(
      screen.queryByText('It is a 7z archive named .zip'),
    ).not.toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', {
        name: 'Files to extract from Next Tetris, The.zip',
      }),
    );
    expect(screen.getByText('It is a 7z archive named .zip')).toBeVisible();
    expect(screen.getByText('Tracks go straight into ROMS/PS')).toBeVisible();
  });
});

describe('SystemGroups — extract rows keep warnings in sight', () => {
  it('folds only the archive notes; warnings and renames stay visible', () => {
    renderGroups([
      discItem(0, {
        rename: { kind: 'collision', name: 'Next Tetris, The (1).cue' },
        notes: [
          { kind: 'container-mismatch', actual: '7z', ext: 'zip' },
          { kind: 'archive-not-fully-read' },
          { kind: 'archive-kept-leftovers', count: 2, examples: ['a.txt'] },
        ],
      }),
    ]);
    // The archive's own facts wait in the detail ...
    expect(
      screen.queryByText('It is a 7z archive named .zip'),
    ).not.toBeInTheDocument();
    // ... but what asks the user to look, or changes the result, does not.
    expect(
      screen.getByText(
        "Couldn't be read in full during the dry run; please review it",
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        'The archive stays: it has 2 files that are not extracted',
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        'Will be saved as Next Tetris, The (1).cue because a different file already exists',
      ),
    ).toBeVisible();
  });

  it('names each detail toggle after its archive', () => {
    renderGroups([discItem(0), discItem(1, { sources: ['Tetris Plus.zip'] })]);
    expect(
      screen.getByRole('button', {
        name: 'Files to extract from Next Tetris, The.zip',
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', {
        name: 'Files to extract from Tetris Plus.zip',
      }),
    ).toBeInTheDocument();
  });

  describe('systems batch 2 (spec §9.1)', () => {
    const demons = (extra: Partial<OrganizeItem> = {}) =>
      autoItem(0, "Demon's Souls (BLUS-30443)", 'ps3', 'ps3', {
        kind: 'folder',
        size: 23_410_000_000,
        target: "ps3/Demon's Souls (BLUS-30443).ps3",
        method: 'header',
        confidence: 0.95,
        why: [
          { kind: 'header', system: 'ps3', weight: 0.95, detail: 'PARAM.SFO' },
        ],
        title_id: { id: 'BLUS30443', source: 'param-sfo' },
        notes: [
          {
            kind: 'folder-suffix',
            name: "Demon's Souls (BLUS-30443).ps3",
            layout: 'ES-DE',
          },
        ],
        ...extra,
      });

    function renderIn(locale: 'es' | 'en', items: OrganizeItem[]) {
      render(
        <I18nProvider locale={locale}>
          <SystemGroups
            groups={groupBySystem(planOf(items))}
            layoutName="ES-DE"
            systemName={(id) =>
              id === 'ps3' ? 'Sony PlayStation 3' : 'Nintendo Switch'
            }
            profile={null}
          />
        </I18nProvider>,
      );
    }

    it('a game folder shows the folder glyph, a hidden label and its size in mono', () => {
      renderIn('es', [demons()]);
      const row = screen.getByRole('listitem');
      const label = within(row).getByText('Carpeta de juego');
      expect(label).toHaveClass('sr-only');
      // The label is read before the name.
      expect(label.parentElement).toHaveTextContent(
        "Carpeta de juego Demon's Souls (BLUS-30443)",
      );
      expect(row.querySelector('svg[aria-hidden="true"]')).not.toBeNull();
      const size = within(row).getByText('23,41 GB');
      expect(size).toHaveClass('font-mono');
      expect(
        within(row).getByText("→ ps3/Demon's Souls (BLUS-30443).ps3"),
      ).toBeInTheDocument();
      // The id's source, then the rename note, as plain sentences.
      expect(
        within(row).getByText('PARAM.SFO').closest('.mono'),
      ).not.toBeNull();
      expect(
        within(row).getByText('BLUS30443').closest('.mono'),
      ).not.toBeNull();
      expect(row).toHaveTextContent('Title ID BLUS30443 en PARAM.SFO');
      expect(
        within(row).getByText(
          "Se renombra a Demon's Souls (BLUS-30443).ps3 para que ES-DE lo muestre",
        ),
      ).toBeInTheDocument();
    });

    it('the size uses the English separator in English', () => {
      renderIn('en', [demons()]);
      expect(screen.getByText('23.41 GB')).toHaveClass('font-mono');
      expect(screen.getByText('Game folder')).toHaveClass('sr-only');
    });

    it('a file row has no folder label', () => {
      renderIn('en', [autoItem(1, 'Halo 3 (USA).iso', 'ps3', 'ps3', {})]);
      expect(screen.queryByText('Game folder')).toBeNull();
    });

    it('an update sits right under its base, whatever order it came in', () => {
      const base = autoItem(
        1,
        'Super Mario Odyssey [0100000000010000][v0].nsp',
        'switch',
        'switch',
        {
          title_id: { id: '0100000000010000', source: 'ticket-name' },
        },
      );
      const update = autoItem(
        2,
        'Super Mario Odyssey [0100000000010800][v393216].nsp',
        'switch',
        'switch',
        { notes: [{ kind: 'update', of: 'Super Mario Odyssey' }] },
      );
      const other = autoItem(3, 'Mario Kart 8.nsp', 'switch', 'switch');
      renderIn('es', [update, other, base]);
      const rows = screen.getAllByRole('listitem');
      expect(rows.map((r) => r.textContent?.slice(0, 24))).toEqual([
        'Mario Kart 8.nsp → switc',
        'Super Mario Odyssey [010',
        'Super Mario Odyssey [010',
      ]);
      expect(rows[1]).toHaveTextContent(
        'Title ID 0100000000010000 en el nombre del ticket',
      );
      expect(rows[2]).toHaveTextContent('Actualización de Super Mario Odyssey');
    });
  });

  describe('systems batch 3 (spec §9.1)', () => {
    const SYSTEMS: SystemInfo[] = [
      {
        id: 'ps4',
        name: 'Sony PlayStation 4',
        family: 'console',
        no_emulator: false,
      },
      {
        id: 'xboxseries',
        name: 'Microsoft Xbox Series X|S',
        family: 'console',
        no_emulator: true,
      },
    ];
    const names = (id: string) => SYSTEMS.find((s) => s.id === id)?.name ?? id;
    const bloodborne = autoItem(0, 'CUSA00900', 'ps4', 'ps4', {
      kind: 'folder',
      size: 48_120_000_000,
      target: 'ps4/CUSA00900.ps4',
      method: 'header',
      confidence: 0.95,
      why: [
        {
          kind: 'header',
          system: 'ps4',
          weight: 0.95,
          detail: 'sce_sys/param.sfo',
        },
      ],
      title_id: { id: 'CUSA00900', source: 'param-sfo' },
      content_id: 'UP9000-CUSA00900_00-BLOODBORNE000000',
    });
    const forza = autoItem(1, 'Forza Horizon 5', 'xboxseries', 'xboxseries', {
      kind: 'folder',
      size: 110_400_000_000,
      method: 'header',
      confidence: 0.95,
      why: [
        {
          kind: 'header',
          system: 'xboxseries',
          weight: 0.95,
          detail: 'MicrosoftGame.config',
        },
      ],
      title_id: { id: '2B3C4D5E', source: 'microsoft-game-config' },
    });

    function renderIn(locale: 'es' | 'en', systems?: SystemInfo[]) {
      render(
        <I18nProvider locale={locale}>
          <SystemGroups
            groups={groupBySystem(planOf([bloodborne, forza]))}
            layoutName="Batocera / Knulli / EmulationStation"
            systemName={names}
            {...(systems ? { systems } : {})}
            profile={null}
          />
        </I18nProvider>,
      );
    }

    it('says «Sin emulador» once, in the Xbox Series heading only', () => {
      renderIn('es', SYSTEMS);
      expect(
        screen.getByRole('button', {
          name: 'xboxseries/ (1) Microsoft Xbox Series X|S Sin emulador',
        }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole('button', { name: 'ps4/ (1) Sony PlayStation 4' }),
      ).toBeInTheDocument();
      const labels = screen.getAllByText('Sin emulador');
      expect(labels).toHaveLength(1);
      // Secondary caption text, no badge; the dot between is not read.
      expect(labels[0]).toHaveClass('text-caption', 'text-cr-text-secondary');
      const dot = labels[0]!.previousElementSibling!;
      expect(dot).toHaveTextContent('·');
      expect(dot).toHaveAttribute('aria-hidden', 'true');
      // Never on a row.
      for (const row of screen.getAllByRole('listitem'))
        expect(row).not.toHaveTextContent('Sin emulador');
    });

    it('reads «No emulator» in English and nothing without the catalogue', () => {
      renderIn('en', SYSTEMS);
      expect(
        screen.getByRole('button', {
          name: 'xboxseries/ (1) Microsoft Xbox Series X|S No emulator',
        }),
      ).toBeInTheDocument();
      cleanup();
      renderIn('en');
      expect(screen.queryByText('No emulator')).toBeNull();
    });

    it('a PS4 folder tells its title id file and its content id in mono', () => {
      renderIn('es', SYSTEMS);
      const row = screen.getAllByRole('listitem')[0]!;
      expect(row).toHaveTextContent('Title ID CUSA00900 en PARAM.SFO');
      expect(row).toHaveTextContent(
        'Content ID UP9000-CUSA00900_00-BLOODBORNE000000',
      );
      expect(
        within(row)
          .getByText('UP9000-CUSA00900_00-BLOODBORNE000000')
          .closest('.mono'),
      ).not.toBeNull();
      const xbox = screen.getAllByRole('listitem')[1]!;
      expect(xbox).toHaveTextContent(
        'Title ID 2B3C4D5E en MicrosoftGame.config',
      );
      expect(xbox).not.toHaveTextContent('Content ID');
    });
  });
});
