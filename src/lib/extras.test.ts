import { describe, expect, it } from 'vitest';

import { translate } from '@/i18n/core';
import type { TFunction } from '@/i18n/useT';
import requests from '@/ipc/contract.requests.json';
import en from '../../locales/en/ui.json';
import es from '../../locales/es/ui.json';
import type {
  CategoryRef,
  CategoryView,
  ExtraFileView,
  ExtraFormat,
} from '@/ipc/types';

import {
  categoryKey,
  categoryLabel,
  extOf,
  filterFiles,
  gameTags,
  MAX_TAGS,
  normalizeTags,
  officialWhatKey,
  sameCategory,
  sortCategories,
  suggestedCategory,
  typeOf,
  typeOfName,
} from './extras';

const tEs: TFunction = (k, v) => translate('es', k, v);
const tEn: TFunction = (k, v) => translate('en', k, v);

const fixed = (name: string): CategoryRef =>
  ({ kind: 'fixed', name }) as CategoryRef;
const user = (name: string): CategoryRef => ({ kind: 'user', name });
const ROOT: CategoryRef = { kind: 'root' };

function file(
  index: number,
  name: string,
  tags: string[] = [],
  category: CategoryRef = fixed('soundtrack'),
): ExtraFileView {
  return {
    index,
    category,
    path: name,
    name,
    kind: typeOfName(name),
    provenance: 'user',
    format: null,
    tags,
    size: 1,
    link: false,
    playable: 'inline',
  };
}

describe('typeOf', () => {
  it('sorts extensions into the types Rust uses, any case', () => {
    expect(typeOf('FLAC')).toBe('audio');
    expect(typeOf('spc')).toBe('audio');
    expect(typeOf('minipsf')).toBe('audio');
    expect(typeOf('webm')).toBe('video');
    expect(typeOf('MKV')).toBe('video');
    expect(typeOf('png')).toBe('image');
    expect(typeOf('heic')).toBe('image');
    expect(typeOf('pdf')).toBe('document');
    expect(typeOf('nfo')).toBe('document');
    expect(typeOf('p3t')).toBe('other');
    expect(typeOf('')).toBe('other');
  });

  it('reads the extension of a name like Rust (no dot files, last dot)', () => {
    expect(extOf('01 Opening.FLAC')).toBe('flac');
    expect(extOf('a.b.pdf')).toBe('pdf');
    expect(extOf('.hidden')).toBe('');
    expect(extOf('README')).toBe('');
    expect(typeOfName('Theme.p3t')).toBe('other');
    expect(typeOfName('Boss.ogg')).toBe('audio');
  });
});

describe('suggestedCategory', () => {
  const table: [string, string][] = [
    ['01 Opening.flac', 'soundtrack'],
    ['Track.spc', 'soundtrack'],
    ['Trailer.mp4', 'videos'],
    ['Cover.PNG', 'artwork'],
    ['Strategy.pdf', 'guides'],
    ['Game Manual.pdf', 'manuals'],
    ['MANUEL.pdf', 'manuals'],
    ['Handbuch DE.pdf', 'manuals'],
    ['manuale.pdf', 'manuals'],
    ['説明書.pdf', 'manuals'],
    // Only a PDF is a manual by its name; a text file is a guide.
    ['manual.txt', 'guides'],
    ['Theme.p3t', 'themes'],
    ['Avatar.PKG', 'themes'],
    ['Theme.ptf', 'themes'],
    ['Soundtrack.cia', 'themes'],
    ['notes.nfo', 'guides'],
    ['blob.bin', 'guides'],
    ['README', 'guides'],
  ];
  it.each(table)('%s → %s', (name, cat) => {
    expect(suggestedCategory(name)).toBe(cat);
  });
});

describe('categories', () => {
  it('labels fixed categories by copy, user ones by name and the root', () => {
    expect(categoryLabel(fixed('soundtrack'), tEs)).toBe('Banda sonora');
    expect(categoryLabel(fixed('themes'), tEn)).toBe('Themes and avatars');
    expect(categoryLabel(user('Remixes'), tEs)).toBe('Remixes');
    expect(categoryLabel(ROOT, tEs)).toBe('Sin categoría');
    expect(categoryLabel(ROOT, tEn)).toBe('No category');
  });

  it('compares and keys categories by value', () => {
    expect(sameCategory(fixed('videos'), fixed('videos'))).toBe(true);
    expect(sameCategory(fixed('videos'), user('videos'))).toBe(false);
    expect(sameCategory(user('A'), user('A'))).toBe(true);
    expect(sameCategory(user('A'), user('a'))).toBe(false);
    expect(sameCategory(ROOT, { kind: 'root' })).toBe(true);
    const keys = [fixed('videos'), user('videos'), ROOT].map(categoryKey);
    expect(new Set(keys).size).toBe(3);
  });

  it('sorts fixed first in their order, then user A→Z, the root, the missing band', () => {
    const view = (category: CategoryRef): CategoryView => ({
      category,
      present: true,
      count: 0,
      link: false,
      sync: null,
    });
    const sorted = sortCategories([
      { category: { kind: 'missing' } },
      view(ROOT),
      view(user('zeta')),
      view(fixed('guides')),
      view(user('Álbum')),
      view(fixed('soundtrack')),
      view(user('beta')),
      view(fixed('manuals')),
    ]);
    expect(sorted.map((c) => c.category)).toEqual([
      fixed('soundtrack'),
      fixed('manuals'),
      fixed('guides'),
      user('Álbum'),
      user('beta'),
      user('zeta'),
      ROOT,
      { kind: 'missing' },
    ]);
  });

  it('names what every official format is, and nothing for a rip', () => {
    const official: ExtraFormat[] = [
      'ps3-p3t',
      'ps3-theme-pkg',
      'ps3-avatar-pkg',
      'vita-theme-pkg',
      'psp-ptf',
      'x360-theme',
      'x360-gamerpic',
      'x360-avatar',
      'x360-video',
      '3ds-soundtrack-cia',
    ];
    for (const f of official) {
      const key = officialWhatKey(f);
      expect(key, f).not.toBeNull();
      expect(tEs(key!), f).not.toBe(key);
      expect(tEn(key!), f).not.toBe(key);
    }
    expect(tEs(officialWhatKey('ps3-p3t')!)).toBe('tema de PS3');
    expect(tEn(officialWhatKey('3ds-soundtrack-cia')!)).toBe('3DS soundtrack');
    for (const f of ['spc', 'nsf', 'gbs', 'vgm', 'psf'] as const)
      expect(officialWhatKey(f)).toBeNull();
  });
});

