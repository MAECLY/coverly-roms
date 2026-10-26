/* Duplicates — mirror of coverly_core::settings::DedupeSettings,
 * coverly_core::dedupe and src-tauri/src/dedupe/view.rs. `types.ts`
 * re-exports everything here. */

import type { ItemErrorKind, OrganizeJobState, WireError } from './types';

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
  /** A set's files' modification times, in `files` order; absent for one file. */
  mtimes_ns?: number[];
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
  /** Extras go to the OS Trash (the plan was made with `quarantine:
   * trash`); Rust leaves it out when false, the adapter fills it in. */
  trash: boolean;
  /** The library's volume has a Trash: false on a Windows removable
   * drive or a FAT volume without `.Trashes`; the screen disables the
   * Trash option with that reason. */
  trash_available: boolean;
}

/** What Rust sends: `trash` is left out when false. */
export type DedupePlanViewWire = Omit<DedupePlanView, 'trash'> & {
  trash?: boolean;
};

/* ---- Apply report: mirror of coverly_core::dedupe::apply ------------- */

export const DEST_LIMITS = [
  'fat32-size',
  'bad-name',
  'path-too-long',
  'case-collision',
] as const;
export type DestLimitView = (typeof DEST_LIMITS)[number];

export const DEDUPE_SKIP_KINDS = [
  'keeper-changed',
  'changed-since-plan',
  'source-missing',
  'destination-limit',
  'trash-unavailable',
  'conflict',
  'report-only',
  'excluded',
  'cancelled',
  'taken',
] as const;

/** Why an extra copy stayed where it was. */
export type DedupeSkipView =
  | { kind: 'keeper-changed' }
  | { kind: 'changed-since-plan' }
  | { kind: 'source-missing' }
  | { kind: 'destination-limit'; why: DestLimitView }
  /** Its volume has no Trash. */
  | { kind: 'trash-unavailable' }
  | { kind: 'conflict' }
  | { kind: 'report-only' }
  | { kind: 'excluded' }
  | { kind: 'cancelled' }
  /** A file of the set already sits in the quarantine under that name. */
  | { kind: 'taken' };

export type DedupeItemStatusView =
  /** `to`: where its primary file landed in the quarantine. */
  | { status: 'quarantined'; to: string }
  /** `location`: its place in the Trash, when the OS said. */
  | { status: 'trashed'; location: string | null }
  | { status: 'skipped'; reason: DedupeSkipView }
  | { status: 'failed'; kind: ItemErrorKind; detail: string };

/** One extra copy and what happened to it. */
export type DedupeOutcomeView = DedupeItemStatusView & {
  group: number;
  /** `MemberView.id`. */
  member: number;
  /** Where its files are now: the unit's files, then companions and art. */
  files: string[];
};

export type DedupeApplyNoteView =
  /** The game list of `system` lies outside the folder Coverly writes
   * into: its entries stay. */
  | { kind: 'gamelist-not-updated'; system: string }
  /** Art left with a copy and `keeper` has none in that slot now. */
  | { kind: 'art-leaves'; keeper: string };

/** What an apply (or a resume) did. */
export interface DedupeReportView {
  /** The journal; `null` when nothing changed. */
  job: string | null;
  outcomes: DedupeOutcomeView[];
  /** Bytes that left the ROMs folder. */
  bytes_out: number;
  /** Covers that left with an extra: keepers now without one. */
  art_left: string[];
  /** Set when the run stopped on a full, read-only or vanished volume. */
  paused: ItemErrorKind | null;
  /** `[group, member]` not attempted yet: what a resume does. */
  resumable: [number, number][];
  notes: DedupeApplyNoteView[];
}

/* ---- Requests, progress and the job: mirror of src-tauri dedupe/view.rs */

/** Where extras go. */
export type QuarantineChoiceView =
  /** `_Duplicados/` inside the library root (the default). */
  | { kind: 'next-to-roms' }
  | { kind: 'folder'; path: string }
  /** The OS Trash (opt-in). */
  | { kind: 'trash' };

/** What to look for; missing fields take `settings.dedupe`. A regroup
 * reads only `levels`, `one_game_one_rom`, `rules` and `quarantine`. */
export interface DedupeRequest {
  profile?: string | null;
  levels?: DedupeLevel[] | null;
  one_game_one_rom?: boolean | null;
  /** Keeper rules in the order they are weighed. */
  rules?: RuleToggle[] | null;
  /** System ids (`null` = every system). */
  systems?: string[] | null;
  /** Cached catalogues only: no download. */
  offline?: boolean | null;
  quarantine?: QuarantineChoiceView | null;
  /** Copies that must stay, whatever the rules say (absolute paths). */
  forced_keepers?: string[];
  /** Forget the keepers chosen in earlier runs. */
  reset_choices?: boolean;
}

/** Planning phases in the order they run, then `apply`. */
export const DEDUPE_PHASES = [
  'listing',
  'sizes',
  'fingerprints',
  'serials',
  'catalogues',
] as const;
export type DedupePhaseView = (typeof DEDUPE_PHASES)[number];
export type DedupeEventPhase = DedupePhaseView | 'apply';

/** `dedupe-progress`: at most every 100 ms; phase changes and ends always. */
export interface DedupeProgressEvent {
  plan_id: number;
  phase: DedupeEventPhase;
  done: number;
  total: number;
}

export type DedupeJobState = OrganizeJobState;

/** The duplicates apply of the app (`dedupe-changed` event). */
export interface DedupeJobView {
  /** The journal, once known (always for a resume). */
  job: string | null;
  /** The plan applied; a resume gets a new id for its progress events. */
  plan_id: number;
  state: DedupeJobState;
  /** The last progress, while applying. */
  progress: DedupeProgressEvent | null;
  /** What was done, once it ended (`done`, `paused`, `cancelled`). */
  report: DedupeReportView | null;
  /** The library the copies came from. */
  root: string;
  /** The quarantine folder (also set for the Trash, as the plan named it). */
  quarantine: string;
  /** The copies went to the OS Trash instead of the quarantine. */
  trash: boolean;
  /** Why it failed (`failed` only). */
  error: WireError | null;
}

/* ---- Organise wire additions (the duplicates step, DD-6) -------------- */

/** A pile copy of a game the library holds (or of a pile copy that
 * stays): moved to the pile's quarantine (or the Trash). */
export interface OrganizeSetAsideAction {
  action: 'set-aside';
  /** The copy that stays, relative to the target root (a pile copy that
   * stays on the pile: relative to the source root). */
  keeper: string;
  relation: RelationView;
  reason: KeepReasonView;
  /** Where the primary file goes: relative to the source root, or
   * absolute in the library's quarantine. */
  dest: string;
}

/** The game is already in the library: reported, not set aside. */
export interface InLibraryReason {
  kind: 'in-library';
  /** That copy, relative to the target root (or the source root). */
  path: string;
  relation: RelationView;
}

/** The options the UI can change without a new scan. */
export interface DedupeViewOptions {
  levels: DedupeLevel[];
  one_game_one_rom: boolean;
}
