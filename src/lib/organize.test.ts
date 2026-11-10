import { describe, expect, it } from 'vitest';

import en from '../../locales/en/ui.json';
import es from '../../locales/es/ui.json';
import { translate, type TranslateVars } from '@/i18n/core';
import type {
  AddonLink,
  DiscId,
  LinkVia,
  OrganizeReport,
  TranslatedGameView,
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
  addonText,
  addonViaKey,
  childTarget,
  confidenceKey,
  contentIdKey,
  duplicateCounts,
  extractDetail,
  extractSummary,
  FIXED_FOLDER_DISK,
  fixedFolderLabel,
  gameFolderPreview,
  groupBySystem,
  itemKey,
  encryptedName,
  itemNotes,
  keyedText,
  layoutNote,
  leftItems,
  leftReasonKey,
  leftReasonText,
  licenceLine,
  licencesStaying,
  lostTrashedCopies,
  noteKey,
  organizeDupGroups,
  playlistKey,
  PREVIEW_NAMES,
  reviewBuckets,
  ROWS_PER_GROUP,
  rowNotes,
  targetDir,
  titleIdKey,
  titleSourceKey,
  translatedFor,
  translateNoteKey,
  translatedPreview,
  translatedTitle,
  updatesSummaryKey,
  updatesUnderBases,
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
  { kind: 'key-material' },
  { kind: 'licence-of', of: 'ps3/Resistance.pkg' },
  { kind: 'emulator-data', folder: 'ux0' },
  { kind: 'not-a-rom', what: 'installer' },
  { kind: 'not-a-rom', what: 'model3d' },
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

  it('maps every addon reason and note to its own key in both locales', () => {
    const reasons: [LeftReason, string][] = [
      [
        {
          kind: 'addon-unlinked',
          addon: 'patch',
          format: 'ips',
          why: { kind: 'no-key' },
        },
        'organize.left.addonUnlinked',
      ],
      [
        {
          kind: 'addon-unlinked',
          addon: 'mod',
          format: 'cemu-graphic-pack',
          why: { kind: 'several', count: 2 },
        },
        'organize.left.addonSeveral',
      ],
      [
        {
          kind: 'addon-unlinked',
          addon: 'cheat',
          format: 'rpcs3-patch',
          why: { kind: 'many', count: 14 },
        },
        'organize.left.addonMany',
      ],
      [
        {
          kind: 'addon-unlinked',
          addon: 'mod',
          format: 'layered-fs',
          why: { kind: 'partial' },
        },
        'organize.left.addonPart',
      ],
      [
        { kind: 'addon-name-taken', at: 'PS1/Game/Cheats' },
        'organize.left.addonNameTaken',
      ],
      [{ kind: 'addon-no-place', addon: 'mod' }, 'organize.left.addonNoPlace'],
      [
        { kind: 'addon-for-emulator', emulator: 'DuckStation', path: 'cheats' },
        'organize.left.addonForEmulator',
      ],
      [
        { kind: 'not-supported', emulator: 'RPCS3', why: 'rpcs3-mods' },
        'organize.left.notSupported',
      ],
      [
        { kind: 'patch-not-applied', emulator: 'RetroArch' },
        'organize.left.patchNotApplied',
      ],
    ];
    for (const [reason, want] of reasons) {
      const { key, vars } = leftReasonKey(reason);
      expect(key, reason.kind).toBe(want);
      expect(has(en, key) && has(es, key), key).toBe(true);
      for (const v of Object.values(vars))
        if (typeof v === 'string' && v.startsWith('organize.'))
          expect(has(en, v) && has(es, v), v).toBe(true);
    }
    const notes: [ItemNote, string | null][] = [
      [{ kind: 'mod-of', of: 'Zelda' }, 'organize.note.modOf'],
      [{ kind: 'patch-of', of: 'Zelda' }, 'organize.note.patchOf'],
      [{ kind: 'cheat-of', of: 'Zelda' }, 'organize.note.cheatOf'],
      [
        { kind: 'header-mismatch', needs: 'needs-headered' },
        'organize.addon.headered',
      ],
      [
        { kind: 'header-mismatch', needs: 'needs-headerless' },
        'organize.addon.headerless',
      ],
      [{ kind: 'header-mismatch', needs: 'matches' }, null],
      [
        { kind: 'point-emulator-here', emulator: 'shadPS4', folder: 'addcont' },
        'organize.note.pointEmulatorHere',
      ],
    ];
    for (const [note, want] of notes) {
      const keyed = noteKey(note);
      expect(keyed?.key ?? null, note.kind).toBe(want);
      if (keyed) expect(has(en, keyed.key) && has(es, keyed.key)).toBe(true);
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

describe('systems batch 2 copy (spec §10)', () => {
  const t = (key: string, vars?: TranslateVars) =>
    translate('es', key, vars ?? {});
  const tEn = (key: string, vars?: TranslateVars) =>
    translate('en', key, vars ?? {});

  it('key material and emulator data read as the spec says', () => {
    expect(leftReasonText(t, { kind: 'key-material' })).toBe(
      'Archivo de claves o licencia: Coverly no lo lee y lo deja aquí',
    );
    expect(leftReasonText(tEn, { kind: 'key-material' })).toBe(
      'Key or licence file: Coverly never reads it and leaves it here',
    );
    expect(
      leftReasonText(t, { kind: 'licence-of', of: 'ps3/Resistance.pkg' }),
    ).toBe('Licencia de Resistance.pkg: va con su juego; Coverly no la abre');
    expect(
      leftReasonText(tEn, { kind: 'licence-of', of: 'ps3\\Resistance.pkg' }),
    ).toBe(
      'Licence of Resistance.pkg: goes with its game; Coverly does not open it',
    );
    expect(leftReasonText(t, { kind: 'emulator-data', folder: 'ux0' })).toBe(
      'Está dentro de los datos de un emulador (ux0); se deja tal cual',
    );
    expect(leftReasonKey({ kind: 'emulator-data', folder: 'ux0' })).toEqual({
      key: 'organize.left.emulatorData',
      vars: { folder: 'ux0' },
    });
  });

  it('installers and 3D models fill the not-a-ROM sentence', () => {
    expect(leftReasonText(t, { kind: 'not-a-rom', what: 'installer' })).toBe(
      'No es una ROM (instalador de programas)',
    );
    expect(leftReasonText(t, { kind: 'not-a-rom', what: 'model3d' })).toBe(
      'No es una ROM (modelo 3D)',
    );
    expect(leftReasonText(tEn, { kind: 'not-a-rom', what: 'model3d' })).toBe(
      'Not a ROM (3D model)',
    );
  });

  it('every new note has its sentence', () => {
    const cases: [ItemNote, string][] = [
      { kind: 'update', of: 'Super Mario Odyssey' },
      { kind: 'dlc', of: 'Super Mario Odyssey' },
      { kind: 'encrypted-content', emulator: 'Azahar' },
      { kind: 'install-first', emulator: 'RPCS3' },
      {
        kind: 'folder-suffix',
        name: "Demon's Souls (BLUS-30443).ps3",
        layout: 'ES-DE',
      },
    ].map((n, i) => [
      n as ItemNote,
      [
        'Actualización de Super Mario Odyssey',
        'Contenido descargable de Super Mario Odyssey',
        'Copia cifrada: Azahar solo abre copias descifradas',
        'RPCS3 lo instala; no se abre desde la carpeta',
        "Se renombra a Demon's Souls (BLUS-30443).ps3 para que ES-DE lo muestre",
      ][i]!,
    ]);
    for (const [note, text] of cases) {
      const item = autoItem(0, 'x', 'switch', 'switch', { notes: [note] });
      expect(itemNotes(t, item, 'ES-DE', (id) => id)).toEqual([text]);
    }
    expect(noteKey({ kind: 'update', of: 'Halo' })).toEqual({
      key: 'organize.note.update',
      vars: { title: 'Halo' },
    });
  });

  it('an update or DLC says whose it is before any other note', () => {
    // The row sits under its base; the first line under it must say why.
    const row = autoItem(0, 'x.nsp', 'switch', 'switch', {
      rename: { kind: 'path-length' },
      notes: [
        { kind: 'no-folder-on-device', folder: 'SWITCH' },
        { kind: 'dlc', of: 'Super Mario Odyssey' },
      ],
    });
    const lines = itemNotes(t, row, 'Anbernic stock', (id) => id);
    expect(lines[0]).toBe('Contenido descargable de Super Mario Odyssey');
    expect(lines).toHaveLength(3);
    expect(rowNotes(t, row, 'Anbernic stock', (id) => id).inline[0]).toBe(
      'Contenido descargable de Super Mario Odyssey',
    );
  });

  it('a title id line names its file, the ticket or the file name', () => {
    expect(titleIdKey({ id: 'BLUS30443', source: 'param-sfo' })).toEqual({
      key: 'organize.why.titleIdFile',
      vars: { id: 'BLUS30443', file: 'PARAM.SFO' },
    });
    const text = (r: Parameters<typeof titleIdKey>[0], tt = t) => {
      const k = titleIdKey(r);
      return k ? tt(k.key, k.vars) : null;
    };
    expect(text({ id: 'BLUS30443', source: 'param-sfo' })).toBe(
      'Title ID BLUS30443 en PARAM.SFO',
    );
    expect(text({ id: '0100000000010000', source: 'ticket-name' })).toBe(
      'Title ID 0100000000010000 en el nombre del ticket',
    );
    expect(text({ id: '0100000000010000', source: 'file-name' })).toBe(
      'Title ID 0100000000010000 tomado del nombre del archivo',
    );
    expect(text({ id: '0100000000010000', source: 'ticket-name' }, tEn)).toBe(
      'Title ID 0100000000010000 in the ticket name',
    );
    // `{file}` only ever comes from the fixed set.
    const files = (
      [
        'param-sfo',
        'meta-xml',
        'app-xml',
        'default-xbe',
        'default-xex',
        'cnmt-xml',
      ] as const
    ).map((source) => titleIdKey({ id: 'X', source })!.vars.file);
    expect(files).toEqual([
      'PARAM.SFO',
      'meta.xml',
      'app.xml',
      'default.xbe',
      'default.xex',
      'cnmt.xml',
    ]);
    // A header id is already told by the header line.
    expect(titleIdKey({ id: 'GM8E01', source: 'header' })).toBeNull();
  });

  it('the updates summary picks the singular for one', () => {
    expect(updatesSummaryKey({ updates: 1 } as OrganizePlan)).toEqual({
      key: 'organize.summaryUpdatesOne',
      vars: {},
    });
    const many = updatesSummaryKey({ updates: 3 } as OrganizePlan)!;
    expect(t(many.key, many.vars)).toBe(
      'Incluye 3 actualizaciones o contenidos descargables',
    );
    expect(t('organize.summaryUpdatesOne')).toBe(
      'Incluye 1 actualización o contenido descargable',
    );
    expect(updatesSummaryKey({} as OrganizePlan)).toBeNull();
    expect(updatesSummaryKey({ updates: 0 } as OrganizePlan)).toBeNull();
  });

  it('the new copy exists in both locales with the same placeholders', () => {
    for (const key of [
      'organize.left.keyMaterial',
      'organize.left.licenceOf',
      'organize.left.emulatorData',
      'organize.left.installer',
      'organize.left.model3d',
      'organize.why.titleIdFile',
      'organize.why.titleIdName',
      'organize.why.titleIdTicket',
      'organize.note.update',
      'organize.note.dlc',
      'organize.note.encryptedContent',
      'organize.note.installFirst',
      'organize.note.folderSuffix',
      'organize.summaryUpdates',
      'organize.summaryUpdatesOne',
      'organize.item.folder',
      'settings.sources.hint.embedded',
    ]) {
      expect(has(es, key) && has(en, key), key).toBe(true);
      expect(placeholders(lookup(en, key)), key).toEqual(
        placeholders(lookup(es, key)),
      );
    }
  });
});

describe('updatesUnderBases', () => {
  const sw = (id: number, name: string, extra: Partial<OrganizeItem> = {}) =>
    autoItem(id, name, 'switch', 'switch', extra);
  const names = (items: OrganizeItem[]) => items.map((i) => i.sources[0]);

  it('puts updates and DLC under the base they name, by title or title id', () => {
    const dlc = sw(1, 'Odyssey DLC.nsp', {
      notes: [{ kind: 'dlc', of: '0100000000010000' }],
    });
    const update = sw(2, 'SMO [v393216].nsp', {
      notes: [{ kind: 'update', of: 'Super Mario Odyssey' }],
    });
    const other = sw(3, 'Mario Kart 8.nsp');
    const base = sw(4, 'Super Mario Odyssey [0100000000010000][v0].nsp', {
      title_id: { id: '0100000000010000', source: 'ticket-name' },
    });
    expect(names(updatesUnderBases([dlc, update, other, base]))).toEqual([
      'Mario Kart 8.nsp',
      'Super Mario Odyssey [0100000000010000][v0].nsp',
      'Odyssey DLC.nsp',
      'SMO [v393216].nsp',
    ]);
  });

  it('an update whose base is not in the group keeps its place', () => {
    const orphan = sw(1, 'Zelda [v2].nsp', {
      notes: [{ kind: 'update', of: '01007EF00011E000' }],
    });
    const other = sw(2, 'Mario Kart 8.nsp');
    const items = [orphan, other];
    expect(updatesUnderBases(items)).toBe(items);
  });

  it('the first base with the name wins, and game folders keep their dots', () => {
    const first = sw(1, 'Game. Part One', { kind: 'folder' });
    const second = sw(2, 'Game. Part One (Rev 1).nsp');
    const update = sw(3, 'patch.nsp', {
      notes: [{ kind: 'update', of: 'Game. Part One' }],
    });
    expect(names(updatesUnderBases([update, first, second]))).toEqual([
      'Game. Part One',
      'patch.nsp',
      'Game. Part One (Rev 1).nsp',
    ]);
  });

  it('stays linear on a large library (one lookup per update)', () => {
    const items: OrganizeItem[] = [];
    const n = 8000;
    for (let i = 0; i < n; i++) items.push(sw(i, `Game ${i} [v0].nsp`));
    for (let i = 0; i < n; i++)
      items.push(
        sw(n + i, `Game ${i} [v1].nsp`, {
          notes: [{ kind: 'update', of: `Game ${i}` }],
        }),
      );
    const start = performance.now();
    const out = updatesUnderBases(items);
    const elapsed = performance.now() - start;
    expect(out[1]!.sources[0]).toBe('Game 0 [v1].nsp');
    expect(out[2 * n - 1]!.sources[0]).toBe(`Game ${n - 1} [v1].nsp`);
    // Quadratic matching took seconds here; a map takes milliseconds.
    expect(elapsed).toBeLessThan(1000);
  });
});

describe('systems batch 3 copy (spec §10)', () => {
  const t = (key: string, vars?: TranslateVars) =>
    translate('es', key, vars ?? {});
  const tEn = (key: string, vars?: TranslateVars) =>
    translate('en', key, vars ?? {});

  it('a mods folder and a console system file read as the spec says', () => {
    const overlay: LeftReason = { kind: 'game-overlay', of: 'Bloodborne' };
    expect(leftReasonKey(overlay)).toEqual({
      key: 'organize.left.gameOverlay',
      vars: { title: 'Bloodborne' },
    });
    expect(leftReasonText(t, overlay)).toBe(
      'Carpeta de mods o parches de Bloodborne: se deja donde está',
    );
    expect(leftReasonText(tEn, overlay)).toBe(
      'Mods or patch folder of Bloodborne: left where it is',
    );
    const system: LeftReason = { kind: 'not-a-rom', what: 'console-system' };
    expect(leftReasonText(t, system)).toBe(
      'No es una ROM (archivo del sistema de una consola)',
    );
    expect(leftReasonText(tEn, system)).toBe('Not a ROM (console system file)');
  });

  it('every new note has its sentence in both languages', () => {
    const layout = 'Batocera / Knulli / EmulationStation';
    const cases: [ItemNote, string, string][] = [
      [
        { kind: 'extract-first' },
        'Este paquete no se abre así: hay que extraerlo antes',
        'This package does not open as is: it has to be extracted first',
      ],
      [
        { kind: 'no-emulator' },
        'Ningún emulador lo abre así',
        'No emulator opens it in this form',
      ],
      [
        { kind: 'pro-enhanced' },
        'Mejorado para PS5 Pro',
        'Enhanced for PS5 Pro',
      ],
      [
        { kind: 'dlc-folder', layout },
        `Va a la carpeta DLC, donde ${layout} busca el contenido descargable`,
        `Goes to the DLC folder, where ${layout} looks for downloadable content`,
      ],
    ];
    for (const [note, esText, enText] of cases) {
      const keyed = noteKey(note)!;
      expect(t(keyed.key, keyed.vars), note.kind).toBe(esText);
      expect(tEn(keyed.key, keyed.vars), note.kind).toBe(enText);
      const row = autoItem(0, 'x', 'ps4', 'ps4', { notes: [note] });
      expect(itemNotes(t, row, layout, (id) => id)).toEqual([esText]);
    }
  });

  it('a title id names param.json, MicrosoftGame.config or AppxManifest.xml', () => {
    const text = (source: Parameters<typeof titleIdKey>[0]['source']) => {
      const k = titleIdKey({ id: 'PPSA01284', source })!;
      return t(k.key, k.vars);
    };
    expect(text('param-json')).toBe('Title ID PPSA01284 en param.json');
    expect(text('microsoft-game-config')).toBe(
      'Title ID PPSA01284 en MicrosoftGame.config',
    );
    expect(text('appx-manifest')).toBe(
      'Title ID PPSA01284 en AppxManifest.xml',
    );
    // A PKG entry's param.sfo is still PARAM.SFO.
    expect(text('param-sfo')).toBe('Title ID PPSA01284 en PARAM.SFO');
  });

  it('a content id is its own why line, codes untouched', () => {
    const row = autoItem(0, 'CUSA00900', 'ps4', 'ps4', {
      content_id: 'UP9000-CUSA00900_00-BLOODBORNE000000',
    });
    const keyed = contentIdKey(row)!;
    expect(keyed).toEqual({
      key: 'organize.why.contentId',
      vars: { id: 'UP9000-CUSA00900_00-BLOODBORNE000000' },
    });
    expect(t(keyed.key, keyed.vars)).toBe(
      'Content ID UP9000-CUSA00900_00-BLOODBORNE000000',
    );
    expect(tEn(keyed.key, keyed.vars)).toBe(
      'Content ID UP9000-CUSA00900_00-BLOODBORNE000000',
    );
    expect(contentIdKey(autoItem(1, 'x', 'ps4', 'ps4'))).toBeNull();
    // An empty id is no id.
    expect(
      contentIdKey(autoItem(2, 'g', 'xboxone', 'xboxone', { content_id: '' })),
    ).toBeNull();
  });
});

describe('adjustments copy (spec §10)', () => {
  const t = (key: string, vars?: TranslateVars) =>
    translate('es', key, vars ?? {});
  const tEn = (key: string, vars?: TranslateVars) =>
    translate('en', key, vars ?? {});
  const text = (k: { key: string; vars: TranslateVars }) => [
    t(k.key, k.vars),
    tEn(k.key, k.vars),
  ];

  it('a game part and a Vita backup say why they stay', () => {
    const part: LeftReason = {
      kind: 'game-part',
      part: 'exe-fs',
      system: 'switch',
      title_id: null,
    };
    expect(text(leftReasonKey(part))).toEqual([
      'Parte de un juego; no se mueve suelta',
      'Part of a game; not moved on its own',
    ]);
    for (const p of ['nca', 'library', 'title-meta', 'module'] as const)
      expect(leftReasonKey({ ...part, part: p }).key).toBe(
        'organize.left.gamePart',
      );
    const backup: LeftReason = {
      kind: 'game-part',
      part: 'backup',
      system: 'psvita',
      title_id: 'PCSE00120',
    };
    expect(text(leftReasonKey(backup))).toEqual([
      'Copia de seguridad cifrada de Vita',
      'Encrypted Vita backup',
    ]);
    expect(text(leftReasonKey({ kind: 'not-confirmed' }))).toEqual([
      'No confirmado; se queda donde está',
      'Not confirmed; stays where it is',
    ]);
  });

  it('every new note and review reason has its exact sentence', () => {
    const cases: [ItemNote, string, string][] = [
      [{ kind: 'homebrew' }, 'Homebrew', 'Homebrew'],
      [
        { kind: 'vita7z' },
        'Vita3K solo instala .zip, .vpk o .vci',
        'Vita3K only installs .zip, .vpk or .vci',
      ],
      [{ kind: 'missing-disc', n: 2 }, 'Falta el disco 2', 'Disc 2 is missing'],
      [
        { kind: 'archive-games', count: 3 },
        'El comprimido contiene 3 juegos',
        'The archive holds 3 games',
      ],
      [
        { kind: 'ps4-or-vita' },
        'Hay que leer param.sfo para saber si es de PS4 o de Vita',
        'param.sfo must be read to tell PS4 from Vita',
      ],
      [
        { kind: 'disc-set' },
        'Parece un juego de varios discos',
        'Looks like a multi-disc game',
      ],
      [
        { kind: 'archive-case-collision' },
        'Dos nombres del comprimido solo cambian en mayúsculas',
        'Two names in the archive differ only in case',
      ],
    ];
    for (const [note, esText, enText] of cases) {
      expect(text(noteKey(note)!), note.kind).toEqual([esText, enText]);
      const row = autoItem(0, 'x', 'switch', 'switch', { notes: [note] });
      expect(itemNotes(t, row, 'Anbernic', (id) => id)).toEqual([esText]);
    }
  });

  it('«Cifrado» is a label, never a note line, and never said twice', () => {
    const row = autoItem(0, '0100F2C0115B6000.xci', 'switch', 'switch', {
      notes: [{ kind: 'encrypted-name' }, { kind: 'encrypted' }],
    });
    expect(encryptedName(row)).toBe(true);
    expect(noteKey({ kind: 'encrypted-name' })).toBeNull();
    // The label replaces the generic «Cifrado; …» line.
    expect(itemNotes(t, row, 'Anbernic', (id) => id)).toEqual([]);
    expect(t('organize.encryptedLabel')).toBe('Cifrado');
    expect(tEn('organize.encryptedLabel')).toBe('Encrypted');
    expect(t('organize.encryptedUnresolved')).toBe(
      'No se puede leer el nombre sin descifrarlo',
    );
    expect(tEn('organize.encryptedUnresolved')).toBe(
      'The name cannot be read without decrypting it',
    );
    // A resolved encrypted 3DS game keeps only its existing note.
    const resolved = autoItem(1, 'Pokemon Y.3ds', '3ds', '3ds', {
      notes: [{ kind: 'encrypted-content', emulator: 'Azahar' }],
    });
    expect(encryptedName(resolved)).toBe(false);
    expect(itemNotes(t, resolved, 'Anbernic', (id) => id)).toEqual([
      'Copia cifrada: Azahar solo abre copias descifradas',
    ]);
  });

  it('a catalogue name and an NCA title id say where they came from', () => {
    const named = autoItem(0, 'Cartridge.xci', 'switch', 'switch', {
      catalogue_name: 'Super Mario Odyssey',
      title_id: { id: '0100000000010000', source: 'nca-catalog' },
      title_source: 'titledb',
    });
    // The name itself: with «Conservar los nombres» the row shows only the
    // file name, so a line about a name it never shows would dangle.
    expect(text(titleSourceKey(named)!)).toEqual([
      'Nombre del catálogo de blawar/titledb: Super Mario Odyssey',
      'Name from the blawar/titledb catalogue: Super Mario Odyssey',
    ]);
    expect(text(titleIdKey(named.title_id!)!)).toEqual([
      'Title ID por el id del NCA en el catálogo',
      'Title ID from the NCA id in the catalogue',
    ]);
    expect(
      text(titleSourceKey({ ...named, title_source: 'nlib' } as OrganizeItem)!),
    ).toEqual([
      'Nombre del respaldo en línea: Super Mario Odyssey',
      'Name from the online fallback: Super Mario Odyssey',
    ]);
    // A source with no name to show says nothing.
    expect(titleSourceKey({ ...named, catalogue_name: null })).toBeNull();
    expect(titleSourceKey(autoItem(1, 'x.nsp', 'switch', 'switch'))).toBeNull();
  });

  it('a loose disc set names the playlist it gets', () => {
    const set = autoItem(
      0,
      'Grandia (Japan) (Disc 1).cue',
      'saturn',
      'Saturn',
      {
        kind: 'set',
        playlist: {
          path: 'ROMS/Saturn/Grandia (Japan).m3u',
          discs: [
            'ROMS/Saturn/Grandia (Japan) (Disc 1).cue',
            'ROMS/Saturn/Grandia (Japan) (Disc 2).cue',
          ],
        },
      },
    );
    expect(text(playlistKey(set)!)).toEqual([
      'Se crea Grandia (Japan).m3u para cambiar de disco',
      'Grandia (Japan).m3u is created for disc swapping',
    ]);
    const win = {
      ...set,
      playlist: { path: 'ROMS\\Saturn\\Grandia (Japan).m3u', discs: [] },
    } as OrganizeItem;
    expect(playlistKey(win)!.vars).toEqual({ name: 'Grandia (Japan).m3u' });
    expect(playlistKey(autoItem(1, 'x.cue', 'saturn', 'Saturn'))).toBeNull();
  });

  it('the adjustments copy exists in both locales with the same placeholders', () => {
    for (const key of [
      'organize.encryptedLabel',
      'organize.encryptedUnresolved',
      'organize.note.homebrew',
      'organize.note.vita7z',
      'organize.note.missingDisc',
      'organize.left.gamePart',
      'organize.left.backup',
      'organize.left.notConfirmed',
      'organize.review.archiveGames',
      'organize.review.ps4OrVita',
      'organize.review.discSet',
      'organize.review.caseCollision',
      'organize.why.titledb',
      'organize.why.nlib',
      'organize.why.ncaCatalog',
    ]) {
      expect(has(es, key) && has(en, key), key).toBe(true);
      expect(placeholders(lookup(en, key)), key).toEqual(
        placeholders(lookup(es, key)),
      );
    }
    // `organize.encrypted` (the older note) is still its own string.
    expect(has(es, 'organize.encrypted')).toBe(true);
  });
});

describe('game folders (spec §9.1, §10)', () => {
  const t = (key: string, vars?: TranslateVars) =>
    translate('es', key, vars ?? {});
  const tEn = (key: string, vars?: TranslateVars) =>
    translate('en', key, vars ?? {});
  const both = (k: { key: string; vars: TranslateVars }) => [
    t(k.key, k.vars),
    tEn(k.key, k.vars),
  ];
  const ds = (id: number, name: string, extra: Partial<OrganizeItem> = {}) =>
    autoItem(id, name, '3ds', 'ROMS/3DS', extra);
  const names = (items: OrganizeItem[]) => items.map((i) => i.sources[0]);

  it('folds children under the base they link to, even with two bases of one name', () => {
    const a = ds(0, 'Tetris (USA).cia', {
      game: { role: 'base', folder: 'ROMS/3DS/Tetris (USA)' },
    });
    const b = ds(1, 'Tetris (Japan).cia', {
      game: { role: 'base', folder: 'ROMS/3DS/Tetris (Japan)' },
    });
    const updB = ds(2, 'upd-b.cia', {
      notes: [{ kind: 'update', of: 'Tetris' }],
      game: { role: 'update', of: 1, version: '1.1.0' },
    });
    const updA = ds(3, 'upd-a.cia', {
      notes: [{ kind: 'update', of: 'Tetris' }],
      game: { role: 'update', of: 0, version: '1.0.1' },
    });
    expect(names(updatesUnderBases([updB, a, updA, b]))).toEqual([
      'Tetris (USA).cia',
      'upd-a.cia',
      'Tetris (Japan).cia',
      'upd-b.cia',
    ]);
  });

  it('a legacy plan without links still folds by name; an orphan link keeps its place', () => {
    const base = ds(0, 'Pokemon Y.cia');
    const upd = ds(1, 'patch.cia', {
      notes: [{ kind: 'update', of: 'Pokemon Y' }],
    });
    expect(names(updatesUnderBases([upd, base]))).toEqual([
      'Pokemon Y.cia',
      'patch.cia',
    ]);
    const orphan = ds(2, 'orphan.cia', {
      notes: [{ kind: 'update', of: 'Pokemon Y' }],
      game: { role: 'update' },
    });
    const items = [orphan, base];
    expect(updatesUnderBases(items)).toBe(items);
  });

  it('previews the folder a base and its children make', () => {
    const base = ds(0, 'Pokemon Y.cia', {
      target: 'ROMS/3DS/Pokemon Y (World)/Pokemon Y (World).cia',
      game: { role: 'base', folder: 'ROMS/3DS/Pokemon Y (World)' },
      companions: [
        {
          source: 'Pokemon Y.tik',
          target: 'ROMS/3DS/Pokemon Y (World)/Pokemon Y.tik',
          licence: true,
        },
      ],
    });
    const upd = ds(1, '0004000E00055D00.cia', {
      target:
        'ROMS/3DS/Pokemon Y (World)/Updates/Pokemon Y (World) (v1.1.0).cia',
      game: { role: 'update', of: 0, version: '1.1.0', newest_here: true },
    });
    const dlc = (id: number, name: string) =>
      ds(id, `${name}.cia`, {
        target: `ROMS\\3DS\\Pokemon Y (World)\\DLC\\Pokemon Y (World) - ${name}.cia`,
        game: { role: 'dlc', of: 0 },
      });
    const other = ds(9, 'Other.cia', { game: { role: 'update', of: 5 } });
    const p = gameFolderPreview(
      [base, upd, dlc(2, 'Pack 2'), dlc(3, 'Pack 1'), other],
      base,
    )!;
    expect(p.name).toBe('Pokemon Y (World)');
    expect(p.exists).toBe(false);
    expect(p.files).toEqual(['Pokemon Y (World).cia', 'Pokemon Y.tik']);
    expect(p.folders).toEqual([
      {
        folder: 'updates',
        disk: 'Updates',
        count: 1,
        names: ['Pokemon Y (World) (v1.1.0).cia'],
        more: 0,
      },
      {
        folder: 'dlc',
        disk: 'DLC',
        count: 2,
        names: [
          'Pokemon Y (World) - Pack 1.cia',
          'Pokemon Y (World) - Pack 2.cia',
        ],
        more: 0,
      },
    ]);
    // No children, or no folder: nothing to preview.
    expect(gameFolderPreview([base], base)).toBeNull();
    expect(gameFolderPreview([upd], ds(0, 'Pokemon Y.cia'))).toBeNull();
  });

  it('caps a folder at 20 names and says how many more, and marks a folder already there', () => {
    const base = ds(0, 'Rock Band 4.pkg', {
      target: 'ROMS/PS4/Rock Band 4 (USA)/Rock Band 4 (USA).pkg',
      game: {
        role: 'base',
        folder: 'ROMS/PS4/Rock Band 4 (USA)',
        folder_exists: true,
      },
    });
    const dlc = Array.from({ length: 25 }, (_, i) =>
      ds(i + 1, `song ${i}.pkg`, {
        target: `ROMS/PS4/Rock Band 4 (USA)/DLC/Rock Band 4 (USA) - Song ${String(i + 1).padStart(2, '0')}.pkg`,
        game: { role: 'dlc', of: 0 },
      }),
    );
    const p = gameFolderPreview([base, ...dlc], base)!;
    expect(p.exists).toBe(true);
    expect(p.folders[0]!.count).toBe(25);
    expect(p.folders[0]!.names).toHaveLength(20);
    expect(p.folders[0]!.more).toBe(5);
    expect(
      both({ key: 'organize.gameFolder.more', vars: { count: 5 } }),
    ).toEqual(['y 5 más', 'and 5 more']);
  });

  it('names fixed folders in the UI language and on disk in English', () => {
    expect(fixedFolderLabel('updates')).toEqual({
      key: 'gameFolder.updates',
      vars: {},
    });
    expect(
      (['updates', 'dlc', 'mods', 'cheats', 'extras', 'saves'] as const).map(
        (f) => [t(fixedFolderLabel(f).key), FIXED_FOLDER_DISK[f]],
      ),
    ).toEqual([
      ['Actualizaciones', 'Updates'],
      ['Contenido descargable', 'DLC'],
      ['Mods', 'Mods'],
      ['Trucos', 'Cheats'],
      ['Extras', 'Extras'],
      ['Partidas', 'Saves'],
    ]);
  });

  it('words demos, missing bases, folder renames and fixed folder content', () => {
    expect(both(noteKey({ kind: 'demo-of', of: 'Resistance (USA)' })!)).toEqual(
      ['Demo de Resistance (USA)', 'Demo of Resistance (USA)'],
    );
    expect(both(noteKey({ kind: 'base-missing' })!)).toEqual([
      'Su juego no está aquí ni en la biblioteca; revisa dónde va',
      'Its game is not here or in the library; check where it goes',
    ]);
    const renamed = ds(0, 'Pokemon Y.cia', {
      rename: { kind: 'game-folder', from: 'Pokemon Y.cia' },
    });
    expect(itemNotes(t, renamed, 'Coverly', (s) => s)).toEqual([
      'Renombrado para su carpeta; antes era Pokemon Y.cia',
    ]);
    const content: LeftReason = {
      kind: 'game-folder-content',
      folder: 'mods',
      of: 'Xenoblade Chronicles 3 (World)',
    };
    expect([leftReasonText(t, content), leftReasonText(tEn, content)]).toEqual([
      'La carpeta Mods de Xenoblade Chronicles 3 (World) se queda donde está por ahora',
      'The Mods folder of Xenoblade Chronicles 3 (World) stays where it is for now',
    ]);
    expect(leftReasonText(t, { ...content, folder: 'saves' })).toBe(
      'La carpeta Partidas de Xenoblade Chronicles 3 (World) se queda donde está por ahora',
    );
    const cache: LeftReason = { kind: 'not-a-rom', what: 'emulator-cache' };
    const config: LeftReason = { kind: 'not-a-rom', what: 'emulator-config' };
    expect([leftReasonText(t, cache), leftReasonText(tEn, config)]).toEqual([
      'No es una ROM (caché de un emulador)',
      'Not a ROM (emulator settings)',
    ]);
  });

  it('a licence goes with its base, or stays when the core left it', () => {
    const base = ds(0, 'Resistance (USA).pkg', {
      title_id: { id: 'BLUS30001', source: 'param-sfo' },
      companions: [
        {
          source: 'BLUS30001.rap',
          target: 'ROMS/PS3/x/BLUS30001.rap',
          licence: true,
        },
      ],
    });
    expect(licenceLine(base, new Set())).toEqual({
      key: 'organize.companion.licence',
      vars: {},
    });
    expect(both(licenceLine(base, new Set())!)).toEqual([
      'Su licencia va con él; Coverly no la abre',
      'Its licence goes with it; Coverly does not open it',
    ]);
    const bare = { ...base, companions: [] };
    expect(licenceLine(bare, new Set())).toBeNull();
    expect(both(licenceLine(bare, new Set([0]))!)).toEqual([
      'Su licencia se queda donde estaba; Coverly no la abre',
      'Its licence stays where it was; Coverly does not open it',
    ]);
    // By name only: a left licence named by the base's title id, content
    // id or stem; global key files never match.
    const left = (id: number, name: string) =>
      item(id, {
        action: 'left',
        sources: [name],
        reason: { kind: 'key-material' },
      } as Partial<OrganizeItem> & Pick<OrganizeItem, 'action'>);
    const withContent = ds(1, 'Gravity.pkg', {
      content_id: 'EP0002-CUSA04447_00-GRAVITYRUSH00000',
    });
    const stays = licencesStaying(
      plan([
        bare,
        withContent,
        ds(2, 'Other.pkg'),
        left(3, 'blus30001.RAP'),
        left(4, 'EP0002-CUSA04447_00-GRAVITYRUSH00000.rif'),
        left(5, 'prod.keys'),
      ]),
    );
    expect([...stays].sort()).toEqual([0, 1]);
  });

  it('versions read as words and the newest mark is text', () => {
    expect(
      both({ key: 'organize.version', vars: { version: '1.1.0' } }),
    ).toEqual(['versión 1.1.0', 'version 1.1.0']);
    expect([t('organize.newestHere'), tEn('organize.newestHere')]).toEqual([
      'la más nueva aquí',
      'newest here',
    ]);
  });

  it('shortens a child target to its fixed folder and name', () => {
    expect(
      childTarget(
        ds(1, 'u.cia', {
          target:
            'ROMS/3DS/Pokemon Y (World)/Updates/Pokemon Y (World) (v1.1.0).cia',
          game: { role: 'update', of: 0 },
        }),
      ),
    ).toBe('…/Updates/Pokemon Y (World) (v1.1.0).cia');
    expect(
      childTarget(
        ds(1, 'd.cia', {
          target: 'ROMS\\3DS\\P\\DLC\\P - Pack 1.cia',
          game: { role: 'dlc', of: 0 },
        }),
      ),
    ).toBe('…\\DLC\\P - Pack 1.cia');
    // Flat (option off) or not a child: the whole target.
    const flat = ds(1, 'u.cia', { notes: [{ kind: 'update', of: 'P' }] });
    expect(childTarget(flat)).toBe(flat.target);
  });

  it('the game folder copy exists in both locales with the same placeholders', () => {
    for (const key of [
      'gameFolder.updates',
      'gameFolder.dlc',
      'gameFolder.mods',
      'gameFolder.cheats',
      'gameFolder.extras',
      'gameFolder.saves',
      'organize.gameFolder.show',
      'organize.gameFolder.hide',
      'organize.gameFolder.label',
      'organize.gameFolder.count',
      'organize.gameFolder.exists',
      'organize.gameFolder.onDisk',
      'organize.gameFolder.more',
      'organize.version',
      'organize.note.demoOf',
      'organize.note.baseMissing',
      'organize.renamedForFolder',
      'organize.left.gameFolderContent',
      'organize.newestHere',
      'organize.licenceStays',
      'organize.companion.licence',
    ]) {
      expect(has(es, key) && has(en, key), key).toBe(true);
      expect(placeholders(lookup(en, key)), key).toEqual(
        placeholders(lookup(es, key)),
      );
      expect(lookup(es, key), key).not.toMatch(/[!¡]/);
    }
  });
});

describe('mods and cheats (mods spec §9, §10)', () => {
  const t = (key: string, vars?: TranslateVars) =>
    translate('es', key, vars ?? {});
  const tEn = (key: string, vars?: TranslateVars) =>
    translate('en', key, vars ?? {});
  const DIR = 'ROMS/3DS/Pokemon Y (World)';
  const ds = (id: number, name: string, extra: Partial<OrganizeItem> = {}) =>
    autoItem(id, name, '3ds', 'ROMS/3DS', extra);
  const names = (items: OrganizeItem[]) => items.map((i) => i.sources[0]);
  const azaharMod: AddonLink = {
    kind: 'mod',
    format: 'layered-fs',
    key_text: '0004000000055D00',
    via: 'key',
  };
  const azaharCheat: AddonLink = {
    kind: 'cheat',
    format: 'ctr-cheats',
    key_text: '0004000000055D00',
    via: 'key',
  };
  const pokemon = () => {
    const base = ds(0, 'Pokemon Y.cia', {
      target: `${DIR}/Pokemon Y (World).cia`,
      game: { role: 'base', folder: DIR },
    });
    const update = ds(1, 'upd.cia', {
      target: `${DIR}/Updates/Pokemon Y (World) (v1.1.0).cia`,
      game: { role: 'update', of: 0, version: '1.1.0' },
    });
    const dlc = ds(2, 'dlc.cia', {
      target: `${DIR}/DLC/Pokemon Y (World) - Pack 1.cia`,
      game: { role: 'dlc', of: 0 },
    });
    const mod = ds(3, '0004000000055D00', {
      kind: 'folder',
      target: `${DIR}/Mods/0004000000055D00`,
      notes: [{ kind: 'mod-of', of: 'Pokemon Y' }],
      game: { role: 'mod', of: 0, addon: azaharMod },
    });
    const cheat = ds(4, '0004000000055D00.txt', {
      target: `${DIR}/Cheats/0004000000055D00.txt`,
      notes: [{ kind: 'cheat-of', of: 'Pokemon Y' }],
      game: { role: 'cheat', of: 0, addon: azaharCheat },
    });
    return { base, update, dlc, mod, cheat };
  };

  it('folds mods and cheats under their game after updates and DLC', () => {
    const { base, update, dlc, mod, cheat } = pokemon();
    expect(names(updatesUnderBases([cheat, mod, dlc, base, update]))).toEqual([
      'Pokemon Y.cia',
      'upd.cia',
      'dlc.cia',
      '0004000000055D00',
      '0004000000055D00.txt',
    ]);
    // An addon linked to a library base (no `of`) keeps its place.
    const loose = ds(5, 'loose.ips', {
      game: { role: 'mod', addon: azaharMod },
    });
    const items = [loose, base];
    expect(updatesUnderBases(items)).toBe(items);
  });

  it('says what an addon is in words, its format and key as data', () => {
    const bps: AddonLink = {
      kind: 'patch',
      format: 'bps',
      key_text: 'CRC32 1A2B3C4D',
      via: 'source-crc',
    };
    expect(addonText(bps)).toEqual({
      key: 'organize.addon.line',
      vars: {
        what: 'organize.addon.patch',
        format: 'BPS',
        key: 'CRC32 1A2B3C4D',
      },
    });
    expect(keyedText(t, addonText(bps))).toBe(
      'Parche de traducción o hack · BPS CRC32 1A2B3C4D',
    );
    expect(keyedText(tEn, addonText(bps))).toBe(
      'Translation patch or hack · BPS CRC32 1A2B3C4D',
    );
    const cht: AddonLink = {
      kind: 'cheat',
      format: 'duck-station-cht',
      key_text: 'SLUS-00594',
      via: 'key',
    };
    expect(keyedText(t, addonText(cht))).toBe(
      'Trucos · DuckStation SLUS-00594',
    );
    expect(keyedText(t, addonText(azaharCheat))).toBe(
      'Trucos · Azahar 0004000000055D00',
    );
    expect(keyedText(t, addonText(azaharMod))).toBe(
      'Mod · LayeredFS 0004000000055D00',
    );
    // A generic format adds no name of its own; the family says more.
    expect(
      keyedText(
        t,
        addonText({
          kind: 'mod',
          format: 'texture-pack',
          key_text: 'SLUS-00594',
          via: 'key',
        }),
      ),
    ).toBe('Pack de texturas · SLUS-00594');
    expect(
      keyedText(
        t,
        addonText({
          kind: 'cheat',
          format: 'rpcs3-patch',
          key_text: 'BLUS30443',
          via: 'key',
        }),
      ),
    ).toBe('Parches del emulador · RPCS3 BLUS30443');
  });

  it('words how each addon was linked, the key in the sentence', () => {
    const via = (v: LinkVia) =>
      addonViaKey({ ...azaharMod, via: v, key_text: 'SLUS-00594' });
    expect(t(via('source-crc').key, via('source-crc').vars)).toBe(
      'Enlazado por el CRC32 de la ROM de origen',
    );
    expect(t(via('dat').key)).toBe('Enlazado por el CRC32 y el DAT');
    expect(t(via('name').key)).toBe('Enlazado por el nombre');
    expect(t(via('readme').key)).toBe('Enlazado por el readme');
    expect(via('key')).toEqual({
      key: 'organize.addon.via.key',
      vars: { key: 'SLUS-00594' },
    });
    expect(t(via('key').key, via('key').vars)).toBe('Enlazado por SLUS-00594');
    expect(t(via('folder').key)).toBe('Ya estaba en la carpeta del juego');
  });

  it('previews Mods and Cheats only when something goes in them', () => {
    const { base, update, dlc, mod, cheat } = pokemon();
    const p = gameFolderPreview([base, update, dlc, mod, cheat], base)!;
    expect(p.folders.map((f) => [f.folder, f.disk, f.count, f.names])).toEqual([
      ['updates', 'Updates', 1, ['Pokemon Y (World) (v1.1.0).cia']],
      ['dlc', 'DLC', 1, ['Pokemon Y (World) - Pack 1.cia']],
      // A folder reads as one, like the game folder itself (§9.1).
      ['mods', 'Mods', 1, ['0004000000055D00/']],
      ['cheats', 'Cheats', 1, ['0004000000055D00.txt']],
    ]);
    expect(
      t('organize.gameFolder.count', {
        folder: t('gameFolder.mods'),
        count: 1,
      }),
    ).toBe('Mods (1)');
    expect(
      t('organize.gameFolder.count', {
        folder: t('gameFolder.cheats'),
        count: 1,
      }),
    ).toBe('Trucos (1)');
    const only = gameFolderPreview([base, update], base)!;
    expect(only.folders.map((f) => f.folder)).toEqual(['updates']);
    // A mod folder one level deeper keeps its inner path.
    const deep = ds(6, 'SLUS-00594', {
      kind: 'folder',
      target: `${DIR}/Mods/textures/SLUS-00594`,
      game: { role: 'mod', of: 0, addon: azaharMod },
    });
    expect(gameFolderPreview([base, deep], base)!.folders).toEqual([
      {
        folder: 'mods',
        disk: 'Mods',
        count: 1,
        names: ['textures/SLUS-00594/'],
        more: 0,
      },
    ]);
    // A Windows plan keeps its own separator in the inner path.
    const winBase = ds(10, 'Crash.cue', {
      target: 'ROMS\\PS1\\Crash (USA)\\Crash (USA).cue',
      game: { role: 'base', folder: 'ROMS\\PS1\\Crash (USA)' },
    });
    const winDeep = ds(11, 'SCUS-94900', {
      kind: 'folder',
      target: 'ROMS\\PS1\\Crash (USA)\\Mods\\textures\\SCUS-94900',
      game: { role: 'mod', of: 10, addon: azaharMod },
    });
    expect(
      gameFolderPreview([winBase, winDeep], winBase)!.folders[0]!.names,
    ).toEqual(['textures\\SCUS-94900\\']);
  });

  it('shortens a mod or cheats target to its fixed folder and name', () => {
    const { mod, cheat } = pokemon();
    expect(childTarget(mod)).toBe('…/Mods/0004000000055D00');
    expect(childTarget(cheat)).toBe('…/Cheats/0004000000055D00.txt');
  });

  it('a translated game reads as three lists, capped at 20 with the rest counted', () => {
    const view: TranslatedGameView = {
      system: 'ps4',
      base: 0,
      emulator: 'shadps4',
      placements: [
        { from: 'Game/CUSA04943', to: { kind: 'system', path: 'CUSA04943' } },
        {
          from: 'Game/Updates/CUSA04943-patch',
          to: { kind: 'system', path: 'CUSA04943-UPDATE' },
        },
        ...Array.from({ length: 22 }, (_, i) => ({
          from: `Game/DLC/pack${i}`,
          to: {
            kind: 'emulator-data' as const,
            path: `addcont/CUSA04943/pack${String(i).padStart(2, '0')}`,
          },
        })),
        {
          from: 'Game/Updates/BLUS-patch.pkg',
          to: { kind: 'library' },
          note: { kind: 'install-in-emulator', emulator: 'rpcs3' },
        },
      ],
    };
    const p = translatedPreview(view);
    expect(p.system).toEqual(['CUSA04943', 'CUSA04943-UPDATE']);
    expect(p.emulatorData).toHaveLength(PREVIEW_NAMES);
    expect(p.emulatorData[0]).toBe('addcont/CUSA04943/pack00');
    expect(p.library).toEqual([
      { path: 'Game/Updates/BLUS-patch.pkg', why: null },
    ]);
    expect(p.more).toEqual({ system: 0, emulatorData: 2, library: 0 });
    expect(t('organize.gameFolder.more', { count: p.more.emulatorData })).toBe(
      'y 2 más',
    );
    const small = translatedPreview(view, 1);
    expect(small.system).toEqual(['CUSA04943']);
    expect(small.more.system).toBe(1);
  });

  it('says why a translator leaves something in the library, in words', () => {
    const line = (n: Parameters<typeof translateNoteKey>[0]) => {
      const k = translateNoteKey(n);
      return k ? keyedText(t, k) : null;
    };
    expect(line({ kind: 'install-in-emulator', emulator: 'rpcs3' })).toBeNull();
    expect(line({ kind: 'header-mismatch', needs: 'needs-headerless' })).toBe(
      'Este parche espera la ROM sin cabecera',
    );
    expect(line({ kind: 'header-mismatch', needs: 'matches' })).toBeNull();
    expect(
      line({
        kind: 'not-supported',
        emulator: 'vita3k',
        why: 'vita-repatch',
      }),
    ).toBe(
      'Vita3K no lo usa así: Vita3K no lee rePatch; hay que mezclar los archivos en el juego',
    );
    expect(line({ kind: 'patch-not-applied', emulator: 'retroarch' })).toBe(
      'RetroArch lo aplicaría al jugar; elige RetroArch para este sistema si lo quieres',
    );
    expect(line({ kind: 'extract-first' })).toBe(
      'Este paquete no se abre así: hay que extraerlo antes',
    );
  });

  it('finds a base its translated layout and names it like its folder', () => {
    const view: TranslatedGameView = {
      system: 'ps4',
      base: 3,
      emulator: 'shadps4',
      placements: [],
    };
    const library: TranslatedGameView = {
      system: 'ps4',
      emulator: 'shadps4',
      placements: [],
    };
    expect(translatedFor([library, view], 3)).toBe(view);
    expect(translatedFor([library], 3)).toBeNull();
    expect(translatedFor(undefined, 3)).toBeNull();
    const inFolder = autoItem(3, 'CUSA00900', 'ps4', 'ROMS/PS4', {
      target: 'ROMS/PS4/Bloodborne (Europe)/CUSA00900',
      game: { role: 'base', folder: 'ROMS/PS4/Bloodborne (Europe)' },
    });
    expect(translatedTitle(inFolder)).toBe('Bloodborne (Europe)');
    const flat = autoItem(4, 'sd3.sfc', 'snes', 'ROMS/SFC', {
      target: 'ROMS\\SFC\\Seiken Densetsu 3 (Japan).sfc',
      game: { role: 'base' },
    });
    expect(translatedTitle(flat)).toBe('Seiken Densetsu 3 (Japan)');
  });

  it('a plan from before mods keeps its rows and preview as they were', () => {
    const { base, update, dlc } = pokemon();
    const items = [base, update, dlc];
    expect(updatesUnderBases(items)).toEqual(items);
    expect(
      gameFolderPreview(items, base)!.folders.map((f) => f.folder),
    ).toEqual(['updates', 'dlc']);
  });

  it('reads the addon copy as the spec says, in both languages', () => {
    const left = (r: LeftReason) => [
      leftReasonText(t, r),
      leftReasonText(tEn, r),
    ];
    expect(
      left({
        kind: 'addon-unlinked',
        addon: 'mod',
        format: 'cemu-graphic-pack',
        why: { kind: 'several', count: 2 },
      }),
    ).toEqual([
      'Encaja con 2 juegos; se queda donde está',
      'It fits 2 games; it stays where it is',
    ]);
    expect(
      left({
        kind: 'addon-unlinked',
        addon: 'cheat',
        format: 'rpcs3-patch',
        why: { kind: 'many', count: 14 },
      }),
    ).toEqual([
      'Tiene parches de 14 juegos; se queda donde está',
      'It holds patches for 14 games; it stays where it is',
    ]);
    for (const why of [
      { kind: 'no-match' },
      { kind: 'no-key' },
      { kind: 'budget' },
    ] as const)
      expect(
        leftReasonText(t, {
          kind: 'addon-unlinked',
          addon: 'patch',
          format: 'ips',
          why,
        }),
      ).toBe('No se sabe de qué juego es; se queda donde está');
    expect(
      left({ kind: 'not-supported', emulator: 'RPCS3', why: 'rpcs3-mods' }),
    ).toEqual([
      'RPCS3 no lo usa así: RPCS3 no tiene carpeta de mods; se reemplazan archivos del juego',
      "RPCS3 does not use it like this: RPCS3 has no mods folder; mods replace the game's files",
    ]);
    expect(
      leftReasonText(t, {
        kind: 'not-supported',
        emulator: 'shadPS4',
        why: 'older-update',
      }),
    ).toBe(
      'shadPS4 no lo usa así: shadPS4 solo usa la actualización más nueva',
    );
    expect(left({ kind: 'patch-not-applied', emulator: 'RetroArch' })[0]).toBe(
      'RetroArch lo aplicaría al jugar; elige RetroArch para este sistema si lo quieres',
    );
    expect(
      left({
        kind: 'addon-for-emulator',
        emulator: 'DuckStation',
        path: 'textures/SLUS-00594',
      })[0],
    ).toBe(
      'Va en la carpeta de DuckStation: textures/SLUS-00594. Coverly no sabe dónde está',
    );
    const line = (n: ItemNote) => {
      const k = noteKey(n)!;
      return t(k.key, k.vars);
    };
    expect(line({ kind: 'mod-of', of: 'Pokemon Y' })).toBe('Mod de Pokemon Y');
    expect(line({ kind: 'patch-of', of: 'Mother' })).toBe('Parche de Mother');
    expect(line({ kind: 'cheat-of', of: 'Pokemon Y' })).toBe(
      'Trucos de Pokemon Y',
    );
    expect(
      line({
        kind: 'point-emulator-here',
        emulator: 'shadPS4',
        folder: 'PS4/CUSA04943',
      }),
    ).toBe('Indica esta carpeta en shadPS4: PS4/CUSA04943');
  });

  it('says whose an addon is before any other note', () => {
    const row = ds(7, 'Mother [T-En].bps', {
      rename: { kind: 'safe-name', from: 'Mother?.bps' },
      notes: [
        { kind: 'header-mismatch', needs: 'needs-headerless' },
        { kind: 'patch-of', of: 'Mother' },
      ],
    });
    expect(itemNotes(t, row, 'Coverly', (id) => id)).toEqual([
      'Parche de Mother',
      t('organize.renamedFrom', { name: 'Mother?.bps' }),
      'Este parche espera la ROM sin cabecera',
    ]);
  });

  it('every spec §10 key exists in both locales with the same placeholders', () => {
    const keys = [
      'organize.note.modOf',
      'organize.note.patchOf',
      'organize.note.cheatOf',
      'organize.addon.patch',
      'organize.addon.mod',
      'organize.addon.texturePack',
      'organize.addon.cheat',
      'organize.addon.emulatorPatch',
      'organize.addon.line',
      'organize.addon.lineBare',
      'organize.addon.via.sourceCrc',
      'organize.addon.via.dat',
      'organize.addon.via.name',
      'organize.addon.via.readme',
      'organize.addon.via.key',
      'organize.addon.via.folder',
      'organize.addon.headerless',
      'organize.addon.headered',
      'organize.left.addonUnlinked',
      'organize.left.addonSeveral',
      'organize.left.addonMany',
      'organize.left.addonNameTaken',
      'organize.left.addonNoPlace',
      'organize.left.addonPart',
      'organize.left.patchNotApplied',
      'organize.left.addonForEmulator',
      'organize.left.notSupported',
      'organize.note.pointEmulatorHere',
      'organize.emulator.label',
      'organize.emulator.default',
      'organize.emulator.none',
      'organize.gameFolder.showFor',
      'organize.gameFolder.labelFor',
      'organize.translate.system',
      'organize.translate.emulatorData',
      'organize.translate.emulatorDataUnknown',
      'organize.translate.library',
      'organize.translate.installHere',
      'organize.translate.why.rpcs3Mods',
      'organize.translate.why.vitaRepatch',
      'organize.translate.why.severalPatches',
      'organize.translate.why.olderUpdate',
      'organize.translate.why.configExists',
    ];
    for (const key of keys) {
      expect(has(es, key) && has(en, key), key).toBe(true);
      expect(placeholders(lookup(en, key)), key).toEqual(
        placeholders(lookup(es, key)),
      );
      expect(lookup(es, key), key).not.toMatch(/[!¡]/);
    }
    expect(lookup(es, 'organize.emulator.none')).toBe(
      'Ninguno (estructura de la biblioteca)',
    );
    expect(lookup(es, 'organize.translate.emulatorDataUnknown')).toBe(
      'Coverly no la toca si no le dices dónde está',
    );
  });
});
