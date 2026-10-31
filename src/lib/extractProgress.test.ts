import { describe, expect, it } from 'vitest';
import type {
  OrganizeItem,
  OrganizePlan,
  OrganizeProgressEvent,
} from '@/ipc/types';
import {
  autoItem,
  discItem,
  planOf,
  reviewItem,
} from '@/test/organizeFixtures';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { MOCK_DOWNLOADS } from '@/ipc/mock/mockOrganize';
import {
  extractView,
  gameIndex,
  middleTruncate,
  targetDir,
} from './extractProgress';

type ExtractEvent = Extract<OrganizeProgressEvent, { phase: 'extract' }>;

const ev = (item: number, extra: Partial<ExtractEvent> = {}): ExtractEvent => ({
  plan_id: 1,
  done: 0,
  total: 1,
  phase: 'extract',
  item,
  bytes_done: 0,
  bytes_total: 1000,
  ...extra,
});

/** A disc item with 12 entries (a cue and 11 bins), listed shuffled. */
function shuffledDisc(id: number): OrganizeItem {
  const item = discItem(id);
  if (item.transfer.kind !== 'extract') throw new Error('extract expected');
  const entries = [...item.transfer.entries];
  const order = [5, 0, 11, 3, 8, 1, 10, 2, 7, 4, 9, 6];
  return {
    ...item,
    transfer: { ...item.transfer, entries: order.map((k) => entries[k]!) },
  } as OrganizeItem;
}

describe('extractView rows', () => {
  const plan = planOf([shuffledDisc(7)]);

  it('sorts rows by index whatever the plan order', () => {
    const v = extractView(plan, ev(7, { entries_done: 0, entries_total: 12 }));
    expect(v.rows.map((r) => r.index)).toEqual([
      0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    ]);
    expect(v.rows[0]!.name).toBe('Next Tetris, The.cue');
    expect(v.rows[0]!.checked).toBe('crc');
  });

  it('marks the first row current at the start', () => {
    const v = extractView(plan, ev(7, { entries_done: 0, entries_total: 12 }));
    expect(v.rows[0]!.state).toBe('current');
    expect(v.rows.slice(1).every((r) => r.state === 'waiting')).toBe(true);
    expect(v.entries).toEqual({ done: 0, total: 12 });
    expect(v.phase).toBe('decoding');
  });

  it('marks the first k rows done mid-way', () => {
    const v = extractView(
      plan,
      ev(7, { entries_done: 4, entries_total: 12, bytes_done: 400 }),
    );
    expect(v.rows.slice(0, 4).every((r) => r.state === 'done')).toBe(true);
    expect(v.rows[4]!.state).toBe('current');
    expect(v.rows.slice(5).every((r) => r.state === 'waiting')).toBe(true);
    expect(v.phase).toBe('decoding');
  });

  it('switches to checking once every entry is staged but bytes remain', () => {
    const v = extractView(
      plan,
      ev(7, { entries_done: 12, entries_total: 12, bytes_done: 900 }),
    );
    expect(v.rows.every((r) => r.state === 'done')).toBe(true);
    expect(v.phase).toBe('checking');
  });

  it('checks once every entry is staged even with the bytes all decoded', () => {
    // A zip whose entries are all wanted decodes nothing else: the
    // all-staged edge (phase B starts) already carries bytes_total.
    const v = extractView(
      plan,
      ev(7, { entries_done: 12, entries_total: 12, bytes_done: 1000 }),
    );
    expect(v.phase).toBe('checking');
    expect(v.bytes).toEqual({ done: 1000, total: 1000 });
  });

  it('reads checking during the mock check pause', async () => {
    const b = createMockBackend({
      delayMs: 1,
      storage: null,
      pauseAfter: null,
      extractRate: 2000,
    });
    const { plan_id, plan: wire } = await b.organizePlan({
      source: MOCK_DOWNLOADS,
    });
    let first: number | null = null;
    const edges: ExtractEvent[] = [];
    await b.onOrganizeProgress((e) => {
      if (e.phase !== 'extract') return;
      first ??= e.item;
      if (e.item !== first) return;
      if (e.entries_total! > 1 && e.entries_done === e.entries_total) {
        edges.push(e);
        void b.organizeCancelApply();
      }
    });
    const ended = new Promise<void>((resolve) => {
      void b.onOrganizeChanged((v) => {
        if (v.state !== 'applying') resolve();
      });
    });
    await b.organizeApply(plan_id, []);
    await ended;
    expect(edges.length).toBeGreaterThan(0);
    expect(extractView(wire, edges[0]!).phase).toBe('checking');
  });

  it('marks entries without a CRC as size-checked', () => {
    const item = discItem(3, {}, 1);
    if (item.transfer.kind !== 'extract') throw new Error('extract expected');
    const noCrc = {
      ...item,
      transfer: {
        ...item.transfer,
        entries: item.transfer.entries.map((x) => ({ ...x, crc32: null })),
      },
    } as OrganizeItem;
    const v = extractView(
      planOf([noCrc]),
      ev(3, { entries_done: 0, entries_total: 2 }),
    );
    expect(v.rows.map((r) => r.checked)).toEqual(['size', 'size']);
  });
});

