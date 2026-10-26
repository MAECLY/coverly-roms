import { describe, expect, it } from 'vitest';

import {
  DEFAULT_SETTINGS,
  cloneSettings,
  leafPaths,
  mergePatch,
  needsReopen,
  normaliseSettings,
  tolerantSettings,
  validateSettings,
} from './settingsRules';

const known = ['anbernic-stock', 'onion', 'esde', 'batocera', 'retroarch'];
const codes = (s = DEFAULT_SETTINGS) =>
  validateSettings(s, known).map((e) => `${e.field}:${e.code}`);

describe('settings rules (mirror of coverly_core::settings)', () => {
  it('defaults are valid', () => {
    expect(codes()).toEqual([]);
  });

  it('reports ranges with their bounds', () => {
    const s = cloneSettings(DEFAULT_SETTINGS);
    s.network.timeout_secs = 1;
    const e = validateSettings(s, known).find(
      (x) => x.field === 'network.timeout_secs',
    );
    expect(e).toEqual({
      field: 'network.timeout_secs',
      code: 'range',
      min: 5,
      max: 300,
    });
  });

  it('checks lists, order, proxy, locked backups and folders', () => {
    const s = cloneSettings(DEFAULT_SETTINGS);
    s.matching.review_threshold = 0.95;
    s.matching.regions = [];
    s.art.media = [];
    s.network.proxy = 'manual';
    s.safety.backup_before_overwrite = false;
    s.safety.backup_dir = 'relative';
    s.library.default_profile = 'ghost';
    expect(codes(s)).toEqual(
      expect.arrayContaining([
        'matching.review_threshold:order',
        'matching.regions:required',
        'art.media:required',
        'network.proxy_url:required',
        'safety.backup_before_overwrite:invalid',
        'safety.backup_dir:not-absolute',
        'library.default_profile:unknown',
      ]),
    );
  });

  it('checks the organize group like Rust', () => {
    const s = cloneSettings(DEFAULT_SETTINGS);
    s.organize.auto_threshold = 0.4;
    s.organize.review_threshold = 0.6;
    s.organize.recent_seconds = 4000;
    s.organize.default_layout = 'nope';
    s.organize.folder_names = { ghost: { gba: 'GBA' } };
    s.organize.arcade_order = { 'anbernic-stock': ['mame', 'mame'] };
    expect(codes(s)).toEqual([
      'organize.default_layout:unknown',
      'organize.review_threshold:order',
      'organize.recent_seconds:range',
      'organize.folder_names.ghost:unknown',
      'organize.arcade_order.anbernic-stock:invalid',
    ]);
    // Built-in layouts are known even when the caller lists none.
    expect(validateSettings(DEFAULT_SETTINGS, [])).toEqual([]);
    const fixed = tolerantSettings(
      { organize: { auto_threshold: 0.4, review_threshold: 0.6 } },
      known,
    );
    expect(fixed.settings.organize.auto_threshold).toBe(0.85);
    expect(fixed.settings.organize.review_threshold).toBe(0.5);
    // Like Rust, only the unknown layout's entry is dropped.
    const keep = tolerantSettings(
      {
        organize: {
          folder_names: { ghost: { gba: 'GBA' }, esde: { gba: 'gba' } },
          arcade_order: { ghost: ['mame'], esde: ['fbneo'] },
        },
      },
      known,
    );
    expect(keep.settings.organize.folder_names).toEqual({
      esde: { gba: 'gba' },
    });
    expect(keep.settings.organize.arcade_order).toEqual({ esde: ['fbneo'] });
    expect(keep.fixed).toEqual([
      'organize.folder_names',
      'organize.arcade_order',
    ]);
    const n = normaliseSettings({
      ...cloneSettings(DEFAULT_SETTINGS),
      organize: {
        ...DEFAULT_SETTINGS.organize,
        companion_exts: [' SAV ', '', '.Srm', 'sav'],
      },
    });
    expect(n.organize.companion_exts).toEqual(['sav', 'srm']);
  });

  it('merges patches like RFC 7386', () => {
    const next = mergePatch(DEFAULT_SETTINGS, {
      network: { timeout_secs: 60 },
      safety: { backup_dir: null },
    });
    expect(next.network.timeout_secs).toBe(60);
    expect(next.network.retries).toBe(3);
    expect(DEFAULT_SETTINGS.network.timeout_secs).toBe(30);
    expect(
      leafPaths({ network: { timeout_secs: 1 }, art: { media: ['cover'] } }),
    ).toEqual(['network.timeout_secs', 'art.media']);
  });

  it('normalises lists and keeps unavailable sources off', () => {
    const s = cloneSettings(DEFAULT_SETTINGS);
    s.library.ignored_exts = ['.PNG', 'png', ' txt '];
    s.sources.order = [
      { id: 'screenscraper', enabled: true },
      { id: 'steamgriddb', enabled: true },
      { id: 'libretro', enabled: true },
    ];
    const n = normaliseSettings(s);
    expect(n.library.ignored_exts).toEqual(['png', 'txt']);
    // SteamGridDB ships (M6); ScreenScraper is not in this build.
    expect(n.sources.order).toEqual([
      { id: 'screenscraper', enabled: false },
      { id: 'steamgriddb', enabled: true },
      { id: 'libretro', enabled: true },
    ]);
  });

  it('reads stored settings field by field', () => {
    const { settings, fixed } = tolerantSettings(
      {
        general: { theme: 'dark' },
        network: { timeout_secs: 'soon', concurrency: 99 },
        junk: 1,
      },
      known,
    );
    expect(settings.general.theme).toBe('dark');
    expect(settings.network.timeout_secs).toBe(30);
    expect(settings.network.concurrency).toBe(4);
    expect(fixed).toEqual(['network.timeout_secs', 'network.concurrency']);
    expect(tolerantSettings('garbage', known).settings).toEqual(
      DEFAULT_SETTINGS,
    );
  });

  it('reads organize.external_tools like Rust', () => {
    // An old file without the field loads `auto` with nothing fixed.
    const old = tolerantSettings(
      { organize: { mode: 'copy', recent_seconds: 30 } },
      known,
    );
    expect(old.settings.organize.external_tools).toBe('auto');
    expect(old.settings.organize.mode).toBe('copy');
    expect(old.fixed).toEqual([]);
    // `off` round-trips; an unknown value resets only that field.
    const off = tolerantSettings(
      { organize: { external_tools: 'off' } },
      known,
    );
    expect(off.settings.organize.external_tools).toBe('off');
    expect(off.fixed).toEqual([]);
    const bad = tolerantSettings(
      { organize: { external_tools: 'sometimes', mode: 'copy' } },
      known,
    );
    expect(bad.settings.organize.external_tools).toBe('auto');
    expect(bad.settings.organize.mode).toBe('copy');
    expect(bad.fixed).toEqual(['organize.external_tools']);
  });

  it('reads and normalises the dedupe group like Rust', () => {
    const { settings, fixed } = tolerantSettings(
      {
        dedupe: {
          one_game_one_rom: true,
          action: 'shred',
          exclude_systems: [' Ports ', 'ports', 'BIOS'],
          keep_rules: [{ rule: 'cover', on: false }],
        },
      },
      known,
    );
    expect(fixed).toEqual(['dedupe.action']);
    expect(settings.dedupe.one_game_one_rom).toBe(true);
    expect(settings.dedupe.action).toBe('quarantine');
    expect(settings.dedupe.exclude_systems).toEqual(['ports', 'bios']);
    const rules = settings.dedupe.keep_rules;
    expect(rules).toHaveLength(10);
    expect(rules[0]).toEqual({ rule: 'cover', on: false });
    expect(rules[1]).toEqual({ rule: 'has_save', on: true });
    expect(tolerantSettings({ version: 2 }, known).settings.dedupe).toEqual(
      DEFAULT_SETTINGS.dedupe,
    );
  });

  it('settings_rejects_quarantine_inside_a_system_folder', () => {
    const folders = ['GB', 'gba', 'SFC'];
    const at = (dir: string | null) => {
      const s = cloneSettings(DEFAULT_SETTINGS);
      s.dedupe.quarantine_dir = dir;
      return validateSettings(s, known, folders).map(
        (e) => `${e.field}:${e.code}`,
      );
    };
    expect(at(null)).toEqual([]);
    expect(at('/Users/me/Duplicados')).toEqual([]);
    // A card named ROMS or GB is a mount label, not a ROMs folder.
    expect(at('/Volumes/ROMS/_Duplicados')).toEqual([]);
    expect(at('/Volumes/GB/_Duplicados')).toEqual([]);
    expect(at('/Volumes/SD/ROMS/GB/_Duplicados')).toEqual([
      'dedupe.quarantine_dir:invalid',
    ]);
    expect(at('/Volumes/SD/Roms/_Duplicados')).toEqual([
      'dedupe.quarantine_dir:invalid',
    ]);
    expect(at('/Users/me/ROMs/gb/x')).toEqual([
      'dedupe.quarantine_dir:invalid',
    ]);
    expect(at('D:\\ROMS\\SFC\\dup')).toEqual(['dedupe.quarantine_dir:invalid']);
    expect(at('relative/dup')).toEqual(['dedupe.quarantine_dir:not-absolute']);
  });

  it('checks_the_dedupe_group_like_rust', () => {
    const s = cloneSettings(DEFAULT_SETTINGS);
    s.dedupe.levels = [];
    s.dedupe.keep_rules = [
      { rule: 'cover', on: true },
      { rule: 'cover', on: false },
    ];
    s.dedupe.quarantine_name = 'a/b';
    s.dedupe.exclude_systems = ['ports/x'];
    expect(codes(s)).toEqual([
      'dedupe.levels:required',
      'dedupe.keep_rules:duplicate',
      'dedupe.quarantine_name:invalid',
      'dedupe.exclude_systems:invalid',
    ]);
    s.dedupe.levels = ['exact', 'exact'];
    expect(codes(s)[0]).toBe('dedupe.levels:duplicate');
  });

  it('knows which changes need a reopen', () => {
    const b = cloneSettings(DEFAULT_SETTINGS);
    b.general.theme = 'dark';
    expect(needsReopen(DEFAULT_SETTINGS, b)).toBe(false);
    b.library.include_hidden = true;
    expect(needsReopen(DEFAULT_SETTINGS, b)).toBe(true);
  });
});
