/**
 * Pure helpers for the duplicates proposal: reasons (data from the core)
 * into one localised line, relation badges, per-system sums, the count
 * delta of an option change and the display order of groups.
 */
import {
  CircleHelp,
  Copy,
  Disc3,
  History,
  type LucideIcon,
} from 'lucide-react';

import type { TranslateVars } from '@/i18n/core';
import {
  RELATIONS,
  type ArtSeenView,
  type DedupeLevel,
  type DedupeOutcomeView,
  type DedupePlanView,
  type DedupeViewOptions,
  type DupGroupView,
  type KeepReasonView,
  type MediaView,
  type MemberFactsView,
  type MemberView,
  type ModClassView,
  type RegionId,
  type RelationView,
  type ReportWhyView,
  type TagSourceView,
  type UnitRefView,
} from '@/ipc/dedupeTypes';

export { formatSize as formatBytes } from './format';

type Translate = (key: string, vars?: TranslateVars) => string;

/** What a reason line may need beyond the reason itself. */
export interface ReasonCtx {
  /** Display name of the profile a `container` reason names. */
  profileName?: string;
}

/** Last segment of a `/` or `\` path. */
export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** "Se queda: región preferida (Mundo); la otra es Japón." */
export function reasonText(
  t: Translate,
  reason: KeepReasonView,
  ctx: ReasonCtx,
): string {
  switch (reason.kind) {
    case 'user-choice':
      return t('dedupe.why.user');
    case 'has-save':
      return t('dedupe.why.hasSave', { path: fileName(reason.path) });
    case 'played':
      return reason.playcount === 1
        ? t('dedupe.why.playedOnce')
        : t('dedupe.why.played', { count: reason.playcount });
    case 'in-library':
      return t('dedupe.why.inLibrary');
    case 'verified':
      return t('dedupe.why.verified', {
        kept: t(`dedupe.quality.kept.${reason.kept}`),
        other: t(`dedupe.quality.other.${reason.other}`),
      });
    case 'retail':
      return t('dedupe.why.retail', {
        other: t(`dedupe.release.${reason.other}`),
      });
    case 'region':
      return t('dedupe.why.region', {
        kept: t(`regions.${reason.kept}`),
        other: t(`regions.${reason.other}`),
      });
    case 'revision':
      return t('dedupe.why.revision', {
        kept: reason.kept,
        other: reason.other,
      });
    case 'container':
      return t('dedupe.why.container', {
        profile: ctx.profileName ?? reason.profile,
        kept: reason.kept.toUpperCase(),
        other: reason.other.toUpperCase(),
      });
    case 'has-cover':
      return t('dedupe.why.cover');
    case 'catalogue-name':
      return t('dedupe.why.catalogueName');
    case 'tie':
      return t('dedupe.why.tie');
  }
}

/** What a member's badge says: its relation, or "not confirmed" (R1). */
export type BadgeRelation = RelationView | 'unconfirmed';

const RELATION_ICONS: Record<BadgeRelation, LucideIcon> = {
  exact: Copy,
  format: Disc3,
  version: History,
  unconfirmed: CircleHelp,
};

/** Icon and i18n key of a relation badge (R3: lucide icons, never glyphs). */
export function relationMeta(r: BadgeRelation): {
  icon: LucideIcon;
  key: string;
} {
  return { icon: RELATION_ICONS[r], key: `dedupe.relation.${r}` };
}

/**
 * The badge of a member. An exact copy below confidence 1.00 reads "not
 * confirmed"; the number itself lives only in the tooltip (R1).
 */
export function badgeRelation(m: MemberView): BadgeRelation {
  if (m.relation === 'exact' && m.confidence < 1) return 'unconfirmed';
  return m.relation;
}

export interface SystemSummary {
  system: string;
  /** Groups that act. */
  groups: number;
  /** Copies that leave. */
  extras: number;
  /** Bytes that leave ROMS/. */
  bytes: number;
  conflicts: number;
  /** Every group of the system, whatever its action. */
  total: number;
}

