import { render, screen, waitFor } from '@testing-library/react';
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
  type Mock,
} from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { addonViewsLibrary } from '@/ipc/mock/addonViewsFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { ArtImage, EntryView, SystemView } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';
import { buildGames, type Game } from '@/lib/games';
import { DEFAULT_SETTINGS } from '@/lib/settingsRules';
import type { CoverSize } from '@/lib/viewPrefs';
import { useSettingsStore } from '@/stores/settingsStore';

import { CoverTile } from './CoverTile';

const SWITCH: SystemView = {
  index: 0,
  folder: 'switch',
  dir: '/lib/switch',
  system_id: 'switch',
  name: 'Nintendo Switch',
  roms: 1,
};

function entry(name: string, cover: 'present' | 'missing' = 'missing') {
  return {
    id: 7,
    system: 0,
    rel_path: `switch/${name}`,
    dir: 'switch',
    name: name.replace(/\.nsp$/, ''),
    ext: 'nsp',
    size: 1,
    mtime: 0,
    regions: [],
    art: { cover, screenshot: 'missing', title: 'missing' },
  } as unknown as EntryView;
}

function icons(on: boolean) {
  const s = structuredClone(DEFAULT_SETTINGS);
  s.catalogs.switch.enabled = on;
  s.catalogs.switch.icons = on;
  useSettingsStore.setState({ settings: s });
}

function tile(e: EntryView, system: SystemView = SWITCH) {
  return render(
    <I18nProvider locale="es">
      <CoverTile
        entry={e}
        system={system}
        generation={1}
        art="cover"
        size="m"
        selected={false}
      />
    </I18nProvider>,
  );
}

describe('CoverTile: Switch catalogue icon (adjustments D8)', () => {
  let catalogIcon: Mock<(tid: string) => Promise<ArtImage>>;
  beforeEach(() => {
    globalThis.URL.createObjectURL = vi.fn(() => 'blob:icon');
    globalThis.URL.revokeObjectURL = vi.fn();
    const b = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    catalogIcon = vi.fn(async (_tid: string) => ({
      bytes: new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      mime: 'image/png',
    }));
    b.catalogIcon = catalogIcon;
    setBackend(b);
  });
  afterEach(() => {
    setBackend(null);
    clearArtCache();
    useSettingsStore.setState({ settings: null });
  });

  it('shows the cached eShop icon of a Switch game with no cover while icons are on', async () => {
    icons(true);
    const { container } = tile(entry('Zelda [01007ef00011e000][v0].nsp'));
    await waitFor(() =>
      expect(container.querySelector('img.art-thumb')).not.toBeNull(),
    );
    expect(catalogIcon).toHaveBeenCalledWith('01007EF00011E000');
    // Decorative: the tile is already named by the game.
    expect(container.querySelector('img.art-thumb')!.getAttribute('alt')).toBe(
      '',
    );
  });

  it('asks nothing while icons are off, for another system or without a title id', async () => {
    icons(false);
    tile(entry('Zelda [01007EF00011E000][v0].nsp')).unmount();
    icons(true);
    tile(entry('Zelda [01007EF00011E000][v0].nsp'), {
      ...SWITCH,
      system_id: 'wiiu',
    });
    tile(entry('Zelda.nsp'));
    await new Promise((r) => setTimeout(r, 10));
    expect(catalogIcon).not.toHaveBeenCalled();
    expect(screen.getAllByText('Sin portada')).toHaveLength(2);
  });

  it('never replaces a cover the library already has', async () => {
    icons(true);
    tile(entry('Zelda [01007EF00011E000][v0].nsp', 'present'));
    await new Promise((r) => setTimeout(r, 10));
    expect(catalogIcon).not.toHaveBeenCalled();
  });

  it('keeps the empty slot when the title has no icon', async () => {
    icons(true);
    catalogIcon.mockRejectedValue(new Error('art-missing'));
    tile(entry('Zelda [01007EF00011E000][v0].nsp'));
    await waitFor(() => expect(catalogIcon).toHaveBeenCalled());
    expect(screen.getByText('Sin portada')).toBeTruthy();
  });
});

