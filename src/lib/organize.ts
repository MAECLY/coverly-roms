/**
 * Pure helpers for the organise proposal: grouping a plan for display and
 * turning reasons and evidence (data from the core) into i18n keys.
 */
import type { TranslateVars } from '@/i18n/core';
import type {
  ArchiveTool,
  DiscId,
  Evidence,
  ItemNote,
  LeftReason,
  OrganizeItem,
  OrganizePlan,
  ProfileFile,
  RenameReason,
  TransferMode,
} from '@/ipc/types';

type Translate = (key: string, vars?: TranslateVars) => string;

/** Rows a system group (or bucket) renders before "Show more". */
export const ROWS_PER_GROUP = 200;

/**
 * Who an item is across scans: its primary file, `\` as `/`, lower-cased
 * (FAT and macOS volumes ignore case). Ids are positions and shift as soon
 * as the folder gains or loses a file.
 */
export function itemKey(item: OrganizeItem): string {
  return (item.sources[0] ?? '').replace(/\\/g, '/').toLowerCase();
}

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

/** Tool names are never translated (spec §11.2). */
const TOOL_NAMES: Record<ArchiveTool, string> = {
  unrar: 'unrar',
  'seven-zip': '7-Zip',
  bsdtar: 'bsdtar',
};

/**
 * A container's name for the copy (`RAR`, `TAR.LZ4`, `7z`), or `null` when
 * `format` is absent or is a diagnostic rather than a name (a refused tool
 * puts its path and the reason there).
 */
function formatLabel(format: string | null | undefined): string | null {
  const f = format?.trim() ?? '';
  if (!/^[A-Za-z0-9][A-Za-z0-9.]{0,11}$/.test(f)) return null;
  return f.toLowerCase() === '7z' ? '7z' : f.toUpperCase();
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
      if (!r.need) return k('unsupportedArchive');
      {
        const format = formatLabel(r.format);
        const tool = TOOL_NAMES[r.need];
        return format
          ? k('unsupportedArchiveTool', { format, tool })
          : k('unsupportedArchiveInstall', { tool });
      }
    case 'encrypted-archive':
      return k('encryptedArchive');
    case 'archive-needs-memory':
      return k('archiveNeedsMemory', { mib: r.mib });
    case 'archive-too-big':
      return k('archiveTooBig');
    case 'multi-volume-archive':
      return k('multiVolume', { parts: r.parts });
    case 'nested-archive':
      return k('nestedArchive');
    case 'repack-arcade':
      return k('repackArcade');
    case 'unsafe-archive-entry':
      return k('unsafeArchiveEntry', { name: r.name });
    case 'set-unsafe-name':
      return k('setUnsafeName', { name: r.name });
    case 'path-too-long':
      return k('pathTooLong');
    case 'unsafe-target':
      return k('unsafeTarget');
    case 'folder-full':
      return k('folderFull', { folder: r.folder });
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
  disc?: DiscId | null,
): KeyedText {
  const system = systemName(e.system);
  // A disc's header clue names the serials read off the disc too.
  if (e.kind === 'header' && disc && disc.serials.length > 0)
    return {
      key: 'organize.why.disc',
      vars: { system, detail: e.detail, serial: disc.serials.join(', ') },
    };
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

/**
 * The folder an item lands in, relative to the target (`ROMS/GBA`). The
 * backend sends native paths, so Windows targets use backslashes.
 */
export function targetDir(item: OrganizeItem): string {
  const target = item.target ?? '';
  const slash = Math.max(target.lastIndexOf('/'), target.lastIndexOf('\\'));
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
  for (const n of item.notes) {
    const line = noteText(t, n, system, layoutName, item);
    if (line !== null) lines.push(line);
  }
  return lines;
}

/** Archive facts the extract row folds into its detail (spec §11.1). */
const DETAIL_NOTES: ReadonlySet<ItemNote['kind']> = new Set([
  'container-mismatch',
  'twin-sheets',
  'no-stored-checksum',
]);

/**
 * An item's note lines split for its row: an extract row folds the archive's
 * own facts (`ContainerMismatch`, `TwinSheets`, `NoStoredChecksum`) into its
 * detail, while renames and anything that asks the user to look or changes
 * the result stay in sight. Every line is inline for an as-is item.
 */
export function rowNotes(
  t: Translate,
  item: OrganizeItem,
  layoutName: string,
  systemName: (id: string) => string,
): { inline: string[]; detail: string[] } {
  if (item.transfer.kind !== 'extract')
    return { inline: itemNotes(t, item, layoutName, systemName), detail: [] };
  const inline: string[] = [];
  const detail: string[] = [];
  if (item.rename) inline.push(renameText(t, item.rename));
  const system = item.system_id ? systemName(item.system_id) : '';
  for (const n of item.notes) {
    const line = noteText(t, n, system, layoutName, item);
    if (line !== null) (DETAIL_NOTES.has(n.kind) ? detail : inline).push(line);
  }
  return { inline, detail };
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
): string | null {
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
    default: {
      const keyed = noteKey(n);
      return keyed ? t(keyed.key, keyed.vars) : null;
    }
  }
}

