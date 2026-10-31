import type { SystemFamily, SystemInfo } from '@/ipc/types';

export type { SystemFamily, SystemInfo };

/** The order a collector thinks in: home consoles, handhelds, computers,
 * arcade, engines (spec §9.1). */
export const FAMILY_ORDER: readonly SystemFamily[] = [
  'console',
  'handheld',
  'computer',
  'arcade',
  'engine',
];

/** One `<optgroup>` of a system picker. */
export interface SystemOptionGroup {
  family: SystemFamily;
  /** `systems.family.*`: sentence case, no count. */
  label: string;
  systems: SystemInfo[];
}

/**
 * The systems of a full picker, grouped by family in `FAMILY_ORDER` and
 * sorted by name in the user's locale (`Ámbar` before `Bravo` in es).
 * Empty groups are dropped; the input is not changed.
 */
export function systemOptionGroups(
  systems: readonly SystemInfo[],
  t: (key: string) => string,
  locale: string,
): SystemOptionGroup[] {
  const collator = new Intl.Collator(locale, { sensitivity: 'base' });
  return FAMILY_ORDER.map((family) => ({
    family,
    label: t(`systems.family.${family}`),
    systems: systems
      .filter((s) => s.family === family)
      .sort(
        (a, b) => collator.compare(a.name, b.name) || a.id.localeCompare(b.id),
      ),
  })).filter((g) => g.systems.length > 0);
}
