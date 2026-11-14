import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { toast } from '@/components/ui/toaster';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { MOCK_EXTRAS } from '@/ipc/mock/extrasFixture';
import type { MockExtrasPick } from '@/ipc/mock/mockExtras';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import type { EntryView, LibraryView } from '@/ipc/types';
import { useExtrasStore } from '@/stores/extrasStore';
import { usePlayerStore } from '@/stores/playerStore';

import { ExtrasSection } from './ExtrasSection';

vi.mock('@/components/ui/toaster', async (importOriginal) => {
  const real = await importOriginal<typeof import('@/components/ui/toaster')>();
  return { ...real, toast: Object.assign(vi.fn(), { dismiss: vi.fn() }) };
});

const toastMock = vi.mocked(toast);

let b: MockBackend;
let lib: LibraryView;

const entry = (name: string): EntryView => {
  const e = lib.entries.find((x) => x.name === name);
  if (!e) throw new Error(name);
  return e;
};

async function setup(pick: MockExtrasPick = 'two') {
  b = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: null,
    pauseAfter: null,
    extrasPick: pick,
  });
  setBackend(b);
  lib = await b.openLibrary(MOCK_EXTRAS);
  useExtrasStore.getState().reset();
  usePlayerStore.getState().stop();
  toastMock.mockClear();
}

async function show(name: string) {
  render(
    <I18nProvider locale="es">
      <ExtrasSection generation={lib.generation} entry={entry(name)} />
    </I18nProvider>,
  );
  const r = await screen.findByRole('region', { name: /^Extras/ });
  await waitFor(() =>
    expect(r).toHaveAttribute('data-entry', String(entry(name).id)),
  );
  await waitFor(() => expect(r).not.toHaveAttribute('aria-busy', 'true'));
  // The drop layer subscribes once the backend answers.
  await act(async () => {
    await new Promise((res) => setTimeout(res, 0));
  });
  return r;
}

/** Lays the section out: itself at (0,0)–(400,1000), each category (band
 * or dashed slot) a 40 px strip, top to bottom. Answers the strips' tops. */
function layout(r: HTMLElement): Record<string, number> {
  r.getBoundingClientRect = () => new DOMRect(0, 0, 400, 1000);
  const tops: Record<string, number> = {};
  r.querySelectorAll<HTMLElement>('[data-extras-band]').forEach((el, i) => {
    const top = 20 + i * 50;
    tops[el.dataset.extrasBand!] = top;
    el.getBoundingClientRect = () => new DOMRect(0, top, 400, 40);
  });
  return tops;
}

const files = () => useExtrasStore.getState().listing!.files;
const lastToast = () => toastMock.mock.calls.at(-1)!;

