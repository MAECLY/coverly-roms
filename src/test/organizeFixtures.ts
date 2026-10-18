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
  };
}
