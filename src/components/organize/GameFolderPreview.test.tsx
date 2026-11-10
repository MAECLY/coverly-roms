import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { GameFolderPreview } from '@/components/organize/GameFolderPreview';
import { I18nProvider } from '@/i18n/I18nProvider';
import type { OrganizeItem, TranslatedGameView } from '@/ipc/types';
import { autoItem } from '@/test/organizeFixtures';

const DIR = 'ROMS/3DS/Pokemon Y (World)';

function pokemon(extra: { exists?: boolean; dlc?: number } = {}) {
  const base = autoItem(0, 'Pokemon Y.cia', '3ds', 'ROMS/3DS', {
    target: `${DIR}/Pokemon Y (World).cia`,
    game: {
      role: 'base',
      folder: DIR,
      ...(extra.exists && { folder_exists: true }),
    },
  });
  const update = autoItem(1, '0004000E00055D00.cia', '3ds', 'ROMS/3DS', {
    target: `${DIR}/Updates/Pokemon Y (World) (v1.1.0).cia`,
    game: { role: 'update', of: 0, version: '1.1.0' },
  });
  const dlc = Array.from({ length: extra.dlc ?? 2 }, (_, i) =>
    autoItem(i + 2, `pack ${i}.cia`, '3ds', 'ROMS/3DS', {
      target: `${DIR}/DLC/Pokemon Y (World) - Pack ${String(i + 1).padStart(2, '0')}.cia`,
      game: { role: 'dlc', of: 0 },
    }),
  );
  return { base, items: [base, update, ...dlc] as OrganizeItem[] };
}

/** Text split across elements (a mono count): the innermost element
 * whose whole text it is. */
const whole =
  (text: string) =>
  (_: string, el: Element | null): boolean =>
    el?.textContent === text &&
    ![...el.children].some((c) => c.textContent === text);

function renderPreview(
  base: OrganizeItem,
  items: OrganizeItem[],
  locale: 'es' | 'en' = 'es',
  translated?: TranslatedGameView,
) {
  return render(
    <I18nProvider locale={locale}>
      <GameFolderPreview
        base={base}
        items={items}
        {...(translated && { translated })}
      />
    </I18nProvider>,
  );
}

const PS4 = 'ROMS/PS4';

/** A PS4 base laid out for shadPS4: the game and its update in the system
 * folder, a mod in shadPS4's own folder, a DLC that stays. */
function shadps4(extra: { updates?: number } = {}) {
  const base = autoItem(0, 'CUSA00900', 'ps4', PS4, {
    kind: 'folder',
    target: `${PS4}/Bloodborne (Europe)/CUSA00900`,
    game: { role: 'base', folder: `${PS4}/Bloodborne (Europe)` },
  });
  const updates = Array.from({ length: extra.updates ?? 1 }, (_, i) => ({
    from: `Bloodborne (Europe)/Updates/CUSA00900 v1.${String(i).padStart(2, '0')}`,
    to: {
      kind: 'system' as const,
      path: `CUSA00900-UPDATE${i === 0 ? '' : `-${i}`}`,
    },
  }));
  const view: TranslatedGameView = {
    system: 'ps4',
    base: 0,
    emulator: 'shadps4',
    placements: [
      {
        from: 'Bloodborne (Europe)/CUSA00900',
        to: { kind: 'system', path: 'CUSA00900' },
      },
      ...updates,
      {
        from: 'Bloodborne (Europe)/Mods/CUSA00900',
        to: { kind: 'emulator-data', path: 'mods/CUSA00900' },
      },
      {
        from: 'Bloodborne (Europe)/DLC/EP9000-CUSA00900_00-BBDLC',
        to: { kind: 'library' },
        note: { kind: 'install-in-emulator', emulator: 'shadps4' },
      },
    ],
  };
  return { base, view };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  try {
    window.sessionStorage.clear();
  } catch {
    /* no storage */
  }
});

