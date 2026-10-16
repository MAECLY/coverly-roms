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
      { id: 'steamgriddb', enabled: true },
      { id: 'libretro', enabled: true },
    ];
    const n = normaliseSettings(s);
    expect(n.library.ignored_exts).toEqual(['png', 'txt']);
    expect(n.sources.order).toEqual([
      { id: 'steamgriddb', enabled: false },
      { id: 'libretro', enabled: true },
      { id: 'screenscraper', enabled: false },
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

  it('knows which changes need a reopen', () => {
    const b = cloneSettings(DEFAULT_SETTINGS);
    b.general.theme = 'dark';
    expect(needsReopen(DEFAULT_SETTINGS, b)).toBe(false);
    b.library.include_hidden = true;
    expect(needsReopen(DEFAULT_SETTINGS, b)).toBe(true);
  });
});
