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
import type { EntryView, ExtraFileView, LibraryView } from '@/ipc/types';
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

function show(name: string) {
  return render(
    <I18nProvider locale="es">
      <ExtrasSection generation={lib.generation} entry={entry(name)} />
    </I18nProvider>,
  );
}

async function listed(name: string) {
  const r = await screen.findByRole('region', { name: /^Extras/ });
  await waitFor(() =>
    expect(r).toHaveAttribute('data-entry', String(entry(name).id)),
  );
  await waitFor(() => expect(r).not.toHaveAttribute('aria-busy', 'true'));
  return r;
}

const files = (): ExtraFileView[] => useExtrasStore.getState().listing!.files;
const file = (name: string) => {
  const f = files().find((x) => x.name === name);
  if (!f) throw new Error(name);
  return f;
};
const pathOf = (name: string) => files().find((x) => x.name === name)?.path;

/** The name button (primary control) of a row. */
function row(r: HTMLElement, name: string): HTMLElement {
  return within(r).getByRole('button', {
    name: new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')},`),
  });
}

function openMenu(r: HTMLElement, name: string) {
  const trigger = within(r).getByRole('button', {
    name: `Más acciones para ${name}`,
  });
  fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false });
}

/** The last toast: its text and options. */
function lastToast() {
  const call = toastMock.mock.calls.at(-1)!;
  return {
    text: call[0],
    opts: call[1] as {
      duration?: number;
      description?: string;
      action?: { label: string; onClick: () => void };
    },
  };
}

function live(r: HTMLElement) {
  return r.querySelector('[aria-live="polite"]')!;
}

