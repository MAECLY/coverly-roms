import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Root } from '@/Root';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import { CREDIT_GROUPS, CREDITS } from '@/lib/credits';
import { resetLibraryStore } from '@/stores/libraryStore';
import { resetNavStore, useNavStore } from '@/stores/navStore';
import { resetSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

let backend: MockBackend;
let restore: () => void;

async function openCredits(language?: 'es' | 'en') {
  if (language) await backend.updateSettings({ general: { language } });
  render(<Root />);
  await screen.findByRole('button', {
    name: language === 'es' ? 'Configuración' : 'Settings',
  });
  act(() => useNavStore.getState().openSettings('credits'));
  return screen.findByRole('tabpanel');
}

const groupNames = (panel: HTMLElement) =>
  within(panel)
    .getAllByRole('heading', { level: 3 })
    .map((h) => h.textContent);

const RETROARCH_EN =
  'System icons adapted from RetroArch Assets (XMB Monochrome theme), © libretro contributors. CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/), without warranties. Original: https://github.com/libretro/retroarch-assets. Changes: recoloured, optimised with SVGO and cropped.';

describe('CreditsSection', () => {
  beforeEach(() => {
    backend = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: new MemoryStorage(),
    });
    setBackend(backend);
    resetSettingsStore();
    resetLibraryStore();
    resetNavStore();
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    vi.restoreAllMocks();
    setBackend(null);
  });

  it('shows the groups in order, each with its count', async () => {
    const panel = await openCredits();
    const counts = CREDIT_GROUPS.map(
      (g) => CREDITS.filter((c) => c.group === g).length,
    );
    expect(groupNames(panel)).toEqual([
      `Cover and metadata sources ${counts[0]}`,
      `Databases and DATs ${counts[1]}`,
      `Technical docs and wikis ${counts[2]}`,
      `Frontend and firmware conventions ${counts[3]}`,
      `Libraries ${counts[4]}`,
      `Fonts and icons ${counts[5]}`,
    ]);
    expect(within(panel).getAllByRole('article')).toHaveLength(CREDITS.length);
    // The name and the count read as two words, not «Libraries12».
    expect(
      within(panel).getByRole('heading', {
        level: 3,
        name: `Libraries ${counts[4]}`,
      }),
    ).toBeVisible();
  });

  it('filters as you type and announces the count', async () => {
    const panel = await openCredits();
    const field = within(panel).getByRole('searchbox', {
      name: 'Search credits',
    });
    await userEvent.type(field, 'retroarch');
    const articles = within(panel).getAllByRole('article');
    expect(articles.map((a) => a.id)).toContain('credit-retroarch-assets');
    const count = within(panel).getByTestId('credits-count');
    expect(count).toHaveAttribute('aria-live', 'polite');
    expect(count).toHaveTextContent(`${articles.length} of ${CREDITS.length}`);
    expect(count.querySelectorAll('.mono')).toHaveLength(2);
  });

  it('says when nothing matches', async () => {
    const panel = await openCredits();
    await userEvent.type(
      within(panel).getByRole('searchbox', { name: 'Search credits' }),
      'zzzz',
    );
    expect(within(panel).queryAllByRole('article')).toHaveLength(0);
    expect(within(panel).getByText('Nothing matches “zzzz”.')).toBeVisible();
  });

  it('opens links through openCredit by id, never a URL', async () => {
    const spy = vi.spyOn(backend, 'openCredit');
    const panel = await openCredits();
    const article = within(panel)
      .getAllByRole('article')
      .find((a) => a.id === 'credit-screenscraper')!;
    const site = within(article).getByRole('button', {
      name: 'ScreenScraper official site',
    });
    expect(site).toHaveAccessibleDescription('Opens in your browser');
    await userEvent.click(site);
    await userEvent.click(
      within(article).getByRole('button', { name: /ScreenScraper licence/ }),
    );
    expect(spy.mock.calls).toEqual([
      ['screenscraper', 'site'],
      ['screenscraper', 'license'],
    ]);
  });

  it('shows the RetroArch Assets notice verbatim as a quotation', async () => {
    const panel = await openCredits();
    const article = within(panel)
      .getAllByRole('article')
      .find((a) => a.id === 'credit-retroarch-assets')!;
    const quote = article.querySelector('blockquote');
    expect(quote?.textContent).toBe(RETROARCH_EN);
  });

  it('shows no link for an entry without a url', async () => {
    const panel = await openCredits();
    const article = within(panel)
      .getAllByRole('article')
      .find((a) => a.id === 'credit-dolphin')!;
    expect(
      within(article).queryByRole('button', { name: /official site/ }),
    ).toBeNull();
  });

  it('is in Spanish when the app is', async () => {
    const panel = await openCredits('es');
    expect(groupNames(panel)[0]).toMatch(
      /^Fuentes de portadas y metadatos \d+$/,
    );
    expect(
      within(panel).getByRole('searchbox', {
        name: 'Buscar en agradecimientos',
      }),
    ).toBeVisible();
    expect(
      within(panel).getByText(
        'Coverly no incluye ROMs, BIOS, claves ni arte de juegos.',
      ),
    ).toBeVisible();
    expect(
      within(panel).getByRole('button', {
        name: 'Web oficial de ScreenScraper',
      }),
    ).toHaveAccessibleDescription('Se abre en el navegador');
  });

  it('the settings search lands on a credit and focuses its article', async () => {
    render(<Root />);
    await screen.findByRole('button', { name: 'Settings' });
    act(() => useNavStore.getState().openSettings('general'));
    await screen.findByRole('tabpanel');
    await userEvent.keyboard('{Control>}f{/Control}');
    await userEvent.keyboard('ScreenScraper');
    const option = await screen.findByRole('option', {
      name: /ScreenScraper.*Credits/,
    });
    await userEvent.click(option);
    await waitFor(() =>
      expect(document.activeElement?.id).toBe('credit-screenscraper'),
    );
    expect(document.activeElement?.tagName).toBe('ARTICLE');
  });
  it('opens the bundled third-party licences and returns focus', async () => {
    const panel = await openCredits();
    const button = within(panel).getByRole('button', {
      name: 'View third-party licences',
    });
    await userEvent.click(button);
    const dialog = await screen.findByRole('dialog', {
      name: 'Third-party licences',
    });
    // The real generated files: crates and npm packages, with counts.
    expect(
      await within(dialog).findByRole('tab', { name: /^Rust \(\d+\)$/ }),
    ).toBeVisible();
    expect(
      await within(dialog).findByRole('tab', {
        name: /^JavaScript \(\d+\)$/,
      }),
    ).toBeVisible();
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(button).toHaveFocus();
  });
});
