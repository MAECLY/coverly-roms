import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend, type Backend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { MOCK_ADDONS } from '@/ipc/mock/addonViewsFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { ChildView, LibraryView } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';
import { canReadChildArt } from '@/lib/childArt';
import { buildGames, type Game } from '@/lib/games';

import { DlcSection } from './DlcSection';

let backend: Backend;
let lib: LibraryView;
let gameOf: (prefix: string) => Game;

beforeEach(async () => {
  backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
  setBackend(backend);
  lib = await backend.openLibrary(MOCK_ADDONS);
  const { byEntry } = buildGames(lib.entries, {});
  gameOf = (prefix) =>
    byEntry.get(lib.entries.find((e) => e.name.startsWith(prefix))!.id)!;
  vi.spyOn(URL, 'createObjectURL').mockImplementation(() => 'blob:x');
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
  clearArtCache();
  setBackend(null);
});

const systemOf = (g: Game) => lib.systems[g.entry.system]!.system_id;

function section(
  game: Game,
  {
    fresh,
    onChildren = vi.fn(),
  }: {
    fresh?: ReadonlySet<string>;
    onChildren?: (c: ChildView[]) => void;
  } = {},
) {
  render(
    <I18nProvider locale="es">
      <DlcSection
        generation={lib.generation}
        game={game}
        systemId={systemOf(game)}
        fresh={fresh}
        onChildren={onChildren}
      />
    </I18nProvider>,
  );
  return screen.getByRole('region', { name: /^DLC/ });
}

