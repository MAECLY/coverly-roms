import { describe, expect, it } from 'vitest';

import { addonViewsLibrary } from '@/ipc/mock/addonViewsFixture';

import { buildGames } from './games';
import { ALL_SECTION_KEY, type Section } from './grouping';
import {
  buildRowModel,
  buildTreeModel,
  moveVertical,
  treeKey,
  type Row,
  type TreeRow,
} from './rowModel';

const A: Section = { key: 'system:0', label: 'Game Boy', start: 0, count: 5 };
const B: Section = { key: 'system:1', label: 'Mega Drive', start: 5, count: 3 };

function compact(rows: readonly Row[]): string[] {
  return rows.map((r) =>
    r.kind === 'header' ? `H${r.sectionIndex}` : `${r.start}-${r.end}`,
  );
}

describe('buildRowModel', () => {
  it('emits a header per section and rows of perRow items', () => {
    const m = buildRowModel([A, B], 4, true);
    expect(compact(m.rows)).toEqual(['H0', '0-4', '4-5', 'H1', '5-8']);
    expect(m.headerRows).toEqual([0, 3]);
    expect(m.rows[3]).toMatchObject({
      kind: 'header',
      section: B,
      sectionIndex: 1,
    });
    expect(m.rows[2]).toMatchObject({ sectionIndex: 0 });
  });

  it('maps every item index to its row', () => {
    const m = buildRowModel([A, B], 4, true);
    expect(m.rowOfIndex).toHaveLength(8);
    expect(m.rowOfIndex[6]).toBe(4);
    expect(Array.from(m.rowOfIndex)).toEqual([1, 1, 1, 1, 2, 4, 4, 4]);
  });

  it('emits no header rows without headers', () => {
    const m = buildRowModel([A, B], 4, false);
    expect(compact(m.rows)).toEqual(['0-4', '4-5', '5-8']);
    expect(m.headerRows).toEqual([]);
    expect(m.rowOfIndex[6]).toBe(2);
  });

  it('emits no header for the single none section', () => {
    const all: Section = { key: 'all', label: '', start: 0, count: 3 };
    const m = buildRowModel([all], 1, true);
    expect(compact(m.rows)).toEqual(['0-1', '1-2', '2-3']);
    expect(m.headerRows).toEqual([]);
  });

  it('handles an empty list', () => {
    const m = buildRowModel(
      [{ key: 'all', label: '', start: 0, count: 0 }],
      4,
      true,
    );
    expect(m.rows).toEqual([]);
    expect(m.rowOfIndex).toHaveLength(0);
  });

  it('treats perRow below 1 as 1', () => {
    expect(compact(buildRowModel([B], 0, false).rows)).toEqual([
      '5-6',
      '6-7',
      '7-8',
    ]);
  });
});

describe('moveVertical', () => {
  // perRow 4: H0, 0-4, 4-5, H1, 5-8
  const m = buildRowModel([A, B], 4, true);

  it('keeps the column in the next items row and skips headers', () => {
    expect(moveVertical(m, 1, 1)).toBe(4); // partial row clamps
    expect(moveVertical(m, 4, 1)).toBe(5); // into the next section
    expect(moveVertical(m, 6, -1)).toBe(4);
    expect(moveVertical(m, 4, -1)).toBe(0);
  });

  it('clamps a partial row to its last item', () => {
    expect(moveVertical(m, 3, 1)).toBe(4);
  });

  it('stays put at either end', () => {
    expect(moveVertical(m, 2, -1)).toBe(2);
    expect(moveVertical(m, 7, 1)).toBe(7);
  });

  it('moves several rows for a page and stops at the last row', () => {
    expect(moveVertical(m, 1, 2)).toBe(6);
    expect(moveVertical(m, 1, 99)).toBe(6);
    expect(moveVertical(m, 7, -99)).toBe(2);
  });
});

