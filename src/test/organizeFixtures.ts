/** Hand-made plan items for the organise component tests. */
import type { OrganizeItem, OrganizePlan } from '@/ipc/types';

export function autoItem(
  id: number,
  name: string,
  system: string,
  dir: string,
  extra: Partial<OrganizeItem> = {},
): OrganizeItem {
  return {
    action: 'auto',
    id,
    sources: [name],
    kind: 'archive',
    size: 1024,
    fps: [{ size: 1024, mtime_ns: 0 }],
    target: `${dir}/${name}`,
    set_targets: [],
    system_id: system,
    folder: dir.slice(dir.lastIndexOf('/') + 1),
    confidence: 1,
    method: 'hash',
    why: [{ kind: 'hash', system, weight: 1, detail: `7B729804 ${name}` }],
    alternatives: [],
    rename: null,
    notes: [],
    companions: [],
    catalogue_name: null,
    transfer: { kind: 'as-is' },
    disc: null,
    ...extra,
  } as OrganizeItem;
}

export function reviewItem(
  id: number,
  name: string,
  candidates: [string, number][],
): OrganizeItem {
  const [[best, weight], ...rest] = candidates as [
    [string, number],
    ...[string, number][],
  ];
  return {
    ...autoItem(id, name, best, 'ROMS/X'),
    action: 'review',
    target: null,
    confidence: weight,
    method: 'extension',
    why: [{ kind: 'extension', system: best, weight, detail: '.gb' }],
    alternatives: rest,
  } as OrganizeItem;
}

export function planOf(items: OrganizeItem[]): OrganizePlan {
  return {
    source_root: '/pile',
    target_root: '/pile',
    roms_dir: '/pile/ROMS',
    layout: 'anbernic-stock',
    mode: 'move',
    names: 'keep',
    items,
    counts: {
      roms: items.length,
      systems: 1,
      auto: items.length,
      review: 0,
      left: 0,
      junk: 0,
      other_files: 0,
      deep_folders: 0,
      bytes: 0,
      set_aside: 0,
      in_library: 0,
    },
    sorted_folders: [],
    deep_folders: [],
    other_files: [],
    warnings: [],
    broad_root: false,
    fat32: false,
    move_allowed: true,
    target_detected: null,
    created_at: '2026-10-17T10:00:00Z',
    duplicates: [],
    set_aside_trash: false,
  };
}

/**
 * A PS1 disc archive (a 7z named `.zip`) that is extracted flat: a `.cue`
 * and `tracks` `.bin` files, `bytes` in all.
 */
export function discItem(
  id: number,
  extra: Partial<OrganizeItem> = {},
  tracks = 11,
): OrganizeItem {
  const title = 'Next Tetris, The';
  const dir = 'ROMS/PS';
  const bins = Array.from({ length: tracks }, (_, i) => {
    const n = String(i + 1).padStart(2, '0');
    return {
      index: i + 1,
      name: `${title} (Track ${n}).bin`,
      size: i === 0 ? 300 * 1024 * 1024 : 10 * 1024 * 1024,
      crc32: 0x1000 + i,
      target: `${dir}/${title} (Track ${n}).bin`,
    };
  });
  const entries = [
    {
      index: 0,
      name: `${title}.cue`,
      size: 1234,
      crc32: 0xabc,
      target: `${dir}/${title}.cue`,
    },
    ...bins,
  ];
  const bytes = entries.reduce((n, e) => n + e.size, 0);
  return autoItem(id, `${title}.zip`, 'psx', dir, {
    target: `${dir}/${title}.cue`,
    size: bytes,
    method: 'header',
    why: [
      {
        kind: 'header',
        system: 'psx',
        weight: 1,
        detail: 'SYSTEM.CNF BOOT SLUS_008.62',
      },
    ],
    notes: [{ kind: 'container-mismatch', actual: '7z', ext: 'zip' }],
    transfer: {
      kind: 'extract',
      entries,
      bytes,
      archive_after: 'trash',
      source: { kind: 'builtin' },
      sheet_check: 'read',
      m3u: null,
    },
    disc: {
      serials: ['SLUS-00862'],
      disc_no: null,
      volume: null,
      source: 'system-cnf',
    },
    ...extra,
  } as Partial<OrganizeItem>);
}
