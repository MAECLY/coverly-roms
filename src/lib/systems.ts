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

/** `organize::walk::FOLDER_PER_GAME_SYSTEMS`: never a manual Organise
 * target, a lone launcher would leave its game's data behind (P19). */
export const FOLDER_PER_GAME_SYSTEMS: readonly string[] = ['typex'];

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
 * Folder-per-game systems (`FOLDER_PER_GAME_SYSTEMS`) are never offered.
 * Empty groups are dropped; the input is not changed.
 */
export function systemOptionGroups(
  systems: readonly SystemInfo[],
  t: (key: string) => string,
  locale: string,
): SystemOptionGroup[] {
  const collator = new Intl.Collator(locale, { sensitivity: 'base' });
  const offered = systems.filter(
    (s) => !FOLDER_PER_GAME_SYSTEMS.includes(s.id),
  );
  return FAMILY_ORDER.map((family) => ({
    family,
    label: t(`systems.family.${family}`),
    systems: offered
      .filter((s) => s.family === family)
      .sort(
        (a, b) => collator.compare(a.name, b.name) || a.id.localeCompare(b.id),
      ),
  })).filter((g) => g.systems.length > 0);
}
