import { beforeEach, describe, expect, it } from 'vitest';

import type { ChildView, EntryView, LibraryView } from '../types';
import {
  ADDONS_NEW_CHILD,
  addonViewsHuge,
  addonViewsLibrary,
  MOCK_ADDONS,
  MOCK_ADDONS_HUGE,
  MOCK_REVISIONS_KEY,
} from './addonViewsFixture';
import { FIXTURE_NOW } from './fixture';
import { createMockBackend } from './mockBackend';
import { readMockPick } from './mockOrganize';

const byName = (view: LibraryView, name: string): EntryView => {
  const e = view.entries.find((x) => x.name === name);
  if (!e) throw new Error(`no entry ${name}`);
  return e;
};

const systemId = (view: LibraryView, e: EntryView) =>
  view.systems[e.system]?.system_id;

/** Rust `child_views`: versioned children first, newest first (numeric
 * parts), then by name in byte order. */
const rustChildOrder = (a: ChildView, b: ChildView): number => {
  const parts = (c: ChildView) => c.version?.split('.').map(Number);
  const [x, y] = [parts(a), parts(b)];
  if (x && !y) return -1;
  if (!x && y) return 1;
  if (x && y) {
    for (let i = 0; i < Math.max(x.length, y.length); i += 1) {
      const d = (y[i] ?? 0) - (x[i] ?? 0);
      if (d !== 0) return d;
    }
  }
  return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
};

const expectRustOrder = (view: LibraryView) => {
  for (const e of view.entries) {
    const children = e.children ?? [];
    expect(children.map((c) => c.rel_path)).toEqual(
      [...children].sort(rustChildOrder).map((c) => c.rel_path),
    );
  }
};

describe('add-ons mock library (?mockPick=addons)', () => {
  const view = addonViewsLibrary(3, FIXTURE_NOW);

  it('is the mockup library, rooted at MOCK_ADDONS', () => {
    expect(view.root).toBe(MOCK_ADDONS);
    expect(view.generation).toBe(3);
    expect(view.profile.id).toBe('coverly-library');
    view.entries.forEach((e, i) => expect(e.id).toBe(i));
  });

  it('gives Lantern Harbor two updates, a DLC with package art, a demo and folder counts', () => {
    const lh = byName(view, 'Lantern Harbor (USA)');
    expect(systemId(view, lh)).toBe('ps4');
    const updates = lh.children!.filter((c) => c.role === 'update');
    const dlc = lh.children!.filter((c) => c.role === 'dlc');
    // Newest first, as Rust sends them.
    expect(updates.map((c) => c.version)).toEqual(['1.10', '1.05']);
    expect(lh.installed).toBe('1.10');
    expect(dlc).toHaveLength(1);
    expect(dlc[0]!.art).toMatchObject({ icon: true, icon_origin: 'package' });
    expect(lh.addons).toEqual({ mods: 1, cheats: 2, extras: 3, saves: 0 });
    const demo = byName(view, 'Lantern Harbor (Demo)');
    expect(demo.demo).toEqual({ of: lh.id, of_name: lh.name });
  });

  it('has Paper Skies (Demo) without its game', () => {
    const ps = byName(view, 'Paper Skies (Demo)');
    expect(ps.demo).toEqual({});
    expect(view.entries.some((e) => e.name.startsWith('Paper Skies ('))).toBe(
      true,
    );
    expect(
      view.entries.filter((e) => e.name.startsWith('Paper Skies')),
    ).toHaveLength(1);
  });

  it('has Tidewalker missing v1.3.0, which sits orphaned elsewhere', () => {
    const tw = byName(view, 'Tidewalker');
    expect(systemId(view, tw)).toBe('switch');
    const updates = tw.children!.filter((c) => c.role === 'update');
    const dlc = tw.children!.filter((c) => c.role === 'dlc');
    expect(updates.map((c) => c.version)).toEqual(['1.0.2']);
    expect(tw.installed).toBe('1.0.2');
    expect(dlc).toHaveLength(1);
    expect(dlc[0]!.art).toEqual({ icon: false, banner: false });
    const orphan = view.entries[tw.newer_elsewhere!.entry]!;
    expect(tw.newer_elsewhere!.version).toBe('1.3.0');
    expect(orphan.orphan).toBe('update');
    expect(orphan.orphan_of).toBe(tw.id);
    expect(orphan.version).toBe('1.3.0');
  });

  it('has Neon Rhythm with three DLC, one of them the «Nuevo» candidate', () => {
    const nr = byName(view, 'Neon Rhythm');
    const dlc = nr.children!.filter((c) => c.role === 'dlc');
    expect(dlc).toHaveLength(3);
    expect(dlc.map((c) => c.rel_path)).toContain(ADDONS_NEW_CHILD);
  });

  it('has Star Courier as two revisions of one group', () => {
    const plain = byName(view, 'Star Courier (USA)');
    const rev1 = byName(view, 'Star Courier (USA) (Rev 1)');
    expect(systemId(view, plain)).toBe('snes');
    expect(plain.version_group).toBeDefined();
    expect(rev1.version_group).toBe(plain.version_group);
    expect(rev1.revision).toBe('Rev 1');
    expect(plain.revision).toBeUndefined();
    expect(view.revision_defaults).toBeUndefined();
  });

  it('has Kite Days, Pebble League, Moss Garden and a 3DS base', () => {
    for (const n of ['Kite Days', 'Pebble League', 'Moss Garden']) {
      expect(byName(view, n)).toBeDefined();
    }
    expect(
      view.entries.some(
        (e) => systemId(view, e) === '3ds' && !e.orphan && !e.demo,
      ),
    ).toBe(true);
  });

  it('has two orphans, one linked to its game and one not', () => {
    const orphans = view.entries.filter((e) => e.orphan);
    expect(orphans).toHaveLength(2);
    expect(orphans.filter((o) => o.orphan_of !== undefined)).toHaveLength(1);
    expect(orphans.map((o) => o.orphan).sort()).toEqual(['dlc', 'update']);
  });

  it('points every link at an existing entry', () => {
    const ids = new Set(view.entries.map((e) => e.id));
    for (const e of view.entries) {
      if (e.orphan_of !== undefined) expect(ids.has(e.orphan_of)).toBe(true);
      if (e.demo?.of !== undefined) expect(ids.has(e.demo.of)).toBe(true);
      if (e.newer_elsewhere)
        expect(ids.has(e.newer_elsewhere.entry)).toBe(true);
      if (e.version_group)
        expect(
          view.entries.filter((x) => x.version_group === e.version_group)
            .length,
        ).toBeGreaterThan(1);
      const indexes = (e.children ?? []).map((c) => c.index);
      expect(new Set(indexes).size).toBe(indexes.length);
      expect(view.systems[e.system]).toBeDefined();
    }
  });

  it('counts raw entries: 2 orphans, 2 demos, 2 revisions of one group', () => {
    expect(view.entries.filter((e) => e.orphan)).toHaveLength(2);
    expect(view.entries.filter((e) => e.demo)).toHaveLength(2);
    expect(view.entries.filter((e) => e.version_group)).toHaveLength(2);
    const children = view.entries.flatMap((e) => e.children ?? []);
    expect(children.filter((c) => c.role === 'update')).toHaveLength(4);
    expect(children.filter((c) => c.role === 'dlc')).toHaveLength(5);
    for (const s of view.systems) {
      expect(s.roms).toBe(
        view.entries.filter((e) => e.system === s.index).length,
      );
    }
  });

  it('orders every game’s children as Rust sends them', () => {
    expectRustOrder(view);
  });

  it('is deterministic', () => {
    expect(addonViewsLibrary(3, FIXTURE_NOW)).toEqual(view);
  });
});

