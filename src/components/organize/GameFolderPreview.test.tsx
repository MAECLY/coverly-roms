import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { GameFolderPreview } from '@/components/organize/GameFolderPreview';
import { I18nProvider } from '@/i18n/I18nProvider';
import type { OrganizeItem } from '@/ipc/types';
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
) {
  return render(
    <I18nProvider locale={locale}>
      <GameFolderPreview base={base} items={items} />
    </I18nProvider>,
  );
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
});