describe('DlcSection (addon views §8.8 4, D11)', () => {
  it('lists each DLC with its art, name, file and origin in words', () => {
    const lh = gameOf('Lantern Harbor (USA)');
    const sec = section(lh);
    expect(sec).toHaveAttribute('id', 'panel-dlc');
    expect(within(sec).getByText('1 en la carpeta DLC/')).toBeInTheDocument();
    const row = within(sec).getByRole('listitem');
    expect(row).toHaveAttribute('data-panel-section', 'child-2');
    expect(row.querySelector('.child-thumb')).toHaveAttribute(
      'data-art',
      'own',
    );
    expect(
      within(row).getByText('Lantern Harbor - Lighthouse Keeper Pack'),
    ).toBeInTheDocument();
    // The file in mono, truncated, with its full path as a tooltip.
    const file = within(row).getByText(
      'Lantern Harbor - Lighthouse Keeper Pack.pkg',
    );
    expect(file).toHaveClass('mono');
    expect(file).toHaveAttribute(
      'title',
      'ROMS/PS4/Lantern Harbor/DLC/Lantern Harbor - Lighthouse Keeper Pack.pkg',
    );
    expect(within(row).getByText('incluido en el paquete')).toBeInTheDocument();
    // Its updates carry no art of their own yet: the read is offered.
    expect(canReadChildArt(lh, systemOf(lh))).toBe(true);
    expect(
      within(sec).getByRole('button', { name: 'Leer el arte incluido' }),
    ).toBeEnabled();
  });

  it('says «arte del juego» for Switch and «sin arte» in warning tone', () => {
    const tw = gameOf('Tidewalker');
    const sec = section(tw);
    const row = within(sec).getByRole('listitem');
    expect(row.querySelector('.child-thumb')).toHaveAttribute(
      'data-art',
      'base',
    );
    expect(within(row).getByText('arte del juego')).toBeInTheDocument();
    // A Switch package carries nothing in the clear: no read offered.
    expect(canReadChildArt(tw, 'switch')).toBe(false);
  });

  it('tags «Nuevo» only the children the last open had not seen', () => {
    const nr = gameOf('Neon Rhythm');
    const fresh = new Set([nr.dlc[0]!.rel_path]);
    const sec = section(nr, { fresh });
    expect(within(sec).getByText('3 en la carpeta DLC/')).toBeInTheDocument();
    const rows = within(sec).getAllByRole('listitem');
    expect(rows.map((r) => within(r).queryByText('Nuevo') !== null)).toEqual([
      true,
      false,
      false,
    ]);
    const noArt = within(rows[1]!).getByText('sin arte', {
      selector: '.panel-row__origin',
    });
    expect(noArt).toHaveClass('panel-row__origin--warn');
  });

  it('reads the included art into the empty slots and says how many', async () => {
    const onChildren = vi.fn();
    const readChildArt = vi.fn(backend.readChildArt);
    setBackend({ ...backend, readChildArt });
    const nr = gameOf('Neon Rhythm');
    const sec = section(nr, { onChildren });
    await userEvent.click(
      within(sec).getByRole('button', { name: 'Leer el arte incluido' }),
    );
    expect(readChildArt).toHaveBeenCalledWith(lib.generation, nr.entry.id);
    await waitFor(() => expect(onChildren).toHaveBeenCalledTimes(1));
    expect(onChildren.mock.calls[0]![0]).toHaveLength(3);
    expect(
      await within(sec).findByText('Arte leído para 3 archivos.'),
    ).toHaveClass('sr-only');
  });

  it('keeps «Leer el arte incluido» off with the read-only reason', async () => {
    setBackend({
      ...backend,
      readChildArt: () =>
        Promise.reject(new BackendError('write-denied', '/Volumes/SD')),
    });
    const sec = section(gameOf('Neon Rhythm'));
    const read = within(sec).getByRole('button', {
      name: 'Leer el arte incluido',
    });
    await userEvent.click(read);
    await waitFor(() => expect(read).toBeDisabled());
    expect(read).toHaveAccessibleDescription(
      /no puede escribir en la biblioteca.*\/Volumes\/SD/,
    );
  });

  it('«Buscar arte» searches for the DLC and writes nothing without a pick', async () => {
    const searchChildArt = vi.fn(backend.searchChildArt);
    const setChildArtFromSearch = vi.fn(backend.setChildArtFromSearch);
    const onChildren = vi.fn();
    setBackend({ ...backend, searchChildArt, setChildArtFromSearch });
    const nr = gameOf('Neon Rhythm');
    const sec = section(nr, { onChildren });
    const night = nr.dlc.find((c) => c.name.endsWith('Night Drive Pack'))!;
    await userEvent.click(
      within(sec).getByRole('button', {
        name: 'Buscar arte para Neon Rhythm - Night Drive Pack',
      }),
    );
    // The query is the game and the DLC; the search box takes the focus.
    const box = within(sec).getByRole('searchbox', {
      name: 'Título que buscar',
    });
    expect(box).toHaveValue('Neon Rhythm - Night Drive Pack');
    expect(box).toHaveFocus();
    const results = await within(sec).findByRole('group', {
      name: 'Resultados de la búsqueda',
    });
    expect(searchChildArt).toHaveBeenCalledWith(
      lib.generation,
      nr.entry.id,
      night.index,
      'Neon Rhythm - Night Drive Pack',
    );
    expect(setChildArtFromSearch).not.toHaveBeenCalled();
    expect(onChildren).not.toHaveBeenCalled();

    const picks = within(results).getAllByRole('button');
    await userEvent.click(picks[1]!);
    expect(setChildArtFromSearch).toHaveBeenCalledWith(
      lib.generation,
      nr.entry.id,
      night.index,
      'icon',
      1,
    );
    await waitFor(() => expect(onChildren).toHaveBeenCalledTimes(1));
    const written = onChildren.mock.calls[0]![0] as ChildView[];
    expect(written.find((c) => c.index === night.index)!.art).toMatchObject({
      icon: true,
      icon_origin: 'steamgriddb',
    });
    // The search closes once written.
    expect(within(sec).queryByRole('searchbox')).toBeNull();
  });

  it('gives the focus back to «Buscar arte» when the search closes', async () => {
    const sec = section(gameOf('Neon Rhythm'));
    const open = within(sec).getAllByRole('button', {
      name: /^Buscar arte/,
    })[0]!;
    await userEvent.click(open);
    await within(sec).findByRole('group', {
      name: 'Resultados de la búsqueda',
    });
    await userEvent.keyboard('{Escape}');
    expect(open).toHaveFocus();
    await userEvent.click(open);
    await within(sec).findByRole('group', {
      name: 'Resultados de la búsqueda',
    });
    await userEvent.click(
      within(sec).getByRole('button', { name: 'Cerrar la búsqueda' }),
    );
    expect(open).toHaveFocus();
  });

  it('drops the last search error once a new search answers', async () => {
    let calls = 0;
    setBackend({
      ...backend,
      searchChildArt: (...args) => {
        calls += 1;
        return calls === 1
          ? Promise.reject(new BackendError('network', 'offline'))
          : backend.searchChildArt(...args);
      },
    });
    const sec = section(gameOf('Neon Rhythm'));
    await userEvent.click(
      within(sec).getAllByRole('button', { name: /^Buscar arte/ })[0]!,
    );
    expect(await within(sec).findByRole('alert')).toBeInTheDocument();
    await userEvent.click(within(sec).getByRole('button', { name: 'Buscar' }));
    await within(sec).findByRole('group', {
      name: 'Resultados de la búsqueda',
    });
    expect(within(sec).queryByRole('alert')).toBeNull();
  });

  it('starts no new search while a pick is being written', async () => {
    let finish: (c: ChildView[]) => void = () => {};
    const searchChildArt = vi.fn(backend.searchChildArt);
    setBackend({
      ...backend,
      searchChildArt,
      setChildArtFromSearch: () =>
        new Promise<ChildView[]>((r) => {
          finish = r;
        }),
    });
    const nr = gameOf('Neon Rhythm');
    const sec = section(nr);
    await userEvent.click(
      within(sec).getAllByRole('button', { name: /^Buscar arte/ })[0]!,
    );
    const results = await within(sec).findByRole('group', {
      name: 'Resultados de la búsqueda',
    });
    await userEvent.click(within(results).getAllByRole('button')[0]!);
    // The pick names a result of this search: another search now could
    // make it write a result of that one instead.
    const go = within(sec).getByRole('button', { name: 'Buscar' });
    expect(go).toBeDisabled();
    await userEvent.type(within(sec).getByRole('searchbox'), '{Enter}');
    expect(searchChildArt).toHaveBeenCalledTimes(1);
    finish([...nr.children]);
  });

  it('closes the search with Escape, writing nothing', async () => {
    const setChildArtFromSearch = vi.fn(backend.setChildArtFromSearch);
    setBackend({ ...backend, setChildArtFromSearch });
    const sec = section(gameOf('Neon Rhythm'));
    const open = within(sec).getAllByRole('button', {
      name: /^Buscar arte/,
    })[0]!;
    await userEvent.click(open);
    expect(open).toHaveAttribute('aria-expanded', 'true');
    await within(sec).findByRole('group', {
      name: 'Resultados de la búsqueda',
    });
    await userEvent.keyboard('{Escape}');
    expect(within(sec).queryByRole('searchbox')).toBeNull();
    expect(setChildArtFromSearch).not.toHaveBeenCalled();
  });
});
