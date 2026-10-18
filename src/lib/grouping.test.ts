import { describe, expect, it } from 'vitest';

import { generateLibrary } from '@/ipc/mock/fixture';
import {
  REGIONS,
  type ArtMark,
  type EntryView,
  type LibraryView,
  type Region,
  type SystemView,
} from '@/ipc/types';

import {
  groupEntries,
  sectionLabel,
  type GroupContext,
  type Section,
} from './grouping';
import type { GroupBy } from './viewPrefs';

const SYSTEMS: SystemView[] = [
  {
    index: 0,
    folder: 'GB',
    dir: 'ROMS/GB',
    system_id: 'gb',
    name: 'Game Boy',
    roms: 4,
  },
  {
    index: 1,
    folder: 'SFC',
    dir: 'ROMS/SFC',
    system_id: 'snes',
    name: 'Super Nintendo',
    roms: 4,
  },
  {
    index: 2,
    folder: 'MD',
    dir: 'ROMS/MD',
    system_id: 'megadrive',
    name: 'Mega Drive',
    roms: 4,
  },
];

const MARK: Record<string, ArtMark> = {
  p: 'present',
  m: 'missing',
  u: 'unsupported',
};

function entry(
  id: number,
  system: number,
  dir: string,
  name: string,
  regions: Region[],
  art: string,
): EntryView {
  const [cover, screenshot, title] = [...art].map((c) => MARK[c] as ArtMark);
  return {
    id,
    system,
    rel_path: `${dir}/${name}.rom`,
    dir,
    name,
    ext: 'rom',
    size: 1,
    mtime: 0,
    regions,
    art: { cover: cover!, screenshot: screenshot!, title: title! },
  };
}

/** 12 entries over 3 systems, in an arbitrary (already sorted) order. */
const ENTRIES: EntryView[] = [
  entry(0, 1, 'ROMS/SFC', 'Zelda', ['usa'], 'ppp'),
  entry(1, 0, 'ROMS/GB', 'Tetris', ['japan', 'world'], 'mpp'),
  entry(2, 2, 'ROMS/MD/Hacks', 'Sonic', ['europe'], 'pmp'),
  entry(3, 0, 'ROMS/GB/Hacks', 'Élan', [], 'ppp'),
  entry(4, 1, 'ROMS/SFC', '3 Ninjas', ['japan'], 'mmm'),
  entry(5, 2, 'ROMS/MD', 'Altered Beast', ['europe', 'usa'], 'ppm'),
  entry(6, 0, 'ROMS/GB', 'alleyway', ['usa'], 'pmp'),
  entry(7, 1, 'ROMS/SFC/Hacks', 'Zombies', [], 'ppm'),
  entry(8, 2, 'ROMS/MD', 'Columns', ['japan'], 'ppp'),
  entry(9, 0, 'ROMS/GB', 'Kirby', ['usa'], 'mpp'),
  entry(10, 1, 'ROMS/SFC', 'Mario', ['usa'], 'pup'),
  entry(11, 2, 'ROMS/MD/Hacks', 'Ecco', ['brazil'], 'pmp'),
];

/** The fixture profile handles cover and screenshot, not title. */
const CTX: GroupContext = {
  systems: SYSTEMS,
  regionOrder: REGIONS,
  artMedia: ['cover', 'screenshot'],
};

function ctx(view: LibraryView): GroupContext {
  return {
    systems: view.systems,
    regionOrder: REGIONS,
    artMedia: view.profile.media,
  };
}

function shape(sections: readonly Section[]): [string, number][] {
  return sections.map((s) => [s.key, s.count]);
}

function idsOf(ordered: readonly EntryView[], section: Section): number[] {
  return ordered
    .slice(section.start, section.start + section.count)
    .map((e) => e.id);
}

/** Inside every section the input order is kept, and ranges tile the list. */
function expectStable(by: GroupBy): void {
  const { ordered, sections } = groupEntries(ENTRIES, by, CTX);
  expect(ordered).toHaveLength(ENTRIES.length);
  let next = 0;
  for (const s of sections) {
    expect(s.start).toBe(next);
    next += s.count;
    const ids = idsOf(ordered, s);
    const inInput = ENTRIES.filter((e) => ids.includes(e.id)).map((e) => e.id);
    expect(ids).toEqual(inInput);
  }
  expect(next).toBe(ENTRIES.length);
}

