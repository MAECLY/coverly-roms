/**
 * Pure helpers for the organise proposal: grouping a plan for display and
 * turning reasons and evidence (data from the core) into i18n keys.
 */
import type { TranslateVars } from '@/i18n/core';
import type {
  Evidence,
  ItemNote,
  LeftReason,
  OrganizeItem,
  OrganizePlan,
  RenameReason,
} from '@/ipc/types';

type Translate = (key: string, vars?: TranslateVars) => string;

/** Rows a system group (or bucket) renders before "Show more". */
export const ROWS_PER_GROUP = 200;

const collator = new Intl.Collator('en', {
  sensitivity: 'base',
  numeric: true,
});

export interface SystemGroup {
  folder: string;
  systemId: string;
  items: OrganizeItem[];
}

/** Items that move, grouped by target system folder, in folder order. */
export function groupBySystem(plan: OrganizePlan): SystemGroup[] {
  const groups = new Map<string, SystemGroup>();
  for (const item of plan.items) {
    if (item.action !== 'auto' || !item.folder || !item.system_id) continue;
    let g = groups.get(item.folder);
    if (!g) {
      g = { folder: item.folder, systemId: item.system_id, items: [] };
      groups.set(item.folder, g);
    }
    g.items.push(item);
  }
  return [...groups.values()].sort((a, b) =>
    collator.compare(a.folder, b.folder),
  );
}

export interface ReviewBucket {
  /** Stable for a candidate set, whatever the order of the scores. */
  key: string;
  /** Candidate systems, best score (of the first item) first. */
  systems: string[];
  items: OrganizeItem[];
}

function candidates(item: OrganizeItem): string[] {
  const all = [
    ...(item.system_id ? [item.system_id] : []),
    ...item.alternatives.map(([s]) => s),
  ];
  return [...new Set(all)];
}

/** Review items bucketed by their set of candidate systems ("GB or GBC: 37"). */
export function reviewBuckets(plan: OrganizePlan): ReviewBucket[] {
  const buckets = new Map<string, ReviewBucket>();
  for (const item of plan.items) {
    if (item.action !== 'review') continue;
    const systems = candidates(item);
    const key = [...systems].sort().join('|');
    const b = buckets.get(key);
    if (b) b.items.push(item);
    else buckets.set(key, { key, systems, items: [item] });
  }
  return [...buckets.values()];
}

/** Items that stay where they are. */
export function leftItems(plan: OrganizePlan): OrganizeItem[] {
  return plan.items.filter((i) => i.action === 'left');
}

/** High ≥ 0.95, medium ≥ 0.85, otherwise no label (review territory). */
export function confidenceKey(
  c: number,
): 'organize.confidence.high' | 'organize.confidence.medium' | null {
  if (c >= 0.95) return 'organize.confidence.high';
  if (c >= 0.85) return 'organize.confidence.medium';
  return null;
}

export interface KeyedText {
  key: string;
  vars: TranslateVars;
}

/**
 * The i18n key of a left reason. A var whose value is itself an
 * `organize.*` key (the `what` of `not-a-rom`) is a fragment to translate
 * first: `leftReasonText` does that.
 */
export function leftReasonKey(r: LeftReason): KeyedText {
  const k = (name: string, vars: TranslateVars = {}): KeyedText => ({
    key: `organize.left.${name}`,
    vars,
  });
  switch (r.kind) {
    case 'partial-download':
      return k('partialDownload');
    case 'recently-modified':
      return k('recent');
    case 'cloud-only':
      return k('cloudOnly');
    case 'not-a-rom':
      return k('notARom', {
        what:
          r.what === 'web-page'
            ? 'organize.left.webPage'
            : 'organize.left.text',
      });
    case 'bios':
      return k('bios');
    case 'damaged-archive':
      return k('damagedArchive');
    case 'unsupported-archive':
      return k('unsupportedArchive');
    case 'multi-game-archive':
      return k('multiGameArchive', { count: r.count });
    case 'unsupported-system':
      return k('unsupportedSystem', { name: r.name });
    case 'unknown':
      return k('unknown');
    case 'duplicate':
      return k('duplicate', { name: r.of });
    case 'already-present':
      return k('alreadyPresent', { path: r.at });
    case 'already-sorted':
      return k('alreadySorted', { folder: r.folder });
    case 'symlink':
      return k('symlink');
    case 'set-incomplete':
      return k('setIncomplete', { missing: r.missing.join(', ') });
    case 'set-outside-folder':
      return k('setOutsideFolder');
    case 'shared-track':
      return k('sharedTrack');
    case 'ambiguous-companion':
      return k('ambiguousCompanion');
    case 'too-big-for-fat':
      return k('tooBigForFat');
    case 'unreadable':
      return k('unreadable');
    case 'collision-set':
      return k('collisionSet');
  }
}

