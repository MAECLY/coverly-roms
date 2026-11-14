import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { useRef } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend, type Backend } from '@/ipc/backend';
import { MOCK_EXTRAS } from '@/ipc/mock/extrasFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import type { EntryView, ExtrasView, LibraryView } from '@/ipc/types';
import { useExtrasStore } from '@/stores/extrasStore';
import { usePlayerStore } from '@/stores/playerStore';
import { useViewerStore } from '@/stores/viewerStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

import { usePanelSection } from '../usePanelSection';
import { ExtrasSection } from './ExtrasSection';
import { FixedFoldersSection } from './FixedFoldersSection';

let b: MockBackend;
let lib: LibraryView;

const entry = (name: string): EntryView => {
  const e = lib.entries.find((x) => x.name === name);
  if (!e) throw new Error(name);
  return e;
};

function show(name: string, generation = lib.generation) {
  return render(
    <I18nProvider locale="es">
      <ExtrasSection generation={generation} entry={entry(name)} />
    </I18nProvider>,
  );
}

async function region() {
  return screen.findByRole('region', { name: /^Extras/ });
}

/** Waits for the listing of `name` to be on screen. */
async function listed(name: string) {
  const r = await region();
  await waitFor(() =>
    expect(r).toHaveAttribute('data-entry', String(entry(name).id)),
  );
  await waitFor(() => expect(r).not.toHaveAttribute('aria-busy', 'true'));
  return r;
}