describe('normalizeTags', () => {
  const cases = requests.extras_tag_cases as {
    input: string[];
    tags?: string[];
    refused?: 'empty' | 'too-long' | 'too-many';
  }[];

  it.each(cases.map((c, i) => [i, c] as const))(
    'agrees with Rust on shared case %i',
    (_, c) => {
      const got = normalizeTags(c.input);
      if (c.tags) expect(got).toEqual({ ok: true, tags: c.tags });
      else expect(got).toEqual({ ok: false, refused: c.refused });
    },
  );

  it('keeps the first spelling, NFC-normalised', () => {
    const nfd = 'versión';
    expect(normalizeTags([nfd, 'VERSIÓN'])).toEqual({
      ok: true,
      tags: ['versión'],
    });
  });

  it('caps the count at MAX_TAGS', () => {
    const many = Array.from({ length: MAX_TAGS }, (_, i) => `t${i}`);
    expect(normalizeTags(many).ok).toBe(true);
    expect(normalizeTags([...many, 'one more'])).toEqual({
      ok: false,
      refused: 'too-many',
    });
    // Duplicates do not count towards the cap.
    expect(normalizeTags([...many, 'T0']).ok).toBe(true);
  });
});

describe('filterFiles and gameTags', () => {
  const files = [
    file(0, '01 Opening.flac', ['tema principal', 'Orquesta']),
    file(1, '02 Boss.flac', ['tema del jefe final', 'orquesta']),
    file(2, '03 Field.wav', []),
    file(3, 'Manual.pdf', ['scan'], fixed('manuals')),
  ];

  it('passes everything with no filter', () => {
    expect(filterFiles(files, { tags: [], text: '' })).toEqual(files);
    expect(filterFiles(files, { tags: [], text: '   ' })).toEqual(files);
  });

  it('keeps files that carry every chosen tag (AND), ignoring case', () => {
    const ids = (tags: string[]) =>
      filterFiles(files, { tags, text: '' }).map((f) => f.index);
    expect(ids(['orquesta'])).toEqual([0, 1]);
    expect(ids(['ORQUESTA', 'tema del jefe final'])).toEqual([1]);
    expect(ids(['orquesta', 'scan'])).toEqual([]);
  });

  it('matches text against names and tags, case and accent of NFC', () => {
    const ids = (text: string, tags: string[] = []) =>
      filterFiles(files, { tags, text }).map((f) => f.index);
    expect(ids('boss')).toEqual([1]);
    expect(ids('JEFE')).toEqual([1]);
    expect(ids('.flac')).toEqual([0, 1]);
    expect(ids('tema', ['orquesta'])).toEqual([0, 1]);
    expect(ids('nothing')).toEqual([]);
  });

  it('lists the game tags once, by first spelling, A→Z', () => {
    expect(gameTags(files)).toEqual([
      'Orquesta',
      'scan',
      'tema del jefe final',
      'tema principal',
    ]);
  });
});

describe('extras copy (spec §9)', () => {
  type Tree = { [k: string]: string | Tree };
  const leaves = (node: Tree, prefix = ''): [string, string][] =>
    Object.entries(node).flatMap(([k, v]): [string, string][] =>
      typeof v === 'string'
        ? [[`${prefix}${k}`, v]]
        : leaves(v, `${prefix}${k}.`),
    );
  const scoped = (tree: Tree) =>
    leaves(tree).filter(
      ([k]) =>
        k.startsWith('extras.') ||
        k.startsWith('player.') ||
        k.startsWith('viewer.') ||
        [
          'organize.note.extraOf',
          'organize.note.extraRip',
          'organize.left.extraWithoutGame',
          'organize.left.extrasNewer',
        ].includes(k),
    );

  it('bakes no number into Spanish copy and never exclaims', () => {
    const strings = scoped(es as Tree);
    expect(strings.length).toBeGreaterThan(80);
    for (const [key, value] of strings) {
      expect(value, key).not.toMatch(/[!¡]/);
      // A `…One` string names its single item as «1»; every other number
      // is a placeholder the UI renders in mono.
      // System names (PS3, Xbox 360, 3DS) are names, not counts.
      const named = value.replace(/PS3|Xbox 360|3DS/g, '');
      const bare = key.endsWith('One') ? named.replace(/\b1\b/, '') : named;
      expect(bare, key).not.toMatch(/\d/);
    }
  });

  it('has every Spanish key in English with the same placeholders', () => {
    const en_ = new Map(scoped(en as Tree));
    const vars = (s: string) =>
      [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    for (const [key, value] of scoped(es as Tree)) {
      expect(en_.has(key), key).toBe(true);
      expect(vars(en_.get(key)!), key).toEqual(vars(value));
    }
  });
});