/** A left reason as text, nested fragments translated first. */
export function leftReasonText(
  t: (key: string, vars?: TranslateVars) => string,
  r: LeftReason,
): string {
  const { key, vars } = leftReasonKey(r);
  const resolved: TranslateVars = {};
  for (const [name, v] of Object.entries(vars)) {
    resolved[name] =
      typeof v === 'string' && v.startsWith('organize.') ? t(v) : v;
  }
  return t(key, resolved);
}

/** Below this an extension is shared by several systems (spec §6.4). */
const UNIQUE_EXTENSION = 0.85;
const CRC_DETAIL = /^(?:CRC\s+)?([0-9A-Fa-f]{8})(?:\s+(.*))?$/;

/** The "why" line of one clue; `detail` is data from the core, never prose. */
export function whyKey(
  e: Evidence,
  systemName: (id: string) => string,
): KeyedText {
  const system = systemName(e.system);
  switch (e.kind) {
    case 'hash': {
      // Rust: "<crc> <catalogue game>".
      const m = CRC_DETAIL.exec(e.detail.trim());
      return {
        key: 'organize.why.hash',
        vars: m
          ? { crc: m[1]!.toUpperCase(), system, name: m[2] ?? '' }
          : { crc: '', system, name: e.detail },
      };
    }
    case 'header':
      return { key: 'organize.why.header', vars: { system, detail: e.detail } };
    case 'extension': {
      const contradicted = e.detail.endsWith('!');
      const ext = e.detail.replace(/!$/, '').replace(/^\./, '');
      const key = contradicted
        ? 'organize.why.extensionContradicted'
        : e.weight >= UNIQUE_EXTENSION
          ? 'organize.why.extension'
          : 'organize.why.extensionShared';
      return { key, vars: { ext, system } };
    }
    case 'name':
      return { key: 'organize.why.name', vars: { name: e.detail, system } };
    case 'online':
      return { key: 'organize.why.online', vars: { name: e.detail, system } };
    case 'user':
      return { key: 'organize.why.user', vars: { system } };
  }
}

/** The folder an item lands in, relative to the target (`ROMS/GBA`). */
export function targetDir(item: OrganizeItem): string {
  const target = item.target ?? '';
  const slash = target.lastIndexOf('/');
  return slash < 0 ? (item.folder ?? '') : target.slice(0, slash);
}

/** Rename and note lines of one row; data, never markup. */
export function itemNotes(
  t: Translate,
  item: OrganizeItem,
  layoutName: string,
  systemName: (id: string) => string,
): string[] {
  const lines: string[] = [];
  if (item.rename) lines.push(renameText(t, item.rename));
  const system = item.system_id ? systemName(item.system_id) : '';
  for (const n of item.notes)
    lines.push(noteText(t, n, system, layoutName, item));
  return lines;
}

function renameText(t: Translate, r: RenameReason): string {
  switch (r.kind) {
    case 'collision':
      return t('organize.renamed', { name: r.name });
    case 'safe-name':
    case 'catalogue':
      return t('organize.renamedFrom', { name: r.from });
    case 'path-length':
      return t('organize.renamedShort');
  }
}

function noteText(
  t: Translate,
  n: ItemNote,
  system: string,
  layout: string,
  item: OrganizeItem,
): string {
  switch (n.kind) {
    case 'multi-rom-zip':
      return t('organize.multiRomZip', { count: n.count });
    case 'no-folder-on-device':
      return t('organize.noFolderOnDevice', {
        layout,
        system,
        folder: n.folder,
      });
    case 'extension-contradicted':
      return t('organize.why.extensionContradicted', {
        ext: n.ext.replace(/^\./, ''),
        system,
      });
    case 'catalogue-missing':
      return t('organize.noCatalogue', {
        system,
        level: t(`organize.level.${item.method ?? 'extension'}`),
      });
    case 'encrypted':
      return t('organize.encrypted');
  }
}