function extrasOf(g: DupGroupView): MemberView[] {
  return g.members.filter((m) => m.id !== g.keeper);
}

/** Per-system sums in system order; the same numbers as `totals.by_system`. */
export function systemSummary(groups: DupGroupView[]): SystemSummary[] {
  const by = new Map<string, SystemSummary>();
  for (const g of groups) {
    let s = by.get(g.system);
    if (!s) {
      s = {
        system: g.system,
        groups: 0,
        extras: 0,
        bytes: 0,
        conflicts: 0,
        total: 0,
      };
      by.set(g.system, s);
    }
    s.total += 1;
    if (g.action.kind === 'conflict') s.conflicts += 1;
    if (g.action.kind !== 'act') continue;
    const extras = extrasOf(g);
    s.groups += 1;
    s.extras += extras.length;
    s.bytes += extras.reduce((n, m) => n + m.size, 0);
  }
  return [...by.values()].sort((a, b) => a.system.localeCompare(b.system));
}

/**
 * Copies that would leave under `opts`, from the groups already in the
 * plan (no regrouping): acting groups and level-two groups held back only
 * because one-ROM-per-game is off. A copy counts when its level is on, and
 * a version only with one-ROM-per-game.
 */
export function extrasFor(
  plan: DedupePlanView,
  opts: DedupeViewOptions,
): number {
  let n = 0;
  for (const g of plan.groups) {
    const a = g.action;
    const movable =
      a.kind === 'act' ||
      (a.kind === 'report-only' && a.why.kind === 'level-two-off');
    if (!movable) continue;
    for (const m of extrasOf(g)) {
      if (!opts.levels.includes(m.relation)) continue;
      if (m.relation === 'version' && !opts.one_game_one_rom) continue;
      n += 1;
    }
  }
  return n;
}

/** How many more (or fewer) copies leave if the options become `next`. */
export function deltaFor(
  plan: DedupePlanView,
  next: DedupeViewOptions,
): number {
  const now = plan.groups
    .filter((g) => g.action.kind === 'act')
    .reduce((n, g) => n + extrasOf(g).length, 0);
  return extrasFor(plan, next) - now;
}

/** Conflicts first, then by system, then by title. Returns a new array. */
export function sortGroups(groups: DupGroupView[]): DupGroupView[] {
  const rank = (g: DupGroupView) => (g.action.kind === 'conflict' ? 0 : 1);
  return [...groups].sort(
    (a, b) =>
      rank(a) - rank(b) ||
      a.system.localeCompare(b.system) ||
      a.title.localeCompare(b.title),
  );
}

function isAbsolute(path: string): boolean {
  return path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path);
}

function slash(path: string): string {
  return path.replace(/\\/g, '/');
}

/**
 * The folder of system folders, learnt from a peer the plan already sends
 * to the quarantine: its `dest` is `<quarantine>/<rel>` and its primary is
 * `<library>/<rel>` (the core's `dest_for`).
 */
function libraryRoot(q: string, peers: MemberView[]): string | null {
  const qs = slash(q);
  for (const p of peers) {
    if (!p.dest) continue;
    const dest = slash(p.dest);
    if (!dest.startsWith(`${qs}/`)) continue;
    const rel = dest.slice(qs.length + 1);
    const primary = slash(p.unit.primary);
    if (rel && primary.endsWith(`/${rel}`))
      return primary.slice(0, primary.length - rel.length - 1);
  }
  return null;
}

/**
 * Where a copy goes when it leaves. The plan carries `dest` for the copies
 * the rules set aside; a former keeper (after a pick) has none, so it goes
 * to the same place under the quarantine as its library path, the way the
 * core places the others: relative to the library folder learnt from a
 * peer, else `<quarantine>/<system>/<file name>`.
 */
