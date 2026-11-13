import type {
  EntryView,
  FillRequest,
  JobView,
  Media,
  OverwritePolicy,
  RegionPref,
  Settings,
  StrategyKind,
} from '@/ipc/types';

import type { Game } from './games';

/** What the wizard fills: the selected ROM, the visible ones, or all. */
export type FillScope = 'selection' | 'filter' | 'library';

/** Spec §9: the ROM's own region, then World, USA, Europe, Japan. */
export const DEFAULT_REGION_ORDER: readonly RegionPref[] = [
  'rom',
  'world',
  'usa',
  'europe',
  'japan',
];

/** The ids of a shown entry: every revision of its game, default first. */
function idsOf(id: number, games?: ReadonlyMap<number, Game>): number[] {
  const game = games?.get(id);
  return game ? game.revisions.map((r) => r.id) : [id];
}

/**
 * Entry ids of a scope; `null` = the whole library. With the games of the
 * views (`byEntry` of `buildGames`), a game sends its representative id
 * plus its other revisions' ids, as each revision is a file a frontend
 * lists and was filled before folding (addon views D4, §14 Q7). Updates
 * and DLC are never entries, so no fill reaches them.
 */
export function scopeIds(
  scope: FillScope,
  selectedId: number | null,
  visible: readonly EntryView[],
  games?: ReadonlyMap<number, Game>,
): number[] | null {
  switch (scope) {
    case 'selection':
      return selectedId === null ? [] : idsOf(selectedId, games);
    case 'filter': {
      const ids = new Set<number>();
      for (const e of visible) for (const id of idsOf(e.id, games)) ids.add(id);
      return [...ids];
    }
    case 'library':
      return null;
  }
}

/** `list` with item `i` moved one place up (-1) or down (+1). */
export function moveItem<T>(list: readonly T[], i: number, dir: -1 | 1): T[] {
  const out = [...list];
  const j = i + dir;
  const a = out[i];
  const b = out[j];
  if (a === undefined || b === undefined) return out;
  out[i] = b;
  out[j] = a;
  return out;
}

export interface FillChoices {
  scope: FillScope;
  selectedId: number | null;
  visible: readonly EntryView[];
  /** Every entry id → its game (`buildGames().byEntry`). */
  games?: ReadonlyMap<number, Game> | undefined;
  media: readonly Media[];
  strategy: StrategyKind;
  regions: readonly RegionPref[];
  overwrite: OverwritePolicy;
}

export function buildFillRequest(o: FillChoices): FillRequest {
  return {
    entry_ids: scopeIds(o.scope, o.selectedId, o.visible, o.games),
    media: [...o.media],
    strategy: o.strategy,
    regions: [...o.regions],
    overwrite: o.overwrite,
  };
}

export interface WizardDefaults {
  media: Media[];
  strategy: StrategyKind;
  regions: RegionPref[];
  /** `null` = the setting is "ask": the user must choose (D25). */
  overwrite: OverwritePolicy | null;
}

/** The wizard's first answers, from the settings (spec §9 Art and
 * Matching) restricted to the media the layout stores; without settings,
 * the M4 defaults (first media, ROM region, only missing art). */
export function wizardDefaults(
  settings: Settings | null,
  profileMedia: readonly Media[],
): WizardDefaults {
  const wanted =
    settings?.art.media.filter((m) => profileMedia.includes(m)) ?? [];
  const overwrite = settings?.art.overwrite ?? 'never';
  return {
    media: wanted.length > 0 ? [...wanted] : profileMedia.slice(0, 1),
    strategy: settings?.matching.strategy ?? 'rom-region',
    regions: [...(settings?.matching.regions ?? DEFAULT_REGION_ORDER)],
    overwrite: overwrite === 'ask' ? null : overwrite,
  };
}

/** "Dry run by default" off (D27): the wizard runs a planned job by
 * itself, once, when there is something to write and nothing to review. */
export function shouldAutoRun(
  job: JobView | null,
  settings: Settings | null,
  alreadyRan: boolean,
): boolean {
  return Boolean(
    job &&
    job.state === 'planned' &&
    !alreadyRan &&
    settings &&
    !settings.safety.dry_run_default &&
    job.plan &&
    job.plan.totals.review === 0 &&
    job.plan.totals.auto > 0,
  );
}
