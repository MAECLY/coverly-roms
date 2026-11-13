import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { SystemGroups } from '@/components/organize/SystemGroups';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { groupBySystem } from '@/lib/organize';
import type {
  ArtImage,
  EmulatorChoice,
  OrganizeItem,
  SystemInfo,
  TransferMode,
} from '@/ipc/types';
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

  it('gives the changeable system select an id and the name "system"', () => {
    renderChosen(() => {});
    const select = screen.getByRole('combobox', {
      name: 'System for mystery.rom',
    });
    expect(select.id).not.toBe('');
    expect(select).toHaveAttribute('name', 'system');
  });

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

    it('a mod folder that moves is read as a mod folder', () => {
      renderIn('en', [
        {
          ...autoItem(2, 'textures/BLUS30443', 'ps3', 'ps3', {}),
          kind: 'folder',
          size: 1_200_000,
          game: { role: 'mod', of: 9 },
        } as OrganizeItem,
      ]);
      expect(screen.getByText('Mod folder')).toHaveClass('sr-only');
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
      expect(labels[0]!.parentElement).toHaveClass(
        'text-caption',
        'text-cr-text-secondary',
      );
      const dot = labels[0]!.previousElementSibling!;
      expect(dot).toHaveTextContent('·');
      expect(dot).toHaveAttribute('aria-hidden', 'true');
      // Never on a row.
      for (const row of screen.getAllByRole('listitem'))
        expect(row).not.toHaveTextContent('Sin emulador');
    });

    it('keeps «Sin emulador» in one line with the system name at 390 px', () => {
      renderIn('es', SYSTEMS);
      const label = screen.getByText('Sin emulador');
      // Never broken inside («Sin / emulador»), and set in the same text
      // run as the system name, so a narrow window wraps the phrase as a
      // sentence instead of stacking each part as its own flex column.
      expect(label).toHaveClass('whitespace-nowrap');
      expect(label.parentElement).toHaveTextContent(
        'Microsoft Xbox Series X|S · Sin emulador',
      );
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

describe('SystemGroups (adjustments)', () => {
  function renderEs(items: OrganizeItem[]) {
    render(
      <I18nProvider locale="es">
        <SystemGroups
          groups={groupBySystem(planOf(items))}
          layoutName="Anbernic (stock)"
          systemName={systemName}
          mode="move"
          profile={null}
        />
      </I18nProvider>,
    );
  }

  it('an unresolved title is labelled «Cifrado» once, as a word with its reason', () => {
    renderEs([
      autoItem(0, '0100F2C0115B6000.xci', 'switch', 'ROMS/SWITCH', {
        method: 'header',
        why: [
          { kind: 'header', system: 'switch', weight: 0.95, detail: 'HEAD' },
        ],
        notes: [{ kind: 'encrypted-name' }, { kind: 'encrypted' }],
      }),
    ]);
    const label = screen.getByText('Cifrado');
    expect(label).toHaveAccessibleDescription(
      'No se puede leer el nombre sin descifrarlo',
    );
    // The reason is a visible line, not only a description.
    expect(
      screen.getByText('No se puede leer el nombre sin descifrarlo'),
    ).toBeVisible();
    // «Cifrado» is said once: the older note line is not repeated.
    expect(document.body.textContent!.match(/Cifrado/g)).toHaveLength(1);
  });

  it('a resolved encrypted 3DS game shows only its existing note', () => {
    renderEs([
      autoItem(0, 'Pokemon Y (Europe).3ds', '3ds', 'ROMS/3DS', {
        notes: [{ kind: 'encrypted-content', emulator: 'Azahar' }],
      }),
    ]);
    expect(screen.queryByText('Cifrado')).toBeNull();
    expect(
      screen.getByText('Copia cifrada: Azahar solo abre copias descifradas'),
    ).toBeInTheDocument();
  });

  it('homebrew, a catalogue name and a disc set playlist each get their line', () => {
    renderEs([
      autoItem(0, 'hbmenu.nro', 'switch', 'ROMS/SWITCH', {
        notes: [{ kind: 'homebrew' }],
      }),
      autoItem(1, 'Cartridge.xci', 'switch', 'ROMS/SWITCH', {
        catalogue_name: 'Super Mario Odyssey',
        title_id: { id: '0100000000010000', source: 'nca-catalog' },
        title_source: 'titledb',
      }),
      autoItem(2, 'Grandia (Japan) (Disc 1).cue', 'saturn', 'ROMS/SATURN', {
        kind: 'set',
        sources: [
          'Grandia (Japan) (Disc 1).cue',
          'Grandia (Japan) (Disc 3).cue',
        ],
        notes: [{ kind: 'missing-disc', n: 2 }],
        playlist: {
          path: 'ROMS/SATURN/Grandia (Japan).m3u',
          discs: [
            'ROMS/SATURN/Grandia (Japan) (Disc 1).cue',
            'ROMS/SATURN/Grandia (Japan) (Disc 3).cue',
          ],
        },
      }),
    ]);
    expect(screen.getByText('Homebrew')).toBeInTheDocument();
    const source = screen.getByText(
      (_, el) =>
        el?.tagName === 'P' &&
        el.textContent ===
          'Nombre del catálogo de blawar/titledb: Super Mario Odyssey',
    );
    // A game name, not a number or a code: in the body face.
    expect(source.querySelector('.mono')).toBeNull();
    expect(
      screen.getByText('Title ID por el id del NCA en el catálogo'),
    ).toBeInTheDocument();
    expect(screen.getByText('Falta el disco 2')).toBeInTheDocument();
    expect(
      screen.getByText('Se crea Grandia (Japan).m3u para cambiar de disco'),
    ).toBeInTheDocument();
  });
});

describe('SystemGroups game folders (spec §9.1)', () => {
  const DIR = 'ROMS/3DS/Pokemon Y (World)';
  const pokemon = () => [
    autoItem(1, '0004000E00055D00.cia', '3ds', 'ROMS/3DS', {
      target: `${DIR}/Updates/Pokemon Y (World) (v1.1.0).cia`,
      notes: [{ kind: 'update', of: 'Pokemon Y' }],
      game: { role: 'update', of: 0, version: '1.1.0', newest_here: true },
    }),
    autoItem(0, 'Pokemon Y.cia', '3ds', 'ROMS/3DS', {
      target: `${DIR}/Pokemon Y (World).cia`,
      rename: { kind: 'game-folder', from: 'Pokemon Y.cia' },
      game: { role: 'base', folder: DIR },
      companions: [
        {
          source: 'Pokemon Y.tik',
          target: `${DIR}/Pokemon Y.tik`,
          licence: true,
        },
      ],
    }),
    autoItem(2, 'Pokemon Y [DLC2].cia', '3ds', 'ROMS/3DS', {
      target: `${DIR}/DLC/Pokemon Y (World) - Pack 1.cia`,
      notes: [{ kind: 'dlc', of: 'Pokemon Y' }],
      game: { role: 'dlc', of: 0 },
    }),
  ];

  function renderEs(items: OrganizeItem[], licenceStays?: Set<number>) {
    render(
      <I18nProvider locale="es">
        <SystemGroups
          groups={groupBySystem(planOf(items))}
          layoutName="Coverly"
          systemName={(id) => id}
          profile={null}
          {...(licenceStays && { licenceStays })}
        />
      </I18nProvider>,
    );
  }

  it('folds children under their base, with the version as text and the target short', () => {
    renderEs(pokemon());
    const rows = screen.getAllByRole('listitem');
    expect(rows[0]).toHaveTextContent('Pokemon Y.cia');
    expect(rows[0]).toHaveTextContent(
      'Renombrado para su carpeta; antes era Pokemon Y.cia',
    );
    expect(rows[0]).toHaveTextContent(
      'Su licencia va con él; Coverly no la abre',
    );
    const update = rows.find((r) =>
      r.textContent?.includes('0004000E00055D00.cia'),
    )!;
    // The version reads as a word, the mark after it is text.
    const version = within(update).getByText('v1.1.0');
    expect(version).toHaveClass('font-mono');
    expect(version).toHaveAttribute('aria-hidden', 'true');
    expect(within(update).getByText('versión 1.1.0')).toHaveClass('sr-only');
    expect(within(update).getByText('la más nueva aquí')).toBeInTheDocument();
    // The target shortened to its fixed folder, the full path kept.
    const full = `${DIR}/Updates/Pokemon Y (World) (v1.1.0).cia`;
    const short = within(update).getByTitle(full);
    expect(short).toHaveTextContent('…/Updates/Pokemon Y (World) (v1.1.0).cia');
    expect(within(update).getByText(`→ ${full}`)).toHaveClass('sr-only');
    // The DLC has no version line.
    const dlc = rows.find((r) => r.textContent?.includes('[DLC2]'))!;
    expect(within(dlc).queryByText(/^versión/)).toBeNull();
  });

  it('names the group after the system folder, not the game folder', () => {
    renderEs(pokemon());
    expect(
      screen.getByRole('group', { name: /^ROMS\/3DS\/ \(3\)/ }),
    ).toBeInTheDocument();
  });

  it('the base row offers the folder preview', async () => {
    renderEs(pokemon());
    const button = screen.getByRole('button', {
      name: 'Ver cómo queda la carpeta',
    });
    await userEvent.click(button);
    expect(
      screen.getByRole('list', { name: 'Carpeta de Pokemon Y (World)' }),
    ).toBeInTheDocument();
    window.sessionStorage.clear();
  });

  it('a base whose licence the core left says it stays', () => {
    const [update, base, dlc] = pokemon();
    renderEs([update!, { ...base!, companions: [] }, dlc!], new Set([0]));
    expect(
      screen.getByText('Su licencia se queda donde estaba; Coverly no la abre'),
    ).toBeInTheDocument();
  });
});

describe('SystemGroups mods and cheats (mods spec §9.1)', () => {
  const DIR = 'ROMS/3DS/Pokemon Y (World)';
  const KEY = '0004000000055D00';
  const pile = (): OrganizeItem[] => [
    autoItem(4, `${KEY}.txt`, '3ds', 'ROMS/3DS', {
      target: `${DIR}/Cheats/${KEY}.txt`,
      notes: [{ kind: 'cheat-of', of: 'Pokemon Y' }],
      game: {
        role: 'cheat',
        of: 0,
        addon: {
          kind: 'cheat',
          format: 'ctr-cheats',
          key_text: KEY,
          via: 'key',
        },
      },
    }),
    autoItem(3, KEY, '3ds', 'ROMS/3DS', {
      kind: 'folder',
      target: `${DIR}/Mods/${KEY}`,
      notes: [{ kind: 'mod-of', of: 'Pokemon Y' }],
      game: {
        role: 'mod',
        of: 0,
        addon: { kind: 'mod', format: 'layered-fs', key_text: KEY, via: 'key' },
      },
    }),
    autoItem(0, 'Pokemon Y.cia', '3ds', 'ROMS/3DS', {
      target: `${DIR}/Pokemon Y (World).cia`,
      game: { role: 'base', folder: DIR },
    }),
    autoItem(1, '0004000E00055D00.cia', '3ds', 'ROMS/3DS', {
      target: `${DIR}/Updates/Pokemon Y (World) (v1.1.0).cia`,
      notes: [{ kind: 'update', of: 'Pokemon Y' }],
      game: { role: 'update', of: 0, version: '1.1.0' },
    }),
  ];

  function renderEs(items: OrganizeItem[]) {
    render(
      <I18nProvider locale="es">
        <SystemGroups
          groups={groupBySystem(planOf(items))}
          layoutName="Coverly"
          systemName={(id) => id}
          profile={null}
        />
      </I18nProvider>,
    );
  }

  it('folds a mod and cheats under their game, after its update', () => {
    renderEs(pile());
    const rows = screen.getAllByRole('listitem');
    const at = (text: string) =>
      rows.findIndex((r) => r.textContent?.includes(text));
    const order = ['Pokemon Y.cia', '/Updates/', '/Mods/', '/Cheats/'].map(at);
    expect(order.every((i) => i >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
  });

  it('says what an addon is in words, its key in mono and the proof as text', () => {
    renderEs(pile());
    const mod = screen
      .getAllByRole('listitem')
      .find((r) => r.textContent?.includes('Mods/'))!;
    const owner = within(mod).getByText('Mod de Pokemon Y');
    // Whose it is comes first, before what it is and the proof.
    expect(
      owner.compareDocumentPosition(within(mod).getByText(/^Enlazado por/)) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const keys = within(mod).getAllByText(KEY);
    expect(keys.some((k) => k.closest('.mono'))).toBe(true);
    expect(mod).toHaveTextContent(`Mod · LayeredFS ${KEY}`);
    expect(mod).toHaveTextContent(`Enlazado por ${KEY}`);
    // The target is short, the whole path kept for its tooltip.
    expect(within(mod).getByTitle(`${DIR}/Mods/${KEY}`)).toHaveTextContent(
      `…/Mods/${KEY}`,
    );
    const cheat = screen
      .getAllByRole('listitem')
      .find((r) => r.textContent?.includes('Cheats/'))!;
    expect(within(cheat).getByText('Trucos de Pokemon Y')).toBeInTheDocument();
    expect(cheat).toHaveTextContent(`Trucos · Azahar ${KEY}`);
  });

  it('a patch says it was linked by the source ROM and which header it needs', () => {
    const base = autoItem(0, 'Mother (Japan).nes', 'nes', 'ROMS/NES', {
      target: 'ROMS/NES/Mother (Japan)/Mother (Japan).nes',
      game: { role: 'base', folder: 'ROMS/NES/Mother (Japan)' },
    });
    const patch = autoItem(1, 'Mother (Japan) [T-En].bps', 'nes', 'ROMS/NES', {
      target: 'ROMS/NES/Mother (Japan)/Mods/Mother (Japan) [T-En].bps',
      notes: [
        { kind: 'patch-of', of: 'Mother (Japan)' },
        { kind: 'header-mismatch', needs: 'needs-headerless' },
      ],
      game: {
        role: 'mod',
        of: 0,
        addon: {
          kind: 'patch',
          format: 'bps',
          key_text: 'CRC32 7F8E1A20',
          via: 'source-crc',
          header: 'needs-headerless',
        },
      },
    });
    renderEs([base, patch]);
    const row = screen
      .getAllByRole('listitem')
      .find((r) => r.textContent?.includes('[T-En]'))!;
    expect(
      within(row).getByText('Parche de Mother (Japan)'),
    ).toBeInTheDocument();
    expect(row).toHaveTextContent(
      'Parche de traducción o hack · BPS CRC32 7F8E1A20',
    );
    expect(
      within(row).getByText('Enlazado por el CRC32 de la ROM de origen'),
    ).toBeInTheDocument();
    expect(
      within(row).getByText('Este parche espera la ROM sin cabecera'),
    ).toBeInTheDocument();
  });

  it('a plan from before mods draws no addon line', () => {
    renderEs(pile().slice(2));
    expect(screen.queryByText(/^Enlazado por/)).toBeNull();
  });
});

describe('SystemGroups: the emulator of a system', () => {
  const ps4 = autoItem(0, 'CUSA00900', 'ps4', 'ROMS/PS4', { kind: 'folder' });
  const odd = autoItem(1, 'thing.bin', 'nothing-runs-it', 'ROMS/ODD');
  const ES_DE = {
    disc_layout: { kind: 'flat' as const, m3u_for_multi: false },
    emulators: { ps4: 'shadps4' as const },
  };

  function renderChoice(
    props: {
      chosen?: Record<string, EmulatorChoice>;
      disabled?: boolean;
      profile?: typeof ES_DE | null;
      onEmulator?: (system: string, choice: EmulatorChoice | null) => void;
    } = {},
  ) {
    const onEmulator = props.onEmulator ?? vi.fn();
    const ui = (disabled: boolean) => (
      <I18nProvider locale="es">
        <SystemGroups
          groups={groupBySystem(planOf([ps4, odd]))}
          layoutName="ES-DE"
          systemName={(id) => (id === 'ps4' ? 'PlayStation 4' : id)}
          profile={props.profile === undefined ? ES_DE : props.profile}
          emulators={props.chosen ?? {}}
          onEmulator={onEmulator}
          disabled={disabled}
        />
      </I18nProvider>
    );
    const view = render(ui(props.disabled ?? false));
    return {
      onEmulator,
      rerender: (disabled: boolean) => view.rerender(ui(disabled)),
    };
  }

  afterEach(cleanup);

  it('shows a labelled select only for a system with translators', () => {
    renderChoice();
    const select = screen.getByRole('combobox', { name: 'Emulador' });
    expect(select).toHaveAttribute('name', 'emulator');
    expect(select.id).toMatch(/emulator-ps4$/);
    // A real, visible label, and the group's system name describes it.
    const label = document.querySelector(`label[for="${select.id}"]`);
    expect(label).toHaveTextContent('Emulador');
    expect(select).toHaveAccessibleDescription('PlayStation 4');
    expect(screen.getAllByRole('combobox', { name: 'Emulador' })).toHaveLength(
      1,
    );
  });

  it('offers the default, every emulator of the system and none', () => {
    renderChoice();
    const select = screen.getByRole('combobox', { name: 'Emulador' });
    expect(
      within(select)
        .getAllByRole('option')
        .map((o) => o.textContent),
    ).toEqual([
      'Predeterminado: shadPS4',
      'shadPS4',
      'Ninguno (estructura de la biblioteca)',
    ]);
    expect(select).toHaveValue('');
  });

  it('sends nothing for the default, the emulator or none when picked', async () => {
    const { onEmulator } = renderChoice({ chosen: { ps4: 'none' } });
    const select = screen.getByRole('combobox', { name: 'Emulador' });
    expect(select).toHaveValue('none');
    await userEvent.selectOptions(select, 'shadps4');
    expect(onEmulator).toHaveBeenLastCalledWith('ps4', 'shadps4');
    await userEvent.selectOptions(select, '');
    expect(onEmulator).toHaveBeenLastCalledWith('ps4', null);
  });

  it('picks none explicitly', async () => {
    const { onEmulator } = renderChoice();
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Emulador' }),
      'none',
    );
    expect(onEmulator).toHaveBeenLastCalledWith('ps4', 'none');
  });

  it('without a default, none is the default and sends nothing', () => {
    renderChoice({ profile: null });
    const select = screen.getByRole('combobox', { name: 'Emulador' });
    expect(
      within(select)
        .getAllByRole('option')
        .map((o) => [o.getAttribute('value'), o.textContent]),
    ).toEqual([
      ['shadps4', 'shadPS4'],
      ['', 'Ninguno (estructura de la biblioteca)'],
    ]);
    expect(select).toHaveValue('');
  });

  it('is disabled while planning and keeps focus after the re-plan', async () => {
    const { rerender } = renderChoice();
    const select = screen.getByRole('combobox', { name: 'Emulador' });
    await userEvent.selectOptions(select, 'shadps4');
    select.focus();
    rerender(true);
    expect(screen.getByRole('combobox', { name: 'Emulador' })).toBeDisabled();
    rerender(false);
    const after = screen.getByRole('combobox', { name: 'Emulador' });
    expect(after).toBe(select);
    expect(after).toHaveFocus();
  });

  it('draws no select without a handler (a plan shown read-only)', () => {
    render(
      <I18nProvider locale="es">
        <SystemGroups
          groups={groupBySystem(planOf([ps4]))}
          layoutName="ES-DE"
          systemName={(id) => id}
          profile={ES_DE}
        />
      </I18nProvider>,
    );
    expect(screen.queryByRole('combobox', { name: 'Emulador' })).toBeNull();
  });

  it('hands a base its translated layout for the preview', () => {
    const base = autoItem(0, 'CUSA00900', 'ps4', 'ROMS/PS4', {
      kind: 'folder',
      target: 'ROMS/PS4/Bloodborne (Europe)/CUSA00900',
      game: { role: 'base', folder: 'ROMS/PS4/Bloodborne (Europe)' },
    });
    render(
      <I18nProvider locale="es">
        <SystemGroups
          groups={groupBySystem(planOf([base]))}
          layoutName="ES-DE"
          systemName={(id) => id}
          profile={ES_DE}
          translated={[
            {
              system: 'ps4',
              base: 0,
              emulator: 'shadps4',
              placements: [
                {
                  from: 'Bloodborne (Europe)/CUSA00900',
                  to: { kind: 'system', path: 'CUSA00900' },
                },
              ],
            },
          ]}
        />
      </I18nProvider>,
    );
    expect(
      screen.getByRole('button', { name: 'Ver cómo queda para shadPS4' }),
    ).toBeInTheDocument();
  });

  it('previews the layout of a base without a game folder too', () => {
    // Game folders off (ES-DE): the base has no game link, only its view.
    const base = autoItem(5, 'CUSA04943', 'ps4', 'ROMs/ps4', {
      kind: 'folder',
    });
    render(
      <I18nProvider locale="es">
        <SystemGroups
          groups={groupBySystem(planOf([base]))}
          layoutName="ES-DE"
          systemName={(id) => id}
          profile={ES_DE}
          translated={[
            {
              system: 'ps4',
              base: 5,
              emulator: 'shadps4',
              placements: [
                {
                  from: 'CUSA04943',
                  to: { kind: 'system', path: 'CUSA04943' },
                },
              ],
            },
          ]}
        />
      </I18nProvider>,
    );
    expect(
      screen.getByRole('button', { name: 'Ver cómo queda para shadPS4' }),
    ).toBeInTheDocument();
  });
});

describe('SystemGroups: child thumbnails (addon views D15)', () => {
  /** A stand-in for the browser's observer: rows are «in view» when the
   * test says so. */
  class FakeObserver {
    static all: FakeObserver[] = [];
    readonly targets = new Set<Element>();
    constructor(private readonly cb: IntersectionObserverCallback) {
      FakeObserver.all.push(this);
    }
    observe(el: Element) {
      this.targets.add(el);
    }
    unobserve(el: Element) {
      this.targets.delete(el);
    }
    disconnect() {
      this.targets.clear();
    }
    takeRecords() {
      return [];
    }
    static showAll() {
      act(() => {
        for (const o of FakeObserver.all) {
          const entries = [...o.targets].map(
            (target) =>
              ({ target, isIntersecting: true }) as IntersectionObserverEntry,
          );
          if (entries.length > 0)
            o.cb(entries, o as unknown as IntersectionObserver);
        }
      });
    }
  }

  /** Backend reads held open until the test settles them. */
  function held() {
    const pending: {
      itemId: number;
      resolve(img: ArtImage): void;
      reject(e: unknown): void;
    }[] = [];
    const calls: number[] = [];
    const organizeChildThumb = vi.fn(
      (_plan: number, itemId: number) =>
        new Promise<ArtImage>((resolve, reject) => {
          calls.push(itemId);
          pending.push({ itemId, resolve, reject });
        }),
    );
    setBackend({ ...createMockBackend({ delayMs: 0 }), organizeChildThumb });
    return { pending, calls, organizeChildThumb };
  }

  const PNG: ArtImage = {
    bytes: new Uint8Array([0x89, 0x50, 0x4e, 0x47]),
    mime: 'image/png',
  };

  const dlcRows = (n: number) => [
    autoItem(0, 'Rock Band 4 (USA).pkg', 'ps4', 'ROMS/PS4', {
      game: { role: 'base', folder: 'ROMS/PS4/Rock Band 4 (USA)' },
    }),
    ...Array.from({ length: n }, (_, i) =>
      autoItem(i + 1, `Rock Band 4 - Song ${i + 1}.pkg`, 'ps4', 'ROMS/PS4', {
        game: { role: 'dlc', of: 0 },
      }),
    ),
  ];

  const renderThumbs = (items: OrganizeItem[], planId: number | null = 7) =>
    render(
      <I18nProvider locale="es">
        <SystemGroups
          groups={groupBySystem(planOf(items))}
          layoutName="ES-DE"
          systemName={(id) => id}
          planId={planId}
        />
      </I18nProvider>,
    );

  beforeEach(() => {
    FakeObserver.all = [];
    vi.stubGlobal('IntersectionObserver', FakeObserver);
    let n = 0;
    vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:${++n}`);
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  });
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    setBackend(null);
  });

  it('reads a child row’s icon once it is in view, by plan and item id', async () => {
    const { pending, organizeChildThumb } = held();
    const { container } = renderThumbs(dlcRows(1));
    // Nothing is read before the row is in view.
    await act(async () => {});
    expect(organizeChildThumb).not.toHaveBeenCalled();
    FakeObserver.showAll();
    await waitFor(() => expect(pending).toHaveLength(1));
    expect(organizeChildThumb).toHaveBeenCalledWith(7, 1);
    await act(async () => pending[0]!.resolve(PNG));
    const img = await waitFor(() => {
      const el = container.querySelector('img');
      expect(el).not.toBeNull();
      return el!;
    });
    expect(img).toHaveAttribute('src', 'blob:1');
    expect(img.closest('.child-thumb')).toHaveAttribute('aria-hidden', 'true');
    // In view again: no second read.
    FakeObserver.showAll();
    await act(async () => {});
    expect(organizeChildThumb).toHaveBeenCalledTimes(1);
  });

  it('never has more than 4 reads in flight', async () => {
    const { pending, calls } = held();
    renderThumbs(dlcRows(10));
    FakeObserver.showAll();
    await waitFor(() => expect(pending).toHaveLength(4));
    let most = 0;
    while (calls.length < 10 || pending.length > 0) {
      most = Math.max(most, pending.length);
      const next = pending.shift();
      if (!next) {
        await act(async () => {});
        continue;
      }
      await act(async () => next.resolve(PNG));
    }
    expect(most).toBe(4);
    expect([...calls].sort((a, b) => a - b)).toEqual([
      1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
    ]);
  });

  it('closing the plan stops the reads still waiting', async () => {
    const { pending, calls } = held();
    const view = renderThumbs(dlcRows(10));
    FakeObserver.showAll();
    await waitFor(() => expect(pending).toHaveLength(4));
    view.unmount();
    for (const p of pending.splice(0)) await act(async () => p.resolve(PNG));
    await act(async () => {});
    expect(calls).toHaveLength(4);
    // An answer that came after closing makes no image URL.
    expect(URL.createObjectURL).not.toHaveBeenCalled();
  });

  it('a child with no art in the clear shows no image and no placeholder', async () => {
    const { pending } = held();
    const { container } = renderThumbs(dlcRows(1));
    FakeObserver.showAll();
    await waitFor(() => expect(pending).toHaveLength(1));
    await act(async () =>
      pending[0]!.reject(new BackendError('art-missing', 'icon')),
    );
    await act(async () => {});
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('.child-thumb')).toBeNull();
    expect(container.querySelector('.art-placeholder')).toBeNull();
  });

  const ui = (items: OrganizeItem[], planId: number | null) => (
    <I18nProvider locale="es">
      <SystemGroups
        groups={groupBySystem(planOf(items))}
        layoutName="ES-DE"
        systemName={(id) => id}
        planId={planId}
      />
    </I18nProvider>
  );

  it('a plan that closes while the list stays drops its waiting reads and icons', async () => {
    const { pending, calls } = held();
    const items = dlcRows(10);
    const view = render(ui(items, 7));
    FakeObserver.showAll();
    await waitFor(() => expect(pending).toHaveLength(4));
    await act(async () => pending.shift()!.resolve(PNG));
    await waitFor(() =>
      expect(view.container.querySelector('img')).not.toBeNull(),
    );
    view.rerender(ui(items, null));
    for (const p of pending.splice(0)) await act(async () => p.resolve(PNG));
    await act(async () => {});
    // Only the first four ever started; the six waiting never did.
    expect(calls).toHaveLength(5);
    expect(view.container.querySelector('img')).toBeNull();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:1');
    expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
  });

  it('a new plan reads its own icons and never shows the old ones', async () => {
    const { pending, organizeChildThumb } = held();
    const items = dlcRows(1);
    const view = render(ui(items, 7));
    FakeObserver.showAll();
    await waitFor(() => expect(pending).toHaveLength(1));
    await act(async () => pending.shift()!.resolve(PNG));
    await waitFor(() =>
      expect(view.container.querySelector('img')).toHaveAttribute(
        'src',
        'blob:1',
      ),
    );
    view.rerender(ui(items, 8));
    expect(view.container.querySelector('img')).toBeNull();
    await waitFor(() => expect(pending).toHaveLength(1));
    expect(organizeChildThumb).toHaveBeenLastCalledWith(8, 1);
    await act(async () => pending.shift()!.resolve(PNG));
    await waitFor(() =>
      expect(view.container.querySelector('img')).toHaveAttribute(
        'src',
        'blob:2',
      ),
    );
  });

  it('a base or a plain ROM reads nothing', async () => {
    const { organizeChildThumb } = held();
    renderThumbs([
      ...dlcRows(0),
      autoItem(5, 'Tetris (USA).gba', 'gba', 'ROMS/GBA'),
    ]);
    FakeObserver.showAll();
    await act(async () => {});
    expect(organizeChildThumb).not.toHaveBeenCalled();
  });
});
