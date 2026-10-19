import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { SystemGroups } from '@/components/organize/SystemGroups';
import { I18nProvider } from '@/i18n/I18nProvider';
import { groupBySystem } from '@/lib/organize';
import { autoItem, planOf } from '@/test/organizeFixtures';

const NAMES: Record<string, string> = {
  gba: 'Game Boy Advance',
  gb: 'Game Boy',
};
const systemName = (id: string) => NAMES[id] ?? id;

function renderGroups(items: ReturnType<typeof autoItem>[]) {
  render(
    <I18nProvider locale="en">
      <SystemGroups
        groups={groupBySystem(planOf(items))}
        layoutName="Anbernic (stock)"
        systemName={systemName}
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
  const SYSTEMS = [
    { id: 'gb', name: 'Game Boy' },
    { id: 'gba', name: 'Game Boy Advance' },
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