describe('ExtrasSection adds (D11 a, D13, §8.3)', () => {
  afterEach(() => {
    useExtrasStore.getState().reset();
    setBackend(null);
  });

  it('«Añadir archivos» opens the picker, then the dialog, then copies with undo', async () => {
    await setup();
    const r = await show('Tidewalker');
    const before = files().length;
    fireEvent.click(within(r).getByRole('button', { name: 'Añadir archivos' }));
    const dialog = await screen.findByRole('dialog', {
      name: 'Añadir archivos',
    });
    expect(dialog).toHaveTextContent('Harbor Lights (Bonus).flac');
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Copiar 2 archivos' }),
    );
    await waitFor(() => expect(files().length).toBe(before + 2));
    await waitFor(() =>
      expect(lastToast()[0]).toBe('Copiados 2 archivos a Banda sonora'),
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    const opts = lastToast()[1] as unknown as { action: { onClick(): void } };
    act(() => opts.action.onClick());
    await waitFor(() => expect(files().length).toBe(before));
  });

  it('a category’s «+» starts every file in that category', async () => {
    await setup();
    const r = await show('Moss Garden');
    fireEvent.click(within(r).getByRole('button', { name: 'Añadir a Arte' }));
    const dialog = await screen.findByRole('dialog');
    for (const s of within(dialog).getAllByRole('combobox'))
      expect(s).toHaveValue('fixed:artwork');
  });

  it('a rip tag pressed in the dialog lands on the copy', async () => {
    await setup();
    const r = await show('Tidewalker');
    fireEvent.click(within(r).getByRole('button', { name: 'Añadir archivos' }));
    const dialog = await screen.findByRole('dialog');
    fireEvent.click(
      within(dialog).getByRole('button', {
        name: 'Añadir la etiqueta «Tidewalker - Overworld» al copiar',
      }),
    );
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Copiar 2 archivos' }),
    );
    await waitFor(() =>
      expect(
        files().find((f) => f.name === 'Tidewalker - Overworld.spc')?.tags,
      ).toEqual(['Tidewalker - Overworld']),
    );
    // Never added unless pressed.
    expect(
      files().find((f) => f.name === 'Harbor Lights (Bonus).flac')?.tags,
    ).toEqual([]);
  });

  it('a drop on a category whose types all fit copies without the dialog', async () => {
    await setup();
    const r = await show('Moss Garden');
    const tops = layout(r);
    act(() => b.emitDragOver({ x: 10, y: tops['fixed:soundtrack']! + 5 }));
    expect(
      r.querySelector('[data-extras-band="fixed:soundtrack"]'),
    ).toHaveAttribute('data-drop');
    expect(r.querySelector('[aria-live="polite"]')).toHaveTextContent(
      'Suelta en una categoría para copiar',
    );
    act(() => {
      b.emitDrop({
        names: ['Boss.flac', 'Ending.mp3'],
        position: { x: 10, y: tops['fixed:soundtrack']! + 5 },
      });
    });
    await waitFor(() =>
      expect(files().map((f) => f.name)).toEqual(
        expect.arrayContaining(['Boss.flac', 'Ending.mp3']),
      ),
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await waitFor(() =>
      expect(lastToast()[0]).toBe('Copiados 2 archivos a Banda sonora'),
    );
    expect(
      r.querySelector('[data-extras-band="fixed:soundtrack"]'),
    ).not.toHaveAttribute('data-drop');
  });

  it('a drop counts only the files it copied, not the hidden ones it skipped', async () => {
    await setup();
    const r = await show('Moss Garden');
    const tops = layout(r);
    act(() => {
      b.emitDrop({
        names: ['Boss.flac', '._Boss.flac'],
        position: { x: 10, y: tops['fixed:soundtrack']! + 5 },
      });
    });
    await waitFor(() =>
      expect(lastToast()[0]).toBe('Copiado 1 archivo a Banda sonora'),
    );
    expect(
      (lastToast()[1] as unknown as { description?: string }).description,
    ).toBe('1 archivo oculto no se añade.');
  });

  it('a background add that ends after the game changed still gets its toast and undo', async () => {
    await setup('many');
    const tide = entry('Tidewalker');
    const before = (await b.extrasList(lib.generation, tide.id)).files.length;
    const view = render(
      <I18nProvider locale="es">
        <ExtrasSection key={tide.id} generation={lib.generation} entry={tide} />
      </I18nProvider>,
    );
    const r = await screen.findByRole('region', { name: /^Extras/ });
    await waitFor(() => expect(r).not.toHaveAttribute('aria-busy', 'true'));
    fireEvent.click(within(r).getByRole('button', { name: 'Añadir archivos' }));
    const dialog = await screen.findByRole('dialog');
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Copiar 60 archivos' }),
    );
    await within(r).findByText(/^Copiando/);
    const moss = entry('Moss Garden');
    view.rerender(
      <I18nProvider locale="es">
        <ExtrasSection key={moss.id} generation={lib.generation} entry={moss} />
      </I18nProvider>,
    );
    await waitFor(() =>
      expect(lastToast()?.[0]).toBe('Copiados 60 archivos a Arte'),
    );
    expect((await b.extrasList(lib.generation, tide.id)).files.length).toBe(
      before + 60,
    );
    const opts = lastToast()[1] as unknown as { action: { onClick(): void } };
    act(() => opts.action.onClick());
    await waitFor(async () =>
      expect((await b.extrasList(lib.generation, tide.id)).files.length).toBe(
        before,
      ),
    );
  });

  it('a drop with a file that does not fit opens the dialog, keeping the fitting ones there', async () => {
    await setup();
    const r = await show('Moss Garden');
    const tops = layout(r);
    act(() => {
      b.emitDrop({
        names: ['Boss.flac', 'Poster.png', 'prod.keys'],
        position: { x: 10, y: tops['fixed:soundtrack']! + 5 },
      });
    });
    const dialog = await screen.findByRole('dialog', {
      name: 'Añadir archivos',
    });
    expect(
      within(dialog).getByRole('combobox', { name: 'Categoría de Boss.flac' }),
    ).toHaveValue('fixed:soundtrack');
    expect(
      within(dialog).getByRole('combobox', { name: 'Categoría de Poster.png' }),
    ).toHaveValue('fixed:artwork');
    expect(dialog).toHaveTextContent('1 archivo de claves no se añade.');
    expect(dialog).not.toHaveTextContent('prod.keys');
  });

  it('a drop on no category asks which one', async () => {
    await setup();
    const r = await show('Tidewalker');
    layout(r);
    act(() => {
      b.emitDrop({ names: ['Map.png'], position: { x: 10, y: 995 } });
    });
    expect(
      await screen.findByRole('dialog', { name: '¿A qué categoría?' }),
    ).toBeInTheDocument();
  });

  it('a drop over the section never opens it as a library', async () => {
    await setup();
    const r = await show('Tidewalker');
    layout(r);
    const opened = vi.fn();
    const off = await b.onFolderDrop(opened);
    const ev = Object.assign(new Event('drop', { cancelable: true }), {
      clientX: 10,
      clientY: 60,
      dataTransfer: { files: [{ name: 'Music' }], getData: () => '' },
    });
    window.dispatchEvent(ev);
    expect(opened).not.toHaveBeenCalled();
    off();
  });

  it('a long add shows its progress in mono with «Cancelar»; cancelling keeps what was copied, with undo', async () => {
    await setup('many');
    const r = await show('Tidewalker');
    const before = files().length;
    fireEvent.click(within(r).getByRole('button', { name: 'Añadir archivos' }));
    const dialog = await screen.findByRole('dialog');
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Copiar 60 archivos' }),
    );
    const line = await within(r).findByText(/^Copiando/);
    expect(line.querySelectorAll('.mono').length).toBe(4);
    expect(line).toHaveTextContent(/de 60 · .* de 228,9 MB$/);
    expect(
      within(r).getByRole('button', { name: 'Añadir archivos' }),
    ).toBeDisabled();
    fireEvent.click(within(r).getByRole('button', { name: 'Cancelar' }));
    await waitFor(() =>
      expect(String(lastToast()?.[0])).toMatch(/^Cancelado: /),
    );
    expect(within(r).queryByText(/^Copiando/)).not.toBeInTheDocument();
    const copied = files().length - before;
    expect(copied).toBeGreaterThan(0);
    expect(copied).toBeLessThan(60);
    // The exact count, in the singular for one file.
    expect(lastToast()[0]).toBe(
      copied === 1
        ? 'Cancelado: se copió 1 archivo; puedes deshacerlo.'
        : `Cancelado: se copiaron ${copied} archivos; puedes deshacerlo.`,
    );
    const opts = lastToast()[1] as unknown as { action: { onClick(): void } };
    act(() => opts.action.onClick());
    await waitFor(() => expect(files().length).toBe(before));
  });

  it('not enough space copies nothing and says how much is short', async () => {
    await setup('no-space');
    const r = await show('Tidewalker');
    const before = files().length;
    fireEvent.click(within(r).getByRole('button', { name: 'Añadir archivos' }));
    const dialog = await screen.findByRole('dialog');
    fireEvent.click(
      within(dialog).getByRole('button', { name: 'Copiar 2 archivos' }),
    );
    expect(await within(dialog).findByRole('alert')).toHaveTextContent(
      /^No hay espacio suficiente: faltan .+\.$/,
    );
    expect(files().length).toBe(before);
  });

  it('too many files says why and copies nothing', async () => {
    await setup('too-many');
    const r = await show('Tidewalker');
    fireEvent.click(within(r).getByRole('button', { name: 'Añadir archivos' }));
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveTextContent(
      'Son más de 500 archivos o de 20 GB; añade menos de una vez.',
    );
    expect(
      within(dialog).getByRole('button', { name: 'Copiar 501 archivos' }),
    ).toBeDisabled();
  });

  it('a cancelled picker opens nothing', async () => {
    await setup('cancel');
    const r = await show('Tidewalker');
    fireEvent.click(within(r).getByRole('button', { name: 'Añadir archivos' }));
    await act(async () => {
      await new Promise((res) => setTimeout(res, 0));
    });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
