import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { MOCK_ADDONS } from '@/ipc/mock/addonViewsFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { EntryView, LibraryView } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';
import { buildGames, type Game } from '@/lib/games';

import { DemoSection } from './DemoSection';

let lib: LibraryView;
let gameOf: (prefix: string) => Game;

beforeEach(async () => {
  const backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
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

function section(
  game: Game,
  entries: readonly EntryView[] = lib.entries,
  onSelect = vi.fn(),
) {
  const { container } = render(
    <I18nProvider locale="es">
      <DemoSection
        generation={lib.generation}
        game={game}
        entries={entries}
        systemId="ps4"
        onSelect={onSelect}
      />
    </I18nProvider>,
  );
  return { container, onSelect };
}

describe('DemoSection (addon views §8.8 5, D8)', () => {
  it('shows the related demo as a card that selects it', async () => {
    const lh = gameOf('Lantern Harbor (USA)');
    const demo = lib.entries.find((e) => e.name === 'Lantern Harbor (Demo)')!;
    const { onSelect } = section(lh);
    const sec = screen.getByRole('region', { name: 'Demo relacionada' });
    expect(sec).toHaveAttribute('id', 'panel-demo');
    const card = within(sec).getByRole('button', {
      name: /Lantern Harbor \(Demo\)/,
    });
    expect(card).toHaveTextContent(/Juego aparte, 2 GB$/);
    await userEvent.click(card);
    expect(onSelect).toHaveBeenCalledWith(demo.id);
  });

  it('renders nothing for a game without a demo', () => {
    const { container } = section(gameOf('Neon Rhythm'));
    expect(container).toBeEmptyDOMElement();
  });

  it('opens the full game from its demo, with the demo note', async () => {
    const lh = lib.entries.find((e) => e.name === 'Lantern Harbor (USA)')!;
    const { onSelect } = section(gameOf('Lantern Harbor (Demo)'));
    const sec = screen.getByRole('region', { name: 'Demo' });
    const card = within(sec).getByRole('button', {
      name: /Abrir el juego completo/,
    });
    expect(card).toHaveTextContent('Lantern Harbor (USA), en tu biblioteca');
    await userEvent.click(card);
    expect(onSelect).toHaveBeenCalledWith(lh.id);
    expect(
      within(sec).getByText(
        'La demo es un juego aparte con su propia carpeta y su propia portada. Sus partidas no sirven para el juego completo.',
      ),
    ).toBeInTheDocument();
    // The demo has its own cover: none borrowed.
    expect(within(sec).queryByText('del juego completo')).toBeNull();
  });

  it('says when the full game is not in the library', () => {
    section(gameOf('Paper Skies (Demo)'));
    const sec = screen.getByRole('region', { name: 'Demo' });
    expect(
      within(sec).getByText(
        'Es una demo de Paper Skies. El juego completo no está en tu biblioteca.',
      ),
    ).toBeInTheDocument();
    expect(within(sec).queryByRole('button')).toBeNull();
  });

  it('shows the full game’s cover dimmed for a demo without one', () => {
    const entries = lib.entries.map((e) =>
      e.name === 'Lantern Harbor (Demo)'
        ? { ...e, art: { ...e.art, cover: 'missing' as const } }
        : e,
    );
    const { byEntry } = buildGames(entries, {});
    const demo = byEntry.get(
      entries.find((e) => e.name === 'Lantern Harbor (Demo)')!.id,
    )!;
    section(demo, entries);
    const sec = screen.getByRole('region', { name: 'Demo' });
    const figure = within(sec).getByRole('figure');
    expect(figure).toHaveTextContent('del juego completo');
    expect(figure.querySelector('.panel-borrowed__art')).not.toBeNull();
  });
});