describe('GameFolderPreview', () => {
  it('starts collapsed and builds nothing until opened', () => {
    const { base, items } = pokemon();
    renderPreview(base, items);
    const button = screen.getByRole('button', {
      name: 'Ver cómo queda la carpeta',
    });
    expect(button).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByRole('list')).toBeNull();
  });

  it('shows the folder as nested lists, fixed folders translated with their disk name', async () => {
    const { base, items } = pokemon();
    renderPreview(base, items);
    const button = screen.getByRole('button', {
      name: 'Ver cómo queda la carpeta',
    });
    await userEvent.click(button);
    expect(button).toHaveAttribute('aria-expanded', 'true');
    expect(button).toHaveAccessibleName('Ocultar la carpeta');
    const tree = screen.getByRole('list', {
      name: 'Carpeta de Pokemon Y (World)',
    });
    expect(within(tree).getByText('Pokemon Y (World)/')).toBeInTheDocument();
    expect(within(tree).getByText('Pokemon Y (World).cia')).toBeInTheDocument();
    expect(
      within(tree).getByText(whole('Actualizaciones (1)')),
    ).toBeInTheDocument();
    expect(
      within(tree).getByText(whole('Contenido descargable (2)')),
    ).toBeInTheDocument();
    expect(within(tree).getByText('Updates/')).toBeInTheDocument();
    expect(within(tree).getByText('DLC/')).toBeInTheDocument();
    expect(
      within(tree).getByText('Pokemon Y (World) (v1.1.0).cia'),
    ).toBeInTheDocument();
    // Real nesting: the game folder holds a list, each fixed folder too.
    expect(within(tree).getAllByRole('list')).toHaveLength(3);
    expect(screen.queryByText('ya existe')).toBeNull();
    // Closing keeps focus on the button.
    await userEvent.click(button);
    expect(button).toHaveFocus();
    expect(button).toHaveAttribute('aria-expanded', 'false');
  });

  it('caps a folder at 20 names and marks a folder already there', async () => {
    const { base, items } = pokemon({ exists: true, dlc: 25 });
    renderPreview(base, items, 'en');
    await userEvent.click(
      screen.getByRole('button', { name: 'Show how the folder will look' }),
    );
    const tree = screen.getByRole('list', {
      name: 'Folder of Pokemon Y (World)',
    });
    expect(
      within(tree).getByText(whole('Downloadable content (25)')),
    ).toBeInTheDocument();
    expect(within(tree).getByText(whole('and 5 more'))).toBeInTheDocument();
    expect(
      within(tree).queryByText('Pokemon Y (World) - Pack 21.cia'),
    ).toBeNull();
    expect(within(tree).getByText('already there')).toBeInTheDocument();
  });

  it('sets the counts in mono, like every number (brand §7.6)', async () => {
    const { base, items } = pokemon({ dlc: 25 });
    renderPreview(base, items, 'en');
    await userEvent.click(
      screen.getByRole('button', { name: 'Show how the folder will look' }),
    );
    const tree = screen.getByRole('list', {
      name: 'Folder of Pokemon Y (World)',
    });
    for (const n of ['1', '25', '5']) {
      const nodes = within(tree).getAllByText(n);
      expect(
        nodes.some((el) => el.closest('.mono')),
        `count ${n} in mono`,
      ).toBe(true);
    }
    // The translated folder name stays in the body face.
    expect(within(tree).getByText('Updates').closest('.mono')).toBeNull();
  });

  it('remembers the open state per base for the session', async () => {
    const { base, items } = pokemon();
    const first = renderPreview(base, items);
    await userEvent.click(
      screen.getByRole('button', { name: 'Ver cómo queda la carpeta' }),
    );
    first.unmount();
    renderPreview(base, items);
    expect(
      screen.getByRole('button', { name: 'Ocultar la carpeta' }),
    ).toHaveAttribute('aria-expanded', 'true');
  });

  it('stays collapsed and works when storage throws', async () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    const { base, items } = pokemon();
    renderPreview(base, items);
    const button = screen.getByRole('button', {
      name: 'Ver cómo queda la carpeta',
    });
    expect(button).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(button);
    expect(button).toHaveAttribute('aria-expanded', 'true');
  });

  it('adds Mods/ and Cheats/ after the updates and DLC when the game has them', async () => {
    const { base, items } = pokemon({ dlc: 1 });
    const key = '0004000000055D00';
    const mod = autoItem(7, key, '3ds', 'ROMS/3DS', {
      kind: 'folder',
      target: `${DIR}/Mods/${key}`,
      game: {
        role: 'mod',
        of: 0,
        addon: { kind: 'mod', format: 'layered-fs', key_text: key, via: 'key' },
      },
    });
    const cheat = autoItem(8, `${key}.txt`, '3ds', 'ROMS/3DS', {
      target: `${DIR}/Cheats/${key}.txt`,
      game: {
        role: 'cheat',
        of: 0,
        addon: {
          kind: 'cheat',
          format: 'ctr-cheats',
          key_text: key,
          via: 'key',
        },
      },
    });
    renderPreview(base, [...items, cheat, mod]);
    await userEvent.click(
      screen.getByRole('button', { name: 'Ver cómo queda la carpeta' }),
    );
    const tree = screen.getByRole('list', {
      name: 'Carpeta de Pokemon Y (World)',
    });
    const heads = [
      'Actualizaciones (1)',
      'Contenido descargable (1)',
      'Mods (1)',
      'Trucos (1)',
    ].map((h) => within(tree).getByText(whole(h)));
    // In the order of the fixed folders on disk.
    for (let i = 1; i < heads.length; i += 1)
      expect(
        heads[i - 1]!.compareDocumentPosition(heads[i]!) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    expect(within(tree).getByText('Mods/')).toBeInTheDocument();
    expect(within(tree).getByText('Cheats/')).toBeInTheDocument();
    expect(within(tree).getByText(`${key}/`)).toHaveClass('font-mono');
    expect(within(tree).getByText(`${key}.txt`)).toBeInTheDocument();
  });

  it('shows no Mods/ or Cheats/ when the game has none', async () => {
    const { base, items } = pokemon();
    renderPreview(base, items);
    await userEvent.click(
      screen.getByRole('button', { name: 'Ver cómo queda la carpeta' }),
    );
    expect(screen.queryByText(whole('Mods (1)'))).toBeNull();
    expect(screen.queryByText('Cheats/')).toBeNull();
  });

  it('renders nothing for a base without children', () => {
    const { base } = pokemon();
    const { container } = renderPreview(base, [base]);
    expect(container).toBeEmptyDOMElement();
  });

  describe('for an emulator', () => {
    it('names the emulator on the disclosure and builds nothing until opened', () => {
      const { base, view } = shadps4();
      renderPreview(base, [base], 'es', view);
      const button = screen.getByRole('button', {
        name: 'Ver cómo queda para shadPS4',
      });
      expect(button).toHaveAttribute('aria-expanded', 'false');
      expect(screen.queryByRole('list')).toBeNull();
    });

    it('shows the system folder, the emulator folder and what stays', async () => {
      const { base, view } = shadps4();
      renderPreview(base, [base], 'es', view);
      await userEvent.click(
        screen.getByRole('button', { name: 'Ver cómo queda para shadPS4' }),
      );
      const tree = screen.getByRole('list', {
        name: 'Carpeta de Bloodborne (Europe) para shadPS4',
      });
      const heads = [
        'En la carpeta del sistema',
        'En la carpeta de shadPS4',
        'Se queda en la biblioteca',
      ].map((h) => within(tree).getByText(h));
      for (let i = 1; i < heads.length; i += 1)
        expect(
          heads[i - 1]!.compareDocumentPosition(heads[i]!) &
            Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
      expect(within(tree).getByText('CUSA00900')).toHaveClass('font-mono');
      expect(within(tree).getByText('CUSA00900-UPDATE')).toHaveClass(
        'font-mono',
      );
      expect(within(tree).getByText('mods/CUSA00900')).toHaveClass('font-mono');
      // The app never knows the emulator's folder: it says so.
      expect(
        within(tree).getByText('Coverly no la toca si no le dices dónde está'),
      ).toBeInTheDocument();
      expect(
        within(tree).getByText(
          'Bloodborne (Europe)/DLC/EP9000-CUSA00900_00-BBDLC',
        ),
      ).toBeInTheDocument();
      expect(
        within(tree).getByText('Instálalo en el emulador'),
      ).toBeInTheDocument();
    });

    it('says why a patch stays in the library instead of «install it»', async () => {
      const base = autoItem(0, 'Mother (Japan).nes', 'nes', 'ROMS/FC', {
        target: 'ROMS/FC/Mother (Japan).nes',
      });
      const view: TranslatedGameView = {
        system: 'nes',
        base: 0,
        emulator: 'retroarch',
        placements: [
          {
            from: 'Mother (Japan).nes',
            to: { kind: 'system', path: 'Mother (Japan).nes' },
          },
          {
            from: 'Mother (Japan) [T-En].bps',
            to: { kind: 'library' },
            note: { kind: 'header-mismatch', needs: 'needs-headerless' },
          },
          {
            from: 'Mother (Japan) [Hack].ips',
            to: { kind: 'library' },
            note: {
              kind: 'not-supported',
              emulator: 'retroarch',
              why: 'several-patches',
            },
          },
        ],
      };
      renderPreview(base, [base], 'es', view);
      await userEvent.click(
        screen.getByRole('button', { name: 'Ver cómo queda para RetroArch' }),
      );
      const tree = screen.getByRole('list', {
        name: 'Carpeta de Mother (Japan) para RetroArch',
      });
      expect(
        within(tree).getByText('Este parche espera la ROM sin cabecera'),
      ).toBeInTheDocument();
      expect(
        within(tree).getByText(
          'RetroArch no lo usa así: Varios parches para la misma ROM; RetroArch solo aplica uno',
        ),
      ).toBeInTheDocument();
      // Nothing here is installed in the emulator.
      expect(within(tree).queryByText('Instálalo en el emulador')).toBeNull();
    });

    it('leaves out a list with nothing in it', async () => {
      const { base, view } = shadps4();
      const only = { ...view, placements: view.placements.slice(0, 2) };
      renderPreview(base, [base], 'en', only);
      await userEvent.click(
        screen.getByRole('button', { name: 'Show the layout for shadPS4' }),
      );
      expect(screen.getByText('In the system folder')).toBeInTheDocument();
      expect(screen.queryByText("In shadPS4's folder")).toBeNull();
      expect(screen.queryByText('Stays in the library')).toBeNull();
    });

    it('caps a list at 20 entries', async () => {
      const { base, view } = shadps4({ updates: 24 });
      renderPreview(base, [base], 'en', view);
      await userEvent.click(
        screen.getByRole('button', { name: 'Show the layout for shadPS4' }),
      );
      const tree = screen.getByRole('list', {
        name: 'Folder of Bloodborne (Europe) for shadPS4',
      });
      // The base and 19 updates, then the other 5 counted.
      expect(within(tree).getByText('CUSA00900-UPDATE-18')).toBeInTheDocument();
      expect(within(tree).queryByText('CUSA00900-UPDATE-19')).toBeNull();
      expect(within(tree).getByText(whole('and 5 more'))).toBeInTheDocument();
    });

    it('drops the collapsible motion under reduced motion', () => {
      const { base, view } = shadps4();
      renderPreview(base, [base], 'es', view);
      const button = screen.getByRole('button', {
        name: 'Ver cómo queda para shadPS4',
      });
      expect(button.querySelector('svg')).toHaveClass(
        'motion-reduce:transition-none',
      );
    });
  });
});
