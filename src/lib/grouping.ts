/**
 * Group-by (spec §B.1, §B.5): a stable partition of the visible entries into
 * sections. Groups come first; inside a group the incoming (sorted) order is
 * kept. Sections are index ranges into the reordered list, never copies.
 */
import type { EntryView, Media, Region, SystemView } from '@/ipc/types';

import type { GroupBy } from './viewPrefs';

export interface Section {
  /** Stable id, prefixed by the group key (`system:3`, `letter:A`, `all`). */
  key: string;
  /** Plain label, or the `{system}` value of `labelKey` when that is set. */
  label: string;
  /** i18n key to show instead of `label` (see `sectionLabel`). */
  labelKey?: string;
  /** Set when the section belongs to one system (system, folder). */
  systemId?: string | null;
  start: number;
  count: number;
}

export type ArtGroup = 'missingCover' | 'missingOther' | 'complete';

export interface GroupContext {
  systems: readonly SystemView[];
  regionOrder: readonly Region[];
  /** The media the profile handles; others never make a game incomplete. */
  artMedia: readonly Media[];
  /** Art groups frozen while a fill runs, by entry id (spec §B.1). */
  frozenArt?: ReadonlyMap<number, ArtGroup> | null;
}

export interface Grouped {
  ordered: readonly EntryView[];
  sections: readonly Section[];
}

/** The key of the single section `none` produces. */
export const ALL_SECTION_KEY = 'all';

const ART_ORDER: readonly ArtGroup[] = [
  'missingCover',
  'missingOther',
  'complete',
];

const collator = new Intl.Collator(undefined, {
  sensitivity: 'base',
  numeric: true,
});

/** Where an entry's art stands, counting only the profile's media. */
export function artGroupOf(e: EntryView, artMedia: readonly Media[]): ArtGroup {
  if (e.art.cover === 'missing') return 'missingCover';
  for (const m of artMedia) {
    if (m !== 'cover' && e.art[m] === 'missing') return 'missingOther';
  }
  return 'complete';
}

/** `#` or an upper-case A–Z, diacritics stripped. */
export function letterOf(name: string): string {
  const first = name.trimStart().normalize('NFD').charAt(0).toUpperCase();
  return first >= 'A' && first <= 'Z' ? first : '#';
}

/** `dir` relative to the system folder (`''` at the system root). */
export function relativeDir(
  dir: string,
  system: SystemView | undefined,
): string {
  if (!system) return dir;
  if (dir === system.dir) return '';
  const prefix = `${system.dir}/`;
  return dir.startsWith(prefix) ? dir.slice(prefix.length) : dir;
}

interface Bucket {
  section: Omit<Section, 'start' | 'count'>;
  /** Sort keys, compared in order. */
  rank: number;
  sub: string;
  items: EntryView[];
}

/**
 * Stable-partitions `entries` by group (groups first, current order kept
 * inside) and returns the reordered list plus sections as index ranges.
 */
export function groupEntries(
  entries: readonly EntryView[],
  by: GroupBy,
  ctx: GroupContext,
): Grouped {
  if (by === 'none') {
    return {
      ordered: entries,
      sections: [
        { key: ALL_SECTION_KEY, label: '', start: 0, count: entries.length },
      ],
    };
  }

  const systemAt = new Map<number, { system: SystemView; pos: number }>();
  ctx.systems.forEach((system, pos) =>
    systemAt.set(system.index, { system, pos }),
  );
  const unknownRank = ctx.systems.length;
  const regionRank = new Map<Region, number>();
  ctx.regionOrder.forEach((r, i) => regionRank.set(r, i));

  const buckets = new Map<string, Bucket>();
  const bucketFor = (
    key: string,
    make: () => Omit<Bucket, 'items'>,
  ): Bucket => {
    let b = buckets.get(key);
    if (!b) {
      b = { ...make(), items: [] };
      buckets.set(key, b);
    }
    return b;
  };

  for (const e of entries) {
    let b: Bucket;
    switch (by) {
      case 'system': {
        const hit = systemAt.get(e.system);
        b = bucketFor(`system:${e.system}`, () => ({
          section: {
            key: `system:${e.system}`,
            label: hit?.system.name ?? String(e.system),
            systemId: hit?.system.system_id ?? null,
          },
          rank: hit ? hit.pos : unknownRank + e.system,
          sub: '',
        }));
        break;
      }
      case 'folder': {
        const hit = systemAt.get(e.system);
        const rel = relativeDir(e.dir, hit?.system);
        const key = `folder:${e.system}:${rel}`;
        b = bucketFor(key, () => {
          const name = hit?.system.name ?? String(e.system);
          return {
            section:
              rel === ''
                ? {
                    key,
                    label: name,
                    labelKey: 'layouts.group.root',
                    systemId: hit?.system.system_id ?? null,
                  }
                : { key, label: rel, systemId: hit?.system.system_id ?? null },
            rank: hit ? hit.pos : unknownRank + e.system,
            sub: rel,
          };
        });
        break;
      }
      case 'region': {
        let best: Region | null = null;
        let bestRank = Number.POSITIVE_INFINITY;
        for (const r of e.regions) {
          const rank = regionRank.get(r) ?? ctx.regionOrder.length;
          if (rank < bestRank) {
            best = r;
            bestRank = rank;
          }
        }
        b =
          best === null
            ? bucketFor('region:none', () => ({
                section: {
                  key: 'region:none',
                  label: '',
                  labelKey: 'layouts.group.noRegion',
                },
                rank: Number.MAX_SAFE_INTEGER,
                sub: '',
              }))
            : bucketFor(`region:${best}`, () => ({
                section: {
                  key: `region:${best}`,
                  label: best,
                  labelKey: `regions.${best}`,
                },
                rank: bestRank,
                sub: best,
              }));
        break;
      }
      case 'art': {
        const group = ctx.frozenArt?.get(e.id) ?? artGroupOf(e, ctx.artMedia);
        b = bucketFor(`art:${group}`, () => ({
          section: {
            key: `art:${group}`,
            label: group,
            labelKey: `layouts.group.${group}`,
          },
          rank: ART_ORDER.indexOf(group),
          sub: '',
        }));
        break;
      }
      case 'letter': {
        const letter = letterOf(e.name);
        b = bucketFor(`letter:${letter}`, () => ({
          section: { key: `letter:${letter}`, label: letter },
          rank: letter === '#' ? 0 : 1,
          sub: letter,
        }));
        break;
      }
    }
    b.items.push(e);
  }

  const sorted = [...buckets.values()].sort(
    (a, b) =>
      a.rank - b.rank ||
      (a.sub === ''
        ? b.sub === ''
          ? 0
          : -1
        : b.sub === ''
          ? 1
          : collator.compare(a.sub, b.sub)),
  );
  const ordered: EntryView[] = [];
  const sections: Section[] = [];
  for (const b of sorted) {
    sections.push({
      ...b.section,
      start: ordered.length,
      count: b.items.length,
    });
    for (const e of b.items) ordered.push(e);
  }
  return { ordered, sections };
}

/** The text a section header shows. */
export function sectionLabel(
  section: Section,
  t: (key: string, vars?: Record<string, string>) => string,
): string {
  return section.labelKey
    ? t(section.labelKey, { system: section.label })
    : section.label;
}