describe('huge add-ons mock library (?mockPick=addons-huge)', () => {
  const huge = addonViewsHuge(1, FIXTURE_NOW);

  it('has 10 000 games and 50 000 children', () => {
    expect(huge.root).toBe(MOCK_ADDONS_HUGE);
    expect(huge.entries.filter((e) => !e.orphan)).toHaveLength(10_000);
    expect(huge.entries.flatMap((e) => e.children ?? [])).toHaveLength(50_000);
    expect(huge.entries.some((e) => e.demo)).toBe(true);
    const ids = new Set(huge.entries.map((e) => e.id));
    for (const e of huge.entries)
      if (e.demo?.of !== undefined) expect(ids.has(e.demo.of)).toBe(true);
  });

  it('orders every game’s children as Rust sends them', () => {
    expectRustOrder(huge);
  });

  it('is deterministic (seeded)', () => {
    const again = addonViewsHuge(1, FIXTURE_NOW);
    expect(again.entries[4321]).toEqual(huge.entries[4321]);
    expect(again.entries[9999]).toEqual(huge.entries[9999]);
  });
});

describe('add-ons mock backend', () => {
  let storage: Storage;
  beforeEach(() => {
    const data = new Map<string, string>();
    storage = {
      get length() {
        return data.size;
      },
      clear: () => data.clear(),
      getItem: (k) => data.get(k) ?? null,
      key: (i) => [...data.keys()][i] ?? null,
      removeItem: (k) => void data.delete(k),
      setItem: (k, v) => void data.set(k, String(v)),
    };
  });
  const backend = () =>
    createMockBackend({ delayMs: 0, now: FIXTURE_NOW, storage });

  it('resolves the addons picks', () => {
    window.history.replaceState(null, '', '/?mockPick=addons');
    expect(readMockPick()).toBe(MOCK_ADDONS);
    window.history.replaceState(null, '', '/?mockPick=addons-huge');
    expect(readMockPick()).toBe(MOCK_ADDONS_HUGE);
    window.history.replaceState(null, '', '/');
  });

  it('opens both libraries', async () => {
    const b = backend();
    const view = await b.openLibrary(MOCK_ADDONS);
    expect(view.entries.map((e) => e.name)).toContain('Tidewalker');
    const huge = await b.openLibrary(MOCK_ADDONS_HUGE);
    expect(huge.entries.length).toBeGreaterThanOrEqual(10_000);
  });

  it('answers game details, with the 3DS letter T unknown', async () => {
    const b = backend();
    const view = await b.openLibrary(MOCK_ADDONS);
    const mg = byName(view, 'Moss Garden');
    const d = await b.gameDetails(view.generation, mg.id, 9);
    expect(d.request).toBe(9);
    expect(d.region).toMatchObject({ letter: 'T', certainty: 'unknown' });
    expect(d.region!.region).toBeUndefined();
    expect(d.product_code).toMatch(/^CTR-P-...T$/);
    expect(d.latest).toBeUndefined();
    const lh = byName(view, 'Lantern Harbor (USA)');
    const ld = await b.gameDetails(view.generation, lh.id, 10);
    expect(ld.children.map((c) => c.index).sort()).toEqual(
      lh.children!.map((c) => c.index).sort(),
    );
    expect(ld.children.every((c) => c.kind !== undefined)).toBe(true);
  });

  it('persists the revision default across a reload', async () => {
    const b = backend();
    const view = await b.openLibrary(MOCK_ADDONS);
    const plain = byName(view, 'Star Courier (USA)');
    const out = await b.setDefaultRevision(view.generation, plain.id);
    expect(out).toEqual({ [plain.version_group!]: plain.rel_path });
    expect(storage.getItem(MOCK_REVISIONS_KEY)).toContain(plain.rel_path);
    const reloaded = await backend().openLibrary(MOCK_ADDONS);
    expect(reloaded.revision_defaults).toEqual({
      [plain.version_group!]: plain.rel_path,
    });
  });

  it('prunes stale revision choices when one is set, as Rust does', async () => {
    const plain = byName(
      addonViewsLibrary(0, FIXTURE_NOW),
      'Star Courier (USA)',
    );
    const group = plain.version_group!;
    const rev1 = `${plain.dir}/Star Courier (USA) (Rev 1).sfc`;
    const stale = {
      'snes:gone (usa)': 'ROMS/SFC/Gone (USA).sfc',
      [`${group}x`]: plain.rel_path,
    };
    storage.setItem(
      MOCK_REVISIONS_KEY,
      JSON.stringify({
        [MOCK_ADDONS]: { ...stale, [group]: rev1 },
        '/Users/demo/Other': stale,
      }),
    );
    const b = backend();
    const view = await b.openLibrary(MOCK_ADDONS);
    // Like Rust, the library opens with what the settings hold.
    expect(view.revision_defaults).toEqual({ ...stale, [group]: rev1 });
    const out = await b.setDefaultRevision(view.generation, plain.id);
    expect(out).toEqual({ [group]: plain.rel_path });
    const saved = JSON.parse(storage.getItem(MOCK_REVISIONS_KEY)!) as Record<
      string,
      unknown
    >;
    expect(saved[MOCK_ADDONS]).toEqual({ [group]: plain.rel_path });
    // Other libraries' choices are not this library's to prune.
    expect(saved['/Users/demo/Other']).toEqual(stale);
  });

  it('reads child art in the clear as package icons, never on Switch', async () => {
    const b = backend();
    const view = await b.openLibrary(MOCK_ADDONS);
    const nr = byName(view, 'Neon Rhythm');
    const before = nr.children!.filter((c) => !c.art.icon).length;
    expect(before).toBeGreaterThan(0);
    const read = await b.readChildArt(view.generation, nr.id);
    expect(read.written).toBe(before);
    expect(read.job).toBeDefined();
    expect(
      read.children.every((c) => c.art.icon && c.art.icon_origin === 'package'),
    ).toBe(true);
    const child = read.children[0]!;
    const img = await b.readChildArtBytes(
      view.generation,
      nr.id,
      child.index,
      'icon',
    );
    expect(img.mime).toBe('image/svg+xml');
    expect(new TextDecoder().decode(img.bytes)).toContain('<svg');
    await expect(
      b.readChildArtBytes(view.generation, nr.id, child.index, 'banner'),
    ).rejects.toMatchObject({ code: 'art-missing' });

    const tw = byName(view, 'Tidewalker');
    const sw = await b.readChildArt(view.generation, tw.id);
    expect(sw.written).toBe(0);
    expect(sw.job).toBeUndefined();
    expect(sw.skipped.every((s) => s.why === 'no-art-in-clear')).toBe(true);
  });
});