/**
 * The i18n key of an archive or disc note (data only). The older notes need
 * the system, layout or level of their row and are worded by `itemNotes`.
 */
export function noteKey(n: ItemNote): KeyedText | null {
  const k = (name: string, vars: TranslateVars = {}): KeyedText => ({
    key: `organize.note.${name}`,
    vars,
  });
  switch (n.kind) {
    case 'container-mismatch':
      return k('containerMismatch', {
        actual: n.actual,
        ext: n.ext.replace(/^\./, ''),
      });
    case 'twin-sheets':
      return k('twinSheets');
    case 'no-stored-checksum':
      return k('noStoredChecksum');
    case 'archive-not-fully-read':
      return k('notFullyRead');
    case 'archive-kept-leftovers':
      return k('archiveKeptLeftovers', { count: n.count });
    case 'sheet-not-read-yet':
      return k('sheetNotReadYet');
    case 'multi-rom-extract':
      return k('multiRomExtract', { count: n.count });
    case 'multi-rom-zip':
    case 'no-folder-on-device':
    case 'extension-contradicted':
    case 'catalogue-missing':
    case 'encrypted':
      return null;
  }
}

/** What applying does with an item: the plan's mode, or extracting it. */
export function actionOf(
  item: OrganizeItem,
  mode: TransferMode,
): 'move' | 'copy' | 'extract' {
  return item.transfer.kind === 'extract' ? 'extract' : mode;
}

export interface ExtractTotals {
  count: number;
  bytes: number;
}

/**
 * Files an extract item writes out of its archive and their bytes; a
 * generated `.m3u` is not one of them (the CLI counts alike). `null` when
 * the item moves as it is.
 */
export function extractDetail(item: OrganizeItem): ExtractTotals | null {
  const t = item.transfer;
  return t.kind === 'extract'
    ? { count: t.entries.length, bytes: t.bytes }
    : null;
}

/**
 * Games the plan extracts (decided items only, like the apply button) and
 * their bytes; `null` when nothing is extracted.
 */
export function extractSummary(plan: OrganizePlan): ExtractTotals | null {
  let count = 0;
  let bytes = 0;
  for (const item of plan.items) {
    if (item.action !== 'auto' || item.transfer.kind !== 'extract') continue;
    count += 1;
    bytes += item.transfer.bytes;
  }
  return count > 0 ? { count, bytes } : null;
}

function baseName(path: string): string {
  return path.slice(
    Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')) + 1,
  );
}

/**
 * Where an extract item's files go, as lines for the row detail: a disc's
 * layout (flat or one folder per game, absent = flat) and its generated
 * playlist, or a cartridge's folder. Empty when the item moves as it is.
 */
export function layoutNote(
  item: OrganizeItem,
  profile: Pick<ProfileFile, 'disc_layout'> | null | undefined,
): KeyedText[] {
  const t = item.transfer;
  if (t.kind !== 'extract') return [];
  const folder = targetDir(item);
  if (item.kind !== 'set' && item.disc === null && t.m3u === null) {
    const isDisc = t.entries.some((e) => /\.(cue|gdi)$/i.test(e.name));
    if (!isDisc) return [{ key: 'organize.extract.into', vars: { folder } }];
  }
  const lines: KeyedText[] = [
    profile?.disc_layout?.kind === 'game-folder'
      ? { key: 'organize.extract.layoutFolder', vars: { folder } }
      : { key: 'organize.extract.layoutFlat', vars: { folder } },
  ];
  if (t.m3u)
    lines.push({
      key: 'organize.extract.m3u',
      vars: { name: baseName(t.m3u) },
    });
  return lines;
}
