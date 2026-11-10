import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { LeftInPlace } from '@/components/organize/LeftInPlace';
import { I18nProvider } from '@/i18n/I18nProvider';
import type { LeftReason, OrganizeItem, SystemInfo } from '@/ipc/types';
import { autoItem } from '@/test/organizeFixtures';

const unknown = (id: number): OrganizeItem =>
  ({
    ...autoItem(id, `File ${String(id).padStart(3, '0')}.bin`, 'gb', 'X'),
    action: 'left',
    reason: { kind: 'unknown' },
    system_id: null,
    target: null,
  }) as OrganizeItem;

const left = (id: number, name: string, reason: LeftReason): OrganizeItem =>
  ({
    ...unknown(id),
    sources: [name],
    reason,
  }) as OrganizeItem;

/** One system of each family (names as in `systems.toml`), unsorted. */
const SYSTEMS: SystemInfo[] = [
  { id: 'wasm4', name: 'WASM-4', family: 'engine', no_emulator: false },
  {
    id: 'zxspectrum',
    name: 'Sinclair ZX Spectrum',
    family: 'computer',
    no_emulator: false,
  },
  {
    id: 'ps2',
    name: 'Sony PlayStation 2',
    family: 'console',
    no_emulator: false,
  },
  { id: 'mame', name: 'MAME', family: 'arcade', no_emulator: false },
  {
    id: 'gba',
    name: 'Game Boy Advance',
    family: 'handheld',
    no_emulator: false,
  },
  { id: 'jaguar', name: 'Atari Jaguar', family: 'console', no_emulator: false },
  {
    id: 'amstradcpc',
    name: 'Amstrad CPC',
    family: 'computer',
    no_emulator: false,
  },
  { id: 'doom', name: 'Doom (PrBoom)', family: 'engine', no_emulator: false },
];

function show(
  items: OrganizeItem[],
  extra: {
    onRescan?: () => void;
    onDecide?: (ids: number[], c: unknown) => void;
  } = {},
) {
  return render(
    <I18nProvider locale="es">
      <LeftInPlace
        items={items}
        systems={SYSTEMS}
        otherFiles={0}
        deepFolders={0}
        onDecide={vi.fn()}
        {...extra}
      />
    </I18nProvider>,
  );
}

