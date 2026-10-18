/* Duplicates — mirror of coverly_core::settings::DedupeSettings (and, from
 * Task 18 on, of coverly_core::dedupe). Kept apart from `types.ts` until
 * the archive branch lands; Phase B re-exports it from there. */

export const DEDUPE_LEVELS = ['exact', 'format', 'version'] as const;
export type DedupeLevel = (typeof DEDUPE_LEVELS)[number];

export const DEDUPE_ACTIONS = ['quarantine', 'trash'] as const;
export type DedupeActionSetting = (typeof DEDUPE_ACTIONS)[number];

export const ORGANIZE_DEDUPE_MODES = ['set-aside', 'report', 'off'] as const;
export type OrganizeDedupeMode = (typeof ORGANIZE_DEDUPE_MODES)[number];

/** Keeper rules in their default order (spec §6.1). */
export const KEEP_RULES = [
  'has_save',
  'played',
  'in_library',
  'verified',
  'retail',
  'region',
  'revision',
  'container',
  'cover',
  'catalogue_name',
] as const;
export type KeepRule = (typeof KEEP_RULES)[number];

export interface RuleToggle {
  rule: KeepRule;
  on: boolean;
}

export interface DedupeSettings {
  levels: DedupeLevel[];
  one_game_one_rom: boolean;
  action: DedupeActionSetting;
  quarantine_dir: string | null;
  quarantine_name: string | null;
  keep_rules: RuleToggle[];
  /** Profile id (or `*`) → system id, `disc`, `cart`, `arcade` (or `*`) →
   * containers, preferred first. */
  container_order: Record<string, Record<string, string[]>>;
  exclude_systems: string[];
  max_dat_downloads: number;
  organize_step: OrganizeDedupeMode;
}

declare module './types' {
  interface Settings {
    dedupe: DedupeSettings;
  }
}
