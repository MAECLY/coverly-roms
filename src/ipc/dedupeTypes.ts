/* Duplicates — mirror of coverly_core::settings::DedupeSettings (and, from
 * Task 18 on, of coverly_core::dedupe). Kept apart from `types.ts` until
 * the archive branch lands; Phase B re-exports it from there. */

import type { ItemErrorKind } from './types';

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

/* ---- Plan view: mirror of coverly_core::dedupe (Task 6) ----------------
 * Paths are strings as the core serialises them; the view adds `plan_id`. */

export type RegionId =
  | 'world'
  | 'usa'
  | 'europe'
  | 'japan'
  | 'asia'
  | 'australia'
  | 'brazil'
  | 'canada'
  | 'china'
  | 'france'
  | 'germany'
  | 'hong-kong'
  | 'italy'
  | 'korea'
  | 'netherlands'
  | 'spain'
  | 'sweden'
  | 'taiwan'
  | 'united-kingdom';

/** Preference order of `ReleaseClass` (retail first). */
export const RELEASE_CLASSES = [
  'retail',
  'beta',
  'proto',
  'demo',
  'sample',
] as const;
export type ReleaseClassView = (typeof RELEASE_CLASSES)[number];

export const DUMP_QUALITIES = [
  'dat-match',
  'unknown',
  'good',
  'bad',
  'overdump',
  'headerless',
  'headered',
] as const;
export type DumpQualityView = (typeof DUMP_QUALITIES)[number];

/** Weakest link order: exact < format < version. */
export const RELATIONS = ['exact', 'format', 'version'] as const;
export type RelationView = (typeof RELATIONS)[number];

export const KEEP_REASON_KINDS = [
  'user-choice',
  'has-save',
  'played',
  'in-library',
  'verified',
  'retail',
  'region',
  'revision',
  'container',
  'has-cover',
  'catalogue-name',
  'tie',
] as const;

/** Why the keeper stays: the deciding rule and the values it compared. */
export type KeepReasonView =
  | { kind: 'user-choice' }
  | { kind: 'has-save'; path: string }
  | { kind: 'played'; playcount: number }
  | { kind: 'in-library' }
  | { kind: 'verified'; kept: DumpQualityView; other: DumpQualityView }
  | { kind: 'retail'; other: ReleaseClassView }
  | { kind: 'region'; kept: RegionId; other: RegionId }
  | { kind: 'revision'; kept: string; other: string }
  | { kind: 'container'; kept: string; other: string; profile: string }
  | { kind: 'has-cover' }
  | { kind: 'catalogue-name'; name: string }
  | { kind: 'tie' };

export const REPORT_WHY_KINDS = [
  'level-two-off',
  'unconfirmed',
  'header-differs',
  'build-differs',
  'partial-set',
  'serial-conflict',
  'modified',
  'arcade-no-dat',
  'big-group',
  'user-skipped',
  'chd-metadata-differs',
] as const;

export type ReportWhyView =
  | { kind: 'level-two-off' }
  | { kind: 'unconfirmed' }
  | { kind: 'header-differs' }
  | { kind: 'build-differs' }
  | { kind: 'partial-set' }
  | { kind: 'serial-conflict' }
  | { kind: 'modified' }
  | { kind: 'arcade-no-dat' }
  | { kind: 'big-group'; members: number }
  | { kind: 'user-skipped' }
  | { kind: 'chd-metadata-differs' };

export type SaveKeyView = 'stem' | 'serial' | 'set';

export interface SaveInfoView {
  path: string;
  size: number;
  mtime_ns: number;
  /** Lowercase hex. */
  sha1: string;
  keyed_by: SaveKeyView;
}

export type GroupActionView =
  | { kind: 'act' }
  | { kind: 'report-only'; why: ReportWhyView }
  | { kind: 'conflict'; saves: SaveInfoView[] };

