import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { MOCK_ADDONS } from '@/ipc/mock/addonViewsFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { BackendError } from '@/ipc/errors';
import type { LibraryView } from '@/ipc/types';
import { buildGames, type Game } from '@/lib/games';

import { RevisionsSection } from './RevisionsSection';

let lib: LibraryView;
let star: (defaults?: Record<string, string>) => Game;

beforeEach(async () => {
  const backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
  lib = await backend.openLibrary(MOCK_ADDONS);
  star = (defaults = {}) => {
    const { byEntry } = buildGames(lib.entries, defaults);
    return byEntry.get(
      lib.entries.find((e) => e.name.startsWith('Star Courier'))!.id,
    )!;
  };
});

function section(game: Game, onUse: (id: number) => Promise<void>) {
  const view = render(
    <I18nProvider locale="es">
      <RevisionsSection game={game} onUse={onUse} />
    </I18nProvider>,
  );
  return {
    sec: () => screen.getByRole('region', { name: /^Revisiones/ }),
    rerender: (g: Game) =>
      view.rerender(
        <I18nProvider locale="es">
          <RevisionsSection game={g} onUse={onUse} />
        </I18nProvider>,
      ),
  };
}

describe('RevisionsSection (addon views §8.8 3, D7)', () => {
  it('lists the files of one game with the default pressed', () => {
    const { sec } = section(star(), vi.fn());
    expect(sec()).toHaveAttribute('id', 'panel-revisions');
    expect(within(sec()).getByText('2 archivos, un juego')).toBeInTheDocument();
    // Highest revision first, and it is the default.
    const pressed = within(sec()).getByRole('button', {
      name: 'Predeterminada',
    });
    expect(pressed).toHaveAttribute('aria-pressed', 'true');
    expect(pressed.closest('li')).toHaveTextContent('Rev 1');
    const use = within(sec()).getByRole('button', {
      name: 'Usar Original como predeterminada',
    });
    expect(use).toHaveAttribute('aria-pressed', 'false');
    expect(use).toHaveTextContent('Usar esta');
    expect(
      within(sec()).getByText(
        'La predeterminada es la que se ve en la biblioteca y la que se copiará al dispositivo cuando llegue la sincronización.',
      ),
    ).toBeInTheDocument();
  });

  it('makes another file the default and says so politely', async () => {
    const game = star();
    const original = game.revisions.find((e) => e.revision === undefined)!;
    const onUse = vi.fn(() => Promise.resolve());
    const { sec, rerender } = section(game, onUse);
    const rows = within(sec()).getAllByRole('listitem');
    await userEvent.click(
      within(sec()).getByRole('button', {
        name: 'Usar Original como predeterminada',
      }),
    );
    expect(onUse).toHaveBeenCalledWith(original.id);
    await waitFor(() =>
      expect(sec().querySelector('[aria-live="polite"]')).toHaveTextContent(
        'Original es ahora la predeterminada.',
      ),
    );
    // The library store rebuilds the game around the new default; the
    // rows keep their place.
    rerender(star({ [original.version_group!]: original.rel_path }));
    expect(within(sec()).getAllByRole('listitem')).toHaveLength(rows.length);
    expect(
      within(sec())
        .getAllByRole('listitem')[1]!
        .querySelector('[aria-pressed="true"]'),
    ).toHaveTextContent('Predeterminada');
    expect(
      within(sec()).getByRole('button', {
        name: 'Usar Rev 1 como predeterminada',
      }),
    ).toHaveAttribute('aria-pressed', 'false');
  });

  it('keeps the previous default when the choice cannot be saved', async () => {
    const onUse = vi.fn(() =>
      Promise.reject(new BackendError('io', 'settings.json: disk full')),
    );
    const { sec } = section(star(), onUse);
    await userEvent.click(
      within(sec()).getByRole('button', {
        name: 'Usar Original como predeterminada',
      }),
    );
    await waitFor(() =>
      expect(within(sec()).getByRole('alert')).toBeInTheDocument(),
    );
    expect(
      within(sec())
        .getByRole('button', { name: 'Predeterminada' })
        .closest('li'),
    ).toHaveTextContent('Rev 1');
    expect(sec().querySelector('[aria-live="polite"]')).toHaveTextContent('');
  });

  it('leaves a failure and its message on the game it belongs to', async () => {
    const onUse = vi.fn(() =>
      Promise.reject(new BackendError('io', 'settings.json: disk full')),
    );
    const { sec, rerender } = section(star(), onUse);
    await userEvent.click(
      within(sec()).getByRole('button', {
        name: 'Usar Original como predeterminada',
      }),
    );
    await waitFor(() =>
      expect(within(sec()).getByRole('alert')).toBeInTheDocument(),
    );
    // The panel moves on to another game with revisions: same component,
    // another version group.
    const other = star();
    const moved = (e: (typeof other.revisions)[number]) => ({
      ...e,
      id: e.id + 1000,
      name: e.name.replace('Star Courier', 'Comet Relay'),
      rel_path: e.rel_path.replace('Star Courier', 'Comet Relay'),
      version_group: 'comet relay',
    });
    const revisions = other.revisions.map(moved);
    rerender({
      ...other,
      id: revisions[0]!.id,
      entry: revisions[0]!,
      revisions,
    });
    expect(within(sec()).queryByRole('alert')).toBeNull();
    expect(sec().querySelector('[aria-live="polite"]')).toHaveTextContent('');
  });

  it('does nothing when the default is pressed again', async () => {
    const onUse = vi.fn(() => Promise.resolve());
    const { sec } = section(star(), onUse);
    await userEvent.click(
      within(sec()).getByRole('button', { name: 'Predeterminada' }),
    );
    expect(onUse).not.toHaveBeenCalled();
  });
});
