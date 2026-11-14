import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { BackendError } from '@/ipc/errors';
import type { AddPlanView, CategoryRef } from '@/ipc/types';
import { moveTargets } from '@/lib/extras';

import { ExtrasAddDialog, type AddRequest } from './ExtrasAddDialog';

const SOUNDTRACK: CategoryRef = { kind: 'fixed', name: 'soundtrack' };
const ARTWORK: CategoryRef = { kind: 'fixed', name: 'artwork' };
const targets = moveTargets([{ category: { kind: 'user', name: 'Remixes' } }]);

function plan(over: Partial<AddPlanView> = {}): AddPlanView {
  return {
    plan: 7,
    entry_id: 3,
    files: [
      {
        index: 0,
        name: 'Harbor Lights (Bonus).flac',
        size: 31_000_000,
        kind: 'audio',
        category: 'soundtrack',
        rip: null,
      },
      {
        index: 1,
        name: 'Tidewalker - Overworld.spc',
        size: 66_048,
        kind: 'audio',
        category: 'soundtrack',
        rip: {
          game: 'Tidewalker',
          title: 'Overworld',
          artist: 'Coverly Mock Ensemble',
        },
      },
      {
        index: 2,
        name: 'Poster.png',
        size: 2_000_000,
        kind: 'image',
        category: 'artwork',
        rip: null,
      },
    ],
    total: 33_066_048,
    keys: 0,
    hidden: 0,
    too_many: false,
    ...over,
  };
}

function show(
  request: Omit<AddRequest, 'plan'> & { plan?: AddPlanView },
  onCommit = vi.fn().mockResolvedValue(null),
  onClose = vi.fn(),
) {
  render(
    <I18nProvider locale="es">
      <ExtrasAddDialog
        request={{ plan: plan(), ...request }}
        targets={targets}
        onCommit={onCommit}
        onClose={onClose}
      />
    </I18nProvider>,
  );
  return { onCommit, onClose, dialog: screen.getByRole('dialog') };
}

const select = (d: HTMLElement, name: string) =>
  within(d).getByRole('combobox', { name: `Categoría de ${name}` });

describe('ExtrasAddDialog (D13, §8.3)', () => {
  it('lists each file with its suggested category, size and the total, then copies', async () => {
    const { onCommit, dialog } = show({});
    expect(
      within(dialog).getByRole('heading', { name: 'Añadir archivos' }),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        'Se copian; los originales se quedan donde están.',
      ),
    ).toBeInTheDocument();
    expect(select(dialog, 'Harbor Lights (Bonus).flac')).toHaveValue(
      'fixed:soundtrack',
    );
    expect(select(dialog, 'Poster.png')).toHaveValue('fixed:artwork');
    // Sizes and the total are data: mono.
    const total = within(dialog).getByText(/^En total:/);
    expect(total.querySelector('.mono')).toHaveTextContent('31,5 MB');
    fireEvent.change(select(dialog, 'Poster.png'), {
      target: { value: 'user:Remixes' },
    });
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Copiar 3 archivos' }),
    );
    await waitFor(() => expect(onCommit).toHaveBeenCalledTimes(1));
    expect(onCommit.mock.calls[0]![0]).toEqual([
      { index: 0, category: SOUNDTRACK },
      { index: 1, category: SOUNDTRACK },
      { index: 2, category: { kind: 'user', name: 'Remixes' } },
    ]);
    expect(onCommit.mock.calls[0]![1]).toEqual({});
  });

  it('a category «+» puts every file in that category', () => {
    const { dialog } = show({ category: ARTWORK });
    expect(select(dialog, 'Harbor Lights (Bonus).flac')).toHaveValue(
      'fixed:artwork',
    );
    expect(select(dialog, 'Poster.png')).toHaveValue('fixed:artwork');
  });

  it('a drop on a category keeps the files that fit there and suggests the rest', () => {
    const { dialog } = show({ dropped: ARTWORK });
    expect(select(dialog, 'Poster.png')).toHaveValue('fixed:artwork');
    expect(select(dialog, 'Harbor Lights (Bonus).flac')).toHaveValue(
      'fixed:soundtrack',
    );
  });

  it('a drop on no category asks which one', () => {
    const { dialog } = show({ ask: true });
    expect(
      within(dialog).getByRole('heading', { name: '¿A qué categoría?' }),
    ).toBeInTheDocument();
  });

  it('offers a rip’s tags as buttons, never applied until pressed', async () => {
    const { onCommit, dialog } = show({});
    expect(
      within(dialog).getByText('Etiqueta SPC: «Tidewalker»'),
    ).toBeInTheDocument();
    const title = within(dialog).getByRole('button', {
      name: 'Añadir la etiqueta «Overworld» al copiar',
    });
    expect(title).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(title);
    expect(title).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Copiar 3 archivos' }),
    );
    await waitFor(() => expect(onCommit).toHaveBeenCalledTimes(1));
    expect(onCommit.mock.calls[0]![1]).toEqual({ 1: ['Overworld'] });
  });

  it('a file left out is not copied', async () => {
    const { onCommit, dialog } = show({});
    fireEvent.click(
      within(dialog).getByRole('checkbox', { name: 'Copiar Poster.png' }),
    );
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Copiar 2 archivos' }),
    );
    await waitFor(() => expect(onCommit).toHaveBeenCalledTimes(1));
    expect(onCommit.mock.calls[0]![0]).toHaveLength(2);
    fireEvent.click(
      within(dialog).getByRole('checkbox', {
        name: 'Copiar Harbor Lights (Bonus).flac',
      }),
    );
    expect(
      within(dialog).getByRole('button', { name: 'Copiar 1 archivo' }),
    ).toBeInTheDocument();
  });

  it('says how many key and hidden files are left out', () => {
    const { dialog } = show({ plan: plan({ keys: 2, hidden: 1 }) });
    expect(dialog).toHaveTextContent('2 archivos de claves no se añaden.');
    expect(dialog).toHaveTextContent('1 archivo oculto no se añade.');
  });

  it('too many files copies nothing and says why', () => {
    const { dialog } = show({ plan: plan({ too_many: true }) });
    expect(dialog).toHaveTextContent(
      'Son más de 500 archivos o de 20 GB; añade menos de una vez.',
    );
    expect(
      within(dialog).getByRole('button', { name: 'Copiar 3 archivos' }),
    ).toBeDisabled();
  });

  it('not enough space says how much is short and keeps the dialog', async () => {
    const onCommit = vi
      .fn()
      .mockResolvedValue(new BackendError('no-space', '3000000/1000000'));
    const { dialog, onClose } = show({}, onCommit);
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Copiar 3 archivos' }),
    );
    expect(await within(dialog).findByRole('alert')).toHaveTextContent(
      'No hay espacio suficiente: faltan 1,9 MB.',
    );
    expect(onClose).not.toHaveBeenCalled();
  });

  it('a refused add (too many) says why', async () => {
    const onCommit = vi
      .fn()
      .mockResolvedValue(new BackendError('extras-refused', 'too-many'));
    const { dialog } = show({}, onCommit);
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Copiar 3 archivos' }),
    );
    expect(await within(dialog).findByRole('status')).toHaveTextContent(
      'Son más de 500 archivos',
    );
    expect(
      within(dialog).getByRole('button', { name: 'Copiar 3 archivos' }),
    ).toBeDisabled();
  });

  it('Cancelar closes without copying', () => {
    const { onCommit, onClose, dialog } = show({});
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancelar' }));
    expect(onClose).toHaveBeenCalled();
    expect(onCommit).not.toHaveBeenCalled();
  });
});
