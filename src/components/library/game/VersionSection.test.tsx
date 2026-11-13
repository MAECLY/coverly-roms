import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { MOCK_ADDONS } from '@/ipc/mock/addonViewsFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { GameDetailsView, LibraryView } from '@/ipc/types';
import { buildGames, type Game } from '@/lib/games';

import { VersionSection } from './VersionSection';
import { hasVersionFacts } from './versionFacts';

let lib: LibraryView;
let gameOf: (prefix: string) => Game;

beforeEach(async () => {
  const backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
  lib = await backend.openLibrary(MOCK_ADDONS);
  const { byEntry } = buildGames(lib.entries, {});
  gameOf = (prefix) =>
    byEntry.get(lib.entries.find((e) => e.name.startsWith(prefix))!.id)!;
});

const NO_FACTS: GameDetailsView = { request: 1, children: [] };

function section(
  game: Game,
  details: GameDetailsView | null = NO_FACTS,
  onShowElsewhere = vi.fn(),
) {
  render(
    <I18nProvider locale="es">
      <VersionSection
        game={game}
        details={details}
        onShowElsewhere={onShowElsewhere}
      />
    </I18nProvider>,
  );
  return screen.getByRole('region', { name: /^Versión/ });
}

describe('VersionSection (addon views §8.8 2, D5, D6)', () => {
  it('claims no newest version when Rust could not order the updates', () => {
    // D5: two updates equal by version_order (or one without a version)
    // leave `installed` out; the base's own version is not the newest then.
    const g = gameOf('Lantern Harbor (USA)');
    const doubt: Game = {
      ...g,
      installed: null,
      entry: { ...g.entry, version: '1.00' },
    };
    const sec = section(doubt);
    expect(sec.querySelector('.panel-ver__big')).toBeNull();
    expect(within(sec).queryByText(/la más nueva de tu biblioteca/)).toBeNull();
    expect(within(sec).queryByText('La más nueva')).toBeNull();
    // The versions are still listed, with the base.
    expect(within(sec).getByText('Juego base')).toBeInTheDocument();
  });

  it('shows the newest version in the library, then every version newest first', () => {
    const sec = section(gameOf('Lantern Harbor (USA)'));
    expect(sec).toHaveAttribute('id', 'panel-version');
    expect(within(sec).getByText('2 actualizaciones')).toBeInTheDocument();
    expect(sec.querySelector('.panel-ver__big')).toHaveTextContent('v1.10');
    expect(
      within(sec).getByText('la más nueva de tu biblioteca'),
    ).toBeInTheDocument();
    const rows = within(
      within(sec).getByRole('list', { name: 'Versiones' }),
    ).getAllByRole('listitem');
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringMatching(
        /^v1\.10Lantern Harbor \(v1\.10\)\.pkgLa más nueva$/,
      ),
      expect.stringMatching(/^v1\.05Lantern Harbor \(v1\.05\)\.pkg/),
      expect.stringMatching(/Lantern Harbor \(USA\)\.pkgJuego base$/),
    ]);
    // Only the newest is marked; the file keeps its full path as a tooltip.
    expect(within(sec).getAllByText('La más nueva')).toHaveLength(1);
    expect(within(sec).getByText('Lantern Harbor (v1.05).pkg')).toHaveAttribute(
      'title',
      'ROMS/PS4/Lantern Harbor/Updates/Lantern Harbor (v1.05).pkg',
    );
    // No title list and nothing outside the folder: no callout.
    expect(sec.querySelector('.panel-callout')).toBeNull();
  });

  it('says a game without updates shows the base game’s version', () => {
    const game = gameOf('Pebble League');
    const based: Game = {
      ...game,
      entry: { ...game.entry, version: '1.02' },
      installed: '1.02',
    };
    expect(hasVersionFacts(game, NO_FACTS)).toBe(false);
    expect(hasVersionFacts(based, NO_FACTS)).toBe(true);
    const sec = section(based);
    expect(within(sec).getByText('sin actualizaciones')).toBeInTheDocument();
    expect(
      within(sec).getByText(
        'la más nueva de tu biblioteca (la del juego base)',
      ),
    ).toBeInTheDocument();
    expect(within(sec).getAllByText('v1.02')).toHaveLength(2);
  });

  it('points to a newer update outside the game’s folder', async () => {
    const onShow = vi.fn();
    const tw = gameOf('Tidewalker');
    const sec = section(tw, NO_FACTS, onShow);
    const callout = sec.querySelector('.panel-callout')!;
    expect(callout).toHaveTextContent(
      'La v1.3.0 está en tu biblioteca, fuera de la carpeta del juego.',
    );
    expect(callout).toHaveTextContent('↑');
    await userEvent.click(
      within(sec).getByRole('button', { name: 'Ver dónde está' }),
    );
    expect(onShow).toHaveBeenCalledWith(tw.missing!.entry);
  });

  it('trusts the title list only as Rust read it', () => {
    const kd = gameOf('Kite Days');
    const ok = section(kd, {
      ...NO_FACTS,
      latest: { version: '1.1.0', source: 'title-db' },
      latest_installed: true,
    });
    expect(ok.querySelector('.panel-callout--ok')).toHaveTextContent(
      'Es la última que existe, según la lista de títulos.',
    );
  });

  it('shows a version the title list knows and the library lacks', () => {
    const sec = section(gameOf('Kite Days'), {
      ...NO_FACTS,
      latest: { version: '1.3.0', source: 'title-db' },
      latest_installed: false,
    });
    expect(sec.querySelector('.panel-callout')).toHaveTextContent(
      'Existe la v1.3.0 y no está en tu biblioteca. Dato de la lista de títulos.',
    );
    const ghost = sec.querySelector('.panel-row--missing')!;
    expect(ghost).toHaveTextContent('v1.3.0No está en la biblioteca↑Falta');
  });

  it('claims nothing when Rust could not compare', () => {
    const sec = section(gameOf('Kite Days'), {
      ...NO_FACTS,
      latest: { version: '1.3.0', source: 'title-db' },
    });
    expect(sec.querySelector('.panel-callout')).toBeNull();
    expect(sec.querySelector('.panel-row--missing')).toBeNull();
  });

  it('has nothing to say for a demo, an orphan or a game with only DLC', () => {
    expect(hasVersionFacts(gameOf('Lantern Harbor (Demo)'), NO_FACTS)).toBe(
      false,
    );
    expect(hasVersionFacts(gameOf('Neon Rhythm'), NO_FACTS)).toBe(false);
    expect(hasVersionFacts(gameOf('Tidewalker (v1.3.0)'), NO_FACTS)).toBe(
      false,
    );
  });
});
