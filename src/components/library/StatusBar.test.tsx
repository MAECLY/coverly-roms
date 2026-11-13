import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { Locale } from '@/i18n/core';
import type { ArtMarks, EntryView } from '@/ipc/types';
import { buildGames, type Game, type GameCounts } from '@/lib/games';
import { useViewStore } from '@/stores/viewStore';

import { StatusBar } from './StatusBar';

const ART = (cover: ArtMarks['cover']): ArtMarks => ({
  cover,
  screenshot: 'missing',
  title: 'missing',
});

const entry = (id: number, over: Partial<EntryView> = {}): EntryView => ({
  id,
  system: 0,
  rel_path: `SFC/${String(id)}.sfc`,
  dir: 'SFC',
  name: `Game ${String(id)}`,
  ext: 'sfc',
  size: 1,
  mtime: 0,
  regions: [],
  art: ART('present'),
  ...over,
});

const counts = (over: Partial<GameCounts> = {}): GameCounts => ({
  games: 9,
  demos: 2,
  updates: 5,
  dlc: 5,
  moreRevisions: 1,
  orphans: 0,
  ...over,
});

function bar(
  props: {
    total?: GameCounts;
    shown?: GameCounts;
    filtered?: boolean;
    games?: readonly Game[];
  },
  locale: Locale = 'es',
) {
  const total = props.total ?? counts();
  return render(
    <I18nProvider locale={locale}>
      <StatusBar
        total={total}
        shown={props.shown ?? total}
        filtered={props.filtered ?? false}
        games={props.games ?? []}
        scanMs={1200}
      />
    </I18nProvider>,
  );
}

const countsText = () => screen.getByTestId('status-counts').textContent;

describe('StatusBar', () => {
  afterEach(() => useViewStore.getState().reset());

  it('counts games, demos and what is inside them, unfiltered', () => {
    bar({});
    expect(countsText()).toBe(
      '9 juegos, 2 de ellos demos · dentro de ellos: 5 actualizaciones, 5 DLC y 1 revisión más (no cuentan como juegos)',
    );
  });

  it('says «4 de 9 juegos» with a scope or filters, inner clause from the shown games', () => {
    bar({
      shown: counts({
        games: 4,
        demos: 1,
        updates: 2,
        dlc: 0,
        moreRevisions: 0,
      }),
      filtered: true,
    });
    expect(countsText()).toBe(
      '4 de 9 juegos, 1 de ellos demo · dentro de ellos: 2 actualizaciones (no cuentan como juegos)',
    );
  });

  it('omits every clause whose count is 0 and says «1 juego»', () => {
    bar({
      total: counts({
        games: 1,
        demos: 0,
        updates: 0,
        dlc: 0,
        moreRevisions: 0,
      }),
    });
    expect(countsText()).toBe('1 juego');
  });

  it('uses the singular forms', () => {
    bar({
      total: counts({
        games: 3,
        demos: 1,
        updates: 1,
        dlc: 1,
        moreRevisions: 2,
      }),
    });
    expect(countsText()).toBe(
      '3 juegos, 1 de ellos demo · dentro de ellos: 1 actualización, 1 DLC y 2 revisiones más (no cuentan como juegos)',
    );
  });

  it('says the same in English', () => {
    bar({}, 'en');
    expect(countsText()).toBe(
      '9 games, 2 of them demos · inside them: 5 updates, 5 DLC, and 1 more revision (not counted as games)',
    );
  });

  it('formats numbers with the locale and sets them in mono', () => {
    bar({
      total: counts({
        games: 12345,
        demos: 0,
        updates: 0,
        dlc: 0,
        moreRevisions: 0,
      }),
    });
    expect(countsText()).toBe('12.345 juegos');
    const mono = screen.getByTestId('status-counts').querySelector('.mono');
    expect(mono?.textContent).toBe('12.345');
  });

  it('turns the orphans clause into a button that opens their list', async () => {
    bar({ total: counts({ orphans: 2 }) });
    const button = screen.getByRole('button', {
      name: '2 complementos sin juego base',
    });
    await userEvent.click(button);
    expect(useViewStore.getState().scope).toEqual({ kind: 'orphans' });
  });

  it('names one orphan in the singular, and none when there are none', () => {
    const { unmount } = bar({ total: counts({ orphans: 1 }) }, 'en');
    expect(
      screen.getByRole('button', { name: '1 add-on without its game' }),
    ).toBeInTheDocument();
    unmount();
    bar({});
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('counts «sin portada» by each game’s default file', () => {
    // Two revisions of one game: the default (Rev 1) has a cover, the
    // other file does not; a second game has none.
    const { games } = buildGames(
      [
        entry(0, { version_group: 'g', art: ART('missing') }),
        entry(1, { version_group: 'g', revision: 'Rev 1' }),
        entry(2, { art: ART('missing') }),
      ],
      {},
    );
    bar({ games });
    expect(screen.getByText('1 sin portada')).toBeInTheDocument();
  });
});