describe('CoverTile: add-on summary under the cover (addon views §8.2)', () => {
  const lib = addonViewsLibrary(1, FIXTURE_NOW);
  const { games } = buildGames(lib.entries, {});
  const named = (prefix: string): Game => {
    const g = games.find((x) => x.entry.name.startsWith(prefix));
    if (!g) throw new Error(prefix);
    return g;
  };

  beforeEach(() => {
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
  });
  afterEach(() => {
    setBackend(null);
    clearArtCache();
  });

  function gameTile(
    game: Game,
    size: CoverSize = 'l',
    opts: { isNew?: boolean } = {},
  ) {
    return render(
      <I18nProvider locale="es">
        <CoverTile
          entry={game.entry}
          system={lib.systems[game.entry.system]}
          generation={1}
          art="cover"
          size={size}
          selected={false}
          game={game}
          contentLine
          isNew={opts.isNew}
        />
      </I18nProvider>,
    );
  }

  const parts = (c: HTMLElement) =>
    [...c.querySelectorAll('.addon-chip__part')].map((p) => p.textContent);

  it('writes «+1 DLC · v1.10» under the art, never inside it', () => {
    const { container } = gameTile(named('Lantern Harbor (USA)'));
    const chip = container.querySelector('.addon-chip')!;
    expect(chip).not.toBeNull();
    expect(parts(container)).toEqual(['+1 DLC', 'v1.10']);
    expect(container.querySelector('.cover-tile__art .addon-chip')).toBeNull();
    expect(chip.closest('.cover-tile__content')).not.toBeNull();
    expect(chip.getAttribute('aria-hidden')).toBe('true');
  });

  it('warns «↑ falta v1.3.0» with the word, the arrow and the warning tone', () => {
    const { container } = gameTile(named('Tidewalker'));
    expect(parts(container)).toEqual(['+1 DLC', '↑ falta v1.3.0']);
    const warn = container.querySelector('.addon-chip__part--warning')!;
    expect(warn.textContent).toBe('↑ falta v1.3.0');
    expect(container.querySelector('.addon-chip--warning')).not.toBeNull();
  });

  it('drops +N DLC first on a narrow tile, never the warning', () => {
    const { container } = gameTile(named('Tidewalker'), 'm');
    expect(parts(container)).toEqual(['↑ falta v1.3.0']);
  });

  it('shows a dot at XS with the whole state for screen readers', () => {
    const { container } = gameTile(named('Lantern Harbor (USA)'), 'xs');
    expect(container.querySelector('.addon-chip')).toBeNull();
    const dot = container.querySelector('.cover-tile__art .cover-tile__dot')!;
    expect(dot.getAttribute('aria-hidden')).toBe('true');
    expect(dot.textContent).toBe('');
    const sr = container.querySelector('.sr-only')!;
    expect(sr.textContent).toMatch(
      /^Lantern Harbor \(USA\), Sony PlayStation 4\. 1 DLC/,
    );
  });

  it('turns the dot into an arrow when the newest update is missing', () => {
    const { container } = gameTile(named('Tidewalker'), 's');
    const dot = container.querySelector('.cover-tile__dot')!;
    expect(dot.classList.contains('cover-tile__dot--warning')).toBe(true);
    expect(dot.textContent).toBe('↑');
  });

  it('says «Demo de Lantern Harbor» on a demo, with no chip', () => {
    const { container } = gameTile(named('Lantern Harbor (Demo)'));
    expect(container.querySelector('.addon-chip')).toBeNull();
    expect(container.querySelector('.cover-tile__content')!.textContent).toBe(
      'Demo de Lantern Harbor',
    );
  });

  it('tags new content «Nuevo» on the same line', () => {
    const { container } = gameTile(named('Neon Rhythm'), 'l', { isNew: true });
    const tag = container.querySelector('.cover-tile__content .addon-new');
    expect(tag?.textContent).toBe('Nuevo');
    expect(container.querySelector('.sr-only')!.textContent).toContain(
      'contenido nuevo desde el último escaneo',
    );
  });

  it('keeps the visible name in the cell and adds no tab stop or aria-label', () => {
    const tw = named('Tidewalker');
    const { container } = gameTile(tw);
    const cell = screen.getByRole('gridcell');
    expect(cell.getAttribute('aria-label')).toBeNull();
    expect(container.querySelector('[tabindex]')).toBeNull();
    const phrase =
      'Tidewalker, Nintendo Switch. 1 DLC, 1 actualización, la más nueva v1.0.2, falta la última actualización, v1.3.0';
    expect(cell.getAttribute('title')).toBe(phrase);
    expect(container.querySelector('.sr-only')!.textContent).toBe(phrase);
    expect(cell.textContent).toContain('Tidewalker');
  });

  it('keeps a plain game as it is today', () => {
    const { container } = gameTile(named('Pebble League'));
    expect(container.querySelector('.addon-chip')).toBeNull();
    expect(container.querySelector('.cover-tile__dot')).toBeNull();
    expect(screen.getByRole('gridcell').getAttribute('title')).toBe(
      named('Pebble League').entry.name,
    );
  });
});
