import type {
  EntryView,
  FillRequest,
  Media,
  OverwritePolicy,
  RegionPref,
  StrategyKind,
} from '@/ipc/types';

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

/** Entry ids of a scope; `null` = the whole library. */
export function scopeIds(
  scope: FillScope,
  selectedId: number | null,
  visible: readonly EntryView[],
): number[] | null {
  switch (scope) {
    case 'selection':
      return selectedId === null ? [] : [selectedId];
    case 'filter':
      return visible.map((e) => e.id);
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
  media: readonly Media[];
  strategy: StrategyKind;
  regions: readonly RegionPref[];
  overwrite: OverwritePolicy;
}

export function buildFillRequest(o: FillChoices): FillRequest {
  return {
    entry_ids: scopeIds(o.scope, o.selectedId, o.visible),
    media: [...o.media],
    strategy: o.strategy,
    regions: [...o.regions],
    overwrite: o.overwrite,
  };
}
