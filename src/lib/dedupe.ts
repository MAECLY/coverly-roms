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
  type DedupePlanView,
  type DedupeViewOptions,
  type DupGroupView,
  type KeepReasonView,
  type MemberView,
  type RelationView,
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
