import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { LeftInPlace } from '@/components/organize/LeftInPlace';
import { I18nProvider } from '@/i18n/I18nProvider';
import type { LeftReason, OrganizeItem } from '@/ipc/types';
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

function show(items: OrganizeItem[], extra: { onRescan?: () => void } = {}) {
  return render(
    <I18nProvider locale="es">
      <LeftInPlace
        items={items}
        systems={[]}
        otherFiles={0}
        deepFolders={0}
        onDecide={vi.fn()}
        {...extra}
      />
    </I18nProvider>,
  );
}

describe('LeftInPlace', () => {
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

  it('renders at most 200 rows and a show-more button', async () => {
    render(
      <I18nProvider locale="en">
        <LeftInPlace
          items={Array.from({ length: 450 }, (_, i) => unknown(i))}
          systems={[{ id: 'gb', name: 'Game Boy', family: 'handheld' }]}
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
});