export function destOf(
  m: MemberView,
  quarantine: string,
  peers: MemberView[] = [],
  system?: string,
): string {
  if (m.dest) return m.dest;
  const q = quarantine.replace(/[\\/]+$/, '');
  const sep = q.includes('\\') && !q.includes('/') ? '\\' : '/';
  const join = (rel: string) => `${q}${sep}${rel.split('/').join(sep)}`;
  const primary = slash(m.unit.primary);
  if (!isAbsolute(m.unit.primary)) return join(primary);
  const root = libraryRoot(q, peers);
  if (root !== null && primary.startsWith(`${root}/`))
    return join(primary.slice(root.length + 1));
  const name = fileName(m.unit.primary);
  return join(system ? `${system}/${name}` : name);
}

/** A destination as the UI writes it: from the quarantine folder down. */
export function destLabel(dest: string, quarantine: string): string {
  const q = quarantine.replace(/[\\/]+$/, '');
  const name = fileName(q);
  const rest = dest.slice(q.length);
  return dest.startsWith(q) && /^[\\/]/.test(rest) ? `${name}${rest}` : dest;
}

/**
 * The copy that names a group's badge: the weakest link (exact < format <
 * version), then the lowest confidence. `null` when only the keeper is left.
 */
export function weakestExtra(
  members: MemberView[],
  keeper: number,
): MemberView | null {
  let worst: MemberView | null = null;
  for (const m of members) {
    if (m.id === keeper) continue;
    if (
      !worst ||
      RELATIONS.indexOf(m.relation) > RELATIONS.indexOf(worst.relation) ||
      (m.relation === worst.relation && m.confidence < worst.confidence)
    )
      worst = m;
  }
  return worst;
}

/** Why a group is only reported, in one line. */
export function reportWhyText(t: Translate, why: ReportWhyView): string {
  switch (why.kind) {
    case 'level-two-off':
      return t('dedupe.report.levelTwoOff');
    case 'unconfirmed':
      return t('dedupe.report.unconfirmed');
    case 'header-differs':
      return t('dedupe.report.headerDiffers');
    case 'build-differs':
      return t('dedupe.report.buildDiffers');
    case 'partial-set':
      return t('dedupe.report.partialSet');
    case 'serial-conflict':
      return t('dedupe.report.serialConflict');
    case 'modified':
      return t('dedupe.report.modified');
    case 'arcade-no-dat':
      return t('dedupe.report.arcadeNoDat');
    case 'big-group':
      return t('dedupe.report.bigGroupMembers', { count: why.members });
    case 'user-skipped':
      return t('dedupe.report.userSkipped');
    case 'chd-metadata-differs':
      return t('dedupe.report.chdMetadataDiffers');
  }
}

/**
 * Copies each level finds, whether it is on or not: the extras of groups
 * that act or are held back only by one-ROM-per-game, by relation.
 */
export function levelCounts(plan: DedupePlanView): Record<DedupeLevel, number> {
  const out: Record<DedupeLevel, number> = { exact: 0, format: 0, version: 0 };
  for (const g of plan.groups) {
    const a = g.action;
    if (
      a.kind !== 'act' &&
      !(a.kind === 'report-only' && a.why.kind === 'level-two-off')
    )
      continue;
    for (const m of extrasOf(g)) out[m.relation] += 1;
  }
  return out;
}

/** A system with this many groups or fewer starts open (spec §11.3). */
export const OPEN_SYSTEM_MAX = 50;

/** Above this many groups an open system renders them windowed. */
export const WINDOW_ABOVE = 200;

/** Systems with a conflict start open, and so do small ones. */
export function systemStartsOpen(s: SystemSummary): boolean {
  return s.conflicts > 0 || s.total <= OPEN_SYSTEM_MAX;
}

/** A group that changes nothing, and why. */
export interface UnchangedItem {
  group: DupGroupView;
  why: ReportWhyView;
}

/**
 * The groups of "Sin cambios": report-only ones and those you asked to
 * leave alone, in plan order. Conflicts have their own block.
 */
