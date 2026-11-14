import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { toast } from '@/components/ui/toaster';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { MOCK_EXTRAS } from '@/ipc/mock/extrasFixture';
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

async function shown(name: string) {
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
  return r;
}

function row(r: HTMLElement, name: string): HTMLElement {
  return within(r).getByRole('button', {
    name: new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')},`),
  });
}

function openMenu(r: HTMLElement, label: string) {
  fireEvent.pointerDown(
    within(r).getByRole('button', { name: `Más acciones para ${label}` }),
    { button: 0, ctrlKey: false },
  );
}

/** Lets Radix's deferred focus return (a 0 ms timer) run. */
const settle = () => act(() => new Promise((ok) => setTimeout(ok, 30)));

describe('ExtrasSection focus (extras spec §8.3, §8.4)', () => {
  beforeEach(async () => {
    b = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: null,
      pauseAfter: null,
    });
    setBackend(b);
    lib = await b.openLibrary(MOCK_EXTRAS);
    useExtrasStore.getState().reset();
    usePlayerStore.getState().stop();
    toastMock.mockReset();
    vi.mocked(toast.dismiss).mockReset();
  });
  afterEach(() => {
    useExtrasStore.getState().reset();
    usePlayerStore.getState().stop();
    setBackend(null);
  });

  it('«Renombrar» from the row menu keeps the input open and focused', async () => {
    const r = await shown('Tidewalker');
    openMenu(r, 'Key Art.png');
    fireEvent.click(
      await screen.findByRole('menuitem', { name: /^Renombrar/ }),
    );
    await settle();
    const input = within(r).getByRole('textbox', {
      name: 'Nuevo nombre de Key Art.png',
    });
    expect(input).toHaveFocus();
  });

  it('«Etiquetas…» from the row menu keeps the popover open and focused', async () => {
    const r = await shown('Tidewalker');
    openMenu(r, 'Key Art.png');
    fireEvent.click(
      await screen.findByRole('menuitem', { name: /^Etiquetas/ }),
    );
    await settle();
    expect(
      screen.getByRole('textbox', { name: 'Añadir etiqueta' }),
    ).toHaveFocus();
  });

  it('«Renombrar» from a category menu keeps the input open and focused', async () => {
    const r = await shown('Tidewalker');
    openMenu(r, 'Remixes');
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Renombrar' }));
    await settle();
    expect(
      within(r).getByRole('textbox', { name: 'Nombre de la categoría' }),
    ).toHaveFocus();
  });

  it('Esc on a rename, or Enter with the same name, returns focus to the row', async () => {
    const r = await shown('Tidewalker');
    fireEvent.keyDown(row(r, 'Key Art.png'), { key: 'F2' });
    fireEvent.keyDown(within(r).getByRole('textbox'), { key: 'Escape' });
    await waitFor(() => expect(row(r, 'Key Art.png')).toHaveFocus());
    fireEvent.keyDown(row(r, 'Key Art.png'), { key: 'F2' });
    fireEvent.keyDown(within(r).getByRole('textbox'), { key: 'Enter' });
    await waitFor(() => expect(row(r, 'Key Art.png')).toHaveFocus());
  });

  it('keeps the rename input enabled and focused while a name is refused', async () => {
    const r = await shown('Tidewalker');
    fireEvent.keyDown(row(r, '03 Leviathan.flac'), { key: 'F2' });
    const input = within(r).getByRole('textbox');
    fireEvent.change(input, { target: { value: '01 Opening Tide' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    // Disabling a focused input drops its focus in a browser.
    expect(input).not.toBeDisabled();
    await within(r).findByRole('alert');
    expect(input).toHaveFocus();
  });

  it('Esc on the tags popover returns focus to the row', async () => {
    const r = await shown('Tidewalker');
    const name = row(r, 'Key Art.png');
    name.focus();
    fireEvent.keyDown(name, { key: 't', code: 'KeyT' });
    const input = await screen.findByRole('textbox', {
      name: 'Añadir etiqueta',
    });
    fireEvent.keyDown(input, { key: 'Escape' });
    await settle();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(row(r, 'Key Art.png')).toHaveFocus();
  });

  it('Cmd/Ctrl+Z also takes away the toast it undid, so its «Deshacer» cannot fail', async () => {
    toastMock.mockReturnValue('toast-1');
    const r = await shown('Tidewalker');
    const name = row(r, 'Key Art.png');
    name.focus();
    fireEvent.keyDown(name, { key: 'Delete' });
    await waitFor(() =>
      expect(
        within(r).queryByRole('button', { name: /^Key Art\.png,/ }),
      ).toBeNull(),
    );
    fireEvent.keyDown(document.activeElement!, {
      key: 'z',
      code: 'KeyZ',
      metaKey: true,
    });
    await waitFor(() => expect(row(r, 'Key Art.png')).toBeInTheDocument());
    expect(toast.dismiss).toHaveBeenCalledWith('toast-1');
  });
});
