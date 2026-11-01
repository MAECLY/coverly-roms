import { describe, expect, it } from 'vitest';

import en from '../../locales/en/ui.json';
import es from '../../locales/es/ui.json';
import { translate, type TranslateVars } from '@/i18n/core';
import type {
  DiscId,
  OrganizeReport,
  Evidence,
  ExtractEntry,
  ItemNote,
  LeftReason,
  OrganizeItem,
  OrganizePlan,
  Transfer,
} from '@/ipc/types';
import { autoItem } from '@/test/organizeFixtures';

import {
  actionOf,
  confidenceKey,
  duplicateCounts,
  extractDetail,
  extractSummary,
  groupBySystem,
  itemKey,
  itemNotes,
  layoutNote,
  leftItems,
  leftReasonKey,
  leftReasonText,
  lostTrashedCopies,
  noteKey,
  organizeDupGroups,
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
  { kind: 'not-a-rom', what: 'patch' },
  { kind: 'not-a-rom', what: 'pc-game-data' },
  { kind: 'bios' },
  { kind: 'damaged-archive' },
  { kind: 'unsupported-archive' },
  { kind: 'multi-game-archive', count: 3 },
  { kind: 'unsupported-system', name: 'Atari Jaguar' },
  { kind: 'unknown' },
  { kind: 'duplicate', of: 'Tetris Attack.zip' },
  { kind: 'already-present', at: 'ROMS/GBA/x.zip' },
  { kind: 'already-sorted', folder: 'GBA' },
  { kind: 'folder-game', system: 'doom', folder: 'prboom' },
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
    expect(leftReasonText(t, { kind: 'not-a-rom', what: 'patch' })).toBe(
      'organize.left.notARom{"what":"organize.left.patch"}',
    );
    expect(leftReasonText(t, { kind: 'not-a-rom', what: 'pc-game-data' })).toBe(
      'organize.left.notARom{"what":"organize.left.pcGameData"}',
    );
    expect(leftReasonText(t, { kind: 'duplicate', of: 'a.zip' })).toBe(
      'organize.left.duplicate{"name":"a.zip"}',
    );
    expect(
      leftReasonText(t, { kind: 'set-incomplete', missing: ['a', 'b'] }),
    ).toBe('organize.left.setIncomplete{"missing":"a, b"}');
  });

  it('a folder game names the system and its folder', () => {
    const names: Record<string, string> = { doom: 'Doom (PrBoom)' };
    const reason: LeftReason = {
      kind: 'folder-game',
      system: 'doom',
      folder: 'prboom',
    };
    const es_ = (key: string, vars?: TranslateVars) =>
      translate('es', key, vars);
    const en_ = (key: string, vars?: TranslateVars) =>
      translate('en', key, vars);
    expect(leftReasonText(es_, reason, (id) => names[id] ?? id)).toBe(
      'Juego de Doom (PrBoom) en su propia carpeta: muévela entera a prboom',
    );
    expect(leftReasonText(en_, reason, (id) => names[id] ?? id)).toBe(
      'Doom (PrBoom) game in its own folder: move the whole folder to prboom',
    );
    // Without a name lookup the id still reads.
    expect(leftReasonKey(reason).vars).toEqual({
      system: 'doom',
      folder: 'prboom',
    });
    expect(
      translate('es', 'organize.why.extensionWeak', {
        ext: 'st',
        system: 'Bandai Sufami Turbo',
      }),
    ).toBe(
      'Extensión .st: cuenta para Bandai Sufami Turbo solo dentro de su carpeta o con su cabecera',
    );
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
    // A weak extension backs a header or hash; it never decides alone.
    expect(whyKey({ ...ev('extension', 'st~'), weight: 0.5 }, name)).toEqual({
      key: 'organize.why.extensionWeak',
      vars: { ext: 'st', system: 'Game Boy Advance' },
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
      'extensionWeak',
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

  it('a contradicted extension names its owner, not the proven system (D24)', () => {
    // `.st` + the Sufami magic: the item is Sufami, the extension Atari ST's.
    const names: Record<string, string> = {
      sufami: 'Bandai Sufami Turbo',
      atarist: 'Atari ST',
    };
    const item = autoItem(0, 'Sailor Moon (Japan).st', 'sufami', 'SUFAMI', {
      method: 'header',
      notes: [{ kind: 'extension-contradicted', ext: 'st', system: 'atarist' }],
    });
    expect(
      itemNotes(
        (key, vars) => translate('es', key, vars),
        item,
        'Onion OS',
        (id) => names[id] ?? id,
      ),
    ).toEqual(['Extensión .st de Atari ST, pero la cabecera no coincide']);
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

/** The PlayStation archive of the real case: a .cue and 11 tracks. */
function psEntries(): ExtractEntry[] {
  const tracks = Array.from({ length: 11 }, (_, i) => ({
    index: i + 1,
    name: `Next Tetris, The (USA)/Next Tetris, The (USA) (Track ${String(i + 1).padStart(2, '0')}).bin`,
    size: i === 0 ? 405_748_000 - 1_000 * 10 : 1_000,
    crc32: 0x1234_0000 + i,
    target: `PS/Next Tetris, The (USA) (Track ${String(i + 1).padStart(2, '0')}).bin`,
  }));
  return [
    {
      index: 0,
      name: 'Next Tetris, The (USA)/Next Tetris, The (USA).cue',
      size: 1_493,
      crc32: 0xabcd_ef01,
      target: 'PS/Next Tetris, The (USA).cue',
    },
    ...tracks,
  ];
}

function extract(patch: Partial<Extract<Transfer, { kind: 'extract' }>> = {}) {
  return {
    kind: 'extract',
    entries: psEntries(),
    bytes: 405_749_493,
    archive_after: 'trash',
    source: { kind: 'builtin' },
    sheet_check: 'read',
    m3u: null,
    ...patch,
  } as Transfer;
}

function discItem(id: number, patch: Partial<OrganizeItem> = {}) {
  return autoItem(id, 'Next Tetris, The.zip', 'psx', 'PS', {
    target: 'PS/Next Tetris, The (USA).cue',
    transfer: extract(),
    ...patch,
  });
}

const NEW_REASONS: [LeftReason, string, Record<string, string | number>][] = [
  [
    { kind: 'folder-game', system: 'doom', folder: 'prboom' },
    'organize.left.folderGame',
    { system: 'doom', folder: 'prboom' },
  ],
  [
    { kind: 'unsupported-archive', format: 'rar', need: 'unrar' },
    'organize.left.unsupportedArchiveTool',
    { format: 'RAR', tool: 'unrar' },
  ],
  [
    { kind: 'unsupported-archive', format: 'rar', need: 'seven-zip' },
    'organize.left.unsupportedArchiveTool',
    { format: 'RAR', tool: '7-Zip' },
  ],
  [
    { kind: 'unsupported-archive', format: 'tar.lz4', need: 'bsdtar' },
    'organize.left.unsupportedArchiveTool',
    { format: 'TAR.LZ4', tool: 'bsdtar' },
  ],
  [
    { kind: 'unsupported-archive', format: 'arj' },
    'organize.left.unsupportedArchive',
    {},
  ],
  [{ kind: 'encrypted-archive' }, 'organize.left.encryptedArchive', {}],
  [
    { kind: 'archive-needs-memory', mib: 1536 },
    'organize.left.archiveNeedsMemory',
    { mib: 1536 },
  ],
  [
    { kind: 'archive-too-big', entries: 2_000_000, bytes: null },
    'organize.left.archiveTooBig',
    {},
  ],
  [
    { kind: 'multi-volume-archive', parts: 3 },
    'organize.left.multiVolume',
    { parts: 3 },
  ],
  [
    { kind: 'unsafe-archive-entry', name: '../../etc/passwd', why: 'dot-dot' },
    'organize.left.unsafeArchiveEntry',
    { name: '../../etc/passwd' },
  ],
  [
    { kind: 'set-unsafe-name', name: 'Track 01?.bin' },
    'organize.left.setUnsafeName',
    { name: 'Track 01?.bin' },
  ],
  [{ kind: 'path-too-long' }, 'organize.left.pathTooLong', {}],
  [{ kind: 'nested-archive' }, 'organize.left.nestedArchive', {}],
  [{ kind: 'repack-arcade' }, 'organize.left.repackArcade', {}],
  [
    { kind: 'folder-full', folder: 'PS' },
    'organize.left.folderFull',
    { folder: 'PS' },
  ],
  [{ kind: 'unsafe-target' }, 'organize.left.unsafeTarget', {}],
];

const NEW_NOTES: [ItemNote, string, Record<string, string | number>][] = [
  [
    { kind: 'container-mismatch', ext: 'zip', actual: '7z' },
    'organize.note.containerMismatch',
    { actual: '7z', ext: 'zip' },
  ],
  [
    { kind: 'twin-sheets', kept: ['gdi', 'cue'] },
    'organize.note.twinSheets',
    {},
  ],
  [{ kind: 'no-stored-checksum' }, 'organize.note.noStoredChecksum', {}],
  [{ kind: 'archive-not-fully-read' }, 'organize.note.notFullyRead', {}],
  [
    { kind: 'archive-kept-leftovers', count: 2, examples: ['a.sav', 'b.txt'] },
    'organize.note.archiveKeptLeftovers',
    { count: 2 },
  ],
  [{ kind: 'sheet-not-read-yet' }, 'organize.note.sheetNotReadYet', {}],
  [
    { kind: 'multi-rom-extract', count: 3 },
    'organize.note.multiRomExtract',
    { count: 3 },
  ],
];

function placeholders(s: string): string[] {
  return [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]!).sort();
}

function lookup(catalog: object, key: string): string {
  let node: unknown = catalog;
  for (const part of key.split('.'))
    node = (node as Record<string, unknown>)[part];
  return node as string;
}

describe('organise extraction helpers', () => {
  it('action_of_extract', () => {
    expect(actionOf(discItem(0), 'move')).toBe('extract');
    expect(actionOf(discItem(0), 'copy')).toBe('extract');
    const asIs = autoItem(1, 'Tetris (World).zip', 'gb', 'GB');
    expect(actionOf(asIs, 'move')).toBe('move');
    expect(actionOf(asIs, 'copy')).toBe('copy');
  });

  it('extract_detail_counts_and_bytes', () => {
    // The real PlayStation archive: a .cue and 11 tracks, 405,749,493 B.
    expect(extractDetail(discItem(0))).toEqual({
      count: 12,
      bytes: 405_749_493,
    });
    // A generated playlist is not a file of the archive (CLI counts alike).
    expect(
      extractDetail(discItem(0, { transfer: extract({ m3u: 'PS/x.m3u' }) })),
    ).toEqual({ count: 12, bytes: 405_749_493 });
    expect(extractDetail(autoItem(1, 'a.gb', 'gb', 'GB'))).toBeNull();
  });

  it('extract_summary_counts_games', () => {
    const p = plan([
      discItem(0),
      discItem(1, {
        transfer: extract({ bytes: 591_881_790 }),
        folder: 'SATURN',
      }),
      autoItem(2, 'a.gb', 'gb', 'GB'),
      // Undecided and left items never extract.
      { ...discItem(3), action: 'review' } as OrganizeItem,
      {
        ...discItem(4),
        action: 'left',
        reason: { kind: 'collision-set' },
      } as OrganizeItem,
    ]);
    expect(extractSummary(p)).toEqual({
      count: 2,
      bytes: 405_749_493 + 591_881_790,
    });
    expect(extractSummary(plan([autoItem(0, 'a.gb', 'gb', 'GB')]))).toBeNull();
  });

  it('an_archive_set_aside_as_a_duplicate_is_not_extracted', () => {
    const p = plan([
      discItem(0),
      {
        ...discItem(1),
        action: 'set-aside',
        keeper: 'ROMS/PS/Game.cue',
        relation: 'format',
        reason: { kind: 'tie' },
        dest: '_Duplicados/Game.zip',
      } as OrganizeItem,
    ]);
    expect(extractSummary(p)).toEqual({ count: 1, bytes: 405_749_493 });
  });

  it('duplicate_counts_set_aside_and_reported_copies', () => {
    const p = {
      ...plan([
        item(0, { action: 'auto' }),
        item(1, {
          action: 'set-aside',
          keeper: 'a',
          relation: 'exact',
          reason: { kind: 'tie' },
          dest: '_Duplicados/b',
        } as Partial<OrganizeItem> & Pick<OrganizeItem, 'action'>),
        item(2, {
          action: 'left',
          reason: { kind: 'in-library', path: 'ROMS/GB/c', relation: 'exact' },
        } as Partial<OrganizeItem> & Pick<OrganizeItem, 'action'>),
        item(3, {
          action: 'left',
          reason: { kind: 'unknown' },
        } as Partial<OrganizeItem> & Pick<OrganizeItem, 'action'>),
      ]),
      counts: { in_library: 1 },
    } as OrganizePlan;
    expect(duplicateCounts(p)).toEqual({ extras: 2, inLibrary: 1 });
  });

  it('organize_dup_groups_write_where_each_copy_goes', () => {
    const member = (id: number, primary: string) => ({
      id,
      unit: {
        primary,
        files: [primary],
        kind: 'single' as const,
        mtime_ns: 0,
        in_library: false,
      },
      relation: 'exact' as const,
      size: 1,
      travels: [],
      stays: [],
      dest: null,
      catalogue_name: null,
      serials: [],
      confidence: 1,
    });
    const p = {
      ...plan([
        item(0, { action: 'auto', sources: ['a.gb'] }),
        item(1, {
          action: 'set-aside',
          sources: ['sub/b.gb'],
          keeper: 'ROMS/GB/a.gb',
          relation: 'exact',
          reason: { kind: 'tie' },
          dest: '_Duplicados/sub/b.gb',
        } as Partial<OrganizeItem> & Pick<OrganizeItem, 'action'>),
        item(2, {
          action: 'set-aside',
          sources: ['GB/c.gb'],
          keeper: 'ROMS/GB/c.gb',
          relation: 'exact',
          reason: { kind: 'in-library' },
          dest: '/Volumes/SD/_Duplicados/GB/c.gb',
        } as Partial<OrganizeItem> & Pick<OrganizeItem, 'action'>),
      ]),
      source_root: '/pile',
      duplicates: [
        {
          id: 7,
          system: 'gb',
          title: 'A',
          keeper: 0,
          reason: { kind: 'tie' },
          runner_up: null,
          members: [member(0, '/pile/a.gb'), member(1, '/pile/sub/b.gb')],
          action: { kind: 'act' },
          tie: true,
          choice_dropped: false,
        },
        {
          id: 8,
          system: 'gb',
          title: 'C',
          keeper: 0,
          reason: { kind: 'in-library' },
          runner_up: null,
          members: [
            { ...member(0, '/Volumes/SD/ROMS/GB/c.gb'), dest: null },
            member(1, '/pile/GB/c.gb'),
          ],
          action: { kind: 'act' },
          tie: false,
          choice_dropped: false,
        },
      ],
    } as unknown as OrganizePlan;
    const [a, c] = organizeDupGroups(p);
    expect(a!.quarantine).toBe('/pile/_Duplicados');
    expect(a!.setAside).toBe(true);
    expect(a!.group.members.map((m) => m.dest)).toEqual([
      null,
      '/pile/_Duplicados/sub/b.gb',
    ]);
    expect(c!.group.members[1]!.dest).toBe('/Volumes/SD/_Duplicados/GB/c.gb');
    expect(c!.quarantine).toBe('/Volumes/SD/_Duplicados');
    // Report mode: the copies stay, nothing to write.
    const reported = organizeDupGroups({
      ...p,
      items: p.items.map((i) =>
        i.action === 'set-aside'
          ? ({
              ...i,
              action: 'left',
              reason: { kind: 'in-library', path: 'x', relation: 'exact' },
            } as OrganizeItem)
          : i,
      ),
    });
    expect(reported.every((g) => !g.setAside)).toBe(true);
  });

  it('every_new_left_reason_has_a_key', () => {
    for (const [reason, key, vars] of NEW_REASONS) {
      expect(leftReasonKey(reason), reason.kind).toEqual({ key, vars });
      expect(has(en, key), `en ${key}`).toBe(true);
      expect(has(es, key), `es ${key}`).toBe(true);
      // Every placeholder the copy needs is filled.
      expect(placeholders(lookup(es, key))).toEqual(Object.keys(vars).sort());
    }
    // An archive this build cannot open is never "damaged".
    for (const [reason] of NEW_REASONS)
      expect(leftReasonKey(reason).key).not.toBe(
        'organize.left.damagedArchive',
      );
  });

  it('every_new_note_has_a_key', () => {
    for (const [note, key, vars] of NEW_NOTES) {
      expect(noteKey(note), note.kind).toEqual({ key, vars });
      expect(has(en, key), `en ${key}`).toBe(true);
      expect(has(es, key), `es ${key}`).toBe(true);
      expect(placeholders(lookup(es, key))).toEqual(Object.keys(vars).sort());
    }
    const t = (key: string, vars?: Record<string, string | number>) =>
      `${key}${vars ? JSON.stringify(vars) : ''}`;
    const row = discItem(0, {
      notes: [
        { kind: 'container-mismatch', ext: '.zip', actual: '7z' },
        { kind: 'twin-sheets', kept: ['gdi', 'cue'] },
      ],
    });
    expect(itemNotes(t, row, 'Anbernic stock', () => 'PlayStation')).toEqual([
      'organize.note.containerMismatch{"actual":"7z","ext":"zip"}',
      'organize.note.twinSheets{}',
    ]);
  });

  it('layout_note_names_flat_folder_and_m3u', () => {
    const flat = {
      disc_layout: { kind: 'flat', m3u_for_multi: false },
    } as const;
    const folder = {
      disc_layout: { kind: 'game-folder', m3u_for_multi: true },
    } as const;
    expect(layoutNote(discItem(0), flat)).toEqual([
      { key: 'organize.extract.layoutFlat', vars: { folder: 'PS' } },
    ]);
    // Absent = flat (profiles::DiscLayout default).
    expect(layoutNote(discItem(0), {})).toEqual(layoutNote(discItem(0), flat));
    const inFolder = discItem(1, {
      target: 'psx\\Colony Wars\\Colony Wars (Disc 1).cue',
      disc: {
        serials: ['SCUS-94181'],
        disc_no: 1,
        volume: null,
        source: 'system-cnf',
      },
      transfer: extract({ m3u: 'psx\\Colony Wars\\Colony Wars.m3u' }),
    });
    expect(layoutNote(inFolder, folder)).toEqual([
      {
        key: 'organize.extract.layoutFolder',
        vars: { folder: 'psx\\Colony Wars' },
      },
      { key: 'organize.extract.m3u', vars: { name: 'Colony Wars.m3u' } },
    ]);
    // A cartridge extracted from a .tar.gz names only its folder.
    const cart = autoItem(2, 'Tetris DX.tar.gz', 'gbc', 'GBC', {
      kind: 'archive',
      target: 'GBC/Tetris DX (World).gbc',
      transfer: extract({
        entries: [
          {
            index: 0,
            name: 'Tetris DX (World).gbc',
            size: 1_048_576,
            crc32: 1,
            target: 'GBC/Tetris DX (World).gbc',
          },
        ],
        bytes: 1_048_576,
      }),
    });
    expect(layoutNote(cart, folder)).toEqual([
      { key: 'organize.extract.into', vars: { folder: 'GBC' } },
    ]);
    expect(layoutNote(autoItem(3, 'a.gb', 'gb', 'GB'), flat)).toEqual([]);
    for (const k of ['layoutFlat', 'layoutFolder', 'm3u', 'into'])
      expect(
        has(es, `organize.extract.${k}`) && has(en, `organize.extract.${k}`),
        k,
      ).toBe(true);
  });

  it('spanish_copy_matches_the_spec', () => {
    expect(lookup(es, 'organize.action.extract')).toBe('Extraer');
    expect(lookup(es, 'organize.extract.detail')).toBe(
      '{count} archivos · {bytes}',
    );
    expect(lookup(es, 'organize.extract.summary')).toBe(
      'Se extraen: {count} juegos ({bytes})',
    );
    expect(lookup(es, 'organize.result.extracted')).toBe('Extraídos: {count}');
    expect(lookup(es, 'organize.note.containerMismatch')).toBe(
      'Es un archivo {actual} con extensión .{ext}',
    );
    expect(lookup(en, 'organize.left.unsupportedArchiveTool')).toBe(
      'To open {format} archives, install {tool}',
    );
  });
});

describe('organise disc and tool copy', () => {
  it('disc_header_reason_names_the_serial', () => {
    const name = (id: string) => ({ ps: 'PlayStation' })[id] ?? id;
    const ev: Evidence = {
      kind: 'header',
      system: 'ps',
      weight: 0.75,
      detail: 'Licensed by Sony Computer Entertainment America',
    };
    const disc: DiscId = {
      serials: ['SLUS-00862'],
      disc_no: null,
      volume: null,
      source: 'system-cnf',
    };
    expect(whyKey(ev, name, disc)).toEqual({
      key: 'organize.why.disc',
      vars: {
        system: 'PlayStation',
        detail: 'Licensed by Sony Computer Entertainment America',
        serial: 'SLUS-00862',
      },
    });
    // Several discs list every serial; none keeps the header wording.
    expect(
      whyKey(ev, name, { ...disc, serials: ['SCUS-94181', 'SCUS-94182'] }).vars
        .serial,
    ).toBe('SCUS-94181, SCUS-94182');
    expect(whyKey(ev, name, { ...disc, serials: [] }).key).toBe(
      'organize.why.header',
    );
    expect(whyKey(ev, name, null).key).toBe('organize.why.header');
    // Only a header clue is a disc header.
    expect(whyKey({ ...ev, kind: 'name' }, name, disc).key).toBe(
      'organize.why.name',
    );
    expect(has(es, 'organize.why.disc') && has(en, 'organize.why.disc')).toBe(
      true,
    );
  });

  it('tool_hint_never_shows_a_diagnostic_as_the_format', () => {
    // The core puts "<Tool> at <path> (<why>)" in `format` when a found
    // tool is refused; that is no format name to show the user.
    expect(
      leftReasonKey({
        kind: 'unsupported-archive',
        format: 'Unrar at /opt/homebrew/bin/unrar (not executable)',
        need: 'unrar',
      }),
    ).toEqual({
      key: 'organize.left.unsupportedArchiveInstall',
      vars: { tool: 'unrar' },
    });
    expect(
      leftReasonKey({ kind: 'unsupported-archive', need: 'seven-zip' }),
    ).toEqual({
      key: 'organize.left.unsupportedArchiveInstall',
      vars: { tool: '7-Zip' },
    });
    // 7z keeps its own spelling.
    expect(
      leftReasonKey({
        kind: 'unsupported-archive',
        format: '7z',
        need: 'seven-zip',
      }).vars,
    ).toEqual({ format: '7z', tool: '7-Zip' });
    for (const cat of [es, en])
      expect(
        placeholders(lookup(cat, 'organize.left.unsupportedArchiveInstall')),
      ).toEqual(['tool']);
  });
  it('lost_trashed_copies_tell_duplicates_from_archives', () => {
    const p = {
      ...plan([
        item(0, {
          action: 'set-aside',
          sources: ['dup\\a.gb'],
        } as Partial<OrganizeItem> & Pick<OrganizeItem, 'action'>),
        item(1, { action: 'auto', sources: ['b.zip'] }),
      ]),
      source_root: 'C:\\Pile',
    };
    const report = {
      items: [
        { item: 0, status: 'trashed', targets: [] },
        { item: 1, status: 'extracted', targets: [] },
      ],
    } as unknown as OrganizeReport;
    const lost = ['C:\\Pile\\dup\\a.gb', 'C:\\Pile\\b.zip'];
    // Windows separators and case do not hide the copy.
    expect([...lostTrashedCopies(lost, report, p)]).toEqual([lost[0]]);
    // Without the plan: only a job that extracted nothing is sure.
    expect(lostTrashedCopies(lost, report, null).size).toBe(0);
    const onlyCopies = {
      items: [{ item: 0, status: 'trashed', targets: [] }],
    } as unknown as OrganizeReport;
    expect(lostTrashedCopies(lost, onlyCopies, null).size).toBe(2);
    // Nothing went to the Trash as a duplicate: every loss is an archive.
    expect(
      lostTrashedCopies(lost, { items: [] } as unknown as OrganizeReport, p)
        .size,
    ).toBe(0);
  });
});