export function unchangedItems(
  groups: DupGroupView[],
  skipped?: ReadonlySet<number>,
): UnchangedItem[] {
  const out: UnchangedItem[] = [];
  for (const g of groups) {
    if (g.action.kind === 'conflict') continue;
    if (skipped?.has(g.id))
      out.push({ group: g, why: { kind: 'user-skipped' } });
    else if (g.action.kind === 'report-only')
      out.push({ group: g, why: g.action.why });
  }
  return out;
}

/* ---- Copy facts (spec D3, D5–D8, D11; plan DP-4) --------------------- */

/** Short, untranslated region codes (spec D3): a label-maker readout. */
export const REGION_CODES: Record<RegionId, string> = {
  world: 'WLD',
  usa: 'USA',
  europe: 'EUR',
  japan: 'JPN',
  asia: 'ASI',
  australia: 'AUS',
  brazil: 'BRA',
  canada: 'CAN',
  china: 'CHN',
  france: 'FRA',
  germany: 'GER',
  'hong-kong': 'HKG',
  italy: 'ITA',
  korea: 'KOR',
  netherlands: 'NLD',
  spain: 'ESP',
  sweden: 'SWE',
  taiwan: 'TWN',
  'united-kingdom': 'UK',
};

export const regionCode = (r: RegionId): string => REGION_CODES[r];

/** Uppercased extension of a path's file name (`''` without one). */
function extOf(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1).toUpperCase() : '';
}

