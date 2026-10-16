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
  /** Fills also merge the art paths into `gamelist.xml` (D31). */
  gamelist: boolean;
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

export type IdMethod = 'hash' | 'arcade-dat' | 'name' | 'unknown' | 'manual';

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
  /** Review items decided as skip: decided, but not written. */
  skipped: number;
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
  /** Community votes (SteamGridDB), when the source has them. */
  votes: Votes | null;
  /** Where in the source it came from (ids, never a credential). */
  origin: string | null;
  /** Why it was chosen: only on index 0 of a plan item. */
  reason: PickReason | null;
}

export interface Votes {
  up: number;
  down: number;
}

/** Why the chosen image beat the next one (the UI localises it). */
export type PickReason =
  | { why: 'only' }
  | { why: 'better-match' }
  | { why: 'release' }
  | { why: 'votes'; score: number; next: number }
  | { why: 'consensus'; count: number; next: number }
  | { why: 'source-order'; source: string }
  | { why: 'region'; rom: boolean }
  | { why: 'language' }
  | { why: 'tie' }
  | { why: 'user' };

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
  | 'volume-gone'
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

/* Settings, profiles and credentials — mirror of coverly_core::settings,
 * coverly_core::profiles::custom, coverly_core::credentials and
 * src-tauri/src/{prefs,profiles}.rs. */

export type LanguageChoice = 'auto' | 'en' | 'es';
export type ThemeChoice = 'system' | 'light' | 'dark';
export type UpdateChannel = 'stable' | 'beta';
export type OverwriteSetting = 'never' | 'ask' | 'backup';
export type OutputFormat = 'profile' | 'png' | 'jpg';
export type ProxyChoice = 'system' | 'env' | 'manual';
export type LogLevel = 'error' | 'warn' | 'info' | 'debug';
export type MatchMethod = 'hash-then-name' | 'hash-only' | 'name-only';
export type SourceId = 'libretro' | 'steamgriddb' | 'screenscraper';

export interface RecentEntry {
  path: string;
  name: string;
  profile_id: string;
  roms: number;
  opened_at: number;
}

export interface MaxSize {
  width: number;
  height: number;
}

export interface SourceEntry {
  id: SourceId;
  enabled: boolean;
}

export interface Settings {
  version: number;
  general: {
    language: LanguageChoice;
    theme: ThemeChoice;
    check_updates_on_launch: boolean;
    update_channel: UpdateChannel;
  };
  library: {
    default_profile: string | null;
    ignored_dirs: string[];
    ignored_exts: string[];
    include_hidden: boolean;
    follow_symlinks: boolean;
    recent: RecentEntry[];
  };
  art: {
    media: Media[];
    single_slot_media: Media;
    output_format: OutputFormat;
    max_size: Record<string, MaxSize>;
    keep_aspect: boolean;
    jpeg_quality: number;
    overwrite: OverwriteSetting;
  };
  matching: {
    strategy: StrategyKind;
    regions: RegionPref[];
    languages: string[];
    auto_threshold: number;
    review_threshold: number;
    fuzzy: boolean;
    arcade_translation: boolean;
    match_method: MatchMethod;
    max_hash_mb: number;
  };
  sources: { order: SourceEntry[] };
  network: {
    concurrency: number;
    timeout_secs: number;
    retries: number;
    proxy: ProxyChoice;
    proxy_url: string;
    cache_max_mb: number;
    index_refresh_hours: number;
  };
  safety: {
    backup_before_overwrite: boolean;
    backup_dir: string | null;
    dry_run_default: boolean;
    clean_apple_double: boolean;
    undo_depth: number;
  };
  advanced: { log_level: LogLevel; custom_dir: string | null };
}

export type SettingsGroup = Exclude<keyof Settings, 'version'>;

/** A merge patch: maps merge key by key, so removing a per-profile max
 * size takes an explicit `null` for that key. */
export type SettingsPatch = {
  [G in SettingsGroup]?: G extends 'art'
    ? Partial<Omit<Settings['art'], 'max_size'>> & {
        max_size?: Record<string, MaxSize | null>;
      }
    : Partial<Settings[G]>;
};

export type FieldErrorCode =
  | 'range'
  | 'required'
  | 'invalid'
  | 'order'
  | 'unknown'
  | 'duplicate'
  | 'not-absolute';

export interface FieldError {
  field: string;
  code: FieldErrorCode;
  min: number | null;
  max: number | null;
}

export type SettingsNotice =
  'migrated' | 'reset-corrupt' | 'fixed' | 'future-version';

export interface SettingsView {
  settings: Settings;
  notice: SettingsNotice | null;
  backup: string | null;
  fixed: string[];
}

export interface SettingsUpdate {
  settings: Settings;
  errors: FieldError[];
  reopen: boolean;
}

export type LinkId =
  'repo' | 'license' | 'notices' | 'trademarks' | 'releases' | 'issues';

export type CredentialKey =
  'steamgriddb-api-key' | 'screenscraper-user' | 'screenscraper-password';
export const CREDENTIAL_KEYS: readonly CredentialKey[] = [
  'steamgriddb-api-key',
  'screenscraper-user',
  'screenscraper-password',
];

/** What "Test key" found (never the key itself). */
export type KeyCheck =
  'ok' | 'invalid' | 'missing' | 'offline' | 'rate-limited' | 'unavailable';
export const KEY_CHECKS: readonly KeyCheck[] = [
  'ok',
  'invalid',
  'missing',
  'offline',
  'rate-limited',
  'unavailable',
];

/** One source as the Sources settings show it. */
export interface SourceInfo {
  id: SourceId;
  /** This build can use it (ScreenScraper needs developer credentials). */
  built: boolean;
  /** Credentials it needs before it can run. */
  needs: CredentialKey[];
  /** Built and every needed credential stored. */
  ready: boolean;
}

export interface CredentialStatus {
  available: boolean;
  set: Record<CredentialKey, boolean>;
}

export interface SlotFile {
  template: string;
  format: 'png' | 'jpg';
  max_width: number | null;
  max_height: number | null;
}

export interface ProfileFile {
  id: string;
  name: string;
  roms_root: string;
  art_dirs: string[];
  gamelist: boolean;
  slots: Partial<Record<Media, SlotFile>>;
}

export interface ProfileInfo {
  profile: ProfileFile;
  builtin: boolean;
  mirror_subdirs: boolean;
}

export interface ProfileSaveOutcome {
  profiles: ProfileInfo[];
  errors: FieldError[];
}

export interface PreviewExample {
  system_folder: string;
  rel_dir: string;
  file_name: string;
}

export interface ProfilePreview {
  paths: Partial<Record<Media, string>>;
  errors: FieldError[];
}

/** What asked for an update check (Rust `CheckTrigger`). */
export type UpdateTrigger = 'startup' | 'manual';

/** Rust `UpdateOfferView`: the version the channel offers. */
export interface UpdateOffer {
  version: string;
  notes: string | null;
  date: string | null;
}

/** Rust `UpdateCheckView`; `skipped` = a startup check with the setting off. */
export type UpdateCheck =
  | { kind: 'skipped' }
  | { kind: 'up-to-date'; current: string; latest: string }
  | { kind: 'available'; current: string; offer: UpdateOffer };

/** Rust `UpdateProgressEvent` (the `update-progress` event). */
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

/** Rust `UpdateInstallView`. */
export interface UpdateInstall {
  restart_required: boolean;
}

/** Rust `coverly_core::volumes::Volume`: a connected drive or SD card. */
export interface Volume {
  path: string;
  name: string;
}
