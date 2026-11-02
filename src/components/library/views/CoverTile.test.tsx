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
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { ArtImage, EntryView, SystemView } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';
import { DEFAULT_SETTINGS } from '@/lib/settingsRules';
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
