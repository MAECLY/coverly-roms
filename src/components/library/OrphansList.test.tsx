import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { Locale } from '@/i18n/core';
import { addonViewsLibrary } from '@/ipc/mock/addonViewsFixture';
import { buildGames } from '@/lib/games';
import { useSelectionStore } from '@/stores/selectionStore';
import { useViewStore } from '@/stores/viewStore';

import { OrphansList } from './OrphansList';

const view = addonViewsLibrary(1, 1_760_000_000);
const { orphans, byEntry } = buildGames(view.entries, {});
const linked = orphans.find((o) => o.entry.orphan_of !== undefined)!;
const alone = orphans.find((o) => o.entry.orphan_of === undefined)!;
const game = byEntry.get(linked.entry.orphan_of!)!;

function list(locale: Locale = 'es') {
  return render(
    <I18nProvider locale={locale}>
      <OrphansList orphans={orphans} systems={view.systems} byEntry={byEntry} />
    </I18nProvider>,
  );
}

function rowOf(name: string): HTMLElement {
  return screen.getByRole('row', { name: new RegExp(escape(name)) });
}

describe('OrphansList', () => {
  afterEach(() => {
    useViewStore.getState().reset();
    useSelectionStore.getState().select(null);
  });

  it('is a table named after the list, one row per orphan', () => {
    list();
    const table = screen.getByRole('table', {
      name: 'Complementos sin juego base',
    });
    // Header row plus one row per orphan.
    expect(within(table).getAllByRole('row')).toHaveLength(orphans.length + 1);
  });

  it('says what each one is, its version and its folder', () => {
    list();
    const row = rowOf(linked.entry.name);
    expect(within(row).getByText('Actualización')).toBeInTheDocument();
    const version = within(row).getByText('v1.3.0');
    expect(version.closest('.mono')).not.toBeNull();
    expect(within(row).getByText(linked.entry.dir)).toHaveClass('mono');
    expect(
      within(rowOf(alone.entry.name)).getByText('DLC'),
    ).toBeInTheDocument();
  });

  it('says whose it is, or that its game is not in the library', () => {
    list();
    expect(
      within(rowOf(linked.entry.name)).getByText(
        `Es de «${game.entry.name}»; está fuera de su carpeta.`,
      ),
    ).toBeInTheDocument();
    const lone = rowOf(alone.entry.name);
    expect(
      within(lone).getByText('Su juego no está en la biblioteca.'),
    ).toBeInTheDocument();
    expect(within(lone).queryByRole('button')).toBeNull();
  });

  it('«Ver el juego» selects the game and shows the whole library', async () => {
    useViewStore.getState().setScope({ kind: 'orphans' });
    list();
    const button = within(rowOf(linked.entry.name)).getByRole('button', {
      name: `Ver el juego «${game.entry.name}»`,
    });
    await userEvent.click(button);
    expect(useSelectionStore.getState().selectedId).toBe(game.id);
    expect(useViewStore.getState().scope).toEqual({ kind: 'all' });
  });

  it('names each «Ver el juego» after its game, starting with the visible words', () => {
    list();
    const button = within(rowOf(linked.entry.name)).getByRole('button', {
      name: `Ver el juego «${game.entry.name}»`,
    });
    // Visible label stays the short phrase; the name begins with it (WCAG 2.5.3).
    expect(button).toHaveTextContent(/^Ver el juego$/);
  });

  it('speaks English too', () => {
    list('en');
    expect(
      screen.getByRole('table', { name: 'Add-ons without their game' }),
    ).toBeInTheDocument();
    expect(
      within(rowOf(alone.entry.name)).getByText(
        'Its game is not in the library.',
      ),
    ).toBeInTheDocument();
    expect(
      within(rowOf(linked.entry.name)).getByRole('button', {
        name: `Show the game “${game.entry.name}”`,
      }),
    ).toBeInTheDocument();
  });
});

function escape(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