describe('ExtrasSection CRUD (extras spec §8.3, D11 b, D23)', () => {
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
    toastMock.mockClear();
  });
  afterEach(() => {
    useExtrasStore.getState().reset();
    usePlayerStore.getState().stop();
    setBackend(null);
  });

  describe('rename (D14)', () => {
    it('F2 edits the stem with the extension as a fixed mono suffix; Enter saves', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      const name = row(r, '03 Leviathan.flac');
      name.focus();
      fireEvent.keyDown(name, { key: 'F2' });
      const input = within(r).getByRole('textbox', {
        name: 'Nuevo nombre de 03 Leviathan.flac',
      });
      expect(input).toHaveValue('03 Leviathan');
      expect(input).toHaveFocus();
      const ext = input.parentElement!.querySelector('.extras-rename__ext')!;
      expect(ext).toHaveTextContent('.flac');
      expect(ext).toHaveClass('mono');
      fireEvent.change(input, { target: { value: '03 Final Boss' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      await waitFor(() =>
        expect(pathOf('03 Final Boss.flac')).toBe(
          'Soundtrack/03 Final Boss.flac',
        ),
      );
      expect(lastToast().text).toBe('Renombrado: 03 Final Boss.flac');
      // Back on the row it renamed.
      await waitFor(() =>
        expect(document.activeElement).toBe(row(r, '03 Final Boss.flac')),
      );
    });

    it('Esc cancels the edit and never reaches the panel', async () => {
      const outer = vi.fn();
      window.addEventListener('keydown', outer);
      show('Tidewalker');
      const r = await listed('Tidewalker');
      fireEvent.keyDown(row(r, 'Key Art.png'), { key: 'F2' });
      const input = within(r).getByRole('textbox', {
        name: 'Nuevo nombre de Key Art.png',
      });
      fireEvent.change(input, { target: { value: 'Otro' } });
      fireEvent.keyDown(input, { key: 'Escape' });
      expect(within(r).queryByRole('textbox')).toBeNull();
      expect(outer).not.toHaveBeenCalled();
      expect(pathOf('Key Art.png')).toBe('Artwork/Key Art.png');
      window.removeEventListener('keydown', outer);
    });

    it('says the name is taken in this category and keeps the input', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      fireEvent.keyDown(row(r, '03 Leviathan.flac'), { key: 'F2' });
      const input = within(r).getByRole('textbox');
      fireEvent.change(input, { target: { value: '01 Opening Tide' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      expect(
        await within(r).findByRole('alert', {
          name: '',
        }),
      ).toHaveTextContent(
        'Ya hay un archivo con ese nombre en esta categoría.',
      );
      expect(within(r).getByRole('textbox')).toHaveAttribute(
        'aria-invalid',
        'true',
      );
      expect(toastMock).not.toHaveBeenCalled();
    });

    it('opens from the row menu', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      openMenu(r, 'Key Art.png');
      fireEvent.click(
        await screen.findByRole('menuitem', { name: /^Renombrar/ }),
      );
      expect(
        await within(r).findByRole('textbox', {
          name: 'Nuevo nombre de Key Art.png',
        }),
      ).toBeInTheDocument();
    });
  });

  describe('row menu from the keyboard and the pointer', () => {
    it.each([
      ['Shift+F10', { key: 'F10', shiftKey: true }],
      ['the menu key', { key: 'ContextMenu' }],
    ])(
      '%s on a row opens its menu, as in the details view; Esc goes back to the row',
      async (_, key) => {
        show('Tidewalker');
        const r = await listed('Tidewalker');
        const name = row(r, 'Key Art.png');
        name.focus();
        fireEvent.keyDown(name, key);
        const menu = await screen.findByRole('menu');
        expect(
          within(menu).getByRole('menuitem', { name: /^Renombrar/ }),
        ).toBeInTheDocument();
        fireEvent.keyDown(menu, { key: 'Escape' });
        await waitFor(() => expect(screen.queryByRole('menu')).toBeNull());
        await waitFor(() => expect(name).toHaveFocus());
      },
    );

    it('a right click on a row opens its menu', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      fireEvent.contextMenu(row(r, 'Key Art.png'));
      expect(await screen.findByRole('menu')).toBeInTheDocument();
    });
  });

  describe('move (D11 b)', () => {
    it('«Mover a…» lists the other categories and moves the file', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      openMenu(r, 'Key Art.png');
      const sub = await screen.findByRole('menuitem', { name: 'Mover a…' });
      fireEvent.click(sub);
      fireEvent.keyDown(sub, { key: 'ArrowRight' });
      const menus = await screen.findAllByRole('menu');
      const submenu = menus.at(-1)!;
      const labels = within(submenu)
        .getAllByRole('menuitem')
        .map((m) => m.textContent);
      expect(labels).toEqual([
        'Banda sonora',
        'Manuales',
        'Vídeos',
        'Temas y avatares',
        'Guías',
        'Remixes',
        'Sin categoría',
      ]);
      fireEvent.click(within(submenu).getByRole('menuitem', { name: 'Guías' }));
      await waitFor(() =>
        expect(pathOf('Key Art.png')).toBe('Guides/Key Art.png'),
      );
      expect(lastToast().text).toBe('Movido a Guías');
    });

    it('Alt+↓ moves to the next category and says so in the live region', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      const name = row(r, 'Key Art.png');
      name.focus();
      fireEvent.keyDown(name, { key: 'ArrowDown', altKey: true });
      await waitFor(() =>
        expect(pathOf('Key Art.png')).toBe('Videos/Key Art.png'),
      );
      expect(live(r)).toHaveTextContent('Movido a Vídeos');
      // It lands outlined, with the focus on it.
      await waitFor(() =>
        expect(document.activeElement).toBe(row(r, 'Key Art.png')),
      );
      expect(row(r, 'Key Art.png').closest('li')).toHaveAttribute(
        'data-landed',
        'true',
      );
      fireEvent.keyDown(row(r, 'Key Art.png'), {
        key: 'ArrowUp',
        altKey: true,
      });
      await waitFor(() =>
        expect(pathOf('Key Art.png')).toBe('Artwork/Key Art.png'),
      );
      expect(live(r)).toHaveTextContent('Movido a Arte');
    });

    it('drags a row onto another band with the pointer; Esc cancels', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      const guides = within(r).getByRole('group', { name: 'Guías' });
      const rect = (top: number) =>
        ({
          x: 0,
          y: top,
          top,
          left: 0,
          right: 300,
          bottom: top + 80,
          width: 300,
          height: 80,
          toJSON: () => ({}),
        }) as DOMRect;
      for (const band of r.querySelectorAll<HTMLElement>('[data-extras-band]'))
        band.getBoundingClientRect = () => rect(-1000);
      guides.getBoundingClientRect = () => rect(400);
      const li = row(r, 'Key Art.png').closest('li')!;

      // Esc cancels: nothing moves, the ghost goes.
      fireEvent.pointerDown(li, { button: 0, clientX: 10, clientY: 10 });
      fireEvent.pointerMove(document, { clientX: 40, clientY: 420 });
      expect(guides).toHaveAttribute('data-drop', 'true');
      expect(document.body.querySelector('.extras-ghost')).toHaveTextContent(
        'Key Art.png',
      );
      fireEvent.keyDown(document, { key: 'Escape' });
      expect(document.body.querySelector('.extras-ghost')).toBeNull();
      expect(guides).not.toHaveAttribute('data-drop');
      fireEvent.pointerUp(document, { clientX: 40, clientY: 420 });
      expect(pathOf('Key Art.png')).toBe('Artwork/Key Art.png');

      // A real drop moves it.
      fireEvent.pointerDown(li, { button: 0, clientX: 10, clientY: 10 });
      fireEvent.pointerMove(document, { clientX: 40, clientY: 420 });
      fireEvent.pointerUp(document, { clientX: 40, clientY: 420 });
      await waitFor(() =>
        expect(pathOf('Key Art.png')).toBe('Guides/Key Art.png'),
      );
      expect(useExtrasStore.getState().selection.size).toBe(0);
    });
  });

  describe('trash (D8)', () => {
    it('sends one file straight to the Trash with a 10 s undo toast', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      openMenu(r, 'Key Art.png');
      fireEvent.click(
        await screen.findByRole('menuitem', { name: 'Enviar a la Papelera' }),
      );
      await waitFor(() => expect(pathOf('Key Art.png')).toBeUndefined());
      const { text, opts } = lastToast();
      expect(text).toBe('Enviado a la Papelera: Key Art.png');
      expect(opts.duration).toBe(10_000);
      expect(opts.action?.label).toBe('Deshacer');
      // Focus moves to the next row.
      await waitFor(() =>
        expect(document.activeElement).toBe(row(r, 'Launch Trailer.webm')),
      );
      await act(async () => {
        opts.action!.onClick();
        await Promise.resolve();
      });
      await waitFor(() =>
        expect(pathOf('Key Art.png')).toBe('Artwork/Key Art.png'),
      );
      expect(live(r)).toHaveTextContent('Deshecho');
    });

    it('asks first for more than one file', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      fireEvent.click(row(r, '01 Opening Tide.flac'));
      fireEvent.click(row(r, '02 Coral Depths.wav'), { shiftKey: true });
      fireEvent.keyDown(row(r, '02 Coral Depths.wav'), { key: 'Delete' });
      const dialog = await screen.findByRole('alertdialog');
      expect(
        within(dialog).getByText('¿Enviar 2 archivos a la Papelera?'),
      ).toBeInTheDocument();
      expect(
        within(dialog).getByText(
          'Deshacer los devuelve desde la Papelera mientras no la vacíes.',
        ),
      ).toBeInTheDocument();
      fireEvent.click(
        within(dialog).getByRole('button', { name: 'Enviar a la Papelera' }),
      );
      await waitFor(() =>
        expect(pathOf('01 Opening Tide.flac')).toBeUndefined(),
      );
      expect(pathOf('02 Coral Depths.wav')).toBeUndefined();
      expect(lastToast().text).toBe('2 archivos enviados a la Papelera');
    });

    it('asks first on a drive with no Trash and says where files go', async () => {
      show('Comet Derby');
      const r = await listed('Comet Derby');
      openMenu(r, 'Livery.png');
      fireEvent.click(
        await screen.findByRole('menuitem', { name: 'Enviar a la Papelera' }),
      );
      const dialog = await screen.findByRole('alertdialog');
      expect(
        within(dialog).getByText('¿Enviar «Livery.png» a la Papelera?'),
      ).toBeInTheDocument();
      expect(
        within(dialog).getByText(
          /Este disco no tiene Papelera: los archivos se apartan en \.coverly-trash/,
        ),
      ).toBeInTheDocument();
      fireEvent.click(within(dialog).getByRole('button', { name: 'Cancelar' }));
      expect(pathOf('Livery.png')).toBe('Artwork/Livery.png');
    });
  });

  describe('tags (D20)', () => {
    it('T opens the tags popover on a focused row; Enter adds, Backspace removes, closing saves', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      const name = row(r, 'Key Art.png');
      name.focus();
      fireEvent.keyDown(name, { key: 't', code: 'KeyT' });
      const input = await screen.findByRole('textbox', {
        name: 'Añadir etiqueta',
      });
      const pop = screen.getByRole('dialog');
      expect(
        within(pop).getByText('Etiquetas de Key Art.png'),
      ).toBeInTheDocument();
      // Suggestions come from the game's other tags.
      fireEvent.click(within(pop).getByRole('button', { name: 'ambiente' }));
      fireEvent.change(input, { target: { value: 'póster' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      fireEvent.change(input, { target: { value: 'borrar' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      expect(
        screen.getByRole('button', { name: 'Quitar la etiqueta borrar' }),
      ).toBeInTheDocument();
      fireEvent.keyDown(input, { key: 'Backspace' });
      expect(
        screen.queryByRole('button', { name: 'Quitar la etiqueta borrar' }),
      ).toBeNull();
      fireEvent.keyDown(input, { key: 'Escape' });
      await waitFor(() =>
        expect(file('Key Art.png').tags).toEqual(['ambiente', 'póster']),
      );
      expect(lastToast().text).toBe('Etiquetas guardadas: Key Art.png');
    });

    it('T does nothing while typing', async () => {
      show('Archive Run');
      const r = await listed('Archive Run');
      const search = within(r).getByRole('searchbox', {
        name: 'Buscar en los extras',
      });
      fireEvent.keyDown(search, { key: 't', code: 'KeyT' });
      expect(screen.queryByText(/^Etiquetas de/)).toBeNull();
    });

    it('filters by tag chips (all of them) and by text over 12 files', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      // 11 files: no text filter yet.
      expect(within(r).queryByRole('searchbox')).toBeNull();
      const chips = within(r).getByLabelText('Filtrar por etiqueta');
      fireEvent.click(within(chips).getByRole('button', { name: 'lo-fi' }));
      const names = within(r)
        .getAllByRole('group')
        .map(
          (g) => within(g).queryByRole('heading', { level: 4 })?.textContent,
        );
      expect(names).toEqual(['Remixes', 'No encontrados (1)']);
      // AND: no file carries both.
      fireEvent.click(within(chips).getByRole('button', { name: 'ambiente' }));
      expect(
        within(r).getByText('Ningún archivo coincide con el filtro.'),
      ).toBeInTheDocument();
    });

    it('filters a large game by text and opens the band it narrows', async () => {
      show('Archive Run');
      const r = await listed('Archive Run');
      const search = within(r).getByRole('searchbox', {
        name: 'Buscar en los extras',
      });
      fireEvent.change(search, { target: { value: 'scan 01' } });
      const art = within(r).getByRole('group', { name: 'Arte' });
      expect(within(art).getAllByRole('listitem')).toHaveLength(10);
      // The count stays the category's.
      expect(art.querySelector('.extras-band__count')).toHaveTextContent('240');
    });
  });

  describe('selection and the bulk bar', () => {
    it('click selects one, Cmd/Ctrl+click toggles, Shift+click ranges, Space toggles', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      const sel = () => [...useExtrasStore.getState().selection].sort();
      fireEvent.click(row(r, '01 Opening Tide.flac'));
      expect(sel()).toEqual([file('01 Opening Tide.flac').index]);
      fireEvent.click(row(r, '03 Leviathan.flac'), { shiftKey: true });
      expect(sel()).toHaveLength(3);
      fireEvent.click(row(r, '02 Coral Depths.wav'), { metaKey: true });
      expect(sel()).toHaveLength(2);
      fireEvent.keyDown(row(r, '02 Coral Depths.wav'), { key: ' ' });
      expect(sel()).toHaveLength(3);
      // The checkbox says it, named by the file.
      expect(
        within(r).getByRole('checkbox', {
          name: 'Seleccionar 02 Coral Depths.wav',
        }),
      ).toBeChecked();
      const bar = within(r).getByRole('toolbar', { name: /seleccionad/ });
      expect(bar).toHaveTextContent('3 seleccionados');
      expect(bar.querySelector('.mono')).toHaveTextContent('3');
      fireEvent.click(
        within(bar).getByRole('button', { name: 'Quitar la selección' }),
      );
      expect(sel()).toEqual([]);
      expect(
        within(r).queryByRole('toolbar', { name: /seleccionad/ }),
      ).toBeNull();
    });

    it('moves the selection from the bulk bar in one job', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      fireEvent.click(row(r, '01 Opening Tide.flac'));
      fireEvent.click(row(r, '02 Coral Depths.wav'), { ctrlKey: true });
      const bar = within(r).getByRole('toolbar', { name: /seleccionad/ });
      fireEvent.pointerDown(
        within(bar).getByRole('button', { name: 'Mover a…' }),
        { button: 0, ctrlKey: false },
      );
      fireEvent.click(await screen.findByRole('menuitem', { name: 'Remixes' }));
      await waitFor(() =>
        expect(pathOf('01 Opening Tide.flac')).toBe(
          'Remixes/01 Opening Tide.flac',
        ),
      );
      expect(lastToast().text).toBe('2 archivos movidos a Remixes');
      // One job: Ctrl+Z puts both back.
      fireEvent.keyDown(r, { key: 'z', code: 'KeyZ', ctrlKey: true });
      await waitFor(() =>
        expect(pathOf('01 Opening Tide.flac')).toBe(
          'Soundtrack/01 Opening Tide.flac',
        ),
      );
      expect(pathOf('02 Coral Depths.wav')).toBe(
        'Soundtrack/02 Coral Depths.wav',
      );
    });

    it('tags every selected file from the bulk bar, undone together', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      fireEvent.click(row(r, 'Key Art.png'));
      fireEvent.click(row(r, 'Launch Trailer.webm'), { ctrlKey: true });
      const bar = within(r).getByRole('toolbar', { name: /seleccionad/ });
      fireEvent.click(within(bar).getByRole('button', { name: 'Etiquetar' }));
      const input = await screen.findByRole('textbox', {
        name: 'Añadir una etiqueta a 2 archivos',
      });
      fireEvent.change(input, { target: { value: 'prensa' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      await waitFor(() =>
        expect(file('Launch Trailer.webm').tags).toEqual(['prensa']),
      );
      expect(file('Key Art.png').tags).toEqual(['prensa']);
      await act(async () => {
        lastToast().opts.action!.onClick();
        await Promise.resolve();
      });
      await waitFor(() => expect(file('Key Art.png').tags).toEqual([]));
      expect(file('Launch Trailer.webm').tags).toEqual([]);
    });
  });

  describe('own categories (D1)', () => {
    it('creates one inline; a fixed name is refused with the reason', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      fireEvent.click(
        within(r).getByRole('button', { name: 'Nueva categoría' }),
      );
      const input = within(r).getByRole('textbox', {
        name: 'Nombre de la categoría',
      });
      fireEvent.change(input, { target: { value: 'Soundtrack' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      expect(await within(r).findByRole('alert')).toHaveTextContent(
        'Ese nombre no sirve: ya existe',
      );
      fireEvent.change(input, { target: { value: 'Bocetos' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      expect(
        await within(r).findByRole('group', { name: 'Bocetos' }),
      ).toBeInTheDocument();
      expect(lastToast().text).toBe('Categoría creada: Bocetos');
    });

    it('renames and removes its own category from the header menu', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      fireEvent.pointerDown(
        within(r).getByRole('button', { name: 'Más acciones para Remixes' }),
        { button: 0, ctrlKey: false },
      );
      fireEvent.click(
        await screen.findByRole('menuitem', { name: 'Renombrar' }),
      );
      const input = within(r).getByRole('textbox', {
        name: 'Nombre de la categoría',
      });
      fireEvent.change(input, { target: { value: 'Versiones' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      expect(
        await within(r).findByRole('group', { name: 'Versiones' }),
      ).toBeInTheDocument();
      expect(pathOf('02 Coral Depths.mp3')).toBeDefined();

      fireEvent.pointerDown(
        within(r).getByRole('button', { name: 'Más acciones para Versiones' }),
        { button: 0, ctrlKey: false },
      );
      fireEvent.click(
        await screen.findByRole('menuitem', { name: 'Quitar categoría' }),
      );
      const dialog = await screen.findByRole('alertdialog');
      expect(
        within(dialog).getByText(
          'Quitar «Versiones» y enviar su archivo a la Papelera',
        ),
      ).toBeInTheDocument();
      fireEvent.click(
        within(dialog).getByRole('button', { name: 'Quitar categoría' }),
      );
      await waitFor(() =>
        expect(
          within(r).queryByRole('group', { name: 'Versiones' }),
        ).toBeNull(),
      );
    });
  });

  describe('keys (D23)', () => {
    it('Cmd/Ctrl+Z in the section undoes the last Extras job of this game', async () => {
      show('Tidewalker');
      const r = await listed('Tidewalker');
      fireEvent.keyDown(row(r, 'Key Art.png'), {
        key: 'ArrowDown',
        altKey: true,
      });
      await waitFor(() =>
        expect(pathOf('Key Art.png')).toBe('Videos/Key Art.png'),
      );
      const outer = vi.fn();
      window.addEventListener('keydown', outer);
      fireEvent.keyDown(row(r, 'Key Art.png'), {
        key: 'z',
        code: 'KeyZ',
        metaKey: true,
      });
      await waitFor(() =>
        expect(pathOf('Key Art.png')).toBe('Artwork/Key Art.png'),
      );
      expect(outer).not.toHaveBeenCalled();
      window.removeEventListener('keydown', outer);
    });

    it('keeps the library shortcuts + - 0 i / from firing on a row', async () => {
      const outer = vi.fn();
      window.addEventListener('keydown', outer);
      show('Tidewalker');
      const r = await listed('Tidewalker');
      for (const key of ['+', '-', '0', 'i', '/'])
        fireEvent.keyDown(row(r, 'Key Art.png'), { key });
      expect(outer).not.toHaveBeenCalled();
      window.removeEventListener('keydown', outer);
    });

    it('offers no write while the folder is read-only', async () => {
      show('Neon Rhythm');
      const r = await listed('Neon Rhythm');
      fireEvent.keyDown(row(r, 'Pulse.mp3'), { key: 'F2' });
      expect(within(r).queryByRole('textbox')).toBeNull();
      fireEvent.keyDown(row(r, 'Pulse.mp3'), { key: 'Delete' });
      expect(screen.queryByRole('alertdialog')).toBeNull();
      expect(
        within(r).getByRole('button', { name: 'Nueva categoría' }),
      ).toBeDisabled();
    });
  });
});
