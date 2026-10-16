/** Wire types: mirror of src-tauri/src/library/{view,identify}.rs. */

export type Media = 'cover' | 'screenshot' | 'title';
export const MEDIA: readonly Media[] = ['cover', 'screenshot', 'title'];

export type ArtMark = 'present' | 'missing' | 'unsupported';

export interface ArtMarks {
  cover: ArtMark;
  screenshot: ArtMark;
  title: ArtMark;
}

export type Region =
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

export const REGIONS: readonly Region[] = [
  'world',
  'usa',
  'europe',
  'japan',
  'asia',
  'australia',
  'brazil',
  'canada',
  'china',
  'france',
  'germany',
  'hong-kong',
  'italy',
  'korea',
  'netherlands',
  'spain',
  'sweden',
  'taiwan',
  'united-kingdom',
];

export interface EntryView {
  id: number;
  system: number;
  rel_path: string;
  dir: string;
  name: string;
  ext: string;
  size: number;
  mtime: number;
  regions: Region[];
  art: ArtMarks;
}

export interface SystemView {
  index: number;
  folder: string;
  dir: string;
  system_id: string | null;
  name: string;
  roms: number;
}

export interface ProfileView {
  id: string;
  name: string;
  media: Media[];
  detected: boolean;
  evidence: string[];
  fitted: boolean;
}

export interface WarningView {
  path: string;
  message: string;
}

export interface LibraryView {
  generation: number;
  root: string;
  name: string;
  profile: ProfileView;
  systems: SystemView[];
  entries: EntryView[];
  warnings: WarningView[];
  scan_ms: number;
}

export interface ScanProgress {
  generation: number;
  done: number;
  total: number;
  folder: string;
  roms: number;
}

export interface RecentLibrary {
  path: string;
  name: string;
  profile_id: string;
  roms: number;
  opened_at: number;
  available: boolean;
}

export interface LibrarySettings {
  ignored_dirs: string[];
}

export type IdMethod = 'hash' | 'arcade-dat' | 'name' | 'unknown';

export interface IdentityView {
  canonical: string | null;
  method: IdMethod;
  confidence: number;
  title: string;
  regions: Region[];
  offline: boolean;
}

export interface ArtImage {
  bytes: Uint8Array<ArrayBuffer>;
  mime: string;
}

/* Fill jobs — mirror of src-tauri/src/jobs/view.rs. */

export type StrategyKind = 'priority' | 'rom-region' | 'popularity';
export const STRATEGIES: readonly StrategyKind[] = [
  'rom-region',
  'priority',
  'popularity',
];
export type OverwritePolicy = 'never' | 'backup';
/** `rom` = the ROM's own regions. */
export type RegionPref = 'rom' | Region;

export interface FillRequest {
  /** Selection or current filter; null = the whole library. */
  entry_ids: number[] | null;
  media: Media[];
  strategy: StrategyKind;
  regions: RegionPref[];
  overwrite: OverwritePolicy;
}

export type JobState =
  | 'gathering'
  | 'planned'
  | 'applying'
  | 'paused'
  | 'cancelled'
  | 'done'
  | 'failed';
export type Phase = 'gather' | 'apply';
export type PauseKind = 'rate-limit' | 'offline' | 'volume';

export interface JobProgress {
  phase: Phase;
  done: number;
  total: number;
  label: string;
}

export interface PauseView {
  phase: Phase;
  kind: PauseKind;
  detail: string;
}

export interface PlanCounts {
  auto: number;
  review: number;
  none: number;
}

export interface SystemCounts extends PlanCounts {
  folder: string;
}

export interface PlanSummary {
  totals: PlanCounts;
  per_system: SystemCounts[];
  warnings: string[];
}

export interface JobTotals {
  filled: number;
  replaced: number;
  skipped: number;
  no_art: number;
  failed: number;
}

export interface JobView {
  id: number;
  generation: number;
  state: JobState;
  progress: JobProgress | null;
  paused: PauseView | null;
  plan: PlanSummary | null;
  decided: number;
  journal: string | null;
  result: JobTotals | null;
  error: string | null;
}

export type MatchKind = 'fuzzy' | 'title-exact' | 'name-exact' | 'canonical';

export interface CandidateView {
  /** 0 = the chosen image; n = alternative n - 1. */
  index: number;
  source: string;
  name: string;
  regions: Region[];
  kind: MatchKind;
  score: number;
  confidence: number;
}

export type ActionKind = 'auto' | 'review' | 'none';
export type NoneKind =
  | 'no-candidates'
  | 'below-threshold'
  | 'source-error'
  | 'no-target'
  | 'skipped';
export type Decision =
  | { choice: 'accept' }
  | { choice: 'skip' }
  | { choice: 'pick'; candidate: number };
export type DecisionIn = Decision & { index: number };
export type ItemErrorKind =
  | 'no-art'
  | 'network'
  | 'decode'
  | 'write-denied'
  | 'disk-full'
  | 'name-invalid'
  | 'outside-root'
  | 'io';
export type OutcomeKind =
  | 'filled'
  | 'replaced'
  | 'skipped-exists'
  | 'skipped-review'
  | 'skipped-no-match'
  | 'not-attempted'
  | 'failed';

export interface ItemOutcome {
  status: OutcomeKind;
  error: ItemErrorKind | null;
  message: string | null;
}

export interface JobItemView {
  index: number;
  entry_id: number | null;
  folder: string;
  name: string;
  media: Media;
  action: ActionKind;
  confidence: number | null;
  note: 'unreleased-variant' | null;
  none_reason: NoneKind | null;
  candidates: CandidateView[];
  replaces_existing: boolean;
  decision: Decision | null;
  outcome: ItemOutcome | null;
}

export type ItemFilter = 'all' | 'auto' | 'review' | 'failed';

export interface JobProgressEvent {
  job: number;
  phase: Phase;
  done: number;
  total: number;
  label: string;
}

export interface EntryArtUpdate {
  id: number;
  art: ArtMarks;
}

export interface EntriesUpdated {
  generation: number;
  entries: EntryArtUpdate[];
}

export interface UndoView {
  job: string;
  removed: number;
  restored: number;
  kept: number;
  entries: EntryArtUpdate[];
}

export interface JobHistoryItem {
  job: string;
  root: string;
  created_at: string;
  entries: number;
  complete: boolean;
  undone: boolean;
}

export type ExportFormat = 'csv' | 'json';