describe('extractView noun and single entries', () => {
  it('says track for a disc set and file otherwise', () => {
    const disc = discItem(1);
    const cart = { ...discItem(2), disc: null } as OrganizeItem;
    const plan = planOf([disc, cart]);
    expect(
      extractView(plan, ev(1, { entries_done: 0, entries_total: 12 })).noun,
    ).toBe('track');
    expect(
      extractView(plan, ev(2, { entries_done: 0, entries_total: 12 })).noun,
    ).toBe('file');
  });

  it('shows no counter or list for a single-entry item', () => {
    const one = autoItem(5, 'Game.tar.gz', 'gba', 'ROMS/GBA', {
      transfer: {
        kind: 'extract',
        entries: [
          {
            index: 0,
            name: 'Game.gba',
            size: 10,
            crc32: null,
            target: 'ROMS/GBA/Game.gba',
          },
        ],
        bytes: 10,
        archive_after: 'keep',
        source: { kind: 'builtin' },
        sheet_check: 'read',
        m3u: null,
      },
    });
    const v = extractView(
      planOf([one]),
      ev(5, { entries_done: 0, entries_total: 1 }),
    );
    expect(v.entries).toBeNull();
    expect(v.rows).toEqual([]);
    expect(v.noun).toBe('file');
    expect(v.name).toBe('Game.tar.gz');
    expect(v.targetDir).toBe('/pile/ROMS/GBA');
  });

  it('reads checking once the single entry is staged', () => {
    const one = autoItem(5, 'Game.7z', 'psp', 'ROMS/PSP', {
      transfer: {
        kind: 'extract',
        entries: [
          {
            index: 0,
            name: 'Game.iso',
            size: 1000,
            crc32: 1,
            target: 'ROMS/PSP/Game.iso',
          },
        ],
        bytes: 1000,
        archive_after: 'keep',
        source: { kind: 'builtin' },
        sheet_check: 'read',
        m3u: null,
      },
    });
    const plan = planOf([one]);
    expect(
      extractView(plan, ev(5, { entries_done: 0, entries_total: 1 })).phase,
    ).toBe('decoding');
    const v = extractView(
      plan,
      ev(5, { entries_done: 1, entries_total: 1, bytes_done: 1000 }),
    );
    expect(v.phase).toBe('checking');
    expect(v.entries).toBeNull();
  });
});

