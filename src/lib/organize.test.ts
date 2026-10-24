import { describe, expect, it } from 'vitest';

import en from '../../locales/en/ui.json';
import es from '../../locales/es/ui.json';
import type {
  Evidence,
  LeftReason,
  OrganizeItem,
  OrganizePlan,
} from '@/ipc/types';
import { autoItem } from '@/test/organizeFixtures';

import {
  confidenceKey,
  groupBySystem,
  itemKey,
  itemNotes,
  leftItems,
  leftReasonKey,
  leftReasonText,
  reviewBuckets,
  ROWS_PER_GROUP,
  targetDir,
  whyKey,
} from './organize';

function item(
  id: number,
  patch: Partial<OrganizeItem> & Pick<OrganizeItem, 'action'>,
): OrganizeItem {
  return {
    id,
    sources: [`file ${id}.zip`],
    kind: 'archive',
    size: 1,
    fps: [],
    target: null,
    set_targets: [],
    system_id: null,
    folder: null,
    confidence: 0,
    method: null,
    why: [],
    alternatives: [],
    rename: null,
    notes: [],
    companions: [],
    catalogue_name: null,
    transfer: { kind: 'as-is' },
    disc: null,
    ...patch,
  } as OrganizeItem;
}

function plan(items: OrganizeItem[]): OrganizePlan {
  return { items } as OrganizePlan;
}

