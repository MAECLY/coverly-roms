/** What the extraction screen shows, derived from the plan and one event. */
import type {
  ExtractEntry,
  OrganizeItem,
  OrganizePlan,
  OrganizeProgressEvent,
} from '@/ipc/types';

export type RowState = 'done' | 'current' | 'waiting';

export interface TrackRow {
  /** Position in the archive's own order. */
  index: number;
  /** The entry's last path segment. */
  name: string;
  /** The entry path as in the archive. */
  full: string;
  size: number;
  /** D8: a ✓ against the stored CRC, or size and copy only. */
  checked: 'crc' | 'size';
  state: RowState;
}

export interface ExtractView {
  itemId: number;
  name: string;
  systemId: string | null;
  targetDir: string | null;
  /** D7: «pista» for a disc set, «archivo» otherwise. */
  noun: 'track' | 'file';
  /** D6: «Juego n de N», from plan order; null without a plan. */
  game: { n: number; total: number } | null;
  /** Null for a single entry or an event without counts. */
  entries: { done: number; total: number } | null;
  rows: TrackRow[];
  /** `checking` once every entry is staged (phase B). The edge often
   * carries `bytes_total` already (nothing else to decode), so bytes
   * cannot tell it apart; the item's closing event reads the same. */
  phase: 'decoding' | 'checking';
  bytes: { done: number; total: number };
}

export type ExtractEvent = Extract<OrganizeProgressEvent, { phase: 'extract' }>;

const lastSegment = (p: string) =>
  p.slice(Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\')) + 1);

const gameIndexCache = new WeakMap<OrganizePlan, number[]>();

/** D6: the applicable extract items' ids in plan (= apply) order. */
export function gameIndex(plan: OrganizePlan): number[] {
  let ids = gameIndexCache.get(plan);
  if (!ids) {
    ids = plan.items
      .filter(
        (i) =>
          i.action === 'auto' &&
          i.target !== null &&
          i.transfer.kind === 'extract',
      )
      .map((i) => i.id);
    gameIndexCache.set(plan, ids);
  }
  return ids;
}

/** The target folder, joined with the root's own separator. */
export function targetDir(root: string, target: string): string {
  const sep = root.includes('\\') ? '\\' : '/';
  const parts = target.split(/[\\/]/).slice(0, -1);
  return [root.replace(/[\\/]+$/, ''), ...parts].join(sep);
}

/** Shortens a name in the middle, keeping more of its end (track number, extension). */
export function middleTruncate(name: string, max = 36): string {
  if (name.length <= max) return name;
  if (max < 2) return '…'.slice(0, Math.max(0, max));
  const tail = Math.ceil((max - 1) * 0.6);
  return `${name.slice(0, max - 1 - tail)}…${name.slice(-tail)}`;
}

export function extractView(
  plan: OrganizePlan | null,
  e: ExtractEvent,
): ExtractView {
  const item: OrganizeItem | undefined = plan?.items.find(
    (i) => i.id === e.item,
  );
  const transfer = item?.transfer.kind === 'extract' ? item.transfer : null;
  const total = e.entries_total;
  const done = e.entries_done;
  const counted = total !== undefined && done !== undefined && total > 1;
  const sorted: ExtractEntry[] = transfer
    ? [...transfer.entries].sort((a, b) => a.index - b.index)
    : [];
  const rows: TrackRow[] =
    counted && sorted.length === total
      ? sorted.map((x, k) => ({
          index: x.index,
          name: lastSegment(x.name),
          full: x.name,
          size: x.size,
          checked: x.crc32 === null ? 'size' : 'crc',
          state: k < done ? 'done' : k === done ? 'current' : 'waiting',
        }))
      : [];
  const games = plan ? gameIndex(plan) : [];
  const pos = games.indexOf(e.item);
  const first = sorted[0];
  return {
    itemId: e.item,
    name: item?.sources[0] ? lastSegment(item.sources[0]) : `#${e.item}`,
    systemId: item?.system_id ?? null,
    targetDir: plan && first ? targetDir(plan.target_root, first.target) : null,
    noun: item?.disc ? 'track' : 'file',
    game: plan && pos >= 0 ? { n: pos + 1, total: games.length } : null,
    entries: counted ? { done, total } : null,
    rows,
    phase: counted && done >= total ? 'checking' : 'decoding',
    bytes: {
      done: Math.min(e.bytes_done, e.bytes_total),
      total: e.bytes_total,
    },
  };
}
