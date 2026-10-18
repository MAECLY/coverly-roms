import { describe, expect, it } from 'vitest';

import { MemoryStorage } from '@/test/storage';

import {
  COVER_SIZE_IDS,
  DEFAULT_PREFS,
  LEGACY_LAYOUT_KEY,
  MAX_LIBRARIES,
  PREFS_KEY,
  VIEW_IDS,
  prefsFor,
  readPrefsFile,
  remember,
  sanitisePrefs,
  writePrefsFile,
} from './viewPrefs';

describe('view prefs', () => {
  it('lists the views in switcher order and the size stops', () => {
    expect(VIEW_IDS).toEqual([
      'covers',
      'details',
      'columns',
      'gallery',
      'mounts',
      'shelves',
    ]);
    expect(COVER_SIZE_IDS).toEqual(['xs', 's', 'm', 'l', 'xl']);
  });

  it('has the documented defaults', () => {
    expect(DEFAULT_PREFS).toEqual({
      view: 'covers',
      coverSize: 'm',
      groupBy: 'none',
      art: 'cover',
      columns: [
        { id: 'name' },
        { id: 'system' },
        { id: 'size' },
        { id: 'mtime' },
        { id: 'cover' },
        { id: 'screenshot' },
        { id: 'title' },
      ],
      density: 'comfortable',
      inspector: false,
    });
  });

  it('defaults when nothing is stored', () =>
    expect(readPrefsFile(new MemoryStorage()).last).toEqual(DEFAULT_PREFS));

  it('defaults without any storage', () =>
    expect(readPrefsFile(undefined).last).toEqual(DEFAULT_PREFS));

  it.each([
    ['list', 'details'],
    ['grid', 'covers'],
    ['junk', 'covers'],
  ])('migrates coverly.layout=%s to %s', (old, view) => {
    const s = new MemoryStorage();
    s.setItem(LEGACY_LAYOUT_KEY, old);
    expect(readPrefsFile(s).last.view).toBe(view);
  });

  it('ignores the old key once the new one exists', () => {
    const s = new MemoryStorage();
    s.setItem(LEGACY_LAYOUT_KEY, 'list');
    writePrefsFile(s, {
      last: { ...DEFAULT_PREFS, view: 'gallery' },
      byLibrary: {},
      order: [],
    });
    expect(readPrefsFile(s).last.view).toBe('gallery');
  });

  it('falls back to defaults on junk JSON', () => {
    const s = new MemoryStorage();
    s.setItem(PREFS_KEY, '{not json');
    expect(readPrefsFile(s)).toEqual({
      last: DEFAULT_PREFS,
      byLibrary: {},
      order: [],
    });
    s.setItem(PREFS_KEY, '[1,2]');
    expect(readPrefsFile(s).last).toEqual(DEFAULT_PREFS);
  });

  it('falls back field by field', () => {
    expect(
      sanitisePrefs({
        view: 'gallery',
        coverSize: 'huge',
        groupBy: 'system',
        art: 'x',
        columns: 'no',
        density: 'compact',
        inspector: 'yes',
      }),
    ).toEqual({
      ...DEFAULT_PREFS,
      view: 'gallery',
      groupBy: 'system',
      density: 'compact',
    });
  });

  it('treats non-objects as defaults', () => {
    for (const raw of [null, undefined, 3, 'covers', []])
      expect(sanitisePrefs(raw)).toEqual(DEFAULT_PREFS);
  });

  it('drops unknown and duplicate columns and keeps name', () => {
    expect(
      sanitisePrefs({
        columns: [{ id: 'size' }, { id: 'bogus' }, { id: 'size' }],
      }).columns.map((c) => c.id),
    ).toEqual(['name', 'size']);
  });

  it('keeps the order and valid widths of known columns', () => {
    expect(
      sanitisePrefs({
        columns: [
          { id: 'thumb', width: 64 },
          { id: 'name', width: 'wide' },
          { id: 'ext', width: 80.6 },
          { id: 'size', width: -5 },
          'size',
        ],
      }).columns,
    ).toEqual([
      { id: 'thumb', width: 64 },
      { id: 'name' },
      { id: 'ext', width: 81 },
      { id: 'size' },
    ]);
  });

  it('never shares arrays with the defaults', () => {
    const a = sanitisePrefs({});
    a.columns.push({ id: 'ext' });
    expect(DEFAULT_PREFS.columns.map((c) => c.id)).not.toContain('ext');
  });

  it('keeps prefs per library and falls back to last', () => {
    let f = readPrefsFile(new MemoryStorage());
    f = remember(f, '/a', { ...DEFAULT_PREFS, view: 'columns' });
    expect(prefsFor(f, '/a').view).toBe('columns');
    expect(prefsFor(f, '/b').view).toBe('columns'); // last
  });

  it('keeps at most 20 libraries, least recently used dropped', () => {
    let f = readPrefsFile(new MemoryStorage());
    for (let i = 0; i < 21; i++) f = remember(f, `/lib${i}`, DEFAULT_PREFS);
    expect(MAX_LIBRARIES).toBe(20);
    expect(Object.keys(f.byLibrary)).toHaveLength(20);
    expect(f.byLibrary).not.toHaveProperty('/lib0');
  });

  it('refreshes a library on use so it is not the one dropped', () => {
    let f = readPrefsFile(new MemoryStorage());
    for (let i = 0; i < 20; i++) f = remember(f, `/lib${i}`, DEFAULT_PREFS);
    f = remember(f, '/lib0', DEFAULT_PREFS);
    f = remember(f, '/new', DEFAULT_PREFS);
    expect(f.byLibrary).toHaveProperty('/lib0');
    expect(f.byLibrary).not.toHaveProperty('/lib1');
    expect(f.order.at(-1)).toBe('/new');
  });

  it('round-trips through storage and sanitises what it reads', () => {
    const s = new MemoryStorage();
    let f = readPrefsFile(s);
    f = remember(f, '/a', { ...DEFAULT_PREFS, view: 'shelves' });
    writePrefsFile(s, f);
    expect(readPrefsFile(s)).toEqual(f);

    s.setItem(
      PREFS_KEY,
      JSON.stringify({
        last: { view: 'nope' },
        byLibrary: { '/a': { view: 'mounts' }, '/b': 7 },
        order: ['/a', '/ghost', 9],
      }),
    );
    const read = readPrefsFile(s);
    expect(read.last).toEqual(DEFAULT_PREFS);
    expect(read.byLibrary['/a']?.view).toBe('mounts');
    expect(read.byLibrary['/b']).toEqual(DEFAULT_PREFS);
    // A root missing from the order counts as least recently used.
    expect(read.order).toEqual(['/b', '/a']);
  });

  it('survives a throwing storage', () => {
    const bad = {
      getItem: () => {
        throw new Error('x');
      },
      setItem: () => {
        throw new Error('x');
      },
    } as unknown as Storage;
    expect(readPrefsFile(bad).last).toEqual(DEFAULT_PREFS);
    expect(() => writePrefsFile(bad, readPrefsFile(bad))).not.toThrow();
    expect(() => writePrefsFile(undefined, readPrefsFile(bad))).not.toThrow();
  });
});