describe('buildTreeModel (addon views §8.4)', () => {
  const lib = addonViewsLibrary(1, 1_700_000_000);
  const { games, byEntry } = buildGames(lib.entries, {});
  const entries = games.map((g) => g.entry);
  const all = [
    { key: ALL_SECTION_KEY, label: '', start: 0, count: entries.length },
  ];
  const named = (name: string) => games.find((g) => g.entry.name === name)!;
  const lantern = named('Lantern Harbor (USA)');
  const tide = named('Tidewalker');
  const star = named('Star Courier (USA) (Rev 1)');
  const shape = (rows: readonly TreeRow[]) =>
    rows.map((r) => `${r.kind}:${r.level}:${r.posinset}/${r.setsize}`);

  it('lists games only while nothing is expanded', () => {
    const m = buildTreeModel(all, entries, byEntry, new Set(), false);
    expect(m.rows).toHaveLength(entries.length);
    expect(m.rows.every((r) => r.kind === 'game' && r.level === 1)).toBe(true);
    const row = m.rows[m.gameRow[entries.indexOf(lantern.entry)]!]!;
    expect(row).toMatchObject({
      kind: 'game',
      expandable: true,
      expanded: false,
    });
    const plain = m.rows.find(
      (r) => r.kind === 'game' && r.game?.entry.name === 'Pebble League',
    );
    expect(plain).toMatchObject({ expandable: false });
  });

  it('expands a game into updates, DLC, its demo and its folder, newest first', () => {
    const m = buildTreeModel(
      all,
      entries,
      byEntry,
      new Set([lantern.id]),
      false,
    );
    const at = m.gameRow[entries.indexOf(lantern.entry)]!;
    const kids = m.rows.slice(at + 1, at + 6);
    expect(kids.map((r) => r.kind)).toEqual([
      'child',
      'child',
      'child',
      'demo',
      'folder',
    ]);
    expect(shape(kids)).toEqual([
      'child:2:1/5',
      'child:2:2/5',
      'child:2:3/5',
      'demo:2:4/5',
      'folder:2:5/5',
    ]);
    const first = kids[0]!;
    expect(first.kind === 'child' && first.child.version).toBe('1.10');
    expect(first.kind === 'child' && first.newest).toBe(true);
    expect(kids[1]!.kind === 'child' && kids[1]!.newest).toBe(false);
    expect(kids.every((r) => r.parent === at)).toBe(true);
    expect(m.rows[at]).toMatchObject({ expanded: true });
  });

  it('puts a ghost row first when the newest update is elsewhere', () => {
    const m = buildTreeModel(all, entries, byEntry, new Set([tide.id]), false);
    const at = m.gameRow[entries.indexOf(tide.entry)]!;
    const ghost = m.rows[at + 1]!;
    expect(ghost).toMatchObject({ kind: 'ghost', version: '1.3.0', level: 2 });
    // The installed update is not «la más nueva» while a newer one is missing.
    const update = m.rows[at + 2]!;
    expect(update.kind === 'child' && update.newest).toBe(false);
  });

  it('lists the other revisions of a retro game', () => {
    const m = buildTreeModel(all, entries, byEntry, new Set([star.id]), false);
    const at = m.gameRow[entries.indexOf(star.entry)]!;
    const rev = m.rows[at + 1]!;
    expect(rev).toMatchObject({ kind: 'revision', level: 2, setsize: 1 });
    expect(rev.kind === 'revision' && rev.entry.name).toBe(
      'Star Courier (USA)',
    );
  });

  it('nests sections, games and content at levels 1, 2 and 3', () => {
    const sections = [
      { key: 'a', label: 'A', start: 0, count: 2 },
      { key: 'b', label: 'B', start: 2, count: entries.length - 2 },
    ];
    const order = [
      lantern.entry,
      tide.entry,
      ...entries.filter((e) => e !== lantern.entry && e !== tide.entry),
    ];
    const m = buildTreeModel(
      sections,
      order,
      byEntry,
      new Set([lantern.id]),
      true,
    );
    expect(shape(m.rows.slice(0, 4))).toEqual([
      'section:1:1/2',
      'game:2:1/2',
      'child:3:1/5',
      'child:3:2/5',
    ]);
    expect(m.headerRows[0]).toBe(0);
    const second = m.rows[m.headerRows[1]!]!;
    expect(second).toMatchObject({ kind: 'section', posinset: 2, setsize: 2 });
  });

  it('works on entries without games', () => {
    const m = buildTreeModel(
      all,
      entries,
      undefined,
      new Set([lantern.id]),
      false,
    );
    expect(m.rows).toHaveLength(entries.length);
    expect(m.rows[0]).toMatchObject({
      kind: 'game',
      game: null,
      expandable: false,
    });
  });
});

describe('treeKey (APG treegrid keys)', () => {
  const lib = addonViewsLibrary(1, 1_700_000_000);
  const { games, byEntry } = buildGames(lib.entries, {});
  const entries = games.map((g) => g.entry);
  const all = [
    { key: ALL_SECTION_KEY, label: '', start: 0, count: entries.length },
  ];
  const lantern = games.find((g) => g.entry.name === 'Lantern Harbor (USA)')!;
  const li = entries.indexOf(lantern.entry);

  it('→ expands a collapsed parent, then moves to its first child', () => {
    const closed = buildTreeModel(all, entries, byEntry, new Set(), false);
    const at = closed.gameRow[li]!;
    expect(treeKey(closed, at, 'ArrowRight', 10)).toEqual({
      expand: lantern.id,
    });
    const open = buildTreeModel(
      all,
      entries,
      byEntry,
      new Set([lantern.id]),
      false,
    );
    expect(treeKey(open, at, 'ArrowRight', 10)).toEqual({ row: at + 1 });
    // On a child, → does nothing.
    expect(treeKey(open, at + 1, 'ArrowRight', 10)).toEqual({});
  });

  it('← goes from a child to its parent, then collapses it', () => {
    const open = buildTreeModel(
      all,
      entries,
      byEntry,
      new Set([lantern.id]),
      false,
    );
    const at = open.gameRow[li]!;
    expect(treeKey(open, at + 3, 'ArrowLeft', 10)).toEqual({ row: at });
    expect(treeKey(open, at, 'ArrowLeft', 10)).toEqual({
      collapse: lantern.id,
    });
  });

  it('↓ ↑ Home End move over visible rows and skip section rows', () => {
    const sections = [
      { key: 'a', label: 'A', start: 0, count: 1 },
      { key: 'b', label: 'B', start: 1, count: entries.length - 1 },
    ];
    const m = buildTreeModel(sections, entries, byEntry, new Set(), true);
    expect(treeKey(m, 1, 'ArrowDown', 10)).toEqual({ row: 3 });
    expect(treeKey(m, 3, 'ArrowUp', 10)).toEqual({ row: 1 });
    expect(treeKey(m, 1, 'ArrowUp', 10)).toEqual({});
    expect(treeKey(m, 3, 'Home', 10)).toEqual({ row: 1 });
    expect(treeKey(m, 1, 'End', 10)).toEqual({ row: m.rows.length - 1 });
    expect(treeKey(m, 1, 'PageDown', 2)).toEqual({ row: 4 });
  });
});