describe('ExtrasSection (extras spec §8.1, §8.2, §8.4)', () => {
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
  });
  afterEach(() => {
    useExtrasStore.getState().reset();
    usePlayerStore.getState().stop();
    useViewStore.setState({ panelSection: null });
    setBackend(null);
  });

  it('is a region with the file count in mono and one group per category', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    expect(r).toHaveAttribute('data-panel-section', 'extras');
    const h = within(r).getByRole('heading', { level: 3 });
    expect(h).toHaveTextContent(/^Extras/);
    expect(h.querySelector('.mono')).toHaveTextContent('11');
    const groups = within(r).getAllByRole('group');
    const names = within(r)
      .getAllByRole('group')
      .map((g) => within(g).getByRole('heading', { level: 4 }).textContent);
    expect(groups).toHaveLength(8);
    expect(names).toEqual([
      'Banda sonora',
      'Arte',
      'Vídeos',
      'Temas y avatares',
      'Guías',
      'Remixes',
      'Sin categoría',
      'No encontrados (1)',
    ]);
    // Each group is named by its header.
    const soundtrack = within(r).getByRole('group', { name: 'Banda sonora' });
    expect(within(soundtrack).getAllByRole('listitem')).toHaveLength(3);
  });

  it('shows the folder on disk and the counts in mono', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const soundtrack = within(r).getByRole('group', { name: 'Banda sonora' });
    const dir = within(soundtrack).getByText('Soundtrack/');
    expect(dir).toHaveClass('mono');
    const count = soundtrack.querySelector('.extras-band__count')!;
    expect(count).toHaveClass('mono');
    expect(count).toHaveTextContent('3');
    // A user category is its own folder: no second name.
    const remixes = within(r).getByRole('group', { name: 'Remixes' });
    expect(within(remixes).queryByText('Remixes/')).toBeNull();
  });

  it('collapses the empty suggested categories into dashed slots', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const slots = r.querySelectorAll('.extras-slot');
    expect([...slots].map((s) => s.textContent)).toEqual(['Manuales']);
    expect(slots[0]).toHaveAccessibleName('Añadir a Manuales');
  });

  it('marks only the category header of official files', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const marks = r.querySelectorAll('.extras-official');
    expect(marks).toHaveLength(1);
    const themes = within(r).getByRole('group', { name: 'Temas y avatares' });
    expect(themes.querySelector('.extras-band__head .extras-official')).toBe(
      marks[0],
    );
    expect(marks[0]).toHaveTextContent('Oficial');
  });

  it('names each row with its type, size, tags and official mark', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    expect(
      within(r).getByRole('button', {
        name: /^03 Leviathan\.flac, audio, 32,3 MB, etiquetas: tema del jefe final, versión orquestal$/,
      }),
    ).toBeInTheDocument();
    expect(
      within(r).getByRole('button', {
        name: /^Tidewalker Theme\.p3t, otro, 4,6 MB, oficial$/,
      }),
    ).toBeInTheDocument();
    const row = within(r)
      .getByRole('button', { name: /^Key Art\.png/ })
      .closest('li')!;
    expect(row.querySelector('.extras-row__size')).toHaveClass('mono');
    expect(row.querySelector('.extras-row__size')).toHaveTextContent('2,3 MB');
    // Tags are chips on a second line.
    const leviathan = within(r)
      .getByRole('button', { name: /^03 Leviathan/ })
      .closest('li')!;
    expect(
      [...leviathan.querySelectorAll('.extras-tag')].map((t) => t.textContent),
    ).toEqual(['tema del jefe final', 'versión orquestal']);
  });

  it('says what an official file is in its row menu', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const trigger = within(r).getByRole('button', {
      name: 'Más acciones para Tidewalker Theme.p3t',
    });
    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false });
    expect(await screen.findByText('Oficial: tema de PS3')).toBeInTheDocument();
  });

  it('shows «No encontrados» with tags, «Buscar de nuevo» and «Olvidar»', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const missing = within(r).getByRole('group', {
      name: 'No encontrados (1)',
    });
    expect(missing).toHaveClass('extras-missing');
    expect(within(missing).getByText('04 Ending.flac')).toBeInTheDocument();
    expect(within(missing).getByText('créditos')).toHaveClass('extras-tag');
    expect(
      within(missing).getByText(
        'Estaban en extras.json pero ya no están en la carpeta. Sus etiquetas se conservan.',
      ),
    ).toBeInTheDocument();
    const refresh = vi.spyOn(useExtrasStore.getState(), 'refresh');
    fireEvent.click(
      within(missing).getByRole('button', { name: 'Buscar de nuevo' }),
    );
    expect(refresh).toHaveBeenCalled();
    refresh.mockRestore();
    await waitFor(() => expect(r).not.toHaveAttribute('aria-busy', 'true'));
    fireEvent.click(
      within(missing).getByRole('button', { name: 'Olvidar 04 Ending.flac' }),
    );
    await waitFor(() =>
      expect(
        within(r).queryByRole('group', { name: /^No encontrados/ }),
      ).toBeNull(),
    );
  });

  it('shows six dashed slots, the hint and «Añadir archivos» with no Extras/ yet', async () => {
    show('Moss Garden');
    const r = await listed('Moss Garden');
    const slots = [...r.querySelectorAll('.extras-slot')];
    expect(slots.map((s) => s.textContent)).toEqual([
      'Banda sonora',
      'Manuales',
      'Arte',
      'Vídeos',
      'Temas y avatares',
      'Guías',
    ]);
    expect(
      within(r).getByText(
        'Arrastra aquí la banda sonora, los manuales o el arte de este juego, o usa Añadir.',
      ),
    ).toBeInTheDocument();
    expect(
      within(r).getByRole('button', { name: 'Añadir archivos' }),
    ).toBeInTheDocument();
    // 3DS keeps the e-manual inside the game (D16).
    expect(
      within(r).getByText('El manual electrónico va dentro del juego.'),
    ).toBeInTheDocument();
  });

  it('shows the manual note in Manuales when that category is there', async () => {
    const listing = await b.extrasList(lib.generation, entry('Tidewalker').id);
    const manuals: ExtrasView = {
      ...listing,
      manual_inside: true,
      categories: listing.categories.map((c) =>
        c.category.kind === 'fixed' && c.category.name === 'manuals'
          ? { ...c, present: true, count: 1 }
          : c,
      ),
      files: [
        ...listing.files,
        {
          ...listing.files[0]!,
          index: 999,
          category: { kind: 'fixed', name: 'manuals' },
          path: 'Manuals/Manual.pdf',
          name: 'Manual.pdf',
          kind: 'document',
          tags: [],
        },
      ],
    };
    setBackend({
      extrasList: () => Promise.resolve(manuals),
    } as unknown as Backend);
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const group = within(r).getByRole('group', { name: 'Manuales' });
    expect(
      within(group).getByText('El manual electrónico va dentro del juego.'),
    ).toBeInTheDocument();
  });

  it('warns about an extras.json it cannot read and still allows writes', async () => {
    show('Lantern Harbor');
    const r = await listed('Lantern Harbor');
    const text = within(r).getByText(
      'No se puede leer extras.json. Las etiquetas guardadas no se muestran; el archivo no se ha tocado.',
    );
    expect(text.closest('[role="status"]')).toHaveClass('bg-cr-warning-bg');
    expect(
      within(r).getByRole('button', { name: 'Añadir archivos' }),
    ).toBeEnabled();
  });

  it.each([
    [
      'Paper Skies',
      'extras.json es de una versión más nueva de Coverly; aquí solo se puede ver.',
    ],
    [
      'Neon Rhythm',
      'Coverly no puede escribir en esta carpeta; sus extras se pueden ver y reproducir.',
    ],
  ])('%s: one reason line, writes off, rows still open', async (name, why) => {
    show(name);
    const r = await listed(name);
    expect(within(r).getAllByText(why)).toHaveLength(1);
    expect(
      within(r).getByRole('button', { name: 'Añadir archivos' }),
    ).toBeDisabled();
    for (const slot of r.querySelectorAll('.extras-slot'))
      expect(slot).toBeDisabled();
    for (const row of r.querySelectorAll('[data-extras-primary]'))
      expect(row).toBeEnabled();
  });

  it('says the library is busy', async () => {
    const listing = await b.extrasList(lib.generation, entry('Tidewalker').id);
    setBackend({
      extrasList: () =>
        Promise.resolve({ ...listing, writable: { kind: 'busy' } }),
    } as unknown as Backend);
    show('Tidewalker');
    const r = await listed('Tidewalker');
    expect(
      within(r).getByText(
        'Hay otro trabajo en la biblioteca; los cambios esperan a que termine.',
      ),
    ).toBeInTheDocument();
    expect(
      within(r).getByRole('button', { name: 'Añadir archivos' }),
    ).toBeDisabled();
  });

  it('keeps the last listing and shows a spinner in the header while loading', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    let release!: (v: ExtrasView) => void;
    const listing = useExtrasStore.getState().listing!;
    setBackend({
      extrasList: () =>
        new Promise<ExtrasView>((res) => {
          release = res;
        }),
    } as unknown as Backend);
    act(() => {
      void useExtrasStore.getState().refresh();
    });
    await waitFor(() => expect(r).toHaveAttribute('aria-busy', 'true'));
    const h = within(r).getByRole('heading', { level: 3 });
    expect(h.parentElement!.querySelector('[role="status"]')).not.toBeNull();
    expect(
      within(r).getByRole('group', { name: 'Banda sonora' }),
    ).toBeInTheDocument();
    await act(async () => {
      release({ ...listing, rev: listing.rev + 1 });
      await Promise.resolve();
    });
    await waitFor(() => expect(r).not.toHaveAttribute('aria-busy', 'true'));
  });

  it('collapses and virtualises a category of more than 200 files', async () => {
    show('Archive Run');
    const r = await listed('Archive Run');
    const art = within(r).getByRole('group', { name: 'Arte' });
    expect(art.querySelector('.extras-band__count')).toHaveTextContent('240');
    const toggle = within(art).getByRole('button', {
      name: 'Mostrar los archivos de Arte',
    });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(within(art).queryAllByRole('listitem')).toHaveLength(0);
    const undo = stubLayout(320, 352);
    try {
      fireEvent.click(toggle);
      expect(toggle).toHaveAttribute('aria-expanded', 'true');
      const rows = within(art).getAllByRole('listitem');
      expect(rows.length).toBeGreaterThan(0);
      expect(rows.length).toBeLessThan(240);
    } finally {
      undo();
    }
  });

  it('keeps the tab stop on a row in view when a band is collapsed', async () => {
    const listing = await b.extrasList(lib.generation, entry('Tidewalker').id);
    const first = listing.files[0]!;
    const big: ExtrasView = {
      ...listing,
      files: [
        ...Array.from({ length: 201 }, (_, i) => ({
          ...first,
          index: 5000 + i,
          category: { kind: 'fixed' as const, name: 'soundtrack' as const },
          path: `Soundtrack/Track ${String(i).padStart(3, '0')}.flac`,
          name: `Track ${String(i).padStart(3, '0')}.flac`,
          tags: [],
        })),
        ...listing.files.filter(
          (f) =>
            !(f.category.kind === 'fixed' && f.category.name === 'soundtrack'),
        ),
      ],
    };
    setBackend({
      extrasList: () => Promise.resolve(big),
    } as unknown as Backend);
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const soundtrack = within(r).getByRole('group', { name: 'Banda sonora' });
    expect(within(soundtrack).queryAllByRole('listitem')).toHaveLength(0);
    // The collapsed band's rows are not on screen: the stop is on the
    // first row that is.
    const stops = [
      ...r.querySelectorAll<HTMLElement>('[data-extras-primary]'),
    ].filter((p) => p.tabIndex === 0);
    expect(stops).toHaveLength(1);
    expect(stops[0]!.closest('[role="group"]')).toBe(
      within(r).getByRole('group', { name: 'Arte' }),
    );
    // Opened, the band's first row takes it back; a focused row inside it
    // hands it on when the band closes again.
    const undo = stubLayout(320, 352);
    try {
      const toggle = within(soundtrack).getByRole('button', {
        name: 'Mostrar los archivos de Banda sonora',
      });
      fireEvent.click(toggle);
      const row = soundtrack.querySelector<HTMLElement>(
        '[data-extras-primary]',
      )!;
      act(() => row.focus());
      fireEvent.click(toggle);
      const after = [
        ...r.querySelectorAll<HTMLElement>('[data-extras-primary]'),
      ].filter((p) => p.tabIndex === 0);
      expect(after).toHaveLength(1);
    } finally {
      undo();
    }
  });

  it('moves the focus between rows with a roving tabindex', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const primaries = [
      ...r.querySelectorAll<HTMLElement>('[data-extras-primary]'),
    ];
    expect(primaries.filter((p) => p.tabIndex === 0)).toHaveLength(1);
    expect(primaries[0]!.tabIndex).toBe(0);
    primaries[0]!.focus();
    fireEvent.keyDown(primaries[0]!, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(primaries[1]);
    await waitFor(() => expect(primaries[1]!.tabIndex).toBe(0));
    expect(primaries[0]!.tabIndex).toBe(-1);
    fireEvent.keyDown(primaries[1]!, { key: 'End' });
    expect(document.activeElement).toBe(primaries.at(-1));
    fireEvent.keyDown(document.activeElement!, { key: 'Home' });
    expect(document.activeElement).toBe(primaries[0]);
    // → reaches the row's own controls; ← comes back.
    fireEvent.keyDown(primaries[0]!, { key: 'ArrowRight' });
    expect(document.activeElement).toHaveAccessibleName(
      'Reproducir 01 Opening Tide.flac',
    );
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowRight' });
    expect(document.activeElement).toHaveAccessibleName(
      'Más acciones para 01 Opening Tide.flac',
    );
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowLeft' });
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowLeft' });
    expect(document.activeElement).toBe(primaries[0]);
  });

  it('keeps the arrow keys from the library behind it', async () => {
    const outer = vi.fn();
    window.addEventListener('keydown', outer);
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const first = r.querySelector<HTMLElement>('[data-extras-primary]')!;
    fireEvent.keyDown(first, { key: 'ArrowDown' });
    expect(outer).not.toHaveBeenCalled();
    window.removeEventListener('keydown', outer);
  });

  it('marks the row that is playing', async () => {
    show('Tidewalker');
    await listed('Tidewalker');
    const listing = useExtrasStore.getState().listing!;
    const track = listing.files.find((f) => f.name === '02 Coral Depths.wav')!;
    act(() =>
      usePlayerStore.getState().playFrom(listing, track.index, {
        generation: lib.generation,
        title: 'Tidewalker',
      }),
    );
    const row = screen
      .getByRole('button', { name: /^02 Coral Depths\.wav/ })
      .closest('li')!;
    expect(row).toHaveAttribute('data-playing', 'true');
  });

  it("«Reproducir» queues the audio of the row's category from that row", async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    fireEvent.click(
      within(r).getByRole('button', { name: 'Reproducir 02 Coral Depths.wav' }),
    );
    const p = usePlayerStore.getState();
    expect(p.playing).toBe(true);
    expect(p.queue[p.at]!.name).toBe('02 Coral Depths.wav');
    expect(p.queue[p.at]!.game).toBe('Tidewalker');
    const listing = useExtrasStore.getState().listing!;
    const start = listing.files.find((f) => f.name === '02 Coral Depths.wav')!;
    for (const t of p.queue) {
      const f = listing.files.find((x) => x.index === t.index)!;
      expect(f.kind).toBe('audio');
      expect(f.category).toEqual(start.category);
    }
  });

  it('Enter on an audio row plays it', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const name = within(r).getByRole('button', {
      name: /^02 Coral Depths\.wav/,
    });
    fireEvent.keyDown(name, { key: 'Enter' });
    const p = usePlayerStore.getState();
    expect(p.queue[p.at]?.name).toBe('02 Coral Depths.wav');
  });

  it('Enter on an image opens the viewer; Esc gives the row its focus back', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    const name = within(r).getByRole('button', { name: /^Key Art\.png/ });
    name.focus();
    fireEvent.keyDown(name, { key: 'Enter' });
    const dialog = await screen.findByRole('dialog', { name: 'Key Art.png' });
    expect(useViewerStore.getState().open?.path).toBe('Artwork/Key Art.png');
    await within(dialog).findByRole('img', { name: 'Key Art.png' });
    // The viewer's keys stay in the viewer: no rename, no tags, no trash.
    fireEvent.keyDown(dialog, { key: 'Delete' });
    fireEvent.keyDown(dialog, { key: 'F2' });
    expect(screen.queryByRole('textbox')).toBeNull();
    fireEvent.keyDown(dialog, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    await waitFor(() => expect(name).toHaveFocus());
  });

  it('the row menu leads with Abrir, so a click-only or screen-reader user can open a file too', async () => {
    show('Tidewalker');
    const r = await listed('Tidewalker');
    fireEvent.pointerDown(
      within(r).getByRole('button', { name: 'Más acciones para Key Art.png' }),
      { button: 0, ctrlKey: false },
    );
    const items = await screen.findAllByRole('menuitem');
    expect(items[0]).toHaveAccessibleName(/^Abrir/);
    expect(items[0]).toHaveTextContent('↵');
    fireEvent.click(items[0]!);
    await screen.findByRole('dialog', { name: 'Key Art.png' });
    expect(useViewerStore.getState().open?.path).toBe('Artwork/Key Art.png');
  });

  it('an open viewer closes with the panel', async () => {
    const { unmount } = show('Tidewalker');
    const r = await listed('Tidewalker');
    fireEvent.doubleClick(
      within(r).getByRole('button', { name: /^Readme\.nfo/ }),
    );
    await screen.findByRole('dialog', { name: 'Readme.nfo' });
    unmount();
    expect(useViewerStore.getState().open).toBeNull();
  });

  it('the music goes on when the panel unmounts', async () => {
    const { unmount } = show('Tidewalker');
    const r = await listed('Tidewalker');
    fireEvent.click(
      within(r).getByRole('button', { name: 'Reproducir 02 Coral Depths.wav' }),
    );
    unmount();
    expect(usePlayerStore.getState().playing).toBe(true);
  });

  it('shows nothing for an entry that is no game folder', async () => {
    const { container } = show('Pocket Quest (USA)');
    await waitFor(() =>
      expect(useExtrasStore.getState().listing?.entry_id).toBe(
        entry('Pocket Quest (USA)').id,
      ),
    );
    expect(container.querySelector('[data-panel-section="extras"]')).toBeNull();
  });

  it('is reached from «Ver extras» in the fixed folders', async () => {
    function Panel({ e }: { e: EntryView }) {
      const ref = useRef<HTMLDivElement>(null);
      usePanelSection(ref);
      return (
        <div ref={ref}>
          <FixedFoldersSection entry={e} />
          <ExtrasSection generation={lib.generation} entry={e} />
        </div>
      );
    }
    render(
      <I18nProvider locale="es">
        <Panel e={entry('Tidewalker')} />
      </I18nProvider>,
    );
    const r = await listed('Tidewalker');
    fireEvent.click(screen.getByRole('button', { name: 'Ver extras' }));
    const h = within(r).getByRole('heading', { level: 3 });
    await waitFor(() => expect(document.activeElement).toBe(h));
    expect(useViewStore.getState().panelSection).toBeNull();
  });
});