export type UnitKindView =
  'single' | 'zip' | 'arcade' | 'disc-set' | 'chd' | 'image' | 'multi-disc';

export interface UnitRefView {
  primary: string;
  files: string[];
  kind: UnitKindView;
  mtime_ns: number;
  in_library: boolean;
}

export type MediaView = 'cover' | 'screenshot' | 'title';

export type TravelKindView =
  | { kind: 'save' }
  | { kind: 'state' }
  | { kind: 'cheat' }
  | { kind: 'patch' }
  | { kind: 'text' }
  | { kind: 'art'; media: MediaView }
  | { kind: 'apple-double' }
  | { kind: 'other' };

export interface TravelView {
  from: string;
  kind: TravelKindView;
}

/** A companion shared with another member; it stays in place. */
export interface SharedStayView {
  path: string;
  /** `MemberView.id` of the member that keeps using it. */
  with: number;
}

export interface GamelistEntryView {
  gamelist: string;
  path: string;
}

export interface MemberView {
  id: number;
  unit: UnitRefView;
  relation: RelationView;
  size: number;
  travels: TravelView[];
  stays: SharedStayView[];
  /** Absolute destination in the quarantine; `null` for the keeper. */
  dest: string | null;
  catalogue_name: string | null;
  serials: string[];
  /** 0..1; the UI shows it only in the badge tooltip (R1). */
  confidence: number;
  gamelist?: GamelistEntryView;
  art_leaves?: { media: MediaView[] };
}

export interface DupGroupView {
  /** Stable across scans (hash of the members' content keys). */
  id: number;
  system: string;
  title: string;
  /** `MemberView.id` of the copy that stays. */
  keeper: number;
  reason: KeepReasonView;
  runner_up: [number, KeepReasonView] | null;
  members: MemberView[];
  action: GroupActionView;
  tie: boolean;
  choice_dropped: boolean;
}

export interface TotalsView {
  groups: number;
  extras: number;
  bytes: number;
  conflicts: number;
  /** System id → [groups, extras, bytes] of the groups that act. */
  by_system: Record<string, [number, number, number]>;
}

export interface DedupePlanView {
  plan_id: number;
  version: number;
  root: string;
  roms_dir: string;
  quarantine: string;
  same_volume: boolean;
  removable: boolean;
  profile_id: string;
  groups: DupGroupView[];
  unchanged: number[];
  not_roms: string[];
  unreadable: string[];
  no_catalogue: string[];
  chd_unread: number;
  off_card_saves: string[];
  totals: TotalsView;
}

/* ---- Apply report: mirror of coverly_core::dedupe::apply (Task 23) ----
 * Phase B wires it to the backend; the result view is built on it now. */

export const DEST_LIMITS = [
  'fat32-size',
  'bad-name',
  'path-too-long',
  'case-collision',
] as const;
export type DestLimitView = (typeof DEST_LIMITS)[number];

export type DedupeSkipView =
  | { kind: 'keeper-changed' }
  | { kind: 'changed-since-plan' }
  | { kind: 'source-missing' }
  | { kind: 'destination-limit'; why: DestLimitView }
  | { kind: 'excluded' }
  | { kind: 'conflict' }
  | { kind: 'cancelled' };

export type DedupeItemStatusView =
  | { status: 'quarantined' }
  | { status: 'trashed' }
  | { status: 'skipped'; reason: DedupeSkipView }
  | { status: 'failed'; kind: ItemErrorKind; detail: string };

export interface DedupeReportView {
  job: string;
  /** `[group id, MemberView.id, what happened]`, one per extra. */
  outcomes: [number, number, DedupeItemStatusView][];
  /** Bytes that left the ROMs folder. */
  bytes_out: number;
  /** Covers that left with an extra: keepers now without one. */
  art_left: string[];
}

/** The options the UI can change without a new scan. */
export interface DedupeViewOptions {
  levels: DedupeLevel[];
  one_game_one_rom: boolean;
}