function has(catalog: object, key: string): boolean {
  let node: unknown = catalog;
  for (const part of key.split('.')) {
    if (typeof node !== 'object' || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === 'string';
}

const ALL_REASONS: LeftReason[] = [
  { kind: 'partial-download' },
  { kind: 'recently-modified' },
  { kind: 'cloud-only' },
  { kind: 'not-a-rom', what: 'web-page' },
  { kind: 'not-a-rom', what: 'text' },
  { kind: 'bios' },
  { kind: 'damaged-archive' },
  { kind: 'unsupported-archive' },
  { kind: 'multi-game-archive', count: 3 },
  { kind: 'unsupported-system', name: 'Atari Jaguar' },
  { kind: 'unknown' },
  { kind: 'duplicate', of: 'Tetris Attack.zip' },
  { kind: 'already-present', at: 'ROMS/GBA/x.zip' },
  { kind: 'already-sorted', folder: 'GBA' },
  { kind: 'symlink' },
  { kind: 'set-incomplete', missing: ['a.bin', 'b.bin'] },
  { kind: 'set-outside-folder' },
  { kind: 'shared-track' },
  { kind: 'ambiguous-companion' },
  { kind: 'too-big-for-fat' },
  { kind: 'unreadable' },
  { kind: 'collision-set' },
];

describe('organise plan helpers', () => {
  it('groups moving items by system folder, in folder order', () => {
    const p = plan([
      item(0, { action: 'auto', system_id: 'gba', folder: 'GBA' }),
      item(1, { action: 'auto', system_id: 'gb', folder: 'GB' }),
      item(2, { action: 'review', system_id: 'gb', folder: 'GB' }),
      item(3, { action: 'auto', system_id: 'gba', folder: 'GBA' }),
      item(4, { action: 'left', reason: { kind: 'unknown' } }),
    ]);
    const groups = groupBySystem(p);
    expect(groups.map((g) => [g.folder, g.systemId])).toEqual([
      ['GB', 'gb'],
      ['GBA', 'gba'],
    ]);
    expect(groups[1]!.items.map((i) => i.id)).toEqual([0, 3]);
    expect(ROWS_PER_GROUP).toBe(200);
  });

  it('buckets review items by candidate set', () => {
    const p = plan([
      item(0, {
        action: 'review',
        system_id: 'gb',
        alternatives: [['gbc', 0.45]],
      }),
      item(1, {
        action: 'review',
        system_id: 'gbc',
        alternatives: [['gb', 0.4]],
      }),
      item(2, {
        action: 'review',
        system_id: 'megadrive',
        alternatives: [['mastersystem', 0.3]],
      }),
      item(3, { action: 'auto', system_id: 'gba', folder: 'GBA' }),
    ]);
    const buckets = reviewBuckets(p);
    expect(buckets).toHaveLength(2);
    const gb = buckets.find((b) => b.items.length === 2)!;
    expect(gb.systems).toEqual(['gb', 'gbc']);
    expect(gb.items.map((i) => i.id)).toEqual([0, 1]);
    const md = buckets.find((b) => b.items.length === 1)!;
    expect(md.systems).toEqual(['megadrive', 'mastersystem']);
    expect(new Set(buckets.map((b) => b.key)).size).toBe(2);
  });

  it('lists the items left in place', () => {
    const p = plan([
      item(0, { action: 'auto', system_id: 'gba', folder: 'GBA' }),
      item(1, { action: 'left', reason: { kind: 'partial-download' } }),
    ]);
    expect(leftItems(p).map((i) => i.id)).toEqual([1]);
  });

  it('maps every LeftReason to an i18n key that exists in both locales', () => {
    for (const reason of ALL_REASONS) {
      const { key, vars } = leftReasonKey(reason);
      expect(has(en, key), `en ${key}`).toBe(true);
      expect(has(es, key), `es ${key}`).toBe(true);
      for (const v of Object.values(vars)) {
        if (typeof v === 'string' && v.startsWith('organize.')) {
          expect(has(en, v) && has(es, v), v).toBe(true);
        }
      }
    }
  });

  it('renders a left reason with its nested fragment and data', () => {
    const t = (key: string, vars?: Record<string, string | number>) =>
      `${key}${vars ? JSON.stringify(vars) : ''}`;
    expect(leftReasonText(t, { kind: 'not-a-rom', what: 'web-page' })).toBe(
      'organize.left.notARom{"what":"organize.left.webPage"}',
    );
    expect(leftReasonText(t, { kind: 'duplicate', of: 'a.zip' })).toBe(
      'organize.left.duplicate{"name":"a.zip"}',
    );
    expect(
      leftReasonText(t, { kind: 'set-incomplete', missing: ['a', 'b'] }),
    ).toBe('organize.left.setIncomplete{"missing":"a, b"}');
  });

  it('confidence labels', () => {
    expect(confidenceKey(1)).toBe('organize.confidence.high');
    expect(confidenceKey(0.97)).toBe('organize.confidence.high');
    expect(confidenceKey(0.95)).toBe('organize.confidence.high');
    expect(confidenceKey(0.9)).toBe('organize.confidence.medium');
    expect(confidenceKey(0.85)).toBe('organize.confidence.medium');
    expect(confidenceKey(0.84)).toBeNull();
    expect(confidenceKey(0.5)).toBeNull();
  });

  it('explains each rung of the ladder with data, not prose', () => {
    const name = (id: string) => ({ gba: 'Game Boy Advance' })[id] ?? id;
    const ev = (kind: Evidence['kind'], detail: string): Evidence => ({
      kind,
      system: 'gba',
      weight: 1,
      detail,
    });
    // Rust writes "<crc> <game>"; older mocks wrote "CRC <crc> <game>".
    expect(whyKey(ev('hash', '7B729804 Tetris Worlds (USA)'), name)).toEqual({
      key: 'organize.why.hash',
      vars: {
        crc: '7B729804',
        system: 'Game Boy Advance',
        name: 'Tetris Worlds (USA)',
      },
    });
    expect(
      whyKey(ev('hash', 'CRC 1A2B3C4D Tetris Worlds (USA)'), name).vars,
    ).toMatchObject({ crc: '1A2B3C4D', name: 'Tetris Worlds (USA)' });
    expect(whyKey(ev('header', 'GBA logo, 0x96 at 0xB2'), name)).toEqual({
      key: 'organize.why.header',
      vars: { system: 'Game Boy Advance', detail: 'GBA logo, 0x96 at 0xB2' },
    });
    expect(whyKey({ ...ev('extension', 'gba'), weight: 0.85 }, name)).toEqual({
      key: 'organize.why.extension',
      vars: { ext: 'gba', system: 'Game Boy Advance' },
    });
    expect(whyKey({ ...ev('extension', 'bin'), weight: 0.5 }, name).key).toBe(
      'organize.why.extensionShared',
    );
    expect(whyKey({ ...ev('extension', '.gba!'), weight: 0.6 }, name)).toEqual({
      key: 'organize.why.extensionContradicted',
      vars: { ext: 'gba', system: 'Game Boy Advance' },
    });
    expect(whyKey(ev('name', 'Tetris Worlds'), name)).toEqual({
      key: 'organize.why.name',
      vars: { name: 'Tetris Worlds', system: 'Game Boy Advance' },
    });
    expect(whyKey(ev('online', 'Tetris Worlds'), name).key).toBe(
      'organize.why.online',
    );
    expect(whyKey(ev('user', ''), name).key).toBe('organize.why.user');
    for (const kind of [
      'hash',
      'header',
      'extension',
      'extensionShared',
      'extensionContradicted',
      'name',
      'online',
      'user',
    ]) {
      expect(has(en, `organize.why.${kind}`), kind).toBe(true);
      expect(has(es, `organize.why.${kind}`), kind).toBe(true);
    }
    for (const k of ['high', 'medium']) {
      expect(has(en, `organize.confidence.${k}`)).toBe(true);
      expect(has(es, `organize.confidence.${k}`)).toBe(true);
    }
  });
});

describe('organise row helpers', () => {
  const t = (key: string, vars?: Record<string, string | number>) =>
    `${key}${vars ? JSON.stringify(vars) : ''}`;

  it('targetDir is the folder of the target, relative to the target root', () => {
    expect(targetDir(autoItem(0, 'a.gba', 'gba', 'ROMS/GBA'))).toBe('ROMS/GBA');
    expect(targetDir(autoItem(0, 'a.gba', 'gba', 'GBA'))).toBe('GBA');
    expect(
      targetDir({
        ...autoItem(0, 'a.gba', 'gba', 'GBA'),
        target: 'ROMS\\GBA\\a.gba',
      }),
    ).toBe('ROMS\\GBA');
  });

  it('itemNotes explains renames and notes with names, not ids', () => {
    const item = autoItem(0, 'a.gba', 'gba', 'ROMS/GBA', {
      method: 'header',
      rename: { kind: 'catalogue', from: 'a (1).gba' },
      notes: [
        { kind: 'catalogue-missing', system: 'gba' },
        { kind: 'extension-contradicted', ext: '.gb' },
        { kind: 'encrypted' },
      ],
    });
    expect(itemNotes(t, item, 'Anbernic stock', () => 'GBA!')).toEqual([
      'organize.renamedFrom{"name":"a (1).gba"}',
      'organize.noCatalogue{"system":"GBA!","level":"organize.level.header"}',
      'organize.why.extensionContradicted{"ext":"gb","system":"GBA!"}',
      'organize.encrypted',
    ]);
  });
});

describe('itemKey', () => {
  it('names an item by its primary file, whatever the case or slashes', () => {
    const a = item(0, { action: 'auto', sources: ['Roms\\Tetris (USA).GB'] });
    const b = item(7, { action: 'auto', sources: ['roms/tetris (usa).gb'] });
    expect(itemKey(a)).toBe('roms/tetris (usa).gb');
    expect(itemKey(a)).toBe(itemKey(b));
    expect(itemKey(item(1, { action: 'auto', sources: [] }))).toBe('');
  });
});