describe('groupEntries', () => {
  it('none: one section over everything, same array reference', () => {
    const { ordered, sections } = groupEntries(ENTRIES, 'none', CTX);
    expect(ordered).toBe(ENTRIES);
    expect(shape(sections)).toEqual([['all', 12]]);
    expect(sections[0]!.start).toBe(0);
  });

  it('none on an empty list still gives one empty section', () => {
    expect(shape(groupEntries([], 'none', CTX).sections)).toEqual([['all', 0]]);
  });

  it('other keys on an empty list give no sections', () => {
    expect(groupEntries([], 'system', CTX).sections).toEqual([]);
  });

  it('system: sidebar order, with system ids and names', () => {
    const { ordered, sections } = groupEntries(ENTRIES, 'system', CTX);
    expect(shape(sections)).toEqual([
      ['system:0', 4],
      ['system:1', 4],
      ['system:2', 4],
    ]);
    expect(sections.map((s) => [s.label, s.systemId])).toEqual([
      ['Game Boy', 'gb'],
      ['Super Nintendo', 'snes'],
      ['Mega Drive', 'megadrive'],
    ]);
    expect(ordered.map((e) => e.id)).toEqual([
      1, 3, 6, 9, 0, 4, 7, 10, 2, 5, 8, 11,
    ]);
    expectStable('system');
  });

  it('system: an unknown system index gets its own trailing section', () => {
    const stray = entry(12, 7, 'ROMS/X68000', 'Stray', [], 'ppp');
    const { sections } = groupEntries([stray, ...ENTRIES], 'system', CTX);
    expect(shape(sections)).toEqual([
      ['system:0', 4],
      ['system:1', 4],
      ['system:2', 4],
      ['system:7', 1],
    ]);
    expect(sections[3]!.systemId).toBeNull();
  });

  it('folder: path relative to the system, root first, per system', () => {
    const { sections } = groupEntries(ENTRIES, 'folder', CTX);
    expect(shape(sections)).toEqual([
      ['folder:0:', 3],
      ['folder:0:Hacks', 1],
      ['folder:1:', 3],
      ['folder:1:Hacks', 1],
      ['folder:2:', 2],
      ['folder:2:Hacks', 2],
    ]);
    expect(sections[0]).toMatchObject({
      label: 'Game Boy',
      labelKey: 'layouts.group.root',
      systemId: 'gb',
    });
    expect(sections[1]).toMatchObject({ label: 'Hacks', systemId: 'gb' });
    expect(sections[1]!.labelKey).toBeUndefined();
    expectStable('folder');
  });

  it('region: first region in region order, none last', () => {
    const { sections } = groupEntries(ENTRIES, 'region', CTX);
    expect(shape(sections)).toEqual([
      ['region:world', 1],
      ['region:usa', 5],
      ['region:europe', 1],
      ['region:japan', 2],
      ['region:brazil', 1],
      ['region:none', 2],
    ]);
    expect(sections[1]).toMatchObject({ labelKey: 'regions.usa' });
    expect(sections[5]).toMatchObject({ labelKey: 'layouts.group.noRegion' });
    expectStable('region');
  });

  it('art: missing cover, other art missing, complete (only the profile media)', () => {
    const { ordered, sections } = groupEntries(ENTRIES, 'art', CTX);
    expect(shape(sections)).toEqual([
      ['art:missingCover', 3],
      ['art:missingOther', 3],
      ['art:complete', 6],
    ]);
    expect(sections.map((s) => s.labelKey)).toEqual([
      'layouts.group.missingCover',
      'layouts.group.missingOther',
      'layouts.group.complete',
    ]);
    expect(idsOf(ordered, sections[2]!)).toEqual([0, 3, 5, 7, 8, 10]);
    expectStable('art');
  });

  it('art: a frozen assignment wins over the live marks', () => {
    const frozen = new Map([[1, 'complete' as const]]);
    const { sections } = groupEntries(ENTRIES, 'art', {
      ...CTX,
      frozenArt: frozen,
    });
    expect(shape(sections)).toEqual([
      ['art:missingCover', 2],
      ['art:missingOther', 3],
      ['art:complete', 7],
    ]);
  });

  it('letter: # first, then A–Z, diacritics and case folded', () => {
    const { ordered, sections } = groupEntries(ENTRIES, 'letter', CTX);
    expect(shape(sections)).toEqual([
      ['letter:#', 1],
      ['letter:A', 2],
      ['letter:C', 1],
      ['letter:E', 2],
      ['letter:K', 1],
      ['letter:M', 1],
      ['letter:S', 1],
      ['letter:T', 1],
      ['letter:Z', 2],
    ]);
    expect(idsOf(ordered, sections[3]!)).toEqual([3, 11]);
    expect(sections[3]!.label).toBe('E');
    expectStable('letter');
  });

  it('gives equal results for the same inputs', () => {
    for (const by of ['system', 'folder', 'region', 'art', 'letter'] as const) {
      expect(groupEntries(ENTRIES, by, CTX)).toEqual(
        groupEntries(ENTRIES, by, CTX),
      );
    }
  });

  it('does not mutate its input', () => {
    const before = ENTRIES.map((e) => e.id);
    groupEntries(ENTRIES, 'letter', CTX);
    expect(ENTRIES.map((e) => e.id)).toEqual(before);
  });

  it('groups 12k entries under 30 ms', () => {
    const view = generateLibrary({
      seed: 12,
      count: 12_000,
      root: '/m',
      generation: 1,
      now: 1_760_000_000,
    });
    const t0 = performance.now();
    groupEntries(view.entries, 'system', ctx(view));
    expect(performance.now() - t0).toBeLessThan(30);
  });
});

describe('sectionLabel', () => {
  const t = (key: string, vars?: Record<string, unknown>) =>
    `${key}${vars ? `|${JSON.stringify(vars)}` : ''}`;

  it('uses the plain label when there is no key', () => {
    expect(
      sectionLabel({ key: 'letter:A', label: 'A', start: 0, count: 1 }, t),
    ).toBe('A');
  });

  it('passes the label as {system} to the key', () => {
    expect(
      sectionLabel(
        {
          key: 'folder:0:',
          label: 'Game Boy',
          labelKey: 'layouts.group.root',
          start: 0,
          count: 1,
        },
        t,
      ),
    ).toBe('layouts.group.root|{"system":"Game Boy"}');
  });
});