/** A disc's own file: a sheet or a whole image (a sheet's tracks are not). */
const DISC_EXTS = new Set([
  'CUE',
  'GDI',
  'CCD',
  'CDI',
  'MDS',
  'CHD',
  'ISO',
  'CSO',
  'PBP',
  'RVZ',
]);
const DISC_NO = /\((?:disc|disk|cd)\s*(\d+)/i;

/**
 * Discs of a playlist. The unit's files are the `.m3u` itself, its discs
 * and their sheets' tracks, so the file count overstates it: count the
 * distinct `(Disc n)` numbers, else the sheets and images, else one disc
 * per file besides the playlist.
 */
function discCount(unit: UnitRefView): number {
  const rest = unit.files.filter((f) => extOf(f) !== 'M3U');
  const numbers = new Set(
    rest.map((f) => DISC_NO.exec(fileName(f))?.[1]).filter(Boolean),
  );
  if (numbers.size > 0) return numbers.size;
  const discs = rest.filter((f) => DISC_EXTS.has(extOf(f))).length;
  return discs > 0 ? discs : Math.max(rest.length, 1);
}

/** `ZIP`, `7Z`, `CUE/BIN`, `GDI`, `CCD/IMG`, `CHD`, `PBP`, `ISO`, `M3U`, `GB`… (spec D8). */
export function formatOf(unit: UnitRefView, t: Translate): string {
  return formatItems(unit, t).join(' · ');
}

/**
 * `formatOf` as separate inline items: a playlist is `['M3U', '3 discos']`,
 * every other kind one item. The facts line renders each on its own,
 * with gap spacing and no dots (spec §7.2).
 */
export function formatItems(unit: UnitRefView, t: Translate): string[] {
  const ext = extOf(unit.primary);
  switch (unit.kind) {
    case 'arcade':
      return [`${ext} (${t('dedupe.format.arcade')})`];
    case 'disc-set':
      return [ext === 'GDI' ? 'GDI' : ext === 'CCD' ? 'CCD/IMG' : 'CUE/BIN'];
    case 'chd':
      return ['CHD'];
    case 'multi-disc':
      return ['M3U', t('dedupe.format.discs', { count: discCount(unit) })];
    case 'folder':
      // `Game.ps3` reads `PS3 (folder)`; a bare `BLUS30001/` only `folder`.
      return [
        ext
          ? `${ext} (${t('dedupe.format.folder')})`
          : t('dedupe.format.folder'),
      ];
    case 'zip':
    case 'image':
    case 'single':
      return [ext];
  }
}

/** A unit's modification date: the newest file of a set (spec D9). */
export const newestMtime = (u: UnitRefView): number =>
  Math.max(u.mtime_ns, ...(u.mtimes_ns ?? []));

export type TagTone = 'neutral' | 'danger' | 'warning';

export interface TagChip {
  key: string;
  /** What the chip shows (a code, a short word). */
  text: string;
  /** Visually hidden full text for screen readers (spec D3, R14). */
  hidden?: string;
  tone: TagTone;
  /** Which name it was read from; the chip's tooltip says so. */
  from?: TagSourceView;
  /** Hover text beyond the source (region names, the overflow's rest). */
  title?: string;
}

const SHOWN_REGIONS = 3;
const SHOWN_OTHER = 2;

const MOD_TONE: Record<Exclude<ModClassView, 'original'>, TagTone> = {
  hack: 'warning',
  translation: 'warning',
  trainer: 'warning',
  pirate: 'warning',
  fixed: 'neutral',
  unlicensed: 'neutral',
};

/** `{ from }` only when the source is known (exact optional properties). */
const src = (from: TagSourceView | undefined): { from?: TagSourceView } =>
  from ? { from } : {};

const FLAG_KEYS = ['verified', 'bad', 'overdump', 'alternate'] as const;

/** Language names in the app locale; the code itself when unknown. */
function languageNames(codes: string[], locale: string): string[] {
  let names: Intl.DisplayNames | null = null;
  try {
    names = new Intl.DisplayNames([locale], { type: 'language' });
  } catch {
    names = null;
  }
  return codes.map((c) => {
    try {
      return names?.of(c) ?? c.toUpperCase();
    } catch {
      return c.toUpperCase();
    }
  });
}

/**
 * Chips in display order: regions (first 3, then "+n" with the rest in
 * `title`), revision, languages (one chip, codes uppercased, names in
 * `hidden`), class, flags ([b] danger, others neutral), modification
 * (hack/translation/trainer/pirate warning, fixed/unlicensed neutral),
 * other (first 2, then "+n").
 */
export function tagChips(
  f: MemberFactsView | undefined,
  t: Translate,
  locale: string,
): TagChip[] {
  if (!f) return [];
  const out: TagChip[] = [];
  const from = src(f.tags_from);
  const more = (key: string, rest: string[]): TagChip => ({
    key,
    text: t('dedupe.tags.more', { count: rest.length }),
    hidden: rest.join(', '),
    tone: 'neutral',
    ...from,
    title: rest.join(', '),
  });

  const regions = f.regions ?? [];
  for (const r of regions.slice(0, SHOWN_REGIONS)) {
    const name = t(`regions.${r}`);
    out.push({
      key: `region:${r}`,
      text: regionCode(r),
      hidden: name,
      tone: 'neutral',
      ...from,
      title: name,
    });
  }
  if (regions.length > SHOWN_REGIONS)
    out.push(
      more(
        'region:more',
        regions.slice(SHOWN_REGIONS).map((r) => t(`regions.${r}`)),
      ),
    );

  if (f.revision)
    out.push({ key: 'revision', text: f.revision, tone: 'neutral', ...from });

  if (f.languages?.length) {
    const list = languageNames(f.languages, locale).join(', ');
    const label = t('dedupe.tags.languages', { list });
    out.push({
      key: 'languages',
      text: f.languages.map((l) => l.toUpperCase()).join(' '),
      hidden: label,
      tone: 'neutral',
      ...from,
      title: label,
    });
  }

  if (f.class && f.class !== 'retail')
    out.push({
      key: `class:${f.class}`,
      text: t(`dedupe.tags.class.${f.class}`),
      tone: 'neutral',
      ...from,
    });

  if (f.flags)
    for (const flag of FLAG_KEYS)
      if (f.flags[flag])
        out.push({
          key: `flag:${flag}`,
          text: t(`dedupe.tags.${flag}`),
          tone: flag === 'bad' ? 'danger' : 'neutral',
          ...src(f.flags_from),
        });

  if (f.modification && f.modification !== 'original')
    out.push({
      key: `mod:${f.modification}`,
      text: t(`dedupe.tags.${f.modification}`),
      tone: MOD_TONE[f.modification],
      ...src(f.modification_from),
    });

  const other = f.other ?? [];
  for (const [i, o] of other.slice(0, SHOWN_OTHER).entries())
    out.push({ key: `other:${i}`, text: o, tone: 'neutral', ...from });
  if (other.length > SHOWN_OTHER)
    out.push(more('other:more', other.slice(SHOWN_OTHER)));

  return out;
}

const RULE_KEYS: Record<KeepReasonView['kind'], string> = {
  'user-choice': 'dedupe.rule.userChoice',
  'has-save': 'dedupe.rule.hasSave',
  played: 'dedupe.rule.played',
  'in-library': 'dedupe.rule.inLibrary',
  verified: 'dedupe.rule.verified',
  retail: 'dedupe.rule.retail',
  region: 'dedupe.rule.region',
  revision: 'dedupe.rule.revision',
  container: 'dedupe.rule.container',
  'has-cover': 'dedupe.rule.hasCover',
  'catalogue-name': 'dedupe.rule.catalogueName',
  tie: 'dedupe.rule.tie',
};

/** Rule chip key for the keeper (spec D11); `user-choice` when the user picked. */
export const ruleKey = (reason: KeepReasonView['kind']): string =>
  RULE_KEYS[reason];

/**
 * Where the copy is now (DP-4): primary before apply and after undo, the
 * quarantine `to` once moved, null after a Trash apply.
 */
export function currentPath(
  m: MemberView,
  group: number,
  outcomes: DedupeOutcomeView[] | null,
): string | null {
  const o = outcomes?.find((x) => x.group === group && x.member === m.id);
  if (o?.status === 'quarantined') return o.to;
  if (o?.status === 'trashed') return null;
  return m.unit.primary;
}

/** Cover first, then the title screen, then a screenshot (spec D5). */
const COVER_ORDER: readonly MediaView[] = ['cover', 'title', 'screenshot'];

const hasMedia = (m: MemberView | undefined, media: MediaView) =>
  m?.facts?.art?.some((a) => a.media === media) ?? false;

/** The media to show as the group cover (spec D5) and its caption key. */
export function coverMedia(g: DupGroupView): {
  media: MediaView;
  caption: string | null;
} {
  const keeper = g.members.find((m) => m.id === g.keeper);
  const pick =
    COVER_ORDER.find((media) => hasMedia(keeper, media)) ??
    COVER_ORDER.find((media) => g.members.some((m) => hasMedia(m, media)));
  if (!pick) return { media: 'cover', caption: null };
  return {
    media: pick,
    caption: pick === 'cover' ? null : `dedupe.cover.media.${pick}`,
  };
}

/**
 * The art the group cover shows in `media`, as Rust resolves it (D4): the
 * keeper's, else the first copy's that has it. Null when only the libretro
 * cache could fill the slot.
 */
export function coverArt(
  g: DupGroupView,
  media: MediaView,
): ArtSeenView | null {
  const of = (m: MemberView | undefined) =>
    m?.facts?.art?.find((a) => a.media === media);
  const keeper = g.members.find((m) => m.id === g.keeper);
  return of(keeper) ?? g.members.map(of).find((a) => a !== undefined) ?? null;
}

/** A copy shows its own thumbnail when its art in `media` differs from the cover's (D6, D7). */
export function ownArtDiffers(
  m: MemberView,
  cover: ArtSeenView | null,
  media: MediaView,
): boolean {
  const own = m.facts?.art?.find((a) => a.media === media);
  if (!own) return false;
  if (!cover) return true;
  return !(
    own.path === cover.path ||
    (own.media === cover.media && own.size === cover.size)
  );
}

/** `aria-keyshortcuts` of a copy's radio: its number, up to 9. */
export const copyKey = (n: number | undefined) =>
  n !== undefined && n >= 1 && n <= 9 ? String(n) : undefined;
