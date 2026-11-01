import { describe, expect, it } from 'vitest';

import { DEFAULT_SETTINGS } from '@/lib/settingsRules';
import { MemoryStorage } from '@/test/storage';

import { BackendError } from '../errors';
import { FIXTURE_NOW } from './fixture';
import { createMockBackend } from './mockBackend';

const make = (storage: Storage | null = new MemoryStorage()) =>
  createMockBackend({ delayMs: 0, now: FIXTURE_NOW, storage });

describe('mock settings', () => {
  it('starts with the defaults and persists valid updates across a reload', async () => {
    const storage = new MemoryStorage();
    const a = make(storage);
    expect((await a.getSettings()).settings).toEqual(DEFAULT_SETTINGS);
    const changed: string[] = [];
    await a.onSettingsChanged((s) => changed.push(s.general.theme));
    const out = await a.updateSettings({
      general: { theme: 'dark' },
      network: { timeout_secs: 45 },
    });
    expect(out.errors).toEqual([]);
    expect(changed).toEqual(['dark']);
    const b = make(storage);
    const again = (await b.getSettings()).settings;
    expect(again.general.theme).toBe('dark');
    expect(again.network.timeout_secs).toBe(45);
  });

  it('returns field errors and saves nothing for invalid input', async () => {
    const storage = new MemoryStorage();
    const m = make(storage);
    const out = await m.updateSettings({ network: { timeout_secs: 1 } });
    expect(out.errors).toEqual([
      { field: 'network.timeout_secs', code: 'range', min: 5, max: 300 },
    ]);
    expect(out.settings.network.timeout_secs).toBe(30);
    expect(storage.getItem('coverly.mock.settings')).toBeNull();
  });

  it('falls back to defaults with a notice when storage holds garbage', async () => {
    const storage = new MemoryStorage();
    storage.setItem('coverly.mock.settings', '{nope');
    const view = await make(storage).getSettings();
    expect(view.settings).toEqual(DEFAULT_SETTINGS);
    expect(view.notice).toBe('reset-corrupt');
  });

  it('works without storage', async () => {
    const m = make(null);
    await m.updateSettings({ general: { language: 'es' } });
    expect((await m.getSettings()).settings.general.language).toBe('es');
  });

  it('resets to defaults', async () => {
    const m = make();
    await m.updateSettings({ art: { jpeg_quality: 70 } });
    expect((await m.resetSettings()).settings.art.jpeg_quality).toBe(90);
  });

  it('external_tools_found: none by default, like the organise mock', async () => {
    const m = make();
    expect((await m.getSettings()).external_tools_found).toEqual([]);
    expect((await m.resetSettings()).external_tools_found).toEqual([]);
  });

  it('external_tools_found: lists the tools it was given, whatever the setting says', async () => {
    const found = [
      { tool: 'seven-zip', version: '24.08', path: '/opt/homebrew/bin/7zz' },
      { tool: 'bsdtar', version: '3.5.3', path: '/usr/bin/bsdtar' },
    ] as const;
    const m = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: new MemoryStorage(),
      externalTools: [...found],
    });
    const view = await m.getSettings();
    expect(view.external_tools_found).toEqual(found);
    // A copy: what the caller does with it never changes the mock.
    view.external_tools_found.pop();
    expect((await m.getSettings()).external_tools_found).toEqual(found);
    const off = await m.updateSettings({ organize: { external_tools: 'off' } });
    expect(off.errors).toEqual([]);
    expect(off.settings.organize.external_tools).toBe('off');
    expect((await m.getSettings()).external_tools_found).toEqual(found);
    expect((await m.resetSettings()).external_tools_found).toEqual(found);
  });

  it('reports SteamGridDB ready only with a key and tests it without keeping the value', async () => {
    const storage = new MemoryStorage();
    const m = make(storage);
    const info = await m.sourcesInfo();
    expect(info.map((i) => [i.id, i.built, i.ready])).toEqual([
      ['libretro', true, true],
      ['steamgriddb', true, false],
      ['screenscraper', false, false],
      ['embedded', true, true],
    ]);
    expect(info[1]!.needs).toEqual(['steamgriddb-api-key']);
    expect(await m.testSourceKey('steamgriddb')).toBe('missing');
    await m.setCredential('steamgriddb-api-key', 'bad-key');
    expect(await m.testSourceKey('steamgriddb')).toBe('invalid');
    await m.setCredential('steamgriddb-api-key', 'sk-good');
    expect(await m.testSourceKey('steamgriddb')).toBe('ok');
    expect((await m.sourcesInfo())[1]!.ready).toBe(true);
    for (let i = 0; i < storage.length; i++) {
      const v = storage.getItem(storage.key(i)!) ?? '';
      expect(v).not.toContain('sk-good');
      expect(v).not.toContain('bad-key');
    }
    await m.setCredential('steamgriddb-api-key', 'bad-again');
    await m.clearCredential('steamgriddb-api-key');
    expect(await m.testSourceKey('steamgriddb')).toBe('missing');
    expect(await m.testSourceKey('screenscraper')).toBe('unavailable');
    await expect(m.testSourceKey('libretro')).rejects.toMatchObject({
      code: 'bad-request',
    });
    await expect(m.testSourceKey('nope' as never)).rejects.toBeInstanceOf(
      BackendError,
    );
  });

  it('never stores secret values', async () => {
    const storage = new MemoryStorage();
    const m = make(storage);
    const st = await m.setCredential('steamgriddb-api-key', 'sk-SECRET');
    expect(st.set['steamgriddb-api-key']).toBe(true);
    for (let i = 0; i < storage.length; i++) {
      expect(storage.getItem(storage.key(i)!)).not.toContain('SECRET');
    }
    expect(
      (await make(storage).credentialStatus()).set['steamgriddb-api-key'],
    ).toBe(true);
    expect(
      (await m.clearCredential('steamgriddb-api-key')).set[
        'steamgriddb-api-key'
      ],
    ).toBe(false);
  });

  it('lists built-in profiles read-only and saves, renames and deletes custom ones', async () => {
    const m = make();
    const builtins = await m.listProfiles();
    expect(builtins.length).toBeGreaterThan(0);
    expect(builtins.every((p) => p.builtin)).toBe(true);
    const onion = builtins.find((p) => p.profile.id === 'onion')!;
    const copy = { ...onion.profile, id: 'my-onion', name: 'My Onion' };
    expect((await m.saveProfile(copy, null)).errors).toEqual([]);
    const renamed = await m.saveProfile(
      { ...copy, id: 'my-onion-2' },
      'my-onion',
    );
    expect(renamed.profiles.map((p) => p.profile.id)).toContain('my-onion-2');
    expect(renamed.profiles.map((p) => p.profile.id)).not.toContain('my-onion');
    const bad = await m.saveProfile({ ...copy, id: 'onion' }, null);
    expect(bad.errors.map((e) => e.field)).toContain('id');
    await expect(m.deleteProfile('onion')).rejects.toBeInstanceOf(BackendError);
    expect(
      (await m.deleteProfile('my-onion-2')).some(
        (p) => p.profile.id === 'my-onion-2',
      ),
    ).toBe(false);
  });

  it('previews a profile and records opened folders and links', async () => {
    const m = make();
    const p = (await m.listProfiles()).find(
      (x) => x.profile.id === 'onion',
    )!.profile;
    const prev = await m.previewProfile(p, {
      system_folder: 'GBA',
      rel_dir: '',
      file_name: 'Golden Sun.gba',
    });
    expect(prev.paths.cover).toBe('Roms/GBA/Imgs/Golden Sun.png');
    await m.openLink('license');
    await m.openLogsFolder();
    expect(m.opened).toEqual(['link:license', 'folder:logs']);
    expect(await m.cacheUsage()).toBeGreaterThan(0);
    expect(await m.clearCache()).toBeGreaterThan(0);
    expect(await m.cacheUsage()).toBe(0);
  });

  it('keeps what it understands from wrong-shaped or newer stored settings', async () => {
    const storage = new MemoryStorage();
    storage.setItem(
      'coverly.mock.settings',
      JSON.stringify({
        version: 9,
        general: { theme: 'neon', language: 'es' },
        network: 'fast',
        art: {
          jpeg_quality: 70.5,
          max_size: { ghost: { width: 100, height: 100 } },
        },
        future_group: { x: 1 },
      }),
    );
    const view = await make(storage).getSettings();
    expect(view.notice).toBe('future-version');
    expect(view.settings.general).toEqual({
      ...DEFAULT_SETTINGS.general,
      language: 'es',
    });
    expect(view.settings.network).toEqual(DEFAULT_SETTINGS.network);
    expect(view.settings.art.jpeg_quality).toBe(90);
    expect(view.settings.art.max_size).toEqual({});
    expect(view.fixed).toEqual(
      expect.arrayContaining([
        'general.theme',
        'network',
        'art.jpeg_quality',
        'art.max_size',
      ]),
    );
    expect(Object.keys(view.settings)).not.toContain('future_group');
  });

  it('refuses patches of the wrong shape without saving anything', async () => {
    const storage = new MemoryStorage();
    const m = make(storage);
    const bad = [
      [{ general: { theme: 'neon' } }, 'general.theme'],
      [{ network: { timeout_secs: 'soon' } }, 'network.timeout_secs'],
      [{ network: { retries: 2.5 } }, 'network.retries'],
      [{ network: 5 }, 'network'],
      [
        { art: { max_size: { onion: { width: 'x', height: 1 } } } },
        'art.max_size.onion',
      ],
    ] as const;
    for (const [patch, field] of bad) {
      const out = await m.updateSettings(patch as never);
      expect(out.errors.map((e) => `${e.field}:${e.code}`)).toEqual([
        `${field}:invalid`,
      ]);
    }
    expect((await m.updateSettings('nope' as never)).errors[0]?.code).toBe(
      'invalid',
    );
    expect(storage.getItem('coverly.mock.settings')).toBeNull();
  });

  it('treats null as the default and ignores unknown keys', async () => {
    const m = make();
    await m.updateSettings({
      network: { timeout_secs: 60 },
      art: { max_size: { onion: { width: 200, height: 300 } } },
    });
    const sized = await m.updateSettings({
      art: { max_size: { onion: null } },
    });
    expect(sized.errors).toEqual([]);
    expect(sized.settings.art.max_size).toEqual({});
    expect(sized.reopen).toBe(true);
    const out = await m.updateSettings({
      network: null,
      junk: { a: 1 },
    } as never);
    expect(out.settings.network.timeout_secs).toBe(30);
    expect(Object.keys(out.settings)).not.toContain('junk');
    const proto = JSON.parse(
      '{"__proto__": {"polluted": true}, "general": {"theme": "dark"}}',
    );
    expect((await m.updateSettings(proto)).settings.general.theme).toBe('dark');
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
  });

  it('skips hostile stored profiles and renames references with a profile', async () => {
    const storage = new MemoryStorage();
    storage.setItem(
      'coverly.mock.profiles',
      JSON.stringify([
        null,
        { id: 'x' },
        {
          id: 'evil',
          name: 'Evil',
          roms_root: '',
          art_dirs: [],
          gamelist: false,
          slots: {
            cover: {
              template: '{root}/../{stem}.png',
              format: 'png',
              max_width: null,
              max_height: null,
            },
          },
        },
      ]),
    );
    const m = make(storage);
    expect((await m.listProfiles()).every((p) => p.builtin)).toBe(true);
    const onion = (await m.listProfiles()).find(
      (p) => p.profile.id === 'onion',
    )!;
    await m.saveProfile({ ...onion.profile, id: 'mine', name: 'Mine' }, null);
    await m.updateSettings({
      library: { default_profile: 'mine' },
      art: { max_size: { mine: { width: 100, height: 100 } } },
    });
    await m.saveProfile({ ...onion.profile, id: 'ours', name: 'Ours' }, 'mine');
    let s = (await m.getSettings()).settings;
    expect(s.library.default_profile).toBe('ours');
    expect(s.art.max_size).toEqual({ ours: { width: 100, height: 100 } });
    await m.deleteProfile('ours');
    s = (await make(storage).getSettings()).settings;
    expect(s.library.default_profile).toBeNull();
    expect(s.art.max_size).toEqual({});
    await expect(
      m.saveProfile({ id: 'x' } as never, null),
    ).rejects.toMatchObject({ code: 'bad-request' });
  });

  it('refuses unknown credential keys and control characters', async () => {
    const m = make();
    await expect(m.setCredential('nope' as never, 'v')).rejects.toMatchObject({
      code: 'bad-request',
    });
    await expect(
      m.setCredential('screenscraper-user', 'a\u0000b'),
    ).rejects.toMatchObject({ code: 'bad-request' });
    expect(
      (await m.setCredential('screenscraper-user', '   ')).set[
        'screenscraper-user'
      ],
    ).toBe(false);
  });
});