describe('extractView game counter', () => {
  function bigPlan(): OrganizePlan {
    const items: OrganizeItem[] = [];
    let id = 0;
    const extractIds: number[] = [];
    for (let k = 0; k < 40; k++) {
      id += 1;
      if (k === 5 || k === 19 || k === 33) {
        items.push(discItem(id));
        extractIds.push(id);
      } else if (k === 10) {
        // A review item never applies, even with an extract transfer.
        items.push({
          ...discItem(id),
          action: 'review',
          target: null,
        } as OrganizeItem);
      } else if (k === 25) {
        // A set-aside copy is not a game the apply extracts.
        items.push({
          ...discItem(id),
          action: 'set-aside',
          keeper: 'ROMS/PS/x.cue',
        } as unknown as OrganizeItem);
      } else if (k === 30) {
        items.push(reviewItem(id, `R${k}.gb`, [['gb', 0.5]]));
      } else {
        items.push(autoItem(id, `G${k}.gba`, 'gba', 'ROMS/GBA'));
      }
    }
    return planOf(items);
  }

  it('numbers the extract items in plan order', () => {
    const plan = bigPlan();
    expect(gameIndex(plan)).toEqual([6, 20, 34]);
    expect(gameIndex(plan)).toBe(gameIndex(plan));
  });

  it('reads game 2 of 3 even as the first event seen', () => {
    const plan = bigPlan();
    const v = extractView(
      plan,
      ev(20, { done: 19, total: 40, entries_done: 3, entries_total: 12 }),
    );
    expect(v.game).toEqual({ n: 2, total: 3 });
  });

  it('has no game counter for an item outside the index', () => {
    const plan = bigPlan();
    expect(extractView(plan, ev(11)).game).toBeNull();
  });
});

describe('extractView without a plan', () => {
  it('falls back to the id and keeps the event counter', () => {
    const v = extractView(null, ev(7, { entries_done: 4, entries_total: 12 }));
    expect(v.name).toBe('#7');
    expect(v.systemId).toBeNull();
    expect(v.game).toBeNull();
    expect(v.rows).toEqual([]);
    expect(v.targetDir).toBeNull();
    expect(v.entries).toEqual({ done: 4, total: 12 });
  });

  it('falls back when the item is missing from the plan', () => {
    const v = extractView(
      planOf([discItem(1)]),
      ev(9, { entries_done: 1, entries_total: 3 }),
    );
    expect(v.name).toBe('#9');
    expect(v.systemId).toBeNull();
    expect(v.game).toBeNull();
    expect(v.rows).toEqual([]);
    expect(v.entries).toEqual({ done: 1, total: 3 });
  });

  it('shows no counter for an event from an older wire', () => {
    const v = extractView(planOf([discItem(1)]), ev(1));
    expect(v.entries).toBeNull();
    expect(v.rows).toEqual([]);
    expect(v.name).toBe('Next Tetris, The.zip');
    expect(v.systemId).toBe('psx');
    expect(v.targetDir).toBe('/pile/ROMS/PS');
  });

  it('clamps bytes_done to bytes_total', () => {
    const v = extractView(null, ev(1, { bytes_done: 1200, bytes_total: 1000 }));
    expect(v.bytes).toEqual({ done: 1000, total: 1000 });
  });
});

describe('targetDir', () => {
  it('uses a Windows root separator', () => {
    expect(targetDir('C:\\Roms', 'PS/Game/Game (Track 1).bin')).toBe(
      'C:\\Roms\\PS\\Game',
    );
    expect(targetDir('C:\\Roms\\', 'PS/Game/Game (Track 1).bin')).toBe(
      'C:\\Roms\\PS\\Game',
    );
  });

  it('uses a Unix root separator', () => {
    expect(targetDir('/Volumes/SD/Roms', 'PS/Game/Game (Track 1).bin')).toBe(
      '/Volumes/SD/Roms/PS/Game',
    );
  });
});

describe('middleTruncate', () => {
  it('keeps the end of a long name', () => {
    const s = middleTruncate('Tetris Plus (Japan) (Track 18).bin', 24);
    expect(s).toContain('…');
    expect(s.endsWith('(Track 18).bin')).toBe(true);
    expect(s.length).toBe(24);
  });

  it('never exceeds a tiny limit', () => {
    expect(middleTruncate('Tetris.bin', 1)).toBe('…');
    expect(middleTruncate('Tetris.bin', 0)).toBe('');
    expect(middleTruncate('Tetris.bin', 2).length).toBe(2);
  });

  it('never cuts a character in half', () => {
    const lone =
      /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/;
    const name = '🎮'.repeat(30);
    for (let max = 2; max < 30; max++) {
      const s = middleTruncate(name, max);
      expect(s).not.toMatch(lone);
      expect([...s].length).toBe(max);
    }
  });

  it('leaves short names alone', () => {
    expect(middleTruncate('Game.cue', 24)).toBe('Game.cue');
  });
});
