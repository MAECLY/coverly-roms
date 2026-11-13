import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { addonViewsLibrary } from '@/ipc/mock/addonViewsFixture';
import { generateLibrary } from '@/ipc/mock/fixture';
import { buildGames } from '@/lib/games';
import { useViewStore } from '@/stores/viewStore';

import { Sidebar } from './Sidebar';

const NOW = 1_760_000_000;
const view = generateLibrary({
  seed: 7,
  count: 240,
  root: '/mock/Small library',
  generation: 1,
  now: NOW,
});
const shown = view.systems.filter((s) => s.roms > 0);
const plain = buildGames(view.entries, {});
const addons = addonViewsLibrary(1, NOW);
const folded = buildGames(addons.entries, {});

function sidebar(locale: 'en' | 'es' = 'en', lib = view, built = plain) {
  return render(
    <I18nProvider locale={locale}>
      <Sidebar
        view={lib}
        games={built.games}
        orphans={built.orphans.length}
        now={NOW}
      />
    </I18nProvider>,
  );
}

describe('Sidebar', () => {
  afterEach(() => useViewStore.getState().reset());

  it('shows each system icon beside its name in the Systems tab', async () => {
    sidebar();
    await userEvent.click(screen.getByRole('tab', { name: 'Systems' }));
    const panel = screen.getByRole('tabpanel', { name: 'Systems' });
    const buttons = within(panel).getAllByRole('button');
    expect(shown.length).toBeGreaterThan(1);
    expect(buttons).toHaveLength(shown.length);
    shown.forEach((s, i) => {
      const button = buttons[i]!;
      const icon = button.querySelector(`[data-system-icon="${s.system_id}"]`);
      expect(icon).not.toBeNull();
      expect(icon).toHaveAttribute('aria-hidden', 'true');
      expect(button).toHaveAccessibleName(
        expect.stringMatching(new RegExp(`^${escape(s.name)}`)),
      );
    });
  });

  it('keeps folder and date buttons free of system icons', () => {
    const { container } = sidebar();
    expect(container.querySelector('[data-system-icon]')).toBeNull();
  });

  it('counts games under «Todos los juegos», never revisions or orphans', async () => {
    sidebar('es', addons, folded);
    await userEvent.click(screen.getByRole('tab', { name: 'Todo' }));
    const all = screen.getByRole('button', { name: /^Todos los juegos/ });
    expect(all).toHaveAccessibleName(
      new RegExp(`^Todos los juegos\\s*${String(folded.games.length)}$`),
    );
    expect(folded.games.length).toBeLessThan(addons.entries.length);
  });

  it('shows «Complementos sin juego base» with its count only when there are orphans', async () => {
    expect(folded.orphans.length).toBeGreaterThan(0);
    const { unmount } = sidebar('es', addons, folded);
    const entry = screen.getByRole('button', {
      name: new RegExp(
        `^Complementos sin juego base\\s*${String(folded.orphans.length)}$`,
      ),
    });
    await userEvent.click(entry);
    expect(useViewStore.getState().scope).toEqual({ kind: 'orphans' });
    expect(entry).toHaveAttribute('aria-current', 'true');
    unmount();
    sidebar('es');
    expect(
      screen.queryByRole('button', { name: /Complementos sin juego base/ }),
    ).toBeNull();
  });
});

function escape(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