describe('LeftInPlace', () => {
  it('groups the system picker by family, placeholder first and outside any group', async () => {
    const onDecide = vi.fn();
    show([unknown(0)], { onDecide });
    const select = screen.getByRole('combobox', {
      name: 'Sistema para File 000.bin',
    });
    const groups = [...select.querySelectorAll('optgroup')];
    expect(groups.map((g) => g.label)).toEqual([
      'Consolas',
      'Portátiles',
      'Ordenadores',
      'Recreativas',
      'Motores y consolas de fantasía',
    ]);
    // Sorted by name inside a group; no icon, no count in the labels.
    expect(
      [...groups[0]!.querySelectorAll('option')].map((o) => o.textContent),
    ).toEqual(['Atari Jaguar', 'Sony PlayStation 2']);
    expect(
      [...groups[2]!.querySelectorAll('option')].map((o) => o.textContent),
    ).toEqual(['Amstrad CPC', 'Sinclair ZX Spectrum']);
    const first = select.querySelector('option')!;
    expect(first.textContent).toBe('Elige un sistema…');
    expect(first.parentElement).toBe(select);
    expect(first).toBeDisabled();
    expect(select).toHaveValue('');
    // jsdom has no native type-ahead (checked in the browser); a choice
    // from inside a group decides the file.
    await userEvent.selectOptions(select, 'zxspectrum');
    expect(onDecide).toHaveBeenCalledWith([0], { system: 'zxspectrum' });
  });

  it('a folder game names its system and the folder, in mono', () => {
    show([
      left(0, 'Freedoom Phase 1', {
        kind: 'folder-game',
        system: 'doom',
        folder: 'prboom',
      }),
    ]);
    const line = screen.getByText(
      (_, el) =>
        el?.tagName === 'P' &&
        (el.textContent ?? '').startsWith('Juego de Doom (PrBoom)'),
    );
    expect(line).toHaveTextContent(
      'Juego de Doom (PrBoom) en su propia carpeta: muévela entera a prboom',
    );
    expect(within(line).getByText('prboom').closest('.mono')).not.toBeNull();
    // A folder game is not an unknown file: no system picker.
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
  });

  it('tool_hint_and_rescan_button', async () => {
    const onRescan = vi.fn();
    show(
      [
        left(0, 'Juego.rar', {
          kind: 'unsupported-archive',
          format: 'rar',
          need: 'unrar',
        }),
        left(1, 'Otro.lz', { kind: 'unsupported-archive' }),
      ],
      { onRescan },
    );
    expect(
      screen.getByText('Para abrir archivos RAR instala unrar'),
    ).toBeInTheDocument();
    // Only the row that a tool would open offers to scan again.
    const rows = screen.getAllByRole('listitem');
    const rescan = within(rows[0]!).getByRole('button', {
      name: 'Volver a analizar',
    });
    expect(rescan).toHaveAttribute('aria-keyshortcuts', 'Meta+R Control+R');
    expect(within(rows[1]!).queryByRole('button')).not.toBeInTheDocument();
    expect(
      within(rows[1]!).getByText('Formato comprimido no compatible'),
    ).toBeInTheDocument();
    await userEvent.click(rescan);
    expect(onRescan).toHaveBeenCalledTimes(1);
  });

  it('the rescan button waits while a plan is made', () => {
    render(
      <I18nProvider locale="es">
        <LeftInPlace
          items={[
            left(0, 'Juego.rar', {
              kind: 'unsupported-archive',
              format: 'rar',
              need: 'seven-zip',
            }),
          ]}
          systems={[]}
          otherFiles={0}
          deepFolders={0}
          disabled
          onDecide={vi.fn()}
          onRescan={vi.fn()}
        />
      </I18nProvider>,
    );
    expect(
      screen.getByText('Para abrir archivos RAR instala 7-Zip'),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Volver a analizar' }),
    ).toBeDisabled();
  });

  it('every_new_left_reason_renders', () => {
    const cases: [LeftReason, string][] = [
      [{ kind: 'encrypted-archive' }, 'Protegido con contraseña'],
      [
        { kind: 'archive-needs-memory', mib: 1536 },
        'Necesita 1536 MB de memoria para descomprimirse',
      ],
      [
        { kind: 'archive-too-big', entries: 70000, bytes: null },
        'Demasiado grande para abrirlo con seguridad',
      ],
      [
        { kind: 'multi-volume-archive', parts: 3 },
        'Archivo en varias partes (3); únelas antes',
      ],
      [
        {
          kind: 'unsafe-archive-entry',
          name: '../../etc/passwd',
          why: 'dot-dot',
        },
        'Contiene una ruta no segura (../../etc/passwd); no se extrae',
      ],
      [
        { kind: 'set-unsafe-name', name: 'Track 01?.bin' },
        'Un archivo del juego tiene un nombre que la tarjeta no admite: Track 01?.bin',
      ],
      [{ kind: 'path-too-long' }, 'La ruta sería demasiado larga para Windows'],
      [
        { kind: 'nested-archive' },
        'Archivo dentro de otro archivo; extráelo antes',
      ],
      [
        { kind: 'repack-arcade' },
        'Los juegos de recreativa deben ir en .zip; vuelve a comprimirlo',
      ],
      [
        { kind: 'folder-full', folder: 'ROMS/PS' },
        'La carpeta ROMS/PS ya no admite más archivos en FAT32',
      ],
      [
        { kind: 'unsafe-target' },
        'La carpeta de destino es un enlace; no se escribe en ella',
      ],
      [
        { kind: 'unsupported-archive', format: 'rar', need: 'unrar' },
        'Para abrir archivos RAR instala unrar',
      ],
      [{ kind: 'damaged-archive' }, 'Archivo comprimido dañado'],
    ];
    show(cases.map(([reason], i) => left(i, `f${i}.7z`, reason)));
    const rows = screen.getAllByRole('listitem');
    expect(rows).toHaveLength(cases.length);
    cases.forEach(([, text], i) => {
      expect(within(rows[i]!).getByText(text)).toBeInTheDocument();
    });
  });

  it('gives every system select a unique id and the name "system"', () => {
    show([unknown(0), unknown(1), unknown(2)]);
    const selects = screen.getAllByRole('combobox');
    expect(selects).toHaveLength(3);
    for (const s of selects) {
      expect(s.id).not.toBe('');
      expect(s).toHaveAttribute('name', 'system');
    }
    expect(new Set(selects.map((s) => s.id)).size).toBe(3);
    // The accessible name is unchanged.
    expect(
      screen.getByRole('combobox', { name: 'Sistema para File 001.bin' }),
    ).toBeInTheDocument();
  });

  it('renders at most 200 rows and a show-more button', async () => {
    render(
      <I18nProvider locale="en">
        <LeftInPlace
          items={Array.from({ length: 450 }, (_, i) => unknown(i))}
          systems={[
            {
              id: 'gb',
              name: 'Game Boy',
              family: 'handheld',
              no_emulator: false,
            },
          ]}
          otherFiles={0}
          deepFolders={0}
          onDecide={vi.fn()}
        />
      </I18nProvider>,
    );
    // The list scrolls on its own and keyboard users can reach it.
    const list = screen.getByRole('region', {
      name: 'Files staying where they are',
    });
    expect(list).toHaveAttribute('tabindex', '0');
    expect(within(list).getAllByRole('listitem')).toHaveLength(200);
    expect(screen.getAllByRole('combobox')).toHaveLength(200);
    expect(screen.getAllByText('Unknown system')).toHaveLength(200);
    await userEvent.click(
      screen.getByRole('button', { name: 'Show 250 more' }),
    );
    expect(screen.getAllByRole('listitem')).toHaveLength(400);
  });

  it('key files and emulator data stay with their reason, names in mono', () => {
    show([
      left(0, 'prod.keys', { kind: 'key-material' }),
      left(1, 'title.keys', { kind: 'key-material' }),
      left(2, 'NPUB30024.rap', { kind: 'key-material' }),
      left(3, 'ux0', { kind: 'emulator-data', folder: 'ux0' }),
      left(4, 'model.3ds', { kind: 'not-a-rom', what: 'model3d' }),
    ]);
    const keys = screen.getAllByText(
      'Archivo de claves o licencia: Coverly no lo lee y lo deja aquí',
    );
    expect(keys).toHaveLength(3);
    expect(screen.getByText('prod.keys')).toHaveClass('font-mono');
    const data = screen.getByText(
      (_, el) =>
        el?.tagName === 'P' &&
        (el.textContent ?? '').startsWith('Está dentro de los datos'),
    );
    expect(data).toHaveTextContent(
      'Está dentro de los datos de un emulador (ux0); se deja tal cual',
    );
    expect(within(data).getByText('ux0').closest('.mono')).not.toBeNull();
    expect(screen.getByText('No es una ROM (modelo 3D)')).toBeInTheDocument();
    // None of them asks for a system.
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
  });

  it('a game folder left in place keeps its hidden label and size', () => {
    show([
      {
        ...left(0, 'PCSB00245', { kind: 'emulator-data', folder: 'ux0' }),
        kind: 'folder',
        size: 1_700_000_000,
      } as OrganizeItem,
    ]);
    expect(screen.getByText('Carpeta de juego')).toHaveClass('sr-only');
    expect(screen.getByText('1,7 GB')).toHaveClass('font-mono');
  });

  it('a mod folder left in place is read as a mod folder, not a game', () => {
    show([
      {
        ...left(0, 'textures/SLUS-00594', {
          kind: 'addon-no-place',
          addon: 'mod',
        }),
        kind: 'folder',
        size: 384_000_000,
      } as OrganizeItem,
      {
        ...left(1, 'load/mods/0004000000055D00', {
          kind: 'addon-for-emulator',
          emulator: 'Azahar',
          path: 'load/mods/0004000000055D00',
        }),
        kind: 'folder',
        size: 46_000_000,
      } as OrganizeItem,
    ]);
    expect(screen.getAllByText('Carpeta de mod')).toHaveLength(2);
    for (const label of screen.getAllByText('Carpeta de mod'))
      expect(label).toHaveClass('sr-only');
    expect(screen.queryByText('Carpeta de juego')).toBeNull();
  });
});

describe('LeftInPlace (adjustments)', () => {
  it('a game part, a Vita backup and a declined item say why they stay', () => {
    show([
      left(0, 'main.npdm', {
        kind: 'game-part',
        part: 'exe-fs',
        system: 'switch',
        title_id: '0100000000010000',
      }),
      left(1, 'PCSE00120/game.psvimg', {
        kind: 'game-part',
        part: 'backup',
        system: 'psvita',
        title_id: 'PCSE00120',
      }),
      left(2, 'Bloodborne (CUSA00900)', { kind: 'not-confirmed' }),
    ]);
    expect(
      screen.getByText('Parte de un juego; no se mueve suelta'),
    ).toBeInTheDocument();
    expect(
      screen.getByText('Copia de seguridad cifrada de Vita'),
    ).toBeInTheDocument();
    expect(
      screen.getByText('No confirmado; se queda donde está'),
    ).toBeInTheDocument();
  });
});
